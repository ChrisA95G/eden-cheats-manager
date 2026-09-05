use crate::db;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleEntry {
    pub title_id: String,
    pub base_title_id: String,
    pub name: String,
    pub image: String,
    pub category: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameGroup {
    pub base_title_id: String,
    pub base_name: String,
    pub base_image: String,
    pub base_installed: bool,
    pub base_game: Option<TitleEntry>,
    pub updates: Vec<TitleEntry>,
    pub dlcs: Vec<TitleEntry>,
}

fn category_from_tid(tid: &str) -> &str {
    if tid.len() < 3 {
        return "dlc";
    }
    match &tid[tid.len() - 3..] {
        "000" => "base",
        "800" => "update",
        _ => "dlc",
    }
}

pub(crate) fn is_valid_tid(s: &str) -> bool {
    s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EdenPresenceRecord {
    pub(crate) observed_title_id: String,
    pub(crate) resolved_base_title_id: Option<String>,
    pub(crate) resolution_issue: Option<String>,
}

pub(crate) fn extract_eden_presence(
    rows: &[db::TitleRow],
    installed_ids: &HashSet<String>,
) -> Vec<EdenPresenceRecord> {
    let mut base_candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        if !is_valid_tid(&row.title_id) {
            continue;
        }
        let title_id = row.title_id.to_ascii_uppercase();
        if title_id.ends_with("000") {
            base_candidates
                .entry(title_id[..12].to_string())
                .or_default()
                .insert(title_id);
        }
    }

    let observed_ids: BTreeSet<String> = installed_ids
        .iter()
        .filter(|title_id| is_valid_tid(title_id))
        .map(|title_id| title_id.to_ascii_uppercase())
        .collect();

    observed_ids
        .into_iter()
        .map(|observed_title_id| {
            let candidates = base_candidates.get(&observed_title_id[..12]);
            let (resolved_base_title_id, resolution_issue) = if observed_title_id
                .ends_with("000")
            {
                if candidates.is_some_and(|candidates| candidates.contains(&observed_title_id)) {
                    (Some(observed_title_id.clone()), None)
                } else {
                    (
                        None,
                        Some(format!(
                            "No authoritative base Title ID was found for observed Eden title {observed_title_id}."
                        )),
                    )
                }
            } else {
                match candidates {
                    Some(candidates) if candidates.len() == 1 => {
                        (candidates.first().cloned(), None)
                    }
                    Some(candidates) if candidates.len() > 1 => (
                        None,
                        Some(format!(
                            "Multiple authoritative base Title IDs match observed Eden title {observed_title_id}."
                        )),
                    ),
                    _ => (
                        None,
                        Some(format!(
                            "No authoritative base Title ID was found for observed Eden title {observed_title_id}."
                        )),
                    ),
                }
            };

            EdenPresenceRecord {
                observed_title_id,
                resolved_base_title_id,
                resolution_issue,
            }
        })
        .collect()
}

pub(crate) fn build_groups_with_presence(
    rows: Vec<db::TitleRow>,
    installed_ids: &HashSet<String>,
) -> (Vec<GameGroup>, Vec<EdenPresenceRecord>) {
    let presence = extract_eden_presence(&rows, installed_ids);
    let groups = build_groups(rows, installed_ids);
    (groups, presence)
}

/// Group SQLite title rows into GameGroups keyed by the first 12 chars of
/// title_id (the game family identifier), marking installed titles against
/// a set of known IDs. Uses 12 chars instead of 13 because some games have
/// DLCs/updates that differ at the 13th character position.
pub(crate) fn build_groups(rows: Vec<db::TitleRow>, installed_ids: &HashSet<String>) -> Vec<GameGroup> {
    log::info!("[games] build_groups: {} db rows, {} installed ids", rows.len(), installed_ids.len());
    for tid in installed_ids {
        log::debug!("[games] installed_id: {tid}");
    }
    for r in &rows {
        log::debug!("[games] db row: title_id={} name={:?}", r.title_id, r.name);
    }

    // Group by first 12 chars (game family), not 13 — some games have
    // DLCs/updates whose 13th char differs from the base game
    let mut prefix_map: std::collections::BTreeMap<String, Vec<db::TitleRow>> = std::collections::BTreeMap::new();
    for row in rows {
        if row.title_id.len() >= 12 {
            let prefix = row.title_id[..12].to_string();
            prefix_map.entry(prefix).or_default().push(row);
        }
    }

    log::debug!("[games] prefix_map keys: {:?}", prefix_map.keys());

    let mut groups = Vec::new();

    for (_prefix, group_rows) in prefix_map {
        // Find the base game (title_id ending in "000") — the real title_id,
        // not a constructed one
        let base_row = group_rows.iter().find(|r| r.title_id.ends_with("000"));
        let base_tid = base_row.map(|r| r.title_id.clone()).unwrap_or_default();
        let base_installed = if base_tid.is_empty() {
            false
        } else {
            installed_ids.contains(&base_tid)
        };
        let base_name = base_row
            .map(|r| r.name.clone())
            .unwrap_or_else(|| group_rows.first().map(|r| r.name.clone()).unwrap_or_default());
        let base_image = base_row
            .map(|r| r.image.clone())
            .unwrap_or_default();

        let mut base_game = None;
        let mut updates = Vec::new();
        let mut dlcs = Vec::new();

        for row in &group_rows {
            let installed = installed_ids.contains(&row.title_id);
            // Updates/DLCs in titles.db almost never have names — fall back to the
            // base game name so the sidebar shows something readable.
            let name = if row.name.is_empty() {
                base_name.clone()
            } else {
                row.name.clone()
            };
            let entry = TitleEntry {
                title_id: row.title_id.clone(),
                base_title_id: if base_tid.is_empty() { row.title_id.clone() } else { base_tid.clone() },
                name,
                image: row.image.clone(),
                category: category_from_tid(&row.title_id).to_string(),
                installed,
            };
            match category_from_tid(&row.title_id) {
                "base" => {
                    base_game = Some(entry);
                }
                "update" => updates.push(entry),
                _ => dlcs.push(entry),
            }
        }

        log::debug!(
            "[games] group prefix={} base_name={:?} base_tid={} base_installed={} base_game={} updates={} dlcs={}",
            _prefix, base_name, base_tid, base_installed, base_game.is_some(), updates.len(), dlcs.len()
        );

        groups.push(GameGroup {
            base_title_id: base_tid,
            base_name,
            base_image,
            base_installed,
            base_game,
            updates,
            dlcs,
        });
    }

    log::info!("[games] pre-merge: {} groups", groups.len());
    for g in &groups {
        log::debug!("[games] pre-merge group: base_name={:?} base_tid={} base_game={} updates={} dlcs={}",
            g.base_name, g.base_title_id, g.base_game.is_some(), g.updates.len(), g.dlcs.len());
    }

    // Merge groups that share the same base_name
    let mut merged: Vec<GameGroup> = Vec::new();
    let mut name_map: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for group in groups {
        if group.base_name.is_empty() {
            log::debug!("[games] merge: base_name empty for {}, pushing as-is", group.base_title_id);
            merged.push(group);
            continue;
        }

        if let Some(&idx) = name_map.get(&group.base_name) {
            log::debug!("[games] merge: {:?} already at index {} — merging dlcs({}) and updates({})",
                group.base_name, idx, group.dlcs.len(), group.updates.len());
            let existing = &mut merged[idx];
            existing.updates.extend(group.updates);
            existing.dlcs.extend(group.dlcs);
            if existing.base_game.is_none() && group.base_game.is_some() {
                log::debug!("[games] merge: adopting base_game from incoming group");
                existing.base_game = group.base_game;
                existing.base_title_id = group.base_title_id;
                existing.base_image = group.base_image.clone();
            }
            if group.base_installed {
                existing.base_installed = true;
            }
        } else {
            log::debug!("[games] merge: first occurrence of {:?} at index {}", group.base_name, merged.len());
            name_map.insert(group.base_name.clone(), merged.len());
            merged.push(group);
        }
    }

    for group in &mut merged {
        group.updates.sort_by(|a, b| a.name.cmp(&b.name));
        group.dlcs.sort_by(|a, b| a.name.cmp(&b.name));
    }
    merged.sort_by(|a, b| a.base_name.cmp(&b.base_name));

    log::info!("[games] post-merge: {} groups", merged.len());
    for g in &merged {
        log::info!("[games] merged group: base_name={:?} base_tid={} base_installed={} base_game={} updates={} dlcs={}",
            g.base_name, g.base_title_id, g.base_installed, g.base_game.is_some(), g.updates.len(), g.dlcs.len());
    }

    // ── Synthetic entries for installed IDs dropped by the DB query ────────────
    // titles.db stores almost no names for update/DLC entries, so the
    // `name != ''` filter silently drops them. Re-inject any installed ID that
    // didn't make it into any group, using the base game name as a fallback.
    let accounted: HashSet<String> = merged.iter().flat_map(|g| {
        let mut ids = Vec::new();
        if let Some(bg) = &g.base_game { ids.push(bg.title_id.clone()); }
        for u in &g.updates { ids.push(u.title_id.clone()); }
        for d in &g.dlcs   { ids.push(d.title_id.clone()); }
        ids
    }).collect();

    // Index: first-12-chars of base_title_id → index in merged
    let mut prefix_to_idx: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (i, g) in merged.iter().enumerate() {
        if g.base_title_id.len() >= 12 {
            prefix_to_idx.entry(g.base_title_id[..12].to_string()).or_insert(i);
        }
    }

    let mut to_add: Vec<(usize, TitleEntry)>  = Vec::new(); // (group_idx, entry)
    let mut new_groups: Vec<GameGroup> = Vec::new();
    let merged_len = merged.len(); // constant during the loop below

    for tid in installed_ids {
        if accounted.contains(tid) { continue; }
        log::info!("[games] unaccounted installed_id: {tid}");
        let category = category_from_tid(tid);
        let p12 = if tid.len() >= 12 { &tid[..12] } else { tid.as_str() };

        // Use base group's name for updates so the entry looks nice.
        let fallback_name = prefix_to_idx.get(p12)
            .map(|&idx| {
                if idx < merged_len { merged[idx].base_name.clone() }
                else { new_groups[idx - merged_len].base_name.clone() }
            })
            .unwrap_or_default();

        let base_tid_for_entry = prefix_to_idx.get(p12)
            .map(|&idx| {
                if idx < merged_len { merged[idx].base_title_id.clone() }
                else { new_groups[idx - merged_len].base_title_id.clone() }
            })
            .unwrap_or_else(|| tid.clone());
        let entry = TitleEntry {
            title_id: tid.clone(),
            base_title_id: base_tid_for_entry,
            name: fallback_name,
            image: String::new(),
            category: category.to_string(),
            installed: true,
        };

        if let Some(&idx) = prefix_to_idx.get(p12) {
            if idx < merged_len {
                // Defer: can't mutably borrow merged while prefix_to_idx borrows it
                to_add.push((idx, entry));
            } else {
                // Index into new_groups — no borrow conflict, insert directly
                let ng = &mut new_groups[idx - merged_len];
                match entry.category.as_str() {
                    "update" => ng.updates.push(entry),
                    "base" => {
                        if ng.base_game.is_none() {
                            ng.base_installed = true;
                            ng.base_game = Some(entry);
                        }
                    }
                    _ => ng.dlcs.push(entry),
                }
            }
        } else {
            // No known game group for this prefix (e.g. Nintendo system firmware
            // titles like 010000000000xxxx). Skip — we don't want to display
            // unrecognised system content in the library.
            log::debug!("[games] skipping unrecognised prefix {p12} for {tid}");
        }
    }

    // Apply deferred insertions (can't borrow merged mutably while iterating above)
    for (idx, entry) in to_add {
        match entry.category.as_str() {
            "update" => merged[idx].updates.push(entry),
            "base" => {
                if merged[idx].base_game.is_none() {
                    merged[idx].base_installed = true;
                    merged[idx].base_game = Some(entry);
                }
            }
            _ => merged[idx].dlcs.push(entry),
        }
    }
    merged.extend(new_groups);

    // Re-sort updates/dlcs inside affected groups
    for g in &mut merged {
        g.updates.sort_by(|a, b| a.title_id.cmp(&b.title_id));
    }

    merged
}

/// Scan the Eden PC load dir for title IDs, cross-reference with the
/// SQLite titles database, and return hierarchical game groups.
pub(crate) fn scan_eden_games_pc_with_presence(
    app: &AppHandle,
    load_dir: &str,
) -> Result<(Vec<GameGroup>, Vec<EdenPresenceRecord>), String> {
    let dir = std::path::PathBuf::from(load_dir);
    log::info!("[games::pc] load_dir={:?}, exists={}", dir, dir.exists());
    if !dir.exists() {
        return Ok((Vec::new(), Vec::new()));
    }

    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut installed_ids = HashSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let tid = name.to_string_lossy().to_string();
        if is_valid_tid(&tid) && entry.path().is_dir() {
            installed_ids.insert(tid);
        }
    }

    log::info!("[games::pc] {} valid installed ids found", installed_ids.len());
    for tid in &installed_ids {
        log::debug!("[games::pc] on-disk: {tid}");
    }

    let state = app.state::<db::DbState>();
    log::debug!("[games::pc] db path: {:?}", state.path);

    // Collect unique base prefixes (first 12 chars — game family)
    let mut seen_prefixes = HashSet::new();
    for tid in &installed_ids {
        if tid.len() >= 12 {
            seen_prefixes.insert(tid[..12].to_string());
        }
    }
    log::debug!("[games::pc] unique 12-char prefixes: {:?}", seen_prefixes);

    let mut all_rows = Vec::new();
    for prefix in &seen_prefixes {
        match db::query_base_prefix(&state, prefix) {
            Ok(rows) => {
                log::debug!("[games::pc] prefix {} -> {} rows", prefix, rows.len());
                all_rows.extend(rows);
            }
            Err(e) => log::warn!("[games::pc] prefix {} query error: {}", prefix, e),
        }
    }

    let (groups, presence) = build_groups_with_presence(all_rows, &installed_ids);
    log::info!("[games::pc] {} groups built", groups.len());
    save_game_cache(app, "pc", &groups);
    Ok((groups, presence))
}

#[tauri::command]
pub async fn scan_eden_games_pc(
    app: AppHandle,
    load_dir: String,
) -> Result<Vec<GameGroup>, String> {
    scan_eden_games_pc_with_presence(&app, &load_dir).map(|(groups, _presence)| groups)
}

// ── Game list cache ───────────────────────────────────────────────────────────

fn game_cache_path(app: &AppHandle, mode: &str) -> std::path::PathBuf {
    let filename = match mode {
        "android" => "game_list_cache_android.json",
        _ => "game_list_cache_pc.json",
    };
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(filename)
}

pub(crate) fn save_game_cache(app: &AppHandle, mode: &str, groups: &[GameGroup]) {
    let path = game_cache_path(app, mode);
    if let Ok(json) = serde_json::to_string(groups) {
        let _ = std::fs::write(&path, json);
    }
}

fn load_game_cache(app: &AppHandle, mode: &str) -> Vec<GameGroup> {
    let path = game_cache_path(app, mode);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}

/// Return the cached game list from the last successful PC scan.
#[tauri::command]
pub fn get_cached_games_pc(app: AppHandle) -> Vec<GameGroup> {
    load_game_cache(&app, "pc")
}

/// Return the cached game list from the last successful Android scan.
#[tauri::command]
pub fn get_cached_games_android(app: AppHandle) -> Vec<GameGroup> {
    load_game_cache(&app, "android")
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(title_id: &str, name: &str, image: &str) -> db::TitleRow {
        db::TitleRow {
            title_id: title_id.into(),
            name: name.into(),
            image: image.into(),
        }
    }

    #[test]
    fn build_groups_keeps_current_prefix_shape_and_synthetic_entries() {
        let rows = vec![
            row("0100AAAA00000800", "", "update.png"),
            row("0100AAAA00000000", "Alpha", "base.png"),
            row("0100AAAA00000001", "", "dlc.png"),
        ];
        let installed_ids = HashSet::from([
            "0100AAAA00000000".to_string(),
            "0100AAAA00000800".to_string(),
            "0100AAAA00001800".to_string(),
            "0100FFFF00000800".to_string(),
        ]);

        assert_eq!(
            serde_json::to_value(build_groups(rows, &installed_ids)).unwrap(),
            json!([{
                "baseTitleId": "0100AAAA00000000",
                "baseName": "Alpha",
                "baseImage": "base.png",
                "baseInstalled": true,
                "baseGame": {
                    "titleId": "0100AAAA00000000",
                    "baseTitleId": "0100AAAA00000000",
                    "name": "Alpha",
                    "image": "base.png",
                    "category": "base",
                    "installed": true,
                },
                "updates": [
                    {
                        "titleId": "0100AAAA00000800",
                        "baseTitleId": "0100AAAA00000000",
                        "name": "Alpha",
                        "image": "update.png",
                        "category": "update",
                        "installed": true,
                    },
                    {
                        "titleId": "0100AAAA00001800",
                        "baseTitleId": "0100AAAA00000000",
                        "name": "Alpha",
                        "image": "",
                        "category": "update",
                        "installed": true,
                    }
                ],
                "dlcs": [{
                    "titleId": "0100AAAA00000001",
                    "baseTitleId": "0100AAAA00000000",
                    "name": "Alpha",
                    "image": "dlc.png",
                    "category": "dlc",
                    "installed": false,
                }]
            }])
        );
    }

    #[test]
    fn build_groups_name_merges_distinct_families_for_display() {
        let rows = vec![
            row("0100BBBB00000000", "Shared", "b.png"),
            row("0100BBBB00000800", "", "b-update.png"),
            row("0100AAAA00000000", "Shared", "a.png"),
            row("0100AAAA00000001", "", "a-dlc.png"),
        ];
        let installed_ids = HashSet::from(["0100BBBB00000000".to_string()]);

        assert_eq!(
            serde_json::to_value(build_groups(rows, &installed_ids)).unwrap(),
            json!([{
                "baseTitleId": "0100AAAA00000000",
                "baseName": "Shared",
                "baseImage": "a.png",
                "baseInstalled": true,
                "baseGame": {
                    "titleId": "0100AAAA00000000",
                    "baseTitleId": "0100AAAA00000000",
                    "name": "Shared",
                    "image": "a.png",
                    "category": "base",
                    "installed": false,
                },
                "updates": [{
                    "titleId": "0100BBBB00000800",
                    "baseTitleId": "0100BBBB00000000",
                    "name": "Shared",
                    "image": "b-update.png",
                    "category": "update",
                    "installed": false,
                }],
                "dlcs": [{
                    "titleId": "0100AAAA00000001",
                    "baseTitleId": "0100AAAA00000000",
                    "name": "Shared",
                    "image": "a-dlc.png",
                    "category": "dlc",
                    "installed": false,
                }]
            }])
        );
    }

    #[test]
    fn presence_normalizes_orders_and_resolves_only_authoritative_base_rows() {
        let rows = vec![
            row("0100AAAA00000000", "A", ""),
            row("0100aaaa00000000", "A duplicate", ""),
            row("0100AAAA00001000", "A sibling", ""),
            row("0100BBBB00000000", "B", ""),
            row("0100BBBB00001000", "B sibling", ""),
            row("0100CCCC00000000", "C", ""),
            row("0100CCCC00000800", "C update", ""),
            row("0100DDDD00000G00", "Malformed", ""),
            row("0100EEEE00001000", "E sibling", ""),
        ];
        let installed_ids = HashSet::from([
            "0100aaaa00000000".to_string(),
            "0100AAAA00000000".to_string(),
            "0100BBBB00000800".to_string(),
            "0100cccc00000800".to_string(),
            "0100DDDD00000800".to_string(),
            "0100EEEE00000000".to_string(),
            "not-a-title-id".to_string(),
        ]);

        assert_eq!(
            extract_eden_presence(&rows, &installed_ids),
            vec![
                EdenPresenceRecord {
                    observed_title_id: "0100AAAA00000000".into(),
                    resolved_base_title_id: Some("0100AAAA00000000".into()),
                    resolution_issue: None,
                },
                EdenPresenceRecord {
                    observed_title_id: "0100BBBB00000800".into(),
                    resolved_base_title_id: None,
                    resolution_issue: Some(
                        "Multiple authoritative base Title IDs match observed Eden title 0100BBBB00000800."
                            .into(),
                    ),
                },
                EdenPresenceRecord {
                    observed_title_id: "0100CCCC00000800".into(),
                    resolved_base_title_id: Some("0100CCCC00000000".into()),
                    resolution_issue: None,
                },
                EdenPresenceRecord {
                    observed_title_id: "0100DDDD00000800".into(),
                    resolved_base_title_id: None,
                    resolution_issue: Some(
                        "No authoritative base Title ID was found for observed Eden title 0100DDDD00000800."
                            .into(),
                    ),
                },
                EdenPresenceRecord {
                    observed_title_id: "0100EEEE00000000".into(),
                    resolved_base_title_id: None,
                    resolution_issue: Some(
                        "No authoritative base Title ID was found for observed Eden title 0100EEEE00000000."
                            .into(),
                    ),
                },
            ]
        );
    }

    #[test]
    fn presence_wrapper_preserves_the_legacy_group_payload() {
        fn rows() -> Vec<db::TitleRow> {
            vec![
                row("0100BBBB00000000", "Shared", "b.png"),
                row("0100BBBB00000800", "", "b-update.png"),
                row("0100AAAA00000000", "Shared", "a.png"),
                row("0100AAAA00000001", "", "a-dlc.png"),
            ]
        }

        let installed_ids = HashSet::from(["0100bbbb00000000".to_string()]);
        let legacy = serde_json::to_vec(&build_groups(rows(), &installed_ids)).unwrap();
        let (groups, presence) = build_groups_with_presence(rows(), &installed_ids);

        assert_eq!(serde_json::to_vec(&groups).unwrap(), legacy);
        assert_eq!(
            presence,
            vec![EdenPresenceRecord {
                observed_title_id: "0100BBBB00000000".into(),
                resolved_base_title_id: Some("0100BBBB00000000".into()),
                resolution_issue: None,
            }]
        );
    }
}
