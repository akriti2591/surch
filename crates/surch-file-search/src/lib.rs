pub mod engine;

use crossbeam_channel::Sender;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use surch_core::channel::*;

/// The built-in file content search channel.
/// Searches text inside files using ripgrep's library crates.
pub struct FileSearchChannel {
    cancelled: Arc<AtomicBool>,
}

impl Default for FileSearchChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSearchChannel {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn detect_editors() -> Vec<ChannelAction> {
        // Auto-discover GUI apps from /Applications and CLI tools from PATH.
        // Each entry: (app_bundle_name or cli_cmd, display_label, action_id)
        let gui_apps: Vec<(&str, &str, &str)> = vec![
            ("Cursor", "Cursor", "open_in_cursor"),
            ("Visual Studio Code", "VS Code", "open_in_code"),
            ("VSCodium", "VSCodium", "open_in_vscodium"),
            ("Zed", "Zed", "open_in_zed"),
            ("Sublime Text", "Sublime Text", "open_in_subl"),
            ("TextEdit", "TextEdit", "open_in_textedit"),
        ];

        let mut actions = Vec::new();

        // Check /Applications for installed GUI editors
        for (app_name, label, action_id) in &gui_apps {
            let app_path = format!("/Applications/{}.app", app_name);
            if std::path::Path::new(&app_path).exists() {
                actions.push(ChannelAction {
                    id: action_id.to_string(),
                    label: format!("Open in {}", label),
                    icon: None,
                });
            }
        }

        actions.push(ChannelAction {
            id: "reveal_in_finder".to_string(),
            label: "Reveal in Finder".to_string(),
            icon: None,
        });

        actions
    }

    /// Run replace all: find all matches and replace them in-place.
    /// Returns (files_modified, replacements_made).
    pub fn replace_all(
        &self,
        query: ChannelQuery,
        replacement: &str,
        tx: Sender<SearchEvent>,
    ) -> (usize, usize) {
        self.cancelled.store(false, Ordering::SeqCst);
        let cancelled = self.cancelled.clone();
        engine::run_replace(query, replacement, tx, cancelled)
    }
}

impl Channel for FileSearchChannel {
    fn metadata(&self) -> ChannelMetadata {
        ChannelMetadata {
            id: "file_search".to_string(),
            name: "Search in Files".to_string(),
            icon: "magnifying_glass".to_string(),
            description: "Search text content inside files".to_string(),
        }
    }

    fn input_fields(&self) -> Vec<InputFieldSpec> {
        vec![
            InputFieldSpec {
                id: "find".to_string(),
                label: "Find".to_string(),
                placeholder: "Search".to_string(),
            },
            InputFieldSpec {
                id: "replace".to_string(),
                label: "Replace".to_string(),
                placeholder: "Replace".to_string(),
            },
            InputFieldSpec {
                id: "include".to_string(),
                label: "Include".to_string(),
                placeholder: "e.g. *.rs, src/**".to_string(),
            },
            InputFieldSpec {
                id: "exclude".to_string(),
                label: "Exclude".to_string(),
                placeholder: "e.g. *.log, target/**".to_string(),
            },
        ]
    }

    fn search(&self, query: ChannelQuery, tx: Sender<SearchEvent>) {
        self.cancelled.store(false, Ordering::SeqCst);
        let cancelled = self.cancelled.clone();
        engine::run_search(query, tx, cancelled);
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn preview(&self, entry: &ResultEntry) -> PreviewContent {
        if let Some(ref path) = entry.file_path {
            PreviewContent::Code {
                path: path.clone(),
                focus_line: entry.line_number.unwrap_or(1),
                language: path.extension().map(|e| e.to_string_lossy().to_string()),
            }
        } else {
            PreviewContent::None
        }
    }

    fn actions(&self, _entry: &ResultEntry) -> Vec<ChannelAction> {
        Self::detect_editors()
    }

    fn execute_action(&self, action_id: &str, entry: &ResultEntry) -> anyhow::Result<()> {
        let file = entry
            .file_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No file path"))?;
        let line = entry.line_number.unwrap_or(1);
        let file_str = file.to_string_lossy();

        if action_id == "reveal_in_finder" {
            Command::new("open").arg("-R").arg(file).spawn()?;
            return Ok(());
        }

        if action_id == "open_in_textedit" {
            Command::new("open")
                .arg("-a")
                .arg("TextEdit")
                .arg(file)
                .spawn()?;
            return Ok(());
        }

        // For editors with CLI tools, use `open -a` as fallback if CLI isn't in PATH.
        // Each editor gets proper argument handling — no split_whitespace.
        match action_id {
            "open_in_cursor" => {
                // Cursor uses --goto file:line (same as VS Code)
                Command::new("cursor")
                    .arg("--goto")
                    .arg(format!("{}:{}", file_str, line))
                    .spawn()
                    .or_else(|_| {
                        Command::new("open")
                            .arg("-a")
                            .arg("Cursor")
                            .arg(file)
                            .spawn()
                    })?;
            }
            "open_in_code" => {
                Command::new("code")
                    .arg("--goto")
                    .arg(format!("{}:{}", file_str, line))
                    .spawn()
                    .or_else(|_| {
                        Command::new("open")
                            .arg("-a")
                            .arg("Visual Studio Code")
                            .arg(file)
                            .spawn()
                    })?;
            }
            "open_in_vscodium" => {
                Command::new("codium")
                    .arg("--goto")
                    .arg(format!("{}:{}", file_str, line))
                    .spawn()
                    .or_else(|_| {
                        Command::new("open")
                            .arg("-a")
                            .arg("VSCodium")
                            .arg(file)
                            .spawn()
                    })?;
            }
            "open_in_zed" => {
                Command::new("zed")
                    .arg(format!("{}:{}", file_str, line))
                    .spawn()
                    .or_else(|_| Command::new("open").arg("-a").arg("Zed").arg(file).spawn())?;
            }
            "open_in_subl" => {
                Command::new("subl")
                    .arg(format!("{}:{}", file_str, line))
                    .spawn()
                    .or_else(|_| {
                        Command::new("open")
                            .arg("-a")
                            .arg("Sublime Text")
                            .arg(file)
                            .spawn()
                    })?;
            }
            _ => return Err(anyhow::anyhow!("Unknown action: {}", action_id)),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_file_search_channel_new() {
        let channel = FileSearchChannel::new();
        assert!(!channel.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn test_metadata() {
        let channel = FileSearchChannel::new();
        let meta = channel.metadata();
        assert_eq!(meta.id, "file_search");
        assert_eq!(meta.name, "Search in Files");
        assert!(!meta.description.is_empty());
    }

    #[test]
    fn test_input_fields() {
        let channel = FileSearchChannel::new();
        let fields = channel.input_fields();

        assert_eq!(fields.len(), 4);

        let ids: Vec<&str> = fields.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"find"));
        assert!(ids.contains(&"replace"));
        assert!(ids.contains(&"include"));
        assert!(ids.contains(&"exclude"));
    }

    #[test]
    fn test_input_fields_have_placeholders() {
        let channel = FileSearchChannel::new();
        let fields = channel.input_fields();

        for field in &fields {
            assert!(
                !field.placeholder.is_empty(),
                "Field {} has no placeholder",
                field.id
            );
        }
    }

    #[test]
    fn test_preview_with_file_path() {
        let channel = FileSearchChannel::new();
        let entry = ResultEntry {
            id: 1,
            file_path: Some(PathBuf::from("/tmp/test.rs")),
            line_number: Some(42),
            column: Some(0),
            line_content: "fn main() {}".to_string(),
            match_ranges: vec![3..7],
        };

        let preview = channel.preview(&entry);
        match preview {
            PreviewContent::Code {
                path,
                focus_line,
                language,
            } => {
                assert_eq!(path, PathBuf::from("/tmp/test.rs"));
                assert_eq!(focus_line, 42);
                assert_eq!(language, Some("rs".to_string()));
            }
            _ => panic!("Expected PreviewContent::Code"),
        }
    }

    #[test]
    fn test_preview_without_file_path() {
        let channel = FileSearchChannel::new();
        let entry = ResultEntry {
            id: 1,
            file_path: None,
            line_number: None,
            column: None,
            line_content: "test".to_string(),
            match_ranges: vec![],
        };

        let preview = channel.preview(&entry);
        assert!(matches!(preview, PreviewContent::None));
    }

    #[test]
    fn test_preview_without_line_number_defaults_to_1() {
        let channel = FileSearchChannel::new();
        let entry = ResultEntry {
            id: 1,
            file_path: Some(PathBuf::from("/tmp/test.txt")),
            line_number: None,
            column: None,
            line_content: "test".to_string(),
            match_ranges: vec![],
        };

        match channel.preview(&entry) {
            PreviewContent::Code { focus_line, .. } => {
                assert_eq!(focus_line, 1);
            }
            _ => panic!("Expected PreviewContent::Code"),
        }
    }

    #[test]
    fn test_preview_language_from_extension() {
        let channel = FileSearchChannel::new();

        let cases = vec![
            ("/tmp/test.py", Some("py")),
            ("/tmp/test.js", Some("js")),
            ("/tmp/test.tsx", Some("tsx")),
            ("/tmp/Makefile", None),
        ];

        for (path, expected_lang) in cases {
            let entry = ResultEntry {
                id: 1,
                file_path: Some(PathBuf::from(path)),
                line_number: Some(1),
                column: None,
                line_content: String::new(),
                match_ranges: vec![],
            };

            match channel.preview(&entry) {
                PreviewContent::Code { language, .. } => {
                    assert_eq!(
                        language.as_deref(),
                        expected_lang,
                        "Wrong language for {}",
                        path
                    );
                }
                _ => panic!("Expected Code for {}", path),
            }
        }
    }

    #[test]
    fn test_cancel() {
        let channel = FileSearchChannel::new();
        assert!(!channel.cancelled.load(Ordering::SeqCst));

        channel.cancel();
        assert!(channel.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn test_search_resets_cancelled_flag() {
        let channel = FileSearchChannel::new();
        channel.cancel();
        assert!(channel.cancelled.load(Ordering::SeqCst));

        let (tx, _rx) = crossbeam_channel::unbounded();
        let query = ChannelQuery {
            fields: {
                let mut m = HashMap::new();
                m.insert("find".to_string(), String::new());
                m.insert("include".to_string(), String::new());
                m.insert("exclude".to_string(), String::new());
                m
            },
            workspace_root: PathBuf::from("/tmp"),
            ..Default::default()
        };

        channel.search(query, tx);
        // After search starts, cancelled should be reset to false
        assert!(!channel.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn test_detect_editors_always_includes_reveal_in_finder() {
        let actions = FileSearchChannel::detect_editors();
        let has_reveal = actions.iter().any(|a| a.id == "reveal_in_finder");
        assert!(has_reveal, "Should always include Reveal in Finder");
    }

    #[test]
    fn test_detect_editors_reveal_is_last() {
        let actions = FileSearchChannel::detect_editors();
        assert_eq!(
            actions.last().unwrap().id,
            "reveal_in_finder",
            "Reveal in Finder should be the last action"
        );
    }

    #[test]
    fn test_execute_action_unknown_action() {
        let channel = FileSearchChannel::new();
        let entry = ResultEntry {
            id: 1,
            file_path: Some(PathBuf::from("/tmp/test.txt")),
            line_number: Some(1),
            column: None,
            line_content: "test".to_string(),
            match_ranges: vec![],
        };

        let result = channel.execute_action("nonexistent_action", &entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown action"));
    }

    #[test]
    fn test_execute_action_no_file_path() {
        let channel = FileSearchChannel::new();
        let entry = ResultEntry {
            id: 1,
            file_path: None,
            line_number: None,
            column: None,
            line_content: "test".to_string(),
            match_ranges: vec![],
        };

        let result = channel.execute_action("reveal_in_finder", &entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No file path"));
    }

    #[test]
    fn test_replace_all_integration() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "hello world\nhello rust\n").unwrap();
        fs::write(dir.path().join("b.txt"), "goodbye world\n").unwrap();

        let channel = FileSearchChannel::new();
        let (tx, _rx) = crossbeam_channel::unbounded();

        let mut fields = HashMap::new();
        fields.insert("find".to_string(), "hello".to_string());
        fields.insert("include".to_string(), String::new());
        fields.insert("exclude".to_string(), String::new());

        let query = ChannelQuery {
            fields,
            workspace_root: dir.path().to_path_buf(),
            case_sensitive: true,
            ..Default::default()
        };

        let (replacements, files) = channel.replace_all(query, "hi", tx);

        assert_eq!(replacements, 2);
        assert_eq!(files, 1); // only a.txt has matches

        let content = fs::read_to_string(dir.path().join("a.txt")).unwrap();
        assert!(content.contains("hi world"));
        assert!(content.contains("hi rust"));
        assert!(!content.contains("hello"));

        // b.txt should be unchanged
        let content_b = fs::read_to_string(dir.path().join("b.txt")).unwrap();
        assert_eq!(content_b, "goodbye world\n");
    }

    #[test]
    fn test_search_end_to_end() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("code.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let channel = FileSearchChannel::new();
        let (tx, rx) = crossbeam_channel::unbounded();

        let mut fields = HashMap::new();
        fields.insert("find".to_string(), "println".to_string());
        fields.insert("include".to_string(), String::new());
        fields.insert("exclude".to_string(), String::new());

        let query = ChannelQuery {
            fields,
            workspace_root: dir.path().to_path_buf(),
            case_sensitive: true,
            ..Default::default()
        };

        channel.search(query, tx);

        let mut results = vec![];
        for event in rx {
            match event {
                SearchEvent::Match(entry) => results.push(entry),
                SearchEvent::Complete { .. } => break,
                _ => {}
            }
        }

        assert_eq!(results.len(), 1);
        assert!(results[0].line_content.contains("println"));
        assert_eq!(results[0].line_number, Some(2));
    }
}
