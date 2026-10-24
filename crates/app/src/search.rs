//! Case-insensitive plain-text matching, shared by filtering and highlighting
//! so that whatever matches is also what gets highlighted.

use std::ops::Range;

/// Byte ranges of the non-overlapping, case-insensitive occurrences of
/// `needle` in `haystack`. Empty if `needle` is blank.
pub fn find(haystack: &str, needle: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = needle.trim().chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut resume = 0;
    for (start, _) in haystack.char_indices() {
        if start < resume {
            continue;
        }
        if let Some(end) = match_at(&haystack[start..], &needle) {
            found.push(start..start + end);
            resume = start + end;
        }
    }
    found
}

/// Length in bytes of the prefix of `s` that matches `needle`, if any.
fn match_at(s: &str, needle: &[char]) -> Option<usize> {
    let mut want = needle.iter();
    for (i, c) in s.char_indices() {
        for lower in c.to_lowercase() {
            if want.next() != Some(&lower) {
                return None;
            }
        }
        if want.len() == 0 {
            return Some(i + c.len_utf8());
        }
    }
    None
}

pub fn matches(haystack: &str, needle: &str) -> bool {
    !find(haystack, needle).is_empty()
}

/// Up to `radius` characters either side of the first match, with ellipses
/// where text was cut. `None` if there's no match.
pub fn snippet(haystack: &str, needle: &str, radius: usize) -> Option<String> {
    let first = find(haystack, needle).into_iter().next()?;
    let start = haystack[..first.start]
        .char_indices()
        .rev()
        .nth(radius.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    let end = haystack[first.end..]
        .char_indices()
        .nth(radius)
        .map_or(haystack.len(), |(i, _)| first.end + i);

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(haystack[start..end].trim());
    if end < haystack.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_case_insensitively() {
        assert_eq!(
            find("Culture Brewing culture", "CULTURE"),
            vec![0..7, 16..23]
        );
        assert_eq!(find("aaaa", "aa"), vec![0..2, 2..4]);
        assert!(find("anything", "  ").is_empty());
        assert!(find("Brew", "brewing").is_empty());
    }

    #[test]
    fn handles_non_ascii() {
        assert_eq!(find("Café Olé", "olé"), vec![6..10]);
        assert_eq!(find("İstanbul", "i̇st").len(), 1);
    }

    #[test]
    fn snippets_around_first_match() {
        let text = "Meet at the trailhead at dawn, then coffee afterwards.";
        assert_eq!(snippet(text, "dawn", 8).unwrap(), "…head at dawn, then c…");
        assert_eq!(snippet("short dawn", "dawn", 20).unwrap(), "short dawn");
        assert_eq!(snippet(text, "nope", 8), None);
    }
}
