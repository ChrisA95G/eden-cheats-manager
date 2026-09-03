use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledCheat {
    pub cheat_name: String,
    pub build_id: String,
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
