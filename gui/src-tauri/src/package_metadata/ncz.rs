use std::io::{BufReader, Read, Seek, SeekFrom};

const NCA_PREFIX_SIZE: u64 = 0x4000;
const SECTION_SIZE: u64 = 64;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Compression {
    Block(BlockIndex),
    Solid,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct BlockIndex {
    pub(super) block_size: u64,
    pub(super) decoded_size: u64,
    // Entry-relative start of each stored block, followed by the end of the last block.
    pub(super) offsets: Vec<u64>,
}

fn read_block_index(mut entry: impl Read + Seek) -> Result<BlockIndex, String> {
    let mut header = [0; 16]; // Fields following the NCZBLOCK marker.
    entry
        .read_exact(&mut header)
        .map_err(|e| format!("Could not read NCZ block header: {e}"))?;
    if header[..2] != [2, 1] || !(14..=32).contains(&header[3]) {
        return Err("Unsupported NCZ block header.".into());
    }
    let block_size = 1_u64 << header[3];
    let count = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let decoded_size = u64::from_le_bytes(header[8..].try_into().unwrap());
    if count == 0 || u64::from(count) != decoded_size.div_ceil(block_size) {
        return Err("NCZ block count does not match its decoded size.".into());
    }
    // At most 32 MiB of offsets, even on a malformed package.
    if count >= 4 * 1024 * 1024 {
        return Err("NCZ block index is too large.".into());
    }
    let table = entry.stream_position().map_err(|e| e.to_string())?;
    let end = entry.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let table_size = u64::from(count) * 4;
    let mut offset = table
        .checked_add(table_size)
        .filter(|&p| p <= end)
        .ok_or("Truncated NCZ block size table.")?;
    entry
        .seek(SeekFrom::Start(table))
        .map_err(|e| e.to_string())?;
    let mut sizes = BufReader::new(entry.take(table_size));
    let mut offsets = Vec::with_capacity(count as usize + 1);
    for i in 0..u64::from(count) {
        let mut bytes = [0; 4];
        sizes
            .read_exact(&mut bytes)
            .map_err(|e| format!("Could not read NCZ block size: {e}"))?;
        let stored = u64::from(u32::from_le_bytes(bytes));
        let decoded = block_size.min(decoded_size - i * block_size);
        if stored == 0 || stored > decoded || stored > end - offset {
            return Err("Invalid NCZ stored block size.".into());
        }
        offsets.push(offset);
        offset += stored;
    }
    if offset != end {
        return Err("NCZ block sizes do not cover the entry.".into());
    }
    offsets.push(end);
    Ok(BlockIndex {
        block_size,
        decoded_size,
        offsets,
    })
}

pub(super) fn compression(mut entry: impl Read + Seek) -> Result<Compression, String> {
    let mut header = [0; 16];
    entry
        .seek(SeekFrom::Start(NCA_PREFIX_SIZE))
        .and_then(|_| entry.read_exact(&mut header))
        .map_err(|e| format!("Could not read NCZ section header: {e}"))?;
    if &header[..8] != b"NCZSECTN" {
        return Err("Missing NCZ section header.".into());
    }
    let count = u64::from_le_bytes(header[8..].try_into().unwrap());
    if count == 0 {
        return Err("NCZ section table is empty.".into());
    }
    let payload = count
        .checked_mul(SECTION_SIZE)
        .and_then(|size| size.checked_add(NCA_PREFIX_SIZE + header.len() as u64))
        .ok_or("NCZ section table size overflowed.")?;
    let mut magic = [0; 8];
    entry
        .seek(SeekFrom::Start(payload))
        .and_then(|_| entry.read_exact(&mut magic))
        .map_err(|e| format!("Could not read NCZ compression marker: {e}"))?;
    if &magic == b"NCZBLOCK" {
        read_block_index(entry).map(Compression::Block)
    } else if magic.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Ok(Compression::Solid)
    } else {
        Err("Unrecognized NCZ compression marker.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn entry(count: u64, payload: &[u8]) -> Cursor<Vec<u8>> {
        let mut bytes = vec![0; 0x4000];
        bytes.extend_from_slice(b"NCZSECTN");
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&[0; 2 * 64]);
        bytes.extend_from_slice(payload);
        Cursor::new(bytes)
    }

    #[test]
    fn detects_compression_after_the_section_table() {
        let zstd = [0x28, 0xb5, 0x2f, 0xfd, 0, 0, 0, 0];
        assert_eq!(compression(entry(2, &zstd)), Ok(Compression::Solid));
    }

    fn blocks(decoded_size: u64, sizes: &[u32], data_len: usize) -> Cursor<Vec<u8>> {
        let mut payload = b"NCZBLOCK\x02\x01\x00\x0e".to_vec();
        payload.extend_from_slice(&(sizes.len() as u32).to_le_bytes());
        payload.extend_from_slice(&decoded_size.to_le_bytes());
        for size in sizes {
            payload.extend_from_slice(&size.to_le_bytes());
        }
        payload.resize(payload.len() + data_len, 0);
        entry(2, &payload)
    }

    #[test]
    fn indexes_compressed_and_raw_blocks_with_a_partial_or_full_final_block() {
        for tail in [3, 16384] {
            let entry = blocks(16384 + tail, &[9, tail as u32], 9 + tail as usize);
            let end = entry.get_ref().len() as u64;
            let Ok(Compression::Block(index)) = compression(entry) else {
                panic!("expected block index")
            };
            assert_eq!(index.offsets, [end - tail - 9, end - tail, end]);
            assert_eq!(index.block_size, 16384);
            assert_eq!(index.decoded_size, 16384 + tail);
        }
    }

    #[test]
    fn rejects_inconsistent_block_counts_sizes_and_bounds() {
        for entry in [
            blocks(16385, &[1], 1),
            blocks(1, &[0], 0),
            blocks(1, &[2], 2),
            blocks(2, &[2], 1),
            blocks(1, &[1], 2),
            entry(2, b"NCZBLOCK"),
        ] {
            assert!(compression(entry).is_err());
        }
    }

    #[test]
    fn rejects_invalid_tables_and_payload_markers() {
        for count in [0, 3, u64::MAX] {
            assert!(compression(entry(count, b"NCZBLOCK")).is_err());
        }
        assert!(compression(entry(2, b"NCZ")).is_err());
        assert!(compression(entry(2, b"garbage!")).is_err());
    }
}
