//! Thumbnails of input images (design §6.1).

use std::io::Cursor;

use image::imageops::FilterType;

use crate::error::ProbeError;
use crate::pdf_write::decode_image_pixels;
use crate::probe::probe_reader;

/// Length of the longer side of a thumbnail in pixels (design §6.1).
pub const THUMBNAIL_SIDE: u32 = 160;

/// Makes a PNG thumbnail of the image in `image_bytes`: the EXIF orientation
/// is applied and the longer side is [`THUMBNAIL_SIDE`] pixels, so smaller
/// images are enlarged to match the thumbnails of PDF pages.
///
/// # Errors
///
/// Returns [`ProbeError::UnsupportedFormat`] if the format is not supported,
/// [`ProbeError::TooLarge`] if the image has more than
/// [`crate::MAX_IMAGE_PIXELS`] pixels, and [`ProbeError::DecodeFailed`] if the
/// header or the pixels are damaged or the PNG cannot be written.
pub fn thumbnail_png(image_bytes: &[u8]) -> Result<Vec<u8>, ProbeError> {
    let info = probe_reader(Cursor::new(image_bytes))?;
    let mut pixels = decode_image_pixels(image_bytes)?;
    pixels.apply_orientation(info.orientation);

    let (width, height) = fit_longer_side(pixels.width(), pixels.height());
    let thumbnail = pixels.resize_exact(width, height, FilterType::Triangle);

    let mut png = Vec::new();
    thumbnail
        .write_to(Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|_| ProbeError::DecodeFailed)?;
    Ok(png)
}

/// Scales `width` × `height` so that the longer side is [`THUMBNAIL_SIDE`],
/// keeping the shorter side at one pixel or more.
fn fit_longer_side(width: u32, height: u32) -> (u32, u32) {
    let longer = u64::from(width.max(height)).max(1);
    let scale = |side: u32| -> u32 {
        let scaled = (u64::from(side) * u64::from(THUMBNAIL_SIDE) + longer / 2) / longer;
        u32::try_from(scaled).unwrap_or(THUMBNAIL_SIDE).max(1)
    };
    (scale(width), scale(height))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::layout::orientation_swaps_dimensions;
    use crate::probe::probe;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn thumbnail_size(name: &str) -> (u32, u32) {
        let bytes = std::fs::read(fixture(name)).expect("reading the fixture");
        let png = thumbnail_png(&bytes).expect("making the thumbnail");
        assert_eq!(
            image::guess_format(&png).ok(),
            Some(image::ImageFormat::Png)
        );
        let decoded = image::load_from_memory(&png).expect("decoding the thumbnail");
        (decoded.width(), decoded.height())
    }

    #[test]
    fn fits_the_longer_side() {
        assert_eq!(fit_longer_side(480, 320), (160, 107));
        assert_eq!(fit_longer_side(320, 480), (107, 160));
        assert_eq!(fit_longer_side(256, 256), (160, 160));
        assert_eq!(fit_longer_side(40, 20), (160, 80));
        assert_eq!(fit_longer_side(80_000, 1), (160, 1));
    }

    #[test]
    fn the_longer_side_is_the_thumbnail_side() {
        assert_eq!(thumbnail_size("photo.jpg"), (THUMBNAIL_SIDE, 107));
        assert_eq!(thumbnail_size("dpi300.png"), (THUMBNAIL_SIDE, 80));
        assert_eq!(
            thumbnail_size("logo_alpha.png"),
            (THUMBNAIL_SIDE, THUMBNAIL_SIDE)
        );
        assert_eq!(thumbnail_size("deep16.png"), (THUMBNAIL_SIDE, 80));
        let (bmp_width, bmp_height) = thumbnail_size("opaque.bmp");
        assert_eq!(bmp_width.max(bmp_height), THUMBNAIL_SIDE);
    }

    #[test]
    fn orientation_swaps_the_sides() {
        let swapping = [
            "rotate90.jpg",
            "rotate270.jpg",
            "rotate90_flip_h.jpg",
            "rotate270_flip_h.jpg",
        ];
        let keeping = ["rotate0.jpg", "rotate180.jpg", "flip_h.jpg", "flip_v.jpg"];

        for name in swapping.iter().chain(keeping.iter()) {
            let info = probe(&fixture(name)).expect("probing the fixture");
            assert_eq!(
                orientation_swaps_dimensions(info.orientation),
                swapping.contains(name)
            );
            assert_ne!(
                info.width, info.height,
                "{name} must not be square to show the swap"
            );

            let stored = fit_longer_side(info.width, info.height);
            let expected = if swapping.contains(name) {
                (stored.1, stored.0)
            } else {
                stored
            };
            assert_eq!(thumbnail_size(name), expected, "{name}");
        }
    }

    #[test]
    fn rejects_what_is_not_an_image() {
        assert_eq!(
            thumbnail_png(b"not an image").unwrap_err(),
            ProbeError::UnsupportedFormat
        );
        let bytes = std::fs::read(fixture("corrupt.png")).expect("reading the fixture");
        assert_eq!(thumbnail_png(&bytes).unwrap_err(), ProbeError::DecodeFailed);
    }
}
