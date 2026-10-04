//! The PDF worker process: the only crate that loads pdfium (design §5).
//!
//! The app starts its own executable with [`WORKER_FLAG`] to run
//! [`server::run`]; [`client::WorkerProcess`] is the main-process side.

pub mod client;
pub mod protocol;
pub mod server;
#[cfg(windows)]
mod windows_job;

/// The first command-line argument that makes the app executable run as a
/// worker; the second is the folder holding the pdfium library.
pub const WORKER_FLAG: &str = "--pdf-worker";
