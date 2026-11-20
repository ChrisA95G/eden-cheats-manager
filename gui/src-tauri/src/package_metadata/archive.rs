use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};

const MAX_ARCHIVE_ENTRIES: u32 = 16_384;
const MAX_STRING_TABLE_SIZE: u32 = 16 * 1024 * 1024;

#[cfg(unix)]
fn positioned_read(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(windows)]
fn positioned_read(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

#[derive(Clone)]
pub(super) struct ReadAtFile {
    file: Arc<File>,
    pub(super) len: u64,
}

impl ReadAtFile {
    pub(super) fn new(file: File) -> Result<Self, String> {
        let len = file
            .metadata()
            .map_err(|error| format!("Could not inspect selected document: {error}"))?
            .len();
        Ok(Self {
            file: Arc::new(file),
            len,
        })
    }

    pub(super) fn read_exact_at(
        &self,
        mut offset: u64,
        mut buffer: &mut [u8],
    ) -> Result<(), String> {
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| "Document read range overflowed.".to_string())?;
        if end > self.len {
            return Err("Selected document is truncated.".into());
        }

        while !buffer.is_empty() {
            let read = positioned_read(self.file.as_ref(), buffer, offset)
                .map_err(|error| format!("Could not read selected document: {error}"))?;
            if read == 0 {
                return Err("Selected document ended unexpectedly.".into());
            }
            offset += read as u64;
            buffer = &mut buffer[read..];
        }
        Ok(())
    }

    pub(super) fn read_vec(
        &self,
        offset: u64,
        size: u64,
        limit: u64,
        label: &str,
    ) -> Result<Vec<u8>, String> {
        if size > limit {
            return Err(format!("{label} is too large to parse safely."));
        }
        let size = usize::try_from(size).map_err(|_| format!("{label} is too large."))?;
        let mut data = vec![0; size];
        self.read_exact_at(offset, &mut data)?;
        Ok(data)
    }

    pub(super) fn bounded(&self, start: u64, len: u64) -> Result<BoundedReader, String> {
        let end = start
            .checked_add(len)
            .ok_or_else(|| "Document range overflowed.".to_string())?;
        if end > self.len {
            return Err("Archive entry extends beyond the selected document.".into());
        }
        Ok(BoundedReader {
            source: self.clone(),
            start,
            len,
            position: 0,
        })
    }
}

#[derive(Clone)]
pub(super) struct BoundedReader {
    source: ReadAtFile,
    start: u64,
    len: u64,
    position: u64,
}

impl Read for BoundedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.len.saturating_sub(self.position);
        let read_len = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        if read_len == 0 {
            return Ok(0);
        }
        let read = positioned_read(
            self.source.file.as_ref(),
            &mut buffer[..read_len],
            self.start + self.position,
        )?;
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for BoundedReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.len) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if next < 0 || next > i128::from(self.len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek outside bounded archive entry",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

#[derive(Debug, Clone)]
pub(super) struct ArchiveEntry {
    pub(super) name: String,
    pub(super) offset: u64,
    pub(super) size: u64,
}

pub(super) struct PackageArchive {
    pub(super) format: &'static str,
    pub(super) entries: Vec<ArchiveEntry>,
}

impl PackageArchive {
    fn from_entries(
        entries: Vec<ArchiveEntry>,
        plain: &'static str,
        compressed: &'static str,
    ) -> Self {
        let has_ncz = entries.iter().any(|entry| {
            entry
                .name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("ncz"))
        });
        Self {
            format: if has_ncz { compressed } else { plain },
            entries,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryTable {
    Pfs,
    RootHfs,
    SecureHfs,
}

pub(super) fn parse_package_archive(source: &ReadAtFile) -> Result<PackageArchive, String> {
    if source.len < 0x104 {
        return Err("Selected package is too small to be an NSP or XCI.".into());
    }

    let mut magic = [0; 4];
    source.read_exact_at(0, &mut magic)?;
    if &magic == b"PFS0" {
        let entries = parse_pfs0(source.bounded(0, source.len)?, source.len)?;
        return Ok(PackageArchive::from_entries(entries, "NSP", "NSZ"));
    }

    source.read_exact_at(0x100, &mut magic)?;
    if &magic == b"HEAD" {
        let hfs0_offset = read_u64_at(source, 0x130)?;
        if hfs0_offset >= source.len {
            return Err("XCI root partition offset is outside the document.".into());
        }
        let root_entries = parse_hfs0(
            source,
            hfs0_offset,
            source.len - hfs0_offset,
            EntryTable::RootHfs,
        )?;
        let secure = root_entries
            .iter()
            .find(|entry| entry.name == "secure")
            .ok_or_else(|| "XCI does not contain a secure partition.".to_string())?;
        let entries = parse_hfs0(source, secure.offset, secure.size, EntryTable::SecureHfs)?;
        return Ok(PackageArchive::from_entries(entries, "XCI", "XCZ"));
    }

    let mut full_xci_magic = [0; 4];
    if source.len >= 0x1104 {
        source.read_exact_at(0x1100, &mut full_xci_magic)?;
    }
    if &full_xci_magic == b"HEAD" {
        return Err("Full/key-area-prefixed XCI files are not supported yet.".into());
    }

    Err(
        "Unsupported package format. Select an NSP, trimmed XCI, or block-compressed NSZ/XCZ file."
            .into(),
    )
}

pub(super) fn parse_pfs0(mut reader: impl Read, size: u64) -> Result<Vec<ArchiveEntry>, String> {
    let mut header = [0; 0x10];
    reader
        .read_exact(&mut header)
        .map_err(|e| format!("Could not read PFS0 header: {e}"))?;
    if &header[..4] != b"PFS0" {
        return Err("Invalid NSP/PFS0 header.".into());
    }
    let file_count = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let string_size = u32::from_le_bytes(header[8..12].try_into().unwrap());
    validate_archive_header(file_count, string_size)?;

    let entries_size = u64::from(file_count)
        .checked_mul(0x18)
        .ok_or_else(|| "PFS0 entry table overflowed.".to_string())?;
    let data_start = 0x10_u64
        .checked_add(entries_size)
        .and_then(|value| value.checked_add(u64::from(string_size)))
        .ok_or_else(|| "PFS0 header size overflowed.".to_string())?;
    if data_start > size {
        return Err("PFS0 header extends beyond the package.".into());
    }

    let mut raw_entries = vec![0; entries_size as usize];
    let mut strings = vec![0; string_size as usize];
    reader
        .read_exact(&mut raw_entries)
        .and_then(|_| reader.read_exact(&mut strings))
        .map_err(|e| format!("Could not read PFS0 file table: {e}"))?;
    build_entries(0, size, data_start, &raw_entries, EntryTable::Pfs, &strings)
}

fn parse_hfs0(
    source: &ReadAtFile,
    start: u64,
    size: u64,
    table: EntryTable,
) -> Result<Vec<ArchiveEntry>, String> {
    let header = source.read_vec(start, 0x10, 0x10, "HFS0 header")?;
    if &header[..4] != b"HFS0" {
        return Err("Invalid XCI/HFS0 partition header.".into());
    }
    let file_count = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let string_size = u32::from_le_bytes(header[8..12].try_into().unwrap());
    validate_archive_header(file_count, string_size)?;

    let entries_size = u64::from(file_count)
        .checked_mul(0x40)
        .ok_or_else(|| "HFS0 entry table overflowed.".to_string())?;
    let data_start = 0x10_u64
        .checked_add(entries_size)
        .and_then(|value| value.checked_add(u64::from(string_size)))
        .ok_or_else(|| "HFS0 header size overflowed.".to_string())?;
    if data_start > size {
        return Err("HFS0 header extends beyond its partition.".into());
    }

    let raw_entries = source.read_vec(start + 0x10, entries_size, entries_size, "HFS0 entries")?;
    let strings = source.read_vec(
        start + 0x10 + entries_size,
        u64::from(string_size),
        u64::from(MAX_STRING_TABLE_SIZE),
        "HFS0 string table",
    )?;
    build_entries(start, size, data_start, &raw_entries, table, &strings)
}

fn validate_archive_header(file_count: u32, string_size: u32) -> Result<(), String> {
    if file_count == 0 || file_count > MAX_ARCHIVE_ENTRIES {
        return Err("Archive contains an invalid number of entries.".into());
    }
    if string_size == 0 || string_size > MAX_STRING_TABLE_SIZE {
        return Err("Archive string table has an invalid size.".into());
    }
    Ok(())
}

fn build_entries(
    archive_start: u64,
    archive_size: u64,
    data_start: u64,
    raw_entries: &[u8],
    table: EntryTable,
    strings: &[u8],
) -> Result<Vec<ArchiveEntry>, String> {
    let entry_size = match table {
        EntryTable::Pfs => 0x18,
        EntryTable::RootHfs | EntryTable::SecureHfs => 0x40,
    };
    let mut rows = raw_entries.chunks_exact(entry_size);
    let mut entries = Vec::with_capacity(rows.len());
    while let Some(raw) = rows.next() {
        let relative_offset = u64::from_le_bytes(raw[0..8].try_into().unwrap());
        let size = u64::from_le_bytes(raw[8..16].try_into().unwrap());
        let name_offset = u32::from_le_bytes(raw[16..20].try_into().unwrap()) as usize;
        if name_offset >= strings.len() {
            return Err("Archive filename offset is outside its string table.".into());
        }
        let name_bytes = &strings[name_offset..];
        let name_end = name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| "Archive filename is not null-terminated.".to_string())?;
        let name = std::str::from_utf8(&name_bytes[..name_end])
            .map_err(|_| "Archive filename is not valid UTF-8.")?
            .to_string();
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err("Archive contains an invalid filename.".into());
        }

        let relative_data_offset = data_start
            .checked_add(relative_offset)
            .ok_or_else(|| "Archive entry offset overflowed.".to_string())?;
        let relative_end = relative_data_offset
            .checked_add(size)
            .ok_or_else(|| "Archive entry size overflowed.".to_string())?;
        let size = if relative_end <= archive_size {
            size
        } else if table == EntryTable::RootHfs
            && rows.len() == 0
            && name == "secure"
            && relative_data_offset <= archive_size
            && size % 0x200 == 0
            && relative_end - archive_size <= 0x200
        {
            // The reference XCZ writer rounds the final partition up to a sector
            // but can end the file immediately after the last content entry.
            archive_size - relative_data_offset
        } else {
            return Err(format!(
                "Archive entry '{name}' extends beyond its container."
            ));
        };
        let offset = archive_start
            .checked_add(relative_data_offset)
            .ok_or_else(|| "Archive entry absolute offset overflowed.".to_string())?;
        entries.push(ArchiveEntry { name, offset, size });
    }
    Ok(entries)
}

fn read_u64_at(source: &ReadAtFile, offset: u64) -> Result<u64, String> {
    let mut bytes = [0; 8];
    source.read_exact_at(offset, &mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temporary_file(bytes: &[u8]) -> File {
        let path = std::env::temp_dir().join(format!(
            "ecm-package-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        drop(file);
        let file = File::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        file
    }

    #[test]
    fn bounded_reader_rejects_out_of_range_seeks() {
        let source = ReadAtFile::new(temporary_file(b"0123456789")).unwrap();
        let mut reader = source.bounded(2, 4).unwrap();
        let mut data = [0; 4];
        reader.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"2345");
        assert!(reader.seek(SeekFrom::Start(5)).is_err());
    }

    #[test]
    fn positioned_reads_honor_offsets_when_interleaved() {
        let source = ReadAtFile::new(temporary_file(b"0123456789")).unwrap();
        let mut direct = [0; 2];
        source.read_exact_at(8, &mut direct).unwrap();
        assert_eq!(&direct, b"89");

        let mut left = source.bounded(1, 4).unwrap();
        let mut right = source.bounded(6, 4).unwrap();
        let mut data = [0; 2];
        left.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"12");
        right.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"67");
        left.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"34");
    }

    #[test]
    fn secure_partition_may_omit_final_sector_padding_but_not_content() {
        fn hfs(name: &[u8], size: u64) -> Vec<u8> {
            let mut table = vec![0; 0x200];
            table[..4].copy_from_slice(b"HFS0");
            table[4..8].copy_from_slice(&1_u32.to_le_bytes());
            table[8..12].copy_from_slice(&0x1b0_u32.to_le_bytes());
            table[0x18..0x20].copy_from_slice(&size.to_le_bytes());
            table[0x50..0x50 + name.len()].copy_from_slice(name);
            table
        }
        let mut image = vec![0; 0x200];
        image[0x100..0x104].copy_from_slice(b"HEAD");
        image[0x130..0x138].copy_from_slice(&0x200_u64.to_le_bytes());
        image.extend(hfs(b"secure", 0x400));
        image.extend(hfs(b"program.ncz", 3));
        image.extend_from_slice(b"abc");
        let parse =
            |bytes: &[u8]| parse_package_archive(&ReadAtFile::new(temporary_file(bytes)).unwrap());
        assert_eq!(parse(&image).unwrap().entries[0].size, 3);
        assert!(parse(&image[..image.len() - 1]).is_err());
    }

    #[test]
    fn pfs0_parser_checks_entry_bounds() {
        let mut package = vec![0; 0x10 + 0x18 + 6 + 4];
        package[..4].copy_from_slice(b"PFS0");
        package[4..8].copy_from_slice(&1_u32.to_le_bytes());
        package[8..12].copy_from_slice(&6_u32.to_le_bytes());
        package[0x10..0x18].copy_from_slice(&8_u64.to_le_bytes());
        package[0x18..0x20].copy_from_slice(&4_u64.to_le_bytes());
        package[0x20..0x24].copy_from_slice(&0_u32.to_le_bytes());
        package[0x28..0x2E].copy_from_slice(b"a.nca\0");
        assert!(parse_pfs0(package.as_slice(), package.len() as u64).is_err());
    }
}
