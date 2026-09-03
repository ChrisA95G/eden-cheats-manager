use regex::Regex;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::OnceLock;

pub(crate) const ANDROID_CONFIG_PATH: &str =
    "/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/config/config.ini";

pub(crate) const EDEN_PKG: &str = "dev.eden.eden_emulator";

pub(crate) const EDEN_VIRTUAL_DIRS: &[&str] = &["SDMC", "UserNAND", "SysNAND"];

static LOADER_BUILD_ID_RE: OnceLock<Regex> = OnceLock::new();

pub(crate) fn loader_build_id_re() -> &'static Regex {
    LOADER_BUILD_ID_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64}),\s*name=main").unwrap()
    })
}

pub(crate) fn adb_bin(override_path: &str) -> String {
    if override_path.is_empty() {
        "adb".to_string()
    } else {
        override_path.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AdbStatus {
    pub connected: bool,
    pub device_id: String,
    pub details: String,
}

#[tauri::command]
pub fn get_adb_status(adb_path: String) -> AdbStatus {
    let adb = adb_bin(&adb_path);
    match Command::new(&adb).arg("devices").output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let devices: Vec<&str> = stdout
                .lines()
                .skip(1)
                .filter(|l| !l.trim().is_empty() && l.contains("\tdevice"))
                .collect();
            if let Some(dev) = devices.first() {
                let device_id = dev.split('\t').next().unwrap_or("").trim().to_string();
                AdbStatus {
                    connected: true,
                    device_id: device_id.clone(),
                    details: format!("Device: {}", device_id),
                }
            } else {
                AdbStatus {
                    connected: false,
                    device_id: String::new(),
                    details: if devices.is_empty() {
                        "No devices found. Check USB or WiFi connection.".to_string()
                    } else {
                        "Device found but not authorized. Accept prompt on phone.".to_string()
                    },
                }
            }
        }
        Err(e) => AdbStatus {
            connected: false,
            device_id: String::new(),
            details: format!("ADB not found: {}", e),
        },
    }
}

#[tauri::command]
pub fn get_usb_devices(adb_path: String) -> Vec<String> {
    let adb = adb_bin(&adb_path);
    match Command::new(&adb).args(["devices", "-l"]).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            stdout
                .lines()
                .skip(1)
                .filter(|l| {
                    let trimmed = l.trim();
                    !trimmed.is_empty() && trimmed.contains("\tdevice") && !trimmed.contains(':')
                })
                .filter_map(|l| {
                    let serial = l.split('\t').next()?.trim().to_string();
                    if serial.is_empty() { None } else { Some(serial) }
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
pub fn adb_tcpip(adb_path: String) -> Result<String, String> {
    let adb = adb_bin(&adb_path);
    let out = Command::new(&adb)
        .args(["tcpip", "5555"])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() || stdout.contains("restarting") {
        Ok(stdout + &stderr)
    } else {
        Err(stdout + &stderr)
    }
}

#[tauri::command]
pub fn adb_pair(adb_path: String, ip_port: String, code: String) -> Result<String, String> {
    let adb = adb_bin(&adb_path);
    let out = Command::new(&adb)
        .args(["pair", &ip_port, &code])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = stdout + &stderr;
    if out.status.success() || combined.to_lowercase().contains("success") {
        Ok(combined)
    } else {
        Err(combined)
    }
}

#[tauri::command]
pub fn adb_connect(adb_path: String, ip_port: String) -> Result<String, String> {
    let adb = adb_bin(&adb_path);
    let out = Command::new(&adb)
        .args(["connect", &ip_port])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = stdout + &stderr;
    if out.status.success() || combined.to_lowercase().contains("connected") {
        Ok(combined)
    } else {
        Err(combined)
    }
}

#[tauri::command]
pub fn extract_build_ids_pc(load_dir: String, _title_id: String) -> Result<Vec<String>, String> {
    let load_path = std::path::PathBuf::from(&load_dir);
    // Log is typically at the sibling `log/` directory next to `load/`
    let base = load_path.parent().unwrap_or(load_path.as_path());
    let candidates = [
        base.join("log/eden_log.txt"),
        base.join("log/eden_log.txt.old.txt"),
        base.join("eden_log.txt"),
    ];
    for log_path in &candidates {
        if let Ok(text) = std::fs::read_to_string(log_path) {
            let ids = parse_build_ids(&text);
            if !ids.is_empty() {
                return Ok(ids);
            }
        }
    }
    Err("No Eden log found with build IDs. Launch Eden and run a game first.".to_string())
}

pub fn parse_build_ids(text: &str) -> Vec<String> {
    let re = loader_build_id_re();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in re.captures_iter(text) {
        let full = cap[1].to_string();
        let bid = full[..16.min(full.len())].to_uppercase();
        if seen.insert(bid.clone()) {
            out.push(bid);
        }
    }
    out
}

#[tauri::command]
pub fn adb_ls(adb_path: String, remote_path: String) -> Result<Vec<String>, String> {
    let adb = adb_bin(&adb_path);
    // Quote the path so names with spaces are treated as a single argument by the Android shell.
    let out = Command::new(&adb)
        .args(["shell", &format!("ls '{}'", remote_path)])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| strip_ansi(l.trim()))
            .filter(|l| !l.is_empty())
            .collect())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Strip ANSI escape sequences (e.g. colour codes) that some Android `ls` versions emit.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip everything up to and including the terminating letter (e.g. 'm').
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn adb_push_internal(adb: &str, local: &str, remote: &str) -> Result<(), String> {
    let out = Command::new(adb)
        .args(["push", local, remote])
        .output()
        .map_err(|e| format!("ADB error: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

pub fn adb_mkdir(adb: &str, remote_path: &str) -> Result<(), String> {
    let out = Command::new(adb)
        .args(["shell", &format!("mkdir -p '{}'", remote_path)])
        .output()
        .map_err(|e| format!("ADB mkdir error: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}
