use std::collections::HashMap;

/// A node in the directory trie used to build the tree view of search results.
pub struct TrieNode {
    /// Full path of this directory node (e.g. "src/components").
    pub path: String,
    /// Child directories by segment name.
    pub children: HashMap<String, TrieNode>,
    /// Files at this level: filename -> (group_index, match_count).
    pub files: HashMap<String, (usize, usize)>,
}

impl TrieNode {
    pub fn new(path: String) -> Self {
        Self {
            path,
            children: HashMap::new(),
            files: HashMap::new(),
        }
    }

    /// Total match count for all files under this node (recursive).
    pub fn total_match_count(&self) -> usize {
        let file_matches: usize = self.files.values().map(|(_, count)| count).sum();
        let child_matches: usize = self.children.values().map(|c| c.total_match_count()).sum();
        file_matches + child_matches
    }
}

/// Input for building a path trie: a relative path and match count per file.
pub struct TrieInput {
    pub relative_path: String,
    pub group_index: usize,
    pub match_count: usize,
}

/// Build a path trie from a list of file paths and their match counts.
pub fn build_path_trie(inputs: &[TrieInput]) -> TrieNode {
    let mut root = TrieNode::new(String::new());

    for input in inputs {
        let parts: Vec<&str> = input.relative_path.split('/').collect();
        let mut current = &mut root;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // This is the filename
                current
                    .files
                    .insert(part.to_string(), (input.group_index, input.match_count));
            } else {
                // This is a directory segment
                let child_path = if current.path.is_empty() {
                    part.to_string()
                } else {
                    format!("{}/{}", current.path, part)
                };
                current = current
                    .children
                    .entry(part.to_string())
                    .or_insert_with(|| TrieNode::new(child_path));
            }
        }
    }

    root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(relative_path: &str, group_index: usize, match_count: usize) -> TrieInput {
        TrieInput {
            relative_path: relative_path.to_string(),
            group_index,
            match_count,
        }
    }

    #[test]
    fn test_build_path_trie_single_file() {
        let inputs = vec![make_input("README.md", 0, 2)];
        let trie = build_path_trie(&inputs);
        assert_eq!(trie.files.len(), 1);
        assert!(trie.files.contains_key("README.md"));
        assert_eq!(trie.children.len(), 0);
        assert_eq!(trie.total_match_count(), 2);
    }

    #[test]
    fn test_build_path_trie_nested_files() {
        let inputs = vec![
            make_input("src/main.rs", 0, 3),
            make_input("src/lib.rs", 1, 1),
            make_input("src/utils/helpers.rs", 2, 2),
        ];
        let trie = build_path_trie(&inputs);

        // Root should have one child directory: "src"
        assert_eq!(trie.children.len(), 1);
        assert_eq!(trie.files.len(), 0);

        let src = &trie.children["src"];
        assert_eq!(src.path, "src");
        assert_eq!(src.files.len(), 2); // main.rs, lib.rs
        assert_eq!(src.children.len(), 1); // utils

        let utils = &src.children["utils"];
        assert_eq!(utils.path, "src/utils");
        assert_eq!(utils.files.len(), 1); // helpers.rs
        assert_eq!(utils.children.len(), 0);

        // Total match counts
        assert_eq!(trie.total_match_count(), 6);
        assert_eq!(src.total_match_count(), 6);
        assert_eq!(utils.total_match_count(), 2);
    }

    #[test]
    fn test_build_path_trie_multiple_top_level_dirs() {
        let inputs = vec![
            make_input("src/app.rs", 0, 1),
            make_input("tests/test_app.rs", 1, 2),
            make_input("Cargo.toml", 2, 1),
        ];
        let trie = build_path_trie(&inputs);

        assert_eq!(trie.children.len(), 2); // src, tests
        assert_eq!(trie.files.len(), 1); // Cargo.toml
        assert_eq!(trie.total_match_count(), 4);
    }

    #[test]
    fn test_build_path_trie_deeply_nested() {
        let inputs = vec![make_input("a/b/c/d/file.rs", 0, 5)];
        let trie = build_path_trie(&inputs);

        assert_eq!(trie.children.len(), 1);
        let a = &trie.children["a"];
        assert_eq!(a.path, "a");
        let b = &a.children["b"];
        assert_eq!(b.path, "a/b");
        let c = &b.children["c"];
        assert_eq!(c.path, "a/b/c");
        let d = &c.children["d"];
        assert_eq!(d.path, "a/b/c/d");
        assert_eq!(d.files.len(), 1);
        assert!(d.files.contains_key("file.rs"));
        assert_eq!(trie.total_match_count(), 5);
    }

    #[test]
    fn test_build_path_trie_empty() {
        let inputs: Vec<TrieInput> = vec![];
        let trie = build_path_trie(&inputs);
        assert_eq!(trie.children.len(), 0);
        assert_eq!(trie.files.len(), 0);
        assert_eq!(trie.total_match_count(), 0);
    }

    #[test]
    fn test_build_path_trie_same_dir_different_files() {
        let inputs = vec![
            make_input("src/components/Button.tsx", 0, 3),
            make_input("src/components/Input.tsx", 1, 1),
            make_input("src/components/Modal.tsx", 2, 2),
        ];
        let trie = build_path_trie(&inputs);

        let src = &trie.children["src"];
        let components = &src.children["components"];
        assert_eq!(components.files.len(), 3);
        assert_eq!(components.total_match_count(), 6);
    }

    #[test]
    fn test_trie_node_total_match_count_recursive() {
        let mut root = TrieNode::new(String::new());
        root.files.insert("a.rs".to_string(), (0, 3));

        let mut child = TrieNode::new("sub".to_string());
        child.files.insert("b.rs".to_string(), (1, 5));

        let mut grandchild = TrieNode::new("sub/deep".to_string());
        grandchild.files.insert("c.rs".to_string(), (2, 2));

        child.children.insert("deep".to_string(), grandchild);
        root.children.insert("sub".to_string(), child);

        assert_eq!(root.total_match_count(), 10); // 3 + 5 + 2
    }

    #[test]
    fn test_build_path_trie_preserves_group_indices() {
        let inputs = vec![
            make_input("src/a.rs", 0, 1),
            make_input("src/b.rs", 3, 2),
            make_input("lib/c.rs", 7, 5),
        ];
        let trie = build_path_trie(&inputs);

        let src = &trie.children["src"];
        assert_eq!(src.files["a.rs"].0, 0);
        assert_eq!(src.files["b.rs"].0, 3);

        let lib = &trie.children["lib"];
        assert_eq!(lib.files["c.rs"].0, 7);
    }

    #[test]
    fn test_build_path_trie_single_segment_path() {
        // A file with no directory path at all
        let inputs = vec![make_input("Makefile", 0, 1), make_input("README.md", 1, 3)];
        let trie = build_path_trie(&inputs);

        assert_eq!(trie.files.len(), 2);
        assert_eq!(trie.children.len(), 0);
        assert!(trie.files.contains_key("Makefile"));
        assert!(trie.files.contains_key("README.md"));
        assert_eq!(trie.total_match_count(), 4);
    }
}
