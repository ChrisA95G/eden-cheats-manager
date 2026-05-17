/// Native Android filesystem access — used when the app runs directly on an
/// Android device instead of talking to one over ADB from a desktop.
///
/// The load directory and log path are fixed: Eden always stores its data at
/// these paths on Android.
use crate::adb::parse_build_ids;
use crate::cheats::InstalledCheat;
use crate::db;
use crate::games::GameGroup;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

const EDEN_PKG: &str = "dev.eden.eden_emulator";
const EDEN_MAIN_ACTIVITY: &str = "org.yuzu.yuzu_emu.ui.main.MainActivity";
const AM: &str = "/system/bin/am";

static LOADER_RE: OnceLock<Regex> = OnceLock::new();
fn loader_re() -> &'static Regex {
    LOADER_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64}),\s*name=main").unwrap()
    })
}

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

/// Launch Eden with the given game, poll the log for a new build ID, then
/// force-stop Eden and return to this app. Mirrors `scan_build_id_android`
/// (the ADB version) but runs entirely on-device with no ADB dependency.
#[tauri::command]
pub async fn scan_build_id_android_native(title_id: String) -> Result<String, String> {
    const SCAN_TIMEOUT: u64 = 90;
    const POLL_INTERVAL_MS: u64 = 2000;
    const KEYS_READY_TIMEOUT: u64 = 25;

    log::info!("[build_ids::native] scan_build_id title={title_id}");

    // 1. Find the ROM's URI (content:// for SD-card games, file:// for internal).
    let rom_uri = find_rom_path_native(&title_id).ok_or_else(|| {
        format!(
            "ROM not found for {title_id}. \
             Make sure it is in a directory configured in Eden's settings."
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

                let Ok(text) = std::fs::read_to_string(ANDROID_LOG_PATH) else {
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
                    if let Some(cap) = loader_re().captures(line) {
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
                    if let Some(bid) = crate::build_ids::find_build_ids_for_title_pub(&chunk, &title_id_poll).into_iter().next() {
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

    // 8. Return our app to the foreground (Eden goes background), then kill Eden.
    //    `am force-stop` requires FORCE_STOP_PACKAGES (system permission) — unavailable
    //    to regular apps.  Instead: bring ourselves to front via JNI startActivity(),
    //    then retry killBackgroundProcesses() with fixed delays.
    //    Android only moves Eden to CACHED oom_adj (killable) after the foreground
    //    transition + oom_adj recalculation — this can take 1-3 s.
    //    NOTE: /proc/<pid>/cmdline is blocked by SELinux for other-app processes on
    //    Android 10+, so process-existence checks always return false; we use fixed
    //    retries instead.
    // Post the full-screen intent notification and set the onResume kill flag.
    // Eden will be killed in MainActivity.onResume() — guaranteed to fire after
    // the user taps the notification and our activity comes to the foreground,
    // at which point Eden is definitively background and killBackgroundProcesses works.
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
fn find_rom_path_native(title_id: &str) -> Option<String> {
    let config = std::fs::read_to_string(ANDROID_CONFIG_PATH).ok()?;
    let tid_lower = title_id.to_lowercase();

    let mut tree_entries: Vec<(String, String)> = Vec::new();
    let mut search_dirs: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for line in config.lines() {
        if !line.contains("\\path=") {
            continue;
        }
        let raw_uri = line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"');
        if let Some(physical) = crate::games::content_uri_to_physical_pub(raw_uri) {
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
        if let Some(physical) = search_dir_for_rom(dir, &tid_lower, 2) {
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
/// name contains `[{tid_lower}]`. Returns the physical path on success.
fn search_dir_for_rom(dir: &str, tid_lower: &str, depth: u32) -> Option<String> {
    let needle = format!("[{}]", tid_lower);
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && depth > 0 {
            if let Some(found) = search_dir_for_rom(&path.to_string_lossy(), tid_lower, depth - 1) {
                return Some(found);
            }
        } else if path.is_file() {
            let fname = path.file_name()?.to_string_lossy().to_lowercase();
            if fname.contains(&needle) {
                let ext = path.extension()?.to_string_lossy().to_lowercase();
                if ext == "nsp" || ext == "xci" {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Count lines in a local file. Used to establish a log baseline.
fn get_local_line_count(path: &str) -> Option<u64> {
    std::fs::read_to_string(path)
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
