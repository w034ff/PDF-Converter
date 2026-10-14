//! Page range parsing and normalization (design §4.5).

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

/// The word that stands for a PDF's last page in a range (design §4.5).
const LAST_PAGE_KEYWORD: &str = "end";

/// A normalized set of 1-based page intervals (design §4.5).
///
/// Intervals are kept as `(start, end)` pairs (inclusive) rather than expanding
/// into individual page numbers, so inputs like `1-4000000000` consume negligible
/// memory and parse instantaneously.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageSet {
    /// Sorted, disjoint, non-adjacent intervals `(start, end)` where `1 <= start <= end`.
    /// `n-end` is `(n, u32::MAX)`, which every PDF clamps to its own last page.
    intervals: Vec<(u32, u32)>,
    /// Whether `end` stood alone, selecting each PDF's last page. Its number
    /// differs per PDF, so it cannot be an interval.
    last_page: bool,
}

impl PageSet {
    /// Returns the normalized intervals `(start, end)` (1-based, inclusive).
    #[must_use]
    pub fn intervals(&self) -> &[(u32, u32)] {
        &self.intervals
    }

    /// Returns `true` if the range named each PDF's last page with `end` alone.
    /// The page is not in [`Self::intervals`].
    #[must_use]
    pub fn last_page(&self) -> bool {
        self.last_page
    }

    /// Returns `true` if the set selects no page in any PDF.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty() && !self.last_page
    }

    /// Whether the last page of a PDF with `page_count` pages must be added
    /// to what the intervals select there.
    fn adds_last_page(&self, page_count: u32) -> bool {
        self.last_page
            && page_count > 0
            && !self
                .intervals
                .iter()
                .any(|&(start, end)| start <= page_count && page_count <= end)
    }

    /// Returns the number of selected pages that fall within `page_count` (1-based, inclusive).
    ///
    /// Only intervals up to `page_count` are counted (design §4.5).
    #[must_use]
    pub fn count_within(&self, page_count: u32) -> u32 {
        if page_count == 0 {
            return 0;
        }
        let mut total = 0u32;
        for &(start, end) in &self.intervals {
            if start > page_count {
                break;
            }
            let bounded_end = end.min(page_count);
            let count = bounded_end - start + 1;
            total = total.saturating_add(count);
        }
        if self.adds_last_page(page_count) {
            total = total.saturating_add(1);
        }
        total
    }

    /// Returns the 1-based page numbers that fall within `page_count` (inclusive).
    ///
    /// Page numbers exceeding `page_count` are omitted (design §4.5). If no pages
    /// fall within range, an empty vector is returned.
    #[must_use]
    pub fn pages_within(&self, page_count: u32) -> Vec<u32> {
        if page_count == 0 {
            return Vec::new();
        }
        let mut result = Vec::new();
        for &(start, end) in &self.intervals {
            if start > page_count {
                break;
            }
            let bounded_end = end.min(page_count);
            for page in start..=bounded_end {
                result.push(page);
            }
        }
        // The last page is above every page pushed so far, so the order holds.
        if self.adds_last_page(page_count) {
            result.push(page_count);
        }
        result
    }
}

/// Parses a 1-based positive integer accepting only halfwidth digits `0`-`9`
/// and fullwidth digits `０`-`９`.
///
/// Leading `+`, `-`, or other non-digit characters are rejected (design §4.5).
/// Returns `None` if input is empty, contains non-digits, overflows `u32`, or evaluates to `0`.
fn parse_page_number(s: &str) -> Option<u32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut val: u32 = 0;
    for c in trimmed.chars() {
        let digit = match c {
            '0'..='9' => c as u32 - '0' as u32,
            '\u{FF10}'..='\u{FF19}' => c as u32 - '\u{FF10}' as u32,
            _ => return None,
        };
        val = val.checked_mul(10)?.checked_add(digit)?;
    }
    if val == 0 {
        return None;
    }
    Some(val)
}

/// One side of a range element: a page number or `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bound {
    Page(u32),
    LastPage,
}

/// Parses a page number or the lowercase [`LAST_PAGE_KEYWORD`].
fn parse_bound(s: &str) -> Option<Bound> {
    if s.trim() == LAST_PAGE_KEYWORD {
        Some(Bound::LastPage)
    } else {
        parse_page_number(s).map(Bound::Page)
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
/// Elements can be single page numbers (`n`) or ranges (`a-b`). `end` (lowercase
/// only) stands for each PDF's last page: alone, as the end of a range (`a-end`),
/// or as both sides (`end-end`).
/// Separators may be halfwidth or fullwidth commas / Japanese commas.
/// Range dashes may be halfwidth or fullwidth hyphens or wave dashes.
/// Surrounding whitespace (including fullwidth space) around elements and dashes is ignored.
///
/// # Errors
///
/// Returns [`ParsePageRangeError`] with the offending token in `detail` if:
/// - The input or any comma-separated element is empty.
/// - Any number contains characters other than halfwidth `0`-`9` or fullwidth `０`-`９` (e.g. `+`, letters).
/// - Any number is `0` or overflows `u32`.
/// - In a range `a-b`, `a > b`, or `a` is `end` and `b` is a number. Other
///   tools read `end-1` as the pages in reverse order, which this set cannot hold.
/// - An element contains multiple range dashes or trailing/leading dashes.
pub fn parse_page_range(text: &str) -> Result<PageSet, ParsePageRangeError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ParsePageRangeError {
            detail: String::new(),
        });
    }

    let mut raw_intervals = Vec::new();
    let mut last_page = false;

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

            match (parse_bound(start_str), parse_bound(end_str)) {
                (Some(Bound::Page(start)), Some(Bound::Page(end))) if start <= end => {
                    raw_intervals.push((start, end));
                }
                (Some(Bound::Page(start)), Some(Bound::LastPage)) => {
                    raw_intervals.push((start, u32::MAX));
                }
                (Some(Bound::LastPage), Some(Bound::LastPage)) => last_page = true,
                _ => {
                    return Err(ParsePageRangeError {
                        detail: part.to_string(),
                    });
                }
            }
        } else {
            match parse_bound(part) {
                Some(Bound::Page(page)) => raw_intervals.push((page, page)),
                Some(Bound::LastPage) => last_page = true,
                None => {
                    return Err(ParsePageRangeError {
                        detail: part.to_string(),
                    });
                }
            }
        }
    }

    // Sort by start, then end.
    raw_intervals.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Merge overlapping and adjacent intervals into canonical disjoint form.
    let mut intervals: Vec<(u32, u32)> = Vec::new();
    for (start, end) in raw_intervals {
        if let Some(last) = intervals.last_mut() {
            // If start <= last.end + 1, the interval overlaps or is immediately adjacent.
            if start <= last.1.saturating_add(1) {
                last.1 = last.1.max(end);
            } else {
                intervals.push((start, end));
            }
        } else {
            intervals.push((start, end));
        }
    }

    Ok(PageSet {
        intervals,
        last_page,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_page() {
        let set = parse_page_range("1").expect("valid range");
        assert_eq!(set.intervals(), &[(1, 1)]);
        assert_eq!(set.pages_within(5), vec![1]);
        assert_eq!(set.count_within(5), 1);
    }

    #[test]
    fn parses_ranges_and_singles() {
        let set = parse_page_range("1-3, 5").expect("valid range");
        assert_eq!(set.intervals(), &[(1, 3), (5, 5)]);
        assert_eq!(set.pages_within(10), vec![1, 2, 3, 5]);
        assert_eq!(set.count_within(10), 4);
    }

    #[test]
    fn ignores_whitespace() {
        let set = parse_page_range("  1  -  3  ,   5  ").expect("valid range");
        assert_eq!(set.intervals(), &[(1, 3), (5, 5)]);
        assert_eq!(set.pages_within(10), vec![1, 2, 3, 5]);

        let set_tabs_newlines = parse_page_range("\t 2 \n").expect("valid range");
        assert_eq!(set_tabs_newlines.intervals(), &[(2, 2)]);
        assert_eq!(set_tabs_newlines.pages_within(5), vec![2]);
    }

    #[test]
    fn parses_fullwidth_digits_and_whitespace() {
        let set1 = parse_page_range("１－３").expect("valid range");
        assert_eq!(set1.intervals(), &[(1, 3)]);
        assert_eq!(set1.pages_within(5), vec![1, 2, 3]);

        let set2 = parse_page_range("２，５").expect("valid range");
        assert_eq!(set2.intervals(), &[(2, 2), (5, 5)]);
        assert_eq!(set2.pages_within(5), vec![2, 5]);

        // Fullwidth space (U+3000)
        let set3 = parse_page_range("\u{3000}1\u{3000}-\u{3000}3\u{3000},\u{3000}5\u{3000}")
            .expect("valid range");
        assert_eq!(set3.intervals(), &[(1, 3), (5, 5)]);
        assert_eq!(set3.pages_within(5), vec![1, 2, 3, 5]);
    }

    #[test]
    fn parses_fullwidth_separators_and_dashes() {
        // Fullwidth comma and Japanese comma
        let set1 = parse_page_range("1，3、5").expect("valid range");
        assert_eq!(set1.intervals(), &[(1, 1), (3, 3), (5, 5)]);

        // Fullwidth wave dash (〜 U+301C) and fullwidth hyphen (－ U+FF0D)
        let set2 = parse_page_range("1〜3、5－7").expect("valid range");
        assert_eq!(set2.intervals(), &[(1, 3), (5, 7)]);

        // Fullwidth tilde (～ U+FF5E)
        let set3 = parse_page_range("2～4").expect("valid range");
        assert_eq!(set3.intervals(), &[(2, 4)]);
    }

    #[test]
    fn normalizes_overlap_and_adjacent() {
        // "5, 1-3, 2" -> [(1, 3), (5, 5)]
        let set = parse_page_range("5, 1-3, 2").expect("valid range");
        assert_eq!(set.intervals(), &[(1, 3), (5, 5)]);
        assert_eq!(set.pages_within(10), vec![1, 2, 3, 5]);
        assert_eq!(set.count_within(10), 4);

        let set_overlap = parse_page_range("4-6, 1-3, 2-5, 3").expect("valid range");
        assert_eq!(set_overlap.intervals(), &[(1, 6)]);

        let set_adjacent = parse_page_range("1-3, 4-5").expect("valid range");
        assert_eq!(set_adjacent.intervals(), &[(1, 5)]);

        let set_duplicates = parse_page_range("3, 3, 3").expect("valid range");
        assert_eq!(set_duplicates.intervals(), &[(3, 3)]);
    }

    #[test]
    fn parses_huge_range_instantly() {
        let set = parse_page_range("1-4000000000").expect("valid range");
        assert_eq!(set.intervals(), &[(1, 4000000000)]);
        assert_eq!(set.pages_within(3), vec![1, 2, 3]);
        assert_eq!(set.count_within(3), 3);
        assert_eq!(set.count_within(0), 0);
        assert_eq!(set.pages_within(0), Vec::<u32>::new());
    }

    #[test]
    fn rejects_plus_sign() {
        let err1 = parse_page_range("+5").unwrap_err();
        assert_eq!(err1.detail, "+5");

        let err2 = parse_page_range("+1-3").unwrap_err();
        assert_eq!(err2.detail, "+1-3");

        let err3 = parse_page_range("1-+3").unwrap_err();
        assert_eq!(err3.detail, "1-+3");

        let err4 = parse_page_range("1, +5").unwrap_err();
        assert_eq!(err4.detail, "+5");
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

        let err5 = parse_page_range("０").unwrap_err();
        assert_eq!(err5.detail, "０");
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
        assert_eq!(set.count_within(6), 5);
        assert_eq!(set.pages_within(8), vec![1, 2, 3, 4, 5, 8]);
        assert_eq!(set.count_within(8), 6);
        assert_eq!(set.pages_within(20), vec![1, 2, 3, 4, 5, 8, 12]);
        assert_eq!(set.count_within(20), 7);
    }

    #[test]
    fn end_alone_selects_each_pdfs_last_page() {
        let set = parse_page_range("end").expect("valid range");
        assert_eq!(set.intervals(), &[] as &[(u32, u32)]);
        assert!(set.last_page());
        assert!(!set.is_empty());
        assert_eq!(set.pages_within(1), vec![1]);
        assert_eq!(set.pages_within(7), vec![7]);
        assert_eq!(set.count_within(7), 1);
        assert_eq!(set.pages_within(0), Vec::<u32>::new());
        assert_eq!(set.count_within(0), 0);

        let both_sides = parse_page_range(" end - end ").expect("valid range");
        assert_eq!(both_sides, set);
    }

    #[test]
    fn end_after_a_number_runs_to_each_pdfs_last_page() {
        let set = parse_page_range("5-end").expect("valid range");
        assert_eq!(set.intervals(), &[(5, u32::MAX)]);
        assert!(!set.last_page());
        assert_eq!(set.pages_within(7), vec![5, 6, 7]);
        assert_eq!(set.count_within(7), 3);
        assert_eq!(set.pages_within(5), vec![5]);
        assert_eq!(set.pages_within(4), Vec::<u32>::new());
        assert_eq!(set.count_within(4), 0);

        let fullwidth_dash = parse_page_range("５〜end").expect("valid range");
        assert_eq!(fullwidth_dash, set);
    }

    #[test]
    fn end_alone_adds_the_last_page_once() {
        let set = parse_page_range("1, end").expect("valid range");
        assert_eq!(set.intervals(), &[(1, 1)]);
        assert!(set.last_page());
        assert_eq!(set.pages_within(4), vec![1, 4]);
        assert_eq!(set.count_within(4), 2);
        // In a one-page PDF, page 1 is the last page.
        assert_eq!(set.pages_within(1), vec![1]);
        assert_eq!(set.count_within(1), 1);

        let covered = parse_page_range("end, 2-3").expect("valid range");
        assert_eq!(covered.pages_within(3), vec![2, 3]);
        assert_eq!(covered.count_within(3), 2);
        assert_eq!(covered.pages_within(5), vec![2, 3, 5]);
        assert_eq!(covered.count_within(5), 3);

        let open_end = parse_page_range("end, 3-end").expect("valid range");
        assert_eq!(open_end.pages_within(4), vec![3, 4]);
        assert_eq!(open_end.count_within(4), 2);
        assert_eq!(open_end.pages_within(2), vec![2]);
        assert_eq!(open_end.count_within(2), 1);
    }

    #[test]
    fn rejects_end_before_a_number_and_other_spellings() {
        for text in [
            "end-3",
            "end-1",
            "END",
            "End",
            "ｅｎｄ",
            "end-",
            "-end",
            "1-end-2",
            "en",
            "ends",
        ] {
            let err = parse_page_range(text).unwrap_err();
            assert_eq!(err.detail, text, "{text}");
        }
        let err = parse_page_range("1, END").unwrap_err();
        assert_eq!(err.detail, "END");
    }

    #[test]
    fn empty_set_has_no_last_page() {
        assert!(PageSet::default().is_empty());
        assert!(!parse_page_range("2").expect("valid range").is_empty());
    }

    #[test]
    fn returns_empty_when_no_pages_remain() {
        let set = parse_page_range("5-10").expect("valid range");
        assert_eq!(set.pages_within(4), Vec::<u32>::new());
        assert_eq!(set.count_within(4), 0);
        assert_eq!(set.pages_within(0), Vec::<u32>::new());
        assert_eq!(set.count_within(0), 0);
    }
}
