use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntegrationError {
    #[error("Garry's Mod root is invalid: expected garrysmod/addons under {0}")]
    InvalidGmodRoot(PathBuf),
    #[error("the link path is not safely contained by the Garry's Mod addons folder")]
    UnsafeLinkPath,
    #[error("the existing path is not a link to this project: {0}")]
    LinkConflict(PathBuf),
    #[error("failed to run Windows junction command: {0}")]
    JunctionCommand(std::io::Error),
    #[error("Windows could not create the junction: {0}")]
    JunctionFailed(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("junctions are only supported on Windows")]
    UnsupportedPlatform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkStatus {
    Unlinked,
    Linked(PathBuf),
    Conflict(PathBuf),
}

pub fn normalize_gmod_root(path: &Path) -> Option<PathBuf> {
    let candidates = [
        path.to_path_buf(),
        path.join("garrysmod"),
        path.join("GarrysMod").join("garrysmod"),
    ];
    candidates
        .into_iter()
        .find(|candidate| candidate.join("addons").is_dir())
}

pub fn detect_garrys_mod() -> Vec<PathBuf> {
    let mut steam_roots = BTreeSet::new();

    #[cfg(windows)]
    {
        use winreg::{
            RegKey,
            enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
        };
        for (hive, subkey) in [
            (HKEY_CURRENT_USER, r"Software\Valve\Steam"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
        ] {
            if let Ok(key) = RegKey::predef(hive).open_subkey(subkey) {
                for value_name in ["SteamPath", "InstallPath"] {
                    if let Ok(value) = key.get_value::<String, _>(value_name) {
                        steam_roots.insert(PathBuf::from(value.replace('/', "\\")));
                    }
                }
            }
        }
    }

    for path in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        let path = PathBuf::from(path);
        if path.is_dir() {
            steam_roots.insert(path);
        }
    }

    let mut library_roots = steam_roots.clone();
    for root in steam_roots {
        let library_file = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(library_file) {
            library_roots.extend(parse_steam_library_paths(&text));
        }
    }

    let mut results = library_roots
        .into_iter()
        .map(|root| {
            root.join("steamapps")
                .join("common")
                .join("GarrysMod")
                .join("garrysmod")
        })
        .filter(|path| path.join("addons").is_dir())
        .collect::<Vec<_>>();
    results.sort();
    results.dedup();
    results
}

pub fn parse_steam_library_paths(text: &str) -> Vec<PathBuf> {
    let quoted = text
        .split('"')
        .enumerate()
        .filter_map(|(index, value)| (index % 2 == 1).then_some(value))
        .collect::<Vec<_>>();
    let mut paths = BTreeSet::new();
    for window in quoted.windows(2) {
        if window[0].eq_ignore_ascii_case("path") {
            paths.insert(PathBuf::from(
                window[1].replace("\\\\", "\\").replace('/', "\\"),
            ));
        }
    }
    paths.into_iter().collect()
}

pub fn link_path(gmod_root: &Path, project_slug: &str) -> Result<PathBuf, IntegrationError> {
    let root = normalize_gmod_root(gmod_root)
        .ok_or_else(|| IntegrationError::InvalidGmodRoot(gmod_root.to_path_buf()))?;
    let addons = root.join("addons");
    let name = crate::domain::slugify(project_slug);
    if name.is_empty() {
        return Err(IntegrationError::UnsafeLinkPath);
    }
    Ok(addons.join(name))
}

pub fn link_status(gmod_root: &Path, project_slug: &str, project_root: &Path) -> LinkStatus {
    let Ok(link) = link_path(gmod_root, project_slug) else {
        return LinkStatus::Unlinked;
    };
    if !link.exists() {
        return LinkStatus::Unlinked;
    }
    match (fs::canonicalize(&link), fs::canonicalize(project_root)) {
        (Ok(actual), Ok(expected)) if actual == expected => LinkStatus::Linked(link),
        _ => LinkStatus::Conflict(link),
    }
}

pub fn create_junction(
    gmod_root: &Path,
    project_slug: &str,
    project_root: &Path,
) -> Result<PathBuf, IntegrationError> {
    let link = link_path(gmod_root, project_slug)?;
    if link.exists() {
        return match link_status(gmod_root, project_slug, project_root) {
            LinkStatus::Linked(_) => Ok(link),
            _ => Err(IntegrationError::LinkConflict(link)),
        };
    }
    let project_root = fs::canonicalize(project_root).map_err(|source| IntegrationError::Io {
        path: project_root.to_path_buf(),
        source,
    })?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&project_root)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(IntegrationError::JunctionCommand)?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(IntegrationError::JunctionFailed(message));
        }
        Ok(link)
    }

    #[cfg(not(windows))]
    {
        let _ = (link, project_root);
        Err(IntegrationError::UnsupportedPlatform)
    }
}

pub fn remove_junction(
    gmod_root: &Path,
    project_slug: &str,
    project_root: &Path,
) -> Result<(), IntegrationError> {
    let link = link_path(gmod_root, project_slug)?;
    if !link.exists() {
        return Ok(());
    }
    if link_status(gmod_root, project_slug, project_root) != LinkStatus::Linked(link.clone()) {
        return Err(IntegrationError::LinkConflict(link));
    }
    fs::remove_dir(&link).map_err(|source| IntegrationError::Io { path: link, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_steam_library_vdf() {
        let vdf = r#""libraryfolders"
        {
            "0" { "path" "C:\\Program Files (x86)\\Steam" }
            "1" { "path" "D:\\Games\\Steam" }
        }"#;
        assert_eq!(
            parse_steam_library_paths(vdf),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\Games\Steam"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn junction_can_be_linked_detected_and_unlinked() {
        let root = std::env::temp_dir().join(format!(
            "nextbot_creator_junction_test_{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let gmod = root.join("GarrysMod").join("garrysmod");
        let project = root.join("projects").join("test_bot");
        fs::create_dir_all(gmod.join("addons")).unwrap();
        fs::create_dir_all(&project).unwrap();
        let link = create_junction(&gmod, "test_bot", &project).unwrap();
        assert_eq!(
            link_status(&gmod, "test_bot", &project),
            LinkStatus::Linked(link)
        );
        remove_junction(&gmod, "test_bot", &project).unwrap();
        assert_eq!(
            link_status(&gmod, "test_bot", &project),
            LinkStatus::Unlinked
        );
        fs::remove_dir_all(root).unwrap();
    }
}
