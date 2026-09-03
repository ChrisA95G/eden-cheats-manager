use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersionPackage {
    pub content_kind: String,
    pub title_id: String,
    pub base_title_id: String,
    pub version: u32,
    pub build_id: String,
    pub module_id: String,
    pub package_format: String,
    pub filename: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersionGroup {
    pub base_title_id: String,
    pub versions: Vec<GameVersionPackage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLibraryScanError {
    pub filename: String,
    pub relative_path: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLibraryScanResult {
    pub scanned_packages: usize,
    pub matched_packages: usize,
    pub skipped_packages: usize,
    pub games: Vec<GameVersionGroup>,
    pub errors: Vec<GameLibraryScanError>,
}
