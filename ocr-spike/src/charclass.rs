//! Per-character scoring by kind (vertical item 1): ground truth and
//! prediction are paired along the edit path, and every ground-truth
//! character is counted as correct, substituted or deleted under its kind;
//! inserted characters are counted under the kind of what was inserted.
//!
//! Both sides go through [`crate::cer::normalize`] first, the same text the
//! CER is computed on, so the edits add up to the CER.

use crate::cer::normalize;

pub const SMALL_KANA: &str = "ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮ";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Comma,
    Period,
    Open,
    Close,
    Choon,
    Small,
    Other,
}

pub const KINDS: [Kind; 7] = [Kind::Comma, Kind::Period, Kind::Open, Kind::Close, Kind::Choon, Kind::Small, Kind::Other];

impl Kind {
    pub fn of(c: char) -> Self {
        match c {
            '、' => Self::Comma,
            '。' => Self::Period,
            '「' => Self::Open,
            '」' => Self::Close,
            'ー' => Self::Choon,
            c if SMALL_KANA.contains(c) => Self::Small,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Comma => "、",
            Self::Period => "。",
            Self::Open => "「",
            Self::Close => "」",
            Self::Choon => "ー",
            Self::Small => "小さいかな",
            Self::Other => "その他",
        }
    }
}

/// One step of the edit path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Ground truth `i` and prediction `j`, equal.
    Match(usize, usize),
    /// Ground truth `i` read as prediction `j`.
    Sub(usize, usize),
    /// Ground truth `i` missing from the prediction.
    Del(usize),
    /// Prediction `j` not in the ground truth.
    Ins(usize),
}

/// The edit path of a minimum Levenshtein alignment (`a` ground truth, `b`
/// prediction). Ties prefer a match or substitution, then a deletion.
pub fn align(a: &[char], b: &[char]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut d = vec![0u32; (n + 1) * w];
    for j in 0..=m {
        d[j] = j as u32;
    }
    for i in 1..=n {
        d[i * w] = i as u32;
        for j in 1..=m {
            let sub = d[(i - 1) * w + j - 1] + u32::from(a[i - 1] != b[j - 1]);
            let del = d[(i - 1) * w + j] + 1;
            let ins = d[i * w + j - 1] + 1;
            d[i * w + j] = sub.min(del).min(ins);
        }
    }
    let mut ops = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        let here = d[i * w + j];
        if i > 0 && j > 0 && here == d[(i - 1) * w + j - 1] + u32::from(a[i - 1] != b[j - 1]) {
            ops.push(if a[i - 1] == b[j - 1] { Op::Match(i - 1, j - 1) } else { Op::Sub(i - 1, j - 1) });
            i -= 1;
            j -= 1;
        } else if i > 0 && here == d[(i - 1) * w + j] + 1 {
            ops.push(Op::Del(i - 1));
            i -= 1;
        } else {
            ops.push(Op::Ins(j - 1));
            j -= 1;
        }
    }
    ops.reverse();
    ops
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KindCount {
    pub truth: usize,
    pub correct: usize,
    pub substituted: usize,
    pub deleted: usize,
    /// Predicted characters of this kind that are not in the ground truth.
    pub inserted: usize,
}

impl KindCount {
    pub fn recall(&self) -> f64 {
        self.correct as f64 / self.truth.max(1) as f64
    }
    fn add(&mut self, o: &KindCount) {
        self.truth += o.truth;
        self.correct += o.correct;
        self.substituted += o.substituted;
        self.deleted += o.deleted;
        self.inserted += o.inserted;
    }
}

#[derive(Clone, Debug, Default)]
pub struct KindStats {
    pub by: [KindCount; 7],
    /// (truth, predicted) of substitutions, for the most common confusions.
    pub confusions: std::collections::BTreeMap<(char, char), usize>,
}

impl KindStats {
    pub fn add_texts(&mut self, gt: &str, pred: &str) {
        let (a, b) = (normalize(gt), normalize(pred));
        for op in align(&a, &b) {
            match op {
                Op::Match(i, _) => {
                    let k = &mut self.by[Kind::of(a[i]) as usize];
                    k.truth += 1;
                    k.correct += 1;
                }
                Op::Sub(i, j) => {
                    let k = &mut self.by[Kind::of(a[i]) as usize];
                    k.truth += 1;
                    k.substituted += 1;
                    *self.confusions.entry((a[i], b[j])).or_default() += 1;
                }
                Op::Del(i) => {
                    let k = &mut self.by[Kind::of(a[i]) as usize];
                    k.truth += 1;
                    k.deleted += 1;
                }
                Op::Ins(j) => self.by[Kind::of(b[j]) as usize].inserted += 1,
            }
        }
    }

    pub fn merge(&mut self, o: &KindStats) {
        for (a, b) in self.by.iter_mut().zip(&o.by) {
            a.add(b);
        }
        for (k, v) in &o.confusions {
            *self.confusions.entry(*k).or_default() += v;
        }
    }

    /// One line: recall per kind with (correct / truth), then deletions,
    /// substitutions and insertions per kind.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        for k in KINDS {
            let c = &self.by[k as usize];
            s.push_str(&format!(
                "{} {:.1}% ({}/{}, del {} sub {} ins {})  ",
                k.label(),
                c.recall() * 100.0,
                c.correct,
                c.truth,
                c.deleted,
                c.substituted,
                c.inserted
            ));
        }
        s.trim_end().to_string()
    }

    /// The `n` most frequent substitutions whose truth is not of kind Other,
    /// then the most frequent overall.
    pub fn top_confusions(&self, n: usize) -> String {
        let mut v: Vec<(&(char, char), &usize)> = self.confusions.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let special: Vec<String> =
            v.iter().filter(|(k, _)| Kind::of(k.0) != Kind::Other).take(n).map(|(k, c)| format!("{}→{}×{}", k.0, k.1, c)).collect();
        let all: Vec<String> = v.iter().take(n).map(|(k, c)| format!("{}→{}×{}", k.0, k.1, c)).collect();
        format!("special: {} | all: {}", special.join(" "), all.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_add_up_to_the_edit_distance() {
        let mut s = KindStats::default();
        s.add_texts("あった、「ニャー」。", "あた「ニヤ」x");
        let by = |k: Kind| s.by[k as usize];
        assert_eq!(by(Kind::Comma).deleted, 1);
        assert_eq!(by(Kind::Open).correct + by(Kind::Close).correct, 2);
        assert_eq!(by(Kind::Period).substituted, 1);
        let edits: usize = s.by.iter().map(|c| c.substituted + c.deleted + c.inserted).sum();
        let (a, b) = (normalize("あった、「ニャー」。"), normalize("あた「ニヤ」x"));
        assert_eq!(edits, crate::cer::levenshtein(&a, &b));
    }
}
