use super::archive::{ArchiveEntry, ReadAtFile};
use nx_archive::formats::{Keyset, TitleKeys};
use std::io::Cursor;

const MAX_KEYS_SIZE: u64 = 2 * 1024 * 1024;
const MAX_TICKET_SIZE: u64 = 1024 * 1024;

pub(super) fn parse_prod_keys(source: &ReadAtFile) -> Result<Keyset, String> {
    let bytes = source.read_vec(0, source.len, MAX_KEYS_SIZE, "prod.keys")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "prod.keys is not valid UTF-8.")?;
    let mut has_header_key = false;

    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with(';')
            || trimmed.starts_with('#')
            || trimmed.starts_with("//")
        {
            continue;
        }
        let (name, value) = trimmed
            .split_once('=')
            .ok_or_else(|| format!("prod.keys line {} is not a key/value pair.", line_index + 1))?;
        let name = name.trim();
        let value = value.split(';').next().unwrap_or_default().trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!(
                "prod.keys line {} has an invalid key name.",
                line_index + 1
            ));
        }
        if value.is_empty()
            || value.len() % 2 != 0
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "prod.keys line {} has an invalid hexadecimal value.",
                line_index + 1
            ));
        }
        if name == "header_key" {
            has_header_key = value.len() == 64;
        }
    }

    if !has_header_key {
        return Err("prod.keys does not contain a valid 32-byte header_key.".into());
    }

    Keyset::from_reader(Cursor::new(bytes))
        .map_err(|error| format!("Could not parse prod.keys: {error}"))
}

pub(super) fn parse_package_tickets(
    source: &ReadAtFile,
    entries: &[ArchiveEntry],
) -> Result<Option<TitleKeys>, String> {
    let mut title_keys = TitleKeys::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.name.to_ascii_lowercase().ends_with(".tik"))
    {
        let ticket = source.read_vec(entry.offset, entry.size, MAX_TICKET_SIZE, "Ticket")?;
        let body_offset = ticket_body_offset(&ticket)?;
        let required_size = body_offset + 0x170;
        if ticket.len() < required_size {
            return Err(format!("Ticket '{}' is truncated.", entry.name));
        }
        let title_key_type = ticket[body_offset + 0x141];
        if title_key_type == 1 {
            return Err(
                "Personalized tickets require console-specific keys and are not supported.".into(),
            );
        }
        if title_key_type != 0 {
            return Err("Package contains an unsupported ticket key type.".into());
        }

        let encrypted_title_key = ticket[body_offset + 0x40..body_offset + 0x50].to_vec();
        let rights_id = &ticket[body_offset + 0x160..body_offset + 0x170];
        if rights_id.iter().all(|byte| *byte == 0) {
            return Err("Package ticket has an empty Rights ID.".into());
        }
        title_keys.add_title_key(&hex::encode_upper(rights_id), encrypted_title_key);
    }

    if title_keys.is_empty() {
        Ok(None)
    } else {
        Ok(Some(title_keys))
    }
}

fn ticket_body_offset(ticket: &[u8]) -> Result<usize, String> {
    let signature_type = ticket
        .get(..4)
        .ok_or_else(|| "Ticket is truncated.".to_string())?;
    let signature_type = u32::from_le_bytes(signature_type.try_into().unwrap());
    match signature_type {
        0x010000 | 0x010003 => Ok(0x240),
        0x010001 | 0x010004 => Ok(0x140),
        0x010002 | 0x010005 => Ok(0x80),
        _ => Err(format!(
            "Package uses unsupported ticket signature type 0x{signature_type:08X}."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_signature_offsets_follow_signature_size() {
        assert_eq!(ticket_body_offset(&0x010000_u32.to_le_bytes()), Ok(0x240));
        assert_eq!(ticket_body_offset(&0x010003_u32.to_le_bytes()), Ok(0x240));
        assert_eq!(ticket_body_offset(&0x010001_u32.to_le_bytes()), Ok(0x140));
        assert_eq!(ticket_body_offset(&0x010004_u32.to_le_bytes()), Ok(0x140));
        assert_eq!(ticket_body_offset(&0x010002_u32.to_le_bytes()), Ok(0x80));
        assert_eq!(ticket_body_offset(&0x010005_u32.to_le_bytes()), Ok(0x80));
    }
}
