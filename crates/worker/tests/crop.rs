//! Cropping against the standalone worker: the embedded images of a page,
//! whether something covers one, and rendering one range (design §11.2, §11.3).
//!
//! `crop_align.pdf` holds one picture whose every pixel differs, placed in
//! several ways on three pages that differ in `/Rotate`. The expected images
//! are computed here by sending each pixel of the picture through the same
//! matrices, a forward mapping that shares no code with the worker's
//! reordering of the rendered pixels.

use std::path::PathBuf;

use image::{GenericImageView, RgbaImage};
use pdfconv_worker::PAGE_VIEW_MAX_SIDE;
use pdfconv_worker::client::{RENDER_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{
    EmbeddedImage, RectPt, RenderColor, RenderFormat, Request, Response,
};

#[path = "../../core/tests/support/crop_pattern.rs"]
mod crop_pattern;
#[path = "support/worker_paths.rs"]
mod worker_paths;

use crop_pattern::{Matrix, PICTURE_HEIGHT, PICTURE_WIDTH};
use worker_paths::start;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures")
}

fn started_with(fixture: &str) -> WorkerProcess {
    let mut worker = start();
    let opened = worker.request(
        &Request::Open {
            path: fixtures_dir().join(fixture),
            password: None,
        },
        RENDER_TIMEOUT,
    );
    assert!(opened.is_ok(), "opening {fixture}: {opened:?}");
    worker
}

fn crop_align() -> WorkerProcess {
    started_with("crop_align.pdf")
}

fn expect_error(result: Result<(Response, Vec<u8>), WorkerError>, expected_code: &str) {
    let Err(WorkerError::Remote { code, .. }) = &result else {
        unreachable!("expected {expected_code}, got {result:?}");
    };
    assert_eq!(code, expected_code);
}

fn page_images(worker: &mut WorkerProcess, page: u32) -> Vec<EmbeddedImage> {
    let answer = worker.request(&Request::PageImages { page }, RENDER_TIMEOUT);
    let Ok((Response::PageImages { images }, body)) = answer else {
        unreachable!("PageImages should answer with the images");
    };
    assert!(body.is_empty());
    images
}

fn image_on_top(worker: &mut WorkerProcess, page: u32, image_index: u32) -> bool {
    let answer = worker.request(&Request::ImageOnTop { page, image_index }, RENDER_TIMEOUT);
    let Ok((Response::ImageOnTop { on_top }, _)) = answer else {
        unreachable!("ImageOnTop should answer with the verdict");
    };
    on_top
}

/// One `RenderRegion` request.
#[derive(Clone)]
struct Region {
    page: u32,
    rect: RectPt,
    image_index: Option<u32>,
    dpi: u32,
    format: RenderFormat,
    color: RenderColor,
    photo_native: bool,
    photo_only: bool,
}

impl Region {
    /// An image in its own pixels, as PNG in colour, with the page around it.
    fn photo(page: u32, image: &EmbeddedImage) -> Self {
        Self {
            page,
            rect: image.rect_pt,
            image_index: Some(image.image_index),
            dpi: 150,
            format: RenderFormat::Png,
            color: RenderColor::Color,
            photo_native: true,
            photo_only: false,
        }
    }

    /// A rectangle drawn at `dpi`.
    fn rectangle(page: u32, rect: RectPt, dpi: u32) -> Self {
        Self {
            page,
            rect,
            image_index: None,
            dpi,
            format: RenderFormat::Png,
            color: RenderColor::Color,
            photo_native: false,
            photo_only: false,
        }
    }

    fn photo_only(self) -> Self {
        Self {
            photo_only: true,
            ..self
        }
    }

    fn in_format(self, format: RenderFormat) -> Self {
        Self { format, ..self }
    }

    fn in_color(self, color: RenderColor) -> Self {
        Self { color, ..self }
    }

    fn request(&self) -> Request {
        Request::RenderRegion {
            page: self.page,
            rect_pt: self.rect,
            image_index: self.image_index,
            dpi: self.dpi,
            format: self.format,
            color: self.color,
            photo_native: self.photo_native,
            photo_only: self.photo_only,
        }
    }
}

struct Rendered {
    width: u32,
    height: u32,
    scaled_down: bool,
    bytes: Vec<u8>,
}

impl Rendered {
    fn decode(&self) -> image::DynamicImage {
        image::load_from_memory(&self.bytes).expect("the region should decode")
    }
}

fn try_render(worker: &mut WorkerProcess, region: &Region) -> Result<Rendered, WorkerError> {
    let (response, bytes) = worker.request(&region.request(), RENDER_TIMEOUT)?;
    let Response::RenderRegion {
        width,
        height,
        scaled_down,
    } = response
    else {
        unreachable!("RenderRegion should answer with the size, got {response:?}");
    };
    Ok(Rendered {
        width,
        height,
        scaled_down,
        bytes,
    })
}

fn render(worker: &mut WorkerProcess, region: &Region) -> Rendered {
    try_render(worker, region).expect("RenderRegion should succeed")
}

fn expect_region_error(worker: &mut WorkerProcess, region: &Region, expected_code: &str) {
    expect_error(
        worker.request(&region.request(), RENDER_TIMEOUT),
        expected_code,
    );
}

const PAGE_COUNT: usize = 3;

/// The corners of the unit square placed by `matrix`, in the shown page.
fn shown_corners(page_index: usize, matrix: Matrix) -> Vec<(f64, f64)> {
    [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .iter()
        .map(|&(u, v)| {
            let (x, y) = crop_pattern::apply(matrix, u, v);
            crop_pattern::to_shown_page(page_index, x, y)
        })
        .collect()
}

/// The frame of an image on the shown page, as [x0, y0, x1, y1].
fn expected_frame(page_index: usize, matrix: Matrix) -> [f64; 4] {
    let corners = shown_corners(page_index, matrix);
    let fold = |pick: fn(&(f64, f64)) -> f64, init: f64, f: fn(f64, f64) -> f64| {
        corners.iter().map(pick).fold(init, f)
    };
    [
        fold(|c| c.0, f64::INFINITY, f64::min),
        fold(|c| c.1, f64::INFINITY, f64::min),
        fold(|c| c.0, f64::NEG_INFINITY, f64::max),
        fold(|c| c.1, f64::NEG_INFINITY, f64::max),
    ]
}

/// What the picture placed by `matrix` looks like on shown page `page_index`
/// when its pixels line up with the output: pixel (column, row) of the
/// picture is `value(column, row)`, and it lands where the matrices send its
/// centre. The image is as wide as the frame holds pictures side by side.
fn expected_view(
    page_index: usize,
    matrix: Matrix,
    value: impl Fn(u32, u32) -> [u8; 4],
) -> RgbaImage {
    let [x0, y0, x1, y1] = expected_frame(page_index, matrix);
    let shown = |u: f64, v: f64| {
        let (x, y) = crop_pattern::apply(matrix, u, v);
        crop_pattern::to_shown_page(page_index, x, y)
    };
    let origin = shown(0.0, 0.0);
    let along_columns = shown(1.0, 0.0);
    let transposed = (along_columns.0 - origin.0).abs() < (along_columns.1 - origin.1).abs();
    let (width, height) = if transposed {
        (PICTURE_HEIGHT, PICTURE_WIDTH)
    } else {
        (PICTURE_WIDTH, PICTURE_HEIGHT)
    };

    let mut view = RgbaImage::new(width, height);
    let cell = |position: f64, low: f64, high: f64, count: u32| {
        let index = ((position - low) / (high - low) * f64::from(count)).floor();
        (index.max(0.0) as u32).min(count - 1)
    };
    for row in 0..PICTURE_HEIGHT {
        for column in 0..PICTURE_WIDTH {
            let (x, y) = shown(
                (f64::from(column) + 0.5) / f64::from(PICTURE_WIDTH),
                1.0 - (f64::from(row) + 0.5) / f64::from(PICTURE_HEIGHT),
            );
            view.put_pixel(
                cell(x, x0, x1, width),
                cell(y, y0, y1, height),
                image::Rgba(value(column, row)),
            );
        }
    }
    view
}

/// `view` cut to the box around its pixels that are not fully transparent:
/// what a photo range holds when the image alone touches only those.
fn trimmed(view: &RgbaImage) -> RgbaImage {
    let touched = |x: u32, y: u32| view.get_pixel(x, y)[3] != 0;
    let columns: Vec<u32> = (0..view.width())
        .filter(|&x| (0..view.height()).any(|y| touched(x, y)))
        .collect();
    let rows: Vec<u32> = (0..view.height())
        .filter(|&y| (0..view.width()).any(|x| touched(x, y)))
        .collect();
    let (left, top) = (columns[0], rows[0]);
    let (right, bottom) = (columns[columns.len() - 1] + 1, rows[rows.len() - 1] + 1);
    image::imageops::crop_imm(view, left, top, right - left, bottom - top).to_image()
}

fn picture_value(column: u32, row: u32) -> [u8; 4] {
    let [r, g, b] = crop_pattern::picture_pixel(column, row);
    [r, g, b, 255]
}

/// How many pixels of `actual` differ from `expected`; both must be the same size.
fn differing_pixels(actual: &RgbaImage, expected: &RgbaImage) -> usize {
    assert_eq!(actual.dimensions(), expected.dimensions());
    actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, e)| a != e)
        .count()
}

/// The grid the clip and the mask are measured on is the image's own 61 × 43
/// pixels, 2.46 × 2.34 pt each for the clipped image. A pixel the clip edge
/// cuts is touched and the box then grows by one more pixel, so the range may
/// reach two pixels past the clip.
const CLIPPED_RANGE_TOLERANCE_PT: f64 = 2.0 * 2.5;

/// The range `PageImages` should give for a listed image, as [x0, y0, x1, y1]
/// on shown page `page_index`, and how far it may be off: the whole frame, but
/// for the clipped image the part inside the clip and for the soft-masked one
/// the columns its mask lets show, widened by one pixel.
fn visible_frame(page_index: usize, image: &crop_pattern::ListedImage) -> ([f64; 4], f64) {
    const EXACT_PT: f64 = 0.01;
    let bounds = |points: &[(f64, f64)]| {
        let shown: Vec<(f64, f64)> = points
            .iter()
            .map(|&(x, y)| crop_pattern::to_shown_page(page_index, x, y))
            .collect();
        let pick = |f: fn(&(f64, f64)) -> f64, init: f64, g: fn(f64, f64) -> f64| {
            shown.iter().map(f).fold(init, g)
        };
        [
            pick(|c| c.0, f64::INFINITY, f64::min),
            pick(|c| c.1, f64::INFINITY, f64::min),
            pick(|c| c.0, f64::NEG_INFINITY, f64::max),
            pick(|c| c.1, f64::NEG_INFINITY, f64::max),
        ]
    };
    match image.image_index {
        7 => {
            let [x, y, width, height] = crop_pattern::CLIP_RECT;
            let corners = [
                (x, y),
                (x + width, y),
                (x, y + height),
                (x + width, y + height),
            ];
            (bounds(&corners), CLIPPED_RANGE_TOLERANCE_PT)
        }
        5 => {
            // The first columns of the mask are 0, so they do not show; the
            // column before the first one that does is added.
            let first = f64::from(crop_pattern::SOFT_MASK_FIRST_VISIBLE_COLUMN - 1);
            let left = first / f64::from(PICTURE_WIDTH);
            let corners = [(left, 0.0), (1.0, 0.0), (left, 1.0), (1.0, 1.0)]
                .map(|(u, v)| crop_pattern::apply(image.matrix, u, v));
            (bounds(&corners), EXACT_PT)
        }
        _ => (expected_frame(page_index, image.matrix), EXACT_PT),
    }
}

#[test]
fn page_images_lists_the_images_with_their_frames_on_every_page() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    for page_index in 0..PAGE_COUNT {
        let images = page_images(&mut worker, page_index as u32 + 1);
        // Left out: the 2 × 2 image (index 1), the 11 × 7.6 pt frame (6) and
        // the picture inside the stamp annotation.
        let indices: Vec<u32> = images.iter().map(|i| i.image_index).collect();
        assert_eq!(indices, [0, 2, 3, 4, 5, 7, 8], "page {}", page_index + 1);

        for (image, expected) in images.iter().zip(&listed) {
            assert_eq!(
                (image.px_width, image.px_height),
                (PICTURE_WIDTH, PICTURE_HEIGHT)
            );
            assert_eq!(
                image.native, expected.native,
                "native of {}",
                image.image_index
            );
            let (frame, tolerance) = visible_frame(page_index, expected);
            let rect = image.rect_pt;
            for (got, want) in [rect.x0, rect.y0, rect.x1, rect.y1].iter().zip(frame) {
                assert!(
                    (got - want).abs() < tolerance,
                    "page {} image {}: frame {rect:?} against {frame:?}",
                    page_index + 1,
                    image.image_index
                );
            }
        }
    }
}

#[test]
fn page_images_numbers_the_images_it_leaves_out_too() {
    let mut worker = crop_align();
    let images = page_images(&mut worker, 1);
    // Index 1 and 6 are not listed, so 0 → 2 and 5 → 7 skip them.
    assert!(
        images
            .windows(2)
            .any(|w| w[1].image_index - w[0].image_index == 2)
    );
    assert_eq!(images.last().map(|i| i.image_index), Some(8));
}

#[test]
fn photos_are_written_in_their_own_pixels_on_every_page() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    // Placed by flips and quarter turns, and without anything over them.
    let plain = [1usize, 2];
    for page_index in 0..PAGE_COUNT {
        let page = page_index as u32 + 1;
        let images = page_images(&mut worker, page);
        for &which in &plain {
            let image = &images[which];
            let written = render(&mut worker, &Region::photo(page, image));
            assert!(!written.scaled_down);
            let actual = written.decode().to_rgba8();
            assert_eq!(actual.dimensions(), (written.width, written.height));
            let expected = expected_view(page_index, listed[which].matrix, picture_value);
            assert_eq!(
                differing_pixels(&actual, &expected),
                0,
                "page {page} image {}",
                image.image_index
            );
        }
    }
}

#[test]
fn an_overlay_is_in_the_photo_unless_only_the_photo_is_asked_for() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    for page_index in 0..PAGE_COUNT {
        let page = page_index as u32 + 1;
        let images = page_images(&mut worker, page);
        // Index 0 has a filled rectangle drawn over it.
        let covered = &images[0];
        let expected = expected_view(page_index, listed[0].matrix, picture_value);

        let with_overlay = render(&mut worker, &Region::photo(page, covered));
        assert!(
            differing_pixels(&with_overlay.decode().to_rgba8(), &expected) > 0,
            "page {page}: the rectangle should be in the written image"
        );

        let photo_only = render(&mut worker, &Region::photo(page, covered).photo_only());
        assert_eq!(
            differing_pixels(&photo_only.decode().to_rgba8(), &expected),
            0,
            "page {page}"
        );
    }
}

#[test]
fn only_the_photo_inside_nested_forms_and_under_a_clip_is_found() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    let images = page_images(&mut worker, 1);
    // The picture in two forms with a matrix and a cm each.
    let nested = images.iter().position(|i| i.image_index == 8).unwrap();
    let written = render(&mut worker, &Region::photo(1, &images[nested]).photo_only());
    let expected = expected_view(0, listed[nested].matrix, picture_value);
    assert_eq!(differing_pixels(&written.decode().to_rgba8(), &expected), 0);
}

/// The columns and rows (from the top) of the clipped image's picture that
/// the clip path touches: every pixel the clip rectangle overlaps at all.
fn clipped_window() -> (std::ops::Range<u32>, std::ops::Range<u32>) {
    let [clip_x, clip_y, clip_width, clip_height] = crop_pattern::CLIP_RECT;
    let m = crop_pattern::CLIPPED;
    let to_unit_x = |x: f64| (x - m[4]) / m[0];
    let to_unit_y = |y: f64| (y - m[5]) / m[3];
    let columns = |low: f64, high: f64| {
        let width = f64::from(PICTURE_WIDTH);
        (low * width).floor() as u32..(high * width).ceil() as u32
    };
    let rows = |low: f64, high: f64| {
        // Row 0 is the top of the picture, the largest y.
        let height = f64::from(PICTURE_HEIGHT);
        ((1.0 - high) * height).floor() as u32..((1.0 - low) * height).ceil() as u32
    };
    (
        columns(to_unit_x(clip_x), to_unit_x(clip_x + clip_width)),
        rows(to_unit_y(clip_y), to_unit_y(clip_y + clip_height)),
    )
}

#[test]
fn a_clipped_photo_is_written_as_the_part_the_clip_shows() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    let clipped = listed.iter().position(|i| i.image_index == 7).unwrap();
    let (columns, rows) = clipped_window();
    // Fewer than all of the picture's columns and rows.
    assert!(columns.len() < PICTURE_WIDTH as usize && rows.len() < PICTURE_HEIGHT as usize);
    let window = |column: u32, row: u32| {
        if columns.contains(&column) && rows.contains(&row) {
            picture_value(column, row)
        } else {
            [0, 0, 0, 0]
        }
    };
    for page_index in 0..PAGE_COUNT {
        let page = page_index as u32 + 1;
        let images = page_images(&mut worker, page);
        let image = images.iter().find(|i| i.image_index == 7).unwrap();
        let expected = trimmed(&expected_view(page_index, listed[clipped].matrix, window));
        for photo_only in [false, true] {
            let mut region = Region::photo(page, image);
            region.photo_only = photo_only;
            let written = render(&mut worker, &region);
            assert_eq!((written.width, written.height), expected.dimensions());
            assert_eq!(
                differing_pixels(&written.decode().to_rgba8(), &expected),
                0,
                "page {page} photo_only {photo_only}"
            );
        }
    }
}

#[test]
fn a_sheared_image_is_not_native_and_is_drawn_as_its_frame_at_the_dpi() {
    let mut worker = crop_align();
    for page in 1..=PAGE_COUNT as u32 {
        let images = page_images(&mut worker, page);
        let sheared = images.iter().find(|i| i.image_index == 4).unwrap();
        assert!(!sheared.native);

        let rect = sheared.rect_pt;
        let dpi = 150;
        let scale = f64::from(dpi) / 72.0;
        let expected_width = ((rect.x1 - rect.x0) * scale).round() as u32;
        let expected_height = ((rect.y1 - rect.y0) * scale).round() as u32;

        // Asking for its own pixels does not make it so.
        let written = render(&mut worker, &Region::photo(page, sheared));
        assert_eq!(
            (written.width, written.height),
            (expected_width, expected_height)
        );
        assert_ne!(
            (written.width, written.height),
            (PICTURE_WIDTH, PICTURE_HEIGHT)
        );
        assert!(!written.scaled_down);
        assert_eq!(
            written.decode().dimensions(),
            (expected_width, expected_height)
        );
    }
}

#[test]
fn a_photo_not_taken_in_its_own_pixels_is_drawn_at_the_dpi() {
    let mut worker = crop_align();
    let images = page_images(&mut worker, 1);
    let first = &images[0];
    let region = Region {
        photo_native: false,
        dpi: 300,
        ..Region::photo(1, first)
    };
    let written = render(&mut worker, &region);
    let rect = first.rect_pt;
    let scale = 300.0 / 72.0;
    assert_eq!(
        (written.width, written.height),
        (
            ((rect.x1 - rect.x0) * scale).round() as u32,
            ((rect.y1 - rect.y0) * scale).round() as u32
        )
    );
}

/// The soft mask's value where the photo's pixel (column, row) lands.
fn soft_masked_value(column: u32, row: u32) -> [u8; 4] {
    let [r, g, b] = crop_pattern::picture_pixel(column, row);
    [r, g, b, crop_pattern::soft_mask_alpha(column, row)]
}

#[test]
fn a_soft_masked_photo_keeps_its_alpha_in_png_and_webp() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    for page_index in 0..PAGE_COUNT {
        let page = page_index as u32 + 1;
        let images = page_images(&mut worker, page);
        let masked = images.iter().position(|i| i.image_index == 5).unwrap();
        let expected = trimmed(&expected_view(
            page_index,
            listed[masked].matrix,
            soft_masked_value,
        ));

        let mut decoded = Vec::new();
        for format in [RenderFormat::Png, RenderFormat::Webp] {
            let region = Region::photo(page, &images[masked])
                .photo_only()
                .in_format(format);
            let written = render(&mut worker, &region);
            let actual = written.decode();
            assert_eq!(actual.color(), image::ColorType::Rgba8, "{format:?}");
            let actual = actual.to_rgba8();
            assert_eq!(actual.dimensions(), expected.dimensions());
            let (mut opaque, mut alpha_off, mut colour_off) = (0, 0, 0);
            for (a, e) in actual.pixels().zip(expected.pixels()) {
                if a[3] != e[3] {
                    alpha_off += 1;
                }
                if e[3] == 255 {
                    opaque += 1;
                    if a != e {
                        colour_off += 1;
                    }
                }
            }
            assert!(opaque > 0, "the mask has opaque pixels");
            assert_eq!(alpha_off, 0, "page {page} {format:?}: alpha off the mask");
            assert_eq!(colour_off, 0, "page {page} {format:?}: opaque colours");
            decoded.push(actual);
        }
        // The lossless WebP carries the same pixels as the PNG.
        assert_eq!(differing_pixels(&decoded[0], &decoded[1]), 0);
    }
}

#[test]
fn a_soft_masked_photo_has_a_white_background_in_jpeg() {
    let mut worker = crop_align();
    let images = page_images(&mut worker, 1);
    let masked = images.iter().find(|i| i.image_index == 5).unwrap();
    let region = Region::photo(1, masked)
        .photo_only()
        .in_format(RenderFormat::Jpeg);
    let written = render(&mut worker, &region);
    let decoded = written.decode();
    assert_eq!(decoded.color(), image::ColorType::Rgb8);
    // The top-left pixel of the picture has alpha 0; its page is upright.
    let [r, g, b] = decoded.to_rgb8().get_pixel(0, 0).0;
    assert!(r >= 240 && g >= 240 && b >= 240, "{r} {g} {b}");
}

#[test]
fn a_gray_photo_only_png_has_gray_and_alpha_channels() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    let images = page_images(&mut worker, 1);
    let masked = images.iter().position(|i| i.image_index == 5).unwrap();
    let region = Region::photo(1, &images[masked])
        .photo_only()
        .in_color(RenderColor::Gray);
    let written = render(&mut worker, &region);
    let decoded = written.decode();
    assert_eq!(decoded.color(), image::ColorType::La8);
    let expected = trimmed(&expected_view(0, listed[masked].matrix, soft_masked_value));
    let actual = decoded.to_luma_alpha8();
    let alpha_off = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, e)| a[1] != e[3])
        .count();
    assert_eq!(alpha_off, 0);
}

#[test]
fn every_format_and_colour_writes_a_photo_and_a_rectangle() {
    let mut worker = crop_align();
    let listed = crop_pattern::listed_images();
    let images = page_images(&mut worker, 1);
    let masked = images.iter().position(|i| i.image_index == 5).unwrap();
    let expected = trimmed(&expected_view(0, listed[masked].matrix, soft_masked_value));
    for format in [RenderFormat::Png, RenderFormat::Jpeg, RenderFormat::Webp] {
        for color in [RenderColor::Color, RenderColor::Gray] {
            let photo = Region::photo(1, &images[masked])
                .photo_only()
                .in_format(format)
                .in_color(color);
            let written = render(&mut worker, &photo);
            let decoded = written.decode();
            assert_eq!(decoded.dimensions(), expected.dimensions());
            // JPEG has no alpha; the others keep the mask.
            let has_alpha = format != RenderFormat::Jpeg;
            assert_eq!(
                decoded.color().has_alpha(),
                has_alpha,
                "{format:?} {color:?}"
            );
            if has_alpha {
                let alpha_off = decoded
                    .to_rgba8()
                    .pixels()
                    .zip(expected.pixels())
                    .filter(|(a, e)| a[3] != e[3])
                    .count();
                assert_eq!(alpha_off, 0, "{format:?} {color:?}");
            }

            let rectangle = Region::rectangle(1, images[masked].rect_pt, 100)
                .in_format(format)
                .in_color(color);
            let written = render(&mut worker, &rectangle);
            assert!(!written.scaled_down);
            assert_eq!(
                written.decode().dimensions(),
                (written.width, written.height)
            );
        }
    }
}

#[test]
fn a_photo_only_png_stays_transparent_where_the_mask_is() {
    // The top-left pixel of the picture has alpha 0 and must not be painted
    // white by a white background.
    let mut worker = crop_align();
    let images = page_images(&mut worker, 1);
    let masked = images.iter().find(|i| i.image_index == 5).unwrap();
    let written = render(&mut worker, &Region::photo(1, masked).photo_only());
    assert_eq!(written.decode().to_rgba8().get_pixel(0, 0)[3], 0);
}

#[test]
fn image_on_top_is_true_only_where_an_object_covers_the_image() {
    let mut worker = crop_align();
    for page in 1..=PAGE_COUNT as u32 {
        let images = page_images(&mut worker, page);
        for image in &images {
            // Index 0 has a rectangle over it, and so has the sheared index 4
            // (checked over its frame). Index 5 has one over the part that
            // is only partly opaque, which the photo does not cover.
            let expected = matches!(image.image_index, 0 | 4);
            assert_eq!(
                image_on_top(&mut worker, page, image.image_index),
                expected,
                "page {page} image {}",
                image.image_index
            );
        }
    }
}

#[test]
fn a_region_is_the_same_crop_of_the_page_as_render_makes() {
    // Rectangles on whole pixels of a 150 dpi render: 48 pt is 100 px.
    let mut worker = started_with("shapes.pdf");
    let (response, page) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Png,
                color: RenderColor::Color,
            },
            RENDER_TIMEOUT,
        )
        .expect("Render should succeed");
    assert_eq!(response, Response::Render);
    let page = image::load_from_memory(&page).unwrap().to_rgb8();

    let rect = RectPt {
        x0: 48.0,
        y0: 96.0,
        x1: 384.0,
        y1: 528.0,
    };
    let written = render(&mut worker, &Region::rectangle(1, rect, 150));
    assert_eq!((written.width, written.height), (700, 900));
    let region = written.decode().to_rgb8();
    let cropped = image::imageops::crop_imm(&page, 100, 200, 700, 900).to_image();
    let ratio = mismatch_ratio(&region, &cropped);
    assert!(ratio <= MAX_MISMATCH_RATIO_RENDER, "mismatch ratio {ratio}");
}

/// Tolerance for pixel colour differences against a render (design §11.2).
const COLOR_TOLERANCE_RENDER: u8 = 32;

/// Largest share of pixels that may differ by more than the tolerance (design §11.2).
const MAX_MISMATCH_RATIO_RENDER: f64 = 0.005;

fn mismatch_ratio(actual: &image::RgbImage, expected: &image::RgbImage) -> f64 {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let differing = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, e)| {
            a.0.iter()
                .zip(e.0)
                .any(|(a, e)| a.abs_diff(e) > COLOR_TOLERANCE_RENDER)
        })
        .count();
    differing as f64 / (actual.width() * actual.height()) as f64
}

#[test]
fn a_region_off_the_pixel_grid_still_matches_the_page_render() {
    let mut worker = started_with("shapes.pdf");
    let (_, page) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Png,
                color: RenderColor::Color,
            },
            RENDER_TIMEOUT,
        )
        .expect("Render should succeed");
    let page = image::load_from_memory(&page).unwrap().to_rgb8();

    // 50.4 pt is 105 px and 100.8 pt is 210 px, but the sides are not whole.
    let rect = RectPt {
        x0: 50.4,
        y0: 100.8,
        x1: 380.9,
        y1: 520.3,
    };
    let written = render(&mut worker, &Region::rectangle(1, rect, 150));
    let region = written.decode().to_rgb8();
    let cropped =
        image::imageops::crop_imm(&page, 105, 210, region.width(), region.height()).to_image();
    let ratio = mismatch_ratio(&region, &cropped);
    assert!(ratio <= MAX_MISMATCH_RATIO_RENDER, "mismatch ratio {ratio}");
}

#[test]
fn a_region_of_a_turned_page_is_cut_from_the_shown_page() {
    // Page 2 of crop_align.pdf is turned a quarter. At 72 dpi one point is
    // one pixel, so whole-point corners are whole-pixel corners.
    let mut worker = crop_align();
    let (_, page) = worker
        .request(
            &Request::Render {
                page: 2,
                dpi: 72,
                format: RenderFormat::Png,
                color: RenderColor::Color,
            },
            RENDER_TIMEOUT,
        )
        .expect("Render should succeed");
    let page = image::load_from_memory(&page).unwrap().to_rgb8();

    let rect = RectPt {
        x0: 60.0,
        y0: 50.0,
        x1: 460.0,
        y1: 400.0,
    };
    let written = render(&mut worker, &Region::rectangle(2, rect, 72));
    assert_eq!((written.width, written.height), (400, 350));
    let region = written.decode().to_rgb8();
    let cropped = image::imageops::crop_imm(&page, 60, 50, 400, 350).to_image();
    let ratio = mismatch_ratio(&region, &cropped);
    assert!(ratio <= MAX_MISMATCH_RATIO_RENDER, "mismatch ratio {ratio}");
}

#[test]
fn a_region_draws_annotations_like_the_page_render() {
    // Page 1 of crop_align.pdf has a stamp over the top-right of its first
    // image, about 4% of this rectangle, so leaving it out fails the ratio.
    // One point is one pixel at 72 dpi.
    let mut worker = crop_align();
    let (_, page) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 72,
                format: RenderFormat::Png,
                color: RenderColor::Color,
            },
            RENDER_TIMEOUT,
        )
        .expect("Render should succeed");
    let page = image::load_from_memory(&page).unwrap().to_rgb8();

    let rect = RectPt {
        x0: 80.0,
        y0: 100.0,
        x1: 300.0,
        y1: 240.0,
    };
    let written = render(&mut worker, &Region::rectangle(1, rect, 72));
    let region = written.decode().to_rgb8();
    let cropped = image::imageops::crop_imm(&page, 80, 100, 220, 140).to_image();
    let ratio = mismatch_ratio(&region, &cropped);
    assert!(ratio <= MAX_MISMATCH_RATIO_RENDER, "mismatch ratio {ratio}");
}

#[test]
fn a_region_over_the_pixel_limit_is_too_large() {
    // Page 2 of mixed_sizes.pdf is 3000 × 3000 pt: 156 million pixels at 300 dpi.
    let mut worker = started_with("mixed_sizes.pdf");
    let whole = RectPt {
        x0: 0.0,
        y0: 0.0,
        x1: 3000.0,
        y1: 3000.0,
    };
    expect_region_error(
        &mut worker,
        &Region::rectangle(2, whole, 300),
        "RenderTooLarge",
    );
    // A part of it that fits is drawn.
    let part = RectPt {
        x1: 1000.0,
        y1: 1000.0,
        ..whole
    };
    let written = render(&mut worker, &Region::rectangle(2, part, 300));
    assert_eq!((written.width, written.height), (4167, 4167));
}

#[test]
fn bad_regions_are_invalid_params() {
    let mut worker = crop_align();
    let rect = RectPt {
        x0: 50.0,
        y0: 50.0,
        x1: 200.0,
        y1: 200.0,
    };
    let ok = Region::rectangle(1, rect, 72);
    render(&mut worker, &ok);

    for bad_rect in [
        RectPt { x1: 50.0, ..rect },
        RectPt { y1: 40.0, ..rect },
        RectPt { x0: -0.6, ..rect },
        RectPt { y0: -0.6, ..rect },
        // The shown page of page 1 is 579.8 × 822.5 pt.
        RectPt { x1: 580.4, ..rect },
        RectPt { y1: 823.1, ..rect },
    ] {
        expect_region_error(
            &mut worker,
            &Region {
                rect: bad_rect,
                ..ok.clone()
            },
            "InvalidParams",
        );
    }
    // Half a point outside is allowed.
    render(
        &mut worker,
        &Region {
            rect: RectPt {
                x0: -0.4,
                y1: 822.9,
                ..rect
            },
            ..ok.clone()
        },
    );

    for page in [0, 4] {
        expect_region_error(&mut worker, &Region { page, ..ok.clone() }, "InvalidParams");
    }
    for dpi in [0, 35, 1001] {
        expect_region_error(&mut worker, &Region { dpi, ..ok.clone() }, "InvalidParams");
    }
}

#[test]
fn bad_image_indices_are_invalid_params() {
    let mut worker = crop_align();
    let images = page_images(&mut worker, 1);
    let photo = Region::photo(1, &images[0]);
    // 1 is the 2 × 2 image and 6 the 11 pt frame, both drawn but not listed;
    // 9 and above do not exist.
    for image_index in [1, 6, 9, 1000] {
        expect_region_error(
            &mut worker,
            &Region {
                image_index: Some(image_index),
                ..photo.clone()
            },
            "InvalidParams",
        );
        expect_error(
            worker.request(
                &Request::ImageOnTop {
                    page: 1,
                    image_index,
                },
                RENDER_TIMEOUT,
            ),
            "InvalidParams",
        );
    }
}

#[test]
fn the_dpi_range_does_not_apply_to_a_photo_in_its_own_pixels() {
    let mut worker = crop_align();
    let images = page_images(&mut worker, 1);
    let native = Region {
        dpi: 0,
        ..Region::photo(1, &images[0])
    };
    render(&mut worker, &native);
    // But it does when the photo is drawn at a dpi, by choice or because of its placement.
    let by_choice = Region {
        photo_native: false,
        ..native.clone()
    };
    expect_region_error(&mut worker, &by_choice, "InvalidParams");
    let sheared = images.iter().find(|i| !i.native).unwrap();
    expect_region_error(
        &mut worker,
        &Region {
            dpi: 0,
            ..Region::photo(1, sheared)
        },
        "InvalidParams",
    );
}

#[test]
fn crop_requests_need_an_open_pdf_and_a_page() {
    let mut worker = start();
    let rect = RectPt {
        x0: 0.0,
        y0: 0.0,
        x1: 10.0,
        y1: 10.0,
    };
    expect_error(
        worker.request(&Request::PageImages { page: 1 }, RENDER_TIMEOUT),
        "InvalidParams",
    );
    expect_error(
        worker.request(
            &Request::ImageOnTop {
                page: 1,
                image_index: 0,
            },
            RENDER_TIMEOUT,
        ),
        "InvalidParams",
    );
    expect_region_error(
        &mut worker,
        &Region::rectangle(1, rect, 72),
        "InvalidParams",
    );

    let mut worker = crop_align();
    for page in [0, 4] {
        expect_error(
            worker.request(&Request::PageImages { page }, RENDER_TIMEOUT),
            "InvalidParams",
        );
        expect_error(
            worker.request(
                &Request::ImageOnTop {
                    page,
                    image_index: 0,
                },
                RENDER_TIMEOUT,
            ),
            "InvalidParams",
        );
    }
}

#[test]
fn thumbnails_are_limited_to_the_page_view_size() {
    let mut worker = started_with("shapes.pdf");
    let at = |max_side| Request::Thumbnail { page: 1, max_side };
    let (response, _) = worker
        .request(&at(PAGE_VIEW_MAX_SIDE), RENDER_TIMEOUT)
        .expect("the limit itself is allowed");
    assert_eq!(response, Response::Thumbnail);
    expect_error(
        worker.request(&at(PAGE_VIEW_MAX_SIDE + 1), RENDER_TIMEOUT),
        "InvalidParams",
    );
}

/// Packs bits into bytes, least significant bit first, as deflate does.
struct BitWriter {
    bytes: Vec<u8>,
    used_bits: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            used_bits: 0,
        }
    }

    /// Writes the `count` low bits of `value`, lowest first.
    fn bits(&mut self, value: u32, count: u32) {
        for index in 0..count {
            if self.used_bits == 0 {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= (((value >> index) & 1) as u8) << self.used_bits;
            self.used_bits = (self.used_bits + 1) % 8;
        }
    }

    /// Writes a Huffman code of `count` bits, which deflate packs highest first.
    fn code(&mut self, code: u32, count: u32) {
        for index in (0..count).rev() {
            self.bits((code >> index) & 1, 1);
        }
    }
}

/// A zlib stream of `length` zero bytes, written by hand with fixed Huffman
/// codes so that a test can make a huge blank image without a compressor: one
/// literal, then runs of 258 copies of the byte before it.
fn zero_zlib_stream(length: u64) -> Vec<u8> {
    const MAX_RUN: u64 = 258;
    const ADLER_MODULUS: u64 = 65_521;
    let mut writer = BitWriter::new();
    writer.bits(1, 1); // the last block
    writer.bits(1, 2); // fixed Huffman codes
    let literal_zero = |writer: &mut BitWriter| writer.code(0b0011_0000, 8);
    literal_zero(&mut writer);
    let after_first = length - 1;
    for _ in 0..after_first / MAX_RUN {
        writer.code(0b1100_0101, 8); // length 258
        writer.code(0, 5); // distance 1
    }
    for _ in 0..after_first % MAX_RUN {
        literal_zero(&mut writer);
    }
    writer.code(0, 7); // end of block
    let adler = (((length % ADLER_MODULUS) as u32) << 16) | 1;
    let mut stream = vec![0x78, 0x01];
    stream.extend(writer.bytes);
    stream.extend(adler.to_be_bytes());
    stream
}

/// A one-page PDF (612 × 792 pt) that draws one black RGB image of
/// `width` × `height` pixels at each of `placements` (`cm` operands).
fn blank_image_pdf(width: u32, height: u32, placements: &[Matrix]) -> Vec<u8> {
    blank_image_pdf_over("", width, height, placements)
}

/// [`blank_image_pdf`] with the content stream `underneath` drawn first.
fn blank_image_pdf_over(
    underneath: &str,
    width: u32,
    height: u32,
    placements: &[Matrix],
) -> Vec<u8> {
    let placed: String = placements
        .iter()
        .map(|m| {
            format!(
                "q {} {} {} {} {} {} cm /Im Do Q\n",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
        })
        .collect();
    let content = format!("{underneath}{placed}");
    let image = zero_zlib_stream(u64::from(width) * u64::from(height) * 3);
    let stream = |entries: &str, data: &[u8]| {
        [
            format!("<< {entries} /Length {} >>\nstream\n", data.len()).into_bytes(),
            data.to_vec(),
            b"\nendstream".to_vec(),
        ]
        .concat()
    };
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
          /Resources << /XObject << /Im 5 0 R >> >> >>"
            .to_vec(),
        stream("", content.as_bytes()),
        stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {width} /Height {height} \
                 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode"
            ),
            &image,
        ),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", index + 1).into_bytes());
        pdf.extend(body);
        pdf.extend(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for offset in offsets {
        pdf.extend(format!("{offset:010} 00000 n \n").into_bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .into_bytes(),
    );
    pdf
}

/// Writes `pdf` to a file of its own in the temporary directory and opens it
/// in a new worker. The file goes when the value does.
struct TemporaryPdf {
    path: PathBuf,
    worker: WorkerProcess,
}

impl TemporaryPdf {
    fn open(name: &str, pdf: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!("pdfconv-{name}-{}.pdf", std::process::id()));
        std::fs::write(&path, pdf).expect("the temporary PDF should be written");
        let mut worker = start();
        let opened = worker.request(
            &Request::Open {
                path: path.clone(),
                password: None,
            },
            RENDER_TIMEOUT,
        );
        assert!(opened.is_ok(), "opening {name}: {opened:?}");
        Self { path, worker }
    }
}

impl Drop for TemporaryPdf {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn a_zero_image_stream_decodes_to_zeros() {
    // The hand-made deflate stream is what the large-photo test stands on.
    let mut pdf = TemporaryPdf::open(
        "zero-stream",
        &blank_image_pdf(300, 200, &[[300.0, 0.0, 0.0, 200.0, 100.0, 100.0]]),
    );
    let images = page_images(&mut pdf.worker, 1);
    assert_eq!(
        images
            .iter()
            .map(|i| (i.px_width, i.px_height))
            .collect::<Vec<_>>(),
        [(300, 200)]
    );
    let photo = render(&mut pdf.worker, &Region::photo(1, &images[0]).photo_only());
    let pixels = photo.decode().to_rgba8();
    assert_eq!(pixels.dimensions(), (300, 200));
    assert!(pixels.pixels().all(|p| p.0 == [0, 0, 0, 255]));
}

/// The peak resident memory of a process so far, in bytes, where the
/// operating system tells (`VmHWM` in `/proc/<pid>/status` on Linux).
fn peak_memory(process_id: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{process_id}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kibibytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kibibytes * 1024)
}

/// Run with
/// `cargo test -p pdfconv-worker --test crop --release -- --ignored --nocapture a_photo_near_the_pixel_limit`
/// to see the times and the memory it prints.
#[test]
#[ignore = "builds and renders a 90 million pixel image, which takes a while and a lot of memory"]
fn a_photo_near_the_pixel_limit_fits_in_the_worker_memory_limit() {
    const SIDE: u32 = 9_500;
    let mut pdf = TemporaryPdf::open(
        "big-photo",
        &blank_image_pdf(SIDE, SIDE, &[[500.0, 0.0, 0.0, 500.0, 50.0, 100.0]]),
    );
    let report = |what: &str, started: std::time::Instant, worker: &WorkerProcess| {
        println!(
            "{what}: {:.1} s, worker peak {} MiB",
            started.elapsed().as_secs_f64(),
            peak_memory(worker.id()).map_or(-1, |bytes| (bytes / (1024 * 1024)) as i64)
        );
    };

    let started = std::time::Instant::now();
    let images = page_images(&mut pdf.worker, 1);
    report("PageImages", started, &pdf.worker);
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].px_width, images[0].px_height), (SIDE, SIDE));

    for photo_only in [false, true] {
        let mut region = Region::photo(1, &images[0]);
        region.photo_only = photo_only;
        let started = std::time::Instant::now();
        let written = try_render(&mut pdf.worker, &region);
        report(
            &format!("RenderRegion photo_only={photo_only}"),
            started,
            &pdf.worker,
        );
        let written = written.expect("the worker should survive the photo");
        assert_eq!((written.width, written.height), (SIDE, SIDE));
        assert!(!written.scaled_down);
    }
}

#[test]
fn a_page_of_a_hundred_small_images_is_listed() {
    // Twenty by twenty pixels and points, ten to a row: all large enough to be listed.
    let placements: Vec<Matrix> = (0..100)
        .map(|i| {
            let (column, row) = (f64::from(i % 10), f64::from(i / 10));
            [
                20.0,
                0.0,
                0.0,
                20.0,
                30.0 + column * 40.0,
                30.0 + row * 40.0,
            ]
        })
        .collect();
    let mut pdf = TemporaryPdf::open("hundred", &blank_image_pdf(20, 20, &placements));
    let started = std::time::Instant::now();
    let images = page_images(&mut pdf.worker, 1);
    println!(
        "PageImages of 100 images: {:.1} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(images.len(), 100);
    assert_eq!(
        images.iter().map(|i| i.image_index).collect::<Vec<_>>(),
        (0..100).collect::<Vec<_>>()
    );

    let mut worker = crop_align();
    let started = std::time::Instant::now();
    page_images(&mut worker, 1);
    println!(
        "PageImages of crop_align.pdf page 1: {:.1} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

/// How many times longer `PageImages` may take on a page with twice as many
/// objects: about 2 when cutting out each image takes time in proportion to
/// the objects, about 4 when it takes their square.
const DOUBLED_OBJECTS_MAX_TIME_RATIO: f64 = 3.0;

/// Fifty listed images drawn over `squares` small filled squares, each square
/// a page object of its own.
fn images_over_squares_pdf(squares: usize) -> Vec<u8> {
    let underneath: String = (0..squares)
        .map(|i| format!("{} {} 3 3 re f\n", (i * 7) % 600, (i * 13) % 780))
        .collect();
    let placements: Vec<Matrix> = (0..50)
        .map(|i| {
            let (column, row) = (f64::from(i % 10), f64::from(i / 10));
            [
                40.0,
                0.0,
                0.0,
                40.0,
                30.0 + column * 55.0,
                30.0 + row * 55.0,
            ]
        })
        .collect();
    blank_image_pdf_over(&underneath, 20, 20, &placements)
}

/// The shorter of two `PageImages` of page 1, in seconds, after checking that
/// all fifty images are listed.
fn page_images_seconds(pdf: &mut TemporaryPdf) -> f64 {
    (0..2)
        .map(|_| {
            let started = std::time::Instant::now();
            let images = page_images(&mut pdf.worker, 1);
            let seconds = started.elapsed().as_secs_f64();
            assert_eq!(images.len(), 50);
            seconds
        })
        .fold(f64::INFINITY, f64::min)
}

/// Run with
/// `cargo test -p pdfconv-worker --test crop --release -- --ignored --nocapture page_images_time_grows`
/// to see the times it prints.
#[test]
#[ignore = "times PageImages on pages of tens of thousands of objects, which takes a while and depends on the machine"]
fn page_images_time_grows_in_proportion_to_the_objects() {
    const SQUARES: usize = 20_000;
    let mut fewer = TemporaryPdf::open("squares", &images_over_squares_pdf(SQUARES));
    let mut more = TemporaryPdf::open("more-squares", &images_over_squares_pdf(2 * SQUARES));
    let (fewer_seconds, more_seconds) = (
        page_images_seconds(&mut fewer),
        page_images_seconds(&mut more),
    );
    println!(
        "PageImages of 50 images over {SQUARES} squares: {fewer_seconds:.2} s, over {}: {more_seconds:.2} s",
        2 * SQUARES
    );
    assert!(
        more_seconds < fewer_seconds * DOUBLED_OBJECTS_MAX_TIME_RATIO,
        "doubling the objects took {:.1} times as long",
        more_seconds / fewer_seconds
    );
}
