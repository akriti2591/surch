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
    /// The ID of the user's preferred editor for "Open in" (e.g., "cursor", "vscode").
    #[serde(default)]
    pub preferred_editor: Option<String>,
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
    #[serde(default)]
    pub fuzzy: bool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // === AppConfig tests ===

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert!(config.recent_workspaces.is_empty());
        assert!(config.editors.is_empty());
    }

    #[test]
    fn test_add_recent_workspace() {
        let mut config = AppConfig::default();
        config.add_recent_workspace(PathBuf::from("/tmp/project-a"));

        assert_eq!(config.recent_workspaces.len(), 1);
        assert_eq!(
            config.recent_workspaces[0].path,
            PathBuf::from("/tmp/project-a")
        );
        assert!(config.recent_workspaces[0].last_opened > 0);
    }

    #[test]
    fn test_add_recent_workspace_deduplicates() {
        let mut config = AppConfig::default();
        config.add_recent_workspace(PathBuf::from("/tmp/a"));
        config.add_recent_workspace(PathBuf::from("/tmp/b"));
        config.add_recent_workspace(PathBuf::from("/tmp/a")); // duplicate

        assert_eq!(config.recent_workspaces.len(), 2);
        // Most recent should be first
        assert_eq!(config.recent_workspaces[0].path, PathBuf::from("/tmp/a"));
        assert_eq!(config.recent_workspaces[1].path, PathBuf::from("/tmp/b"));
    }

    #[test]
    fn test_add_recent_workspace_truncates_to_10() {
        let mut config = AppConfig::default();
        for i in 0..15 {
            config.add_recent_workspace(PathBuf::from(format!("/tmp/project-{}", i)));
        }

        assert_eq!(config.recent_workspaces.len(), 10);
        // Most recent (14) should be first
        assert_eq!(
            config.recent_workspaces[0].path,
            PathBuf::from("/tmp/project-14")
        );
    }

    #[test]
    fn test_add_recent_workspace_moves_to_front() {
        let mut config = AppConfig::default();
        config.add_recent_workspace(PathBuf::from("/tmp/a"));
        config.add_recent_workspace(PathBuf::from("/tmp/b"));
        config.add_recent_workspace(PathBuf::from("/tmp/c"));

        // Re-add "a" — should move to front
        config.add_recent_workspace(PathBuf::from("/tmp/a"));

        assert_eq!(config.recent_workspaces[0].path, PathBuf::from("/tmp/a"));
        assert_eq!(config.recent_workspaces[1].path, PathBuf::from("/tmp/c"));
        assert_eq!(config.recent_workspaces[2].path, PathBuf::from("/tmp/b"));
    }

    #[test]
    fn test_app_config_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");

        let mut config = AppConfig::default();
        config.add_recent_workspace(PathBuf::from("/tmp/test"));
        config.editors.push(EditorConfig {
            name: "Cursor".to_string(),
            command: "cursor".to_string(),
            open_args: "--goto {file}:{line}".to_string(),
        });

        // Save manually to temp dir
        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_path, &content).unwrap();

        // Load back
        let loaded_content = fs::read_to_string(&config_path).unwrap();
        let loaded: AppConfig = toml::from_str(&loaded_content).unwrap();

        assert_eq!(loaded.recent_workspaces.len(), 1);
        assert_eq!(loaded.recent_workspaces[0].path, PathBuf::from("/tmp/test"));
        assert_eq!(loaded.editors.len(), 1);
        assert_eq!(loaded.editors[0].name, "Cursor");
        assert_eq!(loaded.editors[0].open_args, "--goto {file}:{line}");
    }

    #[test]
    fn test_app_config_load_invalid_toml_returns_default() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        fs::write(&config_path, "this is not valid toml {{{{").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let config: AppConfig = toml::from_str(&content).unwrap_or_default();

        assert!(config.recent_workspaces.is_empty());
    }

    #[test]
    fn test_app_config_serialization_with_empty_fields() {
        let config = AppConfig::default();
        let content = toml::to_string_pretty(&config).unwrap();
        let loaded: AppConfig = toml::from_str(&content).unwrap();
        assert!(loaded.recent_workspaces.is_empty());
        assert!(loaded.editors.is_empty());
    }

    // === WorkspaceState tests ===

    #[test]
    fn test_workspace_state_default() {
        let state = WorkspaceState::default();
        assert!(state.search_history.is_empty());
        assert!(state.replace_history.is_empty());
        assert!(state.include_history.is_empty());
        assert!(state.exclude_history.is_empty());
        assert!(!state.case_sensitive);
        assert!(!state.whole_word);
        assert!(!state.is_regex);
    }

    #[test]
    fn test_add_search_basic() {
        let mut state = WorkspaceState::default();
        state.add_search("hello".to_string());

        assert_eq!(state.search_history.len(), 1);
        assert_eq!(state.search_history[0], "hello");
    }

    #[test]
    fn test_add_search_empty_string_ignored() {
        let mut state = WorkspaceState::default();
        state.add_search(String::new());

        assert!(state.search_history.is_empty());
    }

    #[test]
    fn test_add_search_deduplicates() {
        let mut state = WorkspaceState::default();
        state.add_search("hello".to_string());
        state.add_search("world".to_string());
        state.add_search("hello".to_string());

        assert_eq!(state.search_history.len(), 2);
        assert_eq!(state.search_history[0], "hello");
        assert_eq!(state.search_history[1], "world");
    }

    #[test]
    fn test_add_search_truncates_to_20() {
        let mut state = WorkspaceState::default();
        for i in 0..25 {
            state.add_search(format!("query-{}", i));
        }

        assert_eq!(state.search_history.len(), 20);
        assert_eq!(state.search_history[0], "query-24");
    }

    #[test]
    fn test_add_replace_basic() {
        let mut state = WorkspaceState::default();
        state.add_replace("goodbye".to_string());

        assert_eq!(state.replace_history.len(), 1);
        assert_eq!(state.replace_history[0], "goodbye");
    }

    #[test]
    fn test_add_replace_empty_string_ignored() {
        let mut state = WorkspaceState::default();
        state.add_replace(String::new());
        assert!(state.replace_history.is_empty());
    }

    #[test]
    fn test_add_replace_deduplicates() {
        let mut state = WorkspaceState::default();
        state.add_replace("a".to_string());
        state.add_replace("b".to_string());
        state.add_replace("a".to_string());

        assert_eq!(state.replace_history.len(), 2);
        assert_eq!(state.replace_history[0], "a");
    }

    #[test]
    fn test_add_replace_truncates_to_20() {
        let mut state = WorkspaceState::default();
        for i in 0..25 {
            state.add_replace(format!("replace-{}", i));
        }
        assert_eq!(state.replace_history.len(), 20);
    }

    #[test]
    fn test_add_include_basic() {
        let mut state = WorkspaceState::default();
        state.add_include("*.rs".to_string());

        assert_eq!(state.include_history.len(), 1);
        assert_eq!(state.include_history[0], "*.rs");
    }

    #[test]
    fn test_add_include_empty_string_ignored() {
        let mut state = WorkspaceState::default();
        state.add_include(String::new());
        assert!(state.include_history.is_empty());
    }

    #[test]
    fn test_add_include_truncates_to_10() {
        let mut state = WorkspaceState::default();
        for i in 0..15 {
            state.add_include(format!("*.ext{}", i));
        }
        assert_eq!(state.include_history.len(), 10);
        assert_eq!(state.include_history[0], "*.ext14");
    }

    #[test]
    fn test_add_exclude_basic() {
        let mut state = WorkspaceState::default();
        state.add_exclude("target/**".to_string());

        assert_eq!(state.exclude_history.len(), 1);
        assert_eq!(state.exclude_history[0], "target/**");
    }

    #[test]
    fn test_add_exclude_empty_string_ignored() {
        let mut state = WorkspaceState::default();
        state.add_exclude(String::new());
        assert!(state.exclude_history.is_empty());
    }

    #[test]
    fn test_add_exclude_deduplicates() {
        let mut state = WorkspaceState::default();
        state.add_exclude("node_modules".to_string());
        state.add_exclude("target".to_string());
        state.add_exclude("node_modules".to_string());

        assert_eq!(state.exclude_history.len(), 2);
        assert_eq!(state.exclude_history[0], "node_modules");
    }

    #[test]
    fn test_add_exclude_truncates_to_10() {
        let mut state = WorkspaceState::default();
        for i in 0..15 {
            state.add_exclude(format!("dir{}", i));
        }
        assert_eq!(state.exclude_history.len(), 10);
    }

    #[test]
    fn test_workspace_state_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut state = WorkspaceState::default();
        state.add_search("foo".to_string());
        state.add_replace("bar".to_string());
        state.add_include("*.rs".to_string());
        state.add_exclude("target".to_string());
        state.case_sensitive = true;
        state.whole_word = true;
        state.is_regex = false;

        // Save to a temporary workspace dir
        let state_dir = dir.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        let state_path = state_dir.join("state.json");
        let content = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&state_path, &content).unwrap();

        // Load back
        let loaded_content = fs::read_to_string(&state_path).unwrap();
        let loaded: WorkspaceState = serde_json::from_str(&loaded_content).unwrap();

        assert_eq!(loaded.search_history, vec!["foo"]);
        assert_eq!(loaded.replace_history, vec!["bar"]);
        assert_eq!(loaded.include_history, vec!["*.rs"]);
        assert_eq!(loaded.exclude_history, vec!["target"]);
        assert!(loaded.case_sensitive);
        assert!(loaded.whole_word);
        assert!(!loaded.is_regex);
    }

    #[test]
    fn test_workspace_state_load_invalid_json_returns_default() {
        let content = "not valid json {{{";
        let state: WorkspaceState = serde_json::from_str(content).unwrap_or_default();
        assert!(state.search_history.is_empty());
    }

    #[test]
    fn test_workspace_state_load_partial_json() {
        // Should handle missing fields gracefully via #[serde(default)]
        let content = r#"{"search_history": ["test"]}"#;
        let state: WorkspaceState = serde_json::from_str(content).unwrap();
        assert_eq!(state.search_history, vec!["test"]);
        assert!(state.replace_history.is_empty());
        assert!(!state.case_sensitive);
    }

    #[test]
    fn test_workspace_dir_hashing_is_deterministic() {
        let path = PathBuf::from("/Users/dev/projects/surch");
        let dir1 = WorkspaceState::workspace_dir(&path);
        let dir2 = WorkspaceState::workspace_dir(&path);
        assert_eq!(dir1, dir2);
    }

    #[test]
    fn test_workspace_dir_different_paths_different_hashes() {
        let dir1 = WorkspaceState::workspace_dir(&PathBuf::from("/tmp/a"));
        let dir2 = WorkspaceState::workspace_dir(&PathBuf::from("/tmp/b"));
        assert_ne!(dir1, dir2);
    }

    #[test]
    fn test_workspace_state_save_and_load_via_methods() {
        // Use the actual save/load methods (not manual file I/O)
        let dir = TempDir::new().unwrap();
        let workspace_path = dir.path().to_path_buf();

        let mut state = WorkspaceState::default();
        state.add_search("test_query".to_string());
        state.add_replace("test_replace".to_string());
        state.case_sensitive = true;
        state.is_regex = true;

        state.save(&workspace_path).unwrap();

        let loaded = WorkspaceState::load(&workspace_path);
        assert_eq!(loaded.search_history, vec!["test_query"]);
        assert_eq!(loaded.replace_history, vec!["test_replace"]);
        assert!(loaded.case_sensitive);
        assert!(loaded.is_regex);
        assert!(!loaded.whole_word);
    }

    #[test]
    fn test_workspace_state_load_nonexistent_returns_default() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("does_not_exist");
        let state = WorkspaceState::load(&nonexistent);
        assert!(state.search_history.is_empty());
        assert!(!state.case_sensitive);
    }

    #[test]
    fn test_workspace_state_state_path_contains_hash() {
        let workspace_path = PathBuf::from("/tmp/my-project");
        let path = WorkspaceState::state_path(&workspace_path);
        assert!(path.to_string_lossy().contains("workspaces"));
        assert!(path.to_string_lossy().ends_with("state.json"));
    }

    #[test]
    fn test_app_config_config_dir() {
        let dir = AppConfig::config_dir();
        assert!(dir.to_string_lossy().contains("surch"));
    }

    #[test]
    fn test_app_config_config_path() {
        let path = AppConfig::config_path();
        assert!(path.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn test_add_include_deduplicates() {
        let mut state = WorkspaceState::default();
        state.add_include("*.rs".to_string());
        state.add_include("*.py".to_string());
        state.add_include("*.rs".to_string());

        assert_eq!(state.include_history.len(), 2);
        assert_eq!(state.include_history[0], "*.rs");
        assert_eq!(state.include_history[1], "*.py");
    }

    #[test]
    fn test_recent_workspace_timestamp_increases() {
        let mut config = AppConfig::default();
        config.add_recent_workspace(PathBuf::from("/tmp/a"));
        let t1 = config.recent_workspaces[0].last_opened;

        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(10));
        config.add_recent_workspace(PathBuf::from("/tmp/b"));
        let t2 = config.recent_workspaces[0].last_opened;

        assert!(t2 >= t1);
    }

    #[test]
    fn test_editor_config_serialization() {
        let editor = EditorConfig {
            name: "VS Code".to_string(),
            command: "code".to_string(),
            open_args: "--goto {file}:{line}".to_string(),
        };
        let json = serde_json::to_string(&editor).unwrap();
        let loaded: EditorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.name, "VS Code");
        assert_eq!(loaded.command, "code");
        assert_eq!(loaded.open_args, "--goto {file}:{line}");
    }
}
