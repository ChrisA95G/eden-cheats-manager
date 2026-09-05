use crate::adb::loader_build_id_re;
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

// Looser regex used for contextual log window: no `name=main` requirement.
static LOG_BUILD_ID_RE: OnceLock<Regex> = OnceLock::new();
fn log_build_id_re() -> &'static Regex {
    LOG_BUILD_ID_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64})").unwrap()
    })
}

// ── Log helpers ───────────────────────────────────────────────────────────────

/// Search `text` for build IDs that appear near (within a 35-line window of)
/// a line containing `title_id`.  Returns deduplicated, 16-char uppercase IDs.
///
/// Two-pass strategy: NSO-loader lines (`build_id=…, name=main`) are collected
/// first so they sort before library-scan cache entries in the output.  This
/// ensures callers see the real emulation build ID at index 0 even when the log
/// contains both kinds of entry for the same title.
pub(crate) fn find_build_ids_for_title_in_log(text: &str, title_id: &str) -> Vec<String> {
    let tid_lower = title_id.to_lowercase();
    let lines: Vec<&str> = text.lines().collect();
    let loader_re = loader_build_id_re();
    let loose_re = log_build_id_re();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    // Collect all (start, end) windows around every line that mentions title_id.
    let windows: Vec<(usize, usize)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.to_lowercase().contains(&tid_lower))
        .map(|(i, _)| (i.saturating_sub(5), (i + 30).min(lines.len())))
        .collect();

    // Pass 1 — strict: only lines that contain `, name=main` (NSO loader entries).
    for &(start, end) in &windows {
        for window_line in &lines[start..end] {
            for cap in loader_re.captures_iter(window_line) {
                let full = &cap[1];
                let bid = full[..16.min(full.len())].to_uppercase();
                if seen.insert(bid.clone()) {
                    out.push(bid);
                }
            }
        }
    }

    // Pass 2 — loose: any build_id= line (catches library-scan cache entries).
    for &(start, end) in &windows {
        for window_line in &lines[start..end] {
            for cap in loose_re.captures_iter(window_line) {
                let full = &cap[1];
                let bid = full[..16.min(full.len())].to_uppercase();
                if seen.insert(bid.clone()) {
                    out.push(bid);
                }
            }
        }
    }

    out
}

// ── PC ────────────────────────────────────────────────────────────────────────

/// Detect build IDs for a specific title on PC.
///
/// Strategy 1 — Installed cheat files (instant, offline):
///   Scans `{load_dir}/{title_id}/*/cheats/*.txt`; the filename stem *is*
///   the build ID.
///
/// Strategy 2 — Eden log (per-title contextual search):
///   Reads Eden log files adjacent to `load_dir` and extracts build IDs from
///   context windows around lines containing `title_id`.
///
/// // TODO Strategy 3 — NSO binary header parsing (future).
#[tauri::command]
pub fn detect_build_ids_pc(
    load_dir: String,
    title_id: String,
) -> Result<Vec<String>, String> {
    log::info!("[build_ids] detect_pc title={title_id} load_dir={load_dir}");
    let mut seen: HashSet<String> = HashSet::new();
    let mut ids: Vec<String> = Vec::new();

    // ── Strategy 1: installed cheat filenames ─────────────────────────────
    let title_dir = PathBuf::from(&load_dir).join(&title_id);
    if title_dir.is_dir() {
        for cheat_entry in std::fs::read_dir(&title_dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let cheats_dir = cheat_entry.path().join("cheats");
            if !cheats_dir.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(&cheats_dir)
                .map_err(|e| e.to_string())?
                .flatten()
            {
                let fname = f.file_name().to_string_lossy().to_string();
                if fname.ends_with(".txt") {
                    let bid = fname.trim_end_matches(".txt").to_uppercase();
                    if bid.len() == 16 && seen.insert(bid.clone()) {
                        log::debug!("[build_ids] pc installed-cheat strategy: {bid}");
                        ids.push(bid);
                    }
                }
            }
        }
    }
    log::debug!("[build_ids] pc after strategy 1: {} ids", ids.len());

    // ── Strategy 2: Eden log (contextual window around title_id) ──────────
    let load_path = PathBuf::from(&load_dir);
    let base = load_path.parent().unwrap_or(&load_path);
    let log_candidates = [
        base.join("log/eden_log.txt"),
        base.join("log/eden_log.txt.old.txt"),
        base.join("eden_log.txt"),
        // Also check one level higher in case load_dir == .../load/
        load_path.join("../log/eden_log.txt"),
        load_path.join("../log/eden_log.txt.old.txt"),
    ];

    for log_path in &log_candidates {
        if let Ok(text) = std::fs::read_to_string(log_path) {
            for bid in find_build_ids_for_title_in_log(&text, &title_id) {
                if bid.len() == 16 && seen.insert(bid.clone()) {
                    log::debug!(
                        "[build_ids] pc log strategy ({}): {bid}",
                        log_path.display()
                    );
                    ids.push(bid);
                }
            }
        }
    }

    ids.sort();
    log::info!("[build_ids] detect_pc title={title_id} -> {:?}", ids);
    Ok(ids)
}

// ── PC helpers ────────────────────────────────────────────────────────────────

/// Platform-specific path to Eden's Qt config file.
///
/// Returns the first candidate that exists on disk, logging every path tried.
/// Multiple candidates per platform handle Qt version differences and installs
/// that haven't been verified on real hardware yet.
pub(crate) fn get_eden_config_path() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(target_os = "linux") {
        let home = dirs_next::home_dir().unwrap_or_default();
        vec![
            home.join(".config/eden/qt-config.ini"),           // verified
            home.join(".local/share/eden/config/config.ini"),  // fallback (older builds?)
        ]
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        vec![
            PathBuf::from(&appdata).join("eden\\qt-config.ini"),
            PathBuf::from(&local).join("eden\\qt-config.ini"),
            PathBuf::from(&appdata).join("eden\\config\\qt-config.ini"),
        ]
    } else if cfg!(target_os = "macos") {
        let home = dirs_next::home_dir().unwrap_or_default();
        vec![
            // Qt on macOS with IniFormat may use XDG-style ~/.config or Apple-style Library
            home.join("Library/Application Support/eden/qt-config.ini"),
            home.join(".config/eden/qt-config.ini"),
        ]
    } else {
        vec![]
    };

    for p in &candidates {
        let exists = p.exists();
        log::debug!("[build_ids] get_eden_config_path candidate={} exists={exists}", p.display());
        if exists {
            log::info!("[build_ids] get_eden_config_path -> {}", p.display());
            return Some(p.clone());
        }
    }
    log::warn!("[build_ids] get_eden_config_path: no config found (tried {} candidates)", candidates.len());
    None
}
