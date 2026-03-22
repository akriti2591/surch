use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

/// Metadata describing a channel (extension/search mode).
#[derive(Debug, Clone)]
pub struct ChannelMetadata {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
}

/// Specifies an input field that a channel wants displayed in the search panel.
#[derive(Debug, Clone)]
pub struct InputFieldSpec {
    pub id: String,
    pub label: String,
    pub placeholder: String,
}

/// A query built from the input fields' current values.
#[derive(Debug, Clone, Default)]
pub struct ChannelQuery {
    /// Map from InputFieldSpec.id to the current text value.
    pub fields: HashMap<String, String>,
    /// The workspace root directory being searched.
    pub workspace_root: PathBuf,
    /// Whether the search pattern should be treated as regex.
    pub is_regex: bool,
    /// Whether the search is case-sensitive.
    pub case_sensitive: bool,
    /// Whether to match whole words only.
    pub whole_word: bool,
    /// Whether replacement text should preserve the case pattern of the original match.
    pub preserve_case: bool,
    /// Whether to use fuzzy matching instead of exact/regex matching.
    /// Mutually exclusive with `is_regex`.
    pub fuzzy: bool,
}

impl ChannelQuery {
    pub fn field(&self, id: &str) -> &str {
        self.fields.get(id).map(|s| s.as_str()).unwrap_or("")
    }
}

/// Apply the case pattern of `original` to `replacement`.
///
/// Rules (matching VS Code's behavior):
/// - All lowercase original → replacement lowercased
/// - All uppercase original → replacement uppercased
/// - Title case (first upper, rest lower) → replacement title-cased
/// - Mixed case → replacement used as-is
pub fn apply_case_pattern(original: &str, replacement: &str) -> String {
    if original.is_empty() || replacement.is_empty() {
        return replacement.to_string();
    }

    if original.chars().all(|c| c.is_lowercase() || !c.is_alphabetic()) {
        replacement.to_lowercase()
    } else if original.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
        replacement.to_uppercase()
    } else if is_title_case(original) {
        let mut chars = replacement.chars();
        match chars.next() {
            Some(first) => {
                let mut result = first.to_uppercase().to_string();
                result.extend(chars.map(|c| c.to_lowercase().next().unwrap_or(c)));
                result
            }
            None => replacement.to_string(),
        }
    } else {
        // Mixed case — pass through
        replacement.to_string()
    }
}

fn is_title_case(s: &str) -> bool {
    let mut chars = s.chars().filter(|c| c.is_alphabetic());
    match chars.next() {
        Some(first) => first.is_uppercase() && chars.all(|c| c.is_lowercase()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_query_field_returns_value() {
        let mut fields = HashMap::new();
        fields.insert("find".to_string(), "hello".to_string());
        let query = ChannelQuery {
            fields,
            ..Default::default()
        };
        assert_eq!(query.field("find"), "hello");
    }

    #[test]
    fn test_channel_query_field_returns_empty_for_missing() {
        let query = ChannelQuery::default();
        assert_eq!(query.field("nonexistent"), "");
    }

    #[test]
    fn test_channel_query_field_empty_value() {
        let mut fields = HashMap::new();
        fields.insert("find".to_string(), String::new());
        let query = ChannelQuery {
            fields,
            ..Default::default()
        };
        assert_eq!(query.field("find"), "");
    }

    #[test]
    fn test_channel_query_default_flags() {
        let query = ChannelQuery::default();
        assert!(!query.is_regex);
        assert!(!query.case_sensitive);
        assert!(!query.whole_word);
    }

    #[test]
    fn test_channel_metadata_fields() {
        let meta = ChannelMetadata {
            id: "test".to_string(),
            name: "Test Channel".to_string(),
            icon: "search".to_string(),
            description: "A test channel".to_string(),
        };
        assert_eq!(meta.id, "test");
        assert_eq!(meta.name, "Test Channel");
    }

    #[test]
    fn test_input_field_spec() {
        let spec = InputFieldSpec {
            id: "find".to_string(),
            label: "Find".to_string(),
            placeholder: "Search...".to_string(),
        };
        assert_eq!(spec.id, "find");
        assert_eq!(spec.placeholder, "Search...");
    }

    #[test]
    fn test_result_entry_with_all_fields() {
        let entry = ResultEntry {
            id: 42,
            file_path: Some(PathBuf::from("/tmp/test.rs")),
            line_number: Some(10),
            column: Some(5),
            line_content: "fn hello() {}".to_string(),
            match_ranges: vec![3..8],
        };
        assert_eq!(entry.id, 42);
        assert_eq!(entry.line_number, Some(10));
        assert_eq!(entry.match_ranges.len(), 1);
        assert_eq!(entry.match_ranges[0], 3..8);
    }

    #[test]
    fn test_result_entry_without_optional_fields() {
        let entry = ResultEntry {
            id: 0,
            file_path: None,
            line_number: None,
            column: None,
            line_content: String::new(),
            match_ranges: vec![],
        };
        assert!(entry.file_path.is_none());
        assert!(entry.line_number.is_none());
        assert!(entry.match_ranges.is_empty());
    }

    #[test]
    fn test_search_event_variants() {
        let match_event = SearchEvent::Match(ResultEntry {
            id: 1,
            file_path: None,
            line_number: None,
            column: None,
            line_content: "test".to_string(),
            match_ranges: vec![],
        });
        assert!(matches!(match_event, SearchEvent::Match(_)));

        let progress = SearchEvent::Progress {
            files_searched: 10,
            matches_found: 5,
        };
        assert!(matches!(progress, SearchEvent::Progress { .. }));

        let complete = SearchEvent::Complete {
            total_files: 100,
            total_matches: 50,
        };
        assert!(matches!(complete, SearchEvent::Complete { .. }));

        let error = SearchEvent::Error("bad pattern".to_string());
        assert!(matches!(error, SearchEvent::Error(_)));
    }

    #[test]
    fn test_preview_content_variants() {
        let code = PreviewContent::Code {
            path: PathBuf::from("test.rs"),
            focus_line: 42,
            language: Some("rust".to_string()),
        };
        assert!(matches!(code, PreviewContent::Code { .. }));

        let text = PreviewContent::Text("hello".to_string());
        assert!(matches!(text, PreviewContent::Text(_)));

        let kv = PreviewContent::KeyValue(vec![
            ("key".to_string(), "value".to_string()),
        ]);
        assert!(matches!(kv, PreviewContent::KeyValue(_)));

        let none = PreviewContent::None;
        assert!(matches!(none, PreviewContent::None));
    }

    #[test]
    fn test_apply_case_pattern_all_lowercase() {
        assert_eq!(apply_case_pattern("foo", "bar"), "bar");
        assert_eq!(apply_case_pattern("hello", "WORLD"), "world");
    }

    #[test]
    fn test_apply_case_pattern_all_uppercase() {
        assert_eq!(apply_case_pattern("FOO", "bar"), "BAR");
        assert_eq!(apply_case_pattern("HELLO", "world"), "WORLD");
    }

    #[test]
    fn test_apply_case_pattern_title_case() {
        assert_eq!(apply_case_pattern("Foo", "bar"), "Bar");
        assert_eq!(apply_case_pattern("Hello", "world"), "World");
    }

    #[test]
    fn test_apply_case_pattern_mixed_case_passthrough() {
        assert_eq!(apply_case_pattern("fooBar", "bazQux"), "bazQux");
        assert_eq!(apply_case_pattern("camelCase", "hello"), "hello");
    }

    #[test]
    fn test_apply_case_pattern_empty_strings() {
        assert_eq!(apply_case_pattern("", "bar"), "bar");
        assert_eq!(apply_case_pattern("foo", ""), "");
    }

    #[test]
    fn test_apply_case_pattern_with_numbers() {
        assert_eq!(apply_case_pattern("foo123", "bar"), "bar");
        assert_eq!(apply_case_pattern("FOO123", "bar"), "BAR");
    }

    #[test]
    fn test_channel_action() {
        let action = ChannelAction {
            id: "open_in_cursor".to_string(),
            label: "Open in Cursor".to_string(),
            icon: Some("cursor".to_string()),
        };
        assert_eq!(action.id, "open_in_cursor");
        assert!(action.icon.is_some());

        let action_no_icon = ChannelAction {
            id: "reveal".to_string(),
            label: "Reveal in Finder".to_string(),
            icon: None,
        };
        assert!(action_no_icon.icon.is_none());
    }
}

/// A single result entry produced by a channel's search.
#[derive(Debug, Clone)]
pub struct ResultEntry {
    /// Unique identifier for this entry within the search.
    pub id: u64,
    /// Primary label (e.g. file path for file search).
    pub file_path: Option<PathBuf>,
    /// Line number (1-indexed) if applicable.
    pub line_number: Option<usize>,
    /// Column (0-indexed byte offset) if applicable.
    pub column: Option<usize>,
    /// The text content of this result line.
    pub line_content: String,
    /// Byte ranges within `line_content` that matched the query.
    pub match_ranges: Vec<Range<usize>>,
}

/// Events streamed from a channel's search back to the UI.
#[derive(Debug, Clone)]
pub enum SearchEvent {
    /// A single match was found.
    Match(ResultEntry),
    /// Progress update.
    Progress {
        files_searched: usize,
        matches_found: usize,
    },
    /// Search completed.
    Complete {
        total_files: usize,
        total_matches: usize,
    },
    /// An error occurred during search.
    Error(String),
}

/// What the preview pane should display for a selected result.
#[derive(Debug, Clone)]
pub enum PreviewContent {
    /// Display a source code file, scrolled to a specific line.
    Code {
        path: PathBuf,
        focus_line: usize,
        language: Option<String>,
    },
    /// Display plain text.
    Text(String),
    /// Display structured key-value pairs.
    KeyValue(Vec<(String, String)>),
    /// Nothing to preview.
    None,
}

/// An action that can be performed on a result entry.
#[derive(Debug, Clone)]
pub struct ChannelAction {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
}

/// The core extension interface. Every search mode implements this trait.
pub trait Channel: Send + Sync {
    /// Returns metadata describing this channel.
    fn metadata(&self) -> ChannelMetadata;

    /// Returns the input fields this channel needs displayed.
    fn input_fields(&self) -> Vec<InputFieldSpec>;

    /// Executes a search, streaming results to the sender.
    /// This is called on a background thread.
    fn search(&self, query: ChannelQuery, tx: Sender<SearchEvent>);

    /// Cancels any in-progress search.
    fn cancel(&self);

    /// Returns the preview content for a selected result.
    fn preview(&self, entry: &ResultEntry) -> PreviewContent;

    /// Returns available actions for a result entry.
    fn actions(&self, entry: &ResultEntry) -> Vec<ChannelAction>;

    /// Executes an action on a result entry.
    fn execute_action(&self, action_id: &str, entry: &ResultEntry) -> anyhow::Result<()>;
}
