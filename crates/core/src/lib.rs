//! Conversion logic that does not depend on Tauri: image probing, page layout,
//! PDF writing, thumbnails, page ranges, crop regions and output names (design §4, §6.1, §6.4).

pub mod error;
mod jpeg_strip;
pub mod layout;
pub mod naming;
pub mod organize;
pub mod page_range;
pub mod pdf_encrypt;
pub mod pdf_write;
pub mod probe;
pub mod region;
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
    CropRegion, PdfToImageInput, page_number_width, resolve_crop_names, resolve_image_to_pdf_names,
    resolve_pdf_to_image_names,
};
pub use organize::{
    DestinationView, LINK_RECT_TOLERANCE_PT, LinkStyle, LinkStyleSource, LinkTarget, OutlineItem,
    OutputLink, RAW_MAX_DECOMPRESSED, Relinking, SourceDetails, SourceLinkKey, ViewSettings,
    finish_organized, read_source_details,
};
pub use page_range::{PageSet, ParsePageRangeError, parse_page_range};
pub use pdf_encrypt::{
    MAX_OUTPUT_PASSWORD_LEN, OUTPUT_PASSWORD_CHARS, OutputPassword, OutputPasswordError,
    encrypt_pdf,
};
pub use pdf_write::{AddImageResult, PdfConformance, PdfWriter};
pub use probe::{
    DEFAULT_DPI, IMAGE_EXTENSIONS, ImageFormat, ImageInfo, MAX_DPI, MAX_IMAGE_PIXELS, MIN_DPI,
    probe, probe_reader,
};
pub use region::{PHOTO_MATCH_IOU, PageRect, iou, position_order};
pub use thumbnail::{THUMBNAIL_SIDE, thumbnail_png};
