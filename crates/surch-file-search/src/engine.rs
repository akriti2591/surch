use crossbeam_channel::Sender;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::Searcher;
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use surch_core::channel::{ChannelQuery, ResultEntry, SearchEvent};
use surch_core::fuzzy::fuzzy_match;

pub fn run_search(query: ChannelQuery, tx: Sender<SearchEvent>, cancelled: Arc<AtomicBool>) {
    let pattern = query.field("find");
    if pattern.is_empty() {
        let _ = tx.send(SearchEvent::Complete {
            total_files: 0,
            total_matches: 0,
        });
        return;
    }

    if query.fuzzy {
        run_fuzzy_search(query, tx, cancelled);
        return;
    }

    let include = query.field("include");
    let exclude = query.field("exclude");

    // Build the regex matcher
    let matcher = if query.is_regex {
        RegexMatcher::new_line_matcher(pattern)
    } else {
        // Escape the pattern for literal search
        let escaped = regex::escape(pattern);
        if query.whole_word {
            RegexMatcher::new_line_matcher(&format!(r"\b{}\b", escaped))
        } else {
            let mut builder = grep_regex::RegexMatcherBuilder::new();
            builder.case_insensitive(!query.case_sensitive);
            builder.build(&escaped)
        }
    };

    let matcher = match matcher {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(SearchEvent::Error(format!("Invalid pattern: {}", e)));
            return;
        }
    };

    // Build directory walker with include/exclude globs
    let mut walk_builder = WalkBuilder::new(&query.workspace_root);
    walk_builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .threads(num_cpus::get().min(12)); // Use available cores, cap at 12

    // Apply include/exclude overrides
    let mut override_builder = OverrideBuilder::new(&query.workspace_root);
    let mut has_overrides = false;

    if !include.is_empty() {
        for glob in include.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if override_builder.add(glob).is_ok() {
                has_overrides = true;
            }
        }
    }
    if !exclude.is_empty() {
        for glob in exclude.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let negated = format!("!{}", glob);
            if override_builder.add(&negated).is_ok() {
                has_overrides = true;
            }
        }
    }

    if has_overrides {
        if let Ok(overrides) = override_builder.build() {
            walk_builder.overrides(overrides);
        }
    }

    let id_counter = Arc::new(AtomicU64::new(0));
    let files_searched = Arc::new(AtomicUsize::new(0));
    let total_matches = Arc::new(AtomicUsize::new(0));

    // Use parallel walker — same approach as ripgrep.
    // build_parallel() uses ignore's internal thread pool for directory traversal.
    // Each thread gets its own Searcher (they're not Send).
    let matcher = Arc::new(matcher);
    let pattern = Arc::new(pattern.to_string());
    let case_sensitive = query.case_sensitive;

    walk_builder.build_parallel().run(|| {
        let tx = tx.clone();
        let cancelled = cancelled.clone();
        let matcher = matcher.clone();
        let pattern = pattern.clone();
        let id_counter = id_counter.clone();
        let files_searched = files_searched.clone();
        let total_matches = total_matches.clone();

        Box::new(move |entry| {
            if cancelled.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            let path = entry.path();
            if !path.is_file() {
                return WalkState::Continue;
            }

            let count = files_searched.fetch_add(1, Ordering::Relaxed);

            // Send progress every 100 files
            if count % 100 == 0 {
                let _ = tx.send(SearchEvent::Progress {
                    files_searched: count,
                    matches_found: total_matches.load(Ordering::Relaxed),
                });
            }

            let mut searcher = Searcher::new();
            let tx_clone = tx.clone();
            let path_buf = path.to_path_buf();

            let result = searcher.search_path(
                matcher.as_ref(),
                path,
                UTF8(|line_number, line_content| {
                    if cancelled.load(Ordering::Relaxed) {
                        return Ok(false);
                    }

                    // Find match ranges within the line
                    let match_ranges =
                        find_match_ranges(line_content, &pattern, case_sensitive);

                    let entry = ResultEntry {
                        id: id_counter.fetch_add(1, Ordering::Relaxed),
                        file_path: Some(path_buf.clone()),
                        line_number: Some(line_number as usize),
                        column: match_ranges.first().map(|r| r.start),
                        line_content: line_content.trim_end().to_string(),
                        match_ranges,
                    };

                    total_matches.fetch_add(1, Ordering::Relaxed);
                    let _ = tx_clone.send(SearchEvent::Match(entry));
                    Ok(true)
                }),
            );

            if let Err(e) = result {
                let _ = tx.send(SearchEvent::Error(format!(
                    "Error searching {}: {}",
                    path.display(),
                    e
                )));
            }

            WalkState::Continue
        })
    });

    let _ = tx.send(SearchEvent::Complete {
        total_files: files_searched.load(Ordering::Relaxed),
        total_matches: total_matches.load(Ordering::Relaxed),
    });
}

fn run_fuzzy_search(query: ChannelQuery, tx: Sender<SearchEvent>, cancelled: Arc<AtomicBool>) {
    let pattern = query.field("find").to_string();
    let case_sensitive = query.case_sensitive;

    let include = query.field("include").to_string();
    let exclude = query.field("exclude").to_string();

    let mut walk_builder = WalkBuilder::new(&query.workspace_root);
    walk_builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .threads(num_cpus::get().min(12));

    let mut override_builder = OverrideBuilder::new(&query.workspace_root);
    let mut has_overrides = false;

    if !include.is_empty() {
        for glob in include.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if override_builder.add(glob).is_ok() {
                has_overrides = true;
            }
        }
    }
    if !exclude.is_empty() {
        for glob in exclude.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let negated = format!("!{}", glob);
            if override_builder.add(&negated).is_ok() {
                has_overrides = true;
            }
        }
    }

    if has_overrides {
        if let Ok(overrides) = override_builder.build() {
            walk_builder.overrides(overrides);
        }
    }

    let id_counter = Arc::new(AtomicU64::new(0));
    let files_searched = Arc::new(AtomicUsize::new(0));
    let total_matches = Arc::new(AtomicUsize::new(0));
    let pattern = Arc::new(pattern);

    walk_builder.build_parallel().run(|| {
        let tx = tx.clone();
        let cancelled = cancelled.clone();
        let pattern = pattern.clone();
        let id_counter = id_counter.clone();
        let files_searched = files_searched.clone();
        let total_matches = total_matches.clone();

        Box::new(move |entry| {
            if cancelled.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            let path = entry.path();
            if !path.is_file() {
                return WalkState::Continue;
            }

            let count = files_searched.fetch_add(1, Ordering::Relaxed);
            if count % 100 == 0 {
                let _ = tx.send(SearchEvent::Progress {
                    files_searched: count,
                    matches_found: total_matches.load(Ordering::Relaxed),
                });
            }

            // Read file and fuzzy match each line
            let file = match std::fs::File::open(path) {
                Ok(f) => f,
                Err(_) => return WalkState::Continue,
            };

            let reader = std::io::BufReader::new(file);
            let path_buf = path.to_path_buf();

            for (line_idx, line_result) in reader.lines().enumerate() {
                if cancelled.load(Ordering::Relaxed) {
                    return WalkState::Quit;
                }

                let line = match line_result {
                    Ok(l) => l,
                    Err(_) => break, // Binary file or encoding error
                };

                if let Some(fm) = fuzzy_match(&pattern, &line, case_sensitive) {
                    let entry = ResultEntry {
                        id: id_counter.fetch_add(1, Ordering::Relaxed),
                        file_path: Some(path_buf.clone()),
                        line_number: Some(line_idx + 1),
                        column: fm.matched_ranges.first().map(|r| r.start),
                        line_content: line,
                        match_ranges: fm.matched_ranges,
                    };

                    total_matches.fetch_add(1, Ordering::Relaxed);
                    let _ = tx.send(SearchEvent::Match(entry));
                }
            }

            WalkState::Continue
        })
    });

    let _ = tx.send(SearchEvent::Complete {
        total_files: files_searched.load(Ordering::Relaxed),
        total_matches: total_matches.load(Ordering::Relaxed),
    });
}

/// Replace all occurrences of a pattern in files.
/// Processes files in parallel, streaming progress back to the caller.
pub fn run_replace(
    query: ChannelQuery,
    replacement: &str,
    tx: Sender<SearchEvent>,
    cancelled: Arc<AtomicBool>,
) -> (usize, usize) {
    let pattern = query.field("find");
    if pattern.is_empty() {
        return (0, 0);
    }

    // First, run a search to find all matches
    let (search_tx, search_rx) = crossbeam_channel::unbounded();
    let cancelled_clone = cancelled.clone();
    run_search(query.clone(), search_tx, cancelled_clone);

    // Group matches by file
    let mut file_matches: std::collections::HashMap<PathBuf, Vec<(usize, String, Vec<std::ops::Range<usize>>)>> =
        std::collections::HashMap::new();

    for event in search_rx {
        match event {
            SearchEvent::Match(entry) => {
                if let Some(ref path) = entry.file_path {
                    file_matches
                        .entry(path.clone())
                        .or_default()
                        .push((
                            entry.line_number.unwrap_or(0),
                            entry.line_content.clone(),
                            entry.match_ranges.clone(),
                        ));
                }
            }
            SearchEvent::Complete { .. } => break,
            SearchEvent::Error(_) => {}
            SearchEvent::Progress { .. } => {}
        }
    }

    let mut total_replacements = 0usize;
    let mut files_modified = 0usize;

    for (path, matches) in &file_matches {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        // Handle trailing newline
        let ends_with_newline = content.ends_with('\n');
        let mut file_changed = false;

        // Sort matches by line number descending so replacements don't shift offsets
        let mut sorted_matches: Vec<&(usize, String, Vec<std::ops::Range<usize>>)> =
            matches.iter().collect();
        sorted_matches.sort_by(|a, b| b.0.cmp(&a.0));

        for (line_num, _line_content, match_ranges) in sorted_matches {
            let idx = line_num.saturating_sub(1);
            if idx >= lines.len() {
                continue;
            }

            let line = &lines[idx];
            // Apply replacements in reverse order within the line
            let mut new_line = line.clone();
            let mut sorted_ranges: Vec<&std::ops::Range<usize>> = match_ranges.iter().collect();
            sorted_ranges.sort_by(|a, b| b.start.cmp(&a.start));

            for range in sorted_ranges {
                if range.end <= new_line.len() {
                    let actual_replacement = if query.preserve_case {
                        let original = &new_line[range.clone()];
                        surch_core::channel::apply_case_pattern(original, replacement)
                    } else {
                        replacement.to_string()
                    };
                    new_line.replace_range(range.clone(), &actual_replacement);
                    total_replacements += 1;
                    file_changed = true;
                }
            }

            lines[idx] = new_line;
        }

        if file_changed {
            let mut output = lines.join("\n");
            if ends_with_newline {
                output.push('\n');
            }
            if std::fs::write(path, &output).is_ok() {
                files_modified += 1;
            }
        }
    }

    let _ = tx.send(SearchEvent::Complete {
        total_files: files_modified,
        total_matches: total_replacements,
    });

    (total_replacements, files_modified)
}

/// Find byte ranges of pattern matches within a line.
fn find_match_ranges(
    line: &str,
    pattern: &str,
    case_sensitive: bool,
) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    if pattern.is_empty() {
        return ranges;
    }

    let (haystack, needle) = if case_sensitive {
        (line.to_string(), pattern.to_string())
    } else {
        (line.to_lowercase(), pattern.to_lowercase())
    };

    let mut start = 0;
    while let Some(pos) = haystack[start..].find(&needle) {
        let abs_start = start + pos;
        let abs_end = abs_start + pattern.len();
        ranges.push(abs_start..abs_end);
        start = abs_end;
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn make_query(dir: &std::path::Path, find: &str) -> ChannelQuery {
        let mut fields = HashMap::new();
        fields.insert("find".to_string(), find.to_string());
        fields.insert("include".to_string(), String::new());
        fields.insert("exclude".to_string(), String::new());
        ChannelQuery {
            fields,
            workspace_root: dir.to_path_buf(),
            is_regex: false,
            case_sensitive: true,
            whole_word: false,
            preserve_case: false,
            fuzzy: false,
        }
    }

    fn collect_results(rx: &crossbeam_channel::Receiver<SearchEvent>) -> Vec<ResultEntry> {
        let mut results = Vec::new();
        for event in rx {
            match event {
                SearchEvent::Match(entry) => results.push(entry),
                SearchEvent::Complete { .. } => break,
                _ => {}
            }
        }
        results
    }

    #[test]
    fn test_find_match_ranges_basic() {
        let ranges = find_match_ranges("hello world hello", "hello", true);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], 0..5);
        assert_eq!(ranges[1], 12..17);
    }

    #[test]
    fn test_find_match_ranges_case_insensitive() {
        let ranges = find_match_ranges("Hello HELLO hello", "hello", false);
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn test_find_match_ranges_empty_pattern() {
        let ranges = find_match_ranges("hello", "", true);
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_find_match_ranges_no_match() {
        let ranges = find_match_ranges("hello world", "xyz", true);
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_search_basic() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world\nfoo bar\nhello again\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 2);
        assert!(results[0].line_content.contains("hello"));
        assert!(results[1].line_content.contains("hello"));
    }

    #[test]
    fn test_search_case_insensitive() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "Hello\nhello\nHELLO\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "hello");
        query.case_sensitive = false;

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_case_sensitive() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "Hello\nhello\nHELLO\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello"); // case_sensitive = true by default

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert!(results[0].line_content.contains("hello"));
    }

    #[test]
    fn test_search_regex() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "foo123\nbar456\nfoo789\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), r"foo\d+");
        query.is_regex = true;

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_whole_word() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "for\nformat\nforever\ntransform\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "for");
        query.whole_word = true;
        query.case_sensitive = false;

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_content, "for");
    }

    #[test]
    fn test_search_include_glob() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.rs"), "fn hello() {}\n").unwrap();
        fs::write(dir.path().join("test.txt"), "hello world\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "hello");
        query.fields.insert("include".to_string(), "*.rs".to_string());

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert!(results[0].file_path.as_ref().unwrap().to_string_lossy().ends_with(".rs"));
    }

    #[test]
    fn test_search_cancellation() {
        let dir = TempDir::new().unwrap();
        // Create many files to ensure search takes time
        for i in 0..50 {
            fs::write(dir.path().join(format!("file{}.txt", i)), "needle in haystack\n".repeat(100)).unwrap();
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "needle");

        // Cancel immediately
        cancelled.store(true, Ordering::SeqCst);

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        // Should have found very few or no results due to cancellation
        assert!(results.len() < 5000); // 50 files * 100 lines = 5000 max
    }

    #[test]
    fn test_search_respects_gitignore() {
        let dir = TempDir::new().unwrap();
        // ignore crate needs a .git dir to recognize .gitignore
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir(dir.path().join("ignored")).unwrap();
        fs::write(dir.path().join("ignored/test.txt"), "hello\n").unwrap();
        fs::write(dir.path().join("visible.txt"), "hello\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert!(results[0].file_path.as_ref().unwrap().to_string_lossy().contains("visible"));
    }

    #[test]
    fn test_replace_basic() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world\nfoo hello bar\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        let (replacements, files) = run_replace(query, "goodbye", tx, cancelled);

        assert_eq!(replacements, 2);
        assert_eq!(files, 1);

        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert!(content.contains("goodbye world"));
        assert!(content.contains("foo goodbye bar"));
        assert!(!content.contains("hello"));
    }

    #[test]
    fn test_replace_preserves_trailing_newline() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        run_replace(query, "goodbye", tx, cancelled);

        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "goodbye\n");
    }

    #[test]
    fn test_replace_multiple_matches_per_line() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "foo foo foo\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "foo");

        let (replacements, _) = run_replace(query, "bar", tx, cancelled);

        assert_eq!(replacements, 3);
        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "bar bar bar\n");
    }

    #[test]
    fn test_search_exclude_glob() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("keep.rs"), "hello\n").unwrap();
        fs::write(dir.path().join("skip.log"), "hello\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "hello");
        query.fields.insert("exclude".to_string(), "*.log".to_string());

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert!(results[0].file_path.as_ref().unwrap().to_string_lossy().ends_with(".rs"));
    }

    #[test]
    fn test_search_empty_pattern_sends_complete() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "");

        run_search(query, tx, cancelled);

        // Should get a Complete event with 0 matches
        let mut got_complete = false;
        for event in rx {
            if let SearchEvent::Complete { total_files, total_matches } = event {
                assert_eq!(total_files, 0);
                assert_eq!(total_matches, 0);
                got_complete = true;
                break;
            }
        }
        assert!(got_complete, "Should receive Complete event for empty pattern");
    }

    #[test]
    fn test_search_invalid_regex_sends_error() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "[invalid(regex");
        query.is_regex = true;

        run_search(query, tx, cancelled);

        let mut got_error = false;
        for event in rx {
            if let SearchEvent::Error(msg) = event {
                assert!(msg.contains("Invalid pattern") || msg.contains("pattern"));
                got_error = true;
                break;
            }
        }
        assert!(got_error, "Should receive Error event for invalid regex");
    }

    #[test]
    fn test_search_empty_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("empty.txt"), "").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert!(results.is_empty());
    }

    #[test]
    fn test_search_unicode_content() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("unicode.txt"), "こんにちは世界\nrust は素晴らしい\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "世界");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert!(results[0].line_content.contains("世界"));
    }

    #[test]
    fn test_search_multiple_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "needle here\n").unwrap();
        fs::write(dir.path().join("b.txt"), "no match\n").unwrap();
        fs::write(dir.path().join("c.txt"), "another needle\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "needle");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_subdirectories() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("deep.txt"), "found me\n").unwrap();
        fs::write(dir.path().join("top.txt"), "found me\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "found");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_result_has_line_numbers() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "line one\nline two\nline three\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "two");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_number, Some(2));
    }

    #[test]
    fn test_search_result_has_match_ranges() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world hello\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        assert_eq!(results.len(), 1);
        assert!(!results[0].match_ranges.is_empty());
    }

    #[test]
    fn test_search_sends_complete_event() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "hello");

        run_search(query, tx, cancelled);

        let mut got_complete = false;
        for event in rx {
            if let SearchEvent::Complete { total_matches, .. } = event {
                assert!(total_matches >= 1);
                got_complete = true;
                break;
            }
        }
        assert!(got_complete);
    }

    #[test]
    fn test_replace_case_insensitive() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "Hello HELLO hello\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "hello");
        query.case_sensitive = false;

        let (replacements, _) = run_replace(query, "hi", tx, cancelled);

        assert_eq!(replacements, 3);
        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "hi hi hi\n");
    }

    #[test]
    fn test_replace_no_matches() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "xyz");

        let (replacements, files) = run_replace(query, "abc", tx, cancelled);

        assert_eq!(replacements, 0);
        assert_eq!(files, 0);

        // File should be unchanged
        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "hello world\n");
    }

    #[test]
    fn test_replace_multiple_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "foo bar\n").unwrap();
        fs::write(dir.path().join("b.txt"), "baz foo\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "foo");

        let (replacements, files) = run_replace(query, "qux", tx, cancelled);

        assert_eq!(replacements, 2);
        assert_eq!(files, 2);

        let a = fs::read_to_string(dir.path().join("a.txt")).unwrap();
        let b = fs::read_to_string(dir.path().join("b.txt")).unwrap();
        assert!(a.contains("qux"));
        assert!(b.contains("qux"));
        assert!(!a.contains("foo"));
        assert!(!b.contains("foo"));
    }

    #[test]
    fn test_replace_with_include_glob() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("code.rs"), "let foo = 1;\n").unwrap();
        fs::write(dir.path().join("text.txt"), "foo bar\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "foo");
        query.fields.insert("include".to_string(), "*.rs".to_string());

        let (replacements, files) = run_replace(query, "bar", tx, cancelled);

        assert_eq!(files, 1);
        assert_eq!(replacements, 1);

        // Only .rs file should be modified
        let rs_content = fs::read_to_string(dir.path().join("code.rs")).unwrap();
        assert!(rs_content.contains("let bar = 1;"));

        // .txt file should be unchanged
        let txt_content = fs::read_to_string(dir.path().join("text.txt")).unwrap();
        assert_eq!(txt_content, "foo bar\n");
    }

    #[test]
    fn test_find_match_ranges_special_regex_chars() {
        // Ensure literal search handles regex metacharacters
        let ranges = find_match_ranges("foo.bar(baz)", "foo.bar", true);
        // This is a literal match function — "." should match literal "."
        assert!(!ranges.is_empty());
    }

    #[test]
    fn test_replace_with_exclude_glob() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("keep.rs"), "foo bar\n").unwrap();
        fs::write(dir.path().join("skip.log"), "foo baz\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "foo");
        query.fields.insert("exclude".to_string(), "*.log".to_string());

        let (replacements, files) = run_replace(query, "qux", tx, cancelled);

        assert_eq!(files, 1);
        assert_eq!(replacements, 1);

        let rs = fs::read_to_string(dir.path().join("keep.rs")).unwrap();
        assert!(rs.contains("qux"));

        let log = fs::read_to_string(dir.path().join("skip.log")).unwrap();
        assert_eq!(log, "foo baz\n"); // unchanged
    }

    #[test]
    fn test_search_include_multiple_globs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.rs"), "needle\n").unwrap();
        fs::write(dir.path().join("b.py"), "needle\n").unwrap();
        fs::write(dir.path().join("c.txt"), "needle\n").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "needle");
        query.fields.insert("include".to_string(), "*.rs, *.py".to_string());

        run_search(query, tx, cancelled);
        let results = collect_results(&rx);

        // Should only find matches in .rs and .py, not .txt
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_replace_preserve_case() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello Hello HELLO\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut query = make_query(dir.path(), "hello");
        query.case_sensitive = false;
        query.preserve_case = true;

        let (replacements, _) = run_replace(query, "world", tx, cancelled);

        assert_eq!(replacements, 3);
        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert!(content.contains("world"));
        assert!(content.contains("World"));
        assert!(content.contains("WORLD"));
    }

    #[test]
    fn test_replace_empty_pattern() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello\n").unwrap();

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancelled = Arc::new(AtomicBool::new(false));
        let query = make_query(dir.path(), "");

        let (replacements, files) = run_replace(query, "world", tx, cancelled);

        assert_eq!(replacements, 0);
        assert_eq!(files, 0);
    }

    #[test]
    fn test_find_match_ranges_unicode() {
        let ranges = find_match_ranges("hello 世界 world", "世界", true);
        assert_eq!(ranges.len(), 1);
    }
}
