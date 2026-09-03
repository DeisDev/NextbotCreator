use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    PROJECT_FILE,
    domain::{Project, slugify},
};

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid project file at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("a project folder already exists at {0}")]
    AlreadyExists(PathBuf),
    #[error("the project name must contain at least one letter or number")]
    InvalidName,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub projects_root: PathBuf,
    pub garrys_mod_root: Option<PathBuf>,
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
}

impl AppSettings {
    pub fn load_or_default(portable_root: &Path) -> Self {
        let path = portable_root.join("settings.json");
        let mut settings = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| Self {
                projects_root: portable_root.join("projects"),
                garrys_mod_root: None,
                recent_projects: Vec::new(),
            });
        if settings.projects_root.is_relative() {
            settings.projects_root = portable_root.join(&settings.projects_root);
        }
        for project in &mut settings.recent_projects {
            if project.is_relative() {
                *project = portable_root.join(&*project);
            }
        }
        settings
    }

    pub fn save(&self, portable_root: &Path) -> Result<(), PersistenceError> {
        let path = portable_root.join("settings.json");
        let mut portable = self.clone();
        if let Ok(relative) = portable.projects_root.strip_prefix(portable_root) {
            portable.projects_root = relative.to_path_buf();
        }
        for project in &mut portable.recent_projects {
            if let Ok(relative) = project.strip_prefix(portable_root) {
                *project = relative.to_path_buf();
            }
        }
        write_json(&path, &portable)
    }

    pub fn remember_project(&mut self, path: &Path) {
        let project_root = if path.file_name().is_some_and(|name| name == PROJECT_FILE) {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let project_root = project_root.to_path_buf();
        self.recent_projects.retain(|existing| {
            existing.join(PROJECT_FILE).is_file() && !same_path(existing, &project_root)
        });
        self.recent_projects.insert(0, project_root);
        self.recent_projects.truncate(12);
    }

    pub fn available_projects(&self) -> Vec<PathBuf> {
        let mut projects: Vec<PathBuf> = Vec::new();
        for path in self
            .recent_projects
            .iter()
            .cloned()
            .chain(discover_projects(&self.projects_root))
        {
            if path.join(PROJECT_FILE).is_file()
                && !projects.iter().any(|existing| same_path(existing, &path))
            {
                projects.push(path);
            }
        }
        projects
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

pub fn portable_root() -> PathBuf {
    if let Some(path) = std::env::var_os("NEXTBOTCREATOR_PORTABLE_ROOT") {
        return PathBuf::from(path);
    }
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn create_project(projects_root: &Path, name: &str) -> Result<Project, PersistenceError> {
    let slug = slugify(name);
    if slug.is_empty() {
        return Err(PersistenceError::InvalidName);
    }
    let root = projects_root.join(&slug);
    if root.exists() {
        return Err(PersistenceError::AlreadyExists(root));
    }
    fs::create_dir_all(&root).map_err(|source| PersistenceError::Io {
        path: root.clone(),
        source,
    })?;
    let project = Project::new(name, root);
    save_project(&project)?;
    Ok(project)
}

pub fn save_project(project: &Project) -> Result<(), PersistenceError> {
    fs::create_dir_all(&project.root).map_err(|source| PersistenceError::Io {
        path: project.root.clone(),
        source,
    })?;
    let mut portable = project.clone();
    portable.root = PathBuf::from(".");
    visit_asset_paths_mut(&mut portable, |path| {
        if let Ok(relative) = path.strip_prefix(&project.root) {
            *path = relative.to_path_buf();
        }
    });
    write_json(&project.root.join(PROJECT_FILE), &portable)
}

pub fn load_project(path: &Path) -> Result<Project, PersistenceError> {
    let project_file = if path.is_dir() {
        path.join(PROJECT_FILE)
    } else {
        path.to_owned()
    };
    let bytes = fs::read(&project_file).map_err(|source| PersistenceError::Io {
        path: project_file.clone(),
        source,
    })?;
    let mut project: Project =
        serde_json::from_slice(&bytes).map_err(|source| PersistenceError::Json {
            path: project_file.clone(),
            source,
        })?;
    if let Some(parent) = project_file.parent() {
        project.root = parent.to_path_buf();
    }
    let root = project.root.clone();
    visit_asset_paths_mut(&mut project, |path| {
        if path.is_relative() {
            *path = root.join(&*path);
        }
    });
    Ok(project)
}

pub fn discover_projects(projects_root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(projects_root) else {
        return Vec::new();
    };
    let mut projects = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(PROJECT_FILE).is_file())
        .collect::<Vec<_>>();
    projects.sort_by_key(|path| path.file_name().map(|name| name.to_ascii_lowercase()));
    projects
}

pub fn import_source_asset(
    project: &Project,
    bot_class: &str,
    source: &Path,
) -> Result<PathBuf, PersistenceError> {
    let file_name = source.file_name().ok_or_else(|| PersistenceError::Io {
        path: source.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "source has no filename"),
    })?;
    let destination_dir = project.root.join("source_assets").join(bot_class);
    fs::create_dir_all(&destination_dir).map_err(|source| PersistenceError::Io {
        path: destination_dir.clone(),
        source,
    })?;
    let destination = unique_destination(&destination_dir, file_name);
    fs::copy(source, &destination).map_err(|source| PersistenceError::Io {
        path: destination.clone(),
        source,
    })?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if extension.eq_ignore_ascii_case("vmt") || extension.eq_ignore_ascii_case("vtf") {
        let sidecar_extension = if extension.eq_ignore_ascii_case("vmt") {
            "vtf"
        } else {
            "vmt"
        };
        let sidecar = source.with_extension(sidecar_extension);
        if sidecar.is_file() {
            let sidecar_destination = destination.with_extension(sidecar_extension);
            fs::copy(&sidecar, &sidecar_destination).map_err(|source| PersistenceError::Io {
                path: sidecar_destination,
                source,
            })?;
        }
    }
    Ok(destination)
}

fn unique_destination(directory: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let initial = directory.join(file_name);
    if !initial.exists() {
        return initial;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let extension = path.extension().and_then(|value| value.to_str());
    let mut index = 2_u32;
    loop {
        let candidate_name = match extension {
            Some(extension) => format!("{stem}_{index}.{extension}"),
            None => format!("{stem}_{index}"),
        };
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), PersistenceError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| PersistenceError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, bytes).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn visit_asset_paths_mut(project: &mut Project, mut visit: impl FnMut(&mut PathBuf)) {
    for bot in &mut project.nextbots {
        if let Some(source) = &mut bot.visual.source {
            visit(source);
        }
        if let Some(source) = &mut bot.visual.killfeed_icon.source {
            visit(source);
        }
        for paths in [
            &mut bot.audio.spawn,
            &mut bot.audio.idle,
            &mut bot.audio.damage,
            &mut bot.audio.death,
            &mut bot.audio.downed,
            &mut bot.audio.jump,
            &mut bot.audio.footsteps,
        ] {
            for path in paths {
                visit(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nextbot_creator_{name}_{}", std::process::id()))
    }

    #[test]
    fn projects_round_trip_and_root_follows_file_location() {
        let root = temp_folder("round_trip");
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let project = create_project(&root, "Test Bot Pack").unwrap();
        let source = project.root.join("source_assets").join("sprite.png");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"test").unwrap();
        let mut project = project;
        project.nextbots[0].visual.source = Some(source.clone());
        project.nextbots[0].visual.killfeed_icon.source = Some(source.clone());
        project.nextbots[0].audio.spawn.push(source.clone());
        project.nextbots[0].audio.jump.push(source.clone());
        save_project(&project).unwrap();
        let persisted = fs::read_to_string(project.root.join(PROJECT_FILE)).unwrap();
        assert!(!persisted.contains(&project.root.display().to_string()));
        let loaded = load_project(&project.root).unwrap();
        assert_eq!(loaded.name, "Test Bot Pack");
        assert_eq!(loaded.root, project.root);
        assert_eq!(loaded.nextbots[0].visual.source.as_ref(), Some(&source));
        assert_eq!(
            loaded.nextbots[0].visual.killfeed_icon.source.as_ref(),
            Some(&source)
        );
        assert_eq!(loaded.nextbots[0].audio.spawn, vec![source]);
        assert_eq!(loaded.nextbots[0].audio.jump.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_settings_keep_the_projects_folder_portable() {
        let root = temp_folder("settings");
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        let settings = AppSettings {
            projects_root: root.join("projects"),
            garrys_mod_root: Some(PathBuf::from(r"C:\Games\GarrysMod\garrysmod")),
            recent_projects: vec![root.join("projects").join("recent")],
        };
        settings.save(&root).unwrap();
        let persisted = fs::read_to_string(root.join("settings.json")).unwrap();
        assert!(!persisted.contains(&root.display().to_string()));
        let loaded = AppSettings::load_or_default(&root);
        assert_eq!(loaded.projects_root, root.join("projects"));
        assert_eq!(loaded.garrys_mod_root, settings.garrys_mod_root);
        assert_eq!(loaded.recent_projects, settings.recent_projects);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remembered_projects_are_most_recent_first_and_deduplicated() {
        let root = temp_folder("recent_projects");
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        let first = create_project(&root, "First").unwrap();
        let second = create_project(&root, "Second").unwrap();
        let mut settings = AppSettings {
            projects_root: root.clone(),
            garrys_mod_root: None,
            recent_projects: Vec::new(),
        };
        settings.remember_project(&first.root);
        settings.remember_project(&second.root);
        settings.remember_project(&first.root);
        assert_eq!(settings.available_projects(), vec![first.root, second.root]);
        fs::remove_dir_all(root).unwrap();
    }
}
