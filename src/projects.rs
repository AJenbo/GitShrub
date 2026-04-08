use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A recent project entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    /// Absolute path to the git repository root.
    pub path: String,
    /// Display name (usually the directory name).
    pub name: String,
}

/// The persisted project list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectList {
    pub recent: Vec<RecentProject>,
}

/// Return the path to the config file: `~/.config/gitshrub/projects.json`
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("gitshrub").join("projects.json"))
}

/// Load the project list from disk, returning an empty list on any error.
pub fn load() -> ProjectList {
    let Some(path) = config_path() else {
        return ProjectList::default();
    };
    let Ok(data) = fs::read_to_string(&path) else {
        return ProjectList::default();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

/// Save the project list to disk. Silently ignores errors.
pub fn save(list: &ProjectList) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(list) {
        let _ = fs::write(&path, data);
    }
}

/// Add a repo path to the project list (moves it to the front if already present).
/// Saves to disk immediately.
pub fn add_project(repo_path: &str) {
    let mut list = load();

    let name = Path::new(repo_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo_path.to_string());

    // Remove any existing entry with the same path.
    list.recent.retain(|p| p.path != repo_path);

    // Insert at the front (most recent first).
    list.recent.insert(
        0,
        RecentProject {
            path: repo_path.to_string(),
            name,
        },
    );

    // Cap the list at a reasonable size.
    list.recent.truncate(50);

    save(&list);
}

/// Remove entries whose paths no longer exist on disk, then save.
pub fn prune_missing(list: &mut ProjectList) {
    list.recent.retain(|p| Path::new(&p.path).is_dir());
    save(list);
}
