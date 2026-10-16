//! The PDF worker process: the only crate that loads pdfium (design §5).
//!
//! The app starts its own executable with [`WORKER_FLAG`] to run
//! [`server::run`]; [`client::WorkerProcess`] is the main-process side.

pub mod client;
mod crop;
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

/// The largest `max_side` that `Thumbnail` accepts: the crop tab shows a page
/// at most this many pixels on its longer side (design §5.1).
pub const PAGE_VIEW_MAX_SIDE: u32 = 2400;

/// Quality for JPEG encoding (design §4.4).
pub const JPEG_QUALITY: u8 = 90;

/// Minimum render resolution in DPI (design §4.4).
pub const RENDER_DPI_MIN: u32 = 36;

/// Maximum render resolution in DPI (design §4.4).
pub const RENDER_DPI_MAX: u32 = 1000;

/// Default render resolution in DPI (design §4.4).
pub const DEFAULT_RENDER_DPI: u32 = 150;

/// Returns whether `dpi` falls within the allowed render resolution range (design §4.4).
pub fn is_render_dpi_in_range(dpi: u32) -> bool {
    (RENDER_DPI_MIN..=RENDER_DPI_MAX).contains(&dpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_render_dpi_in_range() {
        assert!(is_render_dpi_in_range(RENDER_DPI_MIN));
        assert!(is_render_dpi_in_range(RENDER_DPI_MAX));
        assert!(is_render_dpi_in_range(DEFAULT_RENDER_DPI));
        assert!(is_render_dpi_in_range(72));
        assert!(is_render_dpi_in_range(300));
        assert!(is_render_dpi_in_range(600));

        assert!(!is_render_dpi_in_range(0));
        assert!(!is_render_dpi_in_range(RENDER_DPI_MIN - 1));
        assert!(!is_render_dpi_in_range(RENDER_DPI_MAX + 1));
    }
}
