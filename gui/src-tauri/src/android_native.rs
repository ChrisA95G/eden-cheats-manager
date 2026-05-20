/// Native Android filesystem access — used when the app runs directly on an
/// Android device instead of talking to one over ADB from a desktop.
///
/// The load directory and log path are fixed: Eden always stores its data at
/// these paths on Android.
use crate::adb::{loader_build_id_re, parse_build_ids, ANDROID_CONFIG_PATH, EDEN_PKG};
use crate::cheats::InstalledCheat;
use crate::db;
use crate::games::GameGroup;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Manager};

const EDEN_MAIN_ACTIVITY: &str = "org.yuzu.yuzu_emu.ui.main.MainActivity";
const AM: &str = "/system/bin/am";

pub(crate) const ANDROID_LOAD_DIR: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/load";

const ANDROID_LOG_PATH: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt";

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
/// On Android 11+ uses Environment.isExternalStorageManager() via JNI;
/// on desktop always returns true.
#[tauri::command]
pub fn check_storage_permission() -> StoragePermissionStatus {
    #[cfg(target_os = "android")]
    {
        if jni_has_all_files_access() {
            StoragePermissionStatus { granted: true, message: "Storage access granted.".into() }
        } else {
            StoragePermissionStatus {
                granted: false,
                message: "Storage permission required. Tap 'Open Settings' to grant 'All files access' (under Special app access, not Permissions).".into(),
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        StoragePermissionStatus { granted: true, message: "Desktop — no permission needed.".into() }
    }
}

/// Open the Android system page for MANAGE_EXTERNAL_STORAGE.
/// No-op on non-Android builds.
#[tauri::command]
pub fn open_storage_settings() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        jni_open_storage_settings()
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

// ── Games ─────────────────────────────────────────────────────────────────────

/// Scan the on-device Eden load directory directly (no ADB) and return game groups.
#[tauri::command]
pub async fn scan_eden_games_android_native(app: AppHandle) -> Result<Vec<GameGroup>, String> {
    let load_dir = PathBuf::from(ANDROID_LOAD_DIR);
    let load_dir_str = load_dir.to_string_lossy().to_string();

    // On API ≥ 34 fail fast with a clear Shizuku error before any fs probing.
    #[cfg(target_os = "android")]
    ensure_shizuku_for_blocked_path(&load_dir_str)?;

    log::info!("[games::native] load_dir={:?} exists={}", load_dir, native_path_exists(&load_dir_str));

    if !native_path_exists(&load_dir_str) {
        // Check whether Eden's app data dir exists at all — if not, Eden isn't installed.
        let eden_data = load_dir
            .parent().and_then(|p| p.parent()) // .../files/load -> .../files -> .../dev.eden.eden_emulator
            .map(|p| native_path_exists(&p.to_string_lossy()))
            .unwrap_or(false);

        if !eden_data {
            return Err(
                "Eden emulator is not installed or its data directory is not accessible. \
                Install Eden and launch it once to create the required directories, \
                then tap Scan again.".into()
            );
        }

        // Eden is installed but never launched — create the load dir ourselves.
        if let Err(e) = native_mkdirs(&load_dir_str) {
            return Err(format!(
                "Eden is installed but its load directory couldn't be created. \
                Launch Eden once to initialise its data directory, then tap Scan again. ({})",
                e
            ));
        }
        log::info!("[games::native] created load_dir: {:?}", load_dir);
    }

    let names = native_list_dir_names(&load_dir_str)?;
    let mut installed_ids: HashSet<String> = HashSet::new();
    for name in names {
        if crate::games::is_valid_tid(&name) {
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

    let groups = crate::games::build_groups(all_rows, &installed_ids);
    log::info!("[games::native] {} groups built", groups.len());
    crate::games::save_game_cache(&app, "android", &groups);
    Ok(groups)
}

/// Read Eden's config.ini directly from the filesystem to find update title IDs.
fn find_update_tids_native() -> Vec<String> {
    let config_text = match native_read_file(ANDROID_CONFIG_PATH) {
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
        if let Some(physical) = crate::games::content_uri_to_physical(uri) {
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
            if let Some(tid) = crate::games::extract_update_tid_from_filename(&filename) {
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
    let file = PathBuf::from(ANDROID_LOAD_DIR)
        .join(&title_id)
        .join(&cheat_name)
        .join("cheats")
        .join(format!("{}.txt", build_id));
    native_write_file(&file.to_string_lossy(), content.as_bytes())?;
    log::info!("[cheats::native] install OK: {}", file.display());
    Ok(())
}

#[tauri::command]
pub fn list_installed_cheats_android_native(
    title_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    log::debug!("[cheats::native] list title={title_id}");
    let title_dir = PathBuf::from(ANDROID_LOAD_DIR).join(&title_id);
    let title_dir_str = title_dir.to_string_lossy().to_string();

    // On API ≥ 34 with a blocked path, use Shizuku's `find` to enumerate in one call.
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && title_dir_str.contains("/Android/data/") {
        ensure_shizuku_for_blocked_path(&title_dir_str)?;
        if !jni_shizuku_path_exists(&title_dir_str) {
            return Ok(Vec::new());
        }
        let out = jni_shizuku_find_txt_files(&title_dir_str)?;
        let mut result = Vec::new();
        for line in out.lines().filter(|l| !l.trim().is_empty()) {
            let path = std::path::Path::new(line.trim());
            let Some(fname) = path.file_name() else { continue };
            let fname_str = fname.to_string_lossy();
            if !fname_str.ends_with(".txt") { continue; }
            let Some(cheats_dir) = path.parent() else { continue };
            if cheats_dir.file_name().map(|n| n != "cheats").unwrap_or(true) { continue; }
            let Some(cheat_name_dir) = cheats_dir.parent() else { continue };
            let cheat_name = cheat_name_dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let build_id = fname_str.trim_end_matches(".txt").to_uppercase();
            result.push(InstalledCheat { cheat_name, build_id });
        }
        log::info!("[cheats::native] list (shizuku) -> {} entries", result.len());
        return Ok(result);
    }

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
    let file_str = file.to_string_lossy().to_string();

    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && file_str.contains("/Android/data/") {
        ensure_shizuku_for_blocked_path(&file_str)?;
        jni_shizuku_delete_file(&file_str)?;
        // Best-effort cleanup of empty parent dirs.
        if let Some(cheats_dir) = file.parent() {
            jni_shizuku_rmdir(&cheats_dir.to_string_lossy());
            if let Some(cheat_dir) = cheats_dir.parent() {
                jni_shizuku_rmdir(&cheat_dir.to_string_lossy());
            }
        }
        log::info!("[cheats::native] delete OK (shizuku)");
        return Ok(());
    }

    match std::fs::remove_file(&file) {
        Ok(()) => {
            let cheats_dir = file.parent().unwrap();
            let _ = std::fs::remove_dir(cheats_dir);
            let _ = std::fs::remove_dir(cheats_dir.parent().unwrap());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    log::info!("[cheats::native] delete OK");
    Ok(())
}

// ── Build IDs ─────────────────────────────────────────────────────────────────

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
    let text = native_read_file(ANDROID_LOG_PATH)
        .map_err(|e| format!("Could not read Eden log: {e}"))?;
    let build_ids = crate::build_ids::find_build_ids_for_title_in_log(&text, &title_id);
    Ok(DetectedBuildIds { title_id, build_ids })
}

/// Launch Eden with the given game, poll the log for a new build ID, then
/// force-stop Eden and return to this app. Mirrors `scan_build_id_android`
/// (the ADB version) but runs entirely on-device with no ADB dependency.
#[tauri::command]
pub async fn scan_build_id_android_native(title_id: String, base_title_id: String, game_name: String) -> Result<String, String> {
    const SCAN_TIMEOUT: u64 = 5;
    const POLL_INTERVAL_MS: u64 = 2000;
    const KEYS_READY_TIMEOUT: u64 = 25;

    log::info!("[build_ids::native] scan_build_id title={title_id} base={base_title_id}");

    // Use base_title_id for ROM lookup — update/DLC NSPs can't be launched directly.
    let lookup_id = if !base_title_id.is_empty() { base_title_id.as_str() } else { title_id.as_str() };
    // Disable fuzzy name matching when searching by base_title_id to avoid finding Update NSPs.
    let lookup_name = if lookup_id != title_id.as_str() { "" } else { game_name.as_str() };
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
    //    the game.  Without this, EmulationActivity returns "No bootable game found".
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
    launch_uri_from_activity(&rom_uri)
        .map_err(|e| {
            let _ = Command::new(AM).args(["force-stop", EDEN_PKG]).output();
            e
        })?;
    log::info!("[build_ids::native] EmulationActivity launch dispatched");

    // 7. Poll the log for a new build ID.
    let title_id_poll = title_id.clone();
    let found = tokio::time::timeout(
        std::time::Duration::from_secs(SCAN_TIMEOUT),
        async move {
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
                    if let Some(bid) = crate::build_ids::find_build_ids_for_title_in_log(&chunk, &title_id_poll).into_iter().next() {
                        log::info!("[build_ids::native] found via title fallback: {bid}");
                        return Some(bid);
                    }
                }

                last_known = current;
            }
        },
    )
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

// JNI_OnLoad is called by the JVM when it loads libgui_lib.so (triggered by
// `System.loadLibrary("gui_lib")` in Rust.kt, before any Tauri command runs).
// We capture two things here because non-UI Tokio threads have a different
// class loader and cannot use find_class() to find app-level classes.
#[cfg(target_os = "android")]
static JVM_PTR: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

// GlobalRef to MainActivity class — captured while we're on the UI thread
// where the correct class loader is active.  Reused on Tokio threads.
#[cfg(target_os = "android")]
static MAIN_CLASS: std::sync::OnceLock<jni::objects::GlobalRef> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn JNI_OnLoad(
    vm: *mut jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) -> jni::sys::jint {
    let _ = JVM_PTR.set(vm as usize);
    // get_env() works here because JNI_OnLoad is called on an already-attached
    // Java thread; find_class works because the app class loader is active.
    'capture: {
        let Ok(vm_ref) = (unsafe { jni::JavaVM::from_raw(vm) }) else { break 'capture };
        let Ok(mut env) = vm_ref.get_env() else { break 'capture };
        let Ok(cls) = env.find_class("dev/eden/cheats_manager/MainActivity") else { break 'capture };
        let Ok(global) = env.new_global_ref(cls) else { break 'capture };
        let _ = MAIN_CLASS.set(global);
    }
    jni::sys::JNI_VERSION_1_6
}

/// Attach to the JVM, resolve the cached MainActivity class, and run `f`.
/// Keeps lifetimes correct by scoping env/jcls inside the call.
#[cfg(target_os = "android")]
fn with_main_class<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut jni::JNIEnv, &jni::objects::JClass) -> Result<T, String>,
{
    use jni::{objects::JClass, JavaVM};
    let ptr = JVM_PTR
        .get()
        .copied()
        .ok_or_else(|| "JVM not captured (JNI_OnLoad not called)".to_string())?
        as *mut jni::sys::JavaVM;
    let vm = unsafe { JavaVM::from_raw(ptr) }.map_err(|e| format!("JVM: {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("JNI attach: {e}"))?;
    let cls_global = MAIN_CLASS
        .get()
        .ok_or_else(|| "MainActivity class not captured".to_string())?;
    let jcls = unsafe { JClass::from_raw(cls_global.as_raw()) };
    f(&mut env, &jcls)
}

/// Call `MainActivity.hasAllFilesAccess()` — returns true when
/// MANAGE_EXTERNAL_STORAGE is granted (Android 11+) or unconditionally on older APIs.
#[cfg(target_os = "android")]
fn jni_has_all_files_access() -> bool {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "hasAllFilesAccess", "()Z", &[])
            .map_err(|e| format!("JNI hasAllFilesAccess: {e}"))
            .and_then(|v| v.z().map_err(|e| format!("JNI bool: {e}")))
    })
    .unwrap_or(false)
}

/// Call `MainActivity.openStorageSettings()` to launch the system
/// MANAGE_APP_ALL_FILES_ACCESS_PERMISSION settings page.
#[cfg(target_os = "android")]
fn jni_open_storage_settings() -> Result<(), String> {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "openStorageSettings", "()V", &[])
            .map_err(|e| format!("JNI openStorageSettings: {e}"))?;
        Ok(())
    })
}

// ── Shizuku / API-level infrastructure ───────────────────────────────────────
// On API ≤ 33 all blocked-path helpers fall through to direct std::fs.
// On API ≥ 34 any path inside Android/data/ is routed through Shizuku
// (ADB shell uid=2000 — confirmed to bypass the platform restriction).
// Paths outside Android/data/ (user ROM dirs, etc.) always use direct std::fs.

#[cfg(target_os = "android")]
static API_LEVEL: std::sync::OnceLock<i32> = std::sync::OnceLock::new();

fn get_api_level() -> i32 {
    #[cfg(target_os = "android")]
    return *API_LEVEL.get_or_init(|| {
        with_main_class(|env, jcls| {
            env.call_static_method(jcls, "getApiLevel", "()I", &[])
                .map_err(|e| format!("JNI getApiLevel: {e}"))
                .and_then(|v| v.i().map_err(|e| format!("JNI int: {e}")))
        })
        .unwrap_or(0)
    });
    #[cfg(not(target_os = "android"))]
    0
}

/// Returns Ok if (a) API < 34 or (b) Shizuku is running and permission is granted.
fn ensure_shizuku_for_blocked_path(path: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && path.contains("/Android/data/") {
        let api = get_api_level();

        // Android 17+ — Shizuku has no release for this version yet.
        if api >= 37 {
            return Err(format!(
                "Your device runs Android API {api}, which is not yet supported by Shizuku.\n\
                 Native mode cannot access Eden's data folder on this Android version.\n\n\
                 Workaround: use ADB mode (requires a PC with ADB)."
            ));
        }

        if !jni_is_shizuku_available() {
            return Err(if api >= 36 {
                // Android 16 — Play Store build lags behind; GitHub APK needed.
                "Android 16 blocks Eden's data folder. Shizuku is required, but the \
                 Play Store version does not support Android 16 yet.\n\n\
                 Install the latest APK directly from GitHub:\n\
                 github.com/RikkaApps/Shizuku/releases\n\n\
                 Then: Open Shizuku → Start via Wireless ADB → tap Grant Access."
                    .to_string()
            } else {
                // Android 14–15 — Play Store build works fine.
                "Android 14+ blocks Eden's data folder. Shizuku is required.\n\n\
                 1. Enable Developer Options on your phone\n\
                 2. Enable Wireless Debugging\n\
                 3. Install Shizuku from the Play Store (or GitHub for latest)\n\
                 4. Open Shizuku → Start via Wireless ADB\n\
                 5. Tap Grant Access here"
                    .to_string()
            });
        }
        if !jni_is_shizuku_granted() {
            return Err(
                "Shizuku is running but access is not granted yet.\n\
                 Tap Grant Access below."
                    .into(),
            );
        }
    }
    Ok(())
}

// ── Shizuku JNI helpers (android-only) ───────────────────────────────────────

#[cfg(target_os = "android")]
fn jni_is_shizuku_available() -> bool {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "isShizukuAvailable", "()Z", &[])
            .map_err(|e| format!("JNI isShizukuAvailable: {e}"))
            .and_then(|v| v.z().map_err(|e| format!("JNI bool: {e}")))
    })
    .unwrap_or(false)
}

#[cfg(target_os = "android")]
fn jni_is_shizuku_granted() -> bool {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "isShizukuGranted", "()Z", &[])
            .map_err(|e| format!("JNI isShizukuGranted: {e}"))
            .and_then(|v| v.z().map_err(|e| format!("JNI bool: {e}")))
    })
    .unwrap_or(false)
}

#[cfg(target_os = "android")]
fn jni_shizuku_read_file(path: &str) -> Result<String, String> {
    use jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let path_j = env.new_string(path).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                "shizukuReadFile",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&JObject::from(path_j))],
            )
            .map_err(|e| format!("JNI shizukuReadFile: {e}"))?;
        let jobj = result.l().map_err(|e| format!("JNI object: {e}"))?;
        if jobj.is_null() {
            return Err(format!("Cannot read {path} via Shizuku (missing or denied)"));
        }
        let s: String = env
            .get_string(&JString::from(jobj))
            .map_err(|e| format!("JNI string: {e}"))?
            .into();
        Ok(s)
    })
}

#[cfg(target_os = "android")]
fn jni_shizuku_list_dir(path: &str) -> Result<String, String> {
    use jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let path_j = env.new_string(path).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                "shizukuListDir",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&JObject::from(path_j))],
            )
            .map_err(|e| format!("JNI shizukuListDir: {e}"))?;
        let jobj = result.l().map_err(|e| format!("JNI object: {e}"))?;
        env.get_string(&JString::from(jobj))
            .map(|s| s.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
fn jni_shizuku_find_txt_files(dir: &str) -> Result<String, String> {
    use jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let dir_j = env.new_string(dir).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                "shizukuFindTxtFiles",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&JObject::from(dir_j))],
            )
            .map_err(|e| format!("JNI shizukuFindTxtFiles: {e}"))?;
        let jobj = result.l().map_err(|e| format!("JNI object: {e}"))?;
        env.get_string(&JString::from(jobj))
            .map(|s| s.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
fn jni_shizuku_write_file(path: &str, content: &str) -> Result<(), String> {
    use jni::objects::{JObject, JValue};
    with_main_class(|env, jcls| {
        let path_j = env.new_string(path).map_err(|e| format!("JNI string: {e}"))?;
        let content_j = env.new_string(content).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                "shizukuWriteFile",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(&JObject::from(path_j)),
                    JValue::Object(&JObject::from(content_j)),
                ],
            )
            .map_err(|e| format!("JNI shizukuWriteFile: {e}"))?;
        let ok = result.z().map_err(|e| format!("JNI bool: {e}"))?;
        if ok { Ok(()) } else { Err(format!("shizukuWriteFile({path}) returned false")) }
    })
}

// Helper: call a Shizuku JNI method `(String)Z`.
#[cfg(target_os = "android")]
fn jni_shizuku_bool_path(method: &str, path: &str) -> Result<bool, String> {
    use jni::objects::{JObject, JValue};
    with_main_class(|env, jcls| {
        let path_j = env.new_string(path).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                method,
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&JObject::from(path_j))],
            )
            .map_err(|e| format!("JNI {method}: {e}"))?;
        result.z().map_err(|e| format!("JNI bool: {e}"))
    })
}

#[cfg(target_os = "android")]
fn jni_shizuku_mkdirs(path: &str) -> Result<(), String> {
    match jni_shizuku_bool_path("shizukuMkdirs", path) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("shizukuMkdirs({path}) failed")),
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "android")]
fn jni_shizuku_path_exists(path: &str) -> bool {
    jni_shizuku_bool_path("shizukuPathExists", path).unwrap_or(false)
}

#[cfg(target_os = "android")]
fn jni_shizuku_delete_file(path: &str) -> Result<(), String> {
    match jni_shizuku_bool_path("shizukuDeleteFile", path) {
        Ok(_) => Ok(()), // rm -f always succeeds (ignores not-found)
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "android")]
fn jni_shizuku_rmdir(path: &str) {
    // Best-effort — ignore errors (rmdir fails silently on non-empty dirs)
    let _ = jni_shizuku_bool_path("shizukuRmdir", path);
}

// ── Filesystem abstraction (direct fs on API ≤ 33, Shizuku on API ≥ 34) ──────

fn native_read_file(path: &str) -> Result<String, String> {
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && path.contains("/Android/data/") {
        ensure_shizuku_for_blocked_path(path)?;
        return jni_shizuku_read_file(path);
    }
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn native_path_exists(path: &str) -> bool {
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && path.contains("/Android/data/") {
        if let Err(e) = ensure_shizuku_for_blocked_path(path) {
            log::warn!("[native] path_exists({path}) — Shizuku not ready: {e}");
            return false;
        }
        return jni_shizuku_path_exists(path);
    }
    std::path::Path::new(path).exists()
}

fn native_mkdirs(path: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && path.contains("/Android/data/") {
        ensure_shizuku_for_blocked_path(path)?;
        return jni_shizuku_mkdirs(path);
    }
    std::fs::create_dir_all(path).map_err(|e| format!("{path}: {e}"))
}

fn native_list_dir_names(path: &str) -> Result<Vec<String>, String> {
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && path.contains("/Android/data/") {
        ensure_shizuku_for_blocked_path(path)?;
        let out = jni_shizuku_list_dir(path)?;
        return Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect());
    }
    let entries = std::fs::read_dir(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect())
}

/// Write `content` to `path`, creating parent directories automatically.
fn native_write_file(path: &str, content: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "android")]
    if get_api_level() >= 34 && path.contains("/Android/data/") {
        ensure_shizuku_for_blocked_path(path)?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            jni_shizuku_mkdirs(&parent.to_string_lossy())?;
        }
        return jni_shizuku_write_file(path, &String::from_utf8_lossy(content));
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| format!("{path}: {e}"))
}

// ── Shizuku Tauri commands ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShizukuStatus {
    pub api_level: i32,
    pub needs_shizuku: bool,
    pub available: bool,
    pub granted: bool,
}

/// Return current Shizuku readiness state. Safe to call on any platform/API level.
#[tauri::command]
pub fn check_shizuku_status() -> ShizukuStatus {
    #[cfg(target_os = "android")]
    {
        let api_level = get_api_level();
        let needs = api_level >= 34;
        let available = if needs { jni_is_shizuku_available() } else { false };
        let granted = if available { jni_is_shizuku_granted() } else { false };
        return ShizukuStatus { api_level, needs_shizuku: needs, available, granted };
    }
    #[cfg(not(target_os = "android"))]
    ShizukuStatus { api_level: 0, needs_shizuku: false, available: false, granted: false }
}

/// Show the Shizuku permission dialog. No-op on non-Android or API < 34.
#[tauri::command]
pub fn request_shizuku_permission() -> Result<(), String> {
    #[cfg(target_os = "android")]
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "requestShizukuPermission", "()V", &[])
            .map_err(|e| format!("JNI requestShizukuPermission: {e}"))?;
        Ok(())
    })?;
    Ok(())
}

/// Call `MainActivity.launchIntent(uri)` via JNI so that `startActivity()` runs
/// from the app's own Activity context — bypassing the permission restrictions
/// that block `am start -d URI` when called from a forked subprocess.
#[cfg(target_os = "android")]
fn launch_uri_from_activity(uri: &str) -> Result<(), String> {
    use jni::objects::{JObject, JValue};
    with_main_class(|env, jcls| {
        let uri_j = env.new_string(uri).map_err(|e| format!("JNI string: {e}"))?;
        env.call_static_method(
            jcls,
            "launchIntent",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&JObject::from(uri_j))],
        )
        .map_err(|e| format!("JNI launchIntent: {e}"))?;
        Ok(())
    })
}

/// Bring our app to the foreground, pushing Eden to background.
#[cfg(target_os = "android")]
fn return_to_foreground() -> Result<(), String> {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "returnToApp", "()V", &[])
            .map_err(|e| format!("JNI returnToApp: {e}"))?;
        Ok(())
    })
}


/// Start ScanForegroundService — puts our app into foreground-service state so that
/// Android 12+ background activity launch restrictions don't apply when we call
/// returnToApp() later.
#[cfg(target_os = "android")]
fn start_scan_service() -> Result<(), String> {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "startScanService", "()V", &[])
            .map_err(|e| format!("JNI startScanService: {e}"))?;
        Ok(())
    })
}

/// Stop ScanForegroundService once our app is back in the foreground.
#[cfg(target_os = "android")]
fn stop_scan_service() -> Result<(), String> {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "stopScanService", "()V", &[])
            .map_err(|e| format!("JNI stopScanService: {e}"))?;
        Ok(())
    })
}

#[cfg(not(target_os = "android"))]
fn launch_uri_from_activity(_uri: &str) -> Result<(), String> {
    Err("launch_uri_from_activity is Android-only".to_string())
}
#[cfg(not(target_os = "android"))]
fn return_to_foreground() -> Result<(), String> { Ok(()) }
#[cfg(not(target_os = "android"))]
fn start_scan_service() -> Result<(), String> { Ok(()) }
#[cfg(not(target_os = "android"))]
fn stop_scan_service() -> Result<(), String> { Ok(()) }

/// Recursively search `dir` up to `depth` levels for an NSP/XCI file whose
/// name contains `[{tid_lower}]` or fuzzy-matches `name_norm`. Returns the physical path on success.
fn search_dir_for_rom(dir: &str, tid_lower: &str, name_norm: &str, depth: u32) -> Option<String> {
    let needle = format!("[{}]", tid_lower);
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && depth > 0 {
            if let Some(found) = search_dir_for_rom(&path.to_string_lossy(), tid_lower, name_norm, depth - 1) {
                return Some(found);
            }
        } else if path.is_file() {
            let fname = path.file_name()?.to_string_lossy().to_lowercase();
            let ext = path.extension()?.to_string_lossy().to_lowercase();
            if ext != "nsp" && ext != "xci" { continue; }
            let by_tid = fname.contains(&needle);
            let by_name = !name_norm.is_empty()
                && crate::rom_cache::normalize(&fname).contains(name_norm)
                && !crate::build_ids::is_non_base_filename(&fname);
            if by_tid || by_name {
                if by_name && !by_tid {
                    log::info!("[build_ids::native] fuzzy match '{fname}' for name_norm='{name_norm}'");
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
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_permission_status_granted_serializes_camel_case() {
        let status = StoragePermissionStatus { granted: true, message: "ok".into() };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"granted\":true"), "got: {json}");
        assert!(json.contains("\"message\":"), "got: {json}");
        // serde rename_all camelCase — both fields already camelCase so no rename needed,
        // but verify no snake_case keys leak out.
        assert!(!json.contains("_"), "unexpected snake_case in: {json}");
    }

    #[test]
    fn storage_permission_status_denied_serializes() {
        let status = StoragePermissionStatus { granted: false, message: "denied".into() };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"granted\":false"), "got: {json}");
        assert!(json.contains("\"denied\""), "got: {json}");
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn check_storage_permission_always_granted_on_desktop() {
        let status = check_storage_permission();
        assert!(status.granted, "desktop build must always report granted");
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn open_storage_settings_is_noop_on_desktop() {
        // On non-Android builds this should succeed (nothing to open).
        let result = open_storage_settings();
        assert!(result.is_ok(), "desktop open_storage_settings returned: {result:?}");
    }
}
