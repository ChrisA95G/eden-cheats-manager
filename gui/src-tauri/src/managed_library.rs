use crate::{
    db,
    games::{EdenPresenceRecord, GameGroup, TitleEntry},
    package_library::{self, EdenPackageCorrelationResult, GameLibraryScanResult},
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

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

fn not_configured(message: impl Into<String>) -> ManagedLibrarySnapshot {
    ManagedLibrarySnapshot {
        games: Vec::new(),
        package_library: ManagedPackageLibrary::NotConfigured {
            message: message.into(),
        },
    }
}

fn with_package_scan(
    presence: Vec<EdenPresenceRecord>,
    scan: Result<GameLibraryScanResult, String>,
    mut lookup: impl FnMut(&str) -> Option<db::TitleRow>,
) -> ManagedLibrarySnapshot {
    let mut games = Vec::new();
    let package_library = match scan {
        Ok(mut packages) => {
            // Only successfully identified applications/patches define the library.
            for group in &mut packages.games {
                group.versions.retain(|version| {
                    matches!(version.content_kind.as_str(), "application" | "patch")
                });
            }
            packages.games.retain(|group| !group.versions.is_empty());
            let installed: std::collections::HashSet<_> = presence
                .iter()
                .map(|entry| entry.observed_title_id.to_ascii_uppercase())
                .collect();
            for group in &packages.games {
                let title = lookup(&group.base_title_id);
                let name = title
                    .as_ref()
                    .map(|row| row.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| group.versions[0].filename.clone());
                let image = title.map(|row| row.image).unwrap_or_default();
                // CNMT supplies the authoritative base ID, including update-only libraries.
                let base = TitleEntry {
                    title_id: group.base_title_id.clone(),
                    base_title_id: group.base_title_id.clone(),
                    name: name.clone(),
                    image: image.clone(),
                    category: "base".into(),
                    installed: installed.contains(&group.base_title_id),
                };
                let mut update_ids: std::collections::BTreeSet<_> = group
                    .versions
                    .iter()
                    .filter(|version| version.content_kind == "patch")
                    .map(|version| version.title_id.clone())
                    .collect();
                update_ids.extend(
                    presence
                        .iter()
                        .filter(|entry| {
                            entry.resolved_base_title_id.as_deref() == Some(&group.base_title_id)
                                && entry.observed_title_id.ends_with("800")
                        })
                        .map(|entry| entry.observed_title_id.clone()),
                );
                let updates = update_ids
                    .into_iter()
                    .map(|title_id| TitleEntry {
                        installed: installed.contains(&title_id),
                        title_id,
                        base_title_id: group.base_title_id.clone(),
                        name: name.clone(),
                        image: image.clone(),
                        category: "update".into(),
                    })
                    .collect();
                games.push(GameGroup {
                    base_title_id: group.base_title_id.clone(),
                    base_name: name,
                    base_image: image,
                    base_installed: base.installed,
                    base_game: Some(base),
                    updates,
                    dlcs: Vec::new(),
                });
            }
            games.sort_by(|left, right| {
                left.base_name
                    .to_lowercase()
                    .cmp(&right.base_name.to_lowercase())
                    .then_with(|| left.base_title_id.cmp(&right.base_title_id))
            });
            ManagedPackageLibrary::Ready {
                correlation: package_library::correlate_eden_package_inventory(presence, packages),
            }
        }
        Err(message) => ManagedPackageLibrary::Error { message },
    };
    ManagedLibrarySnapshot {
        games,
        package_library,
    }
}

fn package_snapshot(
    app: &AppHandle,
    presence: Vec<EdenPresenceRecord>,
    scan: Result<GameLibraryScanResult, String>,
) -> ManagedLibrarySnapshot {
    let state = app.state::<db::DbState>();
    with_package_scan(presence, scan, |title_id| {
        db::query_base_prefix(&state, title_id)
            .ok()?
            .into_iter()
            .find(|row| row.title_id.eq_ignore_ascii_case(title_id))
    })
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
        if settings.prod_keys_path.trim().is_empty()
            || settings.package_library_path.trim().is_empty()
        {
            return Ok(not_configured(
                "Select prod.keys and a game-package library directory in Settings.",
            ));
        }

        let (_, presence) =
            crate::games::scan_eden_games_pc_with_presence(&app, &settings.pc_load_dir)?;

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

        Ok(package_snapshot(&app, presence, scan))
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
        let package_status = match crate::android_native::get_package_discovery_status() {
            Ok(status) => status,
            Err(message) => return Ok(package_snapshot(&app, Vec::new(), Err(message))),
        };
        let library_status = match crate::android_native::get_game_library_status() {
            Ok(status) => status,
            Err(message) => return Ok(package_snapshot(&app, Vec::new(), Err(message))),
        };
        if let Some(message) = android_package_setup_message(&package_status, &library_status) {
            return Ok(not_configured(message));
        }

        let (_, presence) =
            crate::android_native::scan_eden_games_android_native_with_presence(&app)?;

        let scan = match tauri::async_runtime::spawn_blocking(
            crate::android_native::scan_game_package_library_inner,
        )
        .await
        {
            Ok(scan) => scan,
            Err(error) => Err(format!("Game-library parser task failed: {error}")),
        };

        Ok(package_snapshot(&app, presence, scan))
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
    fn package_setup_state_has_no_eden_fallback() {
        let snapshot = not_configured("Select package setup.");

        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            json!({
                "games": [],
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
            |_| {
                Some(db::TitleRow {
                    title_id: base_title_id.into(),
                    name: game().base_name,
                    image: "image".into(),
                })
            },
        );
        let value = serde_json::to_value(snapshot).unwrap();

        assert_eq!(value["packageLibrary"]["state"], "ready");
        assert_eq!(value["games"][0]["baseName"], "Example");
        assert_eq!(value["games"][0]["baseImage"], "image");
        assert_eq!(value["games"][0]["baseInstalled"], true);
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
    fn package_scan_error_has_no_eden_fallback() {
        let snapshot = with_package_scan(Vec::new(), Err("bad keys".into()), |_| None);
        let value = serde_json::to_value(snapshot).unwrap();

        assert_eq!(value["games"].as_array().unwrap().len(), 0);
        assert_eq!(
            value["packageLibrary"],
            json!({ "state": "error", "message": "bad keys" })
        );
    }

    #[test]
    fn packages_define_cards_without_eden_or_database_membership() {
        let base_id = "0100000000001000";
        let patch_id = "0100000000001800";
        let make_group = |kind: &str, base: &str, title: &str| GameVersionGroup {
            base_title_id: base.into(),
            versions: vec![GameVersionPackage {
                content_kind: kind.into(),
                title_id: title.into(),
                base_title_id: base.into(),
                version: 0,
                build_id: "BUILD".into(),
                module_id: "MODULE".into(),
                package_format: "NSP".into(),
                filename: "Unknown game.nsp".into(),
                relative_path: "Unknown game.nsp".into(),
                size: 42,
            }],
        };
        let make_scan = || {
            Ok(GameLibraryScanResult {
                scanned_packages: 2,
                matched_packages: 1,
                skipped_packages: 1,
                games: vec![
                    make_group("patch", base_id, patch_id),
                    make_group("add_on_content", "0100000000002000", "0100000000002001"),
                ],
                errors: Vec::new(),
            })
        };
        let snapshot = with_package_scan(
            vec![EdenPresenceRecord {
                observed_title_id: "0100000000003000".into(),
                resolved_base_title_id: Some("0100000000003000".into()),
                resolution_issue: None,
            }],
            make_scan(),
            |_| None,
        );
        assert_eq!(
            snapshot.games.len(),
            1,
            "Eden-only and DLC-only titles must not become cards"
        );
        let group = &snapshot.games[0];
        assert_eq!(group.base_title_id, base_id);
        assert_eq!(group.base_name, "Unknown game.nsp");
        assert!(!group.base_installed);
        assert_eq!(group.base_game.as_ref().unwrap().title_id, base_id);
        assert_eq!(group.updates[0].title_id, patch_id);
        assert!(!group.updates[0].installed);
        let present = with_package_scan(
            vec![EdenPresenceRecord {
                observed_title_id: patch_id.into(),
                resolved_base_title_id: None,
                resolution_issue: None,
            }],
            make_scan(),
            |_| None,
        );
        assert!(!present.games[0].base_installed);
        assert!(
            present.games[0].updates[0].installed,
            "Exact patch presence works without database resolution"
        );
        for (observed, expected_updates) in [(patch_id, 1), ("0100000000001001", 0)] {
            let snapshot = with_package_scan(
                vec![EdenPresenceRecord {
                    observed_title_id: observed.into(),
                    resolved_base_title_id: Some(base_id.into()),
                    resolution_issue: None,
                }],
                Ok(GameLibraryScanResult {
                    scanned_packages: 1,
                    matched_packages: 1,
                    skipped_packages: 0,
                    games: vec![make_group("application", base_id, base_id)],
                    errors: Vec::new(),
                }),
                |_| None,
            );
            assert_eq!(snapshot.games.len(), 1);
            assert!(!snapshot.games[0].base_installed);
            assert_eq!(
                snapshot.games[0].updates.len(),
                expected_updates,
                "An Eden update can supply presence; DLC cannot"
            );
        }
        let empty = with_package_scan(
            Vec::new(),
            Ok(GameLibraryScanResult {
                scanned_packages: 0,
                matched_packages: 0,
                skipped_packages: 0,
                games: Vec::new(),
                errors: Vec::new(),
            }),
            |_| None,
        );
        assert!(empty.games.is_empty());
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
