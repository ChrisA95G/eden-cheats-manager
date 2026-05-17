use rusqlite::Connection;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const DB_FILENAME: &str = "titles.db";

#[derive(Debug, Clone)]
pub struct DbState {
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct TitleRow {
    pub title_id: String,
    pub name: String,
    pub image: String,
}

/// Copy titles.db from the bundled resource dir to app_data_dir if it doesn't
/// already exist there. Returns the app_data_dir path to the DB file.
fn ensure_db_file(app: &AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    let data_path = data_dir.join(DB_FILENAME);

    log::info!("[db] data_dir={:?}, data_path={:?}", data_dir, data_path);

    // Check if the cached file is valid (must be > 1 MB; a 0-byte or corrupt SQLite
    // file is created when the first copy attempt failed and SQLite opened the path).
    let cached_size = if data_path.exists() {
        std::fs::metadata(&data_path)
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    const MIN_DB_BYTES: u64 = 1_048_576; // 1 MB
    let needs_copy = cached_size < MIN_DB_BYTES;

    if needs_copy {
        if cached_size > 0 {
            log::warn!("[db] cached titles.db is only {} bytes (likely corrupt), re-copying", cached_size);
            let _ = std::fs::remove_file(&data_path);
        }

        let _ = std::fs::create_dir_all(&data_dir);

        let resource_dir = app
            .path()
            .resource_dir()
            .unwrap_or_default();
        log::info!("[db] resource_dir={:?}", resource_dir);

        // Candidate paths — external storage is the reliable fallback on Android.
        let candidates = vec![
            resource_dir.join(DB_FILENAME),
            resource_dir.join("../../").join(DB_FILENAME),
            std::path::PathBuf::from("/storage/emulated/0").join(DB_FILENAME),
            std::env::current_dir().unwrap_or_default().join(DB_FILENAME),
            std::env::current_dir()
                .unwrap_or_default()
                .join("../../")
                .join(DB_FILENAME),
        ];

        let mut found = false;
        for candidate in &candidates {
            let canonical = std::fs::canonicalize(candidate).unwrap_or_else(|_| candidate.clone());
            log::info!("[db] trying {:?}", canonical);
            let src_size = std::fs::metadata(&canonical).map(|m| m.len()).unwrap_or(0);
            if src_size >= MIN_DB_BYTES {
                match std::fs::copy(&canonical, &data_path) {
                    Ok(_) => {
                        log::info!("[db] copied titles.db ({:.1} MB) from {:?}",
                            src_size as f64 / 1_048_576.0, canonical);
                        found = true;
                        break;
                    }
                    Err(e) => log::warn!("[db] copy failed from {:?}: {}", canonical, e),
                }
            }
        }

        if !found {
            log::warn!("[db] titles.db not found in any candidate location (dev: push via `adb push titles.db /storage/emulated/0/titles.db`)");
        }
    } else {
        log::info!("[db] titles.db cached at {:?} ({:.1} MB)", data_path, cached_size as f64 / 1_048_576.0);
    }

    if !data_path.exists() {
        log::error!("[db] ERROR: titles.db does not exist at {:?}. Game scan will return empty results.", data_path);
    }

    data_path
}

/// Run during Tauri setup to prepare the DB file and store its path in state.
pub fn init_db(app: &AppHandle) {
    let data_path = ensure_db_file(app);
    app.manage(DbState { path: data_path });
}

/// Query titles.db for all non-demo, named titles whose title_id starts
/// with the given base prefix (first 13 characters of a 16-char title ID).
pub fn query_base_prefix(state: &DbState, base_prefix: &str) -> Result<Vec<TitleRow>, String> {
    let conn = Connection::open_with_flags(
        &state.path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open database: {}", e))?;

    let mut stmt = conn
        .prepare(
            "SELECT title_id, name, iconUrl FROM titles \
             WHERE title_id LIKE ?1 || '%' \
             AND isDemo = 0",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows: Vec<TitleRow> = stmt
        .query_map([base_prefix], |row| {
            Ok(TitleRow {
                title_id: row.get(0)?,
                name: row.get(1)?,
                image: row.get::<_, String>(2).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Failed to execute query: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    log::debug!(
        "[db] query prefix={:?} returned {} rows",
        base_prefix, rows.len()
    );

    Ok(rows)
}
