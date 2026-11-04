//! PP-OCRv6 detection and recognition on `ort`, with PaddleOCR 3.x's pre-
//! and post-processing written directly against `Vec<f32>` / `&[f32]`.

use std::path::Path;
use std::time::{Duration, Instant};

use imageproc::contours::find_contours;
use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

use crate::geom::{self, Pt, Quad};
use crate::imgops::{self, Bgr};

pub const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
pub const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];
pub const REC_HEIGHT: usize = 48;
pub const REC_MIN_WIDTH: usize = 320;
pub const REC_MAX_WIDTH: usize = 3200;
/// PP-OCRv6: a crop whose height / width is at least this is turned 90°.
pub const VERTICAL_RATIO: f64 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LimitType {
    Max,
    Min,
}

#[derive(Clone, Debug)]
pub struct DetParams {
    pub limit_type: LimitType,
    pub limit_side_len: usize,
    pub max_side_limit: usize,
    pub thresh: f32,
    pub box_thresh: f64,
    pub unclip_ratio: f64,
    pub max_candidates: usize,
}

impl DetParams {
    /// The configuration chosen in the survey: long side 1600, the pipeline's thresholds.
    pub fn chosen() -> Self {
        Self { limit_type: LimitType::Max, limit_side_len: 1600, ..Self::paddle_default() }
    }

    /// PaddleOCR 3.x pipeline defaults, used for the reference outputs.
    pub fn paddle_default() -> Self {
        Self {
            limit_type: LimitType::Min,
            limit_side_len: 64,
            max_side_limit: 4000,
            thresh: 0.3,
            box_thresh: 0.6,
            unclip_ratio: 1.5,
            max_candidates: 3000,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Timings {
    pub det_pre: Duration,
    pub det_run: Duration,
    pub det_post: Duration,
    pub crop: Duration,
    pub rec_pre: Duration,
    pub rec_run: Duration,
    pub rec_post: Duration,
}

impl Timings {
    pub fn det(&self) -> Duration {
        self.det_pre + self.det_run + self.det_post
    }
    pub fn rec(&self) -> Duration {
        self.crop + self.rec_pre + self.rec_run + self.rec_post
    }
}

/// One recognised line: the detection quad (image pixels, tl/tr/br/bl), text,
/// mean probability, whether it was read as vertical, and per-character
/// columns from the CTC output (see [`RecText::char_cols`]).
#[derive(Clone, Debug)]
pub struct Line {
    pub quad: Quad,
    pub text: String,
    pub score: f32,
    pub vertical: bool,
    pub rec: RecText,
    /// Width of the crop after resizing to height 48 and the tensor width it was padded to.
    pub crop_px: (usize, usize),
    pub tensor_w: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RecText {
    pub text: String,
    pub score: f32,
    /// For each emitted character: the first and last time step of its run
    /// and its probability. One time step covers 8 input columns.
    pub char_cols: Vec<(char, usize, usize, f32)>,
    pub time_steps: usize,
}

pub fn ort_init(runtime: &Path) -> Result<Environment, String> {
    ort::init_from(runtime).map_err(|e| e.to_string())?.build().map_err(|e| e.to_string())
}

/// Same settings as `crates/worker/src/layout.rs::load_session`.
pub fn load_session(env: &Environment, model: &Path, threads: usize) -> ort::Result<Session> {
    Session::builder(env)?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(threads)?
        .with_inter_threads(1)?
        .with_intra_op_spinning(false)?
        .commit_from_file(model)
}

pub fn default_threads() -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    6.min(cpus.saturating_sub(1)).max(1)
}

/// The dictionary of the rec model's `inference.yml`, with the CTC blank in
/// front and the space at the end (`use_space_char=True`).
pub fn load_dictionary(yml: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(yml).expect("read inference.yml");
    let docs = yaml_rust2::YamlLoader::load_from_str(&text).expect("parse inference.yml");
    let list = docs[0]["PostProcess"]["character_dict"].as_vec().expect("character_dict");
    let mut chars = vec!["".to_string()];
    for c in list {
        let s = match c {
            yaml_rust2::Yaml::String(s) => s.clone(),
            yaml_rust2::Yaml::Integer(i) => i.to_string(),
            yaml_rust2::Yaml::Real(r) => r.clone(),
            yaml_rust2::Yaml::Boolean(b) => b.to_string(),
            other => panic!("unexpected dictionary entry {other:?}"),
        };
        chars.push(s);
    }
    chars.push(" ".to_string());
    chars
}

pub struct Ocr {
    pub det: Session,
    pub rec: Session,
    pub dict: Vec<String>,
    pub det_params: DetParams,
    pub rec_batch: usize,
}

/// Size after `DetResizeForTest.resize_image_type0`.
pub fn det_size(w: usize, h: usize, p: &DetParams) -> (usize, usize) {
    let ratio = match p.limit_type {
        LimitType::Max if w.max(h) > p.limit_side_len => p.limit_side_len as f64 / w.max(h) as f64,
        LimitType::Min if w.min(h) < p.limit_side_len => p.limit_side_len as f64 / w.min(h) as f64,
        _ => 1.0,
    };
    let (mut rh, mut rw) = ((h as f64 * ratio) as usize, (w as f64 * ratio) as usize);
    if rh.max(rw) > p.max_side_limit {
        let r = p.max_side_limit as f64 / rh.max(rw) as f64;
        rh = (rh as f64 * r) as usize;
        rw = (rw as f64 * r) as usize;
    }
    // Python's round(): halves go to the even neighbour.
    let r32 = |v: usize| (((v as f64 / 32.0).round_ties_even() as usize) * 32).max(32);
    (r32(rw), r32(rh))
}

pub fn det_input(img: &Bgr, p: &DetParams) -> (Vec<f32>, usize, usize) {
    let (rw, rh) = det_size(img.w, img.h, p);
    let r = imgops::resize_linear(img, rw, rh);
    let plane = rw * rh;
    let mut x = vec![0f32; 3 * plane];
    let alpha: [f32; 3] = std::array::from_fn(|c| 1.0 / 255.0 / DET_STD[c]);
    let beta: [f32; 3] = std::array::from_fn(|c| -DET_MEAN[c] / DET_STD[c]);
    for (i, px) in r.data.chunks_exact(3).enumerate() {
        for c in 0..3 {
            x[c * plane + i] = f32::from(px[c]) * alpha[c] + beta[c];
        }
    }
    (x, rw, rh)
}

/// `DBPostProcess.boxes_from_bitmap` on the probability map `pred` (`pw` × `ph`),
/// returning quads in the coordinates of an image `dw` × `dh`.
pub fn db_boxes(pred: &[f32], pw: usize, ph: usize, dw: usize, dh: usize, p: &DetParams) -> Vec<(Quad, f64)> {
    let bitmap = image::GrayImage::from_fn(pw as u32, ph as u32, |x, y| {
        image::Luma([if pred[y as usize * pw + x as usize] > p.thresh { 255 } else { 0 }])
    });
    let contours = find_contours::<i32>(&bitmap);
    let (sx, sy) = (dw as f64 / pw as f64, dh as f64 / ph as f64);
    let mut out = Vec::new();
    for c in contours.iter().take(p.max_candidates) {
        let pts: Vec<Pt> = c.points.iter().map(|q| [f64::from(q.x), f64::from(q.y)]).collect();
        let (q, sside) = geom::mini_box(&pts);
        if sside < 3.0 {
            continue;
        }
        let score = geom::quad_mean(pred, pw, ph, &q);
        if p.box_thresh > score {
            continue;
        }
        let (q2, sside2) = geom::mini_box(&geom::unclip(&q, p.unclip_ratio));
        if sside2 < 5.0 {
            continue;
        }
        let q2 = q2.map(|v| {
            [
                (v[0] * sx).round_ties_even().clamp(0.0, dw as f64),
                (v[1] * sy).round_ties_even().clamp(0.0, dh as f64),
            ]
        });
        out.push((q2, score));
    }
    out
}

/// `SortQuadBoxes`: top to bottom, then left to right within 10 px.
pub fn sort_quads(mut boxes: Vec<Quad>) -> Vec<Quad> {
    boxes.sort_by(|a, b| a[0][1].total_cmp(&b[0][1]).then(a[0][0].total_cmp(&b[0][0])));
    let n = boxes.len();
    for i in 0..n.saturating_sub(1) {
        for j in (0..=i).rev() {
            if (boxes[j + 1][0][1] - boxes[j][0][1]).abs() < 10.0 && boxes[j + 1][0][0] < boxes[j][0][0] {
                boxes.swap(j, j + 1);
            } else {
                break;
            }
        }
    }
    boxes
}

/// `get_minarea_rect_crop` + `get_rotate_crop_image`: the quad unrolled to an
/// axis-aligned image, turned when it is at least 1.5 times taller than wide.
pub fn crop_quad(img: &Bgr, quad: &Quad) -> (Bgr, bool) {
    let pts: Vec<Pt> = quad.iter().map(|p| [p[0].trunc(), p[1].trunc()]).collect();
    let (q, _) = geom::mini_box(&pts);
    let d = |a: Pt, b: Pt| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let w = d(q[0], q[1]).max(d(q[2], q[3])) as usize;
    let h = d(q[0], q[3]).max(d(q[1], q[2])) as usize;
    if w == 0 || h == 0 {
        return (Bgr::new(0, 0), false);
    }
    let dst = [[0.0, 0.0], [w as f64, 0.0], [w as f64, h as f64], [0.0, h as f64]];
    let inv = imgops::perspective_transform(&dst, &q);
    let crop = imgops::warp_perspective_cubic(img, &inv, w, h);
    if h as f64 / w as f64 >= VERTICAL_RATIO { (crop.rot90(), true) } else { (crop, false) }
}

/// Width of a crop resized to height 48 (`OCRReisizeNormImg.resize_norm_img`),
/// and the width of its own canvas.
pub fn rec_width(w: usize, h: usize) -> (usize, usize) {
    let ratio = w as f64 / h as f64;
    let max_ratio = (REC_MIN_WIDTH as f64 / REC_HEIGHT as f64).max(ratio);
    let canvas = (REC_HEIGHT as f64 * max_ratio) as usize;
    if canvas > REC_MAX_WIDTH {
        return (REC_MAX_WIDTH, REC_MAX_WIDTH);
    }
    let want = (REC_HEIGHT as f64 * ratio).ceil() as usize;
    (if want > canvas { canvas } else { want }, canvas)
}

/// Fills `out` (3 × 48 × `tw`, already zero) with one crop.
pub fn rec_fill(crop: &Bgr, out: &mut [f32], tw: usize) -> usize {
    let (rw, _) = rec_width(crop.w, crop.h);
    let r = imgops::resize_linear(crop, rw, REC_HEIGHT);
    let plane = REC_HEIGHT * tw;
    for y in 0..REC_HEIGHT {
        for x in 0..rw {
            let px = &r.data[(y * rw + x) * 3..(y * rw + x) * 3 + 3];
            for c in 0..3 {
                out[c * plane + y * tw + x] = f32::from(px[c]) / 255.0 * 2.0 - 1.0;
            }
        }
    }
    rw
}

/// Index and value of the largest element; the first one on ties, like numpy.
/// The maximum is found with 16 independent lanes so that it vectorises
/// (`max_by` on an iterator does not), then its first position is looked up.
#[inline]
fn argmax(row: &[f32]) -> (usize, f32) {
    let mut lanes = [f32::NEG_INFINITY; 16];
    let chunks = row.chunks_exact(16);
    let rest = chunks.remainder();
    for c in chunks {
        for k in 0..16 {
            lanes[k] = if c[k] > lanes[k] { c[k] } else { lanes[k] };
        }
    }
    let mut m = lanes.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    for &v in rest {
        m = m.max(v);
    }
    (row.iter().position(|&v| v == m).unwrap_or(0), m)
}

/// Greedy CTC decoding (`CTCLabelDecode`) of one `[T, C]` slice.
pub fn ctc_decode(probs: &[f32], t: usize, c: usize, dict: &[String]) -> RecText {
    let mut out = RecText { time_steps: t, ..Default::default() };
    let mut prev = usize::MAX;
    let mut confs = Vec::new();
    for step in 0..t {
        let row = &probs[step * c..(step + 1) * c];
        let (best, p) = argmax(row);
        if best != 0 && best != prev {
            let s = dict.get(best).map_or("", |s| s.as_str());
            out.text.push_str(s);
            for ch in s.chars() {
                out.char_cols.push((ch, step, step, p));
            }
            confs.push(p);
        } else if best != 0 && best == prev {
            if let Some(last) = out.char_cols.last_mut() {
                last.2 = step;
            }
        }
        prev = best;
    }
    out.score = if confs.is_empty() { 0.0 } else { confs.iter().sum::<f32>() / confs.len() as f32 };
    out
}

impl Ocr {
    pub fn load(env: &Environment, det_dir: &Path, rec_dir: &Path, threads: usize) -> ort::Result<Self> {
        Ok(Self {
            det: load_session(env, &det_dir.join("inference.onnx"), threads)?,
            rec: load_session(env, &rec_dir.join("inference.onnx"), threads)?,
            dict: load_dictionary(&rec_dir.join("inference.yml")),
            det_params: DetParams::chosen(),
            rec_batch: 6,
        })
    }

    pub fn detect(&mut self, img: &Bgr, t: &mut Timings) -> ort::Result<Vec<Quad>> {
        let s = Instant::now();
        let (x, rw, rh) = det_input(img, &self.det_params);
        let input = Tensor::from_array(([1i64, 3, rh as i64, rw as i64], x))?;
        t.det_pre += s.elapsed();
        let s = Instant::now();
        let outputs = self.det.run(ort::inputs![input])?;
        t.det_run += s.elapsed();
        let s = Instant::now();
        let (shape, pred) = outputs[0].try_extract_tensor::<f32>()?;
        let (ph, pw) = (shape[2] as usize, shape[3] as usize);
        let boxes = db_boxes(pred, pw, ph, img.w, img.h, &self.det_params);
        let quads = sort_quads(boxes.into_iter().map(|(q, _)| q).collect());
        t.det_post += s.elapsed();
        Ok(quads)
    }

    /// Recognises crops in PaddleOCR's order: sorted by width / height,
    /// batches of `rec_batch`, each batch padded with zeros to its widest crop.
    pub fn recognize(&mut self, crops: &[Bgr], t: &mut Timings) -> ort::Result<Vec<(RecText, usize, usize)>> {
        let mut order: Vec<usize> = (0..crops.len()).collect();
        order.sort_by(|&a, &b| {
            let ra = crops[a].w as f64 / crops[a].h as f64;
            let rb = crops[b].w as f64 / crops[b].h as f64;
            ra.total_cmp(&rb)
        });
        let mut results: Vec<Option<(RecText, usize, usize)>> = vec![None; crops.len()];
        for chunk in order.chunks(self.rec_batch.max(1)) {
            let s = Instant::now();
            let tw = chunk.iter().map(|&i| rec_width(crops[i].w, crops[i].h).1).max().unwrap();
            let per = 3 * REC_HEIGHT * tw;
            let mut x = vec![0f32; chunk.len() * per];
            let widths: Vec<usize> = chunk
                .iter()
                .enumerate()
                .map(|(k, &i)| rec_fill(&crops[i], &mut x[k * per..(k + 1) * per], tw))
                .collect();
            let input = Tensor::from_array(([chunk.len() as i64, 3, REC_HEIGHT as i64, tw as i64], x))?;
            t.rec_pre += s.elapsed();
            let s = Instant::now();
            let outputs = self.rec.run(ort::inputs![input])?;
            t.rec_run += s.elapsed();
            let s = Instant::now();
            let (shape, probs) = outputs[0].try_extract_tensor::<f32>()?;
            let (steps, classes) = (shape[1] as usize, shape[2] as usize);
            for (k, &i) in chunk.iter().enumerate() {
                let slice = &probs[k * steps * classes..(k + 1) * steps * classes];
                results[i] = Some((ctc_decode(slice, steps, classes, &self.dict), widths[k], tw));
            }
            t.rec_post += s.elapsed();
        }
        Ok(results.into_iter().map(Option::unwrap).collect())
    }

    pub fn run(&mut self, img: &Bgr, t: &mut Timings) -> ort::Result<Vec<Line>> {
        let quads = self.detect(img, t)?;
        let s = Instant::now();
        let mut kept = Vec::new();
        let mut crops = Vec::new();
        for q in quads {
            let (c, vertical) = crop_quad(img, &q);
            if c.w > 0 && c.h > 0 {
                kept.push((q, vertical));
                crops.push(c);
            }
        }
        t.crop += s.elapsed();
        let recs = self.recognize(&crops, t)?;
        Ok(kept
            .into_iter()
            .zip(recs)
            .map(|((quad, vertical), (rec, rw, tw))| Line {
                quad,
                text: rec.text.clone(),
                score: rec.score,
                vertical,
                rec,
                crop_px: (rw, REC_HEIGHT),
                tensor_w: tw,
            })
            .collect())
    }
}

/// Peak resident memory of this process (`VmHWM`), in MiB.
pub fn peak_rss_mib() -> f64 {
    proc_status_mib("VmHWM:")
}

/// Peak virtual memory (`VmPeak`), in MiB: what `RLIMIT_AS` limits.
pub fn peak_vm_mib() -> f64 {
    proc_status_mib("VmPeak:")
}

fn proc_status_mib(key: &str) -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<f64>().ok()))
        })
        .map_or(0.0, |kb| kb / 1024.0)
}
