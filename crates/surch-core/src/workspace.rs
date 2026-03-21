use std::path::PathBuf;

/// Represents an opened workspace (folder).
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub name: String,
}

impl Workspace {
    pub fn from_path(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        Self { root: path, name }
    }
}
