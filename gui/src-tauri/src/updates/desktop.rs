use super::{check_app_update, AvailableUpdate, UpdateTarget};
use serde::Serialize;
use std::time::Duration;
use tauri::{async_runtime::Mutex, ipc::Channel, AppHandle};
use tauri_plugin_updater::{Updater, UpdaterExt};

static UPDATE_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum UpdateProgress {
    Checking,
    Downloading { downloaded: u64, total: u64 },
    Verifying,
    Installing,
    Restarting,
}

impl UpdateTarget {
    fn updater_target(self) -> Result<&'static str, String> {
        match self {
            Self::WindowsNsis => Ok("windows-x86_64-nsis"),
            Self::WindowsMsi => Ok("windows-x86_64-msi"),
            Self::LinuxAppImage => Ok("linux-x86_64-appimage"),
            Self::Android => Err("Desktop updates are unavailable on Android.".into()),
        }
    }
}

async fn download_and_install(
    updater: Updater,
    selected: &AvailableUpdate,
    report: impl Fn(UpdateProgress) + Send + Sync,
) -> Result<(), String> {
    let _guard = UPDATE_LOCK
        .try_lock()
        .map_err(|_| "An app update is already in progress.".to_string())?;
    report(UpdateProgress::Checking);
    let mut update = updater
        .check()
        .await
        .map_err(|e| format!("Could not read update metadata: {e}"))?
        .ok_or("The selected update is no longer available. Check for updates again.")?;
    if update.version != selected.version || update.download_url.as_str() != selected.download.url {
        return Err(
            "Update metadata does not match the selected release. Check for updates again.".into(),
        );
    }

    update.timeout = Some(Duration::from_secs(600));
    let total = selected.download.size;
    let mut downloaded = 0;
    report(UpdateProgress::Downloading { downloaded, total });
    let bytes = update
        .download(
            |length, _| {
                downloaded += length as u64;
                report(UpdateProgress::Downloading { downloaded, total });
            },
            || report(UpdateProgress::Verifying),
        )
        .await
        .map_err(|e| format!("Could not download and verify the update: {e}"))?;
    if bytes.len() as u64 != total {
        return Err("The update download size does not match the release.".into());
    }
    report(UpdateProgress::Installing);
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| format!("The update installer stopped unexpectedly: {e}"))?
        .map_err(|e| format!("Could not install the update: {e}"))
}

#[tauri::command]
pub async fn install_desktop_update(
    app: AppHandle,
    version: String,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    let target = UpdateTarget::for_installation(
        std::env::consts::OS,
        std::env::consts::ARCH,
        tauri::utils::platform::bundle_type(),
    )?;
    let selected = check_app_update(app.clone())
        .await?
        .available
        .ok_or("No newer stable update is available.")?;
    if selected.version != version {
        return Err("The available release has changed. Check for updates again.".into());
    }
    let download_url: reqwest::Url = selected
        .download
        .url
        .parse()
        .map_err(|e| format!("Invalid update URL: {e}"))?;
    // Pin the manifest to the same release the user selected, even if latest changes.
    let endpoint = download_url
        .join("latest.json")
        .map_err(|e| e.to_string())?;
    let updater = app
        .updater_builder()
        .target(target.updater_target()?)
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .timeout(Duration::from_secs(20))
        .configure_client(|client| {
            client
                .https_only(true)
                .connect_timeout(Duration::from_secs(20))
        })
        .build()
        .map_err(|e| e.to_string())?;
    download_and_install(updater, &selected, |event| {
        let _ = on_progress.send(event);
    })
    .await?;
    #[cfg(target_os = "linux")]
    {
        let _ = on_progress.send(UpdateProgress::Restarting);
        app.restart();
    }
    #[cfg(target_os = "windows")]
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
