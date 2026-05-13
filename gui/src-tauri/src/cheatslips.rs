use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const CHEATS_DB_FILENAME: &str = "cheats.db";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheatEntry {
    pub id: u64,
    pub build_id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub credits: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub image: String,
    pub title_id: String,
    #[serde(default)]
    pub cheats: Vec<CheatEntry>,
}

/// Ensure cheats.db is copied from bundled resources to app_data_dir on first run.
/// Also runs schema migrations. Returns the path to the local copy.
fn ensure_cheats_db(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?;
    let data_path = data_dir.join(CHEATS_DB_FILENAME);

    if !data_path.exists() {
        let _ = std::fs::create_dir_all(&data_dir);

        let resource_dir = app
            .path()
            .resource_dir()
            .map_err(|e| format!("Failed to resolve resource dir: {}", e))?;

        let candidates = vec![
            resource_dir.join(CHEATS_DB_FILENAME),
            resource_dir.join("../../").join(CHEATS_DB_FILENAME),
            std::env::current_dir()
                .unwrap_or_default()
                .join(CHEATS_DB_FILENAME),
            std::env::current_dir()
                .unwrap_or_default()
                .join("../../")
                .join(CHEATS_DB_FILENAME),
        ];

        let mut copied = false;
        for candidate in &candidates {
            let canonical =
                std::fs::canonicalize(candidate).unwrap_or_else(|_| candidate.clone());
            log::debug!("[cheats_db] trying {:?}", canonical);
            if canonical.exists() {
                match std::fs::copy(&canonical, &data_path) {
                    Ok(_) => {
                        log::info!(
                            "[cheats_db] copied from {:?} to {:?}",
                            canonical, data_path
                        );
                        copied = true;
                        break;
                    }
                    Err(e) => log::warn!("[cheats_db] copy failed from {:?}: {}", canonical, e),
                }
            }
        }

        if !copied {
            return Err(format!(
                "cheats.db not found in bundled resources. Searched: {:?}",
                candidates
            ));
        }
    }

    migrate_cheats_db(&data_path)?;

    Ok(data_path)
}

/// Add the `custom` column if it doesn't already exist. Safe to run on every startup.
fn migrate_cheats_db(path: &PathBuf) -> Result<(), String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open cheats.db for migration: {}", e))?;
    // Ignore "duplicate column" error — means migration already ran.
    let _ = conn.execute(
        "ALTER TABLE cheats ADD COLUMN custom INTEGER NOT NULL DEFAULT 0",
        [],
    );
    Ok(())
}

fn build_description(content: &str, build_id: &str) -> String {
    let cheat_count = content
        .lines()
        .filter(|l| l.starts_with('[') && l.ends_with(']'))
        .count();
    format!(
        "{} cheat{} — Build ID: {}",
        cheat_count,
        if cheat_count == 1 { "" } else { "s" },
        build_id
    )
}

/// Look up all cheats (bundled + custom) for a given title_id from cheats.db.
#[tauri::command]
pub fn search_cheats(app: AppHandle, title_id: String) -> Result<GameInfo, String> {
    let db_state = app.state::<crate::db::DbState>();

    // Get game name + image from titles.db
    let (game_name, game_image) = {
        let conn = Connection::open_with_flags(
            &db_state.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("Failed to open titles.db: {}", e))?;

        conn.query_row(
            "SELECT name, iconUrl FROM titles WHERE title_id = ?1 LIMIT 1",
            [&title_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1).unwrap_or_default(),
                ))
            },
        )
        .ok()
        .unwrap_or_default()
    };

    let cheats_db_path = ensure_cheats_db(&app)?;

    let conn = Connection::open_with_flags(
        &cheats_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open cheats.db: {}", e))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, build_id, content, credits, COALESCE(custom, 0) \
             FROM cheats WHERE title_id = ?1 \
             ORDER BY custom ASC, build_id",
        )
        .map_err(|e| format!("Failed to prepare cheats query: {}", e))?;

    let cheats: Vec<CheatEntry> = stmt
        .query_map([&title_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| format!("Failed to query cheats: {}", e))?
        .filter_map(|r| r.ok())
        .map(|(id, build_id, content, credits, custom)| {
            let description = build_description(&content, &build_id);
            CheatEntry {
                id: id as u64,
                build_id,
                content,
                credits,
                description,
                custom: custom != 0,
            }
        })
        .collect();

    Ok(GameInfo {
        slug: String::new(),
        name: game_name,
        image: game_image,
        title_id: title_id.to_uppercase(),
        cheats,
    })
}

/// Save a user-defined cheat entry to cheats.db.
#[tauri::command]
pub fn save_custom_cheat(
    app: AppHandle,
    title_id: String,
    build_id: String,
    content: String,
) -> Result<CheatEntry, String> {
    if build_id.trim().is_empty() {
        return Err("Build ID is required".to_string());
    }
    if content.trim().is_empty() {
        return Err("Cheat content is required".to_string());
    }

    let cheats_db_path = ensure_cheats_db(&app)?;
    let conn = Connection::open(&cheats_db_path)
        .map_err(|e| format!("Failed to open cheats.db: {}", e))?;

    let title_id_upper = title_id.to_uppercase();
    let build_id_upper = build_id.trim().to_uppercase();
    let content_trimmed = content.trim().to_string();

    conn.execute(
        "INSERT INTO cheats (title_id, build_id, content, credits, custom) \
         VALUES (?1, ?2, ?3, '', 1)",
        rusqlite::params![title_id_upper, build_id_upper, content_trimmed],
    )
    .map_err(|e| format!("Failed to save custom cheat: {}", e))?;

    let id = conn.last_insert_rowid() as u64;
    let description = build_description(&content_trimmed, &build_id_upper);

    log::info!("[cheats_db] saved custom cheat id={id} title={title_id_upper} build={build_id_upper}");

    Ok(CheatEntry {
        id,
        build_id: build_id_upper,
        content: content_trimmed,
        credits: String::new(),
        description,
        custom: true,
    })
}

/// Delete a custom cheat entry from cheats.db. Only works on custom entries.
#[tauri::command]
pub fn delete_custom_cheat(app: AppHandle, cheat_id: u64) -> Result<(), String> {
    let cheats_db_path = ensure_cheats_db(&app)?;
    let conn = Connection::open(&cheats_db_path)
        .map_err(|e| format!("Failed to open cheats.db: {}", e))?;

    let deleted = conn
        .execute(
            "DELETE FROM cheats WHERE id = ?1 AND custom = 1",
            rusqlite::params![cheat_id as i64],
        )
        .map_err(|e| format!("Failed to delete custom cheat: {}", e))?;

    if deleted == 0 {
        return Err("Cheat not found or is not a custom cheat".to_string());
    }

    log::info!("[cheats_db] deleted custom cheat id={cheat_id}");
    Ok(())
}
