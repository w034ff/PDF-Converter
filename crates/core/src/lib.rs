//! Conversion logic that does not depend on Tauri: image probing, page layout,
//! PDF writing, page ranges and output names (design §4, §6.4).

pub mod error;
pub mod naming;
pub mod page_range;
pub mod probe;

pub use error::ProbeError;
pub use naming::{
    PdfToImageInput, page_number_width, resolve_image_to_pdf_names, resolve_pdf_to_image_names,
};
pub use page_range::{PageSet, ParsePageRangeError, parse_page_range};
pub use probe::{
    DEFAULT_DPI, IMAGE_EXTENSIONS, ImageFormat, ImageInfo, MAX_DPI, MAX_IMAGE_PIXELS, MIN_DPI,
    parse_bmp_dpi, parse_exif_dpi, parse_jfif_dpi, parse_png_dpi, probe, probe_reader,
};
