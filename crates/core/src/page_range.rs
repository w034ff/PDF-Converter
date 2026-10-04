//! Page range parsing and normalization (design §4.5).

use std::collections::BTreeSet;
use std::fmt;

/// Error returned when parsing a page range string fails (design §4.5, §6.6 `InvalidPageRange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePageRangeError {
    /// The erroneous element, or empty string if input was entirely empty or contained an empty element.
    pub detail: String,
}

impl fmt::Display for ParsePageRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.detail.is_empty() {
            write!(f, "empty or missing page range element")
        } else {
            write!(f, "invalid page range element: '{}'", self.detail)
        }
    }
}

impl std::error::Error for ParsePageRangeError {}

/// A sorted, deduplicated set of 1-based page numbers (design §4.5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageSet {
    pages: Vec<u32>,
}

impl PageSet {
    /// Creates a `PageSet` from a vector of 1-based page numbers.
    ///
    /// The pages are sorted and deduplicated.
    #[must_use]
    pub fn new(mut pages: Vec<u32>) -> Self {
        pages.sort_unstable();
        pages.dedup();
        Self { pages }
    }

    /// Returns the sorted, deduplicated 1-based page numbers.
    #[must_use]
    pub fn pages(&self) -> &[u32] {
        &self.pages
    }

    /// Returns `true` if the set contains no pages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Returns the number of unique pages in this set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Returns the page numbers that fall within `page_count` (1-based, inclusive).
    ///
    /// Page numbers exceeding `page_count` are omitted (design §4.5). If no pages
    /// fall within range, an empty vector is returned.
    #[must_use]
    pub fn pages_within(&self, page_count: u32) -> Vec<u32> {
        self.pages
            .iter()
            .copied()
            .filter(|&p| p <= page_count)
            .collect()
    }
}

/// Checks whether a character is an element separator (`,` or fullwidth `，` / `、`).
fn is_element_separator(c: char) -> bool {
    c == ',' || c == '\u{FF0C}' || c == '\u{3001}'
}

/// Checks whether a character is a range separator (`-` or fullwidth `－` / `〜` / `～`).
fn is_range_separator(c: char) -> bool {
    c == '-' || c == '\u{FF0D}' || c == '\u{301C}' || c == '\u{FF5E}'
}

/// Parses a page range string into a normalized [`PageSet`] (design §4.5).
///
/// Elements can be single page numbers (`n`) or ranges (`a-b`).
/// Separators may be halfwidth or fullwidth commas / Japanese commas.
/// Range dashes may be halfwidth or fullwidth hyphens or wave dashes.
/// Surrounding whitespace around elements and dashes is ignored.
///
/// # Errors
///
/// Returns [`ParsePageRangeError`] with the offending token in `detail` if:
/// - The input or any comma-separated element is empty.
/// - Any number cannot be parsed as a positive integer (e.g. non-digits, `0`).
/// - In a range `a-b`, `a > b`.
/// - An element contains multiple range dashes or trailing/leading dashes.
pub fn parse_page_range(text: &str) -> Result<PageSet, ParsePageRangeError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ParsePageRangeError {
            detail: String::new(),
        });
    }

    let mut pages = BTreeSet::new();

    for raw_part in text.split(is_element_separator) {
        let part = raw_part.trim();
        if part.is_empty() {
            return Err(ParsePageRangeError {
                detail: String::new(),
            });
        }

        let dash_chars: Vec<(usize, char)> = part
            .char_indices()
            .filter(|&(_, c)| is_range_separator(c))
            .collect();

        if dash_chars.len() > 1 {
            return Err(ParsePageRangeError {
                detail: part.to_string(),
            });
        }

        if let Some(&(dash_idx, dash_char)) = dash_chars.first() {
            let start_str = part[..dash_idx].trim();
            let end_str = part[dash_idx + dash_char.len_utf8()..].trim();

            if start_str.is_empty() || end_str.is_empty() {
                return Err(ParsePageRangeError {
                    detail: part.to_string(),
                });
            }

            let start = start_str.parse::<u32>().map_err(|_| ParsePageRangeError {
                detail: part.to_string(),
            })?;
            let end = end_str.parse::<u32>().map_err(|_| ParsePageRangeError {
                detail: part.to_string(),
            })?;

            if start == 0 || end == 0 || start > end {
                return Err(ParsePageRangeError {
                    detail: part.to_string(),
                });
            }

            for p in start..=end {
                pages.insert(p);
            }
        } else {
            let page = part.parse::<u32>().map_err(|_| ParsePageRangeError {
                detail: part.to_string(),
            })?;

            if page == 0 {
                return Err(ParsePageRangeError {
                    detail: part.to_string(),
                });
            }

            pages.insert(page);
        }
    }

    Ok(PageSet {
        pages: pages.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_page() {
        let set = parse_page_range("1").expect("valid range");
        assert_eq!(set.pages(), &[1]);
    }

    #[test]
    fn parses_ranges_and_singles() {
        let set = parse_page_range("1-3, 5").expect("valid range");
        assert_eq!(set.pages(), &[1, 2, 3, 5]);
    }

    #[test]
    fn ignores_whitespace() {
        let set = parse_page_range("  1  -  3  ,   5  ").expect("valid range");
        assert_eq!(set.pages(), &[1, 2, 3, 5]);

        let set_tabs_newlines = parse_page_range("\t 2 \n").expect("valid range");
        assert_eq!(set_tabs_newlines.pages(), &[2]);
    }

    #[test]
    fn parses_fullwidth_separators_and_dashes() {
        // Fullwidth comma and Japanese comma
        let set1 = parse_page_range("1，3、5").expect("valid range");
        assert_eq!(set1.pages(), &[1, 3, 5]);

        // Fullwidth wave dash (〜 U+301C) and fullwidth hyphen (－ U+FF0D)
        let set2 = parse_page_range("1〜3、5－7").expect("valid range");
        assert_eq!(set2.pages(), &[1, 2, 3, 5, 6, 7]);

        // Fullwidth tilde (～ U+FF5E)
        let set3 = parse_page_range("2～4").expect("valid range");
        assert_eq!(set3.pages(), &[2, 3, 4]);
    }

    #[test]
    fn normalizes_overlap_and_order() {
        // "5, 1-3, 2" -> [1, 2, 3, 5]
        let set = parse_page_range("5, 1-3, 2").expect("valid range");
        assert_eq!(set.pages(), &[1, 2, 3, 5]);

        let set_overlap = parse_page_range("4-6, 1-3, 2-5, 3").expect("valid range");
        assert_eq!(set_overlap.pages(), &[1, 2, 3, 4, 5, 6]);

        let set_duplicates = parse_page_range("3, 3, 3").expect("valid range");
        assert_eq!(set_duplicates.pages(), &[3]);
    }

    #[test]
    fn rejects_zero() {
        let err1 = parse_page_range("0").unwrap_err();
        assert_eq!(err1.detail, "0");

        let err2 = parse_page_range("0-3").unwrap_err();
        assert_eq!(err2.detail, "0-3");

        let err3 = parse_page_range("1-0").unwrap_err();
        assert_eq!(err3.detail, "1-0");

        let err4 = parse_page_range("1, 0, 3").unwrap_err();
        assert_eq!(err4.detail, "0");
    }

    #[test]
    fn rejects_inverted_range() {
        let err1 = parse_page_range("3-1").unwrap_err();
        assert_eq!(err1.detail, "3-1");

        let err2 = parse_page_range("5〜2").unwrap_err();
        assert_eq!(err2.detail, "5〜2");
    }

    #[test]
    fn rejects_non_digits() {
        let err1 = parse_page_range("abc").unwrap_err();
        assert_eq!(err1.detail, "abc");

        let err2 = parse_page_range("1-a").unwrap_err();
        assert_eq!(err2.detail, "1-a");

        let err3 = parse_page_range("b-3").unwrap_err();
        assert_eq!(err3.detail, "b-3");

        let err4 = parse_page_range("1, foo, 3").unwrap_err();
        assert_eq!(err4.detail, "foo");

        let err5 = parse_page_range("1-2-3").unwrap_err();
        assert_eq!(err5.detail, "1-2-3");

        let err6 = parse_page_range("-1").unwrap_err();
        assert_eq!(err6.detail, "-1");

        let err7 = parse_page_range("1-").unwrap_err();
        assert_eq!(err7.detail, "1-");
    }

    #[test]
    fn rejects_empty_string() {
        let err1 = parse_page_range("").unwrap_err();
        assert_eq!(err1.detail, "");

        let err2 = parse_page_range("   ").unwrap_err();
        assert_eq!(err2.detail, "");

        let err3 = parse_page_range("1,,2").unwrap_err();
        assert_eq!(err3.detail, "");

        let err4 = parse_page_range(",1").unwrap_err();
        assert_eq!(err4.detail, "");

        let err5 = parse_page_range("1,").unwrap_err();
        assert_eq!(err5.detail, "");
    }

    #[test]
    fn excludes_pages_exceeding_page_count() {
        let set = parse_page_range("1-5, 8, 12").expect("valid range");
        assert_eq!(set.pages_within(6), vec![1, 2, 3, 4, 5]);
        assert_eq!(set.pages_within(8), vec![1, 2, 3, 4, 5, 8]);
        assert_eq!(set.pages_within(20), vec![1, 2, 3, 4, 5, 8, 12]);
    }

    #[test]
    fn returns_empty_when_no_pages_remain() {
        let set = parse_page_range("5-10").expect("valid range");
        assert_eq!(set.pages_within(4), Vec::<u32>::new());
        assert_eq!(set.pages_within(0), Vec::<u32>::new());
    }
}
