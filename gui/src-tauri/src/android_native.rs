/// Native Android integration used when the app runs on the same device as Eden.
/// Eden's load directory is accessed exclusively through its SAF provider.
use crate::adb::{loader_build_id_re, parse_build_ids, EDEN_PKG};
use crate::cheats::InstalledCheat;
#[cfg(target_os = "android")]
use crate::db;
use crate::games::GameGroup;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::Command;
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::Manager;

const EDEN_MAIN_ACTIVITY: &str = "org.yuzu.yuzu_emu.ui.main.MainActivity";
const AM: &str = "/system/bin/am";

const ANDROID_LOG_PATH: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt";

// ── Games ─────────────────────────────────────────────────────────────────────

/// Scan title-ID directories exposed by Eden's selected SAF `load` directory.
#[tauri::command]
pub async fn scan_eden_games_android_native(app: AppHandle) -> Result<Vec<GameGroup>, String> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        return Err("Native Eden scanning is only available on Android.".into());
    }

    #[cfg(target_os = "android")]
    {
        let entries = jni_saf_list_directory("")?;
        let installed_ids: HashSet<String> = entries
            .into_iter()
            .filter(|entry| entry.directory && crate::games::is_valid_tid(&entry.name))
            .map(|entry| entry.name)
            .collect();
        log::info!("[games::native] {} valid installed IDs via SAF", installed_ids.len());

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
    let relative_path = format!("{title_id}/{cheat_name}/cheats/{build_id}.txt");

    #[cfg(target_os = "android")]
    {
        jni_saf_write_text_file(&relative_path, &content)?;
        log::info!("[cheats::native] install OK via SAF: {relative_path}");
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (relative_path, content);
        Err("Native cheat installation is only available on Android.".into())
    }
}

#[tauri::command]
pub fn list_installed_cheats_android_native(
    title_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    log::debug!("[cheats::native] list title={title_id}");

    #[cfg(target_os = "android")]
    {
        let mut result = Vec::new();
        for cheat in jni_saf_list_directory(&title_id)?
            .into_iter()
            .filter(|entry| entry.directory)
        {
            let cheats_path = format!("{title_id}/{}/cheats", cheat.name);
            for file in jni_saf_list_directory(&cheats_path)? {
                if !file.directory && file.name.ends_with(".txt") {
                    result.push(InstalledCheat {
                        cheat_name: cheat.name.clone(),
                        build_id: file.name.trim_end_matches(".txt").to_uppercase(),
                    });
                }
            }
        }
        log::info!("[cheats::native] list via SAF -> {} entries", result.len());
        return Ok(result);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = title_id;
        Err("Native cheat listing is only available on Android.".into())
    }
}

#[tauri::command]
pub fn delete_cheat_android_native(
    title_id: String,
    cheat_name: String,
    build_id: String,
) -> Result<(), String> {
    log::info!("[cheats::native] delete title={title_id} build={build_id} name={cheat_name}");
    let cheats_path = format!("{title_id}/{cheat_name}/cheats");
    let relative_path = format!("{cheats_path}/{build_id}.txt");

    #[cfg(target_os = "android")]
    {
        jni_saf_delete_file(&relative_path)?;
        jni_saf_remove_empty_directory(&cheats_path)?;
        jni_saf_remove_empty_directory(&format!("{title_id}/{cheat_name}"))?;
        log::info!("[cheats::native] delete OK via SAF");
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (relative_path, cheats_path);
        Err("Native cheat deletion is only available on Android.".into())
    }
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

/// Verify every static method that Rust calls on MainActivity by name still
/// exists after R8/ProGuard minification. Logs to logcat via eprintln! (the
/// Tauri logger is not yet initialised at JNI_OnLoad time). If any method is
/// missing the release APK will silently mis-behave; this makes it loud.
#[cfg(target_os = "android")]
fn probe_jni_methods(env: &mut jni::JNIEnv, cls: &jni::objects::JClass) {
    const METHODS: &[(&str, &str)] = &[
        ("selectEdenLoadDirectory",   "()V"),
        ("safListDirectory",          "(Ljava/lang/String;)Ljava/lang/String;"),
        ("safWriteTextFile",          "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
        ("safDeleteFile",             "(Ljava/lang/String;)Ljava/lang/String;"),
        ("safRemoveEmptyDirectory",   "(Ljava/lang/String;)Ljava/lang/String;"),
        ("launchIntent",              "(Ljava/lang/String;)V"),
        ("returnToApp",              "()V"),
        ("startScanService",         "()V"),
        ("stopScanService",          "()V"),
    ];
    let mut missing: Vec<&str> = Vec::new();
    for (name, sig) in METHODS {
        if env.get_static_method_id(cls, name, sig).is_err() {
            let _ = env.exception_clear();
            missing.push(name);
        }
    }
    if missing.is_empty() {
        eprintln!("[ECM] JNI probe OK — all {} MainActivity methods found", METHODS.len());
    } else {
        eprintln!(
            "[ECM] JNI PROBE FAILED — {}/{} methods missing (R8 renamed them): {:?}",
            missing.len(), METHODS.len(), missing
        );
        eprintln!("[ECM] Fix: -keepclassmembers class dev.eden.cheats_manager.MainActivity {{ public static *; }} in proguard-rules.pro");
    }
}

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
        probe_jni_methods(&mut env, &cls);
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

// ── SAF storage bridge ────────────────────────────────────────────────────────

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
struct SafEntry {
    name: String,
    directory: bool,
}

#[cfg(target_os = "android")]
fn jni_saf_path_call(method: &str, relative_path: &str) -> Result<String, String> {
    use jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let path = env.new_string(relative_path).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                method,
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&JObject::from(path))],
            )
            .map_err(|e| format!("JNI {method}: {e}"))?;
        let object = result.l().map_err(|e| format!("JNI object: {e}"))?;
        if object.is_null() {
            return Err(format!("{method} returned null"));
        }
        env.get_string(&JString::from(object))
            .map(|value| value.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
fn jni_saf_write_call(relative_path: &str, content: &str) -> Result<String, String> {
    use jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let path = env.new_string(relative_path).map_err(|e| format!("JNI string: {e}"))?;
        let content = env.new_string(content).map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                "safWriteTextFile",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&JObject::from(path)),
                    JValue::Object(&JObject::from(content)),
                ],
            )
            .map_err(|e| format!("JNI safWriteTextFile: {e}"))?;
        let object = result.l().map_err(|e| format!("JNI object: {e}"))?;
        if object.is_null() {
            return Err("safWriteTextFile returned null".into());
        }
        env.get_string(&JString::from(object))
            .map(|value| value.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
fn parse_saf_response(response: String) -> Result<String, String> {
    if let Some(error) = response.strip_prefix("ERROR:") {
        Err(error.trim().to_string())
    } else {
        Ok(response)
    }
}

#[cfg(target_os = "android")]
fn jni_saf_list_directory(relative_path: &str) -> Result<Vec<SafEntry>, String> {
    let response = parse_saf_response(jni_saf_path_call("safListDirectory", relative_path)?)?;
    serde_json::from_str(&response).map_err(|e| format!("Invalid SAF directory response: {e}"))
}

#[cfg(target_os = "android")]
fn jni_saf_write_text_file(relative_path: &str, content: &str) -> Result<(), String> {
    let response = parse_saf_response(jni_saf_write_call(relative_path, content)?)?;
    if response == "OK" { Ok(()) } else { Err(format!("Unexpected SAF response: {response}")) }
}

#[cfg(target_os = "android")]
fn jni_saf_delete_file(relative_path: &str) -> Result<(), String> {
    let response = parse_saf_response(jni_saf_path_call("safDeleteFile", relative_path)?)?;
    if response == "OK" { Ok(()) } else { Err(format!("Unexpected SAF response: {response}")) }
}

#[cfg(target_os = "android")]
fn jni_saf_remove_empty_directory(relative_path: &str) -> Result<(), String> {
    let response = parse_saf_response(jni_saf_path_call("safRemoveEmptyDirectory", relative_path)?)?;
    if response == "OK" { Ok(()) } else { Err(format!("Unexpected SAF response: {response}")) }
}

/// Direct Eden log/config access remains disabled while build-ID discovery is redesigned.
fn native_read_file(path: &str) -> Result<String, String> {
    #[cfg(target_os = "android")]
    if path.contains("/Android/data/") {
        return Err("Direct Eden data access is unavailable; build-ID discovery still needs migration.".into());
    }
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

// ── SAF setup commands ─────────────────────────────────────────────────────────

/// Open Androids document-tree picker so the user can grant access to Edens
/// exposed `load` directory. The picker result is persisted by MainActivity.
#[tauri::command]
pub fn select_eden_load_directory() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return with_main_class(|env, jcls| {
            env.call_static_method(jcls, "selectEdenLoadDirectory", "()V", &[])
                .map_err(|e| format!("JNI selectEdenLoadDirectory: {e}"))?;
            Ok(())
        });
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Eden SAF directory selection is only available on Android.".into())
    }
}

/// Exercise the production SAF bridge with nested create, write, list, delete,
/// and empty-directory cleanup operations.
#[tauri::command]
pub fn test_eden_load_directory() -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        let probe_root = format!(".ecm-saf-probe-{timestamp}");
        let probe_directory = format!("{probe_root}/nested");
        let probe_file = format!("{probe_directory}/probe.txt");

        let result = (|| {
            jni_saf_write_text_file(&probe_file, "ECM SAF probe")?;
            let entries = jni_saf_list_directory(&probe_directory)?;
            if !entries.iter().any(|entry| !entry.directory && entry.name == "probe.txt") {
                return Err("Probe file was not listed through SAF.".into());
            }
            jni_saf_delete_file(&probe_file)?;
            jni_saf_remove_empty_directory(&probe_directory)?;
            jni_saf_remove_empty_directory(&probe_root)?;
            Ok("OK".to_string())
        })();

        let _ = jni_saf_delete_file(&probe_file);
        let _ = jni_saf_remove_empty_directory(&probe_directory);
        let _ = jni_saf_remove_empty_directory(&probe_root);
        return result;
    }

    #[cfg(not(target_os = "android"))]
    {
        Err("Eden SAF access testing is only available on Android.".into())
    }
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
