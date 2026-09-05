use crate::{
    games::{EdenPresenceRecord, GameGroup},
    package_library::{self, EdenPackageCorrelationResult, GameLibraryScanResult},
};
use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLibrarySnapshot {
    pub games: Vec<GameGroup>,
    pub package_library: ManagedPackageLibrary,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ManagedPackageLibrary {
    NotConfigured {
        message: String,
    },
    Ready {
        correlation: EdenPackageCorrelationResult,
    },
    Error {
        message: String,
    },
}

fn not_configured(games: Vec<GameGroup>, message: impl Into<String>) -> ManagedLibrarySnapshot {
    ManagedLibrarySnapshot {
        games,
        package_library: ManagedPackageLibrary::NotConfigured {
            message: message.into(),
        },
    }
}

fn with_package_scan(
    games: Vec<GameGroup>,
    presence: Vec<EdenPresenceRecord>,
    scan: Result<GameLibraryScanResult, String>,
) -> ManagedLibrarySnapshot {
    let package_library = match scan {
        Ok(packages) => ManagedPackageLibrary::Ready {
            correlation: package_library::correlate_eden_package_inventory(presence, packages),
        },
        Err(message) => ManagedPackageLibrary::Error { message },
    };
    ManagedLibrarySnapshot {
        games,
        package_library,
    }
}

#[cfg(any(target_os = "android", test))]
fn android_package_setup_message(
    package_status: &crate::android_native::PackageDiscoveryStatus,
    library_status: &crate::android_native::GameLibraryStatus,
) -> Option<&'static str> {
    let keys_ready = package_status.prod_keys_selected
        && package_status.prod_keys_readable
        && package_status.prod_keys_seekable;

    match (keys_ready, library_status.ready) {
        (true, true) => None,
        (false, false) => {
            Some("Select a readable, seekable prod.keys file and a game-package library directory.")
        }
        (false, true) => Some("Select a readable, seekable prod.keys file."),
        (true, false) => Some("Select a readable game-package library directory."),
    }
}

#[tauri::command]
pub async fn scan_managed_library_pc(app: AppHandle) -> Result<ManagedLibrarySnapshot, String> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        return Err("Desktop managed-library scanning is not available on Android.".into());
    }

    #[cfg(not(target_os = "android"))]
    {
        let settings = crate::settings::get_settings(app.clone());
        let (games, presence) =
            crate::games::scan_eden_games_pc_with_presence(&app, &settings.pc_load_dir)?;

        if settings.prod_keys_path.trim().is_empty()
            || settings.package_library_path.trim().is_empty()
        {
            return Ok(not_configured(
                games,
                "Select prod.keys and a game-package library directory in Settings.",
            ));
        }

        let prod_keys_path = std::path::PathBuf::from(settings.prod_keys_path);
        let package_library_path = std::path::PathBuf::from(settings.package_library_path);
        let scan = match tauri::async_runtime::spawn_blocking(move || {
            crate::package_library_pc::scan_game_package_library_pc_inner(
                &prod_keys_path,
                &package_library_path,
            )
        })
        .await
        {
            Ok(scan) => scan,
            Err(error) => Err(format!("Game-library parser task failed: {error}")),
        };

        Ok(with_package_scan(games, presence, scan))
    }
}

#[tauri::command]
pub async fn scan_managed_library_android_native(
    app: AppHandle,
) -> Result<ManagedLibrarySnapshot, String> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        return Err("Native managed-library scanning is only available on Android.".into());
    }

    #[cfg(target_os = "android")]
    {
        let (games, presence) =
            crate::android_native::scan_eden_games_android_native_with_presence(&app)?;
        let package_status = match crate::android_native::get_package_discovery_status() {
            Ok(status) => status,
            Err(message) => return Ok(with_package_scan(games, presence, Err(message))),
        };
        let library_status = match crate::android_native::get_game_library_status() {
            Ok(status) => status,
            Err(message) => return Ok(with_package_scan(games, presence, Err(message))),
        };
        if let Some(message) = android_package_setup_message(&package_status, &library_status) {
            return Ok(not_configured(games, message));
        }

        let scan = match tauri::async_runtime::spawn_blocking(
            crate::android_native::scan_game_package_library_inner,
        )
        .await
        {
            Ok(scan) => scan,
            Err(error) => Err(format!("Game-library parser task failed: {error}")),
        };

        Ok(with_package_scan(games, presence, scan))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        games::TitleEntry,
        package_library::{GameVersionGroup, GameVersionPackage},
    };
    use serde_json::json;

    fn game() -> GameGroup {
        let base = TitleEntry {
            title_id: "0100000000001000".into(),
            base_title_id: "0100000000001000".into(),
            name: "Example".into(),
            image: "image".into(),
            category: "base".into(),
            installed: true,
        };
        GameGroup {
            base_title_id: base.base_title_id.clone(),
            base_name: base.name.clone(),
            base_image: base.image.clone(),
            base_installed: true,
            base_game: Some(base),
            updates: Vec::new(),
            dlcs: Vec::new(),
        }
    }

    #[test]
    fn package_setup_state_preserves_eden_games() {
        let snapshot = not_configured(vec![game()], "Select package setup.");

        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            json!({
                "games": [{
                    "baseTitleId": "0100000000001000",
                    "baseName": "Example",
                    "baseImage": "image",
                    "baseInstalled": true,
                    "baseGame": {
                        "titleId": "0100000000001000",
                        "baseTitleId": "0100000000001000",
                        "name": "Example",
                        "image": "image",
                        "category": "base",
                        "installed": true
                    },
                    "updates": [],
                    "dlcs": []
                }],
                "packageLibrary": {
                    "state": "notConfigured",
                    "message": "Select package setup."
                }
            })
        );
    }

    #[test]
    fn successful_scan_uses_exact_backend_correlation() {
        let base_title_id = "0100000000001000";
        let snapshot = with_package_scan(
            vec![game()],
            vec![EdenPresenceRecord {
                observed_title_id: base_title_id.into(),
                resolved_base_title_id: Some(base_title_id.into()),
                resolution_issue: None,
            }],
            Ok(GameLibraryScanResult {
                scanned_packages: 1,
                matched_packages: 1,
                skipped_packages: 0,
                games: vec![GameVersionGroup {
                    base_title_id: base_title_id.into(),
                    versions: vec![GameVersionPackage {
                        content_kind: "application".into(),
                        title_id: base_title_id.into(),
                        base_title_id: base_title_id.into(),
                        version: 1,
                        build_id: "0011223344556677".into(),
                        module_id: "0011223344556677".into(),
                        package_format: "NSP".into(),
                        filename: "example.nsp".into(),
                        relative_path: "example.nsp".into(),
                        size: 42,
                    }],
                }],
                errors: Vec::new(),
            }),
        );
        let value = serde_json::to_value(snapshot).unwrap();

        assert_eq!(value["packageLibrary"]["state"], "ready");
        assert_eq!(
            value["packageLibrary"]["correlation"]["edenEntries"][0]["packageCandidates"][0]
                ["buildId"],
            "0011223344556677"
        );
        assert_eq!(
            value["packageLibrary"]["correlation"]["unmatchedPackageGroups"],
            json!([])
        );
    }

    #[test]
    fn package_scan_error_preserves_eden_games() {
        let snapshot = with_package_scan(vec![game()], Vec::new(), Err("bad keys".into()));
        let value = serde_json::to_value(snapshot).unwrap();

        assert_eq!(value["games"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["packageLibrary"],
            json!({ "state": "error", "message": "bad keys" })
        );
    }

    #[test]
    fn android_bulk_readiness_ignores_single_package_selection() {
        let package_status = crate::android_native::PackageDiscoveryStatus {
            prod_keys_selected: true,
            prod_keys_name: "prod.keys".into(),
            prod_keys_readable: true,
            prod_keys_seekable: true,
            package_selected: false,
            package_name: String::new(),
            package_readable: false,
            package_seekable: false,
            ready: false,
            message: "Select a game package.".into(),
        };
        let library_status = crate::android_native::GameLibraryStatus {
            selected: true,
            name: "Games".into(),
            read_permission: true,
            readable: true,
            ready: true,
            message: "Ready".into(),
        };

        assert_eq!(
            android_package_setup_message(&package_status, &library_status),
            None
        );
    }
}
