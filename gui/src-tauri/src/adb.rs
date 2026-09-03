use regex::Regex;
use std::sync::OnceLock;

pub(crate) const EDEN_VIRTUAL_DIRS: &[&str] = &["SDMC", "UserNAND", "SysNAND"];

static LOADER_BUILD_ID_RE: OnceLock<Regex> = OnceLock::new();

pub(crate) fn loader_build_id_re() -> &'static Regex {
    LOADER_BUILD_ID_RE.get_or_init(|| {
        Regex::new(r"build_id=([A-Fa-f0-9]{16,64}),\s*name=main").unwrap()
    })
}

#[tauri::command]
pub fn extract_build_ids_pc(load_dir: String, _title_id: String) -> Result<Vec<String>, String> {
    let load_path = std::path::PathBuf::from(&load_dir);
    // Log is typically at the sibling `log/` directory next to `load/`
    let base = load_path.parent().unwrap_or(load_path.as_path());
    let candidates = [
        base.join("log/eden_log.txt"),
        base.join("log/eden_log.txt.old.txt"),
        base.join("eden_log.txt"),
    ];
    for log_path in &candidates {
        if let Ok(text) = std::fs::read_to_string(log_path) {
            let ids = parse_build_ids(&text);
            if !ids.is_empty() {
                return Ok(ids);
            }
        }
    }
    Err("No Eden log found with build IDs. Launch Eden and run a game first.".to_string())
}

pub fn parse_build_ids(text: &str) -> Vec<String> {
    let re = loader_build_id_re();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in re.captures_iter(text) {
        let full = cap[1].to_string();
        let bid = full[..16.min(full.len())].to_uppercase();
        if seen.insert(bid.clone()) {
            out.push(bid);
        }
    }
    out
}
