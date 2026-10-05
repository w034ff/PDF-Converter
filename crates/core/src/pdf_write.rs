//! PDF document creation and page writing using krilla (design §4.3).

use image::metadata::Orientation;
use krilla::Document;
use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::page::PageSettings;

use crate::error::{PdfWriteError, ProbeError};
use crate::layout::{PageSize, calculate_layout};
use crate::probe::{ImageFormat, probe_reader};

/// Writes image pages into a PDF document using `krilla` (design §4.3).
pub struct PdfWriter {
    doc: Document,
}

impl PdfWriter {
    /// Creates a new PDF document writer with the specified creator string.
    ///
    /// The document metadata will only include `/Creator`, leaving title, author,
    /// subject, and keywords unset (design §4.3).
    #[must_use]
    pub fn new(creator: &str) -> Self {
        let mut doc = Document::new();
        doc.set_metadata(Metadata::new().creator(creator.to_string()));
        Self { doc }
    }

    /// Adds a single image as a new page in the PDF (design §4.2, §4.3).
    ///
    /// Decodes pixel data first to verify integrity before mutating the document.
    /// If decoding fails, returns [`ProbeError::DecodeFailed`] and does not add a page,
    /// allowing subsequent calls to continue building the document.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeError::UnsupportedFormat`] if the image format is not supported.
    /// Returns [`ProbeError::TooLarge`] if pixel count exceeds [`crate::MAX_IMAGE_PIXELS`].
    /// Returns [`ProbeError::DecodeFailed`] if headers or pixel data are corrupted or truncated.
    pub fn add_page(&mut self, image_bytes: &[u8], page_size: PageSize) -> Result<(), ProbeError> {
        // 1. Probe image header for format, dimensions, orientation and DPI.
        let info = probe_reader(std::io::Cursor::new(image_bytes))?;

        // 2. Read pixels with `image` to verify data integrity before modifying the document.
        // corrupt.png passes probe but fails here (design §4.3).
        let dynamic_img = image::ImageReader::new(std::io::Cursor::new(image_bytes))
            .with_guessed_format()
            .map_err(|_| ProbeError::DecodeFailed)?
            .decode()
            .map_err(|_| ProbeError::DecodeFailed)?;

        // Construct the krilla Image. JPEG, PNG, and WebP are embedded with their original bytes
        // intact (design §4.3). BMP uses the RGBA pixels read by `image`.
        let img = match info.format {
            ImageFormat::Jpeg => Image::from_jpeg(image_bytes.to_vec().into(), true)
                .map_err(|_| ProbeError::DecodeFailed)?,
            ImageFormat::Png => Image::from_png(image_bytes.to_vec().into(), true)
                .map_err(|_| ProbeError::DecodeFailed)?,
            ImageFormat::WebP => Image::from_webp(image_bytes.to_vec().into(), true)
                .map_err(|_| ProbeError::DecodeFailed)?,
            ImageFormat::Bmp => {
                let rgba = dynamic_img.to_rgba8();
                Image::from_rgba8(rgba.into_raw(), info.width, info.height)
            }
        };

        // 3. Compute page dimensions and image placement rectangle (pure function).
        let layout = calculate_layout(&info, page_size);

        // 4. Set up transforms:
        // Orientation transform (from spike/write-bench) maps unoriented [0, iw] x [0, ih] pixels
        // to oriented display pixels [0, disp_w] x [0, disp_h].
        let (iw, ih) = (info.width as f32, info.height as f32);
        let swaps = matches!(
            info.orientation,
            Orientation::Rotate90
                | Orientation::Rotate270
                | Orientation::Rotate90FlipH
                | Orientation::Rotate270FlipH
        );
        let (disp_w, disp_h) = if swaps { (ih, iw) } else { (iw, ih) };

        let orientation_transform = match info.orientation {
            Orientation::NoTransforms => Transform::from_row(1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            Orientation::FlipHorizontal => Transform::from_row(-1.0, 0.0, 0.0, 1.0, iw, 0.0),
            Orientation::Rotate180 => Transform::from_row(-1.0, 0.0, 0.0, -1.0, iw, ih),
            Orientation::FlipVertical => Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, ih),
            Orientation::Rotate90FlipH => Transform::from_row(0.0, 1.0, 1.0, 0.0, 0.0, 0.0),
            Orientation::Rotate90 => Transform::from_row(0.0, 1.0, -1.0, 0.0, ih, 0.0),
            Orientation::Rotate270FlipH => Transform::from_row(0.0, -1.0, -1.0, 0.0, ih, iw),
            Orientation::Rotate270 => Transform::from_row(0.0, -1.0, 1.0, 0.0, 0.0, iw),
        };

        // Placement transform scales and translates display pixels into the page's image_rect.
        let scale_x = layout.image_rect.width / disp_w;
        let scale_y = layout.image_rect.height / disp_h;
        let placement_transform = Transform::from_row(
            scale_x,
            0.0,
            0.0,
            scale_y,
            layout.image_rect.x,
            layout.image_rect.y,
        );

        let page_settings = PageSettings::from_wh(layout.page_width, layout.page_height)
            .ok_or(ProbeError::DecodeFailed)?;
        let mut page = self.doc.start_page_with(page_settings);
        let mut surface = page.surface();

        // Push placement first, then orientation, mapping image pixels onto the target rect.
        surface.push_transform(&placement_transform);
        surface.push_transform(&orientation_transform);
        let size = Size::from_wh(iw, ih).ok_or(ProbeError::DecodeFailed)?;
        surface.draw_image(img, size);
        surface.pop();
        surface.pop();
        surface.finish();
        page.finish();

        Ok(())
    }

    /// Finishes the PDF document and returns the serialized PDF bytes (design §4.3).
    ///
    /// # Errors
    ///
    /// Returns [`PdfWriteError`] if krilla serialization fails.
    pub fn finish(self) -> Result<Vec<u8>, PdfWriteError> {
        self.doc.finish().map_err(|e| PdfWriteError(e.to_string()))
    }
}
