use crate::adb::{adb_bin, adb_ls, REMOTE_BASE};
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
#[allow(unused_imports)]
use dirs_next;

// Looser regex used for contextual log window: no `name=main` requirement.
static LOG_BUILD_ID_RE: OnceLock<Regex> = OnceLock::new();
fn log_build_id_re() -> &'static Regex {
    LOG_BUILD_ID_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64})").unwrap()
    })
}

// Strict regex that only matches NSO-loader lines: `build_id=HEX, name=main`.
// Library-scan cache entries never include `, name=main` — only the NSO ELF Note
// parser emits this suffix, and it is written exclusively during actual emulation
// startup (not during MainActivity's game-library scan).
static LOADER_BUILD_ID_RE: OnceLock<Regex> = OnceLock::new();
fn loader_build_id_re() -> &'static Regex {
    LOADER_BUILD_ID_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64}),\s*name=main").unwrap()
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
fn find_build_ids_for_title_in_log(text: &str, title_id: &str) -> Vec<String> {
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

// ── Android ───────────────────────────────────────────────────────────────────

/// Detect build IDs for a specific title on an Android device.
///
/// Strategy 1 — Installed cheat files (instant, offline):
///   Scans `{REMOTE_BASE}/{title_id}/*/cheats/*.txt`; the filename stem *is*
///   the build ID.  Works whenever at least one cheat has been installed.
///
/// Strategy 2 — Eden log (per-title contextual search):
///   Greps the Eden log files for lines containing `title_id` and extracts
///   build IDs from the surrounding context window.
///
/// // TODO Strategy 3 — NSO binary header parsing:
///   Pull the first 0x100 bytes of the game executable NSO file from
///   `{eden_data}/cache/game_cache/{title_id}/exefs/main` and parse the
///   build-id Note segment.  Implement when Strategies 1 & 2 are insufficient.
#[tauri::command]
pub fn detect_build_ids_android(
    adb_path: String,
    title_id: String,
) -> Result<Vec<String>, String> {
    log::info!("[build_ids] detect_android title={title_id}");
    let adb = adb_bin(&adb_path);
    let mut seen: HashSet<String> = HashSet::new();
    let mut ids: Vec<String> = Vec::new();

    // ── Strategy 1: installed cheat filenames ─────────────────────────────
    let title_dir = format!("{}/{}", REMOTE_BASE, title_id);
    if let Ok(cheat_names) = adb_ls(adb_path.clone(), title_dir.clone()) {
        for name in &cheat_names {
            let cheats_dir = format!("{}/{}/cheats", title_dir, name);
            if let Ok(files) = adb_ls(adb_path.clone(), cheats_dir) {
                for file in files {
                    if file.ends_with(".txt") {
                        let bid = file.trim_end_matches(".txt").to_uppercase();
                        if bid.len() == 16 && seen.insert(bid.clone()) {
                            log::debug!("[build_ids] installed-cheat strategy: {bid}");
                            ids.push(bid);
                        }
                    }
                }
            }
        }
    }
    log::debug!("[build_ids] after strategy 1: {} ids", ids.len());

    // ── Strategy 2: Eden log (contextual window around title_id) ──────────
    let log_candidates = [
        "/sdcard/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt",
        "/sdcard/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt.old.txt",
        "/sdcard/eden.log",
    ];

    for remote in &log_candidates {
        match Command::new(&adb).args(["shell", "cat", remote]).output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                for bid in find_build_ids_for_title_in_log(&text, &title_id) {
                    if bid.len() == 16 && seen.insert(bid.clone()) {
                        log::debug!("[build_ids] log strategy ({remote}): {bid}");
                        ids.push(bid);
                    }
                }
            }
            _ => {}
        }
    }

    // Dynamic fallback: search for any .log files if none of the above had results
    if ids.is_empty() {
        if let Ok(out) = Command::new(&adb)
            .args([
                "shell",
                "find",
                "/sdcard/Android/data/dev.eden.eden_emulator",
                "-name",
                "*.log",
                "-type",
                "f",
            ])
            .output()
        {
            let paths = String::from_utf8_lossy(&out.stdout);
            for path in paths.lines() {
                let p = path.trim();
                if p.is_empty() {
                    continue;
                }
                if let Ok(cat) = Command::new(&adb).args(["shell", "cat", p]).output() {
                    let text = String::from_utf8_lossy(&cat.stdout);
                    for bid in find_build_ids_for_title_in_log(&text, &title_id) {
                        if bid.len() == 16 && seen.insert(bid.clone()) {
                            log::debug!("[build_ids] log fallback ({p}): {bid}");
                            ids.push(bid);
                        }
                    }
                }
            }
        }
    }

    ids.sort();
    log::info!("[build_ids] detect_android title={title_id} -> {:?}", ids);
    Ok(ids)
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
fn get_eden_config_path() -> Option<PathBuf> {
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
fn search_roms_in_dir(dir: &std::path::Path, title_id_lower: &str, depth: u32) -> Option<PathBuf> {
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
                if fname.contains(&format!("[{}]", title_id_lower)) {
                    return Some(path);
                }
            }
        } else if path.is_dir() && depth > 0 {
            if let Some(found) = search_roms_in_dir(&path, title_id_lower, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Search Eden's configured game directories on PC for a ROM file whose
/// filename contains `[{title_id}]`.
fn find_rom_path_pc(title_id: &str) -> Option<PathBuf> {
    let config_path = get_eden_config_path()?;
    log::debug!("[build_ids] find_rom_pc: config exists={}", config_path.exists());

    let config = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[build_ids] find_rom_pc: cannot read config.ini ({}): {e}", config_path.display());
            return None;
        }
    };

    let tid_lower = title_id.to_lowercase();
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Virtual sentinel values Eden uses for NAND/SDMC — not real filesystem paths.
    const VIRTUAL: &[&str] = &["SDMC", "UserNAND", "SysNAND"];

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
        if VIRTUAL.contains(&raw) {
            log::debug!("[build_ids] find_rom_pc: virtual entry '{raw}', skipping");
            continue;
        }
        let p = PathBuf::from(raw);
        log::debug!(
            "[build_ids] find_rom_pc: game dir={} exists={}",
            p.display(), p.exists()
        );

        // Add parent dir first, then the game dir itself — mirrors find_rom_path_android.
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
        if let Some(found) = search_roms_in_dir(dir, &tid_lower, 2) {
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
    load_dir: String,
    title_id: String,
    eden_exe_path: String,
) -> Result<String, String> {
    log::info!("[build_ids] scan_build_id_pc title={title_id} load_dir={load_dir}");

    // 1. Resolve Eden executable.
    let eden_exe = resolve_eden_exe(&eden_exe_path).ok_or_else(|| {
        "Eden executable not found on PATH. Set the Eden executable path in Settings.".to_string()
    })?;
    log::info!("[build_ids] scan_build_id_pc eden_exe={eden_exe}");

    // 2. Find ROM in Eden's configured game directories.
    let rom_path = find_rom_path_pc(&title_id).ok_or_else(|| {
        format!(
            "ROM not found for {title_id}. \
             Make sure the game folder is added in Eden's settings."
        )
    })?;
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

// ── Scan Build ID (Launch + Log Poll) ───────────────────────────────────────

/// Eden package and emulation activity — update if the package name ever changes.
const EDEN_PKG: &str = "dev.eden.eden_emulator";
const EDEN_ACTIVITY: &str = "org.yuzu.yuzu_emu.activities.EmulationActivity";
/// Eden writes its logs here (not to logcat).
const EDEN_LOG_PATH: &str =
    "/sdcard/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt";
/// How long to poll the log file before giving up.
/// Games with shader compilation can take 60 s or more to boot past the point
/// where Eden writes the build_id line.
const SCAN_TIMEOUT_SECS: u64 = 90;
/// Interval between log-file polls.
const POLL_INTERVAL_SECS: u64 = 2;
/// How long to wait (max) for the MainActivity library scan to settle before
/// recording the log baseline and launching EmulationActivity.
const KEYS_READY_TIMEOUT_SECS: u64 = 25;

/// Returns the current number of lines in a remote file via `wc -l`.
/// Used to establish a baseline so we only inspect *new* log lines after launch.
fn get_remote_line_count(adb: &str, path: &str) -> Option<u64> {
    let out = Command::new(adb)
        .args(["shell", &format!("wc -l '{}'", path)])
        .output()
        .ok()?;
    // `wc -l` output: "  1234 /path/to/file" — take the first whitespace token
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Percent-encode a string as a URI path component, following Android's `Uri.encode()` rules.
/// Preserves letters, digits, and `-_!.~'()*`; encodes everything else (including `:`, `/`,
/// space, `[`, `]`) which is required for document IDs used in content:// URIs.
fn percent_encode_doc_id(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'!' | b'.' | b'~' | b'\'' | b'(' | b')' | b'*' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Build a `content://com.android.externalstorage.documents/tree/…/document/…` URI
/// from a physical file path and the (raw_tree_uri, physical_base) pairs from config.ini.
///
/// Eden scans ROMs via the Storage Access Framework and stores them as content:// URIs.
/// `GameMetadata.getIsValid()` opens files through Android's ContentResolver using the
/// content:// URI.  Raw `file:///storage/<sdcard>/…` paths fail on external SD cards
/// because the FUSE layer requires SAF / ContentResolver access.
fn physical_to_content_uri(physical_path: &str, tree_entries: &[(String, String)]) -> Option<String> {
    // /storage/primary is an alias for /storage/emulated/0 on many devices.
    let normalise = |p: &str| -> String {
        if p.starts_with("/storage/primary") {
            p.replacen("/storage/primary", "/storage/emulated/0", 1)
        } else {
            p.to_string()
        }
    };
    let norm_file = normalise(physical_path);

    for (raw_tree_uri, physical_base) in tree_entries {
        if !norm_file.starts_with(normalise(physical_base).as_str()) {
            continue;
        }
        // The tree document ID is the already-encoded segment after `/tree/` in the URI
        // (e.g. `4A21-0000%3ARoms%2FSwitch`).
        let tree_doc_id_enc = raw_tree_uri.split("/tree/").nth(1)?;

        // File document ID: "{volumeId}:{relativePathFromVolumeRoot}"
        let (volume_id, rel): (&str, &str) =
            if physical_path.starts_with("/storage/emulated/0/") {
                ("primary", &physical_path["/storage/emulated/0/".len()..])
            } else {
                let after = physical_path.strip_prefix("/storage/")?;
                let slash = after.find('/')?;
                (&after[..slash], &after[slash + 1..])
            };

        let file_doc_id = format!("{}:{}", volume_id, rel);
        let file_doc_id_enc = percent_encode_doc_id(&file_doc_id);

        return Some(format!(
            "content://com.android.externalstorage.documents/tree/{}/document/{}",
            tree_doc_id_enc, file_doc_id_enc
        ));
    }
    None
}

/// Search Eden's configured game directories on the Android device for a ROM
/// file whose filename contains `[{title_id}]` (the standard NSP naming convention).
///
/// Returns a `content://com.android.externalstorage.documents/…` URI for games on
/// external SD cards (required for Eden to open them via ContentResolver), or a
/// `file://` URI as a fallback for internal storage.
fn find_rom_path_android(adb: &str, title_id: &str) -> Option<String> {
    const CONFIG_PATH: &str =
        "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/config/config.ini";
    let config_out = Command::new(adb)
        .args(["shell", "cat", CONFIG_PATH])
        .output()
        .ok()?;
    if !config_out.status.success() {
        log::debug!("[build_ids] find_rom: config.ini unavailable");
        return None;
    }
    let config = String::from_utf8_lossy(&config_out.stdout);
    let tid_lower = title_id.to_lowercase();

    // Collect (raw_tree_uri, physical_base) pairs and ordered search directories.
    let mut tree_entries: Vec<(String, String)> = Vec::new();
    let mut search_dirs: Vec<String> = Vec::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();

    for line in config.lines() {
        if !line.contains("\\path=") {
            continue;
        }
        let raw_uri = line.splitn(2, '=').nth(1).unwrap_or("").trim_matches('"');
        if let Some(physical) = crate::games::content_uri_to_physical(raw_uri) {
            if let Some(parent) = std::path::Path::new(&physical).parent() {
                let p = parent.to_string_lossy().to_string();
                if seen_dirs.insert(p.clone()) {
                    search_dirs.push(p);
                }
            }
            if seen_dirs.insert(physical.clone()) {
                search_dirs.push(physical.clone());
            }
            tree_entries.push((raw_uri.to_string(), physical));
        }
    }
    log::debug!("[build_ids] find_rom: searching {} dirs", search_dirs.len());

    for dir in &search_dirs {
        for ext in &["*.nsp", "*.xci"] {
            let cmd = format!("find '{}' -maxdepth 2 -name '{}'", dir, ext);
            if let Ok(find_out) = Command::new(adb).args(["shell", &cmd]).output() {
                for line in String::from_utf8_lossy(&find_out.stdout).lines() {
                    let p = line.trim();
                    if p.is_empty() {
                        continue;
                    }
                    if p.to_lowercase().contains(&format!("[{}]", tid_lower)) {
                        log::info!("[build_ids] find_rom physical: {p}");
                        if let Some(uri) = physical_to_content_uri(p, &tree_entries) {
                            log::info!("[build_ids] find_rom content_uri: {uri}");
                            return Some(uri);
                        }
                        let uri = format!("file://{}", p);
                        log::info!("[build_ids] find_rom file_uri (fallback): {uri}");
                        return Some(uri);
                    }
                }
            }
        }
    }
    None
}

/// Wait until Eden's MainActivity library scan has finished writing to the log,
/// using a log-stability heuristic: proceed once the log line-count has been
/// stable (unchanged) for `stable_polls` consecutive polls, or after the hard
/// timeout, whichever comes first.
///
/// This replaces a fixed sleep so we neither waste time when the scan is fast
/// nor record the baseline while library-scan `build_id=` entries are still
/// being written (which would put those lines in the polling window and cause
/// `scan_build_id_android` to match a wrong build ID).
async fn poll_for_keys_ready(adb: &str) {
    const POLL_MS: u64 = 500;
    const STABLE_POLLS: u32 = 4; // 4 × 500 ms = 2 s of no log growth
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(KEYS_READY_TIMEOUT_SECS);

    let mut prev_lines: Option<u64> = None;
    let mut stable_count = 0u32;

    loop {
        if std::time::Instant::now() >= deadline {
            log::info!("[build_ids] keys-ready: timeout reached, proceeding");
            return;
        }

        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;

        let cur = get_remote_line_count(adb, EDEN_LOG_PATH);
        match (prev_lines, cur) {
            (Some(prev), Some(cur)) if cur == prev => {
                stable_count += 1;
                log::debug!(
                    "[build_ids] keys-ready: stable poll {stable_count}/{STABLE_POLLS} \
                     ({cur} lines)"
                );
                if stable_count >= STABLE_POLLS {
                    log::info!(
                        "[build_ids] keys-ready: log stable at {cur} lines, proceeding"
                    );
                    return;
                }
            }
            (_, cur_opt) => {
                if stable_count > 0 {
                    log::debug!("[build_ids] keys-ready: log grew, resetting stable count");
                }
                stable_count = 0;
                prev_lines = cur_opt;
            }
        }
    }
}

/// Launch a game in Eden via ADB, then poll Eden's log file for a new
/// `build_id=<hex>` line near `title_id`.  Force-stops Eden once found (or on timeout).
///
/// Eden does NOT write build info to logcat — it uses a private log file:
///   /sdcard/Android/data/dev.eden.eden_emulator/files/log/eden_log.txt
///
/// **Root-cause note**: Launching `EmulationActivity` directly (cold start) fails
/// with "No bootable game found" because Eden's JNI `GameMetadata.getIsValid()`
/// requires decryption keys that are only loaded when `MainActivity` calls
/// `GameHelper.getGames()` → `NativeLibrary.reloadKeys()`.  The fix is a two-step
/// launch: first bring up `MainActivity` to warm up the process, then send the
/// `ACTION_VIEW + file:///` intent to `EmulationActivity`.
///
/// **Warmup strategy**: After force-stopping Eden, `am start -W` is used to launch
/// `MainActivity` — this flag blocks ADB until the activity window is drawn, ensuring
/// the Eden process is fully started.  A fixed 15 s sleep then lets the background
/// game-directory scan (which calls `NativeLibrary.reloadKeys()`) complete before
/// we send the `EmulationActivity` intent.  Log-file growth is NOT a reliable
/// readiness signal because Eden may skip writing to `eden_log.txt` during
/// `MainActivity` startup on second runs (process state cached, no log rotation yet).
///
/// Pipeline:
/// 1. Auto-discover the ROM path from Eden's `config.ini` game directories.
/// 2. Force-stop Eden for a clean process state.
/// 3. Launch `MainActivity` with `-W` (blocks until activity is on screen).
/// 4. Sleep 15 s for background game scan / `reloadKeys()` to complete.
/// 5. Record log-file baseline (right before the game launch).
/// 6. Launch `EmulationActivity` with `ACTION_VIEW -d file:///…`.
/// 7. Poll new log lines every 2 s; `last_known` tracks position across rotations.
/// 8. Force-stop Eden, return the 16-char uppercase Build ID.
#[tauri::command]
pub async fn scan_build_id_android(
    adb_path: String,
    title_id: String,
) -> Result<String, String> {
    log::info!("[build_ids] scan_build_id title={title_id}");
    let adb = adb_bin(&adb_path);

    // 1. Find ROM URI — content:// for SD-card games, file:// for internal storage.
    let rom_uri = find_rom_path_android(&adb, &title_id).ok_or_else(|| {
        format!(
            "ROM not found on device for {title_id}. \
             Make sure the game is in a directory configured in Eden's settings."
        )
    })?;
    log::info!("[build_ids] scan_build_id rom_uri={rom_uri}");

    // 2. Force-stop Eden for a clean process state.
    let _ = Command::new(&adb)
        .args(["shell", "am", "force-stop", EDEN_PKG])
        .output();
    log::info!("[build_ids] Eden force-stopped");

    // 3. Launch MainActivity with -W (waits until activity window is drawn = process live).
    //    Without this warm-up, a cold EmulationActivity launch results in getIsValid()
    //    returning false for every ROM, causing the "No bootable game found" toast,
    //    because NativeLibrary.reloadKeys() hasn't been called yet.
    const MAIN_ACTIVITY: &str = "org.yuzu.yuzu_emu.ui.main.MainActivity";
    let main_component = format!("{EDEN_PKG}/{MAIN_ACTIVITY}");
    let main_launch = Command::new(&adb)
        .args(["shell", "am", "start", "-W", "-n", &main_component])
        .output()
        .map_err(|e| format!("Failed to launch Eden MainActivity: {e}"))?;
    log::info!(
        "[build_ids] MainActivity launch exit={} stdout={}",
        main_launch.status,
        String::from_utf8_lossy(&main_launch.stdout).trim()
    );

    // 4. Wait for Eden's background game-scan / reloadKeys() to finish writing to the log.
    //    We poll the log line-count and proceed once it has been stable for 2 s, or after
    //    a 25 s hard timeout.  This is more reliable than a fixed sleep: it avoids the race
    //    where library-scan `build_id=` lines are still being written when we record the
    //    baseline, which caused the poller to match the wrong game's build ID.
    log::info!("[build_ids] waiting for Eden game scan / key loading (log-stability poll)…");
    poll_for_keys_ready(&adb).await;
    log::info!("[build_ids] warmup complete");

    // 5. Record log baseline right before the game launch so we only read NEW lines.
    //    Taking it here (after warmup) avoids the log-rotation problem that occurs
    //    when EmulationActivity starts a fresh Eden log session.
    let start_line = get_remote_line_count(&adb, EDEN_LOG_PATH).unwrap_or(0);
    log::info!("[build_ids] log baseline={start_line} lines");

    // 6. Launch EmulationActivity with ACTION_VIEW + the ROM URI.
    //    For SD-card games this is a content:// URI (Eden opens those via ContentResolver).
    //    For internal-storage games a file:// URI is used as fallback.
    //    Confirmed path (EmulationFragment.kt): `intent.data != null` →
    //    `GameHelper.getGame(uri, false)` → reads game metadata → starts emulation.
    let escaped_uri = rom_uri.replace('\'', r"'\''" );
    let component = format!("{EDEN_PKG}/{EDEN_ACTIVITY}");
    let shell_cmd = format!(
        "am start -a android.intent.action.VIEW -n '{component}' -d '{escaped_uri}'"
    );
    log::info!("[build_ids] am start: {shell_cmd}");
    let launch = Command::new(&adb)
        .args(["shell", &shell_cmd])
        .output()
        .map_err(|e| format!("Failed to run am start: {e}"))?;

    let launch_stdout = String::from_utf8_lossy(&launch.stdout).to_string();
    let launch_stderr = String::from_utf8_lossy(&launch.stderr).to_string();
    log::info!(
        "[build_ids] am start exit={} stdout={:?} stderr={:?}",
        launch.status,
        launch_stdout.trim(),
        launch_stderr.trim()
    );

    if !launch.status.success() {
        let _ = Command::new(&adb)
            .args(["shell", "am", "force-stop", EDEN_PKG])
            .output();
        return Err(format!("Eden launch failed: {launch_stdout}{launch_stderr}"));
    }

    // 7. Poll the log file for build_id lines.
    //    `last_known` tracks the last observed line count and is updated every poll,
    //    so we always read only lines added since the previous check.  This correctly
    //    handles log rotation (EmulationActivity may create a fresh log session).
    let adb_poll = adb.clone();
    let title_id_poll = title_id.clone();
    let found = tokio::time::timeout(
        std::time::Duration::from_secs(SCAN_TIMEOUT_SECS),
        async move {
            let mut poll_n = 0u32;
            let mut last_known = start_line;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
                poll_n += 1;

                let current_lines =
                    get_remote_line_count(&adb_poll, EDEN_LOG_PATH).unwrap_or(0);

                // Detect log rotation (file shrank) or normal growth.
                let read_from = if current_lines < last_known {
                    log::info!(
                        "[build_ids] poll#{poll_n} log rotated \
                         (was {last_known}, now {current_lines}) — reading from line 1"
                    );
                    1
                } else {
                    last_known + 1
                };

                // `tail -n +N` reads from line N to end (1-indexed).
                let tail_cmd = format!("tail -n +{read_from} '{}'", EDEN_LOG_PATH);
                match Command::new(&adb_poll).args(["shell", &tail_cmd]).output() {
                    Err(e) => {
                        log::warn!("[build_ids] poll#{poll_n} tail error: {e}");
                    }
                    Ok(out) => {
                        let text = String::from_utf8_lossy(&out.stdout);
                        log::info!(
                            "[build_ids] poll#{poll_n} read_from={read_from} \
                             current_lines={current_lines} new_lines={}",
                            text.lines().count()
                        );
                        // Primary: strict NSO-loader match — `build_id=HEX, name=main`.
                        // Library-scan cache entries never include `, name=main`, so this
                        // fires only when the emulation session has actually started and
                        // the NSO ELF Note parser has run.  No title_id proximity check
                        // is needed: we launched exactly one game, so the first
                        // `name=main` line must belong to it.
                        let loader_re = loader_build_id_re();
                        for line in text.lines() {
                            if let Some(cap) = loader_re.captures(line) {
                                let full = &cap[1];
                                let bid = full[..16.min(full.len())].to_uppercase();
                                log::info!(
                                    "[build_ids] poll#{poll_n} found via name=main: {bid}"
                                );
                                return Some(bid);
                            }
                        }

                        // Fallback: title-context window — fires only after SCAN_TIMEOUT_SECS/2
                        // to give the primary path enough time.  Handles the unlikely case where
                        // Eden doesn't emit a `, name=main` suffix (e.g. older builds).
                        let elapsed_polls = poll_n * POLL_INTERVAL_SECS as u32;
                        if elapsed_polls as u64 >= SCAN_TIMEOUT_SECS / 2 {
                            let ids = find_build_ids_for_title_in_log(&text, &title_id_poll);
                            if let Some(bid) = ids.into_iter().next() {
                                log::info!(
                                    "[build_ids] poll#{poll_n} found via title-context fallback: {bid}"
                                );
                                return Some(bid);
                            }
                        }
                    }
                }
                // Advance the read position so each poll only fetches truly new lines.
                last_known = current_lines;
            }
        },
    )
    .await
    .unwrap_or(None);

    // 8. Force-stop Eden
    let _ = Command::new(&adb)
        .args(["shell", "am", "force-stop", EDEN_PKG])
        .output();

    match found {
        Some(bid) => {
            log::info!("[build_ids] scan_build_id title={title_id} -> {bid}");
            Ok(bid)
        }
        None => Err(format!(
            "Build ID not found within {SCAN_TIMEOUT_SECS}s. \
             The game may still be loading — try 'Detect Build IDs' after launching \
             it manually in Eden once."
        )),
    }
}
