//! Vertical item 2: where do the characters PP-OCR drops on vertical lines
//! stand in the CTC output?
//!
//! For every detected vertical line paired with a ground-truth column (IoU
//! ≥ 0.3), the column's text and the greedy CTC output are aligned along the
//! edit path. For a ground-truth character that is missing (deleted), the
//! time steps between the neighbouring emitted characters are searched for
//! the true class: its best rank among all 18,710 classes, its highest
//! probability, what wins at that step (blank or another character), and the
//! log-space offset the true class would need to win there. Substituted
//! characters are searched over the steps of the character that replaced
//! them (±1 step).

use crate::charclass::{Kind, KINDS, Op, align};
use crate::engine::Ocr;
use crate::geom;
use crate::sets::EvalPage;

#[derive(Clone, Debug)]
pub struct Probe {
    pub kind: Kind,
    pub truth: char,
    pub deleted: bool,
    pub best_rank: usize,
    pub best_p: f32,
    /// At the step where the true class is most likely: the winner and its probability.
    pub winner_blank: bool,
    pub winner_p: f32,
    /// ln(winner) − ln(true class) at that step: the offset the true class needs.
    pub needed: f32,
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

fn quantile(mut v: Vec<f32>, q: f64) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f64 * q).round() as usize]
}

/// Searches steps `a..=b` of `probs` for class `k`.
fn probe(probs: &[f32], classes: usize, a: usize, b: usize, k: usize) -> (usize, f32, bool, f32, f32) {
    let mut best: Option<(usize, f32, usize)> = None; // (rank, p, step)
    for step in a..=b {
        let row = &probs[step * classes..(step + 1) * classes];
        let p = row[k];
        let rank = 1 + row.iter().filter(|&&q| q > p).count();
        if best.is_none_or(|b| p > b.1) {
            best = Some((rank, p, step));
        }
    }
    let (_, p, step) = best.unwrap();
    let min_rank = (a..=b)
        .map(|s| {
            let row = &probs[s * classes..(s + 1) * classes];
            1 + row.iter().filter(|&&q| q > row[k]).count()
        })
        .min()
        .unwrap();
    let row = &probs[step * classes..(step + 1) * classes];
    let (wi, wp) = row.iter().enumerate().fold((0, f32::MIN), |acc, (i, &q)| if q > acc.1 { (i, q) } else { acc });
    (min_rank, p, wi == 0, wp, wp.max(1e-30).ln() - p.max(1e-30).ln())
}

pub fn study(ocr: &mut Ocr, pages: &[EvalPage]) -> Vec<Probe> {
    ocr.keep_probs = true;
    let index: std::collections::HashMap<char, usize> = ocr
        .dict
        .iter()
        .enumerate()
        .filter(|(_, s)| s.chars().count() == 1)
        .map(|(i, s)| (s.chars().next().unwrap(), i))
        .collect();
    let mut out = Vec::new();
    for page in pages {
        let lines = ocr.run(&page.image, &mut crate::engine::Timings::default()).expect("ocr");
        for l in lines.iter().filter(|l| l.vertical) {
            let Some(probs) = &l.rec.probs else { continue };
            let lb = geom::quad_bounds(&l.quad);
            let Some(g) = page
                .char_lines
                .iter()
                .filter(|g| g.vertical)
                .max_by(|a, b| geom::iou(&a.bbox, &lb).total_cmp(&geom::iou(&b.bbox, &lb)))
            else {
                continue;
            };
            if geom::iou(&g.bbox, &lb) < 0.3 {
                continue;
            }
            let cols = &l.rec.char_cols;
            let pred: Vec<char> = cols.iter().map(|c| c.0).collect();
            let ops = align(&g.chars, &pred);
            let t = l.rec.time_steps;
            // The prediction index of the next op that has one, for each op.
            let mut next_j = vec![None; ops.len() + 1];
            for (n, op) in ops.iter().enumerate().rev() {
                next_j[n] = match op {
                    Op::Match(_, j) | Op::Sub(_, j) | Op::Ins(j) => Some(*j),
                    Op::Del(_) => next_j[n + 1],
                };
            }
            let mut last_j: Option<usize> = None;
            for (n, op) in ops.iter().enumerate() {
                match *op {
                    Op::Del(i) => {
                        let Some(&k) = index.get(&g.chars[i]) else { continue };
                        let a = last_j.map_or(0, |j| cols[j].2 + 1);
                        let b = next_j[n + 1].map_or(t - 1, |j| cols[j].1.saturating_sub(1));
                        let (a, b) = if a <= b { (a, b) } else { (last_j.map_or(0, |j| cols[j].2), next_j[n + 1].map_or(t - 1, |j| cols[j].1)) };
                        let (r, p, wb, wp, need) = probe(&probs.data, probs.classes, a.min(t - 1), b.min(t - 1), k);
                        out.push(Probe { kind: Kind::of(g.chars[i]), truth: g.chars[i], deleted: true, best_rank: r, best_p: p, winner_blank: wb, winner_p: wp, needed: need });
                    }
                    Op::Sub(i, j) => {
                        last_j = Some(j);
                        let Some(&k) = index.get(&g.chars[i]) else { continue };
                        let (a, b) = (cols[j].1.saturating_sub(1), (cols[j].2 + 1).min(t - 1));
                        let (r, p, wb, wp, need) = probe(&probs.data, probs.classes, a, b, k);
                        out.push(Probe { kind: Kind::of(g.chars[i]), truth: g.chars[i], deleted: false, best_rank: r, best_p: p, winner_blank: wb, winner_p: wp, needed: need });
                    }
                    Op::Match(_, j) | Op::Ins(j) => last_j = Some(j),
                }
            }
        }
    }
    ocr.keep_probs = false;
    out
}

/// Prints the probes grouped by kind, deletions and substitutions apart.
pub fn report(probes: &[Probe]) {
    for deleted in [true, false] {
        println!("{}", if deleted { "DELETED (searched between the neighbouring emitted characters)" } else { "SUBSTITUTED (searched over the replacing character's steps ±1)" });
        println!("kind | n | rank 1 | ≤5 | ≤20 | >20 | median best rank | median best p | winner is blank | median needed ln-offset (q25, q75)");
        for k in KINDS {
            let v: Vec<&Probe> = probes.iter().filter(|p| p.kind == k && p.deleted == deleted).collect();
            if v.is_empty() {
                continue;
            }
            let c = |f: &dyn Fn(&Probe) -> bool| v.iter().filter(|p| f(p)).count();
            let ranks: Vec<f32> = v.iter().map(|p| p.best_rank as f32).collect();
            let need: Vec<f32> = v.iter().map(|p| p.needed).collect();
            println!(
                "{} | {} | {} | {} | {} | {} | {} | {:.2e} | {}/{} | {:.1} ({:.1}, {:.1})",
                k.label(),
                v.len(),
                c(&|p| p.best_rank == 1),
                c(&|p| p.best_rank <= 5),
                c(&|p| p.best_rank <= 20),
                c(&|p| p.best_rank > 20),
                median(ranks),
                median(v.iter().map(|p| p.best_p).collect()),
                c(&|p| p.winner_blank),
                v.len(),
                median(need.clone()),
                quantile(need.clone(), 0.25),
                quantile(need, 0.75),
            );
        }
    }
    println!("each probe: kind truth del rank p winner_blank winner_p needed");
    for p in probes {
        println!(
            "  {} {} {} {} {:.2e} {} {:.3} {:.1}",
            p.kind.label(),
            p.truth,
            if p.deleted { "del" } else { "sub" },
            p.best_rank,
            p.best_p,
            p.winner_blank,
            p.winner_p,
            p.needed
        );
    }
}

/// Vertical item 2: grid of [`crate::engine::CtcBias`] settings on a page
/// set. The OCR runs once with the softmax kept; each setting re-decodes the
/// vertical lines only (the reading order depends on the boxes alone).
pub fn grid(ocr: &mut Ocr, pages: &[EvalPage], settings: &[(f32, f32, f32, f32)]) {
    ocr.keep_probs = true;
    let runs: Vec<Vec<crate::engine::Line>> =
        pages.iter().map(|p| ocr.run(&p.image, &mut crate::engine::Timings::default()).expect("ocr")).collect();
    ocr.keep_probs = false;
    println!("blank_penalty punct choon small | CER | kinds");
    for &(blank, punct, choon, small) in settings {
        let bias = crate::engine::CtcBias::for_kinds(&ocr.dict, blank, punct, choon, small);
        let (mut total, mut kinds) = (crate::cer::Score::default(), crate::charclass::KindStats::default());
        for (page, lines) in pages.iter().zip(&runs) {
            let mut lines = lines.clone();
            for l in lines.iter_mut().filter(|l| l.vertical) {
                let Some(p) = l.rec.probs.clone() else { continue };
                let steps = p.data.len() / p.classes;
                l.rec = crate::engine::ctc_decode_biased(&p.data, steps, p.classes, &ocr.dict, &bias);
                l.text = l.rec.text.clone();
            }
            let text = crate::veval::reading_text(&lines);
            total.add(crate::cer::full_text(&page.gt_text, &text));
            kinds.add_texts(&page.gt_text, &text);
        }
        println!("{blank} {punct} {choon} {small} | {:.2} % ({}) | {}", total.cer() * 100.0, total.edits, kinds.summary());
        if std::env::var("GRID_CONFUSIONS").is_ok() {
            println!("    {}", kinds.top_confusions(20));
        }
    }
}
