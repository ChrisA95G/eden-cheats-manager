use rusqlite::Connection;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const DB_FILENAME: &str = "titles.db";
static TITLES_DB_BYTES: &[u8] = include_bytes!("../../../titles.db");

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

/// Extract titles.db from the embedded bytes to app_data_dir on first run (or if
/// the cached copy is corrupt/missing). Returns the app_data_dir path to the DB file.
fn ensure_db_file(app: &AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    let data_path = data_dir.join(DB_FILENAME);

    let cached_size = std::fs::metadata(&data_path).map(|m| m.len()).unwrap_or(0);
    let needs_write = cached_size < 1_048_576;

    if needs_write {
        if cached_size > 0 {
            log::warn!("[db] cached titles.db is only {} bytes (corrupt), overwriting", cached_size);
        }
        let _ = std::fs::create_dir_all(&data_dir);
        match std::fs::write(&data_path, TITLES_DB_BYTES) {
            Ok(_) => log::info!("[db] extracted titles.db ({:.1} MB) to {:?}",
                TITLES_DB_BYTES.len() as f64 / 1_048_576.0, data_path),
            Err(e) => log::error!("[db] failed to write titles.db: {}", e),
        }
    } else {
        log::debug!("[db] titles.db already at {:?} ({:.1} MB)", data_path, cached_size as f64 / 1_048_576.0);
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
