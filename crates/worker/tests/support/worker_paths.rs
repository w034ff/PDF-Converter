//! The worker's arguments in the tests (design §5.4): the pdfium fetched by
//! `npm run pdfium:fetch` and the onnxruntime and model fetched by
//! `npm run layout:fetch` (design §8.1, §8.5). A missing file fails the test
//! that needs it instead of skipping it.

#![allow(
    dead_code,
    reason = "shared across test files where each file uses a subset"
)]

use std::ffi::OsString;
use std::path::PathBuf;

use pdfconv_worker::client::WorkerProcess;

fn src_tauri() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri")
}

pub fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    src_tauri().join("pdfium").join(os_dir)
}

pub fn onnxruntime_file() -> PathBuf {
    let file = if cfg!(windows) {
        "windows/onnxruntime.dll"
    } else {
        "linux/libonnxruntime.so"
    };
    src_tauri().join("onnxruntime").join(file)
}

pub fn model_file() -> PathBuf {
    src_tauri().join("layout-model/PP-DocLayout_plus-L-int8.onnx")
}

/// The three arguments of a worker using the fetched libraries and model.
pub fn worker_args() -> [OsString; 3] {
    [
        library_dir().into(),
        onnxruntime_file().into(),
        model_file().into(),
    ]
}

/// Starts the standalone worker with `args`.
pub fn start_with(args: [OsString; 3]) -> WorkerProcess {
    WorkerProcess::spawn(env!("CARGO_BIN_EXE_pdfconv-worker").as_ref(), args)
        .expect("worker should start")
}

/// Starts the standalone worker with the fetched libraries and model.
pub fn start() -> WorkerProcess {
    start_with(worker_args())
}
