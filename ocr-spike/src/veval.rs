//! Vertical items 1–5: scores a page set with the current engine settings.
//!
//! Per page: page full-text CER after reordering (`order::reading_order`,
//! the 9.84 % of item 5), the per-kind breakdown (`charclass`), and on the
//! generated pages, how often each predicted character's cell centre lies in
//! the true character's em box (characters paired along the edit path of
//! each detected vertical line against the ground-truth column it overlaps).

use crate::cer::{self, Score};
use crate::charclass::{Kind, KindStats, Op, align};
use crate::engine::{Line, Ocr, Timings};
use crate::geom;
use crate::order;
use crate::sets::EvalPage;
use crate::types;

#[derive(Default, Clone, Debug)]
pub struct CellScore {
    pub chars: usize,
    pub hits: usize,
    /// 1-D IoU along the column, summed.
    pub iou: f64,
    /// Hits by kind (index = `Kind as usize`) and totals.
    pub by_kind: [(usize, usize); 7],
}

impl CellScore {
    pub fn add(&mut self, o: &CellScore) {
        self.chars += o.chars;
        self.hits += o.hits;
        self.iou += o.iou;
        for (a, b) in self.by_kind.iter_mut().zip(&o.by_kind) {
            a.0 += b.0;
            a.1 += b.1;
        }
    }
    pub fn rate(&self) -> f64 {
        self.hits as f64 * 100.0 / self.chars.max(1) as f64
    }
}

#[derive(Default)]
pub struct PageResult {
    pub score: Score,
    pub kinds: KindStats,
    /// Character-matched (order-free between lines): each predicted
    /// character goes to the ground-truth line box holding its cell centre.
    pub cm: Score,
    pub cm_kinds: KindStats,
    pub cells: CellScore,
    pub vertical_lines: usize,
    pub timings: Timings,
}

pub fn reading_text(lines: &[Line]) -> String {
    let items: Vec<order::Item> = lines.iter().map(|l| order::Item { bbox: geom::quad_bounds(&l.quad) }).collect();
    order::reading_order(&items)
        .iter()
        .flat_map(|b| b.lines.iter().map(|&i| lines[i].text.as_str()))
        .collect()
}

/// Per-character positions of the detected vertical lines against the em
/// boxes of the ground truth.
pub fn cell_score(lines: &[Line], page: &EvalPage) -> CellScore {
    let mut s = CellScore::default();
    for l in lines.iter().filter(|l| l.vertical) {
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
        let (cells, glyphs) = types::line_chars(l);
        let pred: Vec<char> = glyphs.iter().map(|c| c.0).collect();
        for op in align(&g.chars, &pred) {
            let Op::Match(i, j) = op else { continue };
            let truth = g.boxes[i];
            let (u0, u1) = cells[j];
            let cb = geom::quad_bounds(&types::span_quad(&l.quad, true, u0, u1));
            let (cx, cy) = ((cb[0] + cb[2]) / 2.0, (cb[1] + cb[3]) / 2.0);
            let hit = cx >= truth[0] && cx <= truth[2] && cy >= truth[1] && cy <= truth[3];
            let inter = (cb[3].min(truth[3]) - cb[1].max(truth[1])).max(0.0);
            let uni = (cb[3].max(truth[3]) - cb[1].min(truth[1])).max(1e-9);
            s.chars += 1;
            s.hits += usize::from(hit);
            s.iou += inter / uni;
            let k = &mut s.by_kind[Kind::of(g.chars[i]) as usize];
            k.0 += usize::from(hit);
            k.1 += 1;
        }
    }
    s
}

/// Each predicted character (with its cell from [`types::line_chars`]) goes
/// to the smallest ground-truth line box that holds its centre; the boxes'
/// texts are compared one by one, characters outside every box are inserted.
pub fn char_matched(lines: &[Line], page: &EvalPage) -> (Score, KindStats) {
    let mut per_box = vec![String::new(); page.gt_lines.len()];
    let (mut score, mut kinds) = (Score::default(), KindStats::default());
    for l in lines {
        let (cells, glyphs) = types::line_chars(l);
        for (g, &(u0, u1)) in glyphs.iter().zip(&cells) {
            let b = geom::quad_bounds(&types::span_quad(&l.quad, l.vertical, u0, u1));
            let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            let hit = page
                .gt_lines
                .iter()
                .enumerate()
                .filter(|(_, t)| cx >= t.bbox[0] && cx <= t.bbox[2] && cy >= t.bbox[1] && cy <= t.bbox[3])
                .min_by(|a, b| ((a.1.bbox[2] - a.1.bbox[0]) * (a.1.bbox[3] - a.1.bbox[1])).total_cmp(&((b.1.bbox[2] - b.1.bbox[0]) * (b.1.bbox[3] - b.1.bbox[1]))));
            match hit {
                Some((i, _)) => per_box[i].push(g.0),
                None => {
                    let s = g.0.to_string();
                    score.edits += cer::normalize(&s).len();
                    kinds.add_texts("", &s);
                }
            }
        }
    }
    for (t, p) in page.gt_lines.iter().zip(&per_box) {
        let s = cer::full_text(&t.text, p);
        score.edits += s.edits;
        score.chars += s.chars;
        kinds.add_texts(&t.text, p);
    }
    (score, kinds)
}

pub fn score_page(ocr: &mut Ocr, page: &EvalPage) -> (PageResult, Vec<Line>, String) {
    let mut t = Timings::default();
    let lines = ocr.run(&page.image, &mut t).expect("ocr");
    let text = reading_text(&lines);
    let mut kinds = KindStats::default();
    kinds.add_texts(&page.gt_text, &text);
    let (cm, cm_kinds) = char_matched(&lines, page);
    let r = PageResult {
        score: cer::full_text(&page.gt_text, &text),
        kinds,
        cm,
        cm_kinds,
        cells: if page.char_lines.is_empty() { CellScore::default() } else { cell_score(&lines, page) },
        vertical_lines: lines.iter().filter(|l| l.vertical).count(),
        timings: t,
    };
    (r, lines, text)
}

/// Scores every page and prints one line per page and a total.
pub fn run_set(ocr: &mut Ocr, pages: &[EvalPage], label: &str, show: bool) -> (Score, KindStats, CellScore) {
    let (mut total, mut kinds, mut cells) = (Score::default(), KindStats::default(), CellScore::default());
    let (mut cm, mut cm_kinds) = (Score::default(), KindStats::default());
    for p in pages {
        let (r, lines, text) = score_page(ocr, p);
        println!(
            "{}: CER {:.2} % ({} / {}), char-matched {:.2} %, vertical lines {}, cells {:.1} % of {}",
            p.name,
            r.score.cer() * 100.0,
            r.score.edits,
            r.score.chars,
            r.cm.cer() * 100.0,
            r.vertical_lines,
            r.cells.rate(),
            r.cells.chars
        );
        if show {
            println!("  truth: {}", p.gt_text);
            println!("  ours:  {text}");
            for l in lines.iter().filter(|l| l.vertical) {
                println!("  V {:?} | base {:?}", l.text, l.rec.text);
            }
        }
        total.add(r.score);
        kinds.merge(&r.kinds);
        cm.add(r.cm);
        cm_kinds.merge(&r.cm_kinds);
        cells.add(&r.cells);
    }
    println!("TOTAL {label}: CER {:.2} % ({} / {})", total.cer() * 100.0, total.edits, total.chars);
    println!("  kinds: {}", kinds.summary());
    println!("  confusions: {}", kinds.top_confusions(12));
    println!("TOTAL {label} char-matched: CER {:.2} % ({} / {})", cm.cer() * 100.0, cm.edits, cm.chars);
    println!("  char-matched kinds: {}", cm_kinds.summary());
    if cells.chars > 0 {
        let by: Vec<String> = crate::charclass::KINDS
            .iter()
            .map(|k| {
                let (h, n) = cells.by_kind[*k as usize];
                format!("{} {:.1}%/{}", k.label(), h as f64 * 100.0 / n.max(1) as f64, n)
            })
            .collect();
        println!(
            "  cells: centre inside the true em box {:.1} % of {} paired characters, mean 1-D IoU {:.3} [{}]",
            cells.rate(),
            cells.chars,
            cells.iou / cells.chars as f64,
            by.join(" ")
        );
    }
    (total, kinds, cells)
}
