use reqwest::{Client, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::utils::{config::BundleType, platform::bundle_type};
use tauri::AppHandle;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/ChrisA95G/eden-cheats-manager/releases/latest";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const RELEASES_URL: &str = "https://github.com/ChrisA95G/eden-cheats-manager/releases";

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub mod desktop;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateTarget {
    Android,
    WindowsNsis,
    WindowsMsi,
    LinuxAppImage,
}

impl UpdateTarget {
    fn for_installation(os: &str, arch: &str, bundle: Option<BundleType>) -> Result<Self, String> {
        match (os, arch, &bundle) {
            ("android", "arm" | "aarch64" | "x86" | "x86_64", _) => Ok(Self::Android),
            ("windows", "x86_64", Some(BundleType::Nsis)) => Ok(Self::WindowsNsis),
            ("windows", "x86_64", Some(BundleType::Msi)) => Ok(Self::WindowsMsi),
            ("linux", "x86_64", Some(BundleType::AppImage)) => Ok(Self::LinuxAppImage),
            _ => Err(format!(
                "App updates are unavailable for this installation ({os}/{arch}, {}).",
                bundle.map_or_else(|| "unknown package type".into(), |value| value.to_string())
            )),
        }
    }

    fn asset_name(self, version: &Version) -> String {
        let suffix = match self {
            Self::Android => return "eden-cheats-manager.apk".into(),
            Self::WindowsNsis => "x64-setup.exe",
            Self::WindowsMsi => "x64_en-US.msi",
            Self::LinuxAppImage => "amd64.AppImage",
        };
        format!("Eden.Cheats.Manager_{version}_{suffix}")
    }
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    size: u64,
    state: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Serialize)]
pub struct UpdateDownload {
    name: String,
    url: String,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    version: String,
    notes: String,
    release_url: String,
    download: UpdateDownload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    current_version: String,
    available: Option<AvailableUpdate>,
}

fn select_download(
    release: &Release,
    version: &Version,
    target: UpdateTarget,
) -> Result<UpdateDownload, String> {
    let name = target.asset_name(version);
    let mut matches = release.assets.iter().filter(|asset| asset.name == name);
    let asset = matches
        .next()
        .ok_or_else(|| format!("The release is missing {name}."))?;
    if matches.next().is_some() {
        return Err(format!("The release contains multiple files named {name}."));
    }
    if asset.state != "uploaded" || asset.size == 0 {
        return Err(format!("The release file {name} is not ready to download."));
    }
    // Use the project's release path, never a URL supplied in asset metadata.
    let url = format!("{RELEASES_URL}/download/{}/{name}", release.tag_name);
    Ok(UpdateDownload {
        name,
        url,
        size: asset.size,
    })
}

fn select_release(
    data: &[u8],
    current: &Version,
    target: UpdateTarget,
) -> Result<Option<AvailableUpdate>, String> {
    let release: Release = serde_json::from_slice(data)
        .map_err(|e| format!("Could not read GitHub release information: {e}"))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|e| format!("Invalid release version: {e}"))?;
    if !version.pre.is_empty() || !version.cmp_precedence(current).is_gt() {
        return Ok(None);
    }
    let download = select_download(&release, &version, target)?;
    Ok(Some(AvailableUpdate {
        version: version.to_string(),
        notes: release.body.unwrap_or_default(),
        release_url: format!("{RELEASES_URL}/tag/{}", release.tag_name),
        download,
    }))
}

async fn fetch_update(
    client: &Client,
    url: &str,
    current: &Version,
    target: UpdateTarget,
) -> Result<Option<AvailableUpdate>, String> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Could not reach GitHub to check for updates: {e}"))?;
    if matches!(
        response.status(),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) {
        return Err("GitHub refused the update check. Please try again later.".into());
    }
    response = response
        .error_for_status()
        .map_err(|e| format!("Update check failed: {e}"))?;
    let mut data = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Could not read the update response: {e}"))?
    {
        if data.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("GitHub release information exceeded the size limit.".into());
        }
        data.extend_from_slice(&chunk);
    }
    select_release(&data, current, target)
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<UpdateCheck, String> {
    let target = UpdateTarget::for_installation(
        std::env::consts::OS,
        std::env::consts::ARCH,
        bundle_type(),
    )?;
    let current = &app.package_info().version;
    let client = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(20))
        .user_agent(format!("eden-cheats-manager/{current}"))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(UpdateCheck {
        current_version: current.to_string(),
        available: fetch_update(&client, LATEST_RELEASE_URL, current, target).await?,
    })
}

#[cfg(test)]
mod tests;
