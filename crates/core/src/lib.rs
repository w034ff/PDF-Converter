//! Conversion logic that does not depend on Tauri: image probing, page layout,
//! PDF writing, thumbnails, page ranges and output names (design §4, §6.1, §6.4).

pub mod error;
mod jpeg_strip;
pub mod layout;
pub mod naming;
pub mod page_range;
pub mod pdf_write;
pub mod probe;
pub mod thumbnail;
pub mod tiff;
mod utc_time;

pub use error::{MAX_TIFF_PAGES, PdfWriteError, ProbeError};
pub use jpeg_strip::strip_jpeg_metadata;
pub use layout::{
    MAX_PAGE_SIDE_PT, MM_PER_INCH, PAGE_MARGIN_MM, POINTS_PER_INCH, PageLayout, PageOrientation,
    PageSize, PaperSize, Rect, calculate_layout, orientation_swaps_dimensions,
};
pub use naming::{
    PdfToImageInput, page_number_width, resolve_image_to_pdf_names, resolve_pdf_to_image_names,
};
pub use page_range::{PageSet, ParsePageRangeError, parse_page_range};
pub use pdf_write::{AddImageResult, PdfConformance, PdfWriter};
pub use probe::{
    DEFAULT_DPI, IMAGE_EXTENSIONS, ImageFormat, ImageInfo, MAX_DPI, MAX_IMAGE_PIXELS, MIN_DPI,
    probe, probe_reader,
};
pub use thumbnail::{THUMBNAIL_SIDE, thumbnail_png};
