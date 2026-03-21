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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_path_normal_directory() {
        let ws = Workspace::from_path(PathBuf::from("/Users/dev/projects/surch"));
        assert_eq!(ws.name, "surch");
        assert_eq!(ws.root, PathBuf::from("/Users/dev/projects/surch"));
    }

    #[test]
    fn test_from_path_nested_directory() {
        let ws = Workspace::from_path(PathBuf::from("/a/b/c/deep-folder"));
        assert_eq!(ws.name, "deep-folder");
    }

    #[test]
    fn test_from_path_root() {
        let ws = Workspace::from_path(PathBuf::from("/"));
        // Root path has no file_name, falls back to full path
        assert_eq!(ws.name, "/");
    }

    #[test]
    fn test_from_path_single_component() {
        let ws = Workspace::from_path(PathBuf::from("my-project"));
        assert_eq!(ws.name, "my-project");
    }

    #[test]
    fn test_from_path_with_spaces() {
        let ws = Workspace::from_path(PathBuf::from("/Users/dev/My Project"));
        assert_eq!(ws.name, "My Project");
    }

    #[test]
    fn test_from_path_preserves_root() {
        let path = PathBuf::from("/tmp/test-workspace");
        let ws = Workspace::from_path(path.clone());
        assert_eq!(ws.root, path);
    }

    #[test]
    fn test_from_path_dot_directory() {
        let ws = Workspace::from_path(PathBuf::from("/home/user/.config"));
        assert_eq!(ws.name, ".config");
    }

    #[test]
    fn test_workspace_clone() {
        let ws = Workspace::from_path(PathBuf::from("/tmp/test"));
        let ws2 = ws.clone();
        assert_eq!(ws.name, ws2.name);
        assert_eq!(ws.root, ws2.root);
    }
}
