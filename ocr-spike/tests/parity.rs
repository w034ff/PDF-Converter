//! Item 2: our Det + Rec on `ort` against PaddleOCR 3.7's output kept in
//! pure-onnx-ocr's `tests/reference/` (fetched by `scripts/fetch.sh`).
//! Skips when the models, references or images are missing.

use std::path::{Path, PathBuf};

use ocr_spike::engine::{self, DetParams, Ocr, Timings};
use ocr_spike::geom;
use ocr_spike::imgops::Bgr;

/// OpenCV computes minAreaRect/boxPoints in float32 and pyclipper truncates
/// the result, which moves some quads by a pixel (REPORT.md, item 2).
const MAX_CORNER_DEVIATION_PX: f64 = 2.0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn matches_paddleocr_reference() {
    let runtime = root().join("../src-tauri/onnxruntime/linux/libonnxruntime.so");
    let refdir = root().join("third_party/pure-onnx-ocr/tests/reference");
    if !runtime.exists() || !refdir.exists() {
        eprintln!("skipped: runtime or references missing");
        return;
    }
    let env = engine::ort_init(&runtime).unwrap();
    let mut checked = 0;
    for model in ["small", "medium"] {
        let (det, rec) = (root().join(format!("models/{model}_det")), root().join(format!("models/{model}_rec")));
        if !det.exists() || !rec.exists() {
            continue;
        }
        let mut ocr = Ocr::load(&env, &det, &rec, engine::default_threads()).unwrap();
        ocr.det_params = DetParams::paddle_default();
        for image in ["ja.jpg", "general_ocr_002.jpg"] {
            let reference = refdir.join(format!("v6-{model}__{}.json", Path::new(image).file_stem().unwrap().to_str().unwrap()));
            let Ok(img) = Bgr::open(&root().join("data/ref").join(image)) else { continue };
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(reference).unwrap()).unwrap();
            let regions = v["regions"].as_array().unwrap();
            let ours = ocr.run(&img, &mut Timings::default()).unwrap();
            assert_eq!(ours.len(), regions.len(), "{model}/{image}: number of lines");
            // Same order (both sort with SortQuadBoxes), same text, quads within tolerance.
            for (o, r) in ours.iter().zip(regions) {
                assert_eq!(o.text, r["text"].as_str().unwrap(), "{model}/{image}");
                let q = r["quad"].as_array().unwrap();
                for k in 0..4 {
                    for c in 0..2 {
                        let d = (o.quad[k][c] - q[k][c].as_f64().unwrap()).abs();
                        assert!(d <= MAX_CORNER_DEVIATION_PX, "{model}/{image} {:?}: corner off by {d}", o.text);
                    }
                }
                let _ = geom::quad_bounds(&o.quad);
            }
            checked += 1;
        }
    }
    eprintln!("checked {checked} model/image pairs");
}
