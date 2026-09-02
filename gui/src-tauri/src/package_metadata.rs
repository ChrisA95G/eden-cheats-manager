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
