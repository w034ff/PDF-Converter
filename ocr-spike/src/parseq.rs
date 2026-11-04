//! Item 8: NDLOCR-Lite's PARSeq for vertical lines.
//!
//! Follows `ndlocr-lite/src/parseq.py` and `process_cascade` in `ocr.py`
//! (commit d001dad): the crop is stretched to the model's fixed input,
//! RGB, `x / 127.5 - 1`; decoding is argmax per step up to the first EOS
//! (index 0), class `i` being `charset_train[i - 1]`. Three models (30, 50,
//! 100 characters) are tried from the one the estimated length suggests, and
//! a result that fills a model (≥ 25 of 30, ≥ 45 of 50) is read again by the
//! next one. Input width / height and the maximum length are read from each
//! model, not hard-coded.

use std::path::Path;

use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use crate::engine::load_session_with;
use crate::imgops::{self, Bgr};

pub struct Model {
    pub session: Session,
    pub width: usize,
    pub height: usize,
    /// Characters the model can emit (output steps minus the EOS step).
    pub max_chars: usize,
    pub name: String,
}

pub struct Parseq {
    /// Sorted by `max_chars`.
    pub models: Vec<Model>,
    pub charset: Vec<char>,
}

#[derive(Clone, Debug, Default)]
pub struct ParseqText {
    pub text: String,
    /// Mean softmax probability of the emitted characters.
    pub score: f32,
    pub model_chars: usize,
}

/// A crop that is horizontal but nearly square would be turned by PARSeq's
/// own rule (`h > 0.8 w`); it is padded left and right with white instead.
pub const NO_TURN_RATIO: f64 = 0.8;

/// A model's result this close to its capacity is read again by the next
/// larger model (NDLOCR-Lite: 25 of 30, 45 of 50).
pub const CASCADE_MARGIN: usize = 5;

impl Parseq {
    /// `ndl` is the NDLOCR-Lite checkout (for the charset); `model_dir` holds
    /// the `*parseq*.onnx` files: NDLOCR-Lite's own, or copies already
    /// optimised by onnxruntime, which load about 3× faster.
    pub fn load(env: &Environment, ndl: &Path, model_dir: &Path, threads: usize) -> ort::Result<Self> {
        let mut models = Vec::new();
        for entry in std::fs::read_dir(model_dir).expect("PARSeq models") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.contains("parseq") || !name.ends_with(".onnx") {
                continue;
            }
            let session = load_session_with(env, &path, threads, std::env::var("OCR_PARSEQ_ARENA").as_deref() != Ok("0"))?;
            let input = session.inputs()[0].dtype().tensor_shape().expect("tensor input").to_vec();
            let output = session.outputs()[0].dtype().tensor_shape().expect("tensor output").to_vec();
            models.push(Model {
                width: input[3] as usize,
                height: input[2] as usize,
                max_chars: output[1] as usize - 1,
                session,
                name,
            });
        }
        models.sort_by_key(|m| m.max_chars);
        Ok(Self { models, charset: load_charset(&ndl.join("src/config/NDLmoji.yaml")) })
    }

    fn read_with(&mut self, k: usize, crop: &Bgr) -> ort::Result<ParseqText> {
        let charset = &self.charset;
        let m = &mut self.models[k];
        let r = imgops::resize_linear(crop, m.width, m.height);
        let plane = m.width * m.height;
        let mut x = vec![0f32; 3 * plane];
        for (i, px) in r.data.chunks_exact(3).enumerate() {
            // BGR in, RGB to the model.
            for c in 0..3 {
                x[c * plane + i] = f32::from(px[2 - c]) / 127.5 - 1.0;
            }
        }
        let input = Tensor::from_array(([1i64, 3, m.height as i64, m.width as i64], x))?;
        let outputs = m.session.run(ort::inputs![input])?;
        let (shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
        let (steps, classes) = (shape[1] as usize, shape[2] as usize);
        let mut out = ParseqText { model_chars: m.max_chars, ..Default::default() };
        let mut probs = Vec::new();
        for s in 0..steps {
            let row = &logits[s * classes..(s + 1) * classes];
            let (best, max) = row.iter().enumerate().fold((0, f32::MIN), |b, (i, &v)| if v > b.1 { (i, v) } else { b });
            if best == 0 {
                break;
            }
            let sum: f32 = row.iter().map(|&v| (v - max).exp()).sum();
            probs.push(1.0 / sum);
            if let Some(&ch) = charset.get(best - 1) {
                out.text.push(ch);
            }
        }
        out.score = if probs.is_empty() { 0.0 } else { probs.iter().sum::<f32>() / probs.len() as f32 };
        Ok(out)
    }

    /// Reads one crop that is already horizontal (vertical lines were turned
    /// 90° counter-clockwise by the crop step, as PARSeq would do itself).
    pub fn read(&mut self, crop: &Bgr) -> ort::Result<ParseqText> {
        let mut crop = crop.clone();
        if crop.h as f64 > NO_TURN_RATIO * crop.w as f64 {
            crop = pad_sides(&crop, (crop.h as f64 / NO_TURN_RATIO).ceil() as usize + 1);
        }
        // Estimated length: characters are roughly square.
        let est = (crop.w as f64 / crop.h as f64).round() as usize;
        let mut k = self
            .models
            .iter()
            .position(|m| est + CASCADE_MARGIN <= m.max_chars)
            .unwrap_or(self.models.len() - 1);
        loop {
            let r = self.read_with(k, &crop)?;
            let full = r.text.chars().count() + CASCADE_MARGIN >= self.models[k].max_chars;
            if full && k + 1 < self.models.len() {
                k += 1;
                continue;
            }
            return Ok(r);
        }
    }
}

fn pad_sides(img: &Bgr, w: usize) -> Bgr {
    let mut out = Bgr::new(w, img.h);
    out.data.fill(255);
    let left = (w - img.w) / 2;
    for y in 0..img.h {
        let src = &img.data[y * img.w * 3..(y + 1) * img.w * 3];
        out.data[(y * w + left) * 3..(y * w + left + img.w) * 3].copy_from_slice(src);
    }
    out
}

fn load_charset(path: &Path) -> Vec<char> {
    let text = std::fs::read_to_string(path).expect("NDLmoji.yaml");
    let docs = yaml_rust2::YamlLoader::load_from_str(&text).expect("parse NDLmoji.yaml");
    docs[0]["model"]["charset_train"].as_str().expect("charset_train").chars().collect()
}

/// Character cells (fractions 0–1 along the crop) by the ink between the
/// characters: the crop's darkness is summed per column and each of the
/// `n - 1` cuts goes to the lightest column within ±30 % of a cell of where
/// an even split would put it.
pub fn projection_cells(crop: &Bgr, n: usize) -> Vec<(f64, f64)> {
    if n == 0 {
        return Vec::new();
    }
    let ink: Vec<u32> = (0..crop.w)
        .map(|x| (0..crop.h).map(|y| 255 - u32::from(crop.at(x, y, 1))).sum())
        .collect();
    // Trim blank margins so that the cells cover the text, not the padding.
    let thresh = ink.iter().copied().max().unwrap_or(0) / 20;
    let first = ink.iter().position(|&v| v > thresh).unwrap_or(0);
    let last = ink.iter().rposition(|&v| v > thresh).unwrap_or(crop.w.saturating_sub(1));
    let (a, b) = (first as f64, (last + 1) as f64);
    let cell = (b - a) / n as f64;
    let mut cuts = vec![a];
    for k in 1..n {
        let want = a + cell * k as f64;
        let lo = (want - 0.3 * cell).max(a) as usize;
        let hi = ((want + 0.3 * cell) as usize).min(crop.w.saturating_sub(1));
        let best = (lo..=hi.max(lo)).min_by_key(|&x| (ink[x.min(crop.w - 1)], (x as f64 - want).abs() as u32)).unwrap_or(want as usize);
        cuts.push(best as f64 + 0.5);
    }
    cuts.push(b);
    let w = crop.w as f64;
    (0..n).map(|k| (cuts[k] / w, cuts[k + 1] / w)).collect()
}

/// Character cells for `target` taken from another reading of the same line
/// (`source` with its cells, e.g. PP-OCR's CTC cells): characters that the
/// alignment pairs (equal characters on the edit path) take the source cell;
/// the others (punctuation the CTC model dropped) share the gap between
/// their paired neighbours evenly.
pub fn aligned_cells(target: &str, source: &[(char, (f64, f64))]) -> Vec<(f64, f64)> {
    let t: Vec<char> = target.chars().collect();
    let s: Vec<char> = source.iter().map(|c| c.0).collect();
    let (n, m) = (t.len(), s.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        d[i][0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            d[i][j] = (d[i - 1][j - 1] + usize::from(t[i - 1] != s[j - 1])).min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
        }
    }
    let mut pair: Vec<Option<usize>> = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if t[i - 1] == s[j - 1] && d[i][j] == d[i - 1][j - 1] {
            pair[i - 1] = Some(j - 1);
            i -= 1;
            j -= 1;
        } else if d[i][j] == d[i - 1][j - 1] + 1 {
            i -= 1;
            j -= 1;
        } else if d[i][j] == d[i - 1][j] + 1 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    let mut cells: Vec<Option<(f64, f64)>> = pair.iter().map(|p| p.map(|j| source[j].1)).collect();
    // Fill runs of unpaired characters between the paired neighbours.
    let mut k = 0;
    while k < n {
        if cells[k].is_some() {
            k += 1;
            continue;
        }
        let start = k;
        while k < n && cells[k].is_none() {
            k += 1;
        }
        let lo = if start == 0 { 0.0 } else { cells[start - 1].unwrap().1 };
        let hi = if k == n { 1.0 } else { cells[k].unwrap().0 };
        let step = (hi - lo).max(0.0) / (k - start) as f64;
        for (q, c) in cells[start..k].iter_mut().enumerate() {
            *c = Some((lo + step * q as f64, lo + step * (q + 1) as f64));
        }
    }
    cells.into_iter().map(|c| c.unwrap_or((0.0, 1.0))).collect()
}
