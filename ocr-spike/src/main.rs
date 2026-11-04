use ocr_spike::{engine, geom, imgops};

use std::path::{Path, PathBuf};

use engine::{DetParams, Ocr, Timings};
use imgops::Bgr;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn runtime() -> PathBuf {
    std::env::var_os("ORT_DYLIB")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("../src-tauri/onnxruntime/linux/libonnxruntime.so"))
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

pub fn open_ocr(env: &ort::environment::Environment, args: &[String]) -> Ocr {
    let model = arg(args, "--model").unwrap_or("small");
    let threads = arg(args, "--threads").map_or_else(engine::default_threads, |t| t.parse().unwrap());
    let dir = root().join("models");
    let mut ocr = Ocr::load(env, &dir.join(format!("{model}_det")), &dir.join(format!("{model}_rec")), threads)
        .expect("load models");
    if let Some(b) = arg(args, "--batch") {
        ocr.rec_batch = b.parse().unwrap();
    }
    if args.iter().any(|a| a == "--paddle-default") {
        ocr.det_params = DetParams::paddle_default();
    }
    if let Some(l) = arg(args, "--limit") {
        ocr.det_params.limit_side_len = l.parse().unwrap();
    }
    ocr
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let env = engine::ort_init(&runtime()).expect("load onnxruntime");
    match args.get(1).map(String::as_str) {
        Some("run") => {
            let mut ocr = open_ocr(&env, &args);
            let img = Bgr::open(Path::new(&args[2])).expect("open image");
            let mut t = Timings::default();
            let lines = ocr.run(&img, &mut t).expect("ocr");
            for l in &lines {
                let b = geom::quad_bounds(&l.quad);
                println!(
                    "{:>5.0} {:>5.0} {:>5.0} {:>5.0} {} {:.3} {}",
                    b[0], b[1], b[2], b[3], if l.vertical { "V" } else { "H" }, l.score, l.text
                );
            }
            eprintln!("det {:?} rec {:?} {:?}", t.det(), t.rec(), t);
        }
        Some("parity") => parity(&env, &args),
        _ => eprintln!("usage: ocr-spike run|parity ..."),
    }
}

struct RefRegion {
    text: String,
    quad: geom::Quad,
}

fn load_reference(path: &Path) -> (String, String, Vec<RefRegion>) {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let regions = v["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let q = r["quad"].as_array().unwrap();
            let p = |i: usize| [q[i][0].as_f64().unwrap(), q[i][1].as_f64().unwrap()];
            RefRegion { text: r["text"].as_str().unwrap().to_string(), quad: [p(0), p(1), p(2), p(3)] }
        })
        .collect();
    (v["model"].as_str().unwrap().into(), v["image"].as_str().unwrap().into(), regions)
}

/// Item 2: compares with PaddleOCR 3.7's output in pure-onnx-ocr's `tests/reference/`.
fn parity(env: &ort::environment::Environment, args: &[String]) {
    let model = arg(args, "--model").unwrap_or("small");
    let refdir = root().join("third_party/pure-onnx-ocr/tests/reference");
    let mut cfg: Vec<String> = args.to_vec();
    cfg.push("--paddle-default".into());
    let mut ocr = open_ocr(env, &cfg);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&refdir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("v6-{model}__")))
        .collect();
    files.sort();
    for f in files {
        let (_, image, reference) = load_reference(&f);
        let path = root().join("data/ref").join(&image);
        let Ok(img) = Bgr::open(&path) else {
            println!("{image}: image missing, skipped");
            continue;
        };
        let mut t = Timings::default();
        let ours = ocr.run(&img, &mut t).unwrap();
        // Greedy one-to-one matching by IoU of the bounds.
        let mut pairs = Vec::new();
        for (i, r) in reference.iter().enumerate() {
            for (j, o) in ours.iter().enumerate() {
                let v = geom::iou(&geom::quad_bounds(&r.quad), &geom::quad_bounds(&o.quad));
                if v >= 0.5 {
                    pairs.push((v, i, j));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (mut ur, mut uo) = (vec![false; reference.len()], vec![false; ours.len()]);
        let (mut matched, mut same_quad, mut exact, mut max_dev) = (0, 0, 0, 0f64);
        let mut notes = Vec::new();
        for (_, i, j) in pairs {
            if ur[i] || uo[j] {
                continue;
            }
            ur[i] = true;
            uo[j] = true;
            matched += 1;
            let dev = (0..4)
                .flat_map(|k| (0..2).map(move |c| (k, c)))
                .map(|(k, c)| (reference[i].quad[k][c] - ours[j].quad[k][c]).abs())
                .fold(0f64, f64::max);
            max_dev = max_dev.max(dev);
            if dev == 0.0 {
                same_quad += 1;
            } else {
                notes.push(format!("  quad differs by {dev} px: ref {:?} ours {:?}", reference[i].quad, ours[j].quad));
            }
            if reference[i].text == ours[j].text {
                exact += 1;
            } else {
                notes.push(format!("  text: ref {:?} ours {:?}", reference[i].text, ours[j].text));
            }
        }
        for (i, r) in reference.iter().enumerate() {
            if !ur[i] {
                notes.push(format!("  only in reference: {:?} {:?}", r.text, r.quad));
            }
        }
        for (j, o) in ours.iter().enumerate() {
            if !uo[j] {
                notes.push(format!("  only ours: {:?} {:?}", o.text, o.quad));
            }
        }
        // Same order as PaddleOCR (both sort with SortQuadBoxes)?
        let same_order = reference.len() == ours.len()
            && reference.iter().zip(&ours).all(|(r, o)| r.text == o.text);
        println!(
            "v6-{model} {image}: reference {} ours {} matched {} identical quads {} identical text {} max corner deviation {} px, same order {}",
            reference.len(),
            ours.len(),
            matched,
            same_quad,
            exact,
            max_dev,
            same_order
        );
        for n in notes {
            println!("{n}");
        }
    }
}
