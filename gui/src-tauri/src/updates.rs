use reqwest::{Client, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::AppHandle;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/ChrisA95G/eden-cheats-manager/releases/latest";
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    version: String,
    notes: String,
    release_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    current_version: String,
    available: Option<AvailableUpdate>,
}

fn select_release(data: &[u8], current: &Version) -> Result<Option<AvailableUpdate>, String> {
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
    Ok(Some(AvailableUpdate {
        version: version.to_string(),
        notes: release.body.unwrap_or_default(),
        release_url: format!(
            "https://github.com/ChrisA95G/eden-cheats-manager/releases/tag/{}",
            release.tag_name
        ),
    }))
}

async fn fetch_update(
    client: &Client,
    url: &str,
    current: &Version,
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
    select_release(&data, current)
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<UpdateCheck, String> {
    let current = &app.package_info().version;
    let client = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(20))
        .user_agent(format!("eden-cheats-manager/{current}"))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(UpdateCheck {
        current_version: current.to_string(),
        available: fetch_update(&client, LATEST_RELEASE_URL, current).await?,
    })
}

#[cfg(test)]
mod tests;
