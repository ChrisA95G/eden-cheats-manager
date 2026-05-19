use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const CACHE_FILENAME: &str = "rom_path_cache.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RomCacheEntry {
    pub path: String,
    pub manual: bool,
}

pub type RomCache = HashMap<String, RomCacheEntry>;

fn cache_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(CACHE_FILENAME)
}

pub fn load_cache(app: &AppHandle) -> RomCache {
    let path = cache_path(app);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_cache(app: &AppHandle, cache: &RomCache) {
    let path = cache_path(app);
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(&path, json);
    }
}

/// Normalize a string for fuzzy ROM-filename matching:
/// strips ™®©, lowercases, replaces non-alphanumeric with space, collapses runs.
pub fn normalize(s: &str) -> String {
    let stripped: String = s.chars().filter(|c| !matches!(c, '™' | '®' | '©')).collect();
    let spaced: String = stripped
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    spaced
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns true if the ROM filename matches the given title ID or game name.
/// Priority: exact `[title_id]` bracket > fuzzy normalized name.
fn rom_matches(fname_lower: &str, title_id_lower: &str, name_norm: &str) -> bool {
    if fname_lower.contains(&format!("[{}]", title_id_lower)) {
        return true;
    }
    if !name_norm.is_empty() {
        let fname_norm = normalize(fname_lower);
        if fname_norm.contains(name_norm) {
            return true;
        }
    }
    false
}

fn scan_dir(
    dir: &PathBuf,
    targets: &[(String, String, String)], // (title_id, title_id_lower, name_norm)
    cache: &mut RomCache,
    found: &mut usize,
    depth: u32,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("[rom_cache] read_dir({}) failed: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let fname = path
                .file_name()
                .map(|f| f.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !fname.ends_with(".nsp") && !fname.ends_with(".xci") {
                continue;
            }
            for (tid, tid_lower, name_norm) in targets {
                if cache.get(tid).map(|e| e.manual).unwrap_or(false) {
                    continue;
                }
                if rom_matches(&fname, tid_lower, name_norm) {
                    let path_str = path.to_string_lossy().to_string();
                    log::info!("[rom_cache] matched {tid} -> {path_str}");
                    cache.insert(
                        tid.clone(),
                        RomCacheEntry {
                            path: path_str,
                            manual: false,
                        },
                    );
                    *found += 1;
                    break;
                }
            }
        } else if path.is_dir() && depth > 0 {
            scan_dir(&path, targets, cache, found, depth - 1);
        }
    }
}

/// Scan Eden's configured game directories and match ROM files to the provided
/// `(title_id, game_name)` pairs. Manual cache entries are never overwritten.
/// Returns the number of newly matched ROMs.
pub fn scan_rom_paths(
    config_path: &PathBuf,
    title_names: &[(String, String)],
    cache: &mut RomCache,
) -> usize {
    let config = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[rom_cache] cannot read config: {e}");
            return 0;
        }
    };

    const VIRTUAL: &[&str] = &["SDMC", "UserNAND", "SysNAND"];
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in config.lines() {
        if !line.contains("gamedirs") || !line.contains("\\path=") {
            continue;
        }
        let raw = line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"');
        if raw.is_empty() || VIRTUAL.contains(&raw) {
            continue;
        }
        let p = PathBuf::from(raw);
        if let Some(parent) = p.parent() {
            let key = parent.to_string_lossy().to_string();
            if seen.insert(key) {
                search_dirs.push(parent.to_path_buf());
            }
        }
        let key = p.to_string_lossy().to_string();
        if seen.insert(key) {
            search_dirs.push(p);
        }
    }

    log::info!("[rom_cache] scanning {} dirs for {} titles", search_dirs.len(), title_names.len());

    let targets: Vec<(String, String, String)> = title_names
        .iter()
        .map(|(tid, name)| (tid.clone(), tid.to_lowercase(), normalize(name)))
        .collect();

    let mut found = 0usize;
    for dir in &search_dirs {
        scan_dir(dir, &targets, cache, &mut found, 2);
    }
    log::info!("[rom_cache] scan complete: {found} new matches");
    found
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_rom_cache(app: AppHandle) -> RomCache {
    load_cache(&app)
}

#[tauri::command]
pub fn set_rom_path_manual(app: AppHandle, title_id: String, path: String) -> Result<(), String> {
    let mut cache = load_cache(&app);
    cache.insert(title_id.clone(), RomCacheEntry { path: path.clone(), manual: true });
    save_cache(&app, &cache);
    log::info!("[rom_cache] manual path set for {title_id}: {path}");
    Ok(())
}

#[tauri::command]
pub async fn scan_and_update_rom_cache(
    app: AppHandle,
    title_names: Vec<(String, String)>,
) -> Result<usize, String> {
    let config_path = crate::build_ids::get_eden_config_path_pub()
        .ok_or_else(|| "Eden config not found".to_string())?;
    let mut cache = load_cache(&app);
    let count = scan_rom_paths(&config_path, &title_names, &mut cache);
    save_cache(&app, &cache);
    Ok(count)
}
