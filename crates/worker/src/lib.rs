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

/// Maximum number of pages a PDF can have before being rejected with `TooManyPages` (design §5.1, §6.6).
pub const MAX_PDF_PAGES: u32 = 10_000;

/// Maximum total pixels for a rendered page before being rejected with `RenderTooLarge` (design §4.4, §6.6).
pub const MAX_RENDER_PIXELS: u64 = 100_000_000;

/// Quality for JPEG encoding (design §4.4).
pub const JPEG_QUALITY: u8 = 90;

/// Allowed resolution choices in DPI (design §4.4).
pub const DPI_CHOICES: [u32; 3] = [72, 150, 300];

/// Default render resolution in DPI (design §4.4).
pub const DEFAULT_RENDER_DPI: u32 = 150;

pub use server::check_page_count;
