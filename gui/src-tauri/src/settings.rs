use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub api_token: String,
    pub pc_load_dir: String,
    pub prod_keys_path: String,
    pub package_library_path: String,
    pub eden_exe_path: String,
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            api_token: String::new(),
            pc_load_dir: String::new(),
            prod_keys_path: String::new(),
            package_library_path: String::new(),
            eden_exe_path: String::new(),
            onboarding_done: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPlatform {
    Desktop,
    Android,
}

fn deserialize_settings(data: &str, platform: SettingsPlatform) -> Settings {
    let value: serde_json::Value = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => return Settings::default(),
    };
    let legacy_android = matches!(
        value.get("targetMode").and_then(serde_json::Value::as_str),
        Some("android" | "Android" | "ANDROID")
    );
    let mut settings: Settings = match serde_json::from_value(value) {
        Ok(settings) => settings,
        Err(_) => return Settings::default(),
    };

    if platform == SettingsPlatform::Desktop && legacy_android {
        let load_dir = settings.pc_load_dir.trim();
        if load_dir.is_empty() || !std::path::Path::new(load_dir).is_dir() {
            settings.onboarding_done = false;
        }
    }

    settings
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
        let platform = if cfg!(target_os = "android") {
            SettingsPlatform::Android
        } else {
            SettingsPlatform::Desktop
        };
        deserialize_settings(&data, platform)
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

/// Return the current compile-time platform: "android" or "desktop".
/// The frontend uses this to select the native or desktop backend.
#[tauri::command]
pub fn get_platform() -> &'static str {
    #[cfg(target_os = "android")]
    { "android" }
    #[cfg(not(target_os = "android"))]
    { "desktop" }
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

#[cfg(test)]
mod tests {
    use super::{deserialize_settings, Settings, SettingsPlatform};
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn released_android_settings(target_mode: &str, pc_load_dir: &str) -> String {
        json!({
            "targetMode": target_mode,
            "apiToken": " retained token ",
            "adbPath": "/opt/android/platform-tools/adb",
            "pcLoadDir": pc_load_dir,
            "edenExePath": "/Applications/eden",
            "onboardingDone": true,
            "savedConnections": [{
                "label": "Living room",
                "ip": "192.0.2.10",
                "port": "5555"
            }],
            "activeDevice": {
                "type": "wireless",
                "serial": "192.0.2.10:5555",
                "label": "Living room"
            }
        })
        .to_string()
    }

    fn assert_retained_values(settings: &Settings, pc_load_dir: &str) {
        assert_eq!(settings.api_token, " retained token ");
        assert_eq!(settings.pc_load_dir, pc_load_dir);
        assert_eq!(settings.prod_keys_path, "");
        assert_eq!(settings.package_library_path, "");
        assert_eq!(settings.eden_exe_path, "/Applications/eden");
    }

    #[test]
    fn released_android_modes_trigger_desktop_migration() {
        let temp = TestDirectory::new();
        let stored_path = format!(" {} ", temp.path().join("missing").display());
        for target_mode in ["android", "Android", "ANDROID"] {
            let settings = deserialize_settings(
                &released_android_settings(target_mode, &stored_path),
                SettingsPlatform::Desktop,
            );
            assert_retained_values(&settings, &stored_path);
            assert!(!settings.onboarding_done);
        }
    }

    #[test]
    fn pre_eden_executable_settings_deserialize() {
        let settings = deserialize_settings(
            r#"{
                "targetMode": "android",
                "apiToken": "token",
                "adbPath": "adb",
                "pcLoadDir": "load",
                "onboardingDone": true,
                "savedConnections": [],
                "activeDevice": null
            }"#,
            SettingsPlatform::Android,
        );

        assert_eq!(settings.api_token, "token");
        assert_eq!(settings.pc_load_dir, "load");
        assert_eq!(settings.prod_keys_path, "");
        assert_eq!(settings.package_library_path, "");
        assert_eq!(settings.eden_exe_path, "");
        assert!(settings.onboarding_done);
    }

    fn settings_json(target_mode: Option<&str>, pc_load_dir: &str) -> String {
        let mut value = json!({
            "apiToken": " token kept exactly ",
            "pcLoadDir": pc_load_dir,
            "edenExePath": " eden kept exactly ",
            "onboardingDone": true
        });
        if let Some(target_mode) = target_mode {
            value["targetMode"] = json!(target_mode);
        }
        value.to_string()
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "eden-cheats-manager-settings-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_retained_fields_use_individual_defaults() {
        let settings =
            deserialize_settings(r#"{"apiToken":"preserved"}"#, SettingsPlatform::Desktop);

        assert_eq!(
            settings,
            Settings {
                api_token: "preserved".into(),
                ..Settings::default()
            }
        );
    }

    #[test]
    fn legacy_android_requires_a_desktop_directory() {
        let temp = TestDirectory::new();
        let file = temp.path().join("not-a-directory");
        std::fs::write(&file, b"file").unwrap();
        let nonexistent = temp.path().join("missing");
        let unusable = [
            String::new(),
            "   \t".to_string(),
            nonexistent.to_string_lossy().into_owned(),
            file.to_string_lossy().into_owned(),
        ];

        for stored_path in unusable {
            let settings = deserialize_settings(
                &settings_json(Some("android"), &stored_path),
                SettingsPlatform::Desktop,
            );
            assert_eq!(settings.pc_load_dir, stored_path);
            assert!(!settings.onboarding_done);
        }
    }

    #[test]
    fn legacy_android_accepts_an_empty_desktop_directory() {
        let temp = TestDirectory::new();
        let stored_path = format!("  {}  ", temp.path().display());
        let settings = deserialize_settings(
            &settings_json(Some("android"), &stored_path),
            SettingsPlatform::Desktop,
        );

        assert_eq!(settings.pc_load_dir, stored_path);
        assert!(settings.onboarding_done);
    }

    #[test]
    fn non_adb_desktop_modes_do_not_reopen_onboarding() {
        for target_mode in [
            None,
            Some("pc"),
            Some("Pc"),
            Some("PC"),
            Some("androidNative"),
            Some("AndroidNative"),
            Some("ANDROID_NATIVE"),
            Some("aNdRoId"),
        ] {
            let settings =
                deserialize_settings(&settings_json(target_mode, ""), SettingsPlatform::Desktop);
            assert!(settings.onboarding_done);
        }
    }

    #[test]
    fn native_android_does_not_apply_desktop_normalization() {
        let settings = deserialize_settings(
            &settings_json(Some("android"), ""),
            SettingsPlatform::Android,
        );

        assert!(settings.onboarding_done);
    }

    #[test]
    fn malformed_removed_fields_do_not_discard_retained_values() {
        let settings = deserialize_settings(
            &json!({
                "targetMode": "android",
                "apiToken": "token",
                "adbPath": { "unexpected": true },
                "pcLoadDir": "load",
                "edenExePath": "eden",
                "onboardingDone": true,
                "savedConnections": "unexpected",
                "activeDevice": 42
            })
            .to_string(),
            SettingsPlatform::Android,
        );

        assert_eq!(settings.api_token, "token");
        assert_eq!(settings.pc_load_dir, "load");
        assert_eq!(settings.eden_exe_path, "eden");
        assert!(settings.onboarding_done);
    }

    #[test]
    fn malformed_json_uses_reduced_defaults() {
        assert_eq!(
            deserialize_settings("{not json", SettingsPlatform::Desktop),
            Settings::default()
        );
    }

    #[test]
    fn package_library_paths_round_trip_in_camel_case() {
        let settings = Settings {
            api_token: "token".into(),
            pc_load_dir: "load".into(),
            prod_keys_path: "keys/prod.keys".into(),
            package_library_path: "packages".into(),
            eden_exe_path: "eden".into(),
            onboarding_done: true,
        };

        assert_eq!(
            serde_json::to_value(settings).unwrap(),
            json!({
                "apiToken": "token",
                "pcLoadDir": "load",
                "prodKeysPath": "keys/prod.keys",
                "packageLibraryPath": "packages",
                "edenExePath": "eden",
                "onboardingDone": true
            })
        );
    }
}
