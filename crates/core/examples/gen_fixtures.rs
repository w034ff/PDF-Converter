//! Generates the test fixtures of design §11.1 into `crates/core/tests/fixtures/`.
//!
//! Run with `cargo run -p pdfconv-core --example gen_fixtures` and commit the
//! result. The output depends only on this file and the pinned encoders: every
//! value comes from integer or basic f32 arithmetic (no `sin`, no random
//! numbers), so it is the same on every platform. `tests/fixtures.rs` regenerates it in
//! memory and fails when it differs from the committed files.
//!
//! The EXIF orientation fixtures all display as the same 160×96 picture once
//! their orientation is applied; their stored pixels are that picture with the
//! inverse transform. Names follow `image::metadata::Orientation`:
//!
//! | File | EXIF value |
//! | --- | --- |
//! | `rotate0.jpg` | 1 |
//! | `flip_h.jpg` | 2 |
//! | `rotate180.jpg` | 3 |
//! | `flip_v.jpg` | 4 |
//! | `rotate90_flip_h.jpg` | 5 |
//! | `rotate90.jpg` | 6 |
//! | `rotate270_flip_h.jpg` | 7 |
//! | `rotate270.jpg` | 8 |
//!
//! `shapes_150dpi_p<N>.png` is what page N of `shapes.pdf` should look like when
//! rendered at 150 dpi on a white background (design §11.2). Straight edges
//! and image cells lie on that pixel grid; the disc and the slanted edges of the
//! triangle are anti-aliased from 4 × 4 samples per pixel.
//!
//! `encrypted.pdf` (user password [`ENCRYPTED_USER_PASSWORD`]) and
//! `restricted.pdf` (no user password, printing and copying denied) use the
//! standard security handler, revision 3 with 128-bit RC4, written here because
//! the PDF crates with encryption put random bytes into the output.

use std::error::Error;
use std::path::Path;

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Rect, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use md5::{Digest, Md5};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Rgb = [u8; 3];

/// Relative to the crate root.
pub const FIXTURES_DIR: &str = "tests/fixtures";

const JPEG_QUALITY: u8 = 90;
/// The resolution written into `dpi300.png` and `dpi300.jpg`.
const FIXTURE_DPI: u16 = 300;
const MICROMETERS_PER_INCH: u32 = 25_400;

/// Page sizes of `shapes.pdf` in pt (design §4.2 for A4).
const A4_PT: (f32, f32) = (595.28, 841.89);
const LETTER_PT: (f32, f32) = (612.0, 792.0);
const POINTS_PER_INCH: f32 = 72.0;
/// The resolution of the expected renderings of `shapes.pdf`; the default
/// render dpi of design §4.4.
const SHAPES_RENDER_DPI: f32 = 150.0;
/// Side of one checker cell of the image in `shapes.pdf`, in pixels at
/// [`SHAPES_RENDER_DPI`].
const CHECKER_CELL_PX: u32 = 25;
const CHECKER_CELLS: u32 = 8;

#[path = "../tests/support/fixture_passwords.rs"]
mod fixture_passwords;

/// `/P` with every permission bit of revision 3 set.
const PERMISSIONS_ALL: i32 = -4;
/// `/P` with printing, modifying, copying and annotating denied (bits 3–6 and
/// 9–12 clear, the reserved bits set as revision 3 requires).
const PERMISSIONS_RESTRICTED: i32 = -3904;
/// Padding string of the standard security handler (ISO 32000-1 §7.6.3.3).
const PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];
/// Revision 3 rehashes the key this many times (Algorithms 2 and 3).
const KEY_HASH_ROUNDS: usize = 50;
/// Revision 3 re-encrypts with derived keys this many times (Algorithms 3 and 5).
const RC4_ROUNDS: u8 = 19;
const KEY_LENGTH_BYTES: usize = 16;

const WHITE: Rgb = [255, 255, 255];
const BLACK: Rgb = [0, 0, 0];
const RED: Rgb = [220, 40, 40];
const YELLOW: Rgb = [250, 210, 0];
const GREEN: Rgb = [40, 170, 90];
const BLUE: Rgb = [40, 80, 200];
const ORANGE: Rgb = [240, 130, 20];

/// One generated file: its name in [`FIXTURES_DIR`] and its bytes.
pub struct Fixture {
    pub name: String,
    pub bytes: Vec<u8>,
}

fn main() -> Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    std::fs::create_dir_all(&dir)?;
    for fixture in fixtures()? {
        std::fs::write(dir.join(&fixture.name), &fixture.bytes)?;
    }
    Ok(())
}

/// Every fixture of design §11.1, in memory.
pub fn fixtures() -> Result<Vec<Fixture>> {
    let mut out = Vec::new();
    let mut add = |name: &str, bytes: Vec<u8>| {
        out.push(Fixture {
            name: name.to_owned(),
            bytes,
        })
    };

    let logo = logo_rgba(256, 256);
    let logo_png = encode_png(
        256,
        256,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &logo,
        None,
    )?;
    add("corrupt.png", logo_png[..logo_png.len() / 2].to_vec());
    add("logo_alpha.png", logo_png);
    add("logo_alpha.webp", encode_webp(256, 256, &logo)?);

    add(
        "photo.jpg",
        encode_jpeg(
            480,
            320,
            jpeg_encoder::ColorType::Rgb,
            &photo_rgb(480, 320),
            None,
            None,
        )?,
    );
    add("exif_gps.jpg", encode_exif_gps_jpeg()?);
    add(
        "photo_cmyk.jpg",
        encode_jpeg(
            320,
            240,
            jpeg_encoder::ColorType::Cmyk,
            &photo_cmyk(320, 240),
            None,
            None,
        )?,
    );

    let (display_w, display_h) = (160, 96);
    let display = orientation_rgb(display_w, display_h);
    for (name, exif) in [
        ("rotate0.jpg", 1),
        ("flip_h.jpg", 2),
        ("rotate180.jpg", 3),
        ("flip_v.jpg", 4),
        ("rotate90_flip_h.jpg", 5),
        ("rotate90.jpg", 6),
        ("rotate270_flip_h.jpg", 7),
        ("rotate270.jpg", 8),
    ] {
        let (w, h, stored) = stored_for_orientation(display_w, display_h, &display, exif);
        add(
            name,
            encode_jpeg(
                w,
                h,
                jpeg_encoder::ColorType::Rgb,
                &stored,
                None,
                Some(exif),
            )?,
        );
    }

    add(
        "deep16.png",
        encode_png(
            256,
            128,
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            &deep16_rgba(256, 128),
            None,
        )?,
    );

    // An odd width, so that BMP rows need padding.
    let bmp = bmp_rgba(150, 100);
    add(
        "opaque.bmp",
        encode_bmp(150, 100, &rgba_to_rgb(&bmp), image::ExtendedColorType::Rgb8)?,
    );
    add(
        "alpha32.bmp",
        encode_bmp(150, 100, &bmp, image::ExtendedColorType::Rgba8)?,
    );

    // 2 × 1 inches at 300 dpi.
    let dpi_picture = dpi_rgb(600, 300);
    add(
        "dpi300.png",
        encode_png(
            600,
            300,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &dpi_picture,
            Some(FIXTURE_DPI),
        )?,
    );
    add(
        "dpi300.jpg",
        encode_jpeg(
            600,
            300,
            jpeg_encoder::ColorType::Rgb,
            &dpi_picture,
            Some(FIXTURE_DPI),
            None,
        )?,
    );

    let pages = shapes_pages();
    let shapes = shapes_pdf(&pages)?;
    for (index, page) in pages.iter().enumerate() {
        let (w, h) = page.size_px();
        let expected = render_expected(page);
        add(
            &format!("shapes_150dpi_p{}.png", index + 1),
            encode_png(
                w,
                h,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &expected,
                None,
            )?,
        );
    }
    add("corrupt.pdf", shapes[..shapes.len() / 2].to_vec());
    add("shapes.pdf", shapes);

    let mixed_pages = vec![
        ShapesPage {
            size_pt: A4_PT,
            number: 1,
        },
        ShapesPage {
            size_pt: (3000.0, 3000.0),
            number: 2,
        },
        ShapesPage {
            size_pt: A4_PT,
            number: 3,
        },
    ];
    let mixed = shapes_pdf(&mixed_pages)?;
    add("mixed_sizes.pdf", mixed);

    add(
        "encrypted.pdf",
        encrypted_pdf(
            "encrypted",
            &password_to_pdf_doc_encoding(fixture_passwords::ENCRYPTED_USER_PASSWORD)?,
            PERMISSIONS_ALL,
        )?,
    );
    add(
        "encrypted_cafe.pdf",
        encrypted_pdf(
            "encrypted_cafe",
            &password_to_pdf_doc_encoding(fixture_passwords::ENCRYPTED_CAFE_USER_PASSWORD)?,
            PERMISSIONS_ALL,
        )?,
    );
    add(
        "restricted.pdf",
        encrypted_pdf("restricted", b"", PERMISSIONS_RESTRICTED)?,
    );

    Ok(out)
}

// ---------------------------------------------------------------------------
// Pictures

/// Many colours: two ramps and a checker that mixes them.
fn photo_rgb(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let r = ramp(x, w);
            let g = ramp(y, h);
            let b = if (x / 16 + y / 16) % 2 == 0 {
                255 - r
            } else {
                ((u32::from(r) + u32::from(g)) / 2) as u8
            };
            out.extend_from_slice(&[r, g, b]);
        }
    }
    out
}

fn exif_gps_rgb(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let r = ramp(x, w);
            let g = ramp(y, h);
            let b = ((u32::from(r) + u32::from(g)) / 2) as u8;
            out.extend_from_slice(&[r, g, b]);
        }
    }
    out
}

fn photo_cmyk(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = ramp(x, w);
            let m = ramp(y, h);
            let k = if (x / 32 + y / 32) % 2 == 0 { 0 } else { 64 };
            out.extend_from_slice(&[c, m, 255 - c / 2, k]);
        }
    }
    out
}

/// Four coloured quadrants and a black marker near the top-left corner, so that
/// every flip and rotation looks different.
fn orientation_rgb(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let marker = (8..32).contains(&x) && (8..24).contains(&y);
            let colour = match (marker, x < w / 2, y < h / 2) {
                (true, _, _) => BLACK,
                (false, true, true) => RED,
                (false, false, true) => GREEN,
                (false, true, false) => BLUE,
                (false, false, false) => YELLOW,
            };
            out.extend_from_slice(&colour);
        }
    }
    out
}

/// Returns the stored width, height and RGB pixels that display as `display`
/// (`w` × `h`) once EXIF orientation `exif` is applied.
fn stored_for_orientation(w: u32, h: u32, display: &[u8], exif: u16) -> (u32, u32, Vec<u8>) {
    let (sw, sh) = if exif >= 5 { (h, w) } else { (w, h) };
    let mut stored = vec![0; display.len()];
    for y in 0..h {
        for x in 0..w {
            // Where display pixel (x, y) comes from in the stored picture.
            let (sx, sy) = match exif {
                1 => (x, y),
                2 => (sw - 1 - x, y),
                3 => (sw - 1 - x, sh - 1 - y),
                4 => (x, sh - 1 - y),
                5 => (y, x),
                6 => (y, sh - 1 - x),
                7 => (sw - 1 - y, sh - 1 - x),
                _ => (sw - 1 - y, x),
            };
            let from = ((y * w + x) * 3) as usize;
            let to = ((sy * sw + sx) * 3) as usize;
            stored[to..to + 3].copy_from_slice(&display[from..from + 3]);
        }
    }
    (sw, sh, stored)
}

const LOGO_DISC: Disc = Disc {
    cx: 96,
    cy: 112,
    radius: 72,
};

/// A transparent background with an anti-aliased opaque disc, a half
/// transparent square over it and an alpha ramp along the top.
fn logo_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut pixel = [0, 0, 0, 0];
            if y < 16 {
                pixel = [BLUE[0], BLUE[1], BLUE[2], ramp(x, w)];
            }
            let covered = coverage(x, y, |sx, sy| in_disc(&LOGO_DISC, sx, sy));
            if covered > 0 {
                let alpha = (covered * 255 / SAMPLES_PER_PIXEL) as u8;
                pixel = over(pixel, [ORANGE[0], ORANGE[1], ORANGE[2], alpha]);
            }
            if (112..240).contains(&x) && (128..240).contains(&y) {
                pixel = over(pixel, [GREEN[0], GREEN[1], GREEN[2], 128]);
            }
            out.extend_from_slice(&pixel);
        }
    }
    out
}

/// Values that 8 bits cannot hold: the low byte of each channel varies too.
fn deep16_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 8) as usize);
    for y in 0..h {
        for x in 0..w {
            let r = (x * 256 + y) as u16;
            let g = (y * 512 + x) as u16;
            let b = (65_535 - x * 256) as u16;
            let a = if x < w * 3 / 4 {
                u16::MAX
            } else {
                (y * 65_535 / (h - 1)) as u16
            };
            for channel in [r, g, b, a] {
                out.extend_from_slice(&channel.to_be_bytes());
            }
        }
    }
    out
}

fn bmp_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let alpha = if x < w / 2 { 255 } else { ramp(y, h) };
            out.extend_from_slice(&[ramp(x, w), 255 - ramp(y, h), 160, alpha]);
        }
    }
    out
}

/// A red and a blue square, one inch each at [`FIXTURE_DPI`].
fn dpi_rgb(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for _ in 0..h {
        for x in 0..w {
            out.extend_from_slice(if x < w / 2 { &RED } else { &BLUE });
        }
    }
    out
}

/// `value` in `0..len` scaled to `0..=255`.
fn ramp(value: u32, len: u32) -> u8 {
    (value * 255 / (len - 1)) as u8
}

/// Straight-alpha "source over destination".
fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let sa = u32::from(src[3]);
    let da = u32::from(dst[3]) * (255 - sa) / 255;
    let a = sa + da;
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let mix = |s: u8, d: u8| ((u32::from(s) * sa + u32::from(d) * da + a / 2) / a) as u8;
    [
        mix(src[0], dst[0]),
        mix(src[1], dst[1]),
        mix(src[2], dst[2]),
        a as u8,
    ]
}

fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    let (pixels, _) = rgba.as_chunks::<4>();
    pixels.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect()
}

// ---------------------------------------------------------------------------
// Encoders

fn encode_png(
    w: u32,
    h: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
    dpi: Option<u16>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if let Some(dpi) = dpi {
        // pHYs stores pixels per metre.
        let ppm = (u32::from(dpi) * 1_000_000 + MICROMETERS_PER_INCH / 2) / MICROMETERS_PER_INCH;
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppm,
            yppu: ppm,
            unit: png::Unit::Meter,
        }));
    }
    encoder.write_header()?.write_image_data(data)?;
    Ok(out)
}

fn encode_jpeg(
    w: u32,
    h: u32,
    color: jpeg_encoder::ColorType,
    data: &[u8],
    dpi: Option<u16>,
    exif_orientation: Option<u16>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
    if let Some(dpi) = dpi {
        encoder.set_density(jpeg_encoder::PixelDensity::dpi(dpi));
    }
    if let Some(orientation) = exif_orientation {
        encoder.add_exif_metadata(&exif_with_orientation(orientation))?;
    }
    encoder.encode(data, u16::try_from(w)?, u16::try_from(h)?, color)?;
    Ok(out)
}

/// A little-endian TIFF structure with one IFD holding only the Orientation tag.
fn exif_with_orientation(orientation: u16) -> Vec<u8> {
    const ORIENTATION_TAG: u16 = 0x0112;
    const TYPE_SHORT: u16 = 3;
    const FIRST_IFD_OFFSET: u32 = 8;
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&FIRST_IFD_OFFSET.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&ORIENTATION_TAG.to_le_bytes());
    tiff.extend_from_slice(&TYPE_SHORT.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    // A SHORT value is left-aligned in the 4-byte value field.
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    // No next IFD.
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff
}

fn encode_exif_gps_jpeg() -> Result<Vec<u8>> {
    const W: u32 = 64;
    const H: u32 = 48;
    let rgb = exif_gps_rgb(W, H);

    // Encode base JPEG image data with jpeg_encoder (without JFIF density or EXIF)
    let raw_jpeg = encode_jpeg(W, H, jpeg_encoder::ColorType::Rgb, &rgb, None, None)?;

    // raw_jpeg starts with SOI [0xFF, 0xD8]. Skip encoder's APP0 if present to ensure exactly one JFIF.
    let mut rest_start = 2;
    if raw_jpeg.len() >= 4 && raw_jpeg[2] == 0xFF && raw_jpeg[3] == 0xE0 {
        let seg_len = usize::from(u16::from_be_bytes([raw_jpeg[4], raw_jpeg[5]]));
        rest_start += 2 + seg_len;
    }

    let eoi_pos = raw_jpeg
        .windows(2)
        .rposition(|w| w == [0xFF, 0xD9])
        .ok_or("missing EOI in encoded jpeg")?;

    let mut out = Vec::new();
    // 1. SOI
    out.extend_from_slice(&[0xFF, 0xD8]);

    // 2. Custom JFIF APP0 with 1x1 RGB thumbnail (3 bytes)
    let mut jfif_payload = Vec::new();
    jfif_payload.extend_from_slice(b"JFIF\0");
    jfif_payload.extend_from_slice(&[1, 2]); // version 1.2
    jfif_payload.push(1); // units: dpi
    jfif_payload.extend_from_slice(&96u16.to_be_bytes()); // Xdensity
    jfif_payload.extend_from_slice(&96u16.to_be_bytes()); // Ydensity
    jfif_payload.push(1); // Xthumbnail (1 px)
    jfif_payload.push(1); // Ythumbnail (1 px)
    jfif_payload.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // 1x1 RGB thumbnail bytes
    out.extend_from_slice(&[0xFF, 0xE0]);
    let jfif_len = u16::try_from(jfif_payload.len() + 2)?;
    out.extend_from_slice(&jfif_len.to_be_bytes());
    out.extend_from_slice(&jfif_payload);

    // 3. EXIF APP1 (TIFF with Model, DateTimeOriginal in Exif IFD, GPS in GPS IFD)
    let exif_payload = build_exif_gps_payload();
    out.extend_from_slice(&[0xFF, 0xE1]);
    let exif_len = u16::try_from(exif_payload.len() + 2)?;
    out.extend_from_slice(&exif_len.to_be_bytes());
    out.extend_from_slice(&exif_payload);

    // 4. XMP APP1
    let xmp_xml = b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
<dc:description>PDFCONV-TEST-XMP-DESC</dc:description>\
</rdf:Description>\
</rdf:RDF>\
</x:xmpmeta>";
    let mut xmp_payload = Vec::new();
    xmp_payload.extend_from_slice(b"http://ns.adobe.com/xap/1.0/\0");
    xmp_payload.extend_from_slice(xmp_xml);
    out.extend_from_slice(&[0xFF, 0xE1]);
    let xmp_len = u16::try_from(xmp_payload.len() + 2)?;
    out.extend_from_slice(&xmp_len.to_be_bytes());
    out.extend_from_slice(&xmp_payload);

    // 5. IPTC APP13 (Photoshop 3.0 8BIM record)
    let iptc_payload = build_iptc_payload();
    out.extend_from_slice(&[0xFF, 0xED]);
    let iptc_len = u16::try_from(iptc_payload.len() + 2)?;
    out.extend_from_slice(&iptc_len.to_be_bytes());
    out.extend_from_slice(&iptc_payload);

    // 6. COM (Comment)
    let com_payload = b"PDFCONV-TEST-COMMENT";
    out.extend_from_slice(&[0xFF, 0xFE]);
    let com_len = u16::try_from(com_payload.len() + 2)?;
    out.extend_from_slice(&com_len.to_be_bytes());
    out.extend_from_slice(com_payload);

    // 7. Image segments from encoder up to and including EOI
    out.extend_from_slice(&raw_jpeg[rest_start..=eoi_pos + 1]);

    // 8. Trailer after EOI
    out.extend_from_slice(b"PDFCONV-TEST-TRAILER\n");

    Ok(out)
}

fn build_exif_gps_payload() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Exif\0\0");

    let model = b"PDFCONV-TEST-MODEL\0";
    let date = b"2026:10:14 00:00:00\0";

    let ifd0_offset = 8u32;
    let model_offset = 50u32;
    let exif_ifd_offset = 72u32;
    let date_offset = 90u32;
    let gps_ifd_offset = 112u32;
    let lat_rationals_offset = 168u32;
    let lon_rationals_offset = 192u32;

    let mut tiff = Vec::new();
    // Header
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&ifd0_offset.to_le_bytes());

    // IFD0: 3 entries
    tiff.extend_from_slice(&3u16.to_le_bytes());
    // Entry 0: Model (0x0110, ASCII)
    tiff.extend_from_slice(&0x0110u16.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend_from_slice(&(model.len() as u32).to_le_bytes());
    tiff.extend_from_slice(&model_offset.to_le_bytes());
    // Entry 1: ExifOffset (0x8769, LONG)
    tiff.extend_from_slice(&0x8769u16.to_le_bytes());
    tiff.extend_from_slice(&4u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&exif_ifd_offset.to_le_bytes());
    // Entry 2: GPSInfo (0x8825, LONG)
    tiff.extend_from_slice(&0x8825u16.to_le_bytes());
    tiff.extend_from_slice(&4u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&gps_ifd_offset.to_le_bytes());
    // Next IFD
    tiff.extend_from_slice(&0u32.to_le_bytes());

    // Model data at offset 50
    assert_eq!(tiff.len(), model_offset as usize);
    tiff.extend_from_slice(model);
    while tiff.len() < exif_ifd_offset as usize {
        tiff.push(0);
    }

    // Exif IFD: 1 entry
    assert_eq!(tiff.len(), exif_ifd_offset as usize);
    tiff.extend_from_slice(&1u16.to_le_bytes());
    // Entry 0: DateTimeOriginal (0x9003, ASCII)
    tiff.extend_from_slice(&0x9003u16.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend_from_slice(&(date.len() as u32).to_le_bytes());
    tiff.extend_from_slice(&date_offset.to_le_bytes());
    // Next IFD
    tiff.extend_from_slice(&0u32.to_le_bytes());

    // Date data at offset 90
    assert_eq!(tiff.len(), date_offset as usize);
    tiff.extend_from_slice(date);
    while tiff.len() < gps_ifd_offset as usize {
        tiff.push(0);
    }

    // GPS IFD: 4 entries
    assert_eq!(tiff.len(), gps_ifd_offset as usize);
    tiff.extend_from_slice(&4u16.to_le_bytes());
    // Entry 0: GPSLatitudeRef (0x0001, ASCII)
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend_from_slice(&2u32.to_le_bytes());
    tiff.extend_from_slice(&[b'N', 0, 0, 0]);
    // Entry 1: GPSLatitude (0x0002, RATIONAL)
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend_from_slice(&5u16.to_le_bytes());
    tiff.extend_from_slice(&3u32.to_le_bytes());
    tiff.extend_from_slice(&lat_rationals_offset.to_le_bytes());
    // Entry 2: GPSLongitudeRef (0x0003, ASCII)
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend_from_slice(&2u32.to_le_bytes());
    tiff.extend_from_slice(&[b'E', 0, 0, 0]);
    // Entry 3: GPSLongitude (0x0004, RATIONAL)
    tiff.extend_from_slice(&4u16.to_le_bytes());
    tiff.extend_from_slice(&5u16.to_le_bytes());
    tiff.extend_from_slice(&3u32.to_le_bytes());
    tiff.extend_from_slice(&lon_rationals_offset.to_le_bytes());
    // Next IFD
    tiff.extend_from_slice(&0u32.to_le_bytes());

    // Padding to offset 168
    while tiff.len() < lat_rationals_offset as usize {
        tiff.push(0);
    }

    // Lat rationals (35/1, 41/1, 23/1) at 168
    assert_eq!(tiff.len(), lat_rationals_offset as usize);
    tiff.extend_from_slice(&35u32.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&41u32.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&23u32.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());

    // Lon rationals (139/1, 41/1, 32/1) at 192
    assert_eq!(tiff.len(), lon_rationals_offset as usize);
    tiff.extend_from_slice(&139u32.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&41u32.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&32u32.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());

    out.extend_from_slice(&tiff);
    out
}

fn build_iptc_payload() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Photoshop 3.0\0");

    let caption = b"PDFCONV-TEST-IPTC-CAPTION";
    let mut iptc_data = Vec::new();
    iptc_data.push(0x1C); // Tag marker
    iptc_data.push(0x02); // Record 2
    iptc_data.push(0x78); // Dataset 120 (Caption/Abstract)
    let caption_len = u16::try_from(caption.len()).expect("caption len fits in u16");
    iptc_data.extend_from_slice(&caption_len.to_be_bytes());
    iptc_data.extend_from_slice(caption);

    // 8BIM resource
    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(&0x0404u16.to_le_bytes()); // IPTC-NAA record ID
    out.extend_from_slice(&[0x00, 0x00]); // empty name, padded
    let size = u32::try_from(iptc_data.len()).expect("iptc len fits in u32");
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(&iptc_data);
    if iptc_data.len() % 2 != 0 {
        out.push(0);
    }
    out
}

fn encode_webp(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out).encode(
        rgba,
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

fn encode_bmp(w: u32, h: u32, data: &[u8], color: image::ExtendedColorType) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut out).encode(data, w, h, color)?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// shapes.pdf

/// A page of `shapes.pdf`. Its shapes are the constants below.
struct ShapesPage {
    size_pt: (f32, f32),
    /// 1-based; drawn as that many small black squares.
    number: u32,
}

struct Disc {
    cx: u32,
    cy: u32,
    radius: u32,
}

/// A triangle with a horizontal base from `(left, base_y)` to
/// `(right, base_y)` and its apex above the middle of the base.
struct Triangle {
    left: u32,
    right: u32,
    base_y: u32,
    apex_y: u32,
}

/// Half-open pixel rectangle.
#[derive(Clone, Copy)]
struct PxRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl PxRect {
    fn contains(&self, x: u32, y: u32) -> bool {
        (self.x0..self.x1).contains(&x) && (self.y0..self.y1).contains(&y)
    }
}

// Shape coordinates are in pixels at SHAPES_RENDER_DPI, with y pointing down as
// in krilla and in the rendered picture. Each is a multiple of 25 px, which is
// 12 pt, a value that f32 and the PDF hold exactly, so that straight edges fall
// on pixel boundaries: pdfium fills a whole pixel that an edge enters even
// slightly. The layout fits in 1240 × 1240 px, the shorter side of an A4 page,
// so that one layout serves all three pages.
const MARKER_SIZE_PX: u32 = 50;
const MARKER_STEP_PX: u32 = 75;
const MARKER_ORIGIN_PX: u32 = 50;
const RED_RECT: PxRect = PxRect {
    x0: 125,
    y0: 200,
    x1: 525,
    y1: 500,
};
/// Drawn at [`TRANSLUCENT_OPACITY`] over part of [`RED_RECT`].
const YELLOW_RECT: PxRect = PxRect {
    x0: 400,
    y0: 375,
    x1: 700,
    y1: 650,
};
const DISC: Disc = Disc {
    cx: 950,
    cy: 400,
    radius: 175,
};
const TRIANGLE: Triangle = Triangle {
    left: 150,
    right: 550,
    base_y: 1150,
    apex_y: 800,
};
const CHECKER_ORIGIN_PX: (u32, u32) = (850, 950);
const TRANSLUCENT_OPACITY: f32 = 0.5;
/// [`TRANSLUCENT_OPACITY`] as the integer weight of the expected pictures.
const TRANSLUCENT_WEIGHT_PERCENT: u32 = 50;

impl ShapesPage {
    fn size_px(&self) -> (u32, u32) {
        (
            pt_to_px_rounded(self.size_pt.0),
            pt_to_px_rounded(self.size_pt.1),
        )
    }

    fn markers(&self) -> Vec<PxRect> {
        (0..self.number)
            .map(|i| {
                let x0 = MARKER_ORIGIN_PX + i * MARKER_STEP_PX;
                PxRect {
                    x0,
                    y0: MARKER_ORIGIN_PX,
                    x1: x0 + MARKER_SIZE_PX,
                    y1: MARKER_ORIGIN_PX + MARKER_SIZE_PX,
                }
            })
            .collect()
    }
}

fn checker_area() -> PxRect {
    let (x0, y0) = CHECKER_ORIGIN_PX;
    let side = CHECKER_CELL_PX * CHECKER_CELLS;
    PxRect {
        x0,
        y0,
        x1: x0 + side,
        y1: y0 + side,
    }
}

fn shapes_pages() -> Vec<ShapesPage> {
    vec![
        ShapesPage {
            size_pt: A4_PT,
            number: 1,
        },
        ShapesPage {
            size_pt: (A4_PT.1, A4_PT.0),
            number: 2,
        },
        ShapesPage {
            size_pt: LETTER_PT,
            number: 3,
        },
    ]
}

fn pt_to_px_rounded(pt: f32) -> u32 {
    (pt / POINTS_PER_INCH * SHAPES_RENDER_DPI).round() as u32
}

fn px_to_pt(px: u32) -> f32 {
    px as f32 * POINTS_PER_INCH / SHAPES_RENDER_DPI
}

fn checker_colour(cell_x: u32, cell_y: u32) -> Rgb {
    [BLACK, WHITE, ORANGE, BLUE][((cell_x + 2 * cell_y) % 4) as usize]
}

fn checker_png() -> Result<Vec<u8>> {
    let mut data = Vec::new();
    for y in 0..CHECKER_CELLS {
        for x in 0..CHECKER_CELLS {
            data.extend_from_slice(&checker_colour(x, y));
        }
    }
    encode_png(
        CHECKER_CELLS,
        CHECKER_CELLS,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &data,
        None,
    )
}

fn shapes_pdf(pages: &[ShapesPage]) -> Result<Vec<u8>> {
    let checker = checker_png()?;
    let mut document = Document::new();
    for page in pages {
        let settings =
            PageSettings::from_wh(page.size_pt.0, page.size_pt.1).ok_or("invalid page size")?;
        let mut pdf_page = document.start_page_with(settings);
        let mut surface = pdf_page.surface();

        let mut fill_rect = |rect: PxRect, colour: Rgb, opacity: f32| -> Result<()> {
            let mut builder = PathBuilder::new();
            builder.push_rect(
                Rect::from_ltrb(
                    px_to_pt(rect.x0),
                    px_to_pt(rect.y0),
                    px_to_pt(rect.x1),
                    px_to_pt(rect.y1),
                )
                .ok_or("invalid rectangle")?,
            );
            surface.set_fill(Some(solid(colour, opacity)?));
            surface.draw_path(&builder.finish().ok_or("empty path")?);
            Ok(())
        };
        for marker in page.markers() {
            fill_rect(marker, BLACK, 1.0)?;
        }
        fill_rect(RED_RECT, RED, 1.0)?;
        fill_rect(YELLOW_RECT, YELLOW, TRANSLUCENT_OPACITY)?;

        surface.set_fill(Some(solid(GREEN, 1.0)?));
        surface.draw_path(&disc_path(&DISC).ok_or("empty path")?);

        let triangle = &TRIANGLE;
        let mut builder = PathBuilder::new();
        builder.move_to(px_to_pt(triangle.left), px_to_pt(triangle.base_y));
        builder.line_to(px_to_pt(triangle.right), px_to_pt(triangle.base_y));
        builder.line_to(
            px_to_pt((triangle.left + triangle.right) / 2),
            px_to_pt(triangle.apex_y),
        );
        builder.close();
        surface.set_fill(Some(solid(BLUE, 1.0)?));
        surface.draw_path(&builder.finish().ok_or("empty path")?);

        let area = checker_area();
        // Interpolation off, so that each cell renders as a sharp square.
        let image = Image::from_png(checker.clone().into(), false)?;
        surface.push_transform(&Transform::from_translate(
            px_to_pt(area.x0),
            px_to_pt(area.y0),
        ));
        surface.draw_image(
            image,
            Size::from_wh(px_to_pt(area.x1 - area.x0), px_to_pt(area.y1 - area.y0))
                .ok_or("invalid image size")?,
        );
        surface.pop();

        surface.finish();
        pdf_page.finish();
    }
    Ok(document.finish()?)
}

fn solid(colour: Rgb, opacity: f32) -> Result<Fill> {
    Ok(Fill {
        paint: rgb::Color::new(colour[0], colour[1], colour[2]).into(),
        opacity: NormalizedF32::new(opacity).ok_or("invalid opacity")?,
        rule: FillRule::NonZero,
    })
}

/// A circle from four cubic Béziers.
fn disc_path(disc: &Disc) -> Option<krilla::geom::Path> {
    // Control point distance for a quarter circle, as a fraction of the radius.
    const KAPPA: f32 = 0.552_284_8;
    let (cx, cy, r) = (px_to_pt(disc.cx), px_to_pt(disc.cy), px_to_pt(disc.radius));
    let k = r * KAPPA;
    let mut builder = PathBuilder::new();
    builder.move_to(cx + r, cy);
    builder.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    builder.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    builder.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    builder.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    builder.close();
    builder.finish()
}

/// The expected 150 dpi rendering of `page`, painted in the same order as
/// [`shapes_pdf`]. The disc and the triangle are anti-aliased from
/// [`SAMPLES_PER_PIXEL`] samples; every other edge lies on a pixel boundary.
fn render_expected(page: &ShapesPage) -> Vec<u8> {
    let (w, h) = page.size_px();
    let markers = page.markers();
    let checker = checker_area();
    // Sampling is slow in debug builds, so only pixels that can be covered are.
    let disc_bounds = PxRect {
        x0: DISC.cx - DISC.radius,
        y0: DISC.cy - DISC.radius,
        x1: DISC.cx + DISC.radius,
        y1: DISC.cy + DISC.radius,
    };
    let triangle_bounds = PxRect {
        x0: TRIANGLE.left,
        y0: TRIANGLE.apex_y,
        x1: TRIANGLE.right,
        y1: TRIANGLE.base_y,
    };
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut colour = WHITE;
            if markers.iter().any(|m| m.contains(x, y)) {
                colour = BLACK;
            }
            if RED_RECT.contains(x, y) {
                colour = RED;
            }
            if YELLOW_RECT.contains(x, y) {
                colour = mix(colour, YELLOW, TRANSLUCENT_WEIGHT_PERCENT, 100);
            }
            if disc_bounds.contains(x, y) {
                colour = mix(
                    colour,
                    GREEN,
                    coverage(x, y, |sx, sy| in_disc(&DISC, sx, sy)),
                    SAMPLES_PER_PIXEL,
                );
            }
            if triangle_bounds.contains(x, y) {
                colour = mix(
                    colour,
                    BLUE,
                    coverage(x, y, |sx, sy| in_triangle(&TRIANGLE, sx, sy)),
                    SAMPLES_PER_PIXEL,
                );
            }
            if checker.contains(x, y) {
                colour = checker_colour(
                    (x - checker.x0) / CHECKER_CELL_PX,
                    (y - checker.y0) / CHECKER_CELL_PX,
                );
            }
            out.extend_from_slice(&colour);
        }
    }
    out
}

/// `over` drawn with weight `weight / total` on top of `under`.
fn mix(under: Rgb, over: Rgb, weight: u32, total: u32) -> Rgb {
    let channel = |u: u8, o: u8| {
        ((u32::from(o) * weight + u32::from(u) * (total - weight) + total / 2) / total) as u8
    };
    [
        channel(under[0], over[0]),
        channel(under[1], over[1]),
        channel(under[2], over[2]),
    ]
}

/// Sample positions are in 1/[`SUBPIXEL`] pixel units, so that they are
/// integers: 4 × 4 samples per pixel at 1/8, 3/8, 5/8 and 7/8.
const SUBPIXEL: i64 = 8;
const SAMPLES_PER_AXIS: i64 = 4;
const SAMPLES_PER_PIXEL: u32 = 16;

/// How many of the samples of pixel (x, y) `inside` accepts, out of
/// [`SAMPLES_PER_PIXEL`].
fn coverage(x: u32, y: u32, inside: impl Fn(i64, i64) -> bool) -> u32 {
    let step = SUBPIXEL / SAMPLES_PER_AXIS;
    let mut count = 0;
    for j in 0..SAMPLES_PER_AXIS {
        for i in 0..SAMPLES_PER_AXIS {
            let sx = i64::from(x) * SUBPIXEL + i * step + step / 2;
            let sy = i64::from(y) * SUBPIXEL + j * step + step / 2;
            if inside(sx, sy) {
                count += 1;
            }
        }
    }
    count
}

/// Whether sample (sx, sy), in subpixel units, is inside the disc.
fn in_disc(disc: &Disc, sx: i64, sy: i64) -> bool {
    let dx = sx - i64::from(disc.cx) * SUBPIXEL;
    let dy = sy - i64::from(disc.cy) * SUBPIXEL;
    let r = i64::from(disc.radius) * SUBPIXEL;
    dx * dx + dy * dy <= r * r
}

/// Whether sample (sx, sy), in subpixel units, is inside the triangle.
fn in_triangle(t: &Triangle, sx: i64, sy: i64) -> bool {
    let (left, right) = (i64::from(t.left) * SUBPIXEL, i64::from(t.right) * SUBPIXEL);
    let (base, apex_y) = (
        i64::from(t.base_y) * SUBPIXEL,
        i64::from(t.apex_y) * SUBPIXEL,
    );
    let apex_x = (left + right) / 2;
    if sy > base || sy < apex_y {
        return false;
    }
    // Inside both slanted edges: the cross product of each edge with the point
    // is non-negative, as it is for the opposite vertex.
    let left_edge = (apex_x - left) * (sy - base) - (apex_y - base) * (sx - left);
    let right_edge = (right - apex_x) * (sy - apex_y) - (base - apex_y) * (sx - apex_x);
    left_edge >= 0 && right_edge >= 0
}

// ---------------------------------------------------------------------------
// Encrypted PDFs

/// Converts a fixture password to PDFDocEncoding bytes for revision 3 standard security handler.
///
/// In PDFDocEncoding (ISO 32000-1 Annex D), 'é' (U+00E9) maps to single byte 0xE9.
fn password_to_pdf_doc_encoding(password: &str) -> Result<Vec<u8>> {
    password
        .chars()
        .map(|c| match c {
            'é' => Ok(0xE9),
            ascii if (ascii as u32) < 128 => Ok(ascii as u8),
            other => Err(format!("unsupported character in fixture password: {other:?}").into()),
        })
        .collect()
}

/// A one-page PDF with a blue rectangle, encrypted with the standard security
/// handler (revision 3, 128-bit RC4). `label` only seeds the file identifier.
fn encrypted_pdf(label: &str, user_password: &[u8], permissions: i32) -> Result<Vec<u8>> {
    let owner_password = password_to_pdf_doc_encoding(fixture_passwords::OWNER_PASSWORD)?;
    let file_id = md5(&[b"pdfconv-fixture:", label.as_bytes()].concat());
    let owner_entry = owner_entry(&owner_password, user_password);
    let key = file_key(user_password, &owner_entry, permissions, &file_id);
    let user_entry = user_entry(&key, &file_id);

    const CONTENT_OBJECT: u32 = 4;
    let content = b"0.16 0.31 0.78 rg 100 400 300 200 re f\n";
    let encrypted_content = rc4(&object_key(&key, CONTENT_OBJECT, 0), content);

    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {CONTENT_OBJECT} 0 R /Resources << >> >>",
            A4_PT.0, A4_PT.1
        )
        .into_bytes(),
        [
            format!("<< /Length {} >>\nstream\n", encrypted_content.len()).into_bytes(),
            encrypted_content,
            b"\nendstream".to_vec(),
        ]
        .concat(),
        format!(
            "<< /Filter /Standard /V 2 /R 3 /Length {} /O <{}> /U <{}> /P {permissions} >>",
            KEY_LENGTH_BYTES * 8,
            hex(&owner_entry),
            hex(&user_entry)
        )
        .into_bytes(),
    ];

    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    let id = hex(&file_id);
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Encrypt {} 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1,
            objects.len()
        )
        .as_bytes(),
    );
    Ok(pdf)
}

fn padded(password: &[u8]) -> Vec<u8> {
    password
        .iter()
        .chain(PASSWORD_PADDING.iter())
        .take(PASSWORD_PADDING.len())
        .copied()
        .collect()
}

/// Algorithm 3: the `/O` entry.
fn owner_entry(owner_password: &[u8], user_password: &[u8]) -> Vec<u8> {
    let mut hash = md5(&padded(owner_password));
    for _ in 0..KEY_HASH_ROUNDS {
        hash = md5(&hash[..KEY_LENGTH_BYTES]);
    }
    rc4_rounds(&hash[..KEY_LENGTH_BYTES], padded(user_password))
}

/// Algorithm 2: the file encryption key.
fn file_key(user_password: &[u8], owner_entry: &[u8], permissions: i32, file_id: &[u8]) -> Vec<u8> {
    let input = [
        &padded(user_password)[..],
        owner_entry,
        &permissions.to_le_bytes(),
        file_id,
    ]
    .concat();
    let mut hash = md5(&input);
    for _ in 0..KEY_HASH_ROUNDS {
        hash = md5(&hash[..KEY_LENGTH_BYTES]);
    }
    hash[..KEY_LENGTH_BYTES].to_vec()
}

/// Algorithm 5: the `/U` entry. The last 16 bytes are arbitrary; zeros keep
/// the file reproducible.
fn user_entry(key: &[u8], file_id: &[u8]) -> Vec<u8> {
    let hash = md5(&[&PASSWORD_PADDING[..], file_id].concat());
    let mut entry = rc4_rounds(key, hash.to_vec());
    entry.resize(32, 0);
    entry
}

/// Encrypts with `key`, then once more with `key` XOR i for i in 1..=19.
fn rc4_rounds(key: &[u8], data: Vec<u8>) -> Vec<u8> {
    let mut data = rc4(key, &data);
    for round in 1..=RC4_ROUNDS {
        let round_key: Vec<u8> = key.iter().map(|b| b ^ round).collect();
        data = rc4(&round_key, &data);
    }
    data
}

/// Algorithm 1: the key for the strings and streams of one object.
fn object_key(key: &[u8], object: u32, generation: u16) -> Vec<u8> {
    let input = [key, &object.to_le_bytes()[..3], &generation.to_le_bytes()].concat();
    md5(&input)[..(key.len() + 5).min(16)].to_vec()
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, usize::from(j));
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[usize::from(i)]);
            s.swap(usize::from(i), usize::from(j));
            byte ^ s[usize::from(s[usize::from(i)].wrapping_add(s[usize::from(j)]))]
        })
        .collect()
}

fn md5(data: &[u8]) -> [u8; 16] {
    Md5::digest(data).into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
