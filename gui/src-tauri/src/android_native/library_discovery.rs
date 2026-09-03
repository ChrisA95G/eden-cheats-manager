pub use crate::package_library::GameLibraryScanResult;
#[cfg(test)]
use crate::package_library::{
    group_versions, GameLibraryScanError, GameVersionGroup, GameVersionPackage,
};
#[cfg(target_os = "android")]
use crate::package_library::{scan_package_library, PackageLibraryEntry};
use serde::{Deserialize, Serialize};

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
    let entries = documents
        .into_iter()
        .map(|document| PackageLibraryEntry {
            source: document.relative_path.clone(),
            filename: document.name,
            relative_path: document.relative_path,
            size: document.size,
        })
        .collect();

    let prod_keys = file_from_jni_fd("openProdKeysReadFd", "prod.keys")?;
    scan_package_library(prod_keys, entries, |relative_path| {
        open_library_package(relative_path)
    })
}

#[cfg(target_os = "android")]
fn open_library_package(relative_path: &str) -> Result<std::fs::File, String> {
    let fd = jni_string_int_call("openGameLibraryPackageReadFd", relative_path)?;
    file_from_jni_result(fd, relative_path)
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
        let groups: Vec<GameVersionGroup> = group_versions(vec![
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
