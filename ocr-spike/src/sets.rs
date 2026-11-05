//! Vertical item 1: the page sets the vertical work is scored on.
//!
//! - `dev`: the four item-5 pages (`vertical::pages`), the only pages rules
//!   and thresholds are chosen on.
//! - `val`: the generated validation pages (`valpages::pages`).
//! - `ndl`: real scans from NDL's pdmocrdataset-part1 (`sets/ndl_pages.txt`,
//!   fetched by `scripts/ndl_fetch.py`), `dontcare` painted white.
//!
//! Ground truth of the generated pages is committed under `sets/<set>/`;
//! images are regenerated into `data/` (git-ignored) by `gen-sets`.

use std::path::Path;

use crate::cer::Boxed;
use crate::imgops::Bgr;
use crate::vertical::{GtBlock, Typesetter};

/// A line of the ground truth with its characters' em boxes.
#[derive(Clone, Debug)]
pub struct GtLine {
    pub vertical: bool,
    pub chars: Vec<char>,
    pub boxes: Vec<[f64; 4]>,
    pub bbox: [f64; 4],
}

pub struct EvalPage {
    pub name: String,
    pub image: Bgr,
    /// The page's text in reading order.
    pub gt_text: String,
    /// Lines with boxes (for the matched CER and pairing detected lines).
    pub gt_lines: Vec<Boxed>,
    /// Lines with per-character boxes (generated pages only).
    pub char_lines: Vec<GtLine>,
}

fn image_dir(root: &Path, set: &str) -> std::path::PathBuf {
    match set {
        "dev" => root.join("data/vertical"),
        other => root.join("data").join(other),
    }
}

/// Draws the `dev` and `val` pages into `data/` and writes their ground
/// truth into `sets/dev/` and `sets/val/`.
pub fn generate(root: &Path) {
    let plex = std::fs::read(root.join("data/fonts/IBMPlexSansJP-Regular.otf")).unwrap();
    let noto = std::fs::read(root.join("data/fonts/NotoSerifJP-Regular.otf")).unwrap();
    let legacy = Typesetter::new(&plex, &root.join("data/fonts/plex_vert.json"));
    let mut gothic = Typesetter::new(&plex, &root.join("data/fonts/plex_vert.json"));
    gothic.em_align = true;
    let mut mincho = Typesetter::new(&noto, &root.join("data/fonts/noto_serif_vert.json"));
    mincho.em_align = true;
    // Mincho pages are set solid (1 em per character) like printed books;
    // the Gothic pages keep the development pages' spacing (em = 2/3 of the
    // nominal size, 1.5 em per character) as a control.
    mincho.true_em = true;
    for (set, pages) in [("dev", crate::vertical::pages(&legacy)), ("val", crate::valpages::pages(&mincho, &gothic))] {
        let (img_dir, gt_dir) = (image_dir(root, set), root.join("sets").join(set));
        std::fs::create_dir_all(&img_dir).unwrap();
        std::fs::create_dir_all(&gt_dir).unwrap();
        for (name, img, gt) in pages {
            img.save(img_dir.join(format!("{name}.png"))).unwrap();
            std::fs::write(gt_dir.join(format!("{name}.json")), serde_json::to_string_pretty(&gt).unwrap() + "\n").unwrap();
            let chars: usize = gt.iter().flat_map(|b| &b.lines).map(|l| l.chars().count()).sum();
            println!("{set}/{name}: blocks {}, characters {chars}", gt.len());
        }
    }
}

fn generated(root: &Path, set: &str) -> Vec<EvalPage> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("sets").join(set))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".json"))
        .map(|n| n.trim_end_matches(".json").to_string())
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let gt: Vec<GtBlock> =
                serde_json::from_str(&std::fs::read_to_string(root.join("sets").join(set).join(format!("{name}.json"))).unwrap()).unwrap();
            let image = Bgr::open(&image_dir(root, set).join(format!("{name}.png"))).expect("run gen-sets first");
            let gt_text = gt.iter().flat_map(|b| b.lines.iter().map(String::as_str)).collect();
            let gt_lines = gt
                .iter()
                .flat_map(|b| b.lines.iter().zip(&b.line_boxes).map(|(t, bb)| Boxed { text: t.clone(), bbox: *bb }))
                .collect();
            let char_lines = gt
                .iter()
                .flat_map(|b| {
                    b.lines.iter().zip(&b.char_boxes).map(|(t, boxes)| {
                        let x0 = boxes.iter().map(|q| q[0]).fold(f64::MAX, f64::min);
                        let y0 = boxes.iter().map(|q| q[1]).fold(f64::MAX, f64::min);
                        let x1 = boxes.iter().map(|q| q[2]).fold(f64::MIN, f64::max);
                        let y1 = boxes.iter().map(|q| q[3]).fold(f64::MIN, f64::max);
                        GtLine { vertical: b.vertical, chars: t.chars().collect(), boxes: boxes.clone(), bbox: [x0, y0, x1, y1] }
                    })
                })
                .collect();
            EvalPage { name, image, gt_text, gt_lines, char_lines }
        })
        .collect()
}

fn quad_points(q: &serde_json::Value) -> [[f64; 2]; 4] {
    let g = |k: &str| q[k].as_f64().unwrap();
    [[g("x1"), g("y1")], [g("x2"), g("y2")], [g("x3"), g("y3")], [g("x4"), g("y4")]]
}

/// Paints a convex quad white.
fn fill_quad(img: &mut Bgr, q: &[[f64; 2]; 4]) {
    let y0 = q.iter().map(|p| p[1]).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let y1 = (q.iter().map(|p| p[1]).fold(f64::MIN, f64::max).ceil() as usize).min(img.h);
    let x0 = q.iter().map(|p| p[0]).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let x1 = (q.iter().map(|p| p[0]).fold(f64::MIN, f64::max).ceil() as usize).min(img.w);
    let inside = |x: f64, y: f64| {
        let mut sign = 0.0;
        for k in 0..4 {
            let (a, b) = (q[k], q[(k + 1) % 4]);
            let c = (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
            if c != 0.0 {
                if sign == 0.0 {
                    sign = c.signum();
                } else if c.signum() != sign {
                    return false;
                }
            }
        }
        true
    };
    for y in y0..y1 {
        for x in x0..x1 {
            if inside(x as f64 + 0.5, y as f64 + 0.5) {
                let i = (y * img.w + x) * 3;
                img.data[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
    }
}

fn ndl(root: &Path) -> Vec<EvalPage> {
    let list = std::fs::read_to_string(root.join("sets/ndl_pages.txt")).unwrap();
    list.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|rel| {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("data/ndl/json").join(rel)).unwrap()).unwrap();
            let mut image = Bgr::open(&root.join("data/ndl/img").join(rel.replace(".json", ".jpg"))).expect("run scripts/ndl_fetch.py img");
            for group in v["dontcare"].as_array().unwrap() {
                // Each entry is a list of quads.
                let quads: Vec<&serde_json::Value> = match group.as_array() {
                    Some(a) => a.iter().collect(),
                    None => vec![group],
                };
                for q in quads {
                    fill_quad(&mut image, &quad_points(q));
                }
            }
            let mut boxes: Vec<&serde_json::Value> = v["boxes"].as_array().unwrap().iter().collect();
            boxes.sort_by_key(|b| b["box_id"].as_i64().unwrap());
            let gt_text = boxes.iter().map(|b| b["text"].as_str().unwrap()).collect();
            let gt_lines = boxes
                .iter()
                .map(|b| {
                    let q = quad_points(&b["quad"]);
                    Boxed { text: b["text"].as_str().unwrap().to_string(), bbox: crate::geom::quad_bounds(&q) }
                })
                .collect();
            EvalPage { name: rel.to_string(), image, gt_text, gt_lines, char_lines: Vec::new() }
        })
        .collect()
}

pub fn load(root: &Path, set: &str) -> Vec<EvalPage> {
    match set {
        "ndl" => ndl(root),
        s => generated(root, s),
    }
}
