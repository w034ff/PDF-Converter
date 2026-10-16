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

use crop_pattern::{Matrix, PICTURE_HEIGHT, PICTURE_WIDTH};

fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/pdfium")
        .join(os_dir)
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures")
}

fn start() -> WorkerProcess {
    WorkerProcess::spawn(
        env!("CARGO_BIN_EXE_pdfconv-worker").as_ref(),
        [library_dir()],
    )
    .expect("worker should start")
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
            let frame = expected_frame(page_index, expected.matrix);
            let rect = image.rect_pt;
            for (got, want) in [rect.x0, rect.y0, rect.x1, rect.y1].iter().zip(frame) {
                assert!(
                    (got - want).abs() < 0.01,
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
        let expected = expected_view(page_index, listed[masked].matrix, soft_masked_value);

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
    let expected = expected_view(0, listed[masked].matrix, soft_masked_value);
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
    let expected = expected_view(0, listed[masked].matrix, soft_masked_value);
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
