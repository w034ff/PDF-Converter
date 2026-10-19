//! A standalone worker, used by the tests to start a worker without the app.
//! Usage: pdfconv-worker <pdfium library folder> <onnxruntime library file> <layout model file>

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(library_dir), Some(onnxruntime), Some(model)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!(
            "usage: pdfconv-worker <pdfium library folder> <onnxruntime library file> <layout model file>"
        );
        return ExitCode::FAILURE;
    };
    match pdfconv_worker::server::run(&library_dir, &onnxruntime, &model) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("worker failed: {e}");
            ExitCode::FAILURE
        }
    }
}
