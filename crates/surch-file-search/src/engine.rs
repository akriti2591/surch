use crossbeam_channel::Sender;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::UTF8;
use grep_searcher::Searcher;
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use surch_core::channel::{ChannelQuery, ResultEntry, SearchEvent};

pub fn run_search(query: ChannelQuery, tx: Sender<SearchEvent>, cancelled: Arc<AtomicBool>) {
    let pattern = query.field("find");
    if pattern.is_empty() {
        let _ = tx.send(SearchEvent::Complete {
            total_files: 0,
            total_matches: 0,
        });
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
