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
}

impl ChannelQuery {
    pub fn field(&self, id: &str) -> &str {
        self.fields.get(id).map(|s| s.as_str()).unwrap_or("")
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
