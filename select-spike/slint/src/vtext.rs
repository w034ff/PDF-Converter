//! Lays out Japanese vertical text (縦書き) by hand, since Slint's `Text` has
//! no writing mode: one glyph per cell, columns from right to left, the
//! punctuation that changes shape in vertical text swapped for its vertical
//! presentation form, and Latin letters and the long-vowel mark rotated.

#[derive(Debug, Clone, PartialEq)]
pub struct VGlyph {
    pub ch: String,
    /// Top-left of the glyph cell, in units of the font size.
    pub col: usize,
    pub row: usize,
    /// Rotate the glyph 90° clockwise.
    pub rotate: bool,
    /// Push the glyph to the top-right of its cell (small kana, 、。).
    pub shift: bool,
}

/// The vertical presentation form of `c`, if Unicode has one (U+FE10–FE19, U+FE30–FE4F).
fn vertical_form(c: char) -> Option<char> {
    Some(match c {
        '、' => '\u{FE11}',
        '。' => '\u{FE12}',
        '，' => '\u{FE10}',
        '：' => '\u{FE13}',
        '；' => '\u{FE14}',
        '！' => '\u{FE15}',
        '？' => '\u{FE16}',
        '…' => '\u{FE19}',
        '「' => '\u{FE41}',
        '」' => '\u{FE42}',
        '『' => '\u{FE43}',
        '』' => '\u{FE44}',
        '（' => '\u{FE35}',
        '）' => '\u{FE36}',
        '【' => '\u{FE3B}',
        '】' => '\u{FE3C}',
        '〔' => '\u{FE39}',
        '〕' => '\u{FE3A}',
        _ => return None,
    })
}

fn is_small_kana(c: char) -> bool {
    "ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ".contains(c)
}

/// Characters that must not start a column (行頭禁則, the basic set).
fn no_column_start(c: char) -> bool {
    "、。，．・：；？！ー）」』】〕ぁぃぅぇぉっゃゅょァィゥェォッャュョ".contains(c)
}

/// Lays `text` out in columns of at most `rows` glyphs. A newline starts a
/// new column. A character that must not start a column hangs at the end
/// of the previous one (ぶら下げ), so a column can hold `rows + 1` glyphs.
pub fn layout(text: &str, rows: usize) -> Vec<VGlyph> {
    let rows = rows.max(1);
    let mut out = Vec::new();
    let (mut col, mut row) = (0usize, 0usize);
    for c in text.chars() {
        if c == '\n' {
            col += 1;
            row = 0;
            continue;
        }
        if row >= rows && !(row == rows && no_column_start(c)) {
            col += 1;
            row = 0;
        }
        let (ch, rotate) = match vertical_form(c) {
            Some(v) => (v, false),
            None => (c, c == 'ー' || c == '～' || c == '－' || c.is_ascii_graphic()),
        };
        out.push(VGlyph { ch: ch.to_string(), col, row, rotate, shift: is_small_kana(c) });
        row += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_into_columns_and_swaps_punctuation() {
        let g = layout("あいう、えお", 3);
        let cells: Vec<_> = g.iter().map(|g| (g.ch.as_str(), g.col, g.row)).collect();
        // 、 hangs at the end of the first column instead of starting the second.
        assert_eq!(cells, vec![("あ", 0, 0), ("い", 0, 1), ("う", 0, 2), ("\u{FE11}", 0, 3), ("え", 1, 0), ("お", 1, 1)]);
    }

    #[test]
    fn rotates_long_vowel_and_latin() {
        let g = layout("データPDF", 10);
        assert!(g[1].rotate);
        assert!(g[3].rotate && g[4].rotate && g[5].rotate);
        assert!(!g[0].rotate);
    }

    #[test]
    fn newline_starts_a_column() {
        let g = layout("あ\nい", 10);
        assert_eq!((g[1].col, g[1].row), (1, 0));
    }
}
