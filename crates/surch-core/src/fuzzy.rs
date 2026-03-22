use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::ops::Range;

/// Result of a fuzzy match: the score and the byte ranges of matched characters.
pub struct FuzzyMatch {
    pub score: u32,
    pub matched_ranges: Vec<Range<usize>>,
}

/// Perform a fuzzy match of `query` against `text`.
///
/// Returns `None` if no match. Returns `Some(FuzzyMatch)` with the nucleo score
/// and byte ranges of matched characters (for highlighting).
///
/// `case_sensitive` controls whether matching is case-sensitive.
pub fn fuzzy_match(query: &str, text: &str, case_sensitive: bool) -> Option<FuzzyMatch> {
    if query.is_empty() || text.is_empty() {
        return None;
    }

    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let case_matching = if case_sensitive {
        CaseMatching::Respect
    } else {
        CaseMatching::Ignore
    };
    let pattern = Pattern::new(query, case_matching, Normalization::Smart, AtomKind::Fuzzy);

    // Convert text to Utf32Str for nucleo
    let mut buf = Vec::new();
    let haystack = Utf32Str::new(text, &mut buf);

    // Get indices of matched characters
    let mut indices = Vec::new();
    let score = pattern.indices(haystack, &mut matcher, &mut indices)?;

    if score == 0 {
        return None;
    }

    // Sort indices (nucleo returns them in match order, not position order)
    indices.sort_unstable();

    // Convert char indices to byte ranges, merging consecutive ranges
    let matched_ranges = char_indices_to_byte_ranges(text, &indices);

    Some(FuzzyMatch {
        score,
        matched_ranges,
    })
}

/// Convert sorted char indices into merged byte ranges.
fn char_indices_to_byte_ranges(text: &str, char_indices: &[u32]) -> Vec<Range<usize>> {
    if char_indices.is_empty() {
        return Vec::new();
    }

    let char_byte_offsets: Vec<(usize, usize)> = text
        .char_indices()
        .map(|(byte_pos, ch)| (byte_pos, byte_pos + ch.len_utf8()))
        .collect();

    let mut ranges: Vec<Range<usize>> = Vec::new();

    for &ci in char_indices {
        let ci = ci as usize;
        if ci >= char_byte_offsets.len() {
            continue;
        }
        let (start, end) = char_byte_offsets[ci];

        // Merge with previous range if contiguous
        if let Some(last) = ranges.last_mut() {
            if last.end == start {
                last.end = end;
                continue;
            }
        }
        ranges.push(start..end);
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_basic() {
        let result = fuzzy_match("srch", "search", false);
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(m.score > 0);
        assert!(!m.matched_ranges.is_empty());
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        let result = fuzzy_match("xyz", "search", false);
        assert!(result.is_none());
    }

    #[test]
    fn test_fuzzy_match_empty_query() {
        assert!(fuzzy_match("", "search", false).is_none());
    }

    #[test]
    fn test_fuzzy_match_empty_text() {
        assert!(fuzzy_match("foo", "", false).is_none());
    }

    #[test]
    fn test_fuzzy_match_exact() {
        let result = fuzzy_match("search", "search", false).unwrap();
        assert!(result.score > 0);
        // Should match the entire string
        assert_eq!(result.matched_ranges, vec![0..6]);
    }

    #[test]
    fn test_fuzzy_match_case_insensitive() {
        let result = fuzzy_match("SEARCH", "search", false);
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_match_case_sensitive() {
        let result = fuzzy_match("SEARCH", "search", true);
        assert!(result.is_none());
    }

    #[test]
    fn test_fuzzy_match_case_sensitive_match() {
        let result = fuzzy_match("search", "search", true);
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_match_subsequence() {
        // "sp" should match "search_panel"
        let result = fuzzy_match("sp", "search_panel", false);
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_match_camel_case() {
        let result = fuzzy_match("hqc", "handleQueryChanged", false);
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_match_ranges_are_byte_ranges() {
        // Test with ASCII — byte ranges should equal char ranges
        let result = fuzzy_match("fn", "function", false).unwrap();
        for range in &result.matched_ranges {
            let _ = &"function"[range.clone()]; // Should not panic
        }
    }

    #[test]
    fn test_fuzzy_match_unicode() {
        let result = fuzzy_match("hlo", "héllo", false);
        assert!(result.is_some());
        let m = result.unwrap();
        // Verify ranges are valid byte ranges
        for range in &m.matched_ranges {
            let _ = &"héllo"[range.clone()];
        }
    }

    #[test]
    fn test_fuzzy_match_ranges_merged() {
        // Exact prefix match should produce a single merged range
        let result = fuzzy_match("sea", "search", false).unwrap();
        assert_eq!(result.matched_ranges, vec![0..3]);
    }

    #[test]
    fn test_char_indices_to_byte_ranges_empty() {
        assert!(char_indices_to_byte_ranges("hello", &[]).is_empty());
    }

    #[test]
    fn test_char_indices_to_byte_ranges_merges_consecutive() {
        // Indices 0, 1, 2 in "hello" -> single range 0..3
        let ranges = char_indices_to_byte_ranges("hello", &[0, 1, 2]);
        assert_eq!(ranges, vec![0..3]);
    }

    #[test]
    fn test_char_indices_to_byte_ranges_non_consecutive() {
        // Indices 0, 2, 4 in "hello" -> three separate ranges
        let ranges = char_indices_to_byte_ranges("hello", &[0, 2, 4]);
        assert_eq!(ranges, vec![0..1, 2..3, 4..5]);
    }

    #[test]
    fn test_char_indices_to_byte_ranges_out_of_bounds() {
        let ranges = char_indices_to_byte_ranges("hi", &[0, 1, 99]);
        assert_eq!(ranges, vec![0..2]);
    }
}
