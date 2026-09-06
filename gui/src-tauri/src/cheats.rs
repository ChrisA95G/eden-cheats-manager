use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledCheat {
    pub cheat_name: String,
    pub build_id: String,
}

pub(crate) fn validate_hex_id(value: &str) -> Result<(), String> {
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Title ID and Build ID must be 16 hexadecimal characters.".into());
    }
    Ok(())
}

pub(crate) fn validate_cheat_target(title: &str, name: &str, build: &str) -> Result<(), String> {
    validate_hex_id(title)?;
    validate_hex_id(build)?;
    if name.trim().is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\', ':', '\0'])
    {
        return Err("Cheat name must be a single folder name.".into());
    }
    Ok(())
}

// The selected root may itself be a user-selected link. Children must not redirect
// file operations outside it. This is not a lock against concurrent local changes.
fn checked_path(root: &str, parts: &[&str]) -> Result<PathBuf, String> {
    if root.trim().is_empty() || !std::path::Path::new(root).is_dir() {
        return Err("Select an existing Eden load directory in Settings.".into());
    }
    let mut path = PathBuf::from(root);
    for part in parts {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("Cheat paths must not contain symbolic links.".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(path)
}

// ── PC ────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn install_cheat_pc(
    load_dir: String,
    title_id: String,
    cheat_name: String,
    build_id: String,
    content: String,
) -> Result<(), String> {
    validate_cheat_target(&title_id, &cheat_name, &build_id)?;
    let file = checked_path(
        &load_dir,
        &[&title_id, &cheat_name, "cheats", &format!("{build_id}.txt")],
    )?;
    log::info!("[cheats] install_pc title={title_id} build={build_id} name={cheat_name}");
    let cheats_dir = file.parent().unwrap();
    std::fs::create_dir_all(&cheats_dir).map_err(|e| e.to_string())?;
    std::fs::write(&file, content.as_bytes()).map_err(|e| e.to_string())?;
    log::info!("[cheats] install_pc OK: {}", file.display());
    Ok(())
}

#[tauri::command]
pub fn list_installed_cheats_pc(
    load_dir: String,
    title_id: String,
) -> Result<Vec<InstalledCheat>, String> {
    validate_hex_id(&title_id)?;
    log::debug!("[cheats] list_installed_pc title={title_id} load_dir={load_dir}");
    let title_dir = checked_path(&load_dir, &[&title_id])?;
    if !title_dir.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(&title_dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let cheat_name = entry.file_name().to_string_lossy().to_string();
        let cheats_dir = checked_path(&load_dir, &[&title_id, &cheat_name, "cheats"])?;
        if cheats_dir.is_dir() {
            for f in std::fs::read_dir(&cheats_dir)
                .map_err(|e| e.to_string())?
                .flatten()
            {
                let fname = f.file_name().to_string_lossy().to_string();
                if fname.ends_with(".txt") {
                    let build_id = fname.strip_suffix(".txt").unwrap().to_string();
                    if validate_cheat_target(&title_id, &cheat_name, &build_id).is_err() {
                        continue;
                    }
                    checked_path(&load_dir, &[&title_id, &cheat_name, "cheats", &fname])?;
                    log::debug!("[cheats] found pc installed: {cheat_name}/{build_id}");
                    result.push(InstalledCheat {
                        cheat_name: cheat_name.clone(),
                        build_id,
                    });
                }
            }
        }
    }
    log::info!("[cheats] list_installed_pc -> {} entries", result.len());
    Ok(result)
}

#[tauri::command]
pub fn delete_cheat_pc(
    load_dir: String,
    title_id: String,
    cheat_name: String,
    build_id: String,
) -> Result<(), String> {
    validate_cheat_target(&title_id, &cheat_name, &build_id)?;
    log::info!("[cheats] delete_pc title={title_id} build={build_id} name={cheat_name}");
    let file = checked_path(
        &load_dir,
        &[&title_id, &cheat_name, "cheats", &format!("{build_id}.txt")],
    )?;
    match std::fs::remove_file(&file) {
        Ok(()) => {
            log::info!("[cheats] delete_pc OK: {}", file.display());
            let cheats_dir = file.parent().unwrap();
            let _ = std::fs::remove_dir(cheats_dir);
            let _ = std::fs::remove_dir(cheats_dir.parent().unwrap());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const TITLE: &str = "01007300020FA000";
    const BUILD: &str = "4b159f0f7a360669";

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "ecm-cheat-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn root(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_unsafe_components_before_file_operations() {
        let fixture = Fixture::new();
        for name in [
            "",
            " ",
            ".",
            "..",
            "../escape",
            "/absolute",
            "C:\\escape",
            "foo/bar",
            "bad\0name",
        ] {
            assert!(install_cheat_pc(
                fixture.root(),
                TITLE.into(),
                name.into(),
                BUILD.into(),
                "test".into()
            )
            .is_err());
            assert!(
                delete_cheat_pc(fixture.root(), TITLE.into(), name.into(), BUILD.into()).is_err()
            );
        }
        for id in [
            "",
            "../escape",
            "/absolute",
            "01007300020FA00Z",
            "01007300020FA0000",
        ] {
            assert!(validate_cheat_target(id, "Valid", BUILD).is_err());
            assert!(validate_cheat_target(TITLE, "Valid", id).is_err());
            assert!(list_installed_cheats_pc(fixture.root(), id.into()).is_err());
        }
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 0);
    }

    #[test]
    fn install_list_replace_delete_preserves_filename_case_and_other_files() {
        let fixture = Fixture::new();
        let name = "Max Money (Custom)";
        let write = |content: &str| {
            install_cheat_pc(
                fixture.root(),
                TITLE.into(),
                name.into(),
                BUILD.into(),
                content.into(),
            )
        };
        write("first").unwrap();
        write("replacement").unwrap();
        let dir = fixture.0.join(TITLE).join(name).join("cheats");
        std::fs::write(dir.join("notes.txt"), "keep").unwrap();
        let files = list_installed_cheats_pc(fixture.root(), TITLE.into()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].build_id, BUILD);
        assert_eq!(
            std::fs::read_to_string(dir.join(format!("{BUILD}.txt"))).unwrap(),
            "replacement"
        );
        delete_cheat_pc(
            fixture.root(),
            TITLE.into(),
            files[0].cheat_name.clone(),
            files[0].build_id.clone(),
        )
        .unwrap();
        assert!(!dir.join(format!("{BUILD}.txt")).exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("notes.txt")).unwrap(),
            "keep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_children_without_touching_destination() {
        let fixture = Fixture::new();
        let outside = Fixture::new();
        std::os::unix::fs::symlink(&outside.0, fixture.0.join(TITLE)).unwrap();
        assert!(install_cheat_pc(
            fixture.root(),
            TITLE.into(),
            "Name".into(),
            BUILD.into(),
            "test".into()
        )
        .is_err());
        assert!(list_installed_cheats_pc(fixture.root(), TITLE.into()).is_err());
        assert!(
            delete_cheat_pc(fixture.root(), TITLE.into(), "Name".into(), BUILD.into()).is_err()
        );
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
    }
}
