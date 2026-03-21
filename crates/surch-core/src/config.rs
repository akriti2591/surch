use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration stored at ~/.config/surch/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub recent_workspaces: Vec<PathBuf>,
    #[serde(default)]
    pub editors: Vec<EditorConfig>,
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
        self.recent_workspaces.retain(|p| p != &path);
        self.recent_workspaces.insert(0, path);
        self.recent_workspaces.truncate(10);
    }
}
