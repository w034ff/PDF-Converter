use ocr_spike::{cer, engine, geom, imgops, order, pdf, types, vertical};

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
    if args.iter().any(|a| a == "--parseq") {
        let dir = root().join("third_party/ndlocr-lite");
        let s = std::time::Instant::now();
        let models = arg(args, "--parseq-models").map_or_else(|| dir.join("src/model"), |d| root().join(d));
        ocr.parseq = Some(ocr_spike::parseq::Parseq::load(env, &dir, &models, threads).expect("load PARSeq"));
        eprintln!("PARSeq loaded in {:.2} s", s.elapsed().as_secs_f64());
    }
    if let Some(m) = arg(args, "--cells") {
        ocr.cell_mode = match m {
            "even" => engine::CellMode::Even,
            "projection" => engine::CellMode::Projection,
            _ => engine::CellMode::Aligned,
        };
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
        Some("bench") => bench(&env, &args),
        Some("xfund") => xfund(&env, &args),
        Some("vertical") => vertical_pages(&env, &args),
        Some("chars") => chars(&env, &args),
        Some("pdf") => pdf_page(&env, &args),
        Some("vchars") => vchars(&env, &args),
        Some("page-json") => {
            let mut ocr = open_ocr(&env, &args);
            let img = Bgr::open(Path::new(&args[2])).unwrap();
            let lines = ocr.run(&img, &mut Timings::default()).unwrap();
            // As if the image were an A4 page shown upright.
            let page = types::build_page(&lines, img.w, img.h, (595.0, 842.0), "PP-OCRv6_small");
            println!("{}", serde_json::to_string_pretty(&page).unwrap());
        }
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

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Item 3: per page, `--warmup` runs then the median of `--runs` runs, Det and Rec apart.
fn bench(env: &ort::environment::Environment, args: &[String]) {
    let warmup: usize = arg(args, "--warmup").map_or(3, |v| v.parse().unwrap());
    let runs: usize = arg(args, "--runs").map_or(5, |v| v.parse().unwrap());
    let start = std::time::Instant::now();
    let mut ocr = open_ocr(env, args);
    let load_s = start.elapsed().as_secs_f64();
    let images: Vec<&String> = args[2..].iter().filter(|a| a.ends_with(".jpg") || a.ends_with(".png")).collect();
    let mut pages = Vec::new();
    for path in images {
        let img = Bgr::open(Path::new(path)).unwrap();
        let mut ts = Vec::new();
        let mut texts = Vec::new();
        for i in 0..warmup + runs {
            let mut t = Timings::default();
            let lines = ocr.run(&img, &mut t).unwrap();
            if i >= warmup {
                ts.push(t);
            }
            texts = lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>();
        }
        let ms = |f: &dyn Fn(&Timings) -> std::time::Duration| median(ts.iter().map(|t| f(t).as_secs_f64() * 1000.0).collect());
        let row = serde_json::json!({
            "image": Path::new(path).file_name().unwrap().to_string_lossy(),
            "lines": texts.len(),
            "det_ms": ms(&|t| t.det()), "rec_ms": ms(&|t| t.rec()),
            "det_pre": ms(&|t| t.det_pre), "det_run": ms(&|t| t.det_run), "det_post": ms(&|t| t.det_post),
            "parseq": ms(&|t| t.parseq), "crop": ms(&|t| t.crop), "rec_pre": ms(&|t| t.rec_pre), "rec_run": ms(&|t| t.rec_run), "rec_post": ms(&|t| t.rec_post),
            "texts": texts,
        });
        eprintln!(
            "{}: lines {} det {:.0} ms rec {:.0} ms det_pre {:.1} det_run {:.1} det_post {:.1} crop {:.1} rec_pre {:.1} rec_run {:.1} rec_post {:.1}",
            row["image"].as_str().unwrap(), row["lines"], row["det_ms"].as_f64().unwrap(), row["rec_ms"].as_f64().unwrap(),
            row["det_pre"].as_f64().unwrap(), row["det_run"].as_f64().unwrap(), row["det_post"].as_f64().unwrap(),
            row["crop"].as_f64().unwrap(), row["rec_pre"].as_f64().unwrap(), row["rec_run"].as_f64().unwrap(), row["rec_post"].as_f64().unwrap()
        );
        pages.push(row);
    }
    let peak = engine::peak_rss_mib();
    eprintln!("load {load_s:.2} s, peak RSS {peak:.0} MiB, peak virtual {:.0} MiB", engine::peak_vm_mib());
    if let Some(out) = arg(args, "--json") {
        let v = serde_json::json!({"load_s": load_s, "peak_rss_mib": peak, "pages": pages});
        std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}

/// Item 4: 対応付け CER over XFUND ja val. Writes only numbers to stdout;
/// predictions go to `out/` (git-ignored) for inspection.
fn xfund(env: &ort::environment::Environment, args: &[String]) {
    let mut ocr = open_ocr(env, args);
    let dir = root().join("data/xfund");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("ja.val.json")).unwrap()).unwrap();
    let limit: usize = arg(args, "--pages").map_or(usize::MAX, |n| n.parse().unwrap());
    let mut total = cer::Score::default();
    let mut total_chars = cer::Score::default();
    let (mut pq_lines, mut pq_fallbacks) = (0usize, 0usize);
    let mut dump = Vec::new();
    let mut det_ms = Vec::new();
    let mut rec_ms = Vec::new();
    for doc in v["documents"].as_array().unwrap().iter().take(limit) {
        let fname = doc["img"]["fname"].as_str().unwrap();
        let mut img = Bgr::open(&dir.join("val").join(fname)).unwrap();
        // --prescale N: as if the page had been rendered with a long side of N px.
        let mut f = 1.0;
        if let Some(n) = arg(args, "--prescale") {
            let n: f64 = n.parse().unwrap();
            f = n / img.w.max(img.h) as f64;
            img = imgops::resize_linear(&img, (img.w as f64 * f).round() as usize, (img.h as f64 * f).round() as usize);
        }
        let gt: Vec<cer::Boxed> = doc["document"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                let b = e["box"].as_array().unwrap();
                cer::Boxed {
                    text: e["text"].as_str().unwrap_or("").to_string(),
                    bbox: std::array::from_fn(|k| b[k].as_f64().unwrap() * f),
                }
            })
            .collect();
        let mut t = Timings::default();
        let lines = ocr.run(&img, &mut t).unwrap();
        pq_lines += t.parseq_lines;
        pq_fallbacks += t.parseq_fallbacks;
        det_ms.push(t.det().as_secs_f64() * 1000.0);
        rec_ms.push(t.rec().as_secs_f64() * 1000.0);
        let pred: Vec<cer::Boxed> =
            lines.iter().map(|l| cer::Boxed { text: l.text.clone(), bbox: geom::quad_bounds(&l.quad) }).collect();
        let s = cer::matched(&gt, &pred);
        // Characters with their cells, in reading order (lines as PaddleOCR sorts them).
        let pchars: Vec<cer::Boxed> = lines
            .iter()
            .flat_map(|l| {
                let (cells, glyphs) = types::line_chars(l);
                glyphs.into_iter().zip(cells).map(move |(c, (u0, u1))| cer::Boxed {
                    text: c.0.to_string(),
                    bbox: geom::quad_bounds(&types::span_quad(&l.quad, l.vertical, u0, u1)),
                })
            })
            .collect();
        let sc = cer::matched_chars(&gt, &pchars);
        println!(
            "{fname}: CER line-matched {:.2} % ({} / {}), char-matched {:.2} %, lines {}, unassigned lines {}",
            s.cer() * 100.0, s.edits, s.chars, sc.cer() * 100.0, lines.len(), s.unassigned
        );
        total.add(s);
        total_chars.add(sc);
        dump.push(serde_json::json!({"fname": fname, "lines": lines.iter().map(|l| serde_json::json!({
            "quad": l.quad, "text": l.text, "score": l.score, "vertical": l.vertical, "ctc": l.rec.text})).collect::<Vec<_>>()}));
    }
    println!(
        "TOTAL pages {}: CER line-matched {:.2} % ({} / {}), char-matched {:.2} % ({} / {}), unassigned lines {}, median det {:.0} ms rec {:.0} ms, peak RSS {:.0} MiB",
        dump.len(), total.cer() * 100.0, total.edits, total.chars, total_chars.cer() * 100.0, total_chars.edits, total_chars.chars, total.unassigned,
        median(det_ms), median(rec_ms), engine::peak_rss_mib()
    );
    let model = arg(args, "--model").unwrap_or("small");
    std::fs::create_dir_all(root().join("out")).unwrap();
    let tag = if args.iter().any(|a| a == "--parseq") { "_parseq" } else { "" };
    println!("vertical lines sent to PARSeq {pq_lines}, kept PP-OCR's reading by the guards {pq_fallbacks}");
    std::fs::write(root().join(format!("out/xfund_pred_{model}{tag}.json")), serde_json::to_string(&dump).unwrap()).unwrap();
}

fn ordered_text(lines: &[engine::Line], blocks: &[order::Block]) -> String {
    blocks.iter().flat_map(|b| b.lines.iter().map(|&i| lines[i].text.as_str())).collect()
}

fn items(lines: &[engine::Line]) -> Vec<order::Item> {
    lines.iter().map(|l| order::Item { bbox: geom::quad_bounds(&l.quad) }).collect()
}

/// Item 5: page full-text CER on synthetic vertical pages before and after
/// reordering, and the same check on XFUND (horizontal forms).
fn vertical_pages(env: &ort::environment::Environment, args: &[String]) {
    let mut ocr = open_ocr(env, args);
    let font_bytes = std::fs::read(root().join("data/fonts/IBMPlexSansJP-Regular.otf")).unwrap();
    let ts = vertical::Typesetter::new(&font_bytes, &root().join("data/fonts/plex_vert.json"));
    let dir = root().join("data/vertical");
    std::fs::create_dir_all(&dir).unwrap();
    let (mut before, mut after, mut matched) = (cer::Score::default(), cer::Score::default(), cer::Score::default());
    for (name, page, gt) in vertical::pages(&ts) {
        let path = dir.join(format!("{name}.png"));
        page.save(&path).unwrap();
        std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&gt).unwrap()).unwrap();
        let img = Bgr::open(&path).unwrap();
        let lines = ocr.run(&img, &mut Timings::default()).unwrap();
        let gt_text: String = gt.iter().flat_map(|b| b.lines.iter().map(String::as_str)).collect();
        let paddle: String = lines.iter().map(|l| l.text.as_str()).collect();
        let blocks = order::reading_order(&items(&lines));
        let ours = ordered_text(&lines, &blocks);
        let (b, a) = (cer::full_text(&gt_text, &paddle), cer::full_text(&gt_text, &ours));
        let gt_lines: Vec<cer::Boxed> = gt
            .iter()
            .flat_map(|b| b.lines.iter().zip(&b.line_boxes).map(|(t, bb)| cer::Boxed { text: t.clone(), bbox: *bb }))
            .collect();
        let pred: Vec<cer::Boxed> =
            lines.iter().map(|l| cer::Boxed { text: l.text.clone(), bbox: geom::quad_bounds(&l.quad) }).collect();
        let m = cer::matched(&gt_lines, &pred);
        let vertical_lines = lines.iter().filter(|l| l.vertical).count();
        println!(
            "{name}: lines {} (vertical {}), blocks found {} (truth {}), full-text CER PaddleOCR order {:.2} % -> reordered {:.2} %, matched CER {:.2} %",
            lines.len(), vertical_lines, blocks.len(), gt.len(), b.cer() * 100.0, a.cer() * 100.0, m.cer() * 100.0
        );
        if args.iter().any(|a| a == "--show") {
            for (k, bl) in blocks.iter().enumerate() {
                println!("  block {k} {}: {}", if bl.vertical { "V" } else { "H" }, bl.lines.iter().map(|&i| lines[i].text.as_str()).collect::<Vec<_>>().join(" | "));
            }
        }
        before.add(b);
        after.add(a);
        matched.add(m);
    }
    println!(
        "TOTAL vertical: full-text CER PaddleOCR order {:.2} % -> reordered {:.2} %, matched CER {:.2} %",
        before.cer() * 100.0, after.cer() * 100.0, matched.cer() * 100.0
    );

    // XFUND: the order must not get worse on horizontal forms. The ground
    // truth order is the annotation order of ja.val.json.
    let xdir = root().join("data/xfund");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(xdir.join("ja.val.json")).unwrap()).unwrap();
    let limit: usize = arg(args, "--pages").map_or(usize::MAX, |n| n.parse().unwrap());
    let (mut xb, mut xa) = (cer::Score::default(), cer::Score::default());
    for doc in v["documents"].as_array().unwrap().iter().take(limit) {
        let img = Bgr::open(&xdir.join("val").join(doc["img"]["fname"].as_str().unwrap())).unwrap();
        let lines = ocr.run(&img, &mut Timings::default()).unwrap();
        let gt_text: String = doc["document"].as_array().unwrap().iter().map(|e| e["text"].as_str().unwrap_or("")).collect();
        let paddle: String = lines.iter().map(|l| l.text.as_str()).collect();
        let ours = ordered_text(&lines, &order::reading_order(&items(&lines)));
        xb.add(cer::full_text(&gt_text, &paddle));
        xa.add(cer::full_text(&gt_text, &ours));
    }
    println!(
        "TOTAL XFUND val: full-text CER PaddleOCR order {:.2} % -> reordered {:.2} %",
        xb.cer() * 100.0, xa.cer() * 100.0
    );
}

fn rect_iou(a: [f64; 4], b: [f64; 4]) -> f64 {
    geom::iou(&a, &b)
}

/// Item 6: how well the per-character cells from the CTC steps match
/// XFUND's per-character boxes, against splitting the line evenly.
fn chars(env: &ort::environment::Environment, args: &[String]) {
    let mut ocr = open_ocr(env, args);
    let dir = root().join("data/xfund");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("ja.val.json")).unwrap()).unwrap();
    let limit: usize = arg(args, "--pages").map_or(usize::MAX, |n| n.parse().unwrap());
    let (mut n, mut lines_used) = (0usize, 0usize);
    let (mut iou_ctc, mut iou_even, mut hit_ctc, mut hit_even) = (0.0, 0.0, 0usize, 0usize);
    for doc in v["documents"].as_array().unwrap().iter().take(limit) {
        let img = Bgr::open(&dir.join("val").join(doc["img"]["fname"].as_str().unwrap())).unwrap();
        let lines = ocr.run(&img, &mut Timings::default()).unwrap();
        let page = types::build_page(&lines, img.w, img.h, (img.w as f64, img.h as f64), "x");
        let ents = doc["document"].as_array().unwrap();
        for line in page.blocks.iter().flat_map(|b| &b.lines) {
            if line.orientation != types::Orientation::Horizontal {
                continue;
            }
            let lb = [line.bbox.x0, line.bbox.y0, line.bbox.x1, line.bbox.y1];
            let pred_chars: Vec<&types::OcrChar> = line.words.iter().flat_map(|w| &w.chars).collect();
            let pred_text: String = pred_chars.iter().map(|c| c.text.as_str()).collect();
            // The entity whose text is exactly this line (spaces aside) and that overlaps it.
            let Some(e) = ents.iter().find(|e| {
                let b = e["box"].as_array().unwrap();
                let eb: [f64; 4] = std::array::from_fn(|k| b[k].as_f64().unwrap());
                let words = e["words"].as_array().unwrap();
                let gt: String = words.iter().map(|w| w["text"].as_str().unwrap()).collect();
                rect_iou(eb, lb) > 0.3 && gt == pred_text && words.len() == pred_chars.len()
            }) else { continue };
            lines_used += 1;
            let words = e["words"].as_array().unwrap();
            let k = pred_chars.len() as f64;
            for (i, (pc, w)) in pred_chars.iter().zip(words).enumerate() {
                let b = w["box"].as_array().unwrap();
                let gb: [f64; 4] = std::array::from_fn(|k| b[k].as_f64().unwrap());
                let cb = geom::quad_bounds(&pc.quad);
                let even = [lb[0] + (lb[2] - lb[0]) * i as f64 / k, lb[1], lb[0] + (lb[2] - lb[0]) * (i + 1) as f64 / k, lb[3]];
                iou_ctc += rect_iou(cb, gb);
                iou_even += rect_iou(even, gb);
                let inside = |r: [f64; 4]| {
                    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
                    cx >= gb[0] && cx <= gb[2] && cy >= gb[1] && cy <= gb[3]
                };
                hit_ctc += usize::from(inside(cb));
                hit_even += usize::from(inside(even));
                n += 1;
            }
        }
    }
    println!(
        "lines compared {lines_used}, characters {n}: mean IoU CTC {:.3} vs even split {:.3}; centre inside the true box CTC {:.1} % vs even {:.1} %",
        iou_ctc / n as f64, iou_even / n as f64, hit_ctc as f64 * 100.0 / n as f64, hit_even as f64 * 100.0 / n as f64
    );
}

/// Item 7: a synthetic vertical page as an image-only A4 PDF, rendered with
/// pdfium at a 1600 px long side, then OCR; compared with OCR of the PNG.
fn pdf_page(env: &ort::environment::Environment, args: &[String]) {
    let mut ocr = open_ocr(env, args);
    let name = arg(args, "--page").unwrap_or("v3_three_tiers");
    let dir = root().join("data/vertical");
    let png = image::open(dir.join(format!("{name}.png"))).unwrap().to_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&png).unwrap();
    let doc = pdf::image_pdf(&jpeg, png.width(), png.height(), (595.28, 841.89));
    std::fs::write(root().join(format!("out/{name}.pdf")), &doc).unwrap();
    let lib = root().join("../src-tauri/pdfium/linux/lib/libpdfium.so");
    let s = std::time::Instant::now();
    let (img, page_pt) = pdf::render_first_page(&lib, &doc, 1600);
    let render_ms = s.elapsed().as_secs_f64() * 1000.0;
    let gt: Vec<vertical::GtBlock> = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap())
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|b| vertical::GtBlock {
            vertical: b["vertical"].as_bool().unwrap(),
            lines: b["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect(),
            line_boxes: Vec::new(),
            bbox: [0.0; 4],
        })
        .collect();
    let gt_text: String = gt.iter().flat_map(|b| b.lines.iter().map(String::as_str)).collect();
    for (label, im) in [("PNG 1654x2339", Bgr::from_rgb(&png)), ("PDF rendered by pdfium", img)] {
        let mut t = Timings::default();
        let lines = ocr.run(&im, &mut t).unwrap();
        let page = types::build_page(&lines, im.w, im.h, page_pt, "PP-OCRv6_small");
        let text: String = page.blocks.iter().flat_map(|b| b.lines.iter().map(|l| l.text.as_str())).collect();
        println!(
            "{name} {label} ({}x{}): lines {}, blocks {}, reordered full-text CER {:.2} %, det {:.0} ms rec {:.0} ms",
            im.w, im.h, lines.len(), page.blocks.len(), cer::full_text(&gt_text, &text).cer() * 100.0,
            t.det().as_secs_f64() * 1000.0, t.rec().as_secs_f64() * 1000.0
        );
    }
    println!("render {render_ms:.0} ms (pdfium load included), peak RSS {:.0} MiB", engine::peak_rss_mib());
}

/// Item 8 (4): character positions of PARSeq's vertical lines against the
/// exact cells of the synthetic pages (each character is a font-size square).
fn vchars(env: &ort::environment::Environment, args: &[String]) {
    let mut ocr = open_ocr(env, args);
    let dir = root().join("data/vertical");
    let (mut n, mut hit, mut iou_sum) = (0usize, 0usize, 0.0f64);
    for name in ["v1_title_body", "v2_two_tiers", "v3_three_tiers", "v4_dialogue"] {
        let gt: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        // (column text, column box) for the vertical ground truth.
        let cols: Vec<(Vec<char>, [f64; 4])> = gt
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["vertical"].as_bool().unwrap())
            .flat_map(|b| {
                b["lines"].as_array().unwrap().iter().zip(b["line_boxes"].as_array().unwrap()).map(|(t, bb)| {
                    let bb = bb.as_array().unwrap();
                    (t.as_str().unwrap().chars().collect(), std::array::from_fn(|k| bb[k].as_f64().unwrap()))
                })
            })
            .collect();
        let img = Bgr::open(&dir.join(format!("{name}.png"))).unwrap();
        let lines = ocr.run(&img, &mut Timings::default()).unwrap();
        for l in lines.iter().filter(|l| l.parseq.is_some()) {
            let lb = geom::quad_bounds(&l.quad);
            let Some((gchars, gb)) = cols.iter().max_by(|a, b| geom::iou(&a.1, &lb).total_cmp(&geom::iou(&b.1, &lb))) else { continue };
            if geom::iou(gb, &lb) < 0.3 {
                continue;
            }
            let size = gb[2] - gb[0];
            let (cells, glyphs) = types::line_chars(l);
            // Pair predicted and true characters along the edit path.
            let src: Vec<(char, (f64, f64))> = gchars.iter().enumerate().map(|(k, &c)| (c, (k as f64, k as f64 + 1.0))).collect();
            let text: String = glyphs.iter().map(|g| g.0).collect();
            let paired = ocr_spike::parseq::aligned_cells(&text, &src);
            for (k, (g, &(u0, u1))) in glyphs.iter().zip(&cells).enumerate() {
                let (a, b) = paired[k];
                // aligned_cells returns the true index range for paired characters.
                if (b - a - 1.0).abs() > 1e-9 || a.fract() != 0.0 || gchars.get(a as usize) != Some(&g.0) {
                    continue;
                }
                let truth = [gb[0], gb[1] + a * size, gb[2], gb[1] + (a + 1.0) * size];
                let cb = geom::quad_bounds(&types::span_quad(&l.quad, true, u0, u1));
                let (cx, cy) = ((cb[0] + cb[2]) / 2.0, (cb[1] + cb[3]) / 2.0);
                hit += usize::from(cx >= truth[0] && cx <= truth[2] && cy >= truth[1] && cy <= truth[3]);
                // IoU along the column only (the line box is wider than the glyph cell).
                let inter = (cb[3].min(truth[3]) - cb[1].max(truth[1])).max(0.0);
                let uni = (cb[3].max(truth[3]) - cb[1].min(truth[1])).max(1e-9);
                iou_sum += inter / uni;
                n += 1;
            }
        }
    }
    println!(
        "cells {:?}: characters compared {n}, centre inside the true cell {:.1} %, mean 1-D IoU along the column {:.3}",
        ocr.cell_mode, hit as f64 * 100.0 / n as f64, iou_sum / n as f64
    );
}
