use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "android", test))]
use std::collections::{BTreeMap, HashMap};

#[cfg(target_os = "android")]
use super::jni::{
    jni_noarg_string_call, parse_saf_response, select_eden_root_directory_from_activity,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenRootAccessStatus {
    pub selected: bool,
    pub valid_location: bool,
    pub read_permission: bool,
    pub readable: bool,
    pub ready: bool,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenDirectoryEntry {
    pub name: String,
    pub directory: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenDirectoryProbe {
    pub path: String,
    pub exists: bool,
    pub entries: Vec<EdenDirectoryEntry>,
    pub truncated: bool,
    pub error: Option<String>,
}

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdenDiscoverySnapshot {
    status: EdenRootAccessStatus,
    config_ini: Option<String>,
    config_error: Option<String>,
    directories: Vec<EdenDirectoryProbe>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EdenConfiguredGameDirectory {
    pub path: String,
    pub deep_scan: Option<bool>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EdenDisabledAddOns {
    pub title_id: String,
    pub raw_title_id: String,
    pub disabled: Vec<String>,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EdenConfiguredPaths {
    pub nand_directory: Option<String>,
    pub sdmc_directory: Option<String>,
    pub save_directory: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenDiscoveryReport {
    pub status: EdenRootAccessStatus,
    pub config_found: bool,
    pub config_error: Option<String>,
    pub game_directories: Vec<EdenConfiguredGameDirectory>,
    pub external_content_directories: Vec<String>,
    pub disabled_add_ons: Vec<EdenDisabledAddOns>,
    pub configured_paths: EdenConfiguredPaths,
    pub directories: Vec<EdenDirectoryProbe>,
}

#[cfg(any(target_os = "android", test))]
#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedEdenConfig {
    game_directories: Vec<EdenConfiguredGameDirectory>,
    external_content_directories: Vec<String>,
    disabled_add_ons: Vec<EdenDisabledAddOns>,
    configured_paths: EdenConfiguredPaths,
}

/// Open Android's document-tree picker for a separate, read-only grant to
/// Eden's top-level provider directory.
#[tauri::command]
pub fn select_eden_root_directory() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return select_eden_root_directory_from_activity();
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Eden root selection is only available on Android.".into())
    }
}

/// Read Eden's global config and inspect likely NAND/SDMC content locations.
/// This proof of concept does not modify Eden or ECM's package selections.
#[tauri::command]
pub fn inspect_eden_installation() -> Result<EdenDiscoveryReport, String> {
    #[cfg(target_os = "android")]
    {
        let response = parse_saf_response(jni_noarg_string_call("inspectEdenInstallation")?)?;
        let snapshot: EdenDiscoverySnapshot = serde_json::from_str(&response)
            .map_err(|error| format!("Invalid Eden discovery response: {error}"))?;
        let parsed = snapshot
            .config_ini
            .as_deref()
            .map(parse_eden_config)
            .unwrap_or_default();

        return Ok(EdenDiscoveryReport {
            status: snapshot.status,
            config_found: snapshot.config_ini.is_some(),
            config_error: snapshot.config_error,
            game_directories: parsed.game_directories,
            external_content_directories: parsed.external_content_directories,
            disabled_add_ons: parsed.disabled_add_ons,
            configured_paths: parsed.configured_paths,
            directories: snapshot.directories,
        });
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Eden installation inspection is only available on Android.".into())
    }
}

#[cfg(any(target_os = "android", test))]
fn parse_eden_config(content: &str) -> ParsedEdenConfig {
    let sections = parse_ini_sections(content);
    let paths = collect_path_values(&sections);
    let disabled = sections.get("DisabledAddOns");

    ParsedEdenConfig {
        game_directories: parse_game_directories(&paths),
        external_content_directories: parse_external_content_directories(&paths),
        disabled_add_ons: disabled.map(parse_disabled_add_ons).unwrap_or_default(),
        configured_paths: EdenConfiguredPaths {
            nand_directory: nonempty_value(&paths, "nand_directory"),
            sdmc_directory: nonempty_value(&paths, "sdmc_directory"),
            save_directory: nonempty_value(&paths, "save_directory"),
        },
    }
}

#[cfg(any(target_os = "android", test))]
fn collect_path_values(
    sections: &HashMap<String, BTreeMap<String, String>>,
) -> BTreeMap<String, String> {
    let mut paths = BTreeMap::new();
    for (section, values) in sections {
        if section.eq_ignore_ascii_case("Paths") {
            paths.extend(values.clone());
        }
        for (key, value) in values {
            if let Some(path_key) = key.strip_prefix("Paths\\") {
                paths.insert(path_key.to_string(), value.clone());
            }
        }
        if section.eq_ignore_ascii_case("Data%20Storage")
            || section.eq_ignore_ascii_case("Data Storage")
        {
            for key in ["nand_directory", "sdmc_directory", "save_directory"] {
                if let Some(value) = values.get(key) {
                    paths
                        .entry(key.to_string())
                        .or_insert_with(|| value.clone());
                }
            }
        }
    }
    paths
}

#[cfg(any(target_os = "android", test))]
fn parse_ini_sections(content: &str) -> HashMap<String, BTreeMap<String, String>> {
    let mut sections: HashMap<String, BTreeMap<String, String>> = HashMap::new();
    let mut current_section = String::new();

    for raw_line in content.trim_start_matches('\u{feff}').lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        sections
            .entry(current_section.clone())
            .or_default()
            .insert(key.trim().to_string(), normalize_ini_value(value));
    }

    sections
}

#[cfg(any(target_os = "android", test))]
fn normalize_ini_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

#[cfg(any(target_os = "android", test))]
fn parse_game_directories(values: &BTreeMap<String, String>) -> Vec<EdenConfiguredGameDirectory> {
    let mut indexed: BTreeMap<u32, EdenConfiguredGameDirectory> = BTreeMap::new();
    for (key, value) in values {
        let parts: Vec<&str> = key.split('\\').collect();
        if parts.len() != 3 || parts[0] != "gamedirs" {
            continue;
        }
        let Ok(index) = parts[1].parse::<u32>() else {
            continue;
        };
        let entry = indexed
            .entry(index)
            .or_insert_with(|| EdenConfiguredGameDirectory {
                path: String::new(),
                deep_scan: None,
            });
        match parts[2] {
            "path" => entry.path = value.clone(),
            "deep_scan" => entry.deep_scan = parse_bool(value),
            _ => {}
        }
    }
    indexed
        .into_values()
        .filter(|directory| !directory.path.is_empty())
        .collect()
}

#[cfg(any(target_os = "android", test))]
fn parse_external_content_directories(values: &BTreeMap<String, String>) -> Vec<String> {
    let mut indexed = BTreeMap::new();
    for (key, value) in values {
        let parts: Vec<&str> = key.split('\\').collect();
        if parts.len() != 3 || parts[0] != "external_content_dirs" || parts[2] != "path" {
            continue;
        }
        let Ok(index) = parts[1].parse::<u32>() else {
            continue;
        };
        if !value.is_empty() {
            indexed.insert(index, value.clone());
        }
    }
    indexed.into_values().collect()
}

#[cfg(any(target_os = "android", test))]
fn parse_disabled_add_ons(values: &BTreeMap<String, String>) -> Vec<EdenDisabledAddOns> {
    #[derive(Default)]
    struct PendingAddOns {
        raw_title_id: String,
        disabled: BTreeMap<u32, String>,
    }

    let mut indexed: BTreeMap<u32, PendingAddOns> = BTreeMap::new();
    for (key, value) in values {
        let parts: Vec<&str> = key.split('\\').collect();
        let Some(index) = parts.first().and_then(|part| part.parse::<u32>().ok()) else {
            continue;
        };
        match parts.as_slice() {
            [_, "title_id"] => indexed.entry(index).or_default().raw_title_id = value.clone(),
            [_, "disabled", disabled_index, "d"] => {
                if let Ok(disabled_index) = disabled_index.parse::<u32>() {
                    indexed
                        .entry(index)
                        .or_default()
                        .disabled
                        .insert(disabled_index, value.clone());
                }
            }
            _ => {}
        }
    }

    indexed
        .into_values()
        .filter(|entry| !entry.raw_title_id.is_empty())
        .map(|entry| EdenDisabledAddOns {
            title_id: normalize_title_id(&entry.raw_title_id),
            raw_title_id: entry.raw_title_id,
            disabled: entry.disabled.into_values().collect(),
        })
        .collect()
}

#[cfg(any(target_os = "android", test))]
fn normalize_title_id(value: &str) -> String {
    let trimmed = value.trim();
    let parsed = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .or_else(|| trimmed.parse::<u64>().ok())
        .or_else(|| u64::from_str_radix(trimmed, 16).ok());
    parsed
        .map(|title_id| format!("{title_id:016X}"))
        .unwrap_or_else(|| trimmed.to_uppercase())
}

#[cfg(any(target_os = "android", test))]
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(any(target_os = "android", test))]
fn nonempty_value(values: &BTreeMap<String, String>, key: &str) -> Option<String> {
    values.get(key).filter(|value| !value.is_empty()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_eden_paths_and_disabled_add_ons() {
        let config = r#"
[Paths]
gamedirs\size=2
gamedirs\1\path=content://games/primary
gamedirs\1\deep_scan=true
gamedirs\2\path=content://games/archive
gamedirs\2\deep_scan=false
external_content_dirs\size=1
external_content_dirs\1\path=content://updates
nand_directory=/eden/nand
sdmc_directory=/eden/sdmc
save_directory=/eden/save

[DisabledAddOns]
size=1
1\title_id=72057594037927936
1\disabled\size=2
1\disabled\1\d=Update@65536
1\disabled\2\d=DLC Pack
"#;

        let parsed = parse_eden_config(config);

        assert_eq!(
            parsed.game_directories,
            vec![
                EdenConfiguredGameDirectory {
                    path: "content://games/primary".into(),
                    deep_scan: Some(true),
                },
                EdenConfiguredGameDirectory {
                    path: "content://games/archive".into(),
                    deep_scan: Some(false),
                },
            ]
        );
        assert_eq!(
            parsed.external_content_directories,
            vec!["content://updates".to_string()]
        );
        assert_eq!(
            parsed.configured_paths,
            EdenConfiguredPaths {
                nand_directory: Some("/eden/nand".into()),
                sdmc_directory: Some("/eden/sdmc".into()),
                save_directory: Some("/eden/save".into()),
            }
        );
        assert_eq!(parsed.disabled_add_ons.len(), 1);
        assert_eq!(parsed.disabled_add_ons[0].title_id, "0100000000000000");
        assert_eq!(parsed.disabled_add_ons[0].raw_title_id, "72057594037927936");
        assert_eq!(
            parsed.disabled_add_ons[0].disabled,
            vec!["Update@65536".to_string(), "DLC Pack".to_string()]
        );
    }

    #[test]
    fn parses_paths_nested_under_ui_section() {
        let config = r#"
[UI]
Paths\gamedirs\size=1
Paths\gamedirs\1\path="content://games"
Paths\gamedirs\1\deep_scan=true
Paths\external_content_dirs\size=1
Paths\external_content_dirs\1\path=content://updates

[Data%20Storage]
nand_directory=/custom/nand
"#;

        let parsed = parse_eden_config(config);

        assert_eq!(
            parsed.game_directories,
            vec![EdenConfiguredGameDirectory {
                path: "content://games".into(),
                deep_scan: Some(true),
            }]
        );
        assert_eq!(
            parsed.external_content_directories,
            vec!["content://updates".to_string()]
        );
        assert_eq!(
            parsed.configured_paths.nand_directory,
            Some("/custom/nand".into())
        );
    }

    #[test]
    fn ignores_malformed_and_empty_entries() {
        let config = r#"
[Paths]
gamedirs\1\path=
gamedirs\banana\path=ignored
external_content_dirs\2\path=content://updates

[DisabledAddOns]
1\title_id=0x0100ABCD12340000
1\disabled\nope\d=ignored
"#;

        let parsed = parse_eden_config(config);

        assert!(parsed.game_directories.is_empty());
        assert_eq!(
            parsed.external_content_directories,
            vec!["content://updates".to_string()]
        );
        assert_eq!(parsed.disabled_add_ons[0].title_id, "0100ABCD12340000");
        assert!(parsed.disabled_add_ons[0].disabled.is_empty());
    }
}
