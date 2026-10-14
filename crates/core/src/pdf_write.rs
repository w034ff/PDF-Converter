//! PDF document creation and page writing using krilla (design §4.3).

use image::metadata::Orientation;
use krilla::Document;
use krilla::SerializeSettings;
use krilla::configure::{Archival, ConfigurationBuilder};
use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::metadata::{DateTime, Metadata};
use krilla::page::PageSettings;

use crate::error::{PdfWriteError, ProbeError};
use crate::layout::{PageSize, calculate_layout, orientation_swaps_dimensions};
use crate::probe::{ImageFormat, ImageInfo, probe_reader};
use crate::utc_time::{UtcTime, now_utc};

/// Reads and verifies pixel data using `image` without decoding limits (design §4.3).
///
/// Total pixel count was already verified against [`crate::MAX_IMAGE_PIXELS`] by `probe_reader`.
/// We disable the image crate's default 512 MiB allocation limit so that valid large
/// 16-bit RGBA PNGs (up to 80M pixels * 8 bytes = 640 MiB) are not rejected.
pub(crate) fn decode_image_pixels(image_bytes: &[u8]) -> Result<image::DynamicImage, ProbeError> {
    decode_without_limits(image_bytes).map_err(|_| ProbeError::DecodeFailed)
}

fn decode_without_limits(image_bytes: &[u8]) -> image::ImageResult<image::DynamicImage> {
    let mut reader =
        image::ImageReader::new(std::io::Cursor::new(image_bytes)).with_guessed_format()?;
    reader.no_limits();
    reader.decode()
}

/// The standard a [`PdfWriter`] document is written to (design §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfConformance {
    /// An ordinary PDF.
    Standard,
    /// PDF/A-2b, validated by krilla when the document is finished.
    PdfA2b,
}

/// A UTC time as a krilla date. A zero offset keeps the writer's local time offset, and with it
/// a hint of where they live, out of the PDF.
fn utc_date_time(time: UtcTime) -> DateTime {
    DateTime::new(time.year)
        .month(time.month)
        .day(time.day)
        .hour(time.hour)
        .minute(time.minute)
        .second(time.second)
        .utc_offset_hour(0)
        .utc_offset_minute(0)
}

/// Writes image pages into a PDF document using `krilla` (design §4.3).
pub struct PdfWriter {
    doc: Document,
    conformance: PdfConformance,
}

/// Result of adding an image to a [`PdfWriter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddImageResult {
    /// The image was successfully added to the document.
    Ok {
        /// Number of pages added to the PDF.
        pages_added: u32,
    },
    /// Conversion was cancelled; no pages were added.
    Cancelled,
}

impl PdfWriter {
    /// Creates a new PDF document writer with the specified creator string.
    ///
    /// The document metadata will only include `/Creator`, leaving title, author,
    /// subject, and keywords unset (design §4.3). A PDF/A-2b document also gets the current
    /// time in UTC as its creation date, which krilla requires for PDF/A.
    ///
    /// With [`PdfConformance::PdfA2b`], [`PdfWriter::finish`] fails if the document does
    /// not meet PDF/A-2b, and [`PdfWriter::add_page`] refuses CMYK JPEG images.
    #[must_use]
    pub fn new(creator: &str, conformance: PdfConformance) -> Self {
        let mut metadata = Metadata::new().creator(creator.to_string());
        let mut doc = match conformance {
            PdfConformance::Standard => Document::new(),
            PdfConformance::PdfA2b => {
                let configuration = ConfigurationBuilder::new()
                    .with_archival_validator(Archival::A2_B)
                    .finish()
                    .expect("PDF/A-2b alone has no conflicting version or validator");
                metadata = metadata.creation_date(utc_date_time(now_utc()));
                Document::new_with(SerializeSettings {
                    configuration,
                    ..SerializeSettings::default()
                })
            }
        };
        doc.set_metadata(metadata);
        Self { doc, conformance }
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
    /// Returns [`ProbeError::CmykNotAllowedInPdfA`] if the document is PDF/A-2b and the image is a
    /// CMYK JPEG.
    pub fn add_page(&mut self, image_bytes: &[u8], page_size: PageSize) -> Result<(), ProbeError> {
        match self.add_image(image_bytes, page_size, |_, _| {}, || false)? {
            AddImageResult::Ok { .. } | AddImageResult::Cancelled => Ok(()),
        }
    }

    /// Adds an image (or all pages of a multi-page TIFF) to the PDF document.
    ///
    /// For multi-page TIFF images, all pages are decoded into krilla images first before
    /// committing to the document. If any page fails decoding, no pages are added to the document.
    ///
    /// Invokes `on_page(page_idx, total_pages)` as each page is decoded.
    /// Periodically queries `should_cancel()` and cancels immediately without adding pages if true.
    pub fn add_image<FProg, FCancel>(
        &mut self,
        image_bytes: &[u8],
        page_size: PageSize,
        mut on_page: FProg,
        should_cancel: FCancel,
    ) -> Result<AddImageResult, ProbeError>
    where
        FProg: FnMut(u32, u32),
        FCancel: Fn() -> bool,
    {
        if should_cancel() {
            return Ok(AddImageResult::Cancelled);
        }

        let info = probe_reader(std::io::Cursor::new(image_bytes))?;

        if should_cancel() {
            return Ok(AddImageResult::Cancelled);
        }

        let interpolate = self.conformance == PdfConformance::Standard;

        if info.format == ImageFormat::Tiff {
            let pages = match crate::tiff::decode_tiff_pages(
                &mut std::io::Cursor::new(image_bytes),
                interpolate,
                &mut on_page,
                &should_cancel,
            )? {
                Ok(pages) => pages,
                Err(()) => return Ok(AddImageResult::Cancelled),
            };

            let count = pages.len() as u32;
            for (img, page_info) in pages {
                self.commit_page(img, &page_info, page_size)?;
            }
            return Ok(AddImageResult::Ok { pages_added: count });
        }

        let dynamic_img = decode_image_pixels(image_bytes)?;

        let img = match info.format {
            ImageFormat::Jpeg => {
                if self.conformance == PdfConformance::PdfA2b
                    && crate::jpeg_strip::jpeg_component_count(image_bytes)?
                        == Some(crate::jpeg_strip::CMYK_COMPONENT_COUNT)
                {
                    return Err(ProbeError::CmykNotAllowedInPdfA);
                }
                let stripped = crate::jpeg_strip::strip_jpeg_metadata(image_bytes)?;
                Image::from_jpeg(stripped.into(), interpolate)
                    .map_err(|_| ProbeError::DecodeFailed)?
            }
            ImageFormat::Png => Image::from_png(image_bytes.to_vec().into(), interpolate)
                .map_err(|_| ProbeError::DecodeFailed)?,
            ImageFormat::WebP => Image::from_webp(image_bytes.to_vec().into(), interpolate)
                .map_err(|_| ProbeError::DecodeFailed)?,
            ImageFormat::Bmp => {
                let rgba = dynamic_img.to_rgba8();
                Image::from_rgba8(rgba.into_raw(), info.width, info.height)
            }
            ImageFormat::Tiff => unreachable!(),
        };

        self.commit_page(img, &info, page_size)?;
        on_page(0, 1);
        Ok(AddImageResult::Ok { pages_added: 1 })
    }

    fn commit_page(
        &mut self,
        img: Image,
        info: &ImageInfo,
        page_size: PageSize,
    ) -> Result<(), ProbeError> {
        let layout = calculate_layout(info, page_size);

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

        // The scanlines are missing, so decoding still fails, but not on the
        // allocation limit.
        let no_limits_err =
            decode_without_limits(&png_bytes).expect_err("incomplete scanlines should fail");
        assert!(
            !matches!(no_limits_err, image::ImageError::Limits(_)),
            "decoding for a page must not apply the allocation limit, got: {no_limits_err:?}"
        );
        let decode_res = decode_image_pixels(&png_bytes);
        assert_eq!(decode_res.unwrap_err(), ProbeError::DecodeFailed);
    }

    #[test]
    fn exif_gps_jpeg_metadata_is_not_in_generated_pdf() {
        let fixture_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exif_gps.jpg");
        let raw_bytes = std::fs::read(&fixture_path).expect("reading exif_gps.jpg");

        let mut writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
        writer
            .add_page(&raw_bytes, PageSize::Fit)
            .expect("adding exif_gps.jpg page");
        let pdf_bytes = writer.finish().expect("finishing PDF");

        // Verify sentinels and standard metadata markers are absent in raw PDF bytes.
        // Raw bytes search uses the null-terminated JPEG APP1 XMP header (`http://ns.adobe.com/xap/1.0/\0`)
        // because krilla's PDF document metadata (/Creator) unconditionally includes
        // `xmlns:xmp="http://ns.adobe.com/xap/1.0/"` in XML.
        const RAW_FORBIDDEN_MARKERS: &[&[u8]] = &[
            b"PDFCONV-TEST-MODEL",
            b"PDFCONV-TEST-COMMENT",
            b"PDFCONV-TEST-TRAILER",
            b"PDFCONV-TEST-XMP",
            b"PDFCONV-TEST-IPTC",
            b"Exif\0\0",
            b"http://ns.adobe.com/xap/1.0/\0",
            b"Photoshop 3.0",
        ];

        for marker in RAW_FORBIDDEN_MARKERS {
            assert!(
                !pdf_bytes.windows(marker.len()).any(|w| w == *marker),
                "raw PDF unexpectedly contains marker: {:?}",
                String::from_utf8_lossy(marker)
            );
        }

        // Decompress all FlateDecode streams using flate2 and verify markers are absent.
        // Stream data must not contain any metadata marker, including the XMP namespace string.
        const STREAM_FORBIDDEN_MARKERS: &[&[u8]] = &[
            b"PDFCONV-TEST-MODEL",
            b"PDFCONV-TEST-COMMENT",
            b"PDFCONV-TEST-TRAILER",
            b"PDFCONV-TEST-XMP",
            b"PDFCONV-TEST-IPTC",
            b"Exif\0\0",
            b"http://ns.adobe.com/xap/1.0/",
            b"Photoshop 3.0",
        ];

        let streams = extract_flate_streams(&pdf_bytes);
        assert!(
            !streams.is_empty(),
            "PDF should contain at least one content/object stream"
        );
        for stream in &streams {
            for marker in STREAM_FORBIDDEN_MARKERS {
                assert!(
                    !stream.windows(marker.len()).any(|w| w == *marker),
                    "decompressed stream unexpectedly contains marker: {:?}",
                    String::from_utf8_lossy(marker)
                );
            }
        }
    }

    #[test]
    fn unparseable_jpeg_fails_with_decode_failed_and_writer_continues() {
        let fixture_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exif_gps.jpg");
        let raw_bytes = std::fs::read(&fixture_path).expect("reading exif_gps.jpg");

        // Remove EOI marker from the end to create a JPEG whose segment parsing fails
        let eoi_pos = raw_bytes
            .windows(2)
            .rposition(|w| w == [0xFF, 0xD9])
            .expect("finding EOI");
        let truncated_jpeg = &raw_bytes[..eoi_pos];

        let mut writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
        let err = writer
            .add_page(truncated_jpeg, PageSize::Fit)
            .expect_err("truncated JPEG must fail add_page");
        assert_eq!(err, ProbeError::DecodeFailed);

        // Document can continue and accept another valid image
        let valid_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/photo.jpg");
        let valid_bytes = std::fs::read(&valid_path).expect("reading photo.jpg");
        writer
            .add_page(&valid_bytes, PageSize::Fit)
            .expect("adding valid photo.jpg should succeed after failure");

        let pdf = writer.finish().expect("finishing PDF");
        assert!(!pdf.is_empty());
    }

    fn extract_flate_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
        let mut streams = Vec::new();
        let mut pos = 0;
        while let Some(idx) = pdf[pos..].windows(6).position(|w| w == b"stream") {
            let stream_start = pos + idx + 6;
            let mut data_start = stream_start;
            if data_start < pdf.len() && pdf[data_start] == b'\r' {
                data_start += 1;
            }
            if data_start < pdf.len() && pdf[data_start] == b'\n' {
                data_start += 1;
            }
            if let Some(end_idx) = pdf[data_start..].windows(9).position(|w| w == b"endstream") {
                let mut data_end = data_start + end_idx;
                if data_end > data_start && pdf[data_end - 1] == b'\n' {
                    data_end -= 1;
                }
                if data_end > data_start && pdf[data_end - 1] == b'\r' {
                    data_end -= 1;
                }
                let raw = &pdf[data_start..data_end];
                let mut decoder = flate2::read::ZlibDecoder::new(raw);
                let mut decompressed = Vec::new();
                if std::io::Read::read_to_end(&mut decoder, &mut decompressed).is_ok()
                    && !decompressed.is_empty()
                {
                    streams.push(decompressed);
                }
                pos = data_start + end_idx + 9;
            } else {
                break;
            }
        }
        streams
    }
}
