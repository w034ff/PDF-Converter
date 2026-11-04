//! Character error rate as the survey scored it: both sides NFKC, spaces
//! and line breaks removed, `·` (U+00B7) → `・` (U+30FB), `〜` and `~` the same.

use unicode_normalization::UnicodeNormalization;

pub fn normalize(s: &str) -> Vec<char> {
    s.nfkc()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '\u{00B7}' => '\u{30FB}',
            '\u{301C}' | '\u{FF5E}' => '~',
            c => c,
        })
        .collect()
}

pub fn levenshtein(a: &[char], b: &[char]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// A piece of text with its axis-aligned box `[x0, y0, x1, y1]`.
#[derive(Clone, Debug)]
pub struct Boxed {
    pub text: String,
    pub bbox: [f64; 4],
}

/// Share of a predicted line's area that must lie inside a ground-truth box
/// for the line to be assigned to it.
pub const ASSIGN_MIN_COVER: f64 = 0.5;

#[derive(Default, Clone, Copy, Debug)]
pub struct Score {
    pub edits: usize,
    pub chars: usize,
    /// Predicted lines not assigned to any ground-truth box (counted as insertions).
    pub unassigned: usize,
}

impl Score {
    pub fn add(&mut self, o: Score) {
        self.edits += o.edits;
        self.chars += o.chars;
        self.unassigned += o.unassigned;
    }
    pub fn cer(&self) -> f64 {
        self.edits as f64 / self.chars.max(1) as f64
    }
}

fn inter(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0)
}

/// 対応付け CER: every predicted line goes to the ground-truth box that holds
/// most of it (at least [`ASSIGN_MIN_COVER`] of its area); the lines of one box
/// are joined top to bottom, left to right and compared with its text. Lines
/// that fit nowhere count as insertions.
pub fn matched(gt: &[Boxed], pred: &[Boxed]) -> Score {
    let mut assigned: Vec<Vec<usize>> = vec![Vec::new(); gt.len()];
    let mut score = Score::default();
    for (j, p) in pred.iter().enumerate() {
        let area = ((p.bbox[2] - p.bbox[0]) * (p.bbox[3] - p.bbox[1])).max(1e-9);
        let best = gt
            .iter()
            .enumerate()
            .map(|(i, g)| (i, inter(&g.bbox, &p.bbox) / area))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, cover)) if cover >= ASSIGN_MIN_COVER => assigned[i].push(j),
            _ => {
                score.edits += normalize(&p.text).len();
                score.unassigned += 1;
            }
        }
    }
    for (i, g) in gt.iter().enumerate() {
        let mut idx = assigned[i].clone();
        idx.sort_by(|&a, &b| {
            let (pa, pb) = (&pred[a].bbox, &pred[b].bbox);
            // Same row when the vertical centres are within half a line height.
            let h = ((pa[3] - pa[1]).min(pb[3] - pb[1])) / 2.0;
            let (ca, cb) = ((pa[1] + pa[3]) / 2.0, (pb[1] + pb[3]) / 2.0);
            if (ca - cb).abs() < h { pa[0].total_cmp(&pb[0]) } else { ca.total_cmp(&cb) }
        });
        let text: String = idx.iter().map(|&j| pred[j].text.as_str()).collect();
        let (g, p) = (normalize(&g.text), normalize(&text));
        score.edits += levenshtein(&g, &p);
        score.chars += g.len();
    }
    score
}

/// Like [`matched`], but each predicted *character* goes to the ground-truth
/// box that contains its centre (its cell from the CTC steps, item 6), so a
/// line that runs across two boxes (a key and its value read as one line) is
/// split between them instead of being counted wrong twice. `pred` is in
/// reading order; characters outside every box count as insertions.
pub fn matched_chars(gt: &[Boxed], pred: &[Boxed]) -> Score {
    let mut per_box: Vec<String> = vec![String::new(); gt.len()];
    let mut score = Score::default();
    for p in pred {
        let (cx, cy) = ((p.bbox[0] + p.bbox[2]) / 2.0, (p.bbox[1] + p.bbox[3]) / 2.0);
        let hit = gt
            .iter()
            .enumerate()
            .filter(|(_, g)| cx >= g.bbox[0] && cx <= g.bbox[2] && cy >= g.bbox[1] && cy <= g.bbox[3])
            .min_by(|a, b| {
                let area = |g: &Boxed| (g.bbox[2] - g.bbox[0]) * (g.bbox[3] - g.bbox[1]);
                area(a.1).total_cmp(&area(b.1))
            });
        match hit {
            Some((i, _)) => per_box[i].push_str(&p.text),
            None => {
                score.edits += normalize(&p.text).len();
                score.unassigned += 1;
            }
        }
    }
    for (g, text) in gt.iter().zip(per_box) {
        let (g, p) = (normalize(&g.text), normalize(&text));
        score.edits += levenshtein(&g, &p);
        score.chars += g.len();
    }
    score
}

/// ページ全文 CER: the whole page's text in reading order on both sides.
pub fn full_text(gt: &str, pred: &str) -> Score {
    let (g, p) = (normalize(gt), normalize(pred));
    Score { edits: levenshtein(&g, &p), chars: g.len(), unassigned: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation() {
        assert_eq!(normalize("Ａ １·〜 x\n"), normalize("A1・~x"));
        assert_eq!(levenshtein(&normalize("kitten"), &normalize("sitting")), 3);
    }
}
