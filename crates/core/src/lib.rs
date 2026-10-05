//! Conversion logic that does not depend on Tauri: image probing, page layout,
//! PDF writing, page ranges and output names (design §4, §6.4).

pub mod error;
pub mod layout;
pub mod naming;
pub mod page_range;
pub mod pdf_write;
pub mod probe;

pub use error::{PdfWriteError, ProbeError};
pub use layout::{
    A4_HEIGHT_PT, A4_MARGIN_MM, A4_WIDTH_PT, MAX_PAGE_SIDE_PT, MM_PER_INCH, POINTS_PER_INCH,
    PageLayout, PageSize, Rect, calculate_layout, orientation_swaps_dimensions,
};
pub use naming::{
    PdfToImageInput, page_number_width, resolve_image_to_pdf_names, resolve_pdf_to_image_names,
};
pub use page_range::{PageSet, ParsePageRangeError, parse_page_range};
pub use pdf_write::PdfWriter;
pub use probe::{
    DEFAULT_DPI, IMAGE_EXTENSIONS, ImageFormat, ImageInfo, MAX_DPI, MAX_IMAGE_PIXELS, MIN_DPI,
    probe, probe_reader,
};
