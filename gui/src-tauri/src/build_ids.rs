use std::path::PathBuf;

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
