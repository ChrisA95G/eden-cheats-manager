use crate::{
    games::EdenPresenceRecord,
    package_metadata::{self, PackageMetadata},
};
use serde::Serialize;
use std::{collections::BTreeMap, fs::File};

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenPackageCorrelationEntry {
    pub observed_title_id: String,
    pub resolved_base_title_id: Option<String>,
    pub package_candidates: Vec<GameVersionPackage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenPackageCorrelationIssue {
    pub observed_title_id: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdenPackageCorrelationResult {
    pub scanned_packages: usize,
    pub matched_packages: usize,
    pub skipped_packages: usize,
    pub eden_entries: Vec<EdenPackageCorrelationEntry>,
    pub unmatched_package_groups: Vec<GameVersionGroup>,
    pub package_scan_errors: Vec<GameLibraryScanError>,
    pub correlation_issues: Vec<EdenPackageCorrelationIssue>,
}

pub(crate) struct PackageLibraryEntry<S> {
    pub(crate) source: S,
    pub(crate) filename: String,
    pub(crate) relative_path: String,
    pub(crate) size: u64,
}

pub(crate) fn scan_package_library<S, F>(
    prod_keys: File,
    entries: Vec<PackageLibraryEntry<S>>,
    mut open_package: F,
) -> Result<GameLibraryScanResult, String>
where
    F: FnMut(&S) -> Result<File, String>,
{
    let keys = package_metadata::load_package_keys(prod_keys)?;
    Ok(scan_entries(entries, |source| {
        open_package(source).and_then(|package| {
            package_metadata::discover_package_metadata_with_keys(&keys, package)
        })
    }))
}

fn scan_entries<S, F>(
    entries: Vec<PackageLibraryEntry<S>>,
    mut parse_package: F,
) -> GameLibraryScanResult
where
    F: FnMut(&S) -> Result<PackageMetadata, String>,
{
    let scanned_packages = entries.len();
    let mut matched = Vec::new();
    let mut skipped_packages = 0;
    let mut errors = Vec::new();

    for entry in entries {
        match parse_package(&entry.source) {
            Ok(metadata) => matched.push(version_package(entry, metadata)),
            Err(message) if package_metadata::is_package_without_build_id(&message) => {
                skipped_packages += 1;
            }
            Err(message) => errors.push(GameLibraryScanError {
                filename: entry.filename,
                relative_path: entry.relative_path,
                message,
            }),
        }
    }

    let matched_packages = matched.len();
    GameLibraryScanResult {
        scanned_packages,
        matched_packages,
        skipped_packages,
        games: group_versions(matched),
        errors,
    }
}

fn version_package<S>(
    entry: PackageLibraryEntry<S>,
    metadata: PackageMetadata,
) -> GameVersionPackage {
    GameVersionPackage {
        content_kind: metadata.content_kind,
        title_id: metadata.title_id,
        base_title_id: metadata.base_title_id,
        version: metadata.version,
        build_id: metadata.build_id,
        module_id: metadata.module_id,
        package_format: metadata.package_format,
        filename: entry.filename,
        relative_path: entry.relative_path,
        size: entry.size,
    }
}

pub(crate) fn group_versions(packages: Vec<GameVersionPackage>) -> Vec<GameVersionGroup> {
    let mut groups: BTreeMap<String, Vec<GameVersionPackage>> = BTreeMap::new();
    for package in packages {
        groups
            .entry(package.base_title_id.clone())
            .or_default()
            .push(package);
    }

    groups
        .into_iter()
        .map(|(base_title_id, mut versions)| {
            versions.sort_by(|left, right| {
                let left_kind = if left.content_kind == "application" {
                    0
                } else {
                    1
                };
                let right_kind = if right.content_kind == "application" {
                    0
                } else {
                    1
                };
                (left_kind, left.version, &left.filename).cmp(&(
                    right_kind,
                    right.version,
                    &right.filename,
                ))
            });
            GameVersionGroup {
                base_title_id,
                versions,
            }
        })
        .collect()
}

#[allow(dead_code)] // Consumed by the separately deferred frontend/backend orchestration.
pub(crate) fn correlate_eden_package_inventory(
    mut presence: Vec<EdenPresenceRecord>,
    packages: GameLibraryScanResult,
) -> EdenPackageCorrelationResult {
    presence.sort_by(|left, right| left.observed_title_id.cmp(&right.observed_title_id));

    let GameLibraryScanResult {
        scanned_packages,
        matched_packages,
        skipped_packages,
        games,
        errors,
    } = packages;
    let mut package_groups: BTreeMap<String, Vec<GameVersionPackage>> = BTreeMap::new();
    for group in games {
        package_groups
            .entry(group.base_title_id)
            .or_default()
            .extend(group.versions);
    }

    let mut matched_base_title_ids = std::collections::BTreeSet::new();
    let mut eden_entries = Vec::with_capacity(presence.len());
    let mut correlation_issues = Vec::new();
    for record in presence {
        let EdenPresenceRecord {
            observed_title_id,
            resolved_base_title_id,
            resolution_issue,
        } = record;
        let package_candidates = resolved_base_title_id
            .as_ref()
            .and_then(|base_title_id| {
                package_groups.get(base_title_id).map(|versions| {
                    matched_base_title_ids.insert(base_title_id.clone());
                    versions.clone()
                })
            })
            .unwrap_or_default();

        if resolved_base_title_id.is_none() {
            correlation_issues.push(EdenPackageCorrelationIssue {
                observed_title_id: observed_title_id.clone(),
                message: resolution_issue.unwrap_or_else(|| {
                    format!(
                        "No authoritative base Title ID was found for observed Eden title {observed_title_id}."
                    )
                }),
            });
        }
        eden_entries.push(EdenPackageCorrelationEntry {
            observed_title_id,
            resolved_base_title_id,
            package_candidates,
        });
    }

    let unmatched_package_groups = package_groups
        .into_iter()
        .filter(|(base_title_id, _)| !matched_base_title_ids.contains(base_title_id))
        .map(|(base_title_id, versions)| GameVersionGroup {
            base_title_id,
            versions,
        })
        .collect();

    EdenPackageCorrelationResult {
        scanned_packages,
        matched_packages,
        skipped_packages,
        eden_entries,
        unmatched_package_groups,
        package_scan_errors: errors,
        correlation_issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_metadata::NO_BUILD_ID_CONTENT_ERROR;

    fn entry(source: &'static str, filename: &str, size: u64) -> PackageLibraryEntry<&'static str> {
        PackageLibraryEntry {
            source,
            filename: filename.into(),
            relative_path: format!("games/{filename}"),
            size,
        }
    }

    fn metadata(content_kind: &str, title_id: &str, version: u32) -> PackageMetadata {
        PackageMetadata {
            package_format: "NSP".into(),
            content_kind: content_kind.into(),
            title_id: title_id.into(),
            base_title_id: "0100000000002000".into(),
            program_title_id: title_id.into(),
            version,
            build_id: format!("BUILD{version}"),
            module_id: format!("MODULE{version}"),
            has_bktr: content_kind == "patch",
            matched_program_content_id: true,
        }
    }

    fn package(
        base_title_id: &str,
        title_id: &str,
        content_kind: &str,
        version: u32,
        filename: &str,
        size: u64,
    ) -> GameVersionPackage {
        GameVersionPackage {
            content_kind: content_kind.into(),
            title_id: title_id.into(),
            base_title_id: base_title_id.into(),
            version,
            build_id: format!("BUILD{version}"),
            module_id: format!("MODULE{version}"),
            package_format: "NSP".into(),
            filename: filename.into(),
            relative_path: format!("games/{filename}"),
            size,
        }
    }

    #[test]
    fn scan_entries_isolates_errors_and_preserves_counts_and_provenance() {
        let entries = vec![
            entry("open-error", "broken.nsp", 10),
            entry("application", "base.nsp", 20),
            entry("skip", "dlc.nsp", 30),
            entry("near-skip", "almost-dlc.nsp", 40),
            entry("patch", "update.nsp", 50),
        ];
        let mut visited = Vec::new();

        let result = scan_entries(entries, |source| {
            visited.push(*source);
            match *source {
                "open-error" => Err("could not open package".into()),
                "application" => Ok(metadata("application", "0100000000002000", 9)),
                "skip" => Err(NO_BUILD_ID_CONTENT_ERROR.into()),
                "near-skip" => Err(format!("{NO_BUILD_ID_CONTENT_ERROR} ")),
                "patch" => Ok(metadata("patch", "0100000000002800", 1)),
                _ => unreachable!(),
            }
        });

        assert_eq!(
            visited,
            vec!["open-error", "application", "skip", "near-skip", "patch"]
        );
        assert_eq!(result.scanned_packages, 5);
        assert_eq!(result.matched_packages, 2);
        assert_eq!(result.skipped_packages, 1);
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.errors[0].filename, "broken.nsp");
        assert_eq!(result.errors[0].relative_path, "games/broken.nsp");
        assert_eq!(result.errors[0].message, "could not open package");
        assert_eq!(result.errors[1].filename, "almost-dlc.nsp");
        assert_eq!(result.errors[1].relative_path, "games/almost-dlc.nsp");
        assert_eq!(
            result.errors[1].message,
            format!("{NO_BUILD_ID_CONTENT_ERROR} ")
        );
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].base_title_id, "0100000000002000");
        assert_eq!(
            result.games[0]
                .versions
                .iter()
                .map(|package| (
                    package.content_kind.as_str(),
                    package.title_id.as_str(),
                    package.base_title_id.as_str(),
                    package.version,
                    package.build_id.as_str(),
                    package.module_id.as_str(),
                    package.package_format.as_str(),
                    package.filename.as_str(),
                    package.relative_path.as_str(),
                    package.size,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "application",
                    "0100000000002000",
                    "0100000000002000",
                    9,
                    "BUILD9",
                    "MODULE9",
                    "NSP",
                    "base.nsp",
                    "games/base.nsp",
                    20,
                ),
                (
                    "patch",
                    "0100000000002800",
                    "0100000000002000",
                    1,
                    "BUILD1",
                    "MODULE1",
                    "NSP",
                    "update.nsp",
                    "games/update.nsp",
                    50,
                ),
            ]
        );
    }

    #[test]
    fn correlation_preserves_both_inventories_and_matches_only_exact_base_ids() {
        let presence = vec![
            EdenPresenceRecord {
                observed_title_id: "0100DDDD00000800".into(),
                resolved_base_title_id: None,
                resolution_issue: Some(
                    "Multiple authoritative base Title IDs match observed Eden title 0100DDDD00000800."
                        .into(),
                ),
            },
            EdenPresenceRecord {
                observed_title_id: "0100CCCC00000000".into(),
                resolved_base_title_id: None,
                resolution_issue: Some(
                    "No authoritative base Title ID was found for observed Eden title 0100CCCC00000000."
                        .into(),
                ),
            },
            EdenPresenceRecord {
                observed_title_id: "0100BBBB00000000".into(),
                resolved_base_title_id: Some("0100BBBB00000000".into()),
                resolution_issue: None,
            },
            EdenPresenceRecord {
                observed_title_id: "0100AAAA00000800".into(),
                resolved_base_title_id: Some("0100AAAA00000000".into()),
                resolution_issue: None,
            },
        ];
        let packages = GameLibraryScanResult {
            scanned_packages: 6,
            matched_packages: 4,
            skipped_packages: 1,
            games: vec![
                GameVersionGroup {
                    base_title_id: "0100CCCC00000000".into(),
                    versions: vec![package(
                        "0100CCCC00000000",
                        "0100CCCC00000000",
                        "application",
                        4,
                        "c.nsp",
                        40,
                    )],
                },
                GameVersionGroup {
                    base_title_id: "0100AAAA00001000".into(),
                    versions: vec![package(
                        "0100AAAA00001000",
                        "0100AAAA00001000",
                        "application",
                        3,
                        "near.nsp",
                        30,
                    )],
                },
                GameVersionGroup {
                    base_title_id: "0100AAAA00000000".into(),
                    versions: vec![
                        package(
                            "0100AAAA00000000",
                            "0100AAAA00000000",
                            "application",
                            9,
                            "base.nsp",
                            10,
                        ),
                        package(
                            "0100AAAA00000000",
                            "0100AAAA00000800",
                            "patch",
                            1,
                            "update.nsp",
                            20,
                        ),
                    ],
                },
            ],
            errors: vec![GameLibraryScanError {
                filename: "broken.nsp".into(),
                relative_path: "games/broken.nsp".into(),
                message: "could not parse package".into(),
            }],
        };

        let result = correlate_eden_package_inventory(presence, packages);

        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "scannedPackages": 6,
                "matchedPackages": 4,
                "skippedPackages": 1,
                "edenEntries": [
                    {
                        "observedTitleId": "0100AAAA00000800",
                        "resolvedBaseTitleId": "0100AAAA00000000",
                        "packageCandidates": [
                            {
                                "contentKind": "application",
                                "titleId": "0100AAAA00000000",
                                "baseTitleId": "0100AAAA00000000",
                                "version": 9,
                                "buildId": "BUILD9",
                                "moduleId": "MODULE9",
                                "packageFormat": "NSP",
                                "filename": "base.nsp",
                                "relativePath": "games/base.nsp",
                                "size": 10,
                            },
                            {
                                "contentKind": "patch",
                                "titleId": "0100AAAA00000800",
                                "baseTitleId": "0100AAAA00000000",
                                "version": 1,
                                "buildId": "BUILD1",
                                "moduleId": "MODULE1",
                                "packageFormat": "NSP",
                                "filename": "update.nsp",
                                "relativePath": "games/update.nsp",
                                "size": 20,
                            }
                        ]
                    },
                    {
                        "observedTitleId": "0100BBBB00000000",
                        "resolvedBaseTitleId": "0100BBBB00000000",
                        "packageCandidates": []
                    },
                    {
                        "observedTitleId": "0100CCCC00000000",
                        "resolvedBaseTitleId": null,
                        "packageCandidates": []
                    },
                    {
                        "observedTitleId": "0100DDDD00000800",
                        "resolvedBaseTitleId": null,
                        "packageCandidates": []
                    }
                ],
                "unmatchedPackageGroups": [
                    {
                        "baseTitleId": "0100AAAA00001000",
                        "versions": [{
                            "contentKind": "application",
                            "titleId": "0100AAAA00001000",
                            "baseTitleId": "0100AAAA00001000",
                            "version": 3,
                            "buildId": "BUILD3",
                            "moduleId": "MODULE3",
                            "packageFormat": "NSP",
                            "filename": "near.nsp",
                            "relativePath": "games/near.nsp",
                            "size": 30,
                        }]
                    },
                    {
                        "baseTitleId": "0100CCCC00000000",
                        "versions": [{
                            "contentKind": "application",
                            "titleId": "0100CCCC00000000",
                            "baseTitleId": "0100CCCC00000000",
                            "version": 4,
                            "buildId": "BUILD4",
                            "moduleId": "MODULE4",
                            "packageFormat": "NSP",
                            "filename": "c.nsp",
                            "relativePath": "games/c.nsp",
                            "size": 40,
                        }]
                    }
                ],
                "packageScanErrors": [{
                    "filename": "broken.nsp",
                    "relativePath": "games/broken.nsp",
                    "message": "could not parse package",
                }],
                "correlationIssues": [{
                    "observedTitleId": "0100CCCC00000000",
                    "message": "No authoritative base Title ID was found for observed Eden title 0100CCCC00000000."
                }, {
                    "observedTitleId": "0100DDDD00000800",
                    "message": "Multiple authoritative base Title IDs match observed Eden title 0100DDDD00000800."
                }]
            })
        );
    }
}
