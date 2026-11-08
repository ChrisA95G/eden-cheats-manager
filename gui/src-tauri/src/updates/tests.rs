use super::*;
use serde_json::json;

fn release(tag: &str) -> serde_json::Value {
    json!({"tag_name": tag, "draft": false, "prerelease": false, "body": "Release notes",
        "assets": [{"name": "eden-cheats-manager.apk", "state": "uploaded", "size": 12345}]})
}

fn select(data: serde_json::Value, current: &str) -> Result<Option<AvailableUpdate>, String> {
    select_release(
        &serde_json::to_vec(&data).unwrap(),
        &Version::parse(current).unwrap(),
        UpdateTarget::Android,
    )
}

#[test]
fn newer_release_returns_normalized_version_notes_and_project_url() {
    for tag in ["v0.10.0", "0.10.0"] {
        let update = select(release(tag), "0.2.1").unwrap().unwrap();
        assert_eq!(update.version, "0.10.0");
        assert_eq!(update.notes, "Release notes");
        assert_eq!(
            update.release_url,
            format!("https://github.com/ChrisA95G/eden-cheats-manager/releases/tag/{tag}")
        );
        assert_eq!(
            serde_json::to_value(UpdateCheck {
                current_version: "0.2.1".into(),
                available: Some(update)
            })
            .unwrap()["available"]["releaseUrl"],
            format!("https://github.com/ChrisA95G/eden-cheats-manager/releases/tag/{tag}")
        );
    }
}

#[test]
fn same_older_and_build_metadata_only_changes_do_not_offer_updates() {
    for (tag, current) in [
        ("v0.2.1", "0.2.1"),
        ("v0.2.0", "0.2.1"),
        ("v0.2.1", "0.10.0"),
        ("v0.2.1+new", "0.2.1"),
        ("v0.2.1+zzz", "0.2.1+aaa"),
    ] {
        assert!(
            select(release(tag), current).unwrap().is_none(),
            "{tag} vs {current}"
        );
    }
}

#[test]
fn drafts_and_prereleases_are_ignored_even_if_the_tag_looks_stable() {
    for flag in ["draft", "prerelease"] {
        let mut data = release("v0.3.0");
        data[flag] = json!(true);
        data["assets"] = json!([]);
        assert!(select(data, "0.2.1").unwrap().is_none());
    }
    assert!(select(release("v0.3.0-rc.1"), "0.2.1").unwrap().is_none());
    assert!(select(release("v0.3.0"), "0.3.0-rc.1").unwrap().is_some());
}

#[test]
fn release_without_notes_is_valid() {
    let mut data = release("v0.3.0");
    data["body"] = serde_json::Value::Null;
    assert_eq!(select(data.clone(), "0.2.1").unwrap().unwrap().notes, "");
    data.as_object_mut().unwrap().remove("body");
    assert_eq!(select(data, "0.2.1").unwrap().unwrap().notes, "");
}

#[test]
fn malformed_metadata_is_an_error_rather_than_no_update() {
    for data in [
        json!({}),
        json!({"message": "API rate limit exceeded"}),
        release("latest"),
        release("v0.3"),
        release("v0.3.0/other"),
    ] {
        assert!(select(data, "0.2.1").is_err());
    }
    assert!(select_release(b"not JSON", &Version::new(0, 2, 1), UpdateTarget::Android).is_err());
}

#[test]
fn installation_mapping_preserves_the_installer_and_architecture() {
    for arch in ["arm", "aarch64", "x86", "x86_64"] {
        assert_eq!(
            UpdateTarget::for_installation("android", arch, None).unwrap(),
            UpdateTarget::Android
        );
    }
    for (os, bundle, expected) in [
        ("windows", BundleType::Nsis, UpdateTarget::WindowsNsis),
        ("windows", BundleType::Msi, UpdateTarget::WindowsMsi),
        ("linux", BundleType::AppImage, UpdateTarget::LinuxAppImage),
    ] {
        assert_eq!(
            UpdateTarget::for_installation(os, "x86_64", Some(bundle)).unwrap(),
            expected
        );
    }
    for (os, arch, bundle) in [
        ("windows", "aarch64", Some(BundleType::Nsis)),
        ("windows", "x86", Some(BundleType::Msi)),
        ("windows", "x86_64", None),
        ("linux", "aarch64", Some(BundleType::AppImage)),
        ("linux", "x86_64", Some(BundleType::Deb)),
        ("linux", "x86_64", Some(BundleType::Rpm)),
        ("linux", "x86_64", None),
        ("macos", "aarch64", Some(BundleType::App)),
        ("android", "riscv64", None),
    ] {
        assert!(UpdateTarget::for_installation(os, arch, bundle).is_err());
    }
}

#[test]
fn each_target_selects_its_own_file_from_a_mixed_release() {
    let files = [
        (UpdateTarget::Android, "eden-cheats-manager.apk"),
        (
            UpdateTarget::WindowsNsis,
            "Eden.Cheats.Manager_0.2.1_x64-setup.exe",
        ),
        (
            UpdateTarget::WindowsMsi,
            "Eden.Cheats.Manager_0.2.1_x64_en-US.msi",
        ),
        (
            UpdateTarget::LinuxAppImage,
            "Eden.Cheats.Manager_0.2.1_amd64.AppImage",
        ),
    ];
    let mut data = release("v0.2.1");
    data["assets"] = json!(files
        .iter()
        .map(|(_, name)| json!({
            "name": name, "state": "uploaded", "size": 12345,
            "browser_download_url": "https://untrusted.example/other.exe"
        }))
        .collect::<Vec<_>>());
    data["assets"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"SHA256SUMS","state":"uploaded","size":409}));
    for (target, name) in files {
        let update = select_release(
            &serde_json::to_vec(&data).unwrap(),
            &Version::new(0, 2, 0),
            target,
        )
        .unwrap()
        .unwrap();
        let download = serde_json::to_value(update).unwrap()["download"].clone();
        assert_eq!(
            download,
            json!({"name": name, "size": 12345,
            "url": format!("{RELEASES_URL}/download/v0.2.1/{name}")})
        );
    }
}

#[test]
fn missing_duplicate_and_unfinished_downloads_are_errors() {
    let valid = release("v0.3.0")["assets"][0].clone();
    for assets in [json!([]), json!([valid.clone(), valid.clone()])] {
        let mut data = release("v0.3.0");
        data["assets"] = assets;
        assert!(select(data, "0.2.1").is_err());
    }
    for (field, value) in [
        ("size", json!(0)),
        ("size", json!(-1)),
        ("state", json!("starter")),
        ("name", json!("other.apk")),
    ] {
        let mut data = release("v0.3.0");
        data["assets"][0][field] = value;
        assert!(select(data, "0.2.1").is_err());
    }
    let mut data = release("v0.3.0");
    data.as_object_mut().unwrap().remove("assets");
    assert!(fetch_response(200, &data.to_string()).is_err());
    assert!(select(data.clone(), "0.2.1").is_err());
    assert!(select(data, "0.3.0").unwrap().is_none());
}

#[test]
fn a_different_version_or_architecture_cannot_substitute_for_a_missing_installer() {
    for name in [
        "Eden.Cheats.Manager_0.2.1_x64-setup.exe",
        "Eden.Cheats.Manager_0.3.0_arm64-setup.exe",
        "Eden.Cheats.Manager_0.3.0_x64_en-US.msi",
    ] {
        let mut data = release("v0.3.0");
        data["assets"][0]["name"] = json!(name);
        assert!(select_release(
            &serde_json::to_vec(&data).unwrap(),
            &Version::new(0, 2, 1),
            UpdateTarget::WindowsNsis
        )
        .is_err());
    }
}

fn fetch_response(status: u16, body: &str) -> Result<Option<AvailableUpdate>, String> {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/latest", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).unwrap();
        let _ = stream.write_all(response.as_bytes());
    });
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let result = tauri::async_runtime::block_on(fetch_update(
        &client,
        &url,
        &Version::new(0, 2, 1),
        UpdateTarget::Android,
    ));
    server.join().unwrap();
    result
}

#[test]
fn successful_http_response_is_checked_for_a_newer_release() {
    let update = fetch_response(200, &release("v0.3.0").to_string())
        .unwrap()
        .unwrap();
    assert_eq!(update.version, "0.3.0");
    assert!(fetch_response(200, &release("v0.2.1").to_string())
        .unwrap()
        .is_none());
}

#[test]
fn failed_http_checks_are_not_reported_as_up_to_date() {
    for status in [403, 404, 429, 500] {
        assert!(fetch_response(status, "{}").is_err(), "HTTP {status}");
    }
    assert!(fetch_response(200, "<html>Service unavailable</html>").is_err());
}

#[test]
fn oversized_responses_are_rejected_before_parsing() {
    let result = fetch_response(200, &" ".repeat(MAX_RESPONSE_BYTES + 1));
    assert!(result.unwrap_err().contains("size limit"));
}
