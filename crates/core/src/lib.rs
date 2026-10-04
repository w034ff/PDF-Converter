//! Conversion logic that does not depend on Tauri: image probing, page layout,
//! PDF writing, page ranges and output names (design §4, §6.4).

pub mod naming;
pub mod page_range;

pub use naming::{
    PdfPageItem, PdfToImageInput, format_pdf_page_name, next_available_name, page_number_width,
    resolve_image_to_pdf_names, resolve_pdf_page_output_names, resolve_pdf_to_image_names,
};
pub use page_range::{PageSet, ParsePageRangeError, parse_page_range};
