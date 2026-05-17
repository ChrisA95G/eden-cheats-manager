use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetMode {
    #[serde(alias = "Pc", alias = "PC")]
    Pc,
    #[serde(alias = "Android", alias = "ANDROID")]
    Android,
}

impl Default for TargetMode {
    fn default() -> Self {
        TargetMode::Pc
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub label: String,
    pub ip: String,
    pub port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDevice {
    #[serde(rename = "type")]
    pub device_type: String, // "usb" or "wireless"
    pub serial: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub target_mode: TargetMode,
    pub api_token: String,
    pub adb_path: String,
    pub pc_load_dir: String,
    #[serde(default)]
    pub eden_exe_path: String,
    pub onboarding_done: bool,
    #[serde(default)]
    pub saved_connections: Vec<SavedConnection>,
    #[serde(default)]
    pub active_device: Option<ActiveDevice>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            target_mode: TargetMode::Pc,
            api_token: String::new(),
            adb_path: String::new(),
            pc_load_dir: String::new(),
            eden_exe_path: String::new(),
            onboarding_done: false,
            saved_connections: Vec::new(),
            active_device: None,
        }
    }
}

fn settings_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("failed to resolve config dir")
        .join(SETTINGS_FILE)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    let path = settings_path(&app);
    if let Ok(data) = std::fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Settings::default()
    }
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(())
}

/// Return the ECM app log file path.
#[tauri::command]
pub fn get_app_log_path(app: AppHandle) -> String {
    app.path()
        .app_log_dir()
        .map(|d| d.join("eden-cheats-manager.log").to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Return the Eden PC log path given a `load_dir`.
/// Returns the first existing candidate, or empty string if none exist yet.
#[tauri::command]
pub fn get_eden_log_path_pc(load_dir: String) -> String {
    let load_path = PathBuf::from(&load_dir);
    let base = load_path.parent().unwrap_or(&load_path);
    let candidates = [
        base.join("log/eden_log.txt"),
        base.join("eden_log.txt"),
        load_path.join("../log/eden_log.txt"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.canonicalize()
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy()
                .to_string();
        }
    }
    String::new()
}

/// Try to find the Eden executable via PATH and well-known install locations.
/// Returns the first match, or an empty string if not found.
#[tauri::command]
pub fn detect_eden_exe() -> String {
    // Try PATH first
    #[cfg(unix)]
    if let Ok(out) = std::process::Command::new("which").arg("eden").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return p;
            }
        }
    }
    #[cfg(windows)]
    if let Ok(out) = std::process::Command::new("where").arg("eden").output() {
        if out.status.success() {
            if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().next() {
                let p = line.trim().to_string();
                if !p.is_empty() {
                    return p;
                }
            }
        }
    }

    // Well-known install locations
    let mut candidates: Vec<String> = Vec::new();
    if cfg!(target_os = "linux") {
        if let Some(home) = dirs_next::home_dir() {
            candidates.push(home.join(".local/bin/eden").to_string_lossy().into_owned());
        }
        candidates.push("/usr/bin/eden".into());
        candidates.push("/usr/local/bin/eden".into());
        candidates.push("/opt/eden/eden".into());
    } else if cfg!(target_os = "windows") {
        if let Ok(lad) = std::env::var("LOCALAPPDATA") {
            candidates.push(format!("{}\\Programs\\eden\\eden.exe", lad));
        }
        if let Ok(pf) = std::env::var("PROGRAMFILES") {
            candidates.push(format!("{}\\eden\\eden.exe", pf));
        }
    } else if cfg!(target_os = "macos") {
        candidates.push("/Applications/eden.app/Contents/MacOS/eden".into());
    }
    for p in candidates {
        if PathBuf::from(&p).exists() {
            return p;
        }
    }
    String::new()
}

/// Return the platform-default Eden PC load directory (best-effort).
#[tauri::command]
pub fn detect_pc_load_dir() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs_next::home_dir() {
            let p = home.join(".local/share/eden/load");
            if p.exists() {
                return p.to_string_lossy().to_string();
            }
            // Return the expected path even if it doesn't exist yet
            return p.to_string_lossy().to_string();
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return format!("{}\\eden\\load", appdata);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs_next::home_dir() {
            return home
                .join("Library/Application Support/eden/load")
                .to_string_lossy()
                .to_string();
        }
    }
    String::new()
}
