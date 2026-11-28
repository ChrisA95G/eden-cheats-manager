#[cfg(target_os = "android")]
use super::jni::{
    jni_noarg_string_call, jni_saf_delete_file, jni_saf_list_directory, jni_saf_write_text_file,
    parse_saf_response, select_cheat_install_folder_from_activity,
};
use crate::cheats::InstalledCheat;
#[cfg(target_os = "android")]
use crate::db;
#[cfg(target_os = "android")]
use crate::games::EdenPresenceRecord;
use crate::games::GameGroup;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use std::collections::HashSet;
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::Manager;

// ── Games ─────────────────────────────────────────────────────────────────────

/// Scan title-ID directories exposed by Eden's selected SAF `load` directory.
#[cfg(target_os = "android")]
pub(crate) fn scan_eden_games_android_native_with_presence(
    app: &AppHandle,
) -> Result<(Vec<GameGroup>, Vec<EdenPresenceRecord>), String> {
    let entries = jni_saf_list_directory("", None)?;
    let installed_ids: HashSet<String> = entries
        .into_iter()
        .filter(|entry| entry.directory && crate::games::is_valid_tid(&entry.name))
        .map(|entry| entry.name)
        .collect();
    log::info!(
        "[games::native] {} valid installed IDs via SAF",
        installed_ids.len()
    );

    let state = app.state::<db::DbState>();
    let mut seen_prefixes: HashSet<String> = HashSet::new();
    for tid in &installed_ids {
        if tid.len() >= 12 {
            seen_prefixes.insert(tid[..12].to_string());
        }
    }

    let mut all_rows = Vec::new();
    for prefix in &seen_prefixes {
        match db::query_base_prefix(&state, prefix) {
            Ok(rows) => all_rows.extend(rows),
            Err(e) => log::warn!("[games::native] prefix {} query error: {}", prefix, e),
        }
    }

    let (groups, presence) = crate::games::build_groups_with_presence(all_rows, &installed_ids);
    log::info!("[games::native] {} groups built", groups.len());
    crate::games::save_game_cache(app, "android", &groups);
    Ok((groups, presence))
}

#[tauri::command]
pub async fn scan_eden_games_android_native(app: AppHandle) -> Result<Vec<GameGroup>, String> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        return Err("Native Eden scanning is only available on Android.".into());
    }

    #[cfg(target_os = "android")]
    {
        scan_eden_games_android_native_with_presence(&app).map(|(groups, _presence)| groups)
    }
}

// ── Cheats ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn install_cheat_android_native(
    title_id: String,
    cheat_name: String,
    build_id: String,
    content: String,
    destination_id: String,
    replace: bool,
) -> Result<(), String> {
    crate::cheats::validate_cheat_target(&title_id, &cheat_name, &build_id)?;
    log::info!("[cheats::native] install title={title_id} build={build_id} name={cheat_name}");
    let relative_path = format!("{title_id}/{cheat_name}/cheats/{build_id}.txt");

    #[cfg(target_os = "android")]
    {
        jni_saf_write_text_file(&relative_path, &content, &destination_id, replace)?;
        log::info!("[cheats::native] install OK via SAF: {relative_path}");
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (relative_path, content, destination_id, replace);
        Err("Native cheat installation is only available on Android.".into())
    }
}

#[tauri::command]
pub fn list_installed_cheats_android_native(
    title_id: String,
    destination_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    crate::cheats::validate_hex_id(&title_id)?;
    log::debug!("[cheats::native] list title={title_id}");

    #[cfg(target_os = "android")]
    {
        let mut result = Vec::new();
        for cheat in jni_saf_list_directory(&title_id, Some(&destination_id))?
            .into_iter()
            .filter(|entry| entry.directory)
        {
            let cheats_path = format!("{title_id}/{}/cheats", cheat.name);
            for file in jni_saf_list_directory(&cheats_path, Some(&destination_id))? {
                if !file.directory && file.name.ends_with(".txt") {
                    let build_id = file.name.strip_suffix(".txt").unwrap().to_string();
                    if crate::cheats::validate_cheat_target(&title_id, &cheat.name, &build_id)
                        .is_err()
                    {
                        continue;
                    }
                    result.push(InstalledCheat {
                        cheat_name: cheat.name.clone(),
                        build_id,
                    });
                }
            }
        }
        log::info!("[cheats::native] list via SAF -> {} entries", result.len());
        return Ok(result);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (title_id, destination_id);
        Err("Native cheat listing is only available on Android.".into())
    }
}

#[tauri::command]
pub fn delete_cheat_android_native(
    title_id: String,
    cheat_name: String,
    build_id: String,
    destination_id: String,
) -> Result<(), String> {
    crate::cheats::validate_cheat_target(&title_id, &cheat_name, &build_id)?;
    log::info!("[cheats::native] delete title={title_id} build={build_id} name={cheat_name}");
    let cheats_path = format!("{title_id}/{cheat_name}/cheats");
    let relative_path = format!("{cheats_path}/{build_id}.txt");

    #[cfg(target_os = "android")]
    {
        jni_saf_delete_file(&relative_path, &destination_id)?;
        log::info!("[cheats::native] delete OK via SAF");
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (relative_path, destination_id);
        Err("Native cheat deletion is only available on Android.".into())
    }
}

// ── SAF setup commands ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenLoadAccessStatus {
    pub selected: bool,
    pub valid_location: bool,
    pub read_permission: bool,
    pub write_permission: bool,
    pub readable: bool,
    pub writable: bool,
    pub ready: bool,
    pub message: String,
    /// `"direct"` (an emulator's own `load`, where the emulator creates each
    /// title-ID folder) or `"custom"` (files prepared for import into an emulator).
    #[serde(default)]
    pub install_mode: String,
    /// Identifies the resolved load directory across equivalent Android tree grants.
    #[serde(default)]
    pub destination_id: String,
    /// Display label for the selected destination.
    #[serde(default)]
    pub folder_label: String,
    /// Why the last pick failed; the other fields still describe the active folder.
    #[serde(default)]
    pub selection_error: String,
    /// The picker is open or its selection is being saved.
    #[serde(default)]
    pub selection_pending: bool,
}

/// Return the active cheat install folder and its provider capabilities.
#[tauri::command]
pub fn get_eden_load_access_status() -> Result<EdenLoadAccessStatus, String> {
    #[cfg(target_os = "android")]
    {
        let response = parse_saf_response(jni_noarg_string_call("getEdenLoadAccessStatus")?)?;
        return serde_json::from_str(&response)
            .map_err(|e| format!("Invalid Eden SAF status response: {e}"));
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Eden SAF status is only available on Android.".into())
    }
}

/// Open Android's folder picker for the cheat install folder. It only launches
/// the picker; MainActivity saves the result and `get_eden_load_access_status`
/// reports it.
#[tauri::command]
pub fn select_cheat_install_folder() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return select_cheat_install_folder_from_activity();
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("The Android folder picker is unavailable on this platform.".into())
    }
}

/// Exercise the production SAF bridge with nested create, write, list, delete,
/// and empty-directory cleanup operations.
#[tauri::command]
pub fn test_eden_load_directory(destination_id: String) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let probe_root = format!(".ecm-saf-probe-{timestamp}");
        let probe_directory = format!("{probe_root}/nested");
        let probe_file = format!("{probe_directory}/probe.txt");

        jni_saf_write_text_file(&probe_file, "ECM SAF probe", &destination_id, false)?;
        let listed = jni_saf_list_directory(&probe_directory, Some(&destination_id));
        jni_saf_delete_file(&probe_file, &destination_id)?;
        if !listed?
            .iter()
            .any(|entry| !entry.directory && entry.name == "probe.txt")
        {
            return Err("Probe file was not listed through SAF.".into());
        }
        Ok("OK".into())
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = destination_id;
        Err("Eden SAF access testing is only available on Android.".into())
    }
}
