use nx_archive::formats::{
    nca::{encrypt_with_header_key, Nca, NcaHeader},
    Keyset, TitleKeys,
};
use std::io::Cursor;

// AES-128 encryption of a zero block with a zero key.
const ENCRYPTED_ZERO_KEY: [u8; 16] = [
    0x66, 0xe9, 0x4b, 0xd4, 0xef, 0x8a, 0x2c, 0x3b, 0x88, 0x4c, 0xfa, 0x59, 0xca, 0x34, 0x2b, 0x2e,
];
const RIGHTS_ID: [u8; 16] = [0xAB; 16];

fn header(old_generation: u8, generation: u8) -> [u8; 0x340] {
    let mut bytes = [0; 0x340];
    bytes[0x200..0x204].copy_from_slice(b"NCA3");
    bytes[0x205] = 1; // Metadata NCA.
    bytes[0x206] = old_generation;
    bytes[0x208..0x210].copy_from_slice(&0xC00_u64.to_le_bytes());
    bytes[0x220] = generation;
    for block in bytes[0x300..0x340].chunks_exact_mut(16) {
        block.copy_from_slice(&ENCRYPTED_ZERO_KEY);
    }
    bytes
}

fn header_keys() -> Keyset {
    Keyset::from_reader(Cursor::new(format!(
        "header_key = {}{}\n",
        "11".repeat(16),
        "22".repeat(16)
    )))
    .unwrap()
}

fn encrypted_nca(header: &[u8; 0x340], keys: &Keyset) -> Vec<u8> {
    let mut bytes = [0; 0xC00];
    bytes[..header.len()].copy_from_slice(header);
    encrypt_with_header_key(&bytes, keys, 0x200, 0)
}

#[test]
fn key_generation_fields_preserve_old_new_and_future_values() {
    // Include the old enum's 0xFF sentinel to retain its existing behavior.
    for generation in 0..=u8::MAX {
        for (old, current) in [(generation, 0), (0, generation)] {
            let bytes = header(old, current);
            let parsed = NcaHeader::from_bytes(&bytes)
                .unwrap_or_else(|error| panic!("generations {old}/{current}: {error}"));
            assert_eq!(parsed.to_bytes(), bytes, "generations {old}/{current}");
        }
    }
}

#[test]
fn key_index_uses_the_greater_generation_and_preserves_legacy_rules() {
    for (old, current, expected) in [
        (0, 0, 0),
        (0, 1, 0),
        (1, 0, 0),
        (2, 0, 1),
        (2, 18, 17),
        (2, 19, 18),
        (2, 20, 19),
        (2, 21, 20),
        (22, 21, 21),
        (21, 22, 21),
        (0, 32, 31),
    ] {
        let parsed = NcaHeader::from_bytes(&header(old, current)).unwrap();
        assert_eq!(parsed.get_key_generation(), expected, "{old}/{current}");
    }
}

#[test]
fn encrypted_metadata_and_program_headers_use_matching_key_area_keys() {
    for (generation, key_index) in [
        (0, 0),
        (1, 0),
        (19, 18),
        (20, 19),
        (21, 20),
        (22, 21),
        (32, 31),
    ] {
        for (area_index, area_name) in [(0, "application"), (1, "ocean"), (2, "system")] {
            let mut keys = header_keys();
            keys.raw_keys.insert(
                format!("key_area_key_{area_name}_{key_index:02x}"),
                vec![0; 16],
            );
            for content_type in [0, 1] {
                let mut bytes = header(0, generation);
                bytes[0x205] = content_type;
                bytes[0x207] = area_index;
                let nca = Nca::from_reader(Cursor::new(encrypted_nca(&bytes, &keys)), &keys, None)
                    .unwrap();
                assert!(nca.has_valid_keys());
                assert_eq!(nca.get_aes_ctr_decrypt_key().unwrap(), [0; 16]);
            }
        }
    }
}

#[test]
fn encrypted_generation_21_header_uses_titlekek_14() {
    let mut keys = header_keys();
    keys.raw_keys.insert("titlekek_14".into(), vec![0; 16]);
    let mut title_keys = TitleKeys::new();
    title_keys.add_title_key(&hex::encode(RIGHTS_ID), ENCRYPTED_ZERO_KEY.to_vec());
    let mut bytes = header(2, 21);
    bytes[0x230..0x240].copy_from_slice(&RIGHTS_ID);

    let nca = Nca::from_reader(
        Cursor::new(encrypted_nca(&bytes, &keys)),
        &keys,
        Some(&title_keys),
    )
    .unwrap();
    assert!(nca.has_valid_keys());
    assert_eq!(nca.get_aes_ctr_decrypt_key().unwrap(), [0; 16]);
}

#[test]
fn newer_headers_still_require_the_matching_key_area_key() {
    let mut keys = header_keys();
    // Adjacent indices must not be used in place of the required 0x14 key.
    keys.raw_keys
        .insert("key_area_key_application_13".into(), vec![0; 16]);
    keys.raw_keys
        .insert("key_area_key_application_15".into(), vec![0; 16]);
    let nca = Nca::from_reader(
        Cursor::new(encrypted_nca(&header(2, 21), &keys)),
        &keys,
        None,
    )
    .unwrap();
    assert!(!nca.has_valid_keys());
    assert!(nca.get_aes_ctr_decrypt_key().is_err());
}

#[test]
fn newer_headers_still_require_the_matching_titlekek_and_ticket() {
    let mut bytes = header(2, 21);
    bytes[0x230..0x240].copy_from_slice(&RIGHTS_ID);
    for (has_titlekek, has_ticket) in [(false, true), (true, false)] {
        let mut keys = header_keys();
        keys.raw_keys.insert("titlekek_13".into(), vec![0; 16]);
        keys.raw_keys.insert("titlekek_15".into(), vec![0; 16]);
        if has_titlekek {
            keys.raw_keys.insert("titlekek_14".into(), vec![0; 16]);
        }
        let mut title_keys = TitleKeys::new();
        if has_ticket {
            title_keys.add_title_key(&hex::encode(RIGHTS_ID), ENCRYPTED_ZERO_KEY.to_vec());
        }
        let nca = Nca::from_reader(
            Cursor::new(encrypted_nca(&bytes, &keys)),
            &keys,
            Some(&title_keys),
        )
        .unwrap();
        assert!(!nca.has_valid_keys());
        assert!(nca.get_aes_ctr_decrypt_key().is_err());
    }
}

#[test]
fn accepting_new_generations_does_not_skip_header_validation() {
    let mut bytes = header(2, 21);
    bytes[0x200..0x204].copy_from_slice(b"BAD3");
    assert!(NcaHeader::from_bytes(&bytes).is_err());

    let mut bytes = header(2, 21);
    bytes[0x205] = 0xFF; // Invalid content type.
    assert!(NcaHeader::from_bytes(&bytes).is_err());
}
