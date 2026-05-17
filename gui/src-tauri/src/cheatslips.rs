use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const CHEATS_DB_FILENAME: &str = "cheats.db";
static CHEATS_DB_BYTES: &[u8] = include_bytes!("../../../cheats.db");

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

/// Extract cheats.db from the embedded bytes to app_data_dir on first run (or if
/// the cached copy is corrupt/missing). Also runs schema migrations. Returns the path.
fn ensure_cheats_db(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {}", e))?;
    let data_path = data_dir.join(CHEATS_DB_FILENAME);

    let cached_size = std::fs::metadata(&data_path).map(|m| m.len()).unwrap_or(0);
    let needs_write = cached_size < 1_048_576;

    if needs_write {
        if cached_size > 0 {
            log::warn!("[cheats_db] cached cheats.db is only {} bytes (corrupt), overwriting", cached_size);
        }
        let _ = std::fs::create_dir_all(&data_dir);
        std::fs::write(&data_path, CHEATS_DB_BYTES)
            .map_err(|e| format!("Failed to write cheats.db: {}", e))?;
        log::info!("[cheats_db] extracted cheats.db ({:.1} MB) to {:?}",
            CHEATS_DB_BYTES.len() as f64 / 1_048_576.0, data_path);
    } else {
        log::debug!("[cheats_db] cheats.db already at {:?} ({:.1} MB)", data_path, cached_size as f64 / 1_048_576.0);
    }

    migrate_cheats_db(&data_path)?;

    Ok(data_path)
}

/// Add missing columns if they don't already exist. Safe to run on every startup.
fn migrate_cheats_db(path: &PathBuf) -> Result<(), String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open cheats.db for migration: {}", e))?;
    // Ignore "duplicate column" errors — means that migration already ran.
    let _ = conn.execute(
        "ALTER TABLE cheats ADD COLUMN custom INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE cheats ADD COLUMN api_fetched INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE cheats ADD COLUMN code_hash TEXT",
        [],
    );
    // Invalidate any old-format code_hash values that are not 64-char hex strings
    // (previous version stored the raw normalized opcode text instead of a SHA256 hash).
    let _ = conn.execute(
        "UPDATE cheats SET code_hash = NULL WHERE code_hash IS NOT NULL AND length(code_hash) != 64",
        [],
    );
    // Backfill code_hash for any rows that don't have it yet.
    let mut stmt = conn
        .prepare("SELECT id, content FROM cheats WHERE code_hash IS NULL")
        .map_err(|e| format!("Failed to prepare backfill query: {}", e))?;
    let ids_and_contents: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to query rows for backfill: {}", e))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    for (id, content) in ids_and_contents {
        let hash = code_fingerprint(&content);
        let _ = conn.execute(
            "UPDATE cheats SET code_hash = ?1 WHERE id = ?2",
            rusqlite::params![hash, id],
        );
    }
    Ok(())
}

/// SHA256 fingerprint of the opcode lines in a cheat content blob.
/// Only lines whose first 8 characters are all hex digits count as opcodes —
/// everything else (section headers, comments, plain text) is ignored.
/// Two entries with the same codes but different cheat names hash identically.
fn code_fingerprint(content: &str) -> String {
    let opcodes: String = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| {
            let mut chars = l.chars();
            (0..8).all(|_| chars.next().map(|c| c.is_ascii_hexdigit()).unwrap_or(false))
        })
        .map(|l| l.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    let mut hasher = Sha256::new();
    hasher.update(opcodes.as_bytes());
    hex::encode(hasher.finalize())
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
    let hash = code_fingerprint(&content_trimmed);

    conn.execute(
        "INSERT INTO cheats (title_id, build_id, content, credits, custom, code_hash) \
         VALUES (?1, ?2, ?3, '', 1, ?4)",
        rusqlite::params![title_id_upper, build_id_upper, content_trimmed, hash],
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

// ── Cheatslips API types ─────────────────────────────────────────────────────
// The API returns a single Game object (not an array), with lowercase field names.

#[derive(Debug, Deserialize)]
struct ApiCheat {
    buildid: String,
    content: String,
    credits: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiGame {
    cheats: Vec<ApiCheat>,
}

/// Fetch cheats from the Cheatslips API for the given title ID and cache them
/// in the local cheats.db. Returns the number of new entries inserted.
#[tauri::command]
pub async fn fetch_cheats_online(
    app: AppHandle,
    title_id: String,
    api_token: String,
) -> Result<usize, String> {
    let token = api_token.trim().to_string();
    if token.is_empty() {
        return Err("API token required — add it in Settings.".to_string());
    }

    let title_id_upper = title_id.trim().to_uppercase();
    let url = format!(
        "https://www.cheatslips.com/api/v1/cheats/{}",
        title_id_upper
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let resp = client
        .get(&url)
        .header("X-API-TOKEN", &token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    match resp.status().as_u16() {
        429 => {
            return Err(
                "Daily rate limit reached (3 requests/day). Try again tomorrow.".to_string(),
            )
        }
        401 | 403 => {
            return Err("Invalid or expired API token. Check your Settings.".to_string())
        }
        404 => return Ok(0),
        s if s >= 400 => return Err(format!("API returned HTTP {}", s)),
        _ => {}
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read API response body: {}", e))?;

    log::debug!(
        "[cheatslips_api] raw response for {}: {}",
        title_id_upper,
        body
    );

    let game: ApiGame = serde_json::from_str(&body).map_err(|e| {
        let preview = &body[..body.len().min(400)];
        format!("Failed to parse API response: {} — body: {}", e, preview)
    })?;

    // DB operations — no more .await after this point
    let cheats_db_path = ensure_cheats_db(&app)?;
    let conn = Connection::open(&cheats_db_path)
        .map_err(|e| format!("Failed to open cheats.db: {}", e))?;

    let mut inserted = 0usize;
    for cheat in &game.cheats {
        if cheat.content.trim().is_empty() {
            continue;
        }
        let bid = cheat.buildid.trim().to_uppercase();
        let credits = cheat.credits.as_deref().unwrap_or("").to_string();
        let hash = code_fingerprint(&cheat.content);
        let n = conn
            .execute(
                "INSERT INTO cheats (title_id, build_id, content, credits, custom, api_fetched, code_hash) \
                 SELECT ?1, ?2, ?3, ?4, 0, 1, ?5 \
                 WHERE NOT EXISTS (\
                   SELECT 1 FROM cheats \
                   WHERE title_id = ?1 AND build_id = ?2 AND code_hash = ?5\
                 )",
                rusqlite::params![title_id_upper, bid, cheat.content, credits, hash],
            )
            .map_err(|e| format!("DB insert error: {}", e))?;
        inserted += n;
    }

    log::info!(
        "[cheatslips_api] {}: {} new cheats cached from API",
        title_id_upper,
        inserted
    );
    Ok(inserted)
}

/// Delete all API-fetched cheats for a given title from cheats.db.
/// Bundled and user-custom entries are left untouched.
#[tauri::command]
pub fn clear_api_cheats(app: AppHandle, title_id: String) -> Result<usize, String> {
    let cheats_db_path = ensure_cheats_db(&app)?;
    let conn = Connection::open(&cheats_db_path)
        .map_err(|e| format!("Failed to open cheats.db: {}", e))?;

    let title_id_upper = title_id.trim().to_uppercase();
    let deleted = conn
        .execute(
            "DELETE FROM cheats WHERE title_id = ?1 AND api_fetched = 1",
            rusqlite::params![title_id_upper],
        )
        .map_err(|e| format!("Failed to clear API cheats: {}", e))?;

    log::info!(
        "[cheatslips_api] cleared {} API-fetched cheats for {}",
        deleted,
        title_id_upper
    );
    Ok(deleted)
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
