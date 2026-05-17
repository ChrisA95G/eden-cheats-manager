---
name: android-tauri-dev
description: Patterns, pitfalls, and procedures for developing the Android version of Eden Cheats Manager with Tauri 2. Use when debugging Android builds, adding native Android commands, fixing cross-platform database issues, or setting up the dev environment.
---

# Android Tauri Dev — Eden Cheats Manager

## Architecture

- **Framework**: Tauri 2 with SvelteKit frontend, same repo as desktop
- **Mode flag**: `TargetMode::AndroidNative` in `settings.rs` — detected at runtime via `#[cfg(target_os = "android")]`
- **Android backend**: `gui/src-tauri/src/android_native.rs` — all native FS commands live here
- **Eden load dir**: `/storage/emulated/0/Android/data/dev.eden.eden_emulator/files/load`
- **App package**: `dev.eden.cheats_manager`

## Dev Environment Setup

### Real Device Dev
```bash
# Must set LAN IP for real device hot-reload
TAURI_DEV_HOST=<pc-lan-ip> npm run tauri android dev

# Firewall ports required
firewall-cmd --add-port=1420/tcp
firewall-cmd --add-port=1421/tcp
```

### Best Debugging Tool
```bash
# Stream all Rust log:: and JS console output live
adb logcat | grep -E "RustStdout|Tauri"
```

## Critical: OpenSSL Cross-Compile Fix

`reqwest` must use `rustls-tls` or Android builds fail with `openssl-sys` errors:
```toml
# Cargo.toml
reqwest = { default-features = false, features = ["json", "rustls-tls"] }
```

## Critical: Database Loading on Android

`resource_dir()` points inside the APK (e.g. `base.apk!/`) — **`std::fs` cannot read from it on Android**. The `ensure_db_file()` function in `db.rs` must use size-based validation and include external storage as a candidate:

### The Problem
- `resource_dir()` is unreadable via `std::fs` on Android
- SQLite silently creates a 0-byte file when `Connection::open` targets a non-existent path — subsequent calls think the file "exists" but queries return 0 rows
- Symptom: `prefix_map keys: []` / `0 groups built` despite valid title IDs found

### The Fix (already applied in `db.rs`)
1. Check cached file size — anything < 1 MB is treated as corrupt and deleted
2. Add `/storage/emulated/0/<filename>` as a candidate path (external storage)
3. During development: `adb push titles.db /storage/emulated/0/titles.db`

### For Production / Release Builds
Use `include_bytes!` to embed the DB at compile time:
```rust
#[cfg(target_os = "android")]
const TITLES_DB_BYTES: &[u8] = include_bytes!("../../../../titles.db");
// Write to data_path on first launch if not present
```

### Both DBs need the same treatment
`titles.db` **and** `cheats.db` both need to be pushed for dev:
```bash
adb push titles.db /storage/emulated/0/titles.db
adb push cheats.db /storage/emulated/0/cheats.db
```

## Storage Permissions (Android 11+)

`MANAGE_EXTERNAL_STORAGE` is required. Added to `AndroidManifest.xml`:
```xml
<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE"/>
<uses-permission android:name="android.permission.WRITE_EXTERNAL_STORAGE"/>
<uses-permission android:name="android.permission.MANAGE_EXTERNAL_STORAGE"/>
```
The app shows a permission gate screen in `+page.svelte` before scanning if not granted.

## Key File Locations

| File | Purpose |
|------|---------|
| `gui/src-tauri/src/android_native.rs` | All native Android commands |
| `gui/src-tauri/src/db.rs` | DB loading — `ensure_db_file()` is the critical fn |
| `gui/src-tauri/src/settings.rs` | `TargetMode` enum, `get_platform()` command |
| `gui/src-tauri/src/lib.rs` | Command registration |
| `gui/src/routes/+page.svelte` | Platform detection, permission gate, `DebugPanel` |
| `gui/src/lib/stores/games.js` | `scanGames()` routes to android_native for `androidNative` mode |
| `gui/src/lib/components/DebugPanel.svelte` | Fixed overlay debug log panel |
| `gui/src/lib/stores/debug.js` | `debugLog()` store for in-app debug output |

## Learnings

- **Silent SQLite file creation**: If `Connection::open` is called with a path that doesn't exist, SQLite creates a 0-byte valid (but empty) database. Always validate file size before trusting the cached path.
- **`resource_dir()` on Android**: Returns a path inside the APK — completely unreadable via `std::fs`. Never use it as a file copy source on Android.
- **`adb logcat` beats in-app debug panels**: The `DebugPanel.svelte` component is useful for quick checks, but `adb logcat | grep RustStdout` gives full Rust log output in real time and is far more reliable.
- **cheats.db needs the same fix as titles.db**: Both databases are bundled resources and both fail to load via `resource_dir()` on Android. Apply the same size-check + external-storage candidate to `cheats.db`.
