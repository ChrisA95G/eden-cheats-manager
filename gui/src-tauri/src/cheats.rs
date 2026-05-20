use crate::adb::{adb_bin, adb_ls, adb_mkdir, adb_push_internal, REMOTE_BASE};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledCheat {
    pub cheat_name: String,
    pub build_id: String,
}

// ── Android ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn install_cheat_android(
    adb_path: String,
    title_id: String,
    cheat_name: String,
    build_id: String,
    content: String,
) -> Result<(), String> {
    log::info!("[cheats] install_android title={title_id} build={build_id} name={cheat_name}");
    let adb = adb_bin(&adb_path);

    let remote_dir = format!("{}/{}/{}/cheats", REMOTE_BASE, title_id, cheat_name);
    adb_mkdir(&adb, &remote_dir)?;

    // Write content to a temp file then push
    let tmp = std::env::temp_dir().join(format!(
        "{}_{}.txt",
        build_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&tmp, content.as_bytes()).map_err(|e| e.to_string())?;

    let remote_file = format!("{}/{}.txt", remote_dir, build_id);
    adb_push_internal(&adb, tmp.to_str().unwrap(), &remote_file)?;
    let _ = std::fs::remove_file(&tmp);
    log::info!("[cheats] install_android OK: {cheat_name}/{build_id}");
    Ok(())
}

#[tauri::command]
pub fn list_installed_cheats_android(
    adb_path: String,
    title_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    log::debug!("[cheats] list_installed_android title={title_id}");
    let title_dir = format!("{}/{}", REMOTE_BASE, title_id);
    let cheat_names = adb_ls(adb_path.clone(), title_dir.clone())?;
    let mut result = Vec::new();
    for name in &cheat_names {
        log::debug!("[cheats] scanning android cheat dir: {name}");
        let cheats_dir = format!("{}/{}/cheats", title_dir, name);
        match adb_ls(adb_path.clone(), cheats_dir) {
            Ok(files) => {
                for file in files {
                    if file.ends_with(".txt") {
                        let build_id = file.trim_end_matches(".txt").to_uppercase();
                        log::debug!("[cheats] found android installed: {name}/{build_id}");
                        result.push(InstalledCheat {
                            cheat_name: name.clone(),
                            build_id,
                        });
                    }
                }
            }
            Err(e) => log::warn!("[cheats] could not ls cheats dir for {name}: {e}"),
        }
    }
    log::info!("[cheats] list_installed_android -> {} entries", result.len());
    Ok(result)
}

#[tauri::command]
pub fn delete_cheat_android(
    adb_path: String,
    title_id: String,
    cheat_name: String,
    build_id: String,
) -> Result<(), String> {
    log::info!("[cheats] delete_android title={title_id} build={build_id} name={cheat_name}");
    let adb = adb_bin(&adb_path);
    let remote_file = format!(
        "{}/{}/{}/cheats/{}.txt",
        REMOTE_BASE, title_id, cheat_name, build_id
    );
    let out = Command::new(&adb)
        .args(["shell", &format!("rm -f '{}'", remote_file)])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        log::error!("[cheats] delete_android failed: {err}");
        return Err(err);
    }
    log::info!("[cheats] delete_android OK: {cheat_name}/{build_id}");
    Ok(())
}

// ── PC ────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn install_cheat_pc(
    load_dir: String,
    title_id: String,
    cheat_name: String,
    build_id: String,
    content: String,
) -> Result<(), String> {
    log::info!("[cheats] install_pc title={title_id} build={build_id} name={cheat_name}");
    let cheats_dir = PathBuf::from(&load_dir)
        .join(&title_id)
        .join(&cheat_name)
        .join("cheats");
    std::fs::create_dir_all(&cheats_dir).map_err(|e| e.to_string())?;
    let file = cheats_dir.join(format!("{}.txt", build_id));
    std::fs::write(&file, content.as_bytes()).map_err(|e| e.to_string())?;
    log::info!("[cheats] install_pc OK: {}", file.display());
    Ok(())
}

#[tauri::command]
pub fn list_installed_cheats_pc(
    load_dir: String,
    title_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    log::debug!("[cheats] list_installed_pc title={title_id} load_dir={load_dir}");
    let title_dir = PathBuf::from(&load_dir).join(&title_id);
    if !title_dir.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(&title_dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let cheat_name = entry.file_name().to_string_lossy().to_string();
        let cheats_dir = entry.path().join("cheats");
        if cheats_dir.is_dir() {
            for f in std::fs::read_dir(&cheats_dir)
                .map_err(|e| e.to_string())?
                .flatten()
            {
                let fname = f.file_name().to_string_lossy().to_string();
                if fname.ends_with(".txt") {
                    let build_id = fname.trim_end_matches(".txt").to_uppercase();
                    log::debug!("[cheats] found pc installed: {cheat_name}/{build_id}");
                    result.push(InstalledCheat {
                        cheat_name: cheat_name.clone(),
                        build_id,
                    });
                }
            }
        }
    }
    log::info!("[cheats] list_installed_pc -> {} entries", result.len());
    Ok(result)
}

#[tauri::command]
pub fn delete_cheat_pc(
    load_dir: String,
    title_id: String,
    cheat_name: String,
    build_id: String,
) -> Result<(), String> {
    log::info!("[cheats] delete_pc title={title_id} build={build_id} name={cheat_name}");
    let file = PathBuf::from(&load_dir)
        .join(&title_id)
        .join(&cheat_name)
        .join("cheats")
        .join(format!("{}.txt", build_id));
    match std::fs::remove_file(&file) {
        Ok(()) => {
            log::info!("[cheats] delete_pc OK: {}", file.display());
            let cheats_dir = file.parent().unwrap();
            let _ = std::fs::remove_dir(cheats_dir);
            let _ = std::fs::remove_dir(cheats_dir.parent().unwrap());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}
