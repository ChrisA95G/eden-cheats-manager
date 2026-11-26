use crate::updates::is_beta;
use reqwest::{redirect::Policy, Client, StatusCode, Url};
use semver::Version;
use std::{ffi::OsStr, time::Duration};
use tauri::AppHandle;

const BETA_INBOX: &str = "https://edc-feedback-beta.saluix.workers.dev/v1/feedback";

fn resolve_endpoint(version: &Version, configured: Option<&OsStr>) -> Option<Url> {
    let address = match configured {
        Some(value) => value.to_str()?.trim(),
        None if is_beta(version) => BETA_INBOX,
        None => return None,
    };
    let url = Url::parse(address).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

fn endpoint(app: &AppHandle) -> Option<Url> {
    resolve_endpoint(
        &app.package_info().version,
        std::env::var_os("EDC_FEEDBACK_URL").as_deref(),
    )
}

#[tauri::command]
pub fn feedback_enabled(app: AppHandle) -> bool {
    endpoint(&app).is_some()
}

#[tauri::command]
pub async fn send_feedback(app: AppHandle, message: String) -> Result<(), String> {
    let url = endpoint(&app).ok_or("The feedback inbox is not configured.")?;
    if message.trim().is_empty() || message.len() > 4096 {
        return Err("Enter a message of up to 4,096 UTF-8 bytes.".into());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "Could not start the feedback request.")?;
    let response = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message)
        .send()
        .await
        .map_err(|_| "Could not confirm delivery. Your message is still here.")?;
    match response.status() {
        StatusCode::CREATED => Ok(()),
        StatusCode::PAYLOAD_TOO_LARGE => Err("Shorten your message to 4,096 UTF-8 bytes.".into()),
        _ => Err("The inbox did not confirm delivery. Your message is still here.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_beta_build_defaults_to_the_live_inbox() {
        let beta = Version::parse("0.3.0-beta.1").unwrap();
        assert_eq!(resolve_endpoint(&beta, None).unwrap().as_str(), BETA_INBOX);
        for version in ["0.3.0", "0.3.0-rc.1"] {
            assert!(resolve_endpoint(&Version::parse(version).unwrap(), None).is_none());
        }
    }

    #[test]
    fn explicit_configuration_takes_precedence_even_when_invalid() {
        let local = "http://127.0.0.1:8787/v1/feedback";
        for version in ["0.3.0-beta.1", "0.3.0"] {
            let version = Version::parse(version).unwrap();
            let url = resolve_endpoint(&version, Some(OsStr::new(local))).unwrap();
            assert_eq!(url.as_str(), local);
            for invalid in ["", "invalid", "file:///tmp/feedback"] {
                assert!(resolve_endpoint(&version, Some(OsStr::new(invalid))).is_none());
            }
        }
    }
}
