use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::SystemTime;

/// Application configuration stored at ~/.config/surch/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub recent_workspaces: Vec<RecentWorkspace>,
    #[serde(default)]
    pub editors: Vec<EditorConfig>,
}

/// A recently opened workspace with metadata for the welcome screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentWorkspace {
    pub path: PathBuf,
    pub last_opened: u64, // Unix timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub name: String,
    pub command: String,
    /// Format string for opening a file at a line. Use {file} and {line}.
    /// e.g. "--goto {file}:{line}"
    pub open_args: String,
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("surch")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }

    pub fn add_recent_workspace(&mut self, path: PathBuf) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.recent_workspaces.retain(|w| w.path != path);
        self.recent_workspaces.insert(
            0,
            RecentWorkspace {
                path,
                last_opened: now,
            },
        );
        self.recent_workspaces.truncate(10);
    }
}

/// Per-workspace state stored at ~/.config/surch/workspaces/{hash}/state.json
///
/// Persists search history, filter patterns, and other workspace-specific
/// data across sessions — similar to VS Code's workspaceStorage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceState {
    /// Recent search queries (most recent first, max 20)
    #[serde(default)]
    pub search_history: Vec<String>,
    /// Recent replace strings (most recent first, max 20)
    #[serde(default)]
    pub replace_history: Vec<String>,
    /// Recent include filter patterns
    #[serde(default)]
    pub include_history: Vec<String>,
    /// Recent exclude filter patterns
    #[serde(default)]
    pub exclude_history: Vec<String>,
    /// Last used search options
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub is_regex: bool,
}

impl WorkspaceState {
    /// Get the storage directory for a workspace path.
    /// Uses a hash of the absolute path for the directory name.
    fn workspace_dir(workspace_path: &PathBuf) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        workspace_path.hash(&mut hasher);
        let hash = format!("{:016x}", hasher.finish());

        AppConfig::config_dir().join("workspaces").join(hash)
    }

    fn state_path(workspace_path: &PathBuf) -> PathBuf {
        Self::workspace_dir(workspace_path).join("state.json")
    }

    /// Load workspace state from disk. Returns default if not found.
    pub fn load(workspace_path: &PathBuf) -> Self {
        let path = Self::state_path(workspace_path);
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Save workspace state to disk.
    pub fn save(&self, workspace_path: &PathBuf) -> anyhow::Result<()> {
        let dir = Self::workspace_dir(workspace_path);
        std::fs::create_dir_all(&dir)?;
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::state_path(workspace_path), content)?;
        Ok(())
    }

    /// Add a search query to history (dedup, max 20).
    pub fn add_search(&mut self, query: String) {
        if query.is_empty() {
            return;
        }
        self.search_history.retain(|q| q != &query);
        self.search_history.insert(0, query);
        self.search_history.truncate(20);
    }

    /// Add a replace string to history (dedup, max 20).
    pub fn add_replace(&mut self, replace: String) {
        if replace.is_empty() {
            return;
        }
        self.replace_history.retain(|r| r != &replace);
        self.replace_history.insert(0, replace);
        self.replace_history.truncate(20);
    }

    /// Add an include pattern to history (dedup, max 10).
    pub fn add_include(&mut self, pattern: String) {
        if pattern.is_empty() {
            return;
        }
        self.include_history.retain(|p| p != &pattern);
        self.include_history.insert(0, pattern);
        self.include_history.truncate(10);
    }

    /// Add an exclude pattern to history (dedup, max 10).
    pub fn add_exclude(&mut self, pattern: String) {
        if pattern.is_empty() {
            return;
        }
        self.exclude_history.retain(|p| p != &pattern);
        self.exclude_history.insert(0, pattern);
        self.exclude_history.truncate(10);
    }
}
