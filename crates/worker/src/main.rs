//! A standalone worker, used by the tests to start a worker without the app.
//! Usage: pdfconv-worker <pdfium library folder>

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(library_dir) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: pdfconv-worker <pdfium library folder>");
        return ExitCode::FAILURE;
    };
    match pdfconv_worker::server::run(&library_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("worker failed: {e}");
            ExitCode::FAILURE
        }
    }
}
