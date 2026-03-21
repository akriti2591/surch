mod engine;

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

impl FileSearchChannel {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn detect_editors() -> Vec<ChannelAction> {
        let editors = vec![
            ("cursor", "Cursor", "cursor {file}:{line}"),
            ("code", "VS Code", "code --goto {file}:{line}"),
            ("zed", "Zed", "zed {file}:{line}"),
            ("subl", "Sublime Text", "subl {file}:{line}"),
            ("vim", "Vim", "vim +{line} {file}"),
            ("nvim", "Neovim", "nvim +{line} {file}"),
        ];

        let mut actions = Vec::new();
        for (cmd, label, _args) in editors {
            if Command::new("which")
                .arg(cmd)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                actions.push(ChannelAction {
                    id: format!("open_in_{}", cmd),
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
                language: path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string()),
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

        if action_id == "reveal_in_finder" {
            Command::new("open").arg("-R").arg(file).spawn()?;
            return Ok(());
        }

        let (cmd, args) = match action_id {
            "open_in_cursor" => ("cursor", format!("{}:{}", file.display(), line)),
            "open_in_code" => ("code", format!("--goto {}:{}", file.display(), line)),
            "open_in_zed" => ("zed", format!("{}:{}", file.display(), line)),
            "open_in_subl" => ("subl", format!("{}:{}", file.display(), line)),
            "open_in_vim" => ("vim", format!("+{} {}", line, file.display())),
            "open_in_nvim" => ("nvim", format!("+{} {}", line, file.display())),
            _ => return Err(anyhow::anyhow!("Unknown action: {}", action_id)),
        };

        Command::new(cmd)
            .args(args.split_whitespace())
            .spawn()?;
        Ok(())
    }
}
