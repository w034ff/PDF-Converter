//! Item 2: reads the text layer back out of a written PDF and compares it
//! with the OCR result.
//!
//! - Full text: pdfium (`FPDFText_GetText` via `PdfPageText::all`) and
//!   `pdftotext` (poppler, default reading-order mode). Whitespace is dropped
//!   on both sides, then the text is aligned with the OCR text in reading
//!   order by edit distance; the score is `1 − edits / OCR characters`.
//! - Character boxes: pdfium's loose box (`FPDFText_GetLooseCharBox`, the box
//!   selection highlights are drawn from; the tight box follows the glyph's
//!   outline, which a glyphless font does not have). For each OCR character
//!   aligned to an equal extracted character, does the box centre fall in
//!   the character's quad, and how much do the box and the quad's bounding
//!   box overlap (IoU)?
//! - Search: pdfium's `FPDFText_FindStart` for each line's text and for every
//!   run of 4 characters along each line (stride 4); runs that contain
//!   punctuation, brackets, the long vowel mark or small kana are counted
//!   separately, since vertical forms are where viewers differ.

use std::path::Path;

use pdfium_render::prelude::*;
use serde::Serialize;

use crate::types::{OcrPage, QuadPt};

pub fn pdfium() -> Pdfium {
    let lib = std::env::var("PDFIUM_LIB")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../src-tauri/pdfium/linux/lib/libpdfium.so").into());
    Pdfium::new(Pdfium::bind_to_library(lib).expect("load pdfium (npm run pdfium:fetch)"))
}

/// One extracted character: its text and loose box in page points, top-left origin.
#[derive(Clone, Debug)]
pub struct PdfChar {
    pub ch: char,
    pub bbox: [f64; 4],
}

pub fn pdfium_chars(page: &PdfPage) -> (String, Vec<PdfChar>) {
    let h = page.height().value as f64;
    let text = page.text().unwrap();
    let all = text.all();
    let mut chars = Vec::new();
    for c in text.chars().iter() {
        if c.is_generated().unwrap_or(false) {
            continue;
        }
        let Some(ch) = c.unicode_char() else { continue };
        if ch.is_whitespace() {
            continue;
        }
        let b = c.loose_bounds().unwrap();
        chars.push(PdfChar {
            ch,
            bbox: [b.left().value as f64, h - b.top().value as f64, b.right().value as f64, h - b.bottom().value as f64],
        });
    }
    (all, chars)
}

fn strip(s: &str) -> Vec<char> {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Edit-distance alignment; returns the distance and, for each `a[i]`, the
/// index in `b` it was matched to when the characters are equal.
pub fn align(a: &[char], b: &[char]) -> (usize, Vec<Option<usize>>) {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in 0..=n {
        d[at(i, 0)] = i as u32;
    }
    for j in 0..=m {
        d[at(0, j)] = j as u32;
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = d[at(i - 1, j - 1)] + u32::from(a[i - 1] != b[j - 1]);
            d[at(i, j)] = sub.min(d[at(i - 1, j)] + 1).min(d[at(i, j - 1)] + 1);
        }
    }
    let mut map = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let here = d[at(i, j)];
        if a[i - 1] == b[j - 1] && here == d[at(i - 1, j - 1)] {
            map[i - 1] = Some(j - 1);
            i -= 1;
            j -= 1;
        } else if here == d[at(i - 1, j - 1)] + 1 {
            i -= 1;
            j -= 1;
        } else if here == d[at(i - 1, j)] + 1 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    (d[at(n, m)] as usize, map)
}

pub fn in_quad(q: &QuadPt, p: [f64; 2]) -> bool {
    // The quad is convex; the point is inside if it is on the same side of every edge.
    let mut sign = 0.0;
    for k in 0..4 {
        let (a, b) = (q[k], q[(k + 1) % 4]);
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if cross.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

fn quad_bbox(q: &QuadPt) -> [f64; 4] {
    let xs = q.iter().map(|p| p[0]);
    let ys = q.iter().map(|p| p[1]);
    [
        xs.clone().fold(f64::MAX, f64::min),
        ys.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
        ys.fold(f64::MIN, f64::max),
    ]
}

fn iou(a: [f64; 4], b: [f64; 4]) -> f64 {
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = ix * iy;
    let area = |r: [f64; 4]| (r[2] - r[0]).max(0.0) * (r[3] - r[1]).max(0.0);
    let union = area(a) + area(b) - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

pub fn is_special(c: char) -> bool {
    "、。，．「」『』（）()【】〔〕［］…‥ー―－〜・！？!?ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ".contains(c)
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Score {
    pub ocr_chars: usize,
    /// 1 − edit distance / OCR characters, pdfium's full text.
    pub text_pdfium: f64,
    /// The same with pdftotext.
    pub text_pdftotext: f64,
    /// OCR characters aligned to an equal character in pdfium's per-character list.
    pub aligned: usize,
    /// Of the aligned characters, those whose loose box centre is in the OCR quad.
    pub centre_in_quad: f64,
    /// Mean IoU of the loose box and the bounding box of the OCR quad.
    pub mean_iou: f64,
    pub queries: usize,
    pub found: f64,
    pub special_queries: usize,
    pub special_found: f64,
    /// Spaces in pdfium's text beyond the spaces of the OCR text (pdfium
    /// inserts one where the gap between two characters looks like a word gap).
    pub extra_spaces: i64,
    /// Vertical lines only (0 queries when the page has none).
    pub vertical_queries: usize,
    pub vertical_found: f64,
}

pub fn pdftotext(pdf: &Path, page: usize) -> String {
    pdftotext_pw(pdf, page, None)
}

pub fn pdftotext_pw(pdf: &Path, page: usize, password: Option<&str>) -> String {
    let mut cmd = std::process::Command::new("pdftotext");
    if let Some(pw) = password {
        cmd.args(["-upw", pw]);
    }
    let out = cmd
        .args(["-q", "-enc", "UTF-8", "-f", &(page + 1).to_string(), "-l", &(page + 1).to_string()])
        .arg(pdf)
        .arg("-")
        .output()
        .expect("pdftotext (poppler-utils)");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn score_page(pdfium: &Pdfium, pdf: &Path, page_index: usize, ocr: &OcrPage, password: Option<&str>) -> Score {
    let doc = pdfium.load_pdf_from_file(pdf, password).unwrap();
    let page = doc.pages().get(page_index as i32).unwrap();
    let (all, chars) = pdfium_chars(&page);

    let reference: Vec<char> = ocr.chars().flat_map(|c| c.text.chars()).filter(|c| !c.is_whitespace()).collect();
    let quads: Vec<QuadPt> = ocr
        .chars()
        .flat_map(|c| std::iter::repeat_n(c.quad, c.text.chars().filter(|c| !c.is_whitespace()).count()))
        .collect();
    let n = reference.len().max(1) as f64;

    let (d_all, _) = align(&reference, &strip(&all));
    let (d_pt, _) = align(&reference, &strip(&pdftotext_pw(pdf, page_index, password)));
    let extracted: Vec<char> = chars.iter().map(|c| c.ch).collect();
    let (_, map) = align(&reference, &extracted);
    let (mut aligned, mut inside, mut iou_sum) = (0usize, 0usize, 0.0);
    for (i, m) in map.iter().enumerate() {
        if let Some(j) = m {
            aligned += 1;
            let b = chars[*j].bbox;
            if in_quad(&quads[i], [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]) {
                inside += 1;
            }
            iou_sum += iou(b, quad_bbox(&quads[i]));
        }
    }

    let text = page.text().unwrap();
    let opts = PdfSearchOptions::new().match_case(true);
    let found = |q: &str| text.search(q, &opts).map(|s| s.find_next().is_some()).unwrap_or(false);
    let (mut queries, mut hits, mut sq, mut sh, mut vq, mut vh) = (0, 0, 0, 0, 0, 0);
    for line in ocr.lines() {
        let vertical = line.orientation == crate::types::Orientation::Vertical;
        let c: Vec<char> = line.text.chars().collect();
        let mut qs: Vec<String> = vec![line.text.clone()];
        if c.len() > 4 {
            qs.extend(c.chunks(4).filter(|w| w.len() >= 2).map(|w| w.iter().collect::<String>()));
        }
        for q in qs {
            if q.trim().is_empty() {
                continue;
            }
            let ok = found(&q);
            queries += 1;
            hits += usize::from(ok);
            if q.chars().any(is_special) {
                sq += 1;
                sh += usize::from(ok);
            }
            if vertical {
                vq += 1;
                vh += usize::from(ok);
            }
        }
    }
    let ocr_spaces = ocr.lines().map(|l| l.words.len().saturating_sub(1)).sum::<usize>() as i64;
    let extra_spaces = all.chars().filter(|c| *c == ' ').count() as i64 - ocr_spaces;
    let ratio = |a: usize, b: usize| if b == 0 { f64::NAN } else { a as f64 / b as f64 };
    Score {
        ocr_chars: reference.len(),
        text_pdfium: 1.0 - d_all as f64 / n,
        text_pdftotext: 1.0 - d_pt as f64 / n,
        aligned,
        centre_in_quad: ratio(inside, aligned),
        mean_iou: if aligned == 0 { f64::NAN } else { iou_sum / aligned as f64 },
        queries,
        found: ratio(hits, queries),
        special_queries: sq,
        special_found: ratio(sh, sq),
        extra_spaces,
        vertical_queries: vq,
        vertical_found: ratio(vh, vq),
    }
}

pub fn header() -> String {
    "| ページ | 字 | 全文 pdfium | 全文 pdftotext | 箱の中心が 4 点の中 | IoU | 検索 | 記号を含む検索 | 縦書きの検索 | 余計な空白 |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |".into()
}

pub fn row(name: &str, s: &Score) -> String {
    let p = |v: f64| if v.is_nan() { "—".to_string() } else { format!("{:.1} %", v * 100.0) };
    format!(
        "| {name} | {} | {} | {} | {} | {:.2} | {} ({}) | {} ({}) | {} ({}) | {} |",
        s.ocr_chars,
        p(s.text_pdfium),
        p(s.text_pdftotext),
        p(s.centre_in_quad),
        s.mean_iou,
        p(s.found),
        s.queries,
        p(s.special_found),
        s.special_queries,
        p(s.vertical_found),
        s.vertical_queries,
        s.extra_spaces
    )
}

/// Dumps pdfium's text and per-character boxes for a quick look.
pub fn dump(pdfium: &Pdfium, pdf: &Path, page_index: usize) {
    let doc = pdfium.load_pdf_from_file(pdf, None).unwrap();
    let page = doc.pages().get(page_index as i32).unwrap();
    let (all, chars) = pdfium_chars(&page);
    println!("--- pdfium text\n{all}\n--- pdftotext\n{}", pdftotext(pdf, page_index));
    for c in chars.iter().take(40) {
        println!("{} {:.1} {:.1} {:.1} {:.1}", c.ch, c.bbox[0], c.bbox[1], c.bbox[2], c.bbox[3]);
    }
}
