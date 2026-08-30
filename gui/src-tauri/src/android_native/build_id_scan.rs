use super::jni::{
    launch_uri_from_activity, return_to_foreground, start_scan_service, stop_scan_service,
};
use crate::adb::{loader_build_id_re, parse_build_ids, EDEN_PKG};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::Command;

const EDEN_MAIN_ACTIVITY: &str = "org.yuzu.yuzu_emu.ui.main.MainActivity";
const AM: &str = "/system/bin/am";

const ANDROID_LOG_PATH: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt";

/// Extract all build IDs from Eden's log file directly (no ADB).
#[tauri::command]
pub fn extract_build_ids_android_native() -> Result<Vec<String>, String> {
    let text = native_read_file(ANDROID_LOG_PATH)
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
    let text =
        native_read_file(ANDROID_LOG_PATH).map_err(|e| format!("Could not read Eden log: {e}"))?;
    let build_ids = crate::build_ids::find_build_ids_for_title_in_log(&text, &title_id);
    Ok(DetectedBuildIds {
        title_id,
        build_ids,
    })
}

/// Launch Eden with the given game, poll the log for a new build ID, then
/// force-stop Eden and return to this app. Mirrors `scan_build_id_android`
/// (the ADB version) but runs entirely on-device with no ADB dependency.
#[tauri::command]
pub async fn scan_build_id_android_native(
    title_id: String,
    base_title_id: String,
    game_name: String,
) -> Result<String, String> {
    const SCAN_TIMEOUT: u64 = 5;
    const POLL_INTERVAL_MS: u64 = 2000;
    const KEYS_READY_TIMEOUT: u64 = 25;

    log::info!("[build_ids::native] scan_build_id title={title_id} base={base_title_id}");

    // Use base_title_id for ROM lookup — update/DLC NSPs can't be launched directly.
    let lookup_id = if !base_title_id.is_empty() {
        base_title_id.as_str()
    } else {
        title_id.as_str()
    };
    // Disable fuzzy name matching when searching by base_title_id to avoid finding Update NSPs.
    let lookup_name = if lookup_id != title_id.as_str() {
        ""
    } else {
        game_name.as_str()
    };
    log::info!("[build_ids::native] lookup_id={lookup_id} lookup_name={lookup_name:?}");

    // 1. Find the ROM's URI (content:// for SD-card games, file:// for internal).
    let rom_uri = find_rom_path_native(lookup_id, lookup_name).ok_or_else(|| {
        format!(
            "ROM not found for [{lookup_id}]. \
             Make sure the base game ROM is in a directory configured in Eden's settings."
        )
    })?;
    log::info!("[build_ids::native] rom_uri={rom_uri}");

    // 2. Start a foreground service so that Android 12+ background activity launch
    //    restrictions don't block our returnToApp() call later.
    if let Err(e) = start_scan_service() {
        log::warn!("[build_ids::native] start_scan_service failed (non-fatal): {e}");
    } else {
        log::info!("[build_ids::native] scan service started");
    }

    // 3. Force-stop Eden for a clean process state.
    //    May fail on stock Android (requires FORCE_STOP_PACKAGES), ignored either way.
    let _ = Command::new(AM).args(["force-stop", EDEN_PKG]).output();
    log::info!("[build_ids::native] force-stop sent");

    // 4. Warm up MainActivity so NativeLibrary.reloadKeys() runs before we launch
    //    the game. Without this, EmulationActivity returns "No bootable game found".
    let main_component = format!("{EDEN_PKG}/{EDEN_MAIN_ACTIVITY}");
    Command::new(AM)
        .args(["start", "-W", "-n", &main_component])
        .output()
        .map_err(|e| format!("Failed to launch Eden: {e}"))?;
    log::info!("[build_ids::native] MainActivity launched");

    // 4. Wait for Eden's library scan / key loading to finish writing to the log.
    poll_for_keys_ready_native(KEYS_READY_TIMEOUT).await;
    log::info!("[build_ids::native] warmup complete");

    // 5. Record log baseline before the game launch.
    let start_line = get_local_line_count(ANDROID_LOG_PATH).unwrap_or(0);
    log::info!("[build_ids::native] baseline={start_line} lines");

    // 6. Launch EmulationActivity via JNI → MainActivity.launchIntent().
    //    `am start -d URI` (subprocess) always exits 255 from an app process because
    //    ActivityManagerService does URI permission checks that reject non-shell callers.
    //    Calling startActivity() directly from the Activity object bypasses this.
    launch_uri_from_activity(&rom_uri).map_err(|e| {
        let _ = Command::new(AM).args(["force-stop", EDEN_PKG]).output();
        e
    })?;
    log::info!("[build_ids::native] EmulationActivity launch dispatched");

    // 7. Poll the log for a new build ID.
    let title_id_poll = title_id.clone();
    let found = tokio::time::timeout(std::time::Duration::from_secs(SCAN_TIMEOUT), async move {
        let mut last_known = start_line;
        let mut poll_n = 0u32;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
            poll_n += 1;

            let Ok(text) = native_read_file(ANDROID_LOG_PATH) else {
                continue;
            };
            let current = text.lines().count() as u64;
            let read_from = if current < last_known { 0 } else { last_known } as usize;
            let new_lines: Vec<&str> = text.lines().skip(read_from).collect();

            log::info!(
                "[build_ids::native] poll#{poll_n} read_from={read_from} \
                     current={current} new_lines={}",
                new_lines.len()
            );

            // Primary: strict NSO-loader match — only fires during real emulation.
            for line in &new_lines {
                if let Some(cap) = loader_build_id_re().captures(line) {
                    let full = &cap[1];
                    let bid = full[..16.min(full.len())].to_uppercase();
                    log::info!("[build_ids::native] found via name=main: {bid}");
                    return Some(bid);
                }
            }

            // Fallback: title-context window (after half timeout).
            let elapsed_ms = poll_n as u64 * POLL_INTERVAL_MS;
            if elapsed_ms >= SCAN_TIMEOUT * 500 {
                let chunk = new_lines.join("\n");
                if let Some(bid) =
                    crate::build_ids::find_build_ids_for_title_in_log(&chunk, &title_id_poll)
                        .into_iter()
                        .next()
                {
                    log::info!("[build_ids::native] found via title fallback: {bid}");
                    return Some(bid);
                }
            }

            last_known = current;
        }
    })
    .await
    .unwrap_or(None);

    // 8. Stop Eden.
    //    First try `am force-stop` — succeeds on rooted/OEM devices that grant
    //    FORCE_STOP_PACKAGES; silently fails on stock Android (SecurityException inside am).
    //    Then bring our app back to the foreground so onResume() can attempt
    //    killBackgroundProcesses() as a secondary fallback (only kills cached processes,
    //    not apps holding a foreground service, but handles the rooted case too).
    let force_stop = Command::new(AM).args(["force-stop", EDEN_PKG]).output();
    log::info!(
        "[build_ids::native] am force-stop result: {:?}",
        force_stop.as_ref().map(|o| o.status)
    );
    let _ = return_to_foreground();
    let _ = stop_scan_service();

    match found {
        Some(bid) => {
            log::info!("[build_ids::native] scan complete title={title_id} -> {bid}");
            Ok(bid)
        }
        None => Err(format!(
            "Build ID not found within {SCAN_TIMEOUT}s. \
             The game may still be loading — try 'Detect' after launching it manually."
        )),
    }
}

/// Search Eden's configured game directories (read from config.ini) for a ROM
/// file whose filename contains `[{title_id}]`. Returns a content:// URI for
/// SD-card games (required by Eden's ContentResolver) or file:// for internal
/// storage. The URI is passed to Eden via JNI startActivity(), which can handle
/// content:// URIs that `am start -d` (subprocess) cannot.
fn find_rom_path_native(title_id: &str, game_name: &str) -> Option<String> {
    let config = native_read_file(crate::adb::ANDROID_CONFIG_PATH).ok()?;
    let tid_lower = title_id.to_lowercase();
    let name_norm = crate::rom_cache::normalize(game_name);

    let mut tree_entries: Vec<(String, String)> = Vec::new();
    let mut search_dirs: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for line in config.lines() {
        if !line.contains("\\path=") {
            continue;
        }
        let raw_uri = line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"');
        if let Some(physical) = crate::games::content_uri_to_physical(raw_uri) {
            if let Some(parent) = std::path::Path::new(&physical).parent() {
                let p = parent.to_string_lossy().to_string();
                if seen.insert(p.clone()) {
                    search_dirs.push(p);
                }
            }
            if seen.insert(physical.clone()) {
                search_dirs.push(physical.clone());
            }
            tree_entries.push((raw_uri.to_string(), physical));
        }
    }

    for dir in &search_dirs {
        if let Some(physical) = search_dir_for_rom(dir, &tid_lower, &name_norm, 2) {
            if let Some(uri) = crate::build_ids::physical_to_content_uri(&physical, &tree_entries) {
                return Some(uri);
            }
            return Some(format!("file://{}", physical));
        }
    }
    None
}

/// Direct Eden log/config access remains disabled while build-ID discovery is redesigned.
fn native_read_file(path: &str) -> Result<String, String> {
    #[cfg(target_os = "android")]
    if path.contains("/Android/data/") {
        return Err(
            "Direct Eden data access is unavailable; build-ID discovery still needs migration."
                .into(),
        );
    }
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

/// Recursively search `dir` up to `depth` levels for an NSP/XCI file whose
/// name contains `[{tid_lower}]` or fuzzy-matches `name_norm`. Returns the physical path on success.
fn search_dir_for_rom(dir: &str, tid_lower: &str, name_norm: &str, depth: u32) -> Option<String> {
    let needle = format!("[{}]", tid_lower);
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && depth > 0 {
            if let Some(found) =
                search_dir_for_rom(&path.to_string_lossy(), tid_lower, name_norm, depth - 1)
            {
                return Some(found);
            }
        } else if path.is_file() {
            let fname = path.file_name()?.to_string_lossy().to_lowercase();
            let ext = path.extension()?.to_string_lossy().to_lowercase();
            if ext != "nsp" && ext != "xci" {
                continue;
            }
            let by_tid = fname.contains(&needle);
            let by_name = !name_norm.is_empty()
                && crate::rom_cache::normalize(&fname).contains(name_norm)
                && !crate::build_ids::is_non_base_filename(&fname);
            if by_tid || by_name {
                if by_name && !by_tid {
                    log::info!(
                        "[build_ids::native] fuzzy match '{fname}' for name_norm='{name_norm}'"
                    );
                }
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Count lines in a local file. Used to establish a log baseline.
fn get_local_line_count(path: &str) -> Option<u64> {
    native_read_file(path)
        .ok()
        .map(|t| t.lines().count() as u64)
}

/// Poll the log line-count until it is stable (no growth for 2 s) or the
/// hard timeout elapses. Mirrors `poll_for_keys_ready` in build_ids.rs.
async fn poll_for_keys_ready_native(timeout_secs: u64) {
    const POLL_MS: u64 = 500;
    const STABLE_POLLS: u32 = 4;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut prev: Option<u64> = None;
    let mut stable = 0u32;
    loop {
        if std::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
        let cur = get_local_line_count(ANDROID_LOG_PATH);
        match (prev, cur) {
            (Some(p), Some(c)) if c == p => {
                stable += 1;
                if stable >= STABLE_POLLS {
                    return;
                }
            }
            _ => {
                stable = 0;
                prev = cur;
            }
        }
    }
}
