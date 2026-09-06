use super::archive::{ArchiveEntry, BoundedReader, ReadAtFile};
use nx_archive::formats::{
    cnmt::{Cnmt, ContentMetaType, ExtendedHeader, PackagedContent, PackagedContentType},
    nca::{decrypt_with_header_key, Nca},
    Keyset, TitleKeys,
};
use std::{collections::BTreeSet, io::Cursor};

const MAX_CNMT_SIZE: u64 = 16 * 1024 * 1024;
const NCA_HEADER_SIZE: usize = 0xC00;

pub(super) struct ProgramMetadata {
    pub(super) content_kind: String,
    pub(super) title_id: String,
    pub(super) base_title_id: String,
    pub(super) program_title_id: String,
    pub(super) version: u32,
    pub(super) build_id: String,
    pub(super) module_id: String,
    pub(super) has_bktr: bool,
}

struct SelectedContent<'a> {
    meta: &'a Cnmt,
    program: &'a PackagedContent,
    base_title_id: u64,
    kind: &'static str,
}

struct NcaInspection {
    program_title_id: u64,
    content_type: u8,
    has_bktr: bool,
}

pub(super) fn discover_program_metadata(
    source: &ReadAtFile,
    entries: &[ArchiveEntry],
    keyset: &Keyset,
    title_keys: Option<&TitleKeys>,
) -> Result<ProgramMetadata, String> {
    let cnmts = parse_package_cnmts(source, entries, keyset, title_keys)?;
    let selected = select_program_content(&cnmts)?;

    let content_id = hex::encode(selected.program.info.content_id);
    let expected_name = format!("{content_id}.nca");
    let program_entry = entries
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(&expected_name))
        .ok_or_else(|| {
            format!(
                "The Program NCA selected by CNMT ({expected_name}) is missing from this package."
            )
        })?;

    let inspection = inspect_nca(source, program_entry, keyset)?;
    if inspection.content_type != 0 {
        return Err("The CNMT Program entry does not point to a Program NCA.".into());
    }

    let (module_id, build_id) = extract_main_nso_build_id(
        source.bounded(program_entry.offset, program_entry.size)?,
        keyset,
        title_keys,
    )?;

    Ok(ProgramMetadata {
        content_kind: selected.kind.to_string(),
        title_id: format!("{:016X}", selected.meta.header.title_id),
        base_title_id: format!("{:016X}", selected.base_title_id),
        program_title_id: format!("{:016X}", inspection.program_title_id),
        version: selected.meta.header.title_version,
        build_id,
        module_id,
        has_bktr: inspection.has_bktr,
    })
}

fn parse_package_cnmts(
    source: &ReadAtFile,
    entries: &[ArchiveEntry],
    keyset: &Keyset,
    title_keys: Option<&TitleKeys>,
) -> Result<Vec<Cnmt>, String> {
    let meta_entries: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name.to_ascii_lowercase().ends_with(".cnmt.nca"))
        .collect();
    if meta_entries.is_empty() {
        return Err("Package does not contain a CNMT metadata NCA.".into());
    }

    let mut cnmts = Vec::new();
    for entry in meta_entries {
        let reader = source.bounded(entry.offset, entry.size)?;
        let mut nca = Nca::from_reader(reader, keyset, title_keys)
            .map_err(|error| format!("Could not decrypt metadata NCA '{}': {error}", entry.name))?;
        if !nca.has_valid_keys() {
            return Err(format!(
                "Missing decryption keys for metadata NCA '{}'.",
                entry.name
            ));
        }

        let mut found = false;
        for index in 0..nca.filesystem_count() {
            let Ok(mut filesystem) = nca.open_pfs0_filesystem(index) else {
                continue;
            };
            for file in filesystem
                .list_files()
                .map_err(|error| format!("Could not list CNMT filesystem: {error}"))?
            {
                if !file.name.to_ascii_lowercase().ends_with(".cnmt") {
                    continue;
                }
                if file.size > MAX_CNMT_SIZE {
                    return Err("CNMT metadata is too large to parse safely.".into());
                }
                let bytes = filesystem
                    .read_to_vec(&file)
                    .map_err(|error| format!("Could not read CNMT metadata: {error}"))?;
                let cnmt = Cnmt::from_reader(&mut Cursor::new(bytes))
                    .map_err(|error| format!("Could not parse CNMT metadata: {error}"))?;
                cnmts.push(cnmt);
                found = true;
            }
        }
        if !found {
            return Err(format!(
                "Metadata NCA '{}' contains no readable CNMT.",
                entry.name
            ));
        }
    }
    Ok(cnmts)
}

fn select_program_content(cnmts: &[Cnmt]) -> Result<SelectedContent<'_>, String> {
    let mut application_ids = BTreeSet::new();
    let mut candidates = Vec::new();

    for cnmt in cnmts {
        let (base_title_id, kind) = match (&cnmt.header.meta_type, &cnmt.extended_header) {
            (ContentMetaType::Application, ExtendedHeader::Application(_)) => {
                (cnmt.header.title_id, "application")
            }
            (ContentMetaType::Patch, ExtendedHeader::Patch(header)) => {
                (header.application_id, "patch")
            }
            _ => continue,
        };
        application_ids.insert(base_title_id);
        candidates.push((cnmt, base_title_id, kind));
    }

    if application_ids.is_empty() {
        return Err(super::NO_BUILD_ID_CONTENT_ERROR.into());
    }
    if application_ids.len() != 1 {
        return Err(
            "This package contains multiple applications; select a single-game package.".into(),
        );
    }

    candidates.sort_by_key(|(cnmt, _, kind)| {
        (
            cnmt.header.title_version,
            if *kind == "patch" { 1_u8 } else { 0_u8 },
        )
    });
    let (selected_meta, base_title_id, kind) = candidates.last().copied().unwrap();
    let selected_programs: Vec<_> = selected_meta
        .content_entries
        .iter()
        .filter(|entry| entry.info.content_type == PackagedContentType::Program)
        .collect();

    let program = match selected_programs.as_slice() {
        [program] => *program,
        [] if kind == "patch" => {
            let base_programs: Vec<_> = candidates
                .iter()
                .filter(|(cnmt, _, candidate_kind)| {
                    *candidate_kind == "application" && cnmt.header.title_id == base_title_id
                })
                .flat_map(|(cnmt, _, _)| cnmt.content_entries.iter())
                .filter(|entry| entry.info.content_type == PackagedContentType::Program)
                .collect();
            match base_programs.as_slice() {
                [program] => *program,
                _ => {
                    return Err("This update has no complete Program NCA and its base program is unavailable or ambiguous.".into())
                }
            }
        }
        [] => return Err("Application metadata contains no Program NCA.".into()),
        _ => return Err("Multi-program titles are not supported yet.".into()),
    };

    Ok(SelectedContent {
        meta: selected_meta,
        program,
        base_title_id,
        kind,
    })
}

fn inspect_nca(
    source: &ReadAtFile,
    entry: &ArchiveEntry,
    keyset: &Keyset,
) -> Result<NcaInspection, String> {
    if entry.size < NCA_HEADER_SIZE as u64 {
        return Err("Program NCA is truncated.".into());
    }
    let encrypted = source.read_vec(
        entry.offset,
        NCA_HEADER_SIZE as u64,
        NCA_HEADER_SIZE as u64,
        "NCA header",
    )?;
    let decrypted = decrypt_with_header_key(&encrypted, keyset, 0x200, 0)
        .map_err(|error| format!("Could not decrypt Program NCA header: {error}"))?;
    if &decrypted[0x200..0x203] != b"NCA" || decrypted[0x203] != b'3' {
        return Err("Only NCA3 Program content is supported.".into());
    }

    let content_type = decrypted[0x205];
    let program_title_id = u64::from_le_bytes(decrypted[0x210..0x218].try_into().unwrap());
    let mut has_bktr = false;
    for index in 0..4 {
        let fs_entry = 0x240 + index * 0x10;
        let start = u32::from_le_bytes(decrypted[fs_entry..fs_entry + 4].try_into().unwrap());
        let end = u32::from_le_bytes(decrypted[fs_entry + 4..fs_entry + 8].try_into().unwrap());
        if start == 0 && end == 0 {
            continue;
        }
        let fs_header = 0x400 + index * 0x200;
        let encryption_type = decrypted[fs_header + 4];
        let patch_info = &decrypted[fs_header + 0x100..fs_header + 0x140];
        if matches!(encryption_type, 4 | 6) || patch_info.iter().any(|byte| *byte != 0) {
            has_bktr = true;
        }
    }

    Ok(NcaInspection {
        program_title_id,
        content_type,
        has_bktr,
    })
}

fn extract_main_nso_build_id(
    reader: BoundedReader,
    keyset: &Keyset,
    title_keys: Option<&TitleKeys>,
) -> Result<(String, String), String> {
    let mut nca = Nca::from_reader(reader, keyset, title_keys)
        .map_err(|error| format!("Could not decrypt Program NCA: {error}"))?;
    if !nca.has_valid_keys() {
        return Err("prod.keys is missing the keys required by this Program NCA.".into());
    }

    let mut main_headers = Vec::new();
    for index in 0..nca.filesystem_count() {
        let Ok(mut filesystem) = nca.open_pfs0_filesystem(index) else {
            continue;
        };
        let Some(main) = filesystem.get_file("main") else {
            continue;
        };
        if main.size < 0x100 {
            return Err("The main NSO is truncated.".into());
        }
        let mut header = [0; 0x100];
        filesystem
            .read_buf(&main, &mut header)
            .map_err(|error| format!("Could not read main NSO: {error}"))?;
        validate_nso_header(&header, main.size)?;
        main_headers.push(header);
    }

    let header = match main_headers.as_slice() {
        [header] => header,
        [] => return Err("Program NCA contains no readable main NSO.".into()),
        _ => return Err("Program NCA contains multiple main NSO files.".into()),
    };
    let module_id = &header[0x40..0x60];
    if module_id.iter().all(|byte| *byte == 0) {
        return Err("The main NSO has an empty Module ID.".into());
    }
    Ok((
        hex::encode_upper(module_id),
        hex::encode_upper(&module_id[..8]),
    ))
}

fn validate_nso_header(header: &[u8; 0x100], file_size: u64) -> Result<(), String> {
    if &header[..4] != b"NSO0" {
        return Err("ExeFS 'main' is not a valid NSO0 executable.".into());
    }
    let flags = u32::from_le_bytes(header[0xC..0x10].try_into().unwrap());
    for (index, segment_offset) in [0x10, 0x20, 0x30].into_iter().enumerate() {
        let file_offset = u32::from_le_bytes(
            header[segment_offset..segment_offset + 4]
                .try_into()
                .unwrap(),
        ) as u64;
        let uncompressed_size = u32::from_le_bytes(
            header[segment_offset + 8..segment_offset + 12]
                .try_into()
                .unwrap(),
        ) as u64;
        let compressed_size_offset = 0x60 + index * 4;
        let compressed_size = u32::from_le_bytes(
            header[compressed_size_offset..compressed_size_offset + 4]
                .try_into()
                .unwrap(),
        ) as u64;
        let stored_size = if flags & (1 << index) != 0 {
            compressed_size
        } else {
            uncompressed_size
        };
        let end = file_offset
            .checked_add(stored_size)
            .ok_or_else(|| "NSO segment range overflowed.".to_string())?;
        if end > file_size {
            return Err("NSO segment extends beyond the main executable.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nso_build_id_uses_first_eight_module_id_bytes() {
        let mut header = [0_u8; 0x100];
        header[..4].copy_from_slice(b"NSO0");
        header[0x40..0x60].copy_from_slice(&[0xAB; 0x20]);
        validate_nso_header(&header, 0x100).unwrap();
        assert_eq!(hex::encode_upper(&header[0x40..0x48]), "ABABABABABABABAB");
    }
}
