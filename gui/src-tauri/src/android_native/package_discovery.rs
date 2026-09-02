use crate::package_metadata::PackageMetadata;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "android")]
use super::jni::{
    jni_noarg_int_call, jni_noarg_string_call, jni_noarg_void_call, parse_saf_response,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDiscoveryStatus {
    pub prod_keys_selected: bool,
    pub prod_keys_name: String,
    pub prod_keys_readable: bool,
    pub prod_keys_seekable: bool,
    pub package_selected: bool,
    pub package_name: String,
    pub package_readable: bool,
    pub package_seekable: bool,
    pub ready: bool,
    pub message: String,
}

#[tauri::command]
pub fn get_package_discovery_status() -> Result<PackageDiscoveryStatus, String> {
    #[cfg(target_os = "android")]
    {
        let response = parse_saf_response(jni_noarg_string_call("getPackageDiscoveryStatus")?)?;
        return serde_json::from_str(&response)
            .map_err(|error| format!("Invalid package discovery status: {error}"));
    }

    #[cfg(not(target_os = "android"))]
    {
        Err("Native package discovery is only available on Android.".into())
    }
}

#[tauri::command]
pub fn select_prod_keys_document() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return jni_noarg_void_call("selectProdKeysDocument");
    }

    #[cfg(not(target_os = "android"))]
    {
        Err("Native package discovery is only available on Android.".into())
    }
}

#[tauri::command]
pub fn select_game_package_document() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return jni_noarg_void_call("selectGamePackageDocument");
    }

    #[cfg(not(target_os = "android"))]
    {
        Err("Native package discovery is only available on Android.".into())
    }
}

#[tauri::command]
pub async fn discover_package_metadata() -> Result<PackageMetadata, String> {
    #[cfg(target_os = "android")]
    {
        let prod_keys = file_from_jni_fd("openProdKeysReadFd", "prod.keys")?;
        let package = file_from_jni_fd("openGamePackageReadFd", "game package")?;
        return tauri::async_runtime::spawn_blocking(move || {
            crate::package_metadata::discover_package_metadata(prod_keys, package)
        })
        .await
        .map_err(|error| format!("Package parser task failed: {error}"))?;
    }

    #[cfg(not(target_os = "android"))]
    {
        Err("Native package discovery is only available on Android.".into())
    }
}

#[cfg(target_os = "android")]
fn file_from_jni_fd(method: &str, label: &str) -> Result<std::fs::File, String> {
    use std::os::fd::{FromRawFd, OwnedFd};

    let fd = jni_noarg_int_call(method)?;
    if fd < 0 {
        let reason = match fd {
            -1 => "has not been selected",
            -2 => "no longer has a persisted read grant",
            -3 => "could not be opened",
            -4 => "is not seekable; select a local file",
            _ => "could not be opened",
        };
        return Err(format!("The selected {label} {reason}."));
    }

    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok(std::fs::File::from(owned))
}
