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
