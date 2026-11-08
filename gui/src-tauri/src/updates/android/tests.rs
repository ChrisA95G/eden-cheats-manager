use super::*;
use crate::updates::UpdateDownload;
use std::{
    cell::RefCell,
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

const APK: &[u8] = b"test APK content\n";

struct Server {
    url: String,
    downloads: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(case: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!(
            "http://{}/v0.3.0/eden-cheats-manager.apk",
            listener.local_addr().unwrap()
        );
        let mut checksums = format!(
            "{}  eden-cheats-manager.apk\n",
            hex::encode(Sha256::digest(APK))
        );
        match case {
            "missing checksum" => {
                checksums = checksums.replace("eden-cheats-manager.apk", "other.apk")
            }
            "duplicate checksum" => checksums = checksums.repeat(2),
            "invalid checksum" => checksums = "bad  eden-cheats-manager.apk\n".into(),
            "oversized checksums" => checksums = "x".repeat(MAX_RESPONSE_BYTES + 1),
            _ => {}
        }
        let mut bytes = APK.to_vec();
        match case {
            "tampered" => bytes[0] ^= 1,
            "oversized" => bytes.push(0),
            "incomplete" => {
                bytes.pop();
            }
            _ => {}
        }
        let stop = Arc::new(AtomicBool::new(false));
        let downloads = Arc::new(AtomicUsize::new(0));
        let (thread_stop, thread_downloads) = (stop.clone(), downloads.clone());
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
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
                let checksum_request = request[..count].starts_with(b"GET /v0.3.0/SHA256SUMS ");
                let body = if checksum_request {
                    checksums.as_bytes()
                } else {
                    assert!(request[..count].starts_with(b"GET /v0.3.0/eden-cheats-manager.apk "));
                    thread_downloads.fetch_add(1, Ordering::Relaxed);
                    &bytes
                };
                let failed = case == "checksum HTTP failure" && checksum_request
                    || case == "APK HTTP failure" && !checksum_request;
                let length = body.len() + usize::from(case == "truncated" && !checksum_request);
                let header = format!(
                    "HTTP/1.1 {} Test\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
                    if failed { 500 } else { 200 }
                );
                let _ = stream
                    .write_all(header.as_bytes())
                    .and_then(|_| stream.write_all(body));
            }
        });
        Self {
            url,
            downloads,
            stop,
            thread: Some(thread),
        }
    }

    fn update(&self) -> AvailableUpdate {
        AvailableUpdate {
            version: "0.3.0".into(),
            notes: String::new(),
            release_url: String::new(),
            download: UpdateDownload {
                name: "eden-cheats-manager.apk".into(),
                url: self.url.clone(),
                size: APK.len() as u64,
            },
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
fn checksum_parser_requires_one_exact_apk_entry() {
    let hash = hex::encode(Sha256::digest(APK));
    assert_eq!(
        apk_checksum(&format!(
            "{hash}  other.apk\r\n{hash}  eden-cheats-manager.apk\r\n"
        ))
        .unwrap(),
        <[u8; 32]>::from(Sha256::digest(APK))
    );
    for data in [
        format!("{hash}  ../eden-cheats-manager.apk"),
        format!("{hash}  eden-cheats-manager.apk.sig"),
        format!("{}  eden-cheats-manager.apk", "g".repeat(64)),
    ] {
        assert!(apk_checksum(&data).is_err());
    }
}

#[test]
fn streaming_checks_size_and_propagates_disk_errors() {
    let bytes = vec![42; 150_000];
    let chunks = RefCell::new(Vec::new());
    let mut saved = Vec::new();
    let digest = copy_apk(bytes.as_slice(), &mut saved, bytes.len() as u64, |count| {
        chunks.borrow_mut().push(count)
    })
    .unwrap();
    assert_eq!(saved, bytes);
    assert_eq!(digest, <[u8; 32]>::from(Sha256::digest(&bytes)));
    assert!(chunks.borrow().len() >= 3);
    struct NoSpace;
    impl Write for NoSpace {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(copy_apk(APK, NoSpace, APK.len() as u64, |_| {})
        .unwrap_err()
        .contains("disk full"));
}

#[test]
fn failed_downloads_preserve_cache_and_retries_only_install_verified_bytes() {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let directory = tempfile::Builder::new()
        .prefix("codex-apk-test-")
        .tempdir()
        .unwrap();
    let destination = directory.path().join("eden-cheats-manager.apk");
    for (case, error) in [
        ("checksum HTTP failure", "download release checksums"),
        ("missing checksum", "missing the APK checksum"),
        ("duplicate checksum", "multiple APK checksums"),
        ("invalid checksum", "checksum is invalid"),
        ("oversized checksums", "exceeded the size limit"),
        ("APK HTTP failure", "download the APK"),
        ("incomplete", "incomplete"),
        ("truncated", "Could not read the APK"),
        ("oversized", "exceeds the release file size"),
        ("tampered", "checksum does not match"),
    ] {
        std::fs::write(&destination, b"previous cached APK").unwrap();
        let server = Server::new(case);
        let result = download_and_install(
            &client,
            &server.update(),
            &destination,
            |event| assert!(!matches!(event, UpdateProgress::Installing)),
            |_| panic!("Installer called for {case}"),
        );
        assert!(result.unwrap_err().contains(error), "{case}");
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"previous cached APK",
            "{case}"
        );
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            1,
            "Partial file left for {case}"
        );
    }
    let server = Server::new("success");
    let selected = server.update();
    {
        let _guard = UPDATE_LOCK.try_lock().unwrap();
        assert!(
            download_and_install(&client, &selected, &destination, |_| {}, |_| unreachable!())
                .unwrap_err()
                .contains("already in progress")
        );
        assert_eq!(server.downloads.load(Ordering::Relaxed), 0);
    }
    for (response, expected) in [
        (
            "permissionRequired",
            AndroidInstallResult::PermissionRequired,
        ),
        ("installerOpened", AndroidInstallResult::InstallerOpened),
    ] {
        let events = RefCell::new(Vec::new());
        let result = download_and_install(
            &client,
            &selected,
            &destination,
            |event| {
                events
                    .borrow_mut()
                    .push(serde_json::to_value(event).unwrap())
            },
            |version| {
                assert_eq!(version, "0.3.0");
                assert_eq!(std::fs::read(&destination).unwrap(), APK);
                Ok(response.into())
            },
        )
        .unwrap();
        assert_eq!(result, expected);
        assert_eq!(events.borrow().last().unwrap()["event"], "installing");
        assert_eq!(
            server.downloads.load(Ordering::Relaxed),
            1,
            "Permission retry downloaded again"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    for response in [
        Err("Certificate rejected".into()),
        Ok("unknown result".into()),
    ] {
        assert!(
            download_and_install(&client, &selected, &destination, |_| {}, |_| response).is_err()
        );
    }
    // A modified cache must be downloaded again before another installer handoff.
    std::fs::write(&destination, b"corrupted cache").unwrap();
    assert!(download_and_install(
        &client,
        &selected,
        &destination,
        |_| {},
        |_| Ok("installerOpened".into())
    )
    .is_ok());
    assert_eq!(server.downloads.load(Ordering::Relaxed), 2);
    assert_eq!(std::fs::read(&destination).unwrap(), APK);
}
