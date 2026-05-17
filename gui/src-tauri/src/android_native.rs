/// Native Android filesystem access — used when the app runs directly on an
/// Android device instead of talking to one over ADB from a desktop.
///
/// The load directory and log path are fixed: Eden always stores its data at
/// these paths on Android.
use crate::adb::parse_build_ids;
use crate::cheats::InstalledCheat;
use crate::db;
use crate::games::GameGroup;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub(crate) const ANDROID_LOAD_DIR: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/load";

const ANDROID_LOG_PATH: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt";

const ANDROID_CONFIG_PATH: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/config/config.ini";

// ── Permission helpers ────────────────────────────────────────────────────────

/// Returns a diagnostic string with full path probing details — useful for debugging.
#[tauri::command]
pub fn android_debug_info() -> String {
    let load = std::path::Path::new(ANDROID_LOAD_DIR);
    let log_p = std::path::Path::new(ANDROID_LOG_PATH);
    let cfg = std::path::Path::new(ANDROID_CONFIG_PATH);

    let mut lines = vec![
        format!("LOAD_DIR: {}", ANDROID_LOAD_DIR),
        format!("  exists={}", load.exists()),
        format!("  parent_exists={}", load.parent().map(|p| p.exists()).unwrap_or(false)),
    ];

    if load.exists() {
        match std::fs::read_dir(load) {
            Ok(entries) => {
                let names: Vec<_> = entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .take(20)
                    .collect();
                lines.push(format!("  contents (first 20): {:?}", names));
            }
            Err(e) => lines.push(format!("  read_dir error: {e}")),
        }
    }

    lines.push(format!("LOG_PATH: {} exists={}", ANDROID_LOG_PATH, log_p.exists()));
    lines.push(format!("CONFIG_PATH: {} exists={}", ANDROID_CONFIG_PATH, cfg.exists()));

    // Storage parent chain
    let mut probe = std::path::Path::new("/storage/emulated/0");
    lines.push(format!("/storage/emulated/0 exists={}", probe.exists()));
    probe = std::path::Path::new("/storage/emulated");
    lines.push(format!("/storage/emulated exists={}", probe.exists()));
    probe = std::path::Path::new("/storage");
    lines.push(format!("/storage exists={}", probe.exists()));

    lines.join("\n")
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoragePermissionStatus {
    pub granted: bool,
    pub message: String,
}

/// Check whether the app has "all files access" (MANAGE_EXTERNAL_STORAGE).
/// On Android this probes the actual filesystem; on desktop always returns true.
#[tauri::command]
pub fn check_storage_permission() -> StoragePermissionStatus {
    #[cfg(target_os = "android")]
    {
        let probe = std::path::Path::new(ANDROID_LOAD_DIR);
        if probe.exists() || probe.parent().map(|p| p.exists()).unwrap_or(false) {
            StoragePermissionStatus { granted: true, message: "Storage access granted.".into() }
        } else {
            StoragePermissionStatus {
                granted: false,
                message: "Storage permission required. Please grant 'All files access' to this app in Settings → Apps → Eden Cheats Manager → Permissions.".into(),
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        StoragePermissionStatus { granted: true, message: "Desktop — no permission needed.".into() }
    }
}

// ── Games ─────────────────────────────────────────────────────────────────────

fn is_valid_tid(s: &str) -> bool {
    s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Scan the on-device Eden load directory directly (no ADB) and return game groups.
#[tauri::command]
pub async fn scan_eden_games_android_native(app: AppHandle) -> Result<Vec<GameGroup>, String> {
    let load_dir = PathBuf::from(ANDROID_LOAD_DIR);
    log::info!("[games::native] load_dir={:?} exists={}", load_dir, load_dir.exists());

    if !load_dir.exists() {
        return Err(format!(
            "Eden load directory not found at {}. Make sure Eden has been launched at least once and that storage permission is granted.",
            ANDROID_LOAD_DIR
        ));
    }

    let entries = std::fs::read_dir(&load_dir).map_err(|e| e.to_string())?;
    let mut installed_ids: HashSet<String> = HashSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if is_valid_tid(&name) && entry.path().is_dir() {
            installed_ids.insert(name);
        }
    }
    log::info!("[games::native] {} valid installed IDs", installed_ids.len());

    // Also check for update TIDs from Eden config (same logic as ADB path but direct FS)
    let update_tids = find_update_tids_native();
    log::info!("[games::native] {} update TIDs from config scan", update_tids.len());
    installed_ids.extend(update_tids);

    let state = app.state::<db::DbState>();
    let mut seen_prefixes: HashSet<String> = HashSet::new();
    for tid in &installed_ids {
        if tid.len() >= 12 { seen_prefixes.insert(tid[..12].to_string()); }
    }

    let mut all_rows = Vec::new();
    for prefix in &seen_prefixes {
        match db::query_base_prefix(&state, prefix) {
            Ok(rows) => all_rows.extend(rows),
            Err(e) => log::warn!("[games::native] prefix {} query error: {}", prefix, e),
        }
    }

    let groups = crate::games::build_groups_pub(all_rows, &installed_ids);
    log::info!("[games::native] {} groups built", groups.len());
    Ok(groups)
}

/// Read Eden's config.ini directly from the filesystem to find update title IDs.
fn find_update_tids_native() -> Vec<String> {
    let config_text = match std::fs::read_to_string(ANDROID_CONFIG_PATH) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("[games::native] could not read config.ini: {e}");
            return Vec::new();
        }
    };

    let mut parents: HashSet<String> = HashSet::new();
    for line in config_text.lines() {
        if !line.contains("\\path=") { continue; }
        let uri = line.splitn(2, '=').nth(1).map(|s| s.trim_matches('"')).unwrap_or("");
        if let Some(physical) = crate::games::content_uri_to_physical_pub(uri) {
            if let Some(parent) = std::path::Path::new(&physical).parent() {
                parents.insert(parent.to_string_lossy().to_string());
            }
        }
    }

    let mut tids = Vec::new();
    for parent in &parents {
        let updates_dir = format!("{}/Updates", parent);
        let dir = match std::fs::read_dir(&updates_dir) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for entry in dir.flatten() {
            let filename = entry.file_name().to_string_lossy().to_string();
            if let Some(tid) = crate::games::extract_update_tid_from_filename_pub(&filename) {
                log::info!("[games::native] update found: {tid} ({filename})");
                tids.push(tid);
            }
        }
    }
    tids
}

// ── Cheats ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn install_cheat_android_native(
    title_id: String,
    cheat_name: String,
    build_id: String,
    content: String,
) -> Result<(), String> {
    log::info!("[cheats::native] install title={title_id} build={build_id} name={cheat_name}");
    let cheats_dir = PathBuf::from(ANDROID_LOAD_DIR)
        .join(&title_id)
        .join(&cheat_name)
        .join("cheats");
    std::fs::create_dir_all(&cheats_dir).map_err(|e| e.to_string())?;
    let file = cheats_dir.join(format!("{}.txt", build_id));
    std::fs::write(&file, content.as_bytes()).map_err(|e| e.to_string())?;
    log::info!("[cheats::native] install OK: {}", file.display());
    Ok(())
}

#[tauri::command]
pub fn list_installed_cheats_android_native(
    title_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    log::debug!("[cheats::native] list title={title_id}");
    let title_dir = PathBuf::from(ANDROID_LOAD_DIR).join(&title_id);
    if !title_dir.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(&title_dir).map_err(|e| e.to_string())?.flatten() {
        let cheat_name = entry.file_name().to_string_lossy().to_string();
        let cheats_dir = entry.path().join("cheats");
        if cheats_dir.is_dir() {
            for f in std::fs::read_dir(&cheats_dir).map_err(|e| e.to_string())?.flatten() {
                let fname = f.file_name().to_string_lossy().to_string();
                if fname.ends_with(".txt") {
                    let build_id = fname.trim_end_matches(".txt").to_uppercase();
                    result.push(InstalledCheat { cheat_name: cheat_name.clone(), build_id });
                }
            }
        }
    }
    log::info!("[cheats::native] list -> {} entries", result.len());
    Ok(result)
}

#[tauri::command]
pub fn delete_cheat_android_native(
    title_id: String,
    cheat_name: String,
    build_id: String,
) -> Result<(), String> {
    log::info!("[cheats::native] delete title={title_id} build={build_id} name={cheat_name}");
    let file = PathBuf::from(ANDROID_LOAD_DIR)
        .join(&title_id)
        .join(&cheat_name)
        .join("cheats")
        .join(format!("{}.txt", build_id));
    if file.exists() {
        std::fs::remove_file(&file).map_err(|e| e.to_string())?;
        let cheats_dir = file.parent().unwrap();
        let cheat_dir = cheats_dir.parent().unwrap();
        let _ = std::fs::remove_dir(cheats_dir);
        let _ = std::fs::remove_dir(cheat_dir);
    }
    log::info!("[cheats::native] delete OK");
    Ok(())
}

// ── Build IDs ─────────────────────────────────────────────────────────────────

/// Extract all build IDs from Eden's log file directly (no ADB).
#[tauri::command]
pub fn extract_build_ids_android_native() -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(ANDROID_LOG_PATH)
        .map_err(|e| format!("Could not read Eden log at {ANDROID_LOG_PATH}: {e}"))?;
    let ids = parse_build_ids(&text);
    if ids.is_empty() {
        Err("No build IDs found in Eden log. Launch a game in Eden first.".into())
    } else {
        Ok(ids)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedBuildIds {
    pub title_id: String,
    pub build_ids: Vec<String>,
}

/// Find build IDs in Eden's log that are associated with the given title_id.
#[tauri::command]
pub fn detect_build_ids_android_native(title_id: String) -> Result<DetectedBuildIds, String> {
    let text = std::fs::read_to_string(ANDROID_LOG_PATH)
        .map_err(|e| format!("Could not read Eden log: {e}"))?;
    let build_ids = crate::build_ids::find_build_ids_for_title_pub(&text, &title_id);
    Ok(DetectedBuildIds { title_id, build_ids })
}

/// Poll Eden's log file for a new build_id entry for the given title, waiting
/// up to `timeout_secs` seconds. Returns the first new build ID found.
#[tauri::command]
pub async fn scan_build_id_android_native(
    title_id: String,
    timeout_secs: Option<u64>,
) -> Result<String, String> {
    let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(90));
    let start = std::time::Instant::now();
    let log_path = PathBuf::from(ANDROID_LOG_PATH);

    // Record build IDs already in the log before we start watching
    let initial: HashSet<String> = if let Ok(text) = std::fs::read_to_string(&log_path) {
        crate::build_ids::find_build_ids_for_title_pub(&text, &title_id)
            .into_iter()
            .collect()
    } else {
        HashSet::new()
    };

    log::info!("[build_ids::native] watching log for title={title_id}, {} initial IDs", initial.len());

    loop {
        if start.elapsed() >= timeout {
            return Err(format!(
                "Timed out after {}s waiting for build ID. Make sure the game is launching in Eden.",
                timeout.as_secs()
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Ok(text) = std::fs::read_to_string(&log_path) {
            let current = crate::build_ids::find_build_ids_for_title_pub(&text, &title_id);
            for bid in &current {
                if !initial.contains(bid) {
                    log::info!("[build_ids::native] new build ID found: {bid}");
                    return Ok(bid.clone());
                }
            }
        }
    }
}
