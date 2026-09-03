#[cfg(target_os = "android")]
use super::jni::{
    jni_noarg_string_call, jni_saf_delete_file, jni_saf_list_directory,
    jni_saf_remove_empty_directory, jni_saf_write_text_file, parse_saf_response,
    select_eden_load_directory_from_activity,
};
use crate::cheats::InstalledCheat;
#[cfg(target_os = "android")]
use crate::db;
use crate::games::GameGroup;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use std::collections::HashSet;
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::Manager;

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

        let (groups, _presence) =
            crate::games::build_groups_with_presence(all_rows, &installed_ids);
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
}

/// Return the persisted Eden `load` grant and provider capability state.
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

/// Open Androids document-tree picker so the user can grant access to Edens
/// exposed `load` directory. The picker result is persisted by MainActivity.
#[tauri::command]
pub fn select_eden_load_directory() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return select_eden_load_directory_from_activity();
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
            if !entries
                .iter()
                .any(|entry| !entry.directory && entry.name == "probe.txt")
            {
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
