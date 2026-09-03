pub use crate::package_library::GameLibraryScanResult;
#[cfg(any(target_os = "android", test))]
pub use crate::package_library::{GameLibraryScanError, GameVersionGroup, GameVersionPackage};
#[cfg(target_os = "android")]
use crate::package_metadata::PackageMetadata;
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "android", test))]
use std::collections::BTreeMap;

#[cfg(target_os = "android")]
use super::{
    jni::{jni_noarg_string_call, jni_noarg_void_call, jni_string_int_call, parse_saf_response},
    package_discovery::{file_from_jni_fd, file_from_jni_result},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLibraryStatus {
    pub selected: bool,
    pub name: String,
    pub read_permission: bool,
    pub readable: bool,
    pub ready: bool,
    pub message: String,
}

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryPackageDocument {
    relative_path: String,
    name: String,
    size: u64,
}

#[tauri::command]
pub fn select_game_library_directory() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return jni_noarg_void_call("selectGameLibraryDirectory");
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Game-library selection is only available on Android.".into())
    }
}

#[tauri::command]
pub fn get_game_library_status() -> Result<GameLibraryStatus, String> {
    #[cfg(target_os = "android")]
    {
        let response = parse_saf_response(jni_noarg_string_call("getGameLibraryStatus")?)?;
        return serde_json::from_str(&response)
            .map_err(|error| format!("Invalid game-library status: {error}"));
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Game-library status is only available on Android.".into())
    }
}

#[tauri::command]
pub async fn scan_game_package_library() -> Result<GameLibraryScanResult, String> {
    #[cfg(target_os = "android")]
    {
        return tauri::async_runtime::spawn_blocking(scan_game_package_library_inner)
            .await
            .map_err(|error| format!("Game-library parser task failed: {error}"))?;
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Game-library scanning is only available on Android.".into())
    }
}

#[cfg(target_os = "android")]
fn scan_game_package_library_inner() -> Result<GameLibraryScanResult, String> {
    let response = parse_saf_response(jni_noarg_string_call("listGameLibraryPackages")?)?;
    let documents: Vec<LibraryPackageDocument> = serde_json::from_str(&response)
        .map_err(|error| format!("Invalid game-library package list: {error}"))?;
    let scanned_packages = documents.len();

    let prod_keys = file_from_jni_fd("openProdKeysReadFd", "prod.keys")?;
    let keys = crate::package_metadata::load_package_keys(prod_keys)?;
    let mut matched = Vec::new();
    let mut skipped_packages = 0;
    let mut errors = Vec::new();

    for document in documents {
        let parsed = open_library_package(&document.relative_path).and_then(|package| {
            crate::package_metadata::discover_package_metadata_with_keys(&keys, package)
        });
        match parsed {
            Ok(metadata) => matched.push(version_package(document, metadata)),
            Err(message) if crate::package_metadata::is_package_without_build_id(&message) => {
                skipped_packages += 1;
            }
            Err(message) => errors.push(GameLibraryScanError {
                filename: document.name,
                relative_path: document.relative_path,
                message,
            }),
        }
    }

    let matched_packages = matched.len();
    Ok(GameLibraryScanResult {
        scanned_packages,
        matched_packages,
        skipped_packages,
        games: group_versions(matched),
        errors,
    })
}

#[cfg(target_os = "android")]
fn open_library_package(relative_path: &str) -> Result<std::fs::File, String> {
    let fd = jni_string_int_call("openGameLibraryPackageReadFd", relative_path)?;
    file_from_jni_result(fd, relative_path)
}

#[cfg(target_os = "android")]
fn version_package(
    document: LibraryPackageDocument,
    metadata: PackageMetadata,
) -> GameVersionPackage {
    GameVersionPackage {
        content_kind: metadata.content_kind,
        title_id: metadata.title_id,
        base_title_id: metadata.base_title_id,
        version: metadata.version,
        build_id: metadata.build_id,
        module_id: metadata.module_id,
        package_format: metadata.package_format,
        filename: document.name,
        relative_path: document.relative_path,
        size: document.size,
    }
}

#[cfg(any(target_os = "android", test))]
fn group_versions(packages: Vec<GameVersionPackage>) -> Vec<GameVersionGroup> {
    let mut groups: BTreeMap<String, Vec<GameVersionPackage>> = BTreeMap::new();
    for package in packages {
        groups
            .entry(package.base_title_id.clone())
            .or_default()
            .push(package);
    }

    groups
        .into_iter()
        .map(|(base_title_id, mut versions)| {
            versions.sort_by(|left, right| {
                let left_kind = if left.content_kind == "application" {
                    0
                } else {
                    1
                };
                let right_kind = if right.content_kind == "application" {
                    0
                } else {
                    1
                };
                (left_kind, left.version, &left.filename).cmp(&(
                    right_kind,
                    right.version,
                    &right.filename,
                ))
            });
            GameVersionGroup {
                base_title_id,
                versions,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(
        base_title_id: &str,
        title_id: &str,
        kind: &str,
        version: u32,
        filename: &str,
        size: u64,
    ) -> GameVersionPackage {
        GameVersionPackage {
            content_kind: kind.into(),
            title_id: title_id.into(),
            base_title_id: base_title_id.into(),
            version,
            build_id: format!("BUILD{version}"),
            module_id: "MODULE".into(),
            package_format: "NSP".into(),
            filename: filename.into(),
            relative_path: format!("games/{filename}"),
            size,
        }
    }

    #[test]
    fn groups_versions_deterministically_without_deduplicating_candidates() {
        let groups = group_versions(vec![
            package(
                "0100000000002000",
                "0100000000002800",
                "patch",
                2,
                "z-patch.nsp",
                5,
            ),
            package(
                "0100000000000000",
                "0100000000000000",
                "application",
                4,
                "only-base.nsp",
                1,
            ),
            package(
                "0100000000002000",
                "0100000000002800",
                "patch",
                1,
                "old-patch.nsp",
                2,
            ),
            package(
                "0100000000002000",
                "0100000000002000",
                "application",
                9,
                "base.nsp",
                3,
            ),
            package(
                "0100000000002000",
                "0100000000002800",
                "patch",
                2,
                "a-patch.nsp",
                4,
            ),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].base_title_id, "0100000000000000");
        assert_eq!(groups[0].versions.len(), 1);
        assert_eq!(groups[0].versions[0].filename, "only-base.nsp");
        assert_eq!(groups[1].base_title_id, "0100000000002000");
        assert_eq!(
            groups[1]
                .versions
                .iter()
                .map(|package| (
                    package.content_kind.as_str(),
                    package.version,
                    package.filename.as_str(),
                    package.size,
                ))
                .collect::<Vec<_>>(),
            vec![
                ("application", 9, "base.nsp", 3),
                ("patch", 1, "old-patch.nsp", 2),
                ("patch", 2, "a-patch.nsp", 4),
                ("patch", 2, "z-patch.nsp", 5),
            ]
        );
    }

    #[test]
    fn serializes_the_existing_android_library_result_contract() {
        let result = GameLibraryScanResult {
            scanned_packages: 3,
            matched_packages: 1,
            skipped_packages: 1,
            games: group_versions(vec![package(
                "0100000000002000",
                "0100000000002800",
                "patch",
                7,
                "update.nsp",
                42,
            )]),
            errors: vec![GameLibraryScanError {
                filename: "broken.xci".into(),
                relative_path: "games/broken.xci".into(),
                message: "broken package".into(),
            }],
        };

        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "scannedPackages": 3,
                "matchedPackages": 1,
                "skippedPackages": 1,
                "games": [{
                    "baseTitleId": "0100000000002000",
                    "versions": [{
                        "contentKind": "patch",
                        "titleId": "0100000000002800",
                        "baseTitleId": "0100000000002000",
                        "version": 7,
                        "buildId": "BUILD7",
                        "moduleId": "MODULE",
                        "packageFormat": "NSP",
                        "filename": "update.nsp",
                        "relativePath": "games/update.nsp",
                        "size": 42
                    }]
                }],
                "errors": [{
                    "filename": "broken.xci",
                    "relativePath": "games/broken.xci",
                    "message": "broken package"
                }]
            })
        );
    }
}
