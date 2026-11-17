//! Reads every page of a PDF by OCR through a real worker, to judge the
//! output without a screen (design ocr §12). For development only; the app
//! does not include it.
//!
//! Usage, from the repository root after `npm run pdfium:fetch` and
//! `npm run layout:fetch`:
//!
//! ```text
//! cargo build -p pdfconv-worker --release --bin pdfconv-worker
//! cargo run -p pdfconv-worker --release --example ocr_pdf -- <input.pdf> [--json <folder>]
//! ```
//!
//! The page's text goes to standard output; writing the PDF with the text
//! layer comes with the text layer itself (design ocr §1, stage 2). With
//! `--json`, each page's
//! `OcrPage` is written to `<folder>/page-<n>.json`, to look at the reading
//! order and the confidences. The worker is `pdfconv-worker` of the same
//! build, started with the pdfium, onnxruntime and models that the fetch
//! scripts put under `src-tauri/`, so it reads within `WORKER_MEMORY_LIMIT`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use pdfconv_core::ocr::OcrPage;
use pdfconv_worker::client::{OCR_TIMEOUT, OPEN_TIMEOUT, WorkerProcess};
use pdfconv_worker::protocol::{Request, Response};

fn src_tauri() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri")
}

fn worker_args() -> [PathBuf; 4] {
    let (pdfium, onnxruntime) = if cfg!(windows) {
        ("pdfium/windows/bin", "onnxruntime/windows/onnxruntime.dll")
    } else {
        ("pdfium/linux/lib", "onnxruntime/linux/libonnxruntime.so")
    };
    let dir = src_tauri();
    [
        dir.join(pdfium),
        dir.join(onnxruntime),
        dir.join("layout-model/PP-DocLayout_plus-L-int8.onnx"),
        dir.join("ocr-model"),
    ]
}

/// `pdfconv-worker` of the build this example belongs to:
/// `target/<profile>/examples/ocr_pdf` sits beside `target/<profile>/`.
fn worker_program() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = format!("pdfconv-worker{}", std::env::consts::EXE_SUFFIX);
    let program = exe
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join(&name))
        .ok_or("the example's folder has no parent")?;
    if program.exists() {
        Ok(program)
    } else {
        Err(format!(
            "{} is missing; build it first with cargo build -p pdfconv-worker --bin pdfconv-worker \
             in the same profile",
            program.display()
        ))
    }
}

struct Args {
    input: PathBuf,
    json_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args_os().skip(1);
    let mut input = None;
    let mut json_dir = None;
    while let Some(arg) = args.next() {
        if arg == "--json" {
            json_dir = Some(PathBuf::from(args.next().ok_or("--json needs a folder")?));
        } else if input.is_none() {
            input = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument {}", arg.to_string_lossy()));
        }
    }
    Ok(Args {
        input: input.ok_or("usage: ocr_pdf <input.pdf> [--json <folder>]")?,
        json_dir,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut worker = WorkerProcess::spawn(worker_program()?.as_os_str(), worker_args())
        .map_err(|e| format!("starting the worker: {e:?}"))?;
    let page_count = match worker.request(
        &Request::Open {
            path: args.input.clone(),
            password: None,
        },
        OPEN_TIMEOUT,
    ) {
        Ok((Response::Open { page_count, .. }, _)) => page_count,
        other => return Err(format!("opening {}: {other:?}", args.input.display())),
    };
    if let Some(dir) = &args.json_dir {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    for page in 1..=page_count {
        let started = Instant::now();
        let body = match worker.request(&Request::Ocr { page }, OCR_TIMEOUT) {
            Ok((Response::Ocr, body)) => body,
            other => return Err(format!("page {page}: {other:?}")),
        };
        let result: OcrPage =
            serde_json::from_slice(&body).map_err(|e| format!("page {page}: {e}"))?;
        let lines: usize = result.blocks.iter().map(|b| b.lines.len()).sum();
        eprintln!(
            "page {page}: {} blocks, {lines} lines, {:.2} s, {} KB of JSON",
            result.blocks.len(),
            started.elapsed().as_secs_f64(),
            body.len() / 1024
        );
        print!("{}", result.text());
        println!("\u{c}");
        if let Some(dir) = &args.json_dir {
            let path = dir.join(format!("page-{page}.json"));
            let pretty = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
            std::fs::write(&path, pretty).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ocr_pdf: {message}");
            ExitCode::FAILURE
        }
    }
}
