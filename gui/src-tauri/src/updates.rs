use reqwest::{Client, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::utils::{config::BundleType, platform::bundle_type};
use tauri::AppHandle;

const RELEASES_API: &str = "https://api.github.com/repos/ChrisA95G/eden-cheats-manager/releases";
const RELEASES_PER_PAGE: usize = 20;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const RELEASES_URL: &str = "https://github.com/ChrisA95G/eden-cheats-manager/releases";

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub mod desktop;

#[cfg(any(target_os = "android", test))]
pub mod android;

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum UpdateProgress {
    Checking,
    Downloading {
        downloaded: u64,
        total: u64,
    },
    Verifying,
    Installing,
    #[cfg(not(target_os = "android"))]
    Restarting,
}

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

impl Release {
    fn update_version(&self, include_beta: bool) -> Result<Option<Version>, String> {
        if self.draft || (self.prerelease && !include_beta) {
            return Ok(None);
        }
        let version = Version::parse(self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name))
            .map_err(|e| format!("Invalid release version: {e}"))?;
        let eligible = if self.prerelease {
            is_beta(&version)
        } else {
            version.pre.is_empty()
        };
        Ok(eligible.then_some(version))
    }

    fn into_update(
        self,
        version: &Version,
        target: UpdateTarget,
    ) -> Result<AvailableUpdate, String> {
        let download = select_download(&self, target.asset_name(version))?;
        Ok(AvailableUpdate {
            version: version.to_string(),
            notes: self.body.unwrap_or_default(),
            release_url: format!("{RELEASES_URL}/tag/{}", self.tag_name),
            download,
        })
    }
}

fn is_beta(version: &Version) -> bool {
    version
        .pre
        .as_str()
        .strip_prefix("beta.")
        .is_some_and(|n| !n.is_empty() && n != "0" && n.bytes().all(|b| b.is_ascii_digit()))
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

fn select_download(release: &Release, name: String) -> Result<UpdateDownload, String> {
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
    let Some(version) = release.update_version(false)? else {
        return Ok(None);
    };
    if !version.cmp_precedence(current).is_gt() {
        return Ok(None);
    }
    release.into_update(&version, target).map(Some)
}

async fn fetch_release_data(client: &Client, url: &str) -> Result<Vec<u8>, String> {
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
    Ok(data)
}

async fn fetch_update(
    client: &Client,
    releases_url: &str,
    current: &Version,
    target: UpdateTarget,
    include_beta: bool,
) -> Result<Option<AvailableUpdate>, String> {
    if !include_beta {
        let data = fetch_release_data(client, &format!("{releases_url}/latest")).await?;
        return select_release(&data, current, target);
    }

    // Android cannot reinstall the beta whose build number this stable APK superseded.
    let beta_floor = option_env!("EDC_ROLLBACK_FROM_BETA").and_then(|v| Version::parse(v).ok());
    let mut newest = current.clone();
    let mut selected = None;
    for page in 1.. {
        let url = format!("{releases_url}?per_page={RELEASES_PER_PAGE}&page={page}");
        let data = fetch_release_data(client, &url).await?;
        let releases: Vec<Release> = serde_json::from_slice(&data)
            .map_err(|e| format!("Could not read GitHub release information: {e}"))?;
        let last_page = releases.len() < RELEASES_PER_PAGE;
        for release in releases {
            let Ok(Some(version)) = release.update_version(true) else {
                continue;
            };
            if release.prerelease
                && beta_floor
                    .as_ref()
                    .is_some_and(|floor| !version.cmp_precedence(floor).is_gt())
            {
                continue;
            }
            if version.cmp_precedence(&newest).is_gt() {
                newest = version;
                selected = Some(release);
            }
        }
        if last_page {
            break;
        }
    }
    selected
        .map(|release| release.into_update(&newest, target))
        .transpose()
}

async fn find_stable(
    client: &Client,
    current: &Version,
    target: UpdateTarget,
) -> Result<AvailableUpdate, String> {
    if !is_beta(current) {
        return Err("Return to stable is only available on a beta installation.".into());
    }
    let data = fetch_release_data(client, &format!("{RELEASES_API}/latest")).await?;
    let stable: Release = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    let version = stable
        .update_version(false)?
        .ok_or("No published stable release is available.")?;
    if target != UpdateTarget::Android || version.cmp_precedence(current).is_gt() {
        return stable.into_update(&version, target);
    }

    let tag = format!("v{current}");
    let data = fetch_release_data(client, &format!("{RELEASES_API}/tags/{tag}")).await?;
    let beta: Release = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if beta.tag_name != tag || !beta.prerelease || beta.draft {
        return Err("The installed beta has no published return package.".into());
    }
    // The beta and its release workflow use the same pinned stable version.
    let package: serde_json::Value =
        serde_json::from_str(include_str!("../../package.json")).map_err(|e| e.to_string())?;
    let pinned = package["stableReturnVersion"]
        .as_str()
        .and_then(|s| Version::parse(s).ok())
        .filter(|v| v.pre.is_empty() && v.build.is_empty() && *v <= version)
        .ok_or("This beta has no compatible stable APK.")?;
    android_return_package(&beta, &pinned)
}

fn android_return_package(beta: &Release, version: &Version) -> Result<AvailableUpdate, String> {
    Ok(AvailableUpdate {
        download: select_download(beta, format!("eden-cheats-manager-stable-{version}.apk"))?,
        release_url: format!("{RELEASES_URL}/tag/v{version}"),
        version: version.to_string(),
        notes: String::new(),
    })
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    return_to_stable: Option<bool>,
) -> Result<UpdateCheck, String> {
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
    let available = if return_to_stable == Some(true) {
        Some(find_stable(&client, current, target).await?)
    } else {
        fetch_update(
            &client,
            RELEASES_API,
            current,
            target,
            crate::settings::get_settings(app.clone()).include_beta_updates,
        )
        .await?
    };
    Ok(UpdateCheck {
        current_version: current.to_string(),
        available,
    })
}

#[cfg(test)]
mod tests;
