//! Image header probing and resolution detection (design §4.1).

use std::io::{BufRead, Read, Seek};
use std::path::Path;

use image::ImageDecoder;
use image::metadata::Orientation;

use crate::error::ProbeError;

/// Supported file extensions for input images (design §4.1).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

/// Maximum total pixels allowed for an input image (design §4.1).
///
/// Fits an A3 page scanned at 600 dpi (~7016×9921 = 69.6 million pixels).
pub const MAX_IMAGE_PIXELS: u64 = 80_000_000;

/// Minimum valid DPI value (design §4.1). Values below this fall back to [`DEFAULT_DPI`].
pub const MIN_DPI: u32 = 36;

/// Maximum valid DPI value (design §4.1). Values above this fall back to [`DEFAULT_DPI`].
pub const MAX_DPI: u32 = 2400;

/// Default DPI used when resolution is missing, unknown or out of range (design §4.1).
pub const DEFAULT_DPI: u32 = 96;

/// Supported input image formats (design §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
    WebP,
    Bmp,
}

/// Metadata obtained by probing an image header without decoding pixels (design §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    /// Image format detected from header magic bytes.
    pub format: ImageFormat,
    /// Stored pixel width before orientation is applied.
    pub width: u32,
    /// Stored pixel height before orientation is applied.
    pub height: u32,
    /// EXIF orientation tag, or [`Orientation::NoTransforms`] if absent or unoriented.
    pub orientation: Orientation,
    /// Image resolution in dpi, or [`DEFAULT_DPI`] if absent, invalid or out of range.
    pub dpi: u32,
}

/// Maximum number of header bytes peeked for guessing format and extracting density segments.
const HEADER_PEEK_BYTES: usize = 64 * 1024;

/// Probes an image file at `path`, reading only its header to extract dimensions,
/// orientation and resolution without decoding pixel data (design §4.1).
///
/// # Errors
///
/// Returns [`ProbeError::ReadFailed`] if the file cannot be opened or read.
/// Returns [`ProbeError::UnsupportedFormat`] if the format is not PNG, JPEG, WebP, or BMP.
/// Returns [`ProbeError::DecodeFailed`] if the header is corrupt or truncated.
/// Returns [`ProbeError::TooLarge`] if total pixels exceed [`MAX_IMAGE_PIXELS`].
pub fn probe(path: &Path) -> Result<ImageInfo, ProbeError> {
    let file = std::fs::File::open(path).map_err(|_| ProbeError::ReadFailed)?;
    let reader = std::io::BufReader::new(file);
    probe_reader(reader)
}

/// Probes an image from a buffered, seekable reader (design §4.1).
///
/// # Errors
///
/// Returns [`ProbeError::UnsupportedFormat`] if the format is not PNG, JPEG, WebP, or BMP.
/// Returns [`ProbeError::DecodeFailed`] if the header is corrupt or truncated.
/// Returns [`ProbeError::TooLarge`] if total pixels exceed [`MAX_IMAGE_PIXELS`].
/// Returns [`ProbeError::ReadFailed`] if a seek or read error occurs on the reader.
pub fn probe_reader<R: BufRead + Seek>(mut reader: R) -> Result<ImageInfo, ProbeError> {
    // 1. Read up to 64 KiB of header bytes for format guessing and density parsing.
    let mut header_buf = Vec::new();
    reader
        .by_ref()
        .take(HEADER_PEEK_BYTES as u64)
        .read_to_end(&mut header_buf)
        .map_err(|_| ProbeError::ReadFailed)?;

    if header_buf.is_empty() {
        return Err(ProbeError::UnsupportedFormat);
    }

    // 2. Format detection using image::guess_format (design §4.1).
    let (format, image_crate_fmt) = match image::guess_format(&header_buf) {
        Ok(image::ImageFormat::Png) => (ImageFormat::Png, image::ImageFormat::Png),
        Ok(image::ImageFormat::Jpeg) => (ImageFormat::Jpeg, image::ImageFormat::Jpeg),
        Ok(image::ImageFormat::WebP) => (ImageFormat::WebP, image::ImageFormat::WebP),
        Ok(image::ImageFormat::Bmp) => (ImageFormat::Bmp, image::ImageFormat::Bmp),
        _ => return Err(ProbeError::UnsupportedFormat),
    };

    // 3. Rewind reader to decode the header without reading pixels.
    reader.rewind().map_err(|_| ProbeError::ReadFailed)?;

    let mut img_reader = image::ImageReader::new(&mut reader);
    img_reader.set_format(image_crate_fmt);

    let mut decoder = img_reader
        .into_decoder()
        .map_err(|_| ProbeError::DecodeFailed)?;

    let (width, height) = decoder.dimensions();

    // 4. Check total pixels before touching any pixel data (design §4.1).
    let total_pixels = u64::from(width).saturating_mul(u64::from(height));
    if total_pixels > MAX_IMAGE_PIXELS {
        return Err(ProbeError::TooLarge);
    }

    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let exif_chunk = decoder.exif_metadata().ok().flatten();

    // 5. Resolution resolution (design §4.1).
    let raw_dpi = match format {
        ImageFormat::Png => parse_png_dpi(&header_buf),
        ImageFormat::Jpeg => {
            // Design §4.1: JFIF density first, then EXIF XResolution if JFIF is absent or has no unit.
            parse_jfif_dpi(&header_buf).or_else(|| exif_chunk.as_deref().and_then(parse_exif_dpi))
        }
        ImageFormat::Bmp => parse_bmp_dpi(&header_buf),
        ImageFormat::WebP => None,
    };

    let dpi = match raw_dpi {
        Some(d) if (MIN_DPI..=MAX_DPI).contains(&d) => d,
        _ => DEFAULT_DPI,
    };

    Ok(ImageInfo {
        format,
        width,
        height,
        orientation,
        dpi,
    })
}

/// Parses the resolution in dpi from a PNG `pHYs` chunk if present (design §4.1).
///
/// Returns `None` if the chunk is missing, specifies an unknown unit (unit 0),
/// or contains non-positive values.
#[must_use]
pub fn parse_png_dpi(bytes: &[u8]) -> Option<u32> {
    const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(PNG_MAGIC) {
        return None;
    }
    let mut offset: usize = 8;
    while offset.checked_add(8)? <= bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start.checked_add(length)?;
        if data_end.checked_add(4)? > bytes.len() {
            break;
        }
        if chunk_type == b"pHYs" {
            let chunk_data = &bytes[data_start..data_end];
            if chunk_data.len() >= 9 {
                let ppu_x = u32::from_be_bytes(chunk_data[0..4].try_into().ok()?);
                let unit = chunk_data[8];
                if unit == 1 && ppu_x > 0 {
                    // Unit 1: metre. 1 metre = 1 / 0.0254 inches (~39.3700787 inches).
                    // Round to nearest integer dpi (design §4.1).
                    let dpi = (f64::from(ppu_x) * 0.0254).round() as u32;
                    return if dpi > 0 { Some(dpi) } else { None };
                }
            }
            return None;
        }
        if chunk_type == b"IDAT" || chunk_type == b"IEND" {
            break;
        }
        offset = data_end.checked_add(4)?;
    }
    None
}

/// Parses the resolution in dpi from a JPEG JFIF APP0 segment (design §4.1).
///
/// Returns `None` if JFIF is absent, unit is 0 (aspect ratio only), or density is 0.
#[must_use]
pub fn parse_jfif_dpi(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut offset: usize = 2;
    while offset.checked_add(4)? <= bytes.len() {
        if bytes[offset] != 0xFF {
            break;
        }
        while offset < bytes.len() && bytes[offset] == 0xFF {
            offset += 1;
        }
        if offset >= bytes.len() {
            break;
        }
        let marker = bytes[offset];
        offset += 1;

        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0xDA {
            // SOS: start of scan (image pixel data)
            break;
        }
        if offset.checked_add(2)? > bytes.len() {
            break;
        }
        let length = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if length < 2 {
            break;
        }
        let payload_start = offset + 2;
        let payload_end = match offset.checked_add(length) {
            Some(end) if end <= bytes.len() => end,
            _ => break,
        };
        let payload = &bytes[payload_start..payload_end];

        if marker == 0xE0 {
            // APP0: check for "JFIF\0"
            const JFIF_TAG: &[u8; 5] = b"JFIF\0";
            if payload.starts_with(JFIF_TAG) && payload.len() >= 9 {
                let unit = payload[7];
                let x_density = u16::from_be_bytes(payload[8..10].try_into().ok()?);
                if x_density > 0 {
                    match unit {
                        1 => {
                            // Dots per inch
                            return Some(u32::from(x_density));
                        }
                        2 => {
                            // Dots per cm: 1 inch = 2.54 cm -> dpi = dpcm * 2.54
                            let dpi = (f64::from(x_density) * 2.54).round() as u32;
                            return if dpi > 0 { Some(dpi) } else { None };
                        }
                        _ => {
                            // Unit 0 (aspect ratio) or unknown
                            return None;
                        }
                    }
                }
            }
        }
        offset = payload_end;
    }
    None
}

/// Parses the resolution in dpi from raw TIFF/EXIF bytes (design §4.1).
///
/// Looks for `XResolution` (tag 0x011A) and `ResolutionUnit` (tag 0x0128).
/// Defaults to inches if `ResolutionUnit` is omitted.
/// Returns `None` if `ResolutionUnit` is 1 (no absolute unit), denominator is 0,
/// or tag is missing.
#[must_use]
pub fn parse_exif_dpi(tiff: &[u8]) -> Option<u32> {
    if tiff.len() < 8 {
        return None;
    }
    let is_le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let magic = read_u16(tiff, 2, is_le)?;
    if magic != 42 {
        return None;
    }
    let ifd_offset = read_u32(tiff, 4, is_le)? as usize;
    if ifd_offset.checked_add(2)? > tiff.len() {
        return None;
    }
    let num_entries = read_u16(tiff, ifd_offset, is_le)? as usize;
    let mut x_res: Option<(u32, u32)> = None;
    let mut res_unit: Option<u16> = None;

    const TAG_X_RESOLUTION: u16 = 0x011A;
    const TAG_RESOLUTION_UNIT: u16 = 0x0128;
    const TYPE_SHORT: u16 = 3;
    const TYPE_RATIONAL: u16 = 5;

    for i in 0..num_entries {
        let entry_offset = ifd_offset.checked_add(2)?.checked_add(i.checked_mul(12)?)?;
        if entry_offset.checked_add(12)? > tiff.len() {
            break;
        }
        let tag = read_u16(tiff, entry_offset, is_le)?;
        let field_type = read_u16(tiff, entry_offset + 2, is_le)?;
        let count = read_u32(tiff, entry_offset + 4, is_le)?;

        if tag == TAG_X_RESOLUTION && field_type == TYPE_RATIONAL && count >= 1 {
            let val_offset = read_u32(tiff, entry_offset + 8, is_le)? as usize;
            if val_offset.checked_add(8)? <= tiff.len() {
                let num = read_u32(tiff, val_offset, is_le)?;
                let den = read_u32(tiff, val_offset + 4, is_le)?;
                x_res = Some((num, den));
            }
        } else if tag == TAG_RESOLUTION_UNIT && field_type == TYPE_SHORT && count >= 1 {
            let unit = read_u16(tiff, entry_offset + 8, is_le)?;
            res_unit = Some(unit);
        }
    }

    let (num, den) = x_res?;
    if den == 0 {
        return None;
    }
    // Design §4.1: ResolutionUnit defaults to inch (2) if omitted.
    let unit = res_unit.unwrap_or(2);
    match unit {
        2 => {
            // Inch
            let dpi = (f64::from(num) / f64::from(den)).round() as u32;
            if dpi > 0 { Some(dpi) } else { None }
        }
        3 => {
            // Centimetre: 1 inch = 2.54 cm -> dpi = dpcm * 2.54
            let dpcm = f64::from(num) / f64::from(den);
            let dpi = (dpcm * 2.54).round() as u32;
            if dpi > 0 { Some(dpi) } else { None }
        }
        _ => {
            // Unit 1 (no unit) or other unknown values -> unknown unit (None)
            None
        }
    }
}

/// Parses the resolution in dpi from a BMP header (design §4.1).
///
/// Reads `XPelsPerMeter` at offset 38 from `BITMAPINFOHEADER` (or larger headers).
/// Returns `None` if the header is too small, `XPelsPerMeter` is 0 or negative.
#[must_use]
pub fn parse_bmp_dpi(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 42 || !bytes.starts_with(b"BM") {
        return None;
    }
    let dib_header_size = u32::from_le_bytes(bytes[14..18].try_into().ok()?);
    if dib_header_size < 40 {
        return None;
    }
    let xpels_per_meter = i32::from_le_bytes(bytes[38..42].try_into().ok()?);
    if xpels_per_meter <= 0 {
        return None;
    }
    // 1 m = 1 / 0.0254 inches (~39.3700787 inches).
    let dpi = (f64::from(xpels_per_meter) * 0.0254).round() as u32;
    if dpi > 0 { Some(dpi) } else { None }
}

fn read_u16(bytes: &[u8], offset: usize, is_le: bool) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    let arr: [u8; 2] = slice.try_into().ok()?;
    Some(if is_le {
        u16::from_le_bytes(arr)
    } else {
        u16::from_be_bytes(arr)
    })
}

fn read_u32(bytes: &[u8], offset: usize, is_le: bool) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    let arr: [u8; 4] = slice.try_into().ok()?;
    Some(if is_le {
        u32::from_le_bytes(arr)
    } else {
        u32::from_be_bytes(arr)
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::Path;

    use super::*;

    const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    #[test]
    fn constants_match_design() {
        assert_eq!(IMAGE_EXTENSIONS, &["png", "jpg", "jpeg", "webp", "bmp"]);
        assert_eq!(MAX_IMAGE_PIXELS, 80_000_000);
        assert_eq!(MIN_DPI, 36);
        assert_eq!(MAX_DPI, 2400);
        assert_eq!(DEFAULT_DPI, 96);
    }

    #[test]
    fn probe_fixtures_all_expected() {
        let cases = [
            (
                "logo_alpha.png",
                ImageFormat::Png,
                256,
                256,
                Orientation::NoTransforms,
                96,
            ),
            (
                "logo_alpha.webp",
                ImageFormat::WebP,
                256,
                256,
                Orientation::NoTransforms,
                96,
            ),
            (
                "photo.jpg",
                ImageFormat::Jpeg,
                480,
                320,
                Orientation::NoTransforms,
                96,
            ),
            (
                "photo_cmyk.jpg",
                ImageFormat::Jpeg,
                320,
                240,
                Orientation::NoTransforms,
                96,
            ),
            (
                "rotate0.jpg",
                ImageFormat::Jpeg,
                160,
                96,
                Orientation::NoTransforms,
                96,
            ),
            (
                "flip_h.jpg",
                ImageFormat::Jpeg,
                160,
                96,
                Orientation::FlipHorizontal,
                96,
            ),
            (
                "rotate180.jpg",
                ImageFormat::Jpeg,
                160,
                96,
                Orientation::Rotate180,
                96,
            ),
            (
                "flip_v.jpg",
                ImageFormat::Jpeg,
                160,
                96,
                Orientation::FlipVertical,
                96,
            ),
            (
                "rotate90_flip_h.jpg",
                ImageFormat::Jpeg,
                96,
                160,
                Orientation::Rotate90FlipH,
                96,
            ),
            (
                "rotate90.jpg",
                ImageFormat::Jpeg,
                96,
                160,
                Orientation::Rotate90,
                96,
            ),
            (
                "rotate270_flip_h.jpg",
                ImageFormat::Jpeg,
                96,
                160,
                Orientation::Rotate270FlipH,
                96,
            ),
            (
                "rotate270.jpg",
                ImageFormat::Jpeg,
                96,
                160,
                Orientation::Rotate270,
                96,
            ),
            (
                "deep16.png",
                ImageFormat::Png,
                256,
                128,
                Orientation::NoTransforms,
                96,
            ),
            (
                "opaque.bmp",
                ImageFormat::Bmp,
                150,
                100,
                Orientation::NoTransforms,
                96,
            ),
            (
                "alpha32.bmp",
                ImageFormat::Bmp,
                150,
                100,
                Orientation::NoTransforms,
                96,
            ),
            (
                "dpi300.png",
                ImageFormat::Png,
                600,
                300,
                Orientation::NoTransforms,
                300,
            ),
            (
                "dpi300.jpg",
                ImageFormat::Jpeg,
                600,
                300,
                Orientation::NoTransforms,
                300,
            ),
        ];

        for (name, format, width, height, orientation, dpi) in cases {
            let path = Path::new(FIXTURES_DIR).join(name);
            let info = probe(&path).unwrap_or_else(|e| panic!("failed to probe {name}: {e:?}"));
            assert_eq!(info.format, format, "format mismatch for {name}");
            assert_eq!(info.width, width, "width mismatch for {name}");
            assert_eq!(info.height, height, "height mismatch for {name}");
            assert_eq!(
                info.orientation, orientation,
                "orientation mismatch for {name}"
            );
            assert_eq!(info.dpi, dpi, "dpi mismatch for {name}");
        }
    }

    #[test]
    fn corrupt_png_passes_probe_because_header_is_intact() {
        let path = Path::new(FIXTURES_DIR).join("corrupt.png");
        let info = probe(&path).expect("corrupt.png has a valid header and should pass probe");
        assert_eq!(info.format, ImageFormat::Png);
        assert_eq!(info.width, 256);
        assert_eq!(info.height, 256);
        assert_eq!(info.orientation, Orientation::NoTransforms);
        assert_eq!(info.dpi, 96);
    }

    #[test]
    fn decode_failed_on_truncated_header() {
        let path = Path::new(FIXTURES_DIR).join("logo_alpha.png");
        let bytes = std::fs::read(path).expect("read logo_alpha.png");
        // First 20 bytes is truncated in the middle of the IHDR chunk.
        let truncated = &bytes[..20];
        let err = probe_reader(Cursor::new(truncated)).unwrap_err();
        assert_eq!(err, ProbeError::DecodeFailed);
    }

    #[test]
    fn unsupported_format_on_non_image_or_other_formats() {
        // Text file
        let text = b"This is plain text and definitely not an image.";
        assert_eq!(
            probe_reader(Cursor::new(text)).unwrap_err(),
            ProbeError::UnsupportedFormat
        );

        // GIF magic bytes ("GIF89a")
        let gif_header = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04";
        assert_eq!(
            probe_reader(Cursor::new(gif_header)).unwrap_err(),
            ProbeError::UnsupportedFormat
        );

        // TIFF magic bytes ("II*\0" or "MM\0*")
        let tiff_header = b"II*\0\x08\x00\x00\x00";
        assert_eq!(
            probe_reader(Cursor::new(tiff_header)).unwrap_err(),
            ProbeError::UnsupportedFormat
        );
    }

    #[test]
    fn too_large_detected_before_pixels_from_header_only() {
        let path = Path::new(FIXTURES_DIR).join("opaque.bmp");
        let mut header = std::fs::read(path).expect("read opaque.bmp")[..54].to_vec();
        // Modify width (offset 18) and height (offset 22) to 10,000.
        // Total pixels = 10,000 × 10,000 = 100,000,000 > MAX_IMAGE_PIXELS (80,000,000).
        let dim: u32 = 10_000;
        header[18..22].copy_from_slice(&dim.to_le_bytes());
        header[22..26].copy_from_slice(&dim.to_le_bytes());

        // Header only (no pixel data). Decoder reports dimensions and detects TooLarge.
        let err = probe_reader(Cursor::new(header)).unwrap_err();
        assert_eq!(err, ProbeError::TooLarge);
        assert_eq!(err.code(), "TooLarge");
        assert_eq!(err.detail().as_deref(), Some("80,000,000"));
    }

    #[test]
    fn read_failed_on_nonexistent_file() {
        let path = Path::new(FIXTURES_DIR).join("does_not_exist.png");
        let err = probe(&path).unwrap_err();
        assert_eq!(err, ProbeError::ReadFailed);
        assert_eq!(err.code(), "ReadFailed");
        assert_eq!(err.detail(), None);
    }

    #[test]
    fn parse_png_phys_density() {
        // Valid 300 dpi (11811 ppm)
        let mut png = make_png_with_phys(11811, 1);
        assert_eq!(parse_png_dpi(&png), Some(300));

        // Unit 0 (aspect ratio only) -> None
        png = make_png_with_phys(11811, 0);
        assert_eq!(parse_png_dpi(&png), None);

        // Unknown unit 2 -> None
        png = make_png_with_phys(11811, 2);
        assert_eq!(parse_png_dpi(&png), None);

        // Zero ppm -> None
        png = make_png_with_phys(0, 1);
        assert_eq!(parse_png_dpi(&png), None);
    }

    #[test]
    fn parse_jfif_density() {
        // Unit 1 (dpi): 300
        let mut jfif = make_jpeg_with_jfif(300, 1);
        assert_eq!(parse_jfif_dpi(&jfif), Some(300));

        // Unit 2 (dpcm): 118 dpcm * 2.54 = 299.72 -> 300 dpi
        jfif = make_jpeg_with_jfif(118, 2);
        assert_eq!(parse_jfif_dpi(&jfif), Some(300));

        // Unit 0 (aspect ratio only) -> None
        jfif = make_jpeg_with_jfif(300, 0);
        assert_eq!(parse_jfif_dpi(&jfif), None);

        // Density 0 -> None
        jfif = make_jpeg_with_jfif(0, 1);
        assert_eq!(parse_jfif_dpi(&jfif), None);
    }

    #[test]
    fn parse_exif_density() {
        // Little Endian (II), Unit 2 (Inch): 300/1 -> 300 dpi
        let tiff_le_inch = make_exif_tiff((300, 1), Some(2), true);
        assert_eq!(parse_exif_dpi(&tiff_le_inch), Some(300));

        // Big Endian (MM), Unit 2 (Inch): 300/1 -> 300 dpi
        let tiff_be_inch = make_exif_tiff((300, 1), Some(2), false);
        assert_eq!(parse_exif_dpi(&tiff_be_inch), Some(300));

        // Unit 3 (Centimetre): 118/1 dpcm * 2.54 = 299.72 -> 300 dpi
        let tiff_cm = make_exif_tiff((118, 1), Some(3), true);
        assert_eq!(parse_exif_dpi(&tiff_cm), Some(300));

        // Unit omitted -> defaults to Inch (design §4.1)
        let tiff_no_unit = make_exif_tiff((150, 1), None, true);
        assert_eq!(parse_exif_dpi(&tiff_no_unit), Some(150));

        // Unit 1 (no unit) -> None
        let tiff_unit_1 = make_exif_tiff((300, 1), Some(1), true);
        assert_eq!(parse_exif_dpi(&tiff_unit_1), None);

        // Denominator 0 -> None
        let tiff_zero_den = make_exif_tiff((300, 0), Some(2), true);
        assert_eq!(parse_exif_dpi(&tiff_zero_den), None);
    }

    #[test]
    fn parse_bmp_density() {
        let path = Path::new(FIXTURES_DIR).join("opaque.bmp");
        let mut header = std::fs::read(path).expect("read opaque.bmp")[..54].to_vec();

        // 11811 ppm -> 300 dpi
        let ppm: i32 = 11811;
        header[38..42].copy_from_slice(&ppm.to_le_bytes());
        assert_eq!(parse_bmp_dpi(&header), Some(300));

        // 0 ppm -> None
        header[38..42].copy_from_slice(&0i32.to_le_bytes());
        assert_eq!(parse_bmp_dpi(&header), None);

        // Negative ppm -> None
        header[38..42].copy_from_slice(&(-5i32).to_le_bytes());
        assert_eq!(parse_bmp_dpi(&header), None);
    }

    #[test]
    fn dpi_range_boundaries_and_fallback_to_default() {
        // Below MIN_DPI (35 dpi -> 96 dpi)
        let png_35 = make_png_with_phys(1378, 1); // 1378 ppm * 0.0254 = 35.0012 -> 35
        assert_eq!(parse_png_dpi(&png_35), Some(35));
        let info_35 = probe_reader(Cursor::new(make_complete_png(1378))).unwrap();
        assert_eq!(info_35.dpi, DEFAULT_DPI);

        // Boundary MIN_DPI (36 dpi -> 36 dpi)
        let png_36 = make_png_with_phys(1417, 1); // 1417 ppm * 0.0254 = 35.9918 -> 36
        assert_eq!(parse_png_dpi(&png_36), Some(36));
        let info_36 = probe_reader(Cursor::new(make_complete_png(1417))).unwrap();
        assert_eq!(info_36.dpi, 36);

        // Boundary MAX_DPI (2400 dpi -> 2400 dpi)
        let png_2400 = make_png_with_phys(94488, 1); // 94488 ppm * 0.0254 = 2399.9952 -> 2400
        assert_eq!(parse_png_dpi(&png_2400), Some(2400));
        let info_2400 = probe_reader(Cursor::new(make_complete_png(94488))).unwrap();
        assert_eq!(info_2400.dpi, 2400);

        // Above MAX_DPI (2401 dpi -> 96 dpi)
        let png_2401 = make_png_with_phys(94528, 1); // 94528 ppm * 0.0254 = 2401.0112 -> 2401
        assert_eq!(parse_png_dpi(&png_2401), Some(2401));
        let info_2401 = probe_reader(Cursor::new(make_complete_png(94528))).unwrap();
        assert_eq!(info_2401.dpi, DEFAULT_DPI);
    }

    #[test]
    fn jpeg_with_exif_only_and_no_jfif() {
        // photo.jpg has no JFIF density segment and no EXIF density, falls back to 96
        let path = Path::new(FIXTURES_DIR).join("photo.jpg");
        let info = probe(&path).unwrap();
        assert_eq!(info.dpi, DEFAULT_DPI);

        // Build a JPEG with SOI + APP1 (EXIF with XResolution = 300) + SOF0 + SOS
        let exif_tiff = make_exif_tiff((300, 1), Some(2), true);
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        // APP1 with Exif\0\0 header
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        let app1_len = 2 + 6 + exif_tiff.len();
        jpeg.extend_from_slice(&(app1_len as u16).to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&exif_tiff);
        // SOF0 (baseline DCT): len 11, 8 bits, height 10, width 10, 1 component
        jpeg.extend_from_slice(&[0xFF, 0xC0]);
        jpeg.extend_from_slice(&11u16.to_be_bytes());
        jpeg.push(8); // precision
        jpeg.extend_from_slice(&10u16.to_be_bytes()); // height
        jpeg.extend_from_slice(&10u16.to_be_bytes()); // width
        jpeg.push(1); // 1 component
        jpeg.extend_from_slice(&[1, 0x11, 0]); // component 1: id 1, samp 0x11, qtable 0
        // SOS
        jpeg.extend_from_slice(&[0xFF, 0xDA]);
        jpeg.extend_from_slice(&8u16.to_be_bytes());
        jpeg.push(1);
        jpeg.extend_from_slice(&[1, 0]);
        jpeg.extend_from_slice(&[0, 63, 0]);
        // Scan data + EOI
        jpeg.extend_from_slice(&[0x00, 0xFF, 0xD9]);

        let probed = probe_reader(Cursor::new(jpeg)).expect("probe jpeg with exif only");
        assert_eq!(probed.format, ImageFormat::Jpeg);
        assert_eq!(probed.width, 10);
        assert_eq!(probed.height, 10);
        assert_eq!(probed.dpi, 300);
    }

    // Helper functions for synthesizing test images

    fn make_png_with_phys(ppm: u32, unit: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        // IHDR chunk: 13 bytes
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&1u32.to_be_bytes()); // w = 1
        bytes.extend_from_slice(&1u32.to_be_bytes()); // h = 1
        bytes.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
        bytes.extend_from_slice(&[0; 4]); // dummy CRC
        // pHYs chunk: 9 bytes
        bytes.extend_from_slice(&9u32.to_be_bytes());
        bytes.extend_from_slice(b"pHYs");
        bytes.extend_from_slice(&ppm.to_be_bytes());
        bytes.extend_from_slice(&ppm.to_be_bytes());
        bytes.push(unit);
        bytes.extend_from_slice(&[0; 4]); // dummy CRC
        bytes
    }

    fn make_complete_png(ppm: u32) -> Vec<u8> {
        let path = Path::new(FIXTURES_DIR).join("dpi300.png");
        let original = std::fs::read(path).expect("read dpi300.png");
        // Locate pHYs in dpi300.png and replace ppu_x and ppu_y
        let mut modified = original;
        let mut pos = 8;
        while pos + 8 <= modified.len() {
            let len = u32::from_be_bytes(modified[pos..pos + 4].try_into().unwrap()) as usize;
            if &modified[pos + 4..pos + 8] == b"pHYs" {
                modified[pos + 8..pos + 12].copy_from_slice(&ppm.to_be_bytes());
                modified[pos + 12..pos + 16].copy_from_slice(&ppm.to_be_bytes());
                // Recalculate CRC for pHYs
                let crc_data = &modified[pos + 4..pos + 8 + len];
                let crc = crc32_simple(crc_data);
                modified[pos + 8 + len..pos + 12 + len].copy_from_slice(&crc.to_be_bytes());
                break;
            }
            pos += 8 + len + 4;
        }
        modified
    }

    fn crc32_simple(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    fn make_jpeg_with_jfif(density: u16, unit: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xFF, 0xD8]); // SOI
        bytes.extend_from_slice(&[0xFF, 0xE0]); // APP0
        let payload_len: u16 = 2 + 5 + 2 + 1 + 2 + 2 + 2;
        bytes.extend_from_slice(&payload_len.to_be_bytes());
        bytes.extend_from_slice(b"JFIF\0");
        bytes.extend_from_slice(&[1, 1]); // version 1.1
        bytes.push(unit);
        bytes.extend_from_slice(&density.to_be_bytes());
        bytes.extend_from_slice(&density.to_be_bytes());
        bytes.extend_from_slice(&[0, 0]); // no thumbnail
        bytes
    }

    fn make_exif_tiff(x_res: (u32, u32), res_unit: Option<u16>, is_le: bool) -> Vec<u8> {
        let mut tiff = Vec::new();
        let num_entries = if res_unit.is_some() { 2u16 } else { 1u16 };
        let val_offset = 8 + 2 + (num_entries as u32) * 12 + 4;

        if is_le {
            tiff.extend_from_slice(b"II");
            tiff.extend_from_slice(&42u16.to_le_bytes());
            tiff.extend_from_slice(&8u32.to_le_bytes()); // first IFD offset = 8
            tiff.extend_from_slice(&num_entries.to_le_bytes());
            // Entry 1: XResolution (0x011A), RATIONAL (5), count 1
            tiff.extend_from_slice(&0x011Au16.to_le_bytes());
            tiff.extend_from_slice(&5u16.to_le_bytes());
            tiff.extend_from_slice(&1u32.to_le_bytes());
            tiff.extend_from_slice(&val_offset.to_le_bytes());
            if let Some(unit) = res_unit {
                // Entry 2: ResolutionUnit (0x0128), SHORT (3), count 1
                tiff.extend_from_slice(&0x0128u16.to_le_bytes());
                tiff.extend_from_slice(&3u16.to_le_bytes());
                tiff.extend_from_slice(&1u32.to_le_bytes());
                tiff.extend_from_slice(&unit.to_le_bytes());
                tiff.extend_from_slice(&[0, 0]);
            }
            tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
            tiff.extend_from_slice(&x_res.0.to_le_bytes());
            tiff.extend_from_slice(&x_res.1.to_le_bytes());
        } else {
            tiff.extend_from_slice(b"MM");
            tiff.extend_from_slice(&42u16.to_be_bytes());
            tiff.extend_from_slice(&8u32.to_be_bytes());
            tiff.extend_from_slice(&num_entries.to_be_bytes());
            tiff.extend_from_slice(&0x011Au16.to_be_bytes());
            tiff.extend_from_slice(&5u16.to_be_bytes());
            tiff.extend_from_slice(&1u32.to_be_bytes());
            tiff.extend_from_slice(&val_offset.to_be_bytes());
            if let Some(unit) = res_unit {
                tiff.extend_from_slice(&0x0128u16.to_be_bytes());
                tiff.extend_from_slice(&3u16.to_be_bytes());
                tiff.extend_from_slice(&1u32.to_be_bytes());
                tiff.extend_from_slice(&unit.to_be_bytes());
                tiff.extend_from_slice(&[0, 0]);
            }
            tiff.extend_from_slice(&0u32.to_be_bytes());
            tiff.extend_from_slice(&x_res.0.to_be_bytes());
            tiff.extend_from_slice(&x_res.1.to_be_bytes());
        }
        tiff
    }
}
