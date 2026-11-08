use super::{AvailableUpdate, UpdateProgress, MAX_RESPONSE_BYTES};
use reqwest::blocking::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tauri::async_runtime::Mutex;

static UPDATE_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AndroidInstallResult {
    PermissionRequired,
    InstallerOpened,
}

fn apk_checksum(data: &str) -> Result<[u8; 32], String> {
    let mut matches = data.lines().filter_map(|line| {
        line.split_once("  ")
            .filter(|(_, name)| *name == "eden-cheats-manager.apk")
            .map(|(hash, _)| hash)
    });
    let hash = matches
        .next()
        .ok_or("The release is missing the APK checksum.")?;
    if matches.next().is_some() {
        return Err("The release contains multiple APK checksums.".into());
    }
    let mut digest = [0; 32];
    hex::decode_to_slice(hash, &mut digest).map_err(|_| "The APK checksum is invalid.")?;
    Ok(digest)
}

fn copy_apk(
    mut source: impl Read,
    mut destination: impl Write,
    size: u64,
    progress: impl Fn(u64),
) -> Result<[u8; 32], String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut downloaded = 0;
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|e| format!("Could not read the APK: {e}"))?;
        if count == 0 {
            break;
        }
        downloaded += count as u64;
        if downloaded > size {
            return Err("The APK exceeds the release file size.".into());
        }
        destination
            .write_all(&buffer[..count])
            .map_err(|e| format!("Could not save the APK: {e}"))?;
        hash.update(&buffer[..count]);
        progress(downloaded);
    }
    if downloaded != size {
        return Err("The APK download is incomplete.".into());
    }
    Ok(hash.finalize().into())
}

fn download_and_install(
    client: &Client,
    selected: &AvailableUpdate,
    destination: &Path,
    report: impl Fn(UpdateProgress),
    install: impl FnOnce(&str) -> Result<String, String>,
) -> Result<AndroidInstallResult, String> {
    let _guard = UPDATE_LOCK
        .try_lock()
        .map_err(|_| "An app update is already in progress.")?;
    let url: reqwest::Url = selected
        .download
        .url
        .parse()
        .map_err(|e| format!("Invalid update URL: {e}"))?;
    let checksum_url = url.join("SHA256SUMS").map_err(|e| e.to_string())?;
    let response = client
        .get(checksum_url)
        .timeout(Duration::from_secs(20))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Could not download release checksums: {e}"))?;
    let mut data = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("Could not read release checksums: {e}"))?;
    if data.len() > MAX_RESPONSE_BYTES {
        return Err("Release checksums exceeded the size limit.".into());
    }
    let checksum = apk_checksum(
        std::str::from_utf8(&data).map_err(|_| "Release checksums are not valid text.")?,
    )?;
    let cached = File::open(destination).ok().and_then(|file| {
        report(UpdateProgress::Verifying);
        copy_apk(file, std::io::sink(), selected.download.size, |_| {}).ok()
    }) == Some(checksum);
    if !cached {
        report(UpdateProgress::Downloading {
            downloaded: 0,
            total: selected.download.size,
        });
        let response = client
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|e| format!("Could not download the APK: {e}"))?;
        let directory = destination
            .parent()
            .ok_or("The update cache path is invalid.")?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)
            .map_err(|e| format!("Could not create an APK download file: {e}"))?;
        let digest = copy_apk(
            response,
            &mut temporary,
            selected.download.size,
            |downloaded| {
                report(UpdateProgress::Downloading {
                    downloaded,
                    total: selected.download.size,
                });
            },
        )?;
        report(UpdateProgress::Verifying);
        if digest != checksum {
            return Err("The APK checksum does not match the release.".into());
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| format!("Could not finish saving the APK: {e}"))?;
        temporary
            .persist(destination)
            .map_err(|e| format!("Could not save the verified APK: {e}"))?;
    }
    report(UpdateProgress::Installing);
    match install(&selected.version)?.as_str() {
        "permissionRequired" => Ok(AndroidInstallResult::PermissionRequired),
        "installerOpened" => Ok(AndroidInstallResult::InstallerOpened),
        _ => Err("Unexpected response from the Android update installer.".into()),
    }
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn install_android_update(
    app: tauri::AppHandle,
    version: String,
    on_progress: tauri::ipc::Channel<UpdateProgress>,
) -> Result<AndroidInstallResult, String> {
    let _ = on_progress.send(UpdateProgress::Checking);
    let selected = super::check_app_update(app)
        .await?
        .available
        .ok_or("No newer stable update is available.")?;
    if selected.version != version {
        return Err("The available release has changed. Check for updates again.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let destination = crate::android_native::app_update_download_path()?;
        let client = Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(600))
            .user_agent(concat!("eden-cheats-manager/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        download_and_install(
            &client,
            &selected,
            &destination,
            |event| {
                let _ = on_progress.send(event);
            },
            crate::android_native::install_app_update,
        )
    })
    .await
    .map_err(|e| format!("The Android updater stopped unexpectedly: {e}"))?
}

#[cfg(test)]
mod tests;
