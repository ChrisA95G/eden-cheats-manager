use super::*;
use crate::updates::UpdateDownload;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tauri::test::{mock_builder, mock_context, noop_assets};

const BYTES: &[u8] = b"signed test update\n";
// Disposable test key; no private key or real release data is checked in.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDY4NzA3MkI2M0EwNDNCRjgKUldUNE93UTZ0bkp3YUlWWnRGcEYzSG9YeStZUENOem1jNlhVVXJiNTFicEtzZTN2MEg5NFNNcVIK";
const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUNE93UTZ0bkp3YU5hSWFFL1lyNU9hbmc4Y0UwR2ErS2RoZmNyVnZMV3grLzVHYnZmLzNjemt3dFhlYjRwN1JrUFlyK3NYcTJJN1RhcUJ1dEcxcC9OckIzRTNUdll6TkE0PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0MDgwMDk1CWZpbGU6Zml4dHVyZS5BcHBJbWFnZQp3QU1tZEwrSXBKQnNtdndobnZlU0pUcmdxQmNMMnlwY0NHYnUzSWxPb1hGRCt1L2ord0FZb0RqMWdSZ3pETm9LTUlnK3g4MVpLMWpuRCtXazVlS3FEUT09Cg==";

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(case: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut manifest = json!({"version":"0.3.0", "platforms":{
            "linux-x86_64-appimage":{"url":format!("{url}/update.AppImage"),"signature":SIGNATURE}
        }});
        match case {
            "version changed" => manifest["version"] = json!("0.4.0"),
            "different installer" => {
                manifest["platforms"]["linux-x86_64-appimage"]["url"] =
                    json!(format!("{url}/other.AppImage"))
            }
            "invalid signature" => {
                manifest["platforms"]["linux-x86_64-appimage"]["signature"] = json!("invalid")
            }
            "no update" => manifest["version"] = json!("0.2.1"),
            _ => {}
        }
        let manifest = manifest.to_string();
        let mut bytes = BYTES.to_vec();
        if case == "altered bytes" {
            bytes[0] ^= 1;
        }
        let failed_download = case == "download failed";
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let (server_stop, server_requests) = (stop.clone(), requests.clone());
        let thread = std::thread::spawn(move || {
            while !server_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                server_requests.fetch_add(1, Ordering::Relaxed);
                let is_manifest = request[..count].starts_with(b"GET /latest.json ");
                let body = if is_manifest {
                    manifest.as_bytes()
                } else {
                    &bytes
                };
                let status = if !is_manifest && failed_download {
                    500
                } else {
                    200
                };
                let header = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream
                    .write_all(header.as_bytes())
                    .and_then(|_| stream.write_all(body));
            }
        });
        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn desktop_updates_verify_before_installing_and_allow_retry_after_failure() {
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build({
            let mut context = mock_context(noop_assets());
            context.package_info_mut().version = "0.2.1".parse().unwrap();
            context
                .config_mut()
                .plugins
                .0
                .insert("updater".into(), json!({"pubkey": PUBLIC_KEY}));
            context
        })
        .unwrap();
    for case in [
        "version changed",
        "different installer",
        "no update",
        "altered bytes",
        "invalid signature",
        "wrong size",
        "download failed",
        "success",
    ] {
        let server = Server::new(case);
        let directory = tempfile::Builder::new()
            .prefix("codex-install-test-")
            .tempdir()
            .unwrap();
        let executable = directory.path().join("installed.AppImage");
        let settings = directory.path().join("settings.json");
        std::fs::write(&executable, b"original app").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(&settings, b"retained settings").unwrap();
        let selected = AvailableUpdate {
            version: "0.3.0".into(),
            notes: String::new(),
            release_url: String::new(),
            download: UpdateDownload {
                name: "update.AppImage".into(),
                url: format!("{}/update.AppImage", server.url),
                size: BYTES.len() as u64 + u64::from(case == "wrong size"),
            },
        };
        let build_updater = || {
            app.updater_builder()
                .target("linux-x86_64-appimage")
                .endpoints(vec![format!("{}/latest.json", server.url).parse().unwrap()])
                .unwrap()
                .executable_path(&executable)
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap()
        };
        if case == "success" {
            let guard = UPDATE_LOCK.try_lock().unwrap();
            let result = tauri::async_runtime::block_on(download_and_install(
                build_updater(),
                &selected,
                |_| {},
            ));
            assert!(result.unwrap_err().contains("already in progress"));
            assert_eq!(server.requests.load(Ordering::Relaxed), 0);
            drop(guard);
        }
        let events = std::sync::Mutex::new(Vec::new());
        let result = tauri::async_runtime::block_on(download_and_install(
            build_updater(),
            &selected,
            |event| {
                events
                    .lock()
                    .unwrap()
                    .push(serde_json::to_value(event).unwrap());
            },
        ));
        let events = events.into_inner().unwrap();
        assert_eq!(events[0]["event"], "checking");
        let installed = std::fs::read(&executable).unwrap();
        if case == "success" {
            result.unwrap();
            assert_eq!(installed, BYTES);
            assert_eq!(events.last().unwrap()["event"], "installing");
            assert!(events.iter().any(|event| event["event"] == "verifying"));
            assert!(events.iter().any(|event| event
                == &json!({"event":"downloading","downloaded":BYTES.len(),"total":BYTES.len()})));
            assert_eq!(
                std::fs::metadata(&executable).unwrap().permissions().mode() & 0o777,
                0o755
            );
        } else {
            assert!(result.is_err(), "{case}");
            assert_eq!(installed, b"original app", "{case}");
            assert!(
                !events.iter().any(|event| event["event"] == "installing"),
                "{case}"
            );
        }
        if matches!(
            case,
            "version changed" | "different installer" | "no update"
        ) {
            assert_eq!(server.requests.load(Ordering::Relaxed), 1, "{case}");
        }
        assert_eq!(std::fs::read(settings).unwrap(), b"retained settings");
    }
}
