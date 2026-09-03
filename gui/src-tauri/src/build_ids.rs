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

// ── ROM helpers ───────────────────────────────────────────────────────────────

/// Returns true if `fname_lower` looks like an update or DLC NSP that cannot
/// be launched by Eden as a base game.
/// Used to reject fuzzy name matches that would return the wrong file.
pub(crate) fn is_non_base_filename(fname_lower: &str) -> bool {
    if fname_lower.contains("update") || fname_lower.contains("[dlc]") {
        return true;
    }
    // Reject any [TITLEID] bracket whose id does NOT end in "000" (base games end in "000").
    let b = fname_lower.as_bytes();
    let mut i = 0;
    while i + 17 < b.len() {
        if b[i] == b'[' && b[i + 17] == b']' {
            let candidate = &fname_lower[i + 1..i + 17];
            if candidate.bytes().all(|c| c.is_ascii_hexdigit()) && !candidate.ends_with("000") {
                return true;
            }
        }
        i += 1;
    }
    false
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

/// Recursively search `dir` up to `depth` levels for an NSP/XCI whose filename
/// contains `[{title_id_lower}]`.
fn search_roms_in_dir(dir: &std::path::Path, title_id_lower: &str, name_norm: &str, depth: u32) -> Option<PathBuf> {
    if !dir.is_dir() {
        log::debug!("[build_ids] search_roms_in_dir: skip (not a dir) {}", dir.display());
        return None;
    }
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[build_ids] search_roms_in_dir: read_dir({}) failed: {e}", dir.display());
            return None;
        }
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_file() {
            let fname = path.file_name()
                .map(|f| f.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if fname.ends_with(".nsp") || fname.ends_with(".xci") {
                log::debug!("[build_ids] search_roms_in_dir: candidate {fname}");
                let by_tid = fname.contains(&format!("[{}]", title_id_lower));
                let by_name = !name_norm.is_empty()
                    && crate::rom_cache::normalize(&fname).contains(name_norm)
                    && !is_non_base_filename(&fname);
                if by_tid || by_name {
                    if by_name && !by_tid {
                        log::info!("[build_ids] search_roms_in_dir: fuzzy match '{fname}' for name_norm='{name_norm}'");
                    }
                    return Some(path);
                }
            }
        } else if path.is_dir() && depth > 0 {
            if let Some(found) = search_roms_in_dir(&path, title_id_lower, name_norm, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Search Eden's configured game directories on PC for a ROM file whose
/// filename contains `[{title_id}]`.
fn find_rom_path_pc(title_id: &str, game_name: &str) -> Option<PathBuf> {
    let config_path = get_eden_config_path()?;
    log::debug!("[build_ids] find_rom_pc: config exists={}", config_path.exists());

    let config = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[build_ids] find_rom_pc: cannot read config.ini ({}): {e}", config_path.display());
            return None;
        }
    };
    let name_norm = crate::rom_cache::normalize(game_name);

    let tid_lower = title_id.to_lowercase();
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    use crate::adb::EDEN_VIRTUAL_DIRS;
    for line in config.lines() {
        // qt-config.ini game dirs: `Paths\gamedirs\N\path=<value>`
        if !line.contains("gamedirs") || !line.contains("\\path=") {
            continue;
        }
        log::debug!("[build_ids] find_rom_pc: path line: {line}");
        let raw = line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"');
        if raw.is_empty() {
            log::debug!("[build_ids] find_rom_pc: empty raw value, skipping");
            continue;
        }
        if EDEN_VIRTUAL_DIRS.contains(&raw) {
            log::debug!("[build_ids] find_rom_pc: virtual entry '{raw}', skipping");
            continue;
        }
        let p = PathBuf::from(raw);
        log::debug!(
            "[build_ids] find_rom_pc: game dir={} exists={}",
            p.display(), p.exists()
        );

        // Add the parent dir first, then the game dir itself.
        // This lets us find ROMs that sit next to (not inside) a configured game dir.
        if let Some(parent) = p.parent() {
            let parent_key = parent.to_string_lossy().to_string();
            if seen.insert(parent_key) {
                log::debug!("[build_ids] find_rom_pc: queuing parent={}", parent.display());
                search_dirs.push(parent.to_path_buf());
            }
        }

        let key = p.to_string_lossy().to_string();
        if seen.insert(key) {
            search_dirs.push(p);
        }
    }

    log::info!(
        "[build_ids] find_rom_pc: {} search dirs for [{}]",
        search_dirs.len(), title_id
    );
    for dir in &search_dirs {
        log::debug!("[build_ids] find_rom_pc: searching {} (exists={})", dir.display(), dir.exists());
        if let Some(found) = search_roms_in_dir(dir, &tid_lower, &name_norm, 2) {
            log::info!("[build_ids] find_rom_pc: found {}", found.display());
            return Some(found);
        }
    }

    log::warn!("[build_ids] find_rom_pc: no ROM found for [{title_id}] in {} dirs", search_dirs.len());
    None
}

// ── PC: Scan Build ID (Auto-launch + Log Poll) ────────────────────────────────

/// Resolve the Eden executable: use `eden_exe_path` if set, otherwise try PATH.
fn resolve_eden_exe(eden_exe_path: &str) -> Option<String> {
    if !eden_exe_path.is_empty() {
        return Some(eden_exe_path.to_string());
    }
    #[cfg(unix)]
    {
        if let Ok(out) = std::process::Command::new("which").arg("eden").output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("where").arg("eden").output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Launch Eden with the game's ROM, poll the log file for a `build_id=HEX, name=main`
/// line, kill Eden once found (or on timeout), and return the 16-char build ID.
#[tauri::command]
pub async fn scan_build_id_pc(
    app: tauri::AppHandle,
    load_dir: String,
    title_id: String,
    base_title_id: String,
    game_name: String,
    eden_exe_path: String,
) -> Result<String, String> {
    log::info!("[build_ids] scan_build_id_pc title={title_id} base={base_title_id} load_dir={load_dir}");

    // 1. Resolve Eden executable.
    let eden_exe = resolve_eden_exe(&eden_exe_path).ok_or_else(|| {
        "Eden executable not found on PATH. Set the Eden executable path in Settings.".to_string()
    })?;
    log::info!("[build_ids] scan_build_id_pc eden_exe={eden_exe}");

    // 2. Find ROM: check cache first (by base_title_id then title_id), then scan dirs.
    let lookup_id = if !base_title_id.is_empty() { &base_title_id } else { &title_id };
    let mut cache = crate::rom_cache::load_cache(&app);
    let rom_path: PathBuf = if let Some(entry) = cache.get(lookup_id).or_else(|| cache.get(&title_id)) {
        let p = PathBuf::from(&entry.path);
        if p.exists() {
            log::info!("[build_ids] scan_build_id_pc rom from cache: {}", p.display());
            p
        } else {
            log::warn!("[build_ids] cached path missing, falling back to scan: {}", p.display());
            find_rom_path_pc(lookup_id, &game_name).ok_or_else(|| format!(
                "ROM not found for {lookup_id}. Add the game folder in Eden Settings \
                 or use \"Set ROM path\" to point to it directly."
            ))?
        }
    } else {
        let found = find_rom_path_pc(lookup_id, &game_name).ok_or_else(|| format!(
            "ROM not found for {lookup_id}. Add the game folder in Eden Settings \
             or use \"Set ROM path\" to point to it directly."
        ))?;
        cache.insert(lookup_id.to_string(), crate::rom_cache::RomCacheEntry {
            path: found.to_string_lossy().to_string(),
            manual: false,
        });
        crate::rom_cache::save_cache(&app, &cache);
        found
    };
    log::info!("[build_ids] scan_build_id_pc rom={}", rom_path.display());

    // 3. Record log baseline before launching so we only read new lines.
    let load_path = PathBuf::from(&load_dir);
    let base = load_path.parent().unwrap_or(&load_path);
    let log_candidates = [
        base.join("log/eden_log.txt"),
        base.join("log/eden_log.txt.old.txt"),
        base.join("eden_log.txt"),
        load_path.join("../log/eden_log.txt"),
        load_path.join("../log/eden_log.txt.old.txt"),
    ];
    let mut last_knowns: Vec<u64> = log_candidates
        .iter()
        .map(|p| {
            std::fs::read_to_string(p)
                .map(|s| s.lines().count() as u64)
                .unwrap_or(0)
        })
        .collect();

    // 4. Spawn Eden with the ROM path.
    let mut child = std::process::Command::new(&eden_exe)
        .arg(rom_path.to_string_lossy().as_ref())
        .spawn()
        .map_err(|e| format!("Failed to launch Eden ({eden_exe}): {e}"))?;
    log::info!("[build_ids] scan_build_id_pc: launched Eden pid={}", child.id());

    // 5. Poll the log for the build ID.
    let loader_re = loader_build_id_re();
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(SCAN_TIMEOUT_SECS);
    let mut poll_n = 0u32;
    let mut found: Option<String> = None;

    'poll: loop {
        if std::time::Instant::now() >= deadline {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
        poll_n += 1;

        for (i, log_path) in log_candidates.iter().enumerate() {
            let Ok(text) = std::fs::read_to_string(log_path) else {
                continue;
            };

            let lines: Vec<&str> = text.lines().collect();
            let total = lines.len() as u64;
            let from = if total < last_knowns[i] { 0 } else { last_knowns[i] as usize };
            let new_lines = &lines[from..];

            log::debug!(
                "[build_ids] scan_pc poll#{poll_n} {} from={from} new={}",
                log_path.display(),
                new_lines.len()
            );

            // Pass 1 — strict NSO-loader match.
            for line in new_lines {
                if let Some(cap) = loader_re.captures(line) {
                    let full = &cap[1];
                    found = Some(full[..16.min(full.len())].to_uppercase());
                    log::info!(
                        "[build_ids] scan_pc poll#{poll_n} found via name=main: {}",
                        found.as_deref().unwrap_or("")
                    );
                    break 'poll;
                }
            }

            // Pass 2 — title-context fallback.
            let new_text = new_lines.join("\n");
            if let Some(bid) =
                find_build_ids_for_title_in_log(&new_text, &title_id).into_iter().next()
            {
                log::info!("[build_ids] scan_pc poll#{poll_n} found via title-context: {bid}");
                found = Some(bid);
                break 'poll;
            }

            last_knowns[i] = total;
        }
    }

    // 6. Kill Eden.
    //    AppImage launchers spawn the real binary as a child process, so child.kill()
    //    alone only kills the wrapper and leaves Eden running.  pkill -P {pid} sends
    //    SIGTERM to all direct children of the launcher by PPID — reliable regardless
    //    of process group setup.  child.kill() + wait() then reaps the launcher itself.
    let launcher_pid = child.id().to_string();
    #[cfg(unix)]
    {
        log::info!("[build_ids] scan_build_id_pc: pkill -KILL -P {launcher_pid} (killing AppImage children)");
        let st = std::process::Command::new("pkill")
            .args(["-KILL", "-P", &launcher_pid])
            .status();
        log::debug!("[build_ids] scan_build_id_pc: pkill status={st:?}");
    }
    let _ = child.kill();
    let _ = child.wait();
    log::info!("[build_ids] scan_build_id_pc: Eden stopped");

    match found {
        Some(bid) => {
            log::info!("[build_ids] scan_build_id_pc title={title_id} -> {bid}");
            Ok(bid)
        }
        None => Err(format!(
            "Build ID not found within {SCAN_TIMEOUT_SECS}s. \
             The game may still be loading — try scanning again."
        )),
    }
}

// ── PC scan timing ──────────────────────────────────────────────────────────

/// How long to poll the log file before giving up.
/// The build_id line appears as soon as the NSO loader runs (very early in boot),
/// well before shader compilation — 5 s is enough.
const SCAN_TIMEOUT_SECS: u64 = 5;
/// Interval between log-file polls.
const POLL_INTERVAL_SECS: u64 = 2;
