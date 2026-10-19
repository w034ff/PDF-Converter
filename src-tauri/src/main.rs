// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // The app starts itself with this flag to get a worker process
    // (design §5.4), so the worker must be handled before Tauri starts.
    let mut args = std::env::args_os().skip(1);
    if args
        .next()
        .is_some_and(|flag| flag == pdfconv_worker::WORKER_FLAG)
    {
        let mut paths = args.map(PathBuf::from);
        let (Some(library_dir), Some(onnxruntime), Some(model)) =
            (paths.next(), paths.next(), paths.next())
        else {
            return ExitCode::FAILURE;
        };
        return match pdfconv_worker::server::run(&library_dir, &onnxruntime, &model) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }
    pdf_converter_lib::run();
    ExitCode::SUCCESS
}
