use crate::package_library::GameLibraryScanResult;
use crate::package_metadata::PackageMetadata;

#[cfg(not(target_os = "android"))]
use crate::package_library::{scan_package_library, PackageLibraryEntry};
#[cfg(not(target_os = "android"))]
use crate::package_metadata;
#[cfg(not(target_os = "android"))]
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

#[cfg(not(target_os = "android"))]
const MAX_SCAN_DEPTH: usize = 5;
#[cfg(not(target_os = "android"))]
const MAX_PACKAGES: usize = 2_000;

#[tauri::command]
pub async fn discover_package_metadata_for_title_pc(
    prod_keys_path: String,
    package_path: String,
    expected_base_title_id: String,
) -> Result<PackageMetadata, String> {
    #[cfg(not(target_os = "android"))]
    {
        return tauri::async_runtime::spawn_blocking(move || {
            let prod_keys_path = PathBuf::from(prod_keys_path);
            let package_path = PathBuf::from(package_path);
            let prod_keys = File::open(&prod_keys_path).map_err(|error| {
                format!(
                    "Could not open prod.keys '{}': {error}",
                    prod_keys_path.display()
                )
            })?;
            let package = File::open(&package_path).map_err(|error| {
                format!(
                    "Could not open game package '{}': {error}",
                    package_path.display()
                )
            })?;
            let metadata = package_metadata::discover_package_metadata(prod_keys, package)?;
            package_metadata::validate_package_identity(&expected_base_title_id, metadata)
        })
        .await
        .map_err(|error| format!("Package parser task failed: {error}"))?;
    }

    #[cfg(target_os = "android")]
    {
        let _ = (prod_keys_path, package_path, expected_base_title_id);
        Err("Desktop package discovery is not available on Android.".into())
    }
}

#[tauri::command]
pub async fn scan_game_package_library_pc(
    prod_keys_path: String,
    library_path: String,
) -> Result<GameLibraryScanResult, String> {
    #[cfg(not(target_os = "android"))]
    {
        return tauri::async_runtime::spawn_blocking(move || {
            scan_game_package_library_pc_inner(Path::new(&prod_keys_path), Path::new(&library_path))
        })
        .await
        .map_err(|error| format!("Game-library parser task failed: {error}"))?;
    }

    #[cfg(target_os = "android")]
    {
        let _ = (prod_keys_path, library_path);
        Err("Desktop package-library scanning is not available on Android.".into())
    }
}

#[cfg(not(target_os = "android"))]
fn scan_game_package_library_pc_inner(
    prod_keys_path: &Path,
    library_path: &Path,
) -> Result<GameLibraryScanResult, String> {
    let entries = collect_package_entries(library_path)?;
    let prod_keys = File::open(prod_keys_path).map_err(|error| {
        format!(
            "Could not open prod.keys '{}': {error}",
            prod_keys_path.display()
        )
    })?;

    scan_package_library(prod_keys, entries, |package_path| {
        File::open(package_path).map_err(|error| {
            format!(
                "Could not open game package '{}': {error}",
                package_path.display()
            )
        })
    })
}

#[cfg(not(target_os = "android"))]
fn collect_package_entries(
    library_path: &Path,
) -> Result<Vec<PackageLibraryEntry<PathBuf>>, String> {
    collect_package_entries_with_limit(library_path, MAX_PACKAGES)
}

#[cfg(not(target_os = "android"))]
fn collect_package_entries_with_limit(
    library_path: &Path,
    max_packages: usize,
) -> Result<Vec<PackageLibraryEntry<PathBuf>>, String> {
    let metadata = fs::symlink_metadata(library_path).map_err(|error| {
        format!(
            "Could not inspect game-library directory '{}': {error}",
            library_path.display()
        )
    })?;
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "Game-library path '{}' is not a directory.",
            library_path.display()
        ));
    }

    let mut entries = Vec::new();
    collect_directory(library_path, "", 0, max_packages, &mut entries)?;
    entries.sort_by_cached_key(|entry| {
        (
            entry.relative_path.to_lowercase(),
            entry.relative_path.clone(),
        )
    });
    Ok(entries)
}

#[cfg(not(target_os = "android"))]
fn collect_directory(
    directory: &Path,
    relative_directory: &str,
    depth: usize,
    max_packages: usize,
    packages: &mut Vec<PackageLibraryEntry<PathBuf>>,
) -> Result<(), String> {
    let children = fs::read_dir(directory).map_err(|error| {
        format!(
            "Could not read game-library directory '{}': {error}",
            directory.display()
        )
    })?;

    for child in children {
        let child = child.map_err(|error| {
            format!(
                "Could not read an entry in game-library directory '{}': {error}",
                directory.display()
            )
        })?;
        let path = child.path();
        let file_type = child.file_type().map_err(|error| {
            format!(
                "Could not inspect game-library entry '{}': {error}",
                path.display()
            )
        })?;
        let filename = child.file_name().to_string_lossy().into_owned();
        let relative_path = if relative_directory.is_empty() {
            filename.clone()
        } else {
            format!("{relative_directory}/{filename}")
        };

        if file_type.is_dir() {
            if depth < MAX_SCAN_DEPTH {
                collect_directory(&path, &relative_path, depth + 1, max_packages, packages)?;
            }
            continue;
        }
        if !file_type.is_file() || !is_package_path(&path) {
            continue;
        }
        if packages.len() >= max_packages {
            return Err(format!(
                "The selected library contains more than {max_packages} packages"
            ));
        }

        let size = child
            .metadata()
            .map_err(|error| {
                format!(
                    "Could not inspect game package '{}': {error}",
                    path.display()
                )
            })?
            .len();
        packages.push(PackageLibraryEntry {
            source: path,
            filename,
            relative_path,
            size,
        });
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn is_package_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("nsp") || extension.eq_ignore_ascii_case("xci")
        })
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "eden-cheats-manager-library-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative_path: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn collects_supported_files_with_sorted_relative_provenance() {
        let temp = TestDirectory::new();
        let a = temp.write("a.NSP", b"a");
        let nested = temp.write("nested/B.nsp", b"bbb");
        let z = temp.write("Z.xCi", b"zz");
        temp.write("ignored.zip", b"ignored");

        let entries = collect_package_entries(temp.path()).unwrap();

        assert_eq!(
            entries
                .iter()
                .map(|entry| (
                    entry.filename.as_str(),
                    entry.relative_path.as_str(),
                    entry.size,
                    entry.source.as_path(),
                ))
                .collect::<Vec<_>>(),
            vec![
                ("a.NSP", "a.NSP", 1, a.as_path()),
                ("B.nsp", "nested/B.nsp", 3, nested.as_path()),
                ("Z.xCi", "Z.xCi", 2, z.as_path()),
            ]
        );
    }

    #[test]
    fn includes_depth_five_and_skips_depth_six() {
        let temp = TestDirectory::new();
        temp.write("1/2/3/4/5/included.nsp", b"yes");
        temp.write("1/2/3/4/5/6/excluded.nsp", b"no");

        let entries = collect_package_entries(temp.path()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].relative_path, "1/2/3/4/5/included.nsp");
    }

    #[test]
    fn rejects_the_first_package_beyond_the_limit() {
        let temp = TestDirectory::new();
        temp.write("one.nsp", b"1");
        temp.write("two.nsp", b"2");
        temp.write("three.nsp", b"3");

        assert_eq!(
            collect_package_entries_with_limit(temp.path(), 2)
                .err()
                .unwrap(),
            "The selected library contains more than 2 packages"
        );
    }

    #[test]
    fn rejects_missing_and_non_directory_roots() {
        let temp = TestDirectory::new();
        let file = temp.write("library.nsp", b"file");
        assert!(collect_package_entries(&file)
            .err()
            .unwrap()
            .contains("is not a directory"));
        assert!(collect_package_entries(&temp.path().join("missing"))
            .err()
            .unwrap()
            .contains("Could not inspect game-library directory"));
    }

    #[cfg(unix)]
    #[test]
    fn skips_file_and_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = TestDirectory::new();
        let package = temp.write("real/base.nsp", b"base");
        symlink(&package, temp.path().join("linked.nsp")).unwrap();
        symlink(temp.path().join("real"), temp.path().join("linked-dir")).unwrap();

        let entries = collect_package_entries(temp.path()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].relative_path, "real/base.nsp");
        assert_eq!(entries[0].source, package);
    }
}
