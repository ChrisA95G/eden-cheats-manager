use super::BlockIndex;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};

/// Seekable decoded NCZ payload. Offset zero follows the original 0x4000-byte NCA prefix.
pub(in super::super) struct BlockReader<R> {
    source: R,
    pub(in super::super) index: BlockIndex,
    position: u64,
    cached: Option<(usize, Vec<u8>)>,
}

impl<R> BlockReader<R> {
    pub(super) fn new(source: R, index: BlockIndex) -> Self {
        Self {
            source,
            index,
            position: 0,
            cached: None,
        }
    }
}

impl<R: Read + Seek> Read for BlockReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.position == self.index.decoded_size {
            return Ok(0);
        }
        let size = self.index.block_size;
        let block = (self.position / size) as usize;
        let within = self.position % size;
        let decoded = size.min(self.index.decoded_size - (self.position - within));
        let start = self.index.offsets[block];
        let stored = self.index.offsets[block + 1] - start;
        let len = (decoded - within).min(out.len() as u64) as usize;
        if stored == decoded {
            self.source.seek(SeekFrom::Start(start + within))?;
            self.source.read_exact(&mut out[..len])?;
        } else {
            if self.cached.as_ref().map(|(id, _)| *id) != Some(block) {
                // Limit peak memory on handhelds. Raw blocks above this size can still be read.
                if decoded > 64 * 1024 * 1024 {
                    return Err(io::Error::new(
                        ErrorKind::Unsupported,
                        "Compressed NCZ block exceeds 64 MiB",
                    ));
                }
                self.cached = None;
                self.source.seek(SeekFrom::Start(start))?;
                let mut frame = vec![0; stored as usize];
                self.source.read_exact(&mut frame)?;
                let bytes = zstd::bulk::decompress(&frame, decoded as usize)?;
                if bytes.len() as u64 != decoded {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "Decoded NCZ block size mismatch",
                    ));
                }
                self.cached = Some((block, bytes));
            }
            let (_, bytes) = self.cached.as_ref().unwrap();
            let offset = within as usize;
            out[..len].copy_from_slice(&bytes[offset..offset + len]);
        }
        self.position += len as u64;
        Ok(len)
    }
}

impl<R> Seek for BlockReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(n) => self.position.checked_add_signed(n),
            SeekFrom::End(n) => self.index.decoded_size.checked_add_signed(n),
        };
        self.position = next
            .filter(|&n| n <= self.index.decoded_size)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Seek outside NCZ payload"))?;
        Ok(self.position)
    }
}
