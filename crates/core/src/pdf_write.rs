//! PDF document creation and page writing using krilla (design §4.3).

use image::metadata::Orientation;
use krilla::Document;
use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::page::PageSettings;

use crate::error::{PdfWriteError, ProbeError};
use crate::layout::{PageSize, calculate_layout, orientation_swaps_dimensions};
use crate::probe::{ImageFormat, probe_reader};

/// Reads and verifies pixel data using `image` without decoding limits (design §4.3).
///
/// Total pixel count was already verified against [`crate::MAX_IMAGE_PIXELS`] by `probe_reader`.
/// We disable the image crate's default 512 MiB allocation limit so that valid large
/// 16-bit RGBA PNGs (up to 80M pixels * 8 bytes = 640 MiB) are not rejected.
pub(crate) fn decode_image_pixels(image_bytes: &[u8]) -> Result<image::DynamicImage, ProbeError> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|_| ProbeError::DecodeFailed)?;
    reader.no_limits();
    reader.decode().map_err(|_| ProbeError::DecodeFailed)
}

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
        let info = probe_reader(std::io::Cursor::new(image_bytes))?;
        let dynamic_img = decode_image_pixels(image_bytes)?;

        // JPEG, PNG, and WebP are embedded with original bytes unchanged (design §4.3).
        // BMP uses the RGBA pixels read by `image`.
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

        let layout = calculate_layout(&info, page_size);

        let (iw, ih) = (info.width as f32, info.height as f32);
        let swaps = orientation_swaps_dimensions(info.orientation);
        let (disp_w, disp_h) = if swaps { (ih, iw) } else { (iw, ih) };

        // Orientation transform (from spike/write-bench) maps unoriented [0, iw] x [0, ih] pixels
        // to oriented display pixels [0, disp_w] x [0, disp_h].
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

        // Validate page and image dimensions before starting the page to avoid leaving blank pages.
        let page_settings = PageSettings::from_wh(layout.page_width, layout.page_height)
            .ok_or(ProbeError::DecodeFailed)?;
        let size = Size::from_wh(iw, ih).ok_or(ProbeError::DecodeFailed)?;

        let mut page = self.doc.start_page_with(page_settings);
        let mut surface = page.surface();

        // Push placement first, then orientation, mapping image pixels onto the target rect.
        surface.push_transform(&placement_transform);
        surface.push_transform(&orientation_transform);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_16bit_png_within_pixel_limit_exceeds_default_alloc_limit() {
        // 8000x8500 = 68,000,000 pixels (within MAX_IMAGE_PIXELS of 80,000,000).
        // At 16-bit RGBA (8 bytes/pixel), required buffer is 544 MiB, exceeding Limits::default() (512 MiB).
        // This is a minimal 66-byte PNG structure with IHDR, IDAT, and IEND.
        let png_bytes = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x1f, 0x40, 0x00, 0x00, 0x21, 0x34, 0x10, 0x06, 0x00, 0x00,
            0x00, 0x3e, 0x8e, 0xe0, 0x6f, 0x00, 0x00, 0x00, 0x09, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x5e, 0xff, 0x7d, 0xf9, 0x00, 0x00,
            0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];

        // The default image reader fails with Limits error because 544 MiB exceeds 512 MiB.
        let default_reader = image::ImageReader::new(std::io::Cursor::new(&png_bytes))
            .with_guessed_format()
            .expect("should guess PNG format");
        let default_err = default_reader
            .decode()
            .expect_err("default reader should fail on 544 MiB allocation");
        assert!(
            matches!(default_err, image::ImageError::Limits(_)),
            "default reader must fail with Limits error, got: {default_err:?}"
        );

        // decode_image_pixels disables limits, bypassing the 512 MiB check.
        // It fails with normal decoding error (incomplete scanlines) rather than Limits.
        let mut no_limits_reader = image::ImageReader::new(std::io::Cursor::new(&png_bytes))
            .with_guessed_format()
            .expect("should guess PNG format");
        no_limits_reader.no_limits();
        let no_limits_err = no_limits_reader
            .decode()
            .expect_err("incomplete scanlines should fail decoding");
        assert!(
            !matches!(no_limits_err, image::ImageError::Limits(_)),
            "reader with no_limits must not fail with Limits error, got: {no_limits_err:?}"
        );

        // decode_image_pixels internally uses no_limits(), returning DecodeFailed
        // from incomplete scanlines rather than failing on allocation limits.
        let decode_res = decode_image_pixels(&png_bytes);
        assert_eq!(decode_res.unwrap_err(), ProbeError::DecodeFailed);
    }
}
