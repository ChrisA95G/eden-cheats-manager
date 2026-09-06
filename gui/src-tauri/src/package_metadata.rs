mod archive;
mod keys;
mod program;

pub(crate) const NO_BUILD_ID_CONTENT_ERROR: &str =
    "This package contains no base application or update metadata. DLC alone cannot determine a Build ID.";

use archive::{parse_package_archive, ReadAtFile};
use nx_archive::formats::Keyset;
use serde::Serialize;
use std::fs::File;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMetadata {
    pub package_format: String,
    pub content_kind: String,
    pub title_id: String,
    pub base_title_id: String,
    pub program_title_id: String,
    pub version: u32,
    pub build_id: String,
    pub module_id: String,
    pub has_bktr: bool,
    pub matched_program_content_id: bool,
}

pub(crate) fn is_package_without_build_id(message: &str) -> bool {
    message == NO_BUILD_ID_CONTENT_ERROR
}

pub(crate) fn validate_package_identity(
    expected_base_title_id: &str,
    metadata: PackageMetadata,
) -> Result<PackageMetadata, String> {
    let expected = expected_base_title_id.trim().to_ascii_uppercase();
    if expected.len() != 16 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            "Expected base Title ID must be exactly 16 ASCII hexadecimal characters.".into(),
        );
    }
    if metadata.base_title_id != expected {
        return Err(format!(
            "Package belongs to {}, not selected game {}.",
            metadata.base_title_id, expected
        ));
    }
    Ok(metadata)
}

pub(crate) struct PackageKeys {
    keyset: Keyset,
}

pub fn discover_package_metadata(
    prod_keys_file: File,
    package_file: File,
) -> Result<PackageMetadata, String> {
    let keys = load_package_keys(prod_keys_file)?;
    discover_package_metadata_with_keys(&keys, package_file)
}

pub(crate) fn load_package_keys(prod_keys_file: File) -> Result<PackageKeys, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let started = std::time::Instant::now();
        let source = ReadAtFile::new(prod_keys_file)?;
        let keyset = keys::parse_prod_keys(&source)?;
        log::debug!("[package] keys parsed in {:?}", started.elapsed());
        Ok(PackageKeys { keyset })
    }))
    .map_err(|_| "The key parser rejected malformed prod.keys data.".to_string())?
}

pub(crate) fn discover_package_metadata_with_keys(
    keys: &PackageKeys,
    package_file: File,
) -> Result<PackageMetadata, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        discover_package_metadata_inner(&keys.keyset, package_file)
    }))
    .map_err(|_| "The package parser rejected malformed archive data.".to_string())?
}

fn discover_package_metadata_inner(
    keyset: &Keyset,
    package_file: File,
) -> Result<PackageMetadata, String> {
    let started = std::time::Instant::now();
    let stage = std::time::Instant::now();
    let package_source = ReadAtFile::new(package_file)?;
    let archive = parse_package_archive(&package_source)?;
    log::debug!(
        "[package] {} index parsed in {:?}",
        archive.format,
        stage.elapsed()
    );

    let stage = std::time::Instant::now();
    let title_keys = keys::parse_package_tickets(&package_source, &archive.entries)?;
    log::debug!("[package] tickets parsed in {:?}", stage.elapsed());

    let stage = std::time::Instant::now();
    let program = program::discover_program_metadata(
        &package_source,
        &archive.entries,
        keyset,
        title_keys.as_ref(),
    )?;
    log::debug!(
        "[package] CNMT and main NSO parsed in {:?}",
        stage.elapsed()
    );
    log::info!("[package] Build ID discovered in {:?}", started.elapsed());

    Ok(PackageMetadata {
        package_format: archive.format.to_string(),
        content_kind: program.content_kind,
        title_id: program.title_id,
        base_title_id: program.base_title_id,
        program_title_id: program.program_title_id,
        version: program.version,
        build_id: program.build_id,
        module_id: program.module_id,
        has_bktr: program.has_bktr,
        matched_program_content_id: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch_metadata() -> PackageMetadata {
        PackageMetadata {
            package_format: "NSP".into(),
            content_kind: "patch".into(),
            title_id: "0100ABCD12345800".into(),
            base_title_id: "0100ABCD12345000".into(),
            program_title_id: "0100ABCD12345801".into(),
            version: 65_536,
            build_id: "0011223344556677".into(),
            module_id: "00112233445566778899AABBCCDDEEFF".into(),
            has_bktr: true,
            matched_program_content_id: true,
        }
    }

    #[test]
    fn package_metadata_keeps_distinct_identity_fields_in_its_json_contract() {
        let metadata = patch_metadata();

        assert_eq!(
            serde_json::to_value(metadata.clone()).unwrap(),
            serde_json::json!({
                "packageFormat": "NSP",
                "contentKind": "patch",
                "titleId": "0100ABCD12345800",
                "baseTitleId": "0100ABCD12345000",
                "programTitleId": "0100ABCD12345801",
                "version": 65_536,
                "buildId": "0011223344556677",
                "moduleId": "00112233445566778899AABBCCDDEEFF",
                "hasBktr": true,
                "matchedProgramContentId": true,
            })
        );
        assert_eq!(metadata.title_id, "0100ABCD12345800");
        assert_eq!(metadata.base_title_id, "0100ABCD12345000");
        assert_eq!(metadata.program_title_id, "0100ABCD12345801");
    }

    #[test]
    fn package_identity_normalizes_the_expected_id_and_returns_metadata_unchanged() {
        let metadata = patch_metadata();
        let before = serde_json::to_value(metadata.clone()).unwrap();

        let validated = validate_package_identity("  0100abcd12345000\n", metadata).unwrap();

        assert_eq!(serde_json::to_value(validated).unwrap(), before);
    }

    #[test]
    fn package_identity_rejects_malformed_expected_ids() {
        let invalid = [
            "",
            "0100ABCD1234500",
            "0100ABCD123450000",
            "0100ABCD12345G00",
            "0100 ABCD1234500",
            "0X00ABCD12345000",
            "０100ABCD12345000",
        ];

        for expected in invalid {
            assert_eq!(
                validate_package_identity(expected, patch_metadata()).unwrap_err(),
                "Expected base Title ID must be exactly 16 ASCII hexadecimal characters.",
                "expected {expected:?} to be rejected"
            );
        }
    }

    #[test]
    fn package_identity_compares_only_the_authoritative_base_id() {
        let mut metadata = patch_metadata();
        metadata.title_id = "0100DEAD00000000".into();
        metadata.program_title_id = "0100DEAD00000000".into();

        assert_eq!(
            validate_package_identity("0100dead00000000", metadata).unwrap_err(),
            "Package belongs to 0100ABCD12345000, not selected game 0100DEAD00000000."
        );
    }
}
