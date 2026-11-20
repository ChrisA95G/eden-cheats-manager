use std::io::{Read, Seek, SeekFrom};

const NCA_PREFIX_SIZE: u64 = 0x4000;
const SECTION_SIZE: u64 = 64;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Compression {
    Block,
    Solid,
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
        Ok(Compression::Block)
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
        assert_eq!(compression(entry(2, b"NCZBLOCK")), Ok(Compression::Block));
        let zstd = [0x28, 0xb5, 0x2f, 0xfd, 0, 0, 0, 0];
        assert_eq!(compression(entry(2, &zstd)), Ok(Compression::Solid));
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
