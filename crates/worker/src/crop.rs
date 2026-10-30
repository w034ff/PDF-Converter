//! The worker's side of cropping (design §4.6, §5.1): a page's embedded
//! images, whether something covers one, and rendering one range of a page.
//!
//! Ranges are rectangles in points in the orientation the page is shown in
//! (after `/Rotate`, origin top-left). pdfium-render's matrix is applied after
//! pdfium's own page-to-bitmap matrix, so the matrices built here first undo or
//! measure that one ([`display_matrix`]) instead of rebuilding it from
//! `/Rotate` and `CropBox`.

use std::collections::BTreeSet;

use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfMatrix, PdfPage, PdfPageContentRegenerationStrategy, PdfPageObject,
    PdfPageObjectsCommon, PdfPoints, PdfRenderConfig,
};

use crate::is_render_dpi_in_range;
use crate::protocol::{EmbeddedImage, NativeSize, RectPt, RenderColor, RenderFormat, Response};
use crate::server::{
    CODE_INVALID_PARAMS, CODE_PDF_OPEN_FAILED, CODE_RENDER_TOO_LARGE, encode_pixels,
    error_response, load_page, pixels_from_rgba,
};

/// Device pixels per point when measuring the display matrix. Large, so that
/// pdfium's integer device coordinates are exact to a hundred-thousandth of a
/// point (design §4.6, 前段の座標の測り方).
pub const DISPLAY_PROBE_SCALE: f64 = 100_000.0;

/// How far each edge of an image's pixel grid is pulled inwards, in output
/// pixels. pdfium rounds an image's destination rectangle outwards to whole
/// pixels, so a floating-point error past an edge would stretch the image by
/// a pixel; this keeps the rectangle on exactly the grid (design §4.6).
pub const PHOTO_EDGE_INSET: f64 = 0.001;

/// Images with a side shorter than this many pixels are not listed: they are
/// the 2 × 2 transparent images that slides export by the hundred (design
/// §4.6).
pub const MIN_PHOTO_SIDE_PX: u32 = 16;

/// Images whose frame on the page has a side shorter than this many points
/// are not listed, for the same reason as [`MIN_PHOTO_SIDE_PX`].
pub const MIN_PHOTO_SIDE_PT: f64 = 12.0;

/// How much a colour channel of the page may differ from the image alone
/// before the pixel counts as changed by something on top (design §4.6).
pub const ON_TOP_COLOR_TOLERANCE: u8 = 8;

/// How many changed pixels (see [`ON_TOP_COLOR_TOLERANCE`]) make an image
/// count as having something on top (design §4.6).
pub const ON_TOP_MIN_PIXELS: usize = 20;

/// Upper bound on the pixels of each of the renders that decide whether
/// something is on top, so the check stays fast on large images (design
/// §4.6).
pub const ON_TOP_CHECK_PIXELS: u64 = 4_000_000;

/// Upper bound on the pixels of the two renders that find the part of an
/// image that shows (design §4.6), so listing a page stays fast.
pub const VISIBLE_CHECK_PIXELS: u64 = 250_000;

/// How far outside the page a range may reach, in points (design §4.6).
pub const RANGE_OUTSIDE_TOLERANCE_PT: f64 = 0.5;

/// How much of a grid pixel, as a fraction, the page edge may leave on the
/// page side before that pixel counts as on the page. The edge is mapped
/// through floating-point matrices, so an edge that lies on a pixel boundary
/// would otherwise take in the pixel next to it (design §4.6).
pub const PAGE_EDGE_TOLERANCE_PX: f64 = 0.01;

/// The relative size below which a matrix entry counts as zero when deciding
/// whether an image is placed by flips and quarter turns only. Placement
/// matrices are written with a few decimals, so float noise stays far below.
const NATIVE_RELATIVE_TOLERANCE: f64 = 1e-6;

/// An affine matrix in PDF's order, `[a b c d e f]`: x' = a·x + c·y + e,
/// y' = b·x + d·y + f.
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `first` applied, then `next`.
fn then(first: Matrix, next: Matrix) -> Matrix {
    [
        first[0] * next[0] + first[1] * next[2],
        first[0] * next[1] + first[1] * next[3],
        first[2] * next[0] + first[3] * next[2],
        first[2] * next[1] + first[3] * next[3],
        first[4] * next[0] + first[5] * next[2] + next[4],
        first[4] * next[1] + first[5] * next[3] + next[5],
    ]
}

fn invert(m: Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det == 0.0 {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)])
}

fn apply(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn from_pdf(m: &PdfMatrix) -> Matrix {
    [
        f64::from(m.a()),
        f64::from(m.b()),
        f64::from(m.c()),
        f64::from(m.d()),
        f64::from(m.e()),
        f64::from(m.f()),
    ]
}

fn to_pdf(m: Matrix) -> PdfMatrix {
    PdfMatrix::new(
        m[0] as f32,
        m[1] as f32,
        m[2] as f32,
        m[3] as f32,
        m[4] as f32,
        m[5] as f32,
    )
}

/// Page space → the space pdfium-render's matrix is applied in: the page as
/// shown (after `/Rotate`) at one point per pixel, origin top-left (design
/// §4.6).
///
/// Measured with `points_to_pixels` on a page [`DISPLAY_PROBE_SCALE`] times
/// larger, so no case for `/Rotate` or `CropBox` is needed. The page size is
/// not rounded to whole points: on a page such as A4 (595.276 pt) that would
/// shift the result by whole pixels.
fn display_matrix(page: &PdfPage<'_>) -> Result<Matrix, Response> {
    let width = f64::from(page.width().value);
    let height = f64::from(page.height().value);
    // Keeps the probe's pixel size within i32 on very large pages.
    let scale = DISPLAY_PROBE_SCALE.min(f64::from(i32::MAX / 2) / width.max(height).max(1.0));
    let probe_width = (width * scale).round() as i32;
    let probe_height = (height * scale).round() as i32;
    let config = PdfRenderConfig::new().set_fixed_size(probe_width, probe_height);
    let probe = |x: f32, y: f32| -> Result<(f64, f64), Response> {
        let (device_x, device_y) = page
            .points_to_pixels(PdfPoints::new(x), PdfPoints::new(y), &config)
            .map_err(|e| error_response(CODE_PDF_OPEN_FAILED, Some(format!("{e:?}"))))?;
        Ok((
            f64::from(device_x) * width / f64::from(probe_width),
            f64::from(device_y) * height / f64::from(probe_height),
        ))
    };
    let origin = probe(0.0, 0.0)?;
    let along_x = probe(100.0, 0.0)?;
    let along_y = probe(0.0, 100.0)?;
    Ok([
        (along_x.0 - origin.0) / 100.0,
        (along_x.1 - origin.1) / 100.0,
        (along_y.0 - origin.0) / 100.0,
        (along_y.1 - origin.1) / 100.0,
        origin.0,
        origin.1,
    ])
}

/// An image met while walking a page, with everything needed to list it,
/// render it and isolate it.
struct PlacedImage {
    image_index: u32,
    /// Object indices from the page down through nested forms to the image.
    path: Vec<usize>,
    /// The image's unit square → page space, through the enclosing forms.
    unit_to_page: Matrix,
    px_width: u32,
    px_height: u32,
}

/// Collects the images of a page in drawing order: the page's own objects,
/// and inside a form its objects, depth first. Annotations are not walked:
/// the position of an image in an appearance stream cannot be told here.
fn walk_images(page: &PdfPage<'_>) -> Vec<PlacedImage> {
    fn visit(
        object: &PdfPageObject<'_>,
        outer: Matrix,
        path: &mut Vec<usize>,
        counter: &mut u32,
        found: &mut Vec<PlacedImage>,
    ) {
        match object {
            PdfPageObject::Image(image) => {
                let image_index = *counter;
                *counter += 1;
                if let (Ok(matrix), Ok(width), Ok(height)) =
                    (image.matrix(), image.width(), image.height())
                {
                    found.push(PlacedImage {
                        image_index,
                        path: path.clone(),
                        unit_to_page: then(from_pdf(&matrix), outer),
                        px_width: u32::try_from(width).unwrap_or(0),
                        px_height: u32::try_from(height).unwrap_or(0),
                    });
                }
            }
            PdfPageObject::XObjectForm(form) => {
                let inner = then(form.matrix().map_or(IDENTITY, |m| from_pdf(&m)), outer);
                for index in 0..form.len() {
                    if let Ok(child) = form.get(index) {
                        path.push(index);
                        visit(&child, inner, path, counter, found);
                        path.pop();
                    }
                }
            }
            _ => {}
        }
    }

    let mut found = Vec::new();
    let mut counter = 0;
    for index in 0..page.objects().len() {
        if let Ok(object) = page.objects().get(index) {
            visit(
                &object,
                IDENTITY,
                &mut vec![index],
                &mut counter,
                &mut found,
            );
        }
    }
    found
}

/// The frame of a unit square mapped by `unit_to_display`: its bounding box
/// cut to the page. `None` when nothing of it is on the page.
fn frame_on_page(unit_to_display: Matrix, page_width: f64, page_height: f64) -> Option<RectPt> {
    let corners =
        [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(x, y)| apply(unit_to_display, x, y));
    let x0 = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let x1 = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let y0 = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let y1 = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let frame = RectPt {
        x0: x0.max(0.0),
        y0: y0.max(0.0),
        x1: x1.min(page_width),
        y1: y1.min(page_height),
    };
    (frame.x1 > frame.x0 && frame.y1 > frame.y0).then_some(frame)
}

/// Whether a linear part `[a b c d]` only flips and turns by quarter turns:
/// either `b` and `c` or `a` and `d` vanish next to the largest entry.
fn is_quarter_turn_placement(m: Matrix) -> bool {
    let largest = m[..4].iter().fold(0.0_f64, |acc, v| acc.max(v.abs()));
    if largest == 0.0 || !largest.is_finite() {
        return false;
    }
    let negligible = |v: f64| v.abs() <= largest * NATIVE_RELATIVE_TOLERANCE;
    (negligible(m[1]) && negligible(m[2])) || (negligible(m[0]) && negligible(m[3]))
}

/// An image of a page with its placement. Until [`visible_image`] has run,
/// `info.rect_pt` is the whole frame; `info.native_size` is set by
/// [`page_images`] only.
struct ListedImage {
    placed: PlacedImage,
    info: EmbeddedImage,
    /// The frame of the whole image on the shown page, cut to the page.
    frame: RectPt,
    /// Image pixel (column, row from the top) → shown page, 1 pt = 1.
    pixel_to_display: Matrix,
}

impl ListedImage {
    /// Whether the image is placed by flips and quarter turns only, so that
    /// it can be written in its own pixels.
    fn is_native(&self) -> bool {
        is_quarter_turn_placement(self.pixel_to_display)
    }

    /// The size the image is written at in its own pixels with at most
    /// `max_pixels`, whole and in the orientation it is shown in; `None`
    /// unless [`Self::is_native`]. [`render_region`] writes the same size for
    /// an image that nothing hides: the grid of [`image_grid`] is sized by
    /// [`photo_output_size`] and then turned by [`Orientation::of`].
    fn native_size(&self, max_pixels: u64) -> Option<NativeSize> {
        if !self.is_native() {
            return None;
        }
        let (grid_width, grid_height, scaled_down) =
            photo_output_size(self.placed.px_width, self.placed.px_height, max_pixels);
        let (width, height) =
            Orientation::of(self.pixel_to_display).output_size(grid_width, grid_height);
        Some(NativeSize {
            width,
            height,
            scaled_down,
        })
    }
}

/// The images of a page that are large enough in pixels and lie on the page,
/// with their whole frames. Which of them show, and where, is decided by
/// [`visible_image`].
fn candidate_images(page: &PdfPage<'_>) -> Result<Vec<ListedImage>, Response> {
    let display = display_matrix(page)?;
    let page_width = f64::from(page.width().value);
    let page_height = f64::from(page.height().value);
    let mut candidates = Vec::new();
    for placed in walk_images(page) {
        if placed.px_width < MIN_PHOTO_SIDE_PX || placed.px_height < MIN_PHOTO_SIDE_PX {
            continue;
        }
        let unit_to_display = then(placed.unit_to_page, display);
        let Some(frame) = frame_on_page(unit_to_display, page_width, page_height) else {
            continue;
        };
        let pixel_to_unit = [
            1.0 / f64::from(placed.px_width),
            0.0,
            0.0,
            -1.0 / f64::from(placed.px_height),
            0.0,
            1.0,
        ];
        let pixel_to_display = then(pixel_to_unit, unit_to_display);
        let info = EmbeddedImage {
            image_index: placed.image_index,
            rect_pt: frame,
            px_width: placed.px_width,
            px_height: placed.px_height,
            native_size: None,
        };
        candidates.push(ListedImage {
            placed,
            info,
            frame,
            pixel_to_display,
        });
    }
    Ok(candidates)
}

/// A bitmap the size of a render and the matrix (applied after the display
/// matrix) that draws an image into it.
struct Grid {
    width: u32,
    height: u32,
    matrix: Matrix,
    /// The image's own pixels did not fit in `max_pixels`, so the grid is
    /// smaller than the image.
    scaled_down: bool,
}

/// The grid to render `image` into with at most `max_pixels` pixels: its own
/// pixel grid (design §4.6, 1 and 2) if it is placed by flips and quarter
/// turns, otherwise the bounding box of its frame at the image's own density.
fn image_grid(image: &ListedImage, max_pixels: u64) -> Result<Grid, Response> {
    if image.is_native() {
        let (px_width, px_height) = (image.placed.px_width, image.placed.px_height);
        let (width, height, scaled_down) = photo_output_size(px_width, px_height, max_pixels);
        let matrix = photo_grid_matrix(image.pixel_to_display, px_width, px_height, width, height)
            .ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))?;
        return Ok(Grid {
            width,
            height,
            matrix,
            scaled_down,
        });
    }
    let frame = image.frame;
    let (frame_width, frame_height) = (frame.x1 - frame.x0, frame.y1 - frame.y0);
    let density =
        frame_density(image).min((max_pixels as f64 / (frame_width * frame_height)).sqrt());
    Ok(Grid {
        // Rounded up so that the grid covers the whole frame.
        width: (frame_width * density).ceil().max(1.0) as u32,
        height: (frame_height * density).ceil().max(1.0) as u32,
        matrix: [
            density,
            0.0,
            0.0,
            density,
            -frame.x0 * density,
            -frame.y0 * density,
        ],
        scaled_down: false,
    })
}

/// A box of whole pixels of a grid; `right` and `bottom` are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelBox {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

/// The box around the pixels that the image touches at all: those that are
/// not plain background on both a white and a black render. `None` when there
/// are none. `white` and `black` are RGBA with `width` pixels per row.
///
/// A pixel the image only half covers is touched, so a translucent photo or a
/// feathered edge keeps its range; only fully transparent pixels, which show
/// the background unchanged on both, are left out.
fn touched_box(white: &[u8], black: &[u8], width: u32) -> Option<PixelBox> {
    let mut found: Option<PixelBox> = None;
    for (index, (white, black)) in white
        .as_chunks::<4>()
        .0
        .iter()
        .zip(black.as_chunks::<4>().0)
        .enumerate()
    {
        if white[..3] == [255, 255, 255] && black[..3] == [0, 0, 0] {
            continue;
        }
        let (x, y) = (index as u32 % width, index as u32 / width);
        found = Some(match found {
            None => PixelBox {
                left: x,
                top: y,
                right: x + 1,
                bottom: y + 1,
            },
            Some(b) => PixelBox {
                left: b.left.min(x),
                top: b.top.min(y),
                right: b.right.max(x + 1),
                bottom: b.bottom.max(y + 1),
            },
        });
    }
    found
}

/// The box of pixels of `grid` that the shown page covers, a pixel that the
/// page only partly covers included; `None` when it covers none. pdfium does
/// not clip a render with a matrix to the page box, so what a grid draws
/// outside it must be left out by hand.
fn page_pixels(grid: &Grid, page_width: f64, page_height: f64) -> Option<PixelBox> {
    let corners = [
        (0.0, 0.0),
        (page_width, 0.0),
        (0.0, page_height),
        (page_width, page_height),
    ]
    .map(|(x, y)| apply(grid.matrix, x, y));
    let low = |axis: fn(&(f64, f64)) -> f64, size: u32| {
        let edge = corners.iter().map(axis).fold(f64::INFINITY, f64::min);
        (edge + PAGE_EDGE_TOLERANCE_PX)
            .floor()
            .clamp(0.0, f64::from(size)) as u32
    };
    let high = |axis: fn(&(f64, f64)) -> f64, size: u32| {
        let edge = corners.iter().map(axis).fold(f64::NEG_INFINITY, f64::max);
        (edge - PAGE_EDGE_TOLERANCE_PX)
            .ceil()
            .clamp(0.0, f64::from(size)) as u32
    };
    let page = PixelBox {
        left: low(|c| c.0, grid.width),
        top: low(|c| c.1, grid.height),
        right: high(|c| c.0, grid.width),
        bottom: high(|c| c.1, grid.height),
    };
    (page.right > page.left && page.bottom > page.top).then_some(page)
}

/// The box that `a` and `b` both cover; `None` when they share no pixel.
fn intersect(a: PixelBox, b: PixelBox) -> Option<PixelBox> {
    let both = PixelBox {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (both.right > both.left && both.bottom > both.top).then_some(both)
}

/// The box of pixels of `grid` in which the isolated image shows on the shown
/// page (design §4.6, 埋め込み画像の一覧).
fn visible_box(isolated: &PdfPage<'_>, grid: &Grid) -> Result<Option<PixelBox>, Response> {
    let draw = |background| {
        render_rgba(&Draw {
            page: isolated,
            width: grid.width,
            height: grid.height,
            matrix: grid.matrix,
            background,
            gray: false,
            annotations: false,
        })
    };
    let white = draw(PdfColor::WHITE)?;
    let black = draw(PdfColor::BLACK)?;
    let page_width = f64::from(isolated.width().value);
    let page_height = f64::from(isolated.height().value);
    Ok(touched_box(&white, &black, grid.width)
        .zip(page_pixels(grid, page_width, page_height))
        .and_then(|(touched, page)| intersect(touched, page)))
}

/// The part of the image's frame that shows, on the shown page: `visible`, the
/// visible box of `grid` (of at most [`VISIBLE_CHECK_PIXELS`]), one grid pixel
/// wider on each side, cut to the whole frame. `None` when nothing shows.
fn shown_rect(
    image: &ListedImage,
    grid: &Grid,
    visible: Option<PixelBox>,
) -> Result<Option<RectPt>, Response> {
    let Some(visible) = visible else {
        return Ok(None);
    };
    let to_display =
        invert(grid.matrix).ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))?;
    let (left, top) = (
        visible.left.saturating_sub(1),
        visible.top.saturating_sub(1),
    );
    let (right, bottom) = (
        (visible.right + 1).min(grid.width),
        (visible.bottom + 1).min(grid.height),
    );
    let corners = [(left, top), (right, top), (left, bottom), (right, bottom)]
        .map(|(x, y)| apply(to_display, f64::from(x), f64::from(y)));
    let rect = RectPt {
        x0: corners
            .iter()
            .map(|c| c.0)
            .fold(f64::INFINITY, f64::min)
            .max(image.frame.x0),
        y0: corners
            .iter()
            .map(|c| c.1)
            .fold(f64::INFINITY, f64::min)
            .max(image.frame.y0),
        x1: corners
            .iter()
            .map(|c| c.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .min(image.frame.x1),
        y1: corners
            .iter()
            .map(|c| c.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .min(image.frame.y1),
    };
    Ok((rect.x1 > rect.x0 && rect.y1 > rect.y0).then_some(rect))
}

/// `image` with its range set to the part that shows, or `None` when nothing
/// shows or the visible part is shorter than [`MIN_PHOTO_SIDE_PT`] on a side.
fn visible_image(
    doc: &PdfDocument<'_>,
    page_number: u32,
    image: ListedImage,
) -> Result<Option<ListedImage>, Response> {
    let grid = image_grid(&image, VISIBLE_CHECK_PIXELS)?;
    let isolated = isolated_page(doc, page_number, &[&image.placed.path])?;
    let visible = visible_box(&isolated, &grid)?;
    shown_image(image, &grid, visible)
}

/// `image` with its range set from `visible`, its visible box in `grid`, as
/// [`visible_image`] does.
fn shown_image(
    mut image: ListedImage,
    grid: &Grid,
    visible: Option<PixelBox>,
) -> Result<Option<ListedImage>, Response> {
    let Some(rect) = shown_rect(&image, grid, visible)? else {
        return Ok(None);
    };
    if (rect.x1 - rect.x0).min(rect.y1 - rect.y0) < MIN_PHOTO_SIDE_PT {
        return Ok(None);
    }
    image.info.rect_pt = rect;
    Ok(Some(image))
}

/// The listed image with `image_index`, or `InvalidParams` when `PageImages`
/// does not list one.
fn listed_image(
    doc: &PdfDocument<'_>,
    page: &PdfPage<'_>,
    page_number: u32,
    image_index: u32,
) -> Result<ListedImage, Response> {
    candidate_images(page)?
        .into_iter()
        .find(|image| image.info.image_index == image_index)
        .map(|image| visible_image(doc, page_number, image))
        .transpose()?
        .flatten()
        .ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))
}

/// The embedded images of page `page` (1-based), in drawing order, each with
/// the range that shows (design §4.6). `max_pixels` is the limit that a photo
/// in its own pixels is written within, as in [`render_region`].
pub(crate) fn page_images(
    doc: &PdfDocument<'_>,
    page_number: u32,
    max_pixels: u64,
) -> Result<Vec<EmbeddedImage>, Response> {
    let page = load_page(doc, page_number)?;
    let candidates = candidate_images(&page)?;
    let grids = candidates
        .iter()
        .map(|image| image_grid(image, VISIBLE_CHECK_PIXELS))
        .collect::<Result<Vec<_>, _>>()?;
    let frames: Vec<RectPt> = candidates.iter().map(|image| image.frame).collect();
    let reaches = grids
        .iter()
        .map(grid_reach)
        .collect::<Result<Vec<_>, _>>()?;
    let mut visible = vec![None; candidates.len()];
    for group in apart_groups(&frames, &reaches) {
        let paths: Vec<&[usize]> = group
            .iter()
            .map(|&i| candidates[i].placed.path.as_slice())
            .collect();
        let isolated = isolated_page(doc, page_number, &paths)?;
        for &i in &group {
            visible[i] = visible_box(&isolated, &grids[i])?;
        }
    }
    let mut images = Vec::new();
    for ((candidate, grid), visible) in candidates.into_iter().zip(&grids).zip(visible) {
        if let Some(mut image) = shown_image(candidate, grid, visible)? {
            image.info.native_size = image.native_size(max_pixels);
            images.push(image.info);
        }
    }
    Ok(images)
}

/// The display-space box that the pixels of `grid` reach, one grid pixel
/// wider on each side.
fn grid_reach(grid: &Grid) -> Result<RectPt, Response> {
    let to_display =
        invert(grid.matrix).ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))?;
    let (right, bottom) = (f64::from(grid.width) + 1.0, f64::from(grid.height) + 1.0);
    let corners = [(-1.0, -1.0), (right, -1.0), (-1.0, bottom), (right, bottom)]
        .map(|(x, y)| apply(to_display, x, y));
    Ok(RectPt {
        x0: corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min),
        y0: corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min),
        x1: corners
            .iter()
            .map(|c| c.0)
            .fold(f64::NEG_INFINITY, f64::max),
        y1: corners
            .iter()
            .map(|c| c.1)
            .fold(f64::NEG_INFINITY, f64::max),
    })
}

fn overlaps(a: &RectPt, b: &RectPt) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// Splits images (by index) into groups that can be isolated together and
/// still each draw into its own grid as if it were alone: an image draws only
/// inside its frame (`frames`), so in a group no frame comes within the reach
/// ([`grid_reach`], `reaches`) of another member's grid. Reloading the page
/// for each isolation is what costs (design §4.6, 埋め込み画像の一覧), and
/// side-by-side images such as the tiles of a map then share one.
fn apart_groups(frames: &[RectPt], reaches: &[RectPt]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..frames.len() {
        let apart =
            |j: &usize| !overlaps(&reaches[i], &frames[*j]) && !overlaps(&reaches[*j], &frames[i]);
        match groups.iter_mut().find(|group| group.iter().all(apart)) {
            Some(group) => group.push(i),
            None => groups.push(vec![i]),
        }
    }
    groups
}

/// How the stored pixels of an image sit in the shown page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Orientation {
    /// Image rows run along the shown x axis instead of the y axis.
    transposed: bool,
    flip_x: bool,
    flip_y: bool,
}

impl Orientation {
    /// The orientation of a quarter-turn placement `pixel_to_display`.
    fn of(pixel_to_display: Matrix) -> Self {
        let [a, b, c, d, ..] = pixel_to_display;
        let largest = [a, b, c, d].iter().fold(0.0_f64, |acc, v| acc.max(v.abs()));
        if b.abs() <= largest * NATIVE_RELATIVE_TOLERANCE
            && c.abs() <= largest * NATIVE_RELATIVE_TOLERANCE
        {
            Self {
                transposed: false,
                flip_x: a < 0.0,
                flip_y: d < 0.0,
            }
        } else {
            // x' follows the row (c), y' follows the column (b).
            Self {
                transposed: true,
                flip_x: c < 0.0,
                flip_y: b < 0.0,
            }
        }
    }

    fn is_identity(self) -> bool {
        !self.transposed && !self.flip_x && !self.flip_y
    }

    /// The width and height of an image of the stored `width` × `height`
    /// once it is shown in this orientation.
    fn output_size<T>(self, width: T, height: T) -> (T, T) {
        if self.transposed {
            (height, width)
        } else {
            (width, height)
        }
    }
}

/// Rearranges `source`, `width` × `height` pixels of `channels` bytes in the
/// image's stored orientation, into the orientation it is shown in. Returns
/// the pixels and the new width and height.
fn reorient(
    source: Vec<u8>,
    width: usize,
    height: usize,
    channels: usize,
    orientation: Orientation,
) -> (Vec<u8>, usize, usize) {
    if orientation.is_identity() {
        return (source, width, height);
    }
    let (out_width, out_height) = orientation.output_size(width, height);
    let mut out = vec![0u8; source.len()];
    for y in 0..out_height {
        let source_y = if orientation.flip_y {
            out_height - 1 - y
        } else {
            y
        };
        for x in 0..out_width {
            let source_x = if orientation.flip_x {
                out_width - 1 - x
            } else {
                x
            };
            let (column, row) = if orientation.transposed {
                (source_y, source_x)
            } else {
                (source_x, source_y)
            };
            let from = (row * width + column) * channels;
            let to = (y * out_width + x) * channels;
            out[to..to + channels].copy_from_slice(&source[from..from + channels]);
        }
    }
    (out, out_width, out_height)
}

/// The size to write a `width` × `height` image in: its own, or when that
/// exceeds `max_pixels`, scaled by one factor to fit. The flag tells which.
fn photo_output_size(width: u32, height: u32, max_pixels: u64) -> (u32, u32, bool) {
    let max_pixels = max_pixels.max(1);
    let total = u64::from(width) * u64::from(height);
    if total <= max_pixels {
        return (width, height, false);
    }
    let mut factor = (max_pixels as f64 / total as f64).sqrt();
    loop {
        let out_width = (f64::from(width) * factor).round().max(1.0) as u32;
        let out_height = (f64::from(height) * factor).round().max(1.0) as u32;
        // Rounding both sides up can overshoot by a row or a column.
        if u64::from(out_width) * u64::from(out_height) <= max_pixels {
            return (out_width, out_height, true);
        }
        factor *= 0.999;
    }
}

/// The matrix that makes the bitmap `out_width` × `out_height` the image's
/// pixel grid (design §4.6, 1 and 2): shown page → image pixels, scaled to the
/// output, with every edge pulled in by [`PHOTO_EDGE_INSET`].
fn photo_grid_matrix(
    pixel_to_display: Matrix,
    px_width: u32,
    px_height: u32,
    out_width: u32,
    out_height: u32,
) -> Option<Matrix> {
    let display_to_pixel = invert(pixel_to_display)?;
    let to_output = [
        f64::from(out_width) / f64::from(px_width),
        0.0,
        0.0,
        f64::from(out_height) / f64::from(px_height),
        0.0,
        0.0,
    ];
    let (w, h) = (f64::from(out_width), f64::from(out_height));
    let inset = [
        (w - 2.0 * PHOTO_EDGE_INSET) / w,
        0.0,
        0.0,
        (h - 2.0 * PHOTO_EDGE_INSET) / h,
        PHOTO_EDGE_INSET,
        PHOTO_EDGE_INSET,
    ];
    Some(then(then(display_to_pixel, to_output), inset))
}

/// A page with everything but the images at `paths` removed (design §4.6,
/// 写真だけを書き出す). Each path is a [`PlacedImage::path`].
///
/// The page is loaded again so that the open one stays whole. Its content
/// regeneration is stopped before anything is removed: otherwise
/// pdfium-render rewrites the page content on each change and the reloaded
/// page then draws nothing.
fn isolated_page<'a>(
    doc: &'a PdfDocument<'_>,
    page_number: u32,
    paths: &[&[usize]],
) -> Result<PdfPage<'a>, Response> {
    let mut page = load_page(doc, page_number)?;
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    // pdfium looks up the object to remove from the front of the list, so a
    // removal is cheap only near the front; removing from the back would take
    // time in the square of the object count. Each removal is therefore just
    // past the objects kept so far.
    let kept = kept_heads(paths);
    let count = page.objects().len();
    let mut position = 0;
    for index in 0..count {
        if kept.contains(&index) {
            position += 1;
        } else {
            page.objects_mut()
                .remove_object_at_index(position)
                .map_err(removal_failed)?;
        }
    }
    for (position, &index) in kept.iter().enumerate() {
        let inner = paths_below(paths, index);
        if !inner.is_empty() {
            let mut form = page.objects().get(position).map_err(removal_failed)?;
            keep_in_form(&mut form, &inner)?;
        }
    }
    Ok(page)
}

/// Removes from the form `object` everything but the images at `paths`,
/// which are relative to it.
fn keep_in_form(object: &mut PdfPageObject<'_>, paths: &[&[usize]]) -> Result<(), Response> {
    let PdfPageObject::XObjectForm(form) = object else {
        return Err(error_response(CODE_PDF_OPEN_FAILED, None));
    };
    // From the front, as on the page.
    let kept = kept_heads(paths);
    let count = form.len();
    let mut position = 0;
    for index in 0..count {
        if kept.contains(&index) {
            position += 1;
        } else {
            form.remove_object_at_index(position)
                .map_err(removal_failed)?;
        }
    }
    for (position, &index) in kept.iter().enumerate() {
        let inner = paths_below(paths, index);
        if !inner.is_empty() {
            let mut child = form.get(position).map_err(removal_failed)?;
            keep_in_form(&mut child, &inner)?;
        }
    }
    Ok(())
}

/// The first index of each path, in order and once each.
fn kept_heads(paths: &[&[usize]]) -> BTreeSet<usize> {
    paths.iter().map(|path| path[0]).collect()
}

/// The rest of the paths that start at `index` and go on into a form.
fn paths_below<'p>(paths: &[&'p [usize]], index: usize) -> Vec<&'p [usize]> {
    paths
        .iter()
        .filter(|path| path[0] == index && path.len() > 1)
        .map(|path| &path[1..])
        .collect()
}

fn removal_failed(e: pdfium_render::prelude::PdfiumError) -> Response {
    error_response(CODE_PDF_OPEN_FAILED, Some(format!("{e:?}")))
}

/// What to draw: the page (or the isolated image), into a bitmap of
/// `width` × `height`, through `matrix` (applied after the display matrix).
struct Draw<'a, 'b> {
    page: &'a PdfPage<'b>,
    width: u32,
    height: u32,
    matrix: Matrix,
    background: PdfColor,
    gray: bool,
    annotations: bool,
}

/// Renders to RGBA bytes (straight alpha).
fn render_rgba(draw: &Draw<'_, '_>) -> Result<Vec<u8>, Response> {
    let failed = |e: pdfium_render::prelude::PdfiumError| {
        error_response(CODE_PDF_OPEN_FAILED, Some(format!("{e:?}")))
    };
    let mut config = PdfRenderConfig::new()
        .set_fixed_size(draw.width as i32, draw.height as i32)
        .set_clear_color(draw.background)
        .render_annotations(draw.annotations)
        .apply_matrix(to_pdf(draw.matrix))
        .map_err(failed)?;
    if draw.gray {
        config = config.use_grayscale_rendering(true);
    }
    let bitmap = draw.page.render_with_config(&config).map_err(failed)?;
    Ok(bitmap.as_rgba_bytes())
}

/// How many pixels per point to render an image's frame at when its pixels do
/// not line up with the page: the image's own density.
fn frame_density(image: &ListedImage) -> f64 {
    let m = image.pixel_to_display;
    let area_per_pixel = (m[0] * m[3] - m[1] * m[2]).abs();
    if area_per_pixel > 0.0 {
        (1.0 / area_per_pixel).sqrt()
    } else {
        1.0
    }
}

/// How many pixels the photo covers and something else changes (design §4.6,
/// 写真の上に何かが載っているか).
///
/// All three are RGBA of one size: the whole page, and the photo alone on a
/// white and on a black background. The photo covers a pixel where it looks
/// the same on both backgrounds; the pixel counts when the page differs from
/// the photo on white by more than [`ON_TOP_COLOR_TOLERANCE`] in any channel.
fn count_covered_pixels(page: &[u8], photo_on_white: &[u8], photo_on_black: &[u8]) -> usize {
    page.as_chunks::<4>()
        .0
        .iter()
        .zip(photo_on_white.as_chunks::<4>().0)
        .zip(photo_on_black.as_chunks::<4>().0)
        .filter(|((page, white), black)| {
            white[..3] == black[..3]
                && page[..3]
                    .iter()
                    .zip(&white[..3])
                    .any(|(a, b)| a.abs_diff(*b) > ON_TOP_COLOR_TOLERANCE)
        })
        .count()
}

/// Whether something is drawn over image `image_index` of page `page`
/// (design §4.6). The renders are at most `check_pixels` each.
pub(crate) fn image_on_top(
    doc: &PdfDocument<'_>,
    page_number: u32,
    image_index: u32,
    check_pixels: u64,
) -> Result<bool, Response> {
    let page = load_page(doc, page_number)?;
    let image = listed_image(doc, &page, page_number, image_index)?;
    let grid = image_grid(&image, check_pixels)?;
    let Grid {
        width,
        height,
        matrix,
        ..
    } = grid;

    let isolated = isolated_page(doc, page_number, &[&image.placed.path])?;
    let draw = |page: &PdfPage<'_>, background, annotations| {
        render_rgba(&Draw {
            page,
            width,
            height,
            matrix,
            background,
            gray: false,
            annotations,
        })
    };
    let whole = draw(&page, PdfColor::WHITE, true)?;
    let photo_on_white = draw(&isolated, PdfColor::WHITE, false)?;
    let photo_on_black = draw(&isolated, PdfColor::BLACK, false)?;
    // Only what the shown page covers counts: something drawn over a part of
    // the image that the page box hides is not on top of anything shown.
    let page_width = f64::from(page.width().value);
    let page_height = f64::from(page.height().value);
    let Some(on_page) = page_pixels(&grid, page_width, page_height) else {
        return Ok(false);
    };
    let on_page_only =
        |rgba: Vec<u8>| crop(rgba, width as usize, height as usize, 4, Some(on_page)).0;
    Ok(count_covered_pixels(
        &on_page_only(whole),
        &on_page_only(photo_on_white),
        &on_page_only(photo_on_black),
    ) >= ON_TOP_MIN_PIXELS)
}

/// The parameters of `RenderRegion` (design §5.1).
pub(crate) struct RegionRequest {
    pub page: u32,
    pub rect: RectPt,
    pub image_index: Option<u32>,
    pub dpi: u32,
    pub format: RenderFormat,
    pub color: RenderColor,
    pub photo_native: bool,
    pub photo_only: bool,
}

/// Renders one range (design §4.6): a hand-picked or detected rectangle at
/// `dpi`, or an image in its own pixels. An output of more than `max_pixels`
/// is refused for a rectangle and scaled down for a photo.
pub(crate) fn render_region(
    doc: &PdfDocument<'_>,
    request: &RegionRequest,
    max_pixels: u64,
) -> Result<(Response, Vec<u8>), Response> {
    let page = load_page(doc, request.page)?;
    let page_width = f64::from(page.width().value);
    let page_height = f64::from(page.height().value);

    let rect = request.rect;
    let outside = RANGE_OUTSIDE_TOLERANCE_PT;
    let inside_page = rect.x0 >= -outside
        && rect.y0 >= -outside
        && rect.x1 <= page_width + outside
        && rect.y1 <= page_height + outside;
    if !(rect.x0 < rect.x1 && rect.y0 < rect.y1 && inside_page) {
        return Err(error_response(CODE_INVALID_PARAMS, None));
    }

    let photo = match request.image_index {
        Some(index) => Some(listed_image(doc, &page, request.page, index)?),
        None => None,
    };
    let own_pixels = photo
        .as_ref()
        .filter(|image| request.photo_native && image.is_native());
    if own_pixels.is_none() && !is_render_dpi_in_range(request.dpi) {
        return Err(error_response(CODE_INVALID_PARAMS, None));
    }

    // Everything but the photo is left out of a photo-only render, and the
    // background stays transparent where the format can keep that. A photo in
    // its own pixels needs the image alone in any case, to find what shows.
    let photo_only = photo.is_some() && request.photo_only;
    let isolated = match &photo {
        Some(image) if photo_only || own_pixels.is_some() => {
            Some(isolated_page(doc, request.page, &[&image.placed.path])?)
        }
        _ => None,
    };
    let source = match &isolated {
        Some(isolated) if photo_only => isolated,
        _ => &page,
    };
    let transparent =
        photo_only && matches!(request.format, RenderFormat::Png | RenderFormat::Webp);
    let background = if transparent {
        PdfColor::new(0, 0, 0, 0)
    } else {
        PdfColor::WHITE
    };
    let gray = request.color == RenderColor::Gray;

    let (grid, orientation, visible) = match (own_pixels, &isolated) {
        (Some(image), Some(isolated)) => {
            let grid = image_grid(image, max_pixels)?;
            let visible = visible_box(isolated, &grid)?;
            (grid, Orientation::of(image.pixel_to_display), visible)
        }
        _ => {
            let scale = f64::from(request.dpi) / 72.0;
            let width = ((rect.x1 - rect.x0) * scale).round().max(1.0) as u64;
            let height = ((rect.y1 - rect.y0) * scale).round().max(1.0) as u64;
            if width.saturating_mul(height) > max_pixels {
                return Err(error_response(CODE_RENDER_TOO_LARGE, None));
            }
            let grid = Grid {
                width: width as u32,
                height: height as u32,
                matrix: [scale, 0.0, 0.0, scale, -rect.x0 * scale, -rect.y0 * scale],
                scaled_down: false,
            };
            let upright = Orientation {
                transposed: false,
                flip_x: false,
                flip_y: false,
            };
            (grid, upright, None)
        }
    };

    let rgba = render_rgba(&Draw {
        page: source,
        width: grid.width,
        height: grid.height,
        matrix: grid.matrix,
        background,
        gray,
        annotations: !photo_only,
    })?;
    // Cut to what shows while the pixels are still in the image's own
    // orientation, which is the one `visible` was measured in.
    let (rgba, width, height) = crop(rgba, grid.width as usize, grid.height as usize, 4, visible);
    let (pixels, color_type) = pixels_from_rgba(rgba, request.color, transparent);
    let channels = color_type.bits_per_pixel() as usize / 8;
    let (pixels, out_width, out_height) = reorient(pixels, width, height, channels, orientation);
    let (out_width, out_height) = (out_width as u32, out_height as u32);
    let bytes = encode_pixels(&pixels, out_width, out_height, color_type, request.format)?;
    Ok((
        Response::RenderRegion {
            width: out_width,
            height: out_height,
            scaled_down: grid.scaled_down,
        },
        bytes,
    ))
}

/// The pixels of `region` out of `source`, `width` × `height` pixels of
/// `channels` bytes; all of it when `region` is `None` or covers it. Returns
/// the pixels and their width and height.
fn crop(
    source: Vec<u8>,
    width: usize,
    height: usize,
    channels: usize,
    region: Option<PixelBox>,
) -> (Vec<u8>, usize, usize) {
    let Some(region) = region else {
        return (source, width, height);
    };
    let (left, top) = (region.left as usize, region.top as usize);
    let (right, bottom) = (region.right as usize, region.bottom as usize);
    if (left, top, right, bottom) == (0, 0, width, height) {
        return (source, width, height);
    }
    let mut out = Vec::with_capacity((right - left) * (bottom - top) * channels);
    for row in top..bottom {
        let start = (row * width + left) * channels;
        out.extend_from_slice(&source[start..start + (right - left) * channels]);
    }
    (out, right - left, bottom - top)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pdfium_render::prelude::Pdfium;

    use super::*;

    #[test]
    fn matrices_compose_in_the_order_they_are_applied() {
        let scale = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        let shift = [1.0, 0.0, 0.0, 1.0, 10.0, 20.0];
        // Scale first, then shift.
        assert_eq!(apply(then(scale, shift), 1.0, 1.0), (12.0, 23.0));
        assert_eq!(apply(then(shift, scale), 1.0, 1.0), (22.0, 63.0));
        let both = then(scale, shift);
        let back = invert(both).unwrap();
        let (x, y) = apply(then(both, back), 5.0, 7.0);
        assert!((x - 5.0).abs() < 1e-12 && (y - 7.0).abs() < 1e-12);
        assert!(invert([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn a_frame_is_cut_to_the_page_and_vanishes_outside_it() {
        let place = |x, y, w, h| [w, 0.0, 0.0, h, x, y];
        let inside = frame_on_page(place(10.0, 20.0, 30.0, 40.0), 100.0, 100.0).unwrap();
        assert_eq!(
            (inside.x0, inside.y0, inside.x1, inside.y1),
            (10.0, 20.0, 40.0, 60.0)
        );

        let cut = frame_on_page(place(-10.0, 90.0, 30.0, 40.0), 100.0, 100.0).unwrap();
        assert_eq!((cut.x0, cut.y0, cut.x1, cut.y1), (0.0, 90.0, 20.0, 100.0));

        assert!(frame_on_page(place(100.0, 0.0, 10.0, 10.0), 100.0, 100.0).is_none());
        assert!(frame_on_page(place(-50.0, 0.0, 10.0, 10.0), 100.0, 100.0).is_none());
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> RectPt {
        RectPt { x0, y0, x1, y1 }
    }

    #[test]
    fn a_grid_reaches_one_of_its_pixels_beyond_itself() {
        // Two pixels a point over the frame from (10, 20) to (20, 30).
        let grid = Grid {
            width: 20,
            height: 20,
            matrix: [2.0, 0.0, 0.0, 2.0, -20.0, -40.0],
            scaled_down: false,
        };
        let reach = grid_reach(&grid).unwrap();
        assert_eq!(
            (reach.x0, reach.y0, reach.x1, reach.y1),
            (9.5, 19.5, 20.5, 30.5)
        );
    }

    #[test]
    fn images_share_a_group_only_out_of_each_others_reach() {
        let frames = [
            rect(0.0, 0.0, 10.0, 10.0),
            // Apart from the first, but by less than a grid pixel.
            rect(10.5, 0.0, 20.5, 10.0),
            rect(30.0, 0.0, 40.0, 10.0),
            // Overlaps the first two.
            rect(5.0, 5.0, 15.0, 15.0),
        ];
        let reaches = frames.map(|f| rect(f.x0 - 1.0, f.y0 - 1.0, f.x1 + 1.0, f.y1 + 1.0));
        assert_eq!(
            apart_groups(&frames, &reaches),
            vec![vec![0, 2], vec![1], vec![3]]
        );
    }

    #[test]
    fn only_flips_and_quarter_turns_are_native_placements() {
        for native in [
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [-3.0, 0.0, 0.0, 2.0, 5.0, 5.0],
            [0.0, 4.0, -2.0, 0.0, 1.0, 1.0],
            [0.0, -4.0, -2.0, 0.0, 1.0, 1.0],
            // Float noise of a computed matrix.
            [100.0, 1e-9, -1e-9, 50.0, 0.0, 0.0],
            [1e-9, 100.0, 50.0, -1e-9, 0.0, 0.0],
        ] {
            assert!(is_quarter_turn_placement(native), "{native:?}");
        }
        for other in [
            // A shear of 1e-3 of the size is not noise.
            [100.0, 0.1, 0.0, 50.0, 0.0, 0.0],
            [100.0, 0.0, 0.1, 50.0, 0.0, 0.0],
            [120.0, 40.0, -30.0, 90.0, 0.0, 0.0],
            // A turn by 45 degrees.
            [1.0, 1.0, -1.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ] {
            assert!(!is_quarter_turn_placement(other), "{other:?}");
        }
    }

    /// 3 × 2 pixels, one byte each: `A B C` over `D E F`.
    const SAMPLE: [u8; 6] = *b"ABCDEF";

    fn reoriented(orientation: Orientation) -> (String, usize, usize) {
        let (pixels, width, height) = reorient(SAMPLE.to_vec(), 3, 2, 1, orientation);
        (String::from_utf8(pixels).unwrap(), width, height)
    }

    fn orientation(transposed: bool, flip_x: bool, flip_y: bool) -> Orientation {
        Orientation {
            transposed,
            flip_x,
            flip_y,
        }
    }

    #[test]
    fn all_eight_orientations_rearrange_the_pixels() {
        let expected = [
            (orientation(false, false, false), "ABCDEF", 3, 2),
            (orientation(false, true, false), "CBAFED", 3, 2),
            (orientation(false, false, true), "DEFABC", 3, 2),
            (orientation(false, true, true), "FEDCBA", 3, 2),
            (orientation(true, false, false), "ADBECF", 2, 3),
            (orientation(true, true, false), "DAEBFC", 2, 3),
            (orientation(true, false, true), "CFBEAD", 2, 3),
            (orientation(true, true, true), "FCEBDA", 2, 3),
        ];
        for (orientation, pixels, width, height) in expected {
            assert_eq!(
                reoriented(orientation),
                (pixels.to_string(), width, height),
                "{orientation:?}"
            );
        }
    }

    #[test]
    fn reordering_keeps_every_channel_of_a_pixel_together() {
        let rgb = vec![1, 2, 3, 4, 5, 6];
        let (flipped, width, height) = reorient(rgb, 2, 1, 3, orientation(false, true, false));
        assert_eq!((width, height), (2, 1));
        assert_eq!(flipped, [4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn the_orientation_follows_where_the_matrix_sends_the_pixels() {
        // A 3 × 2 image placed by each of the eight matrices; the pixel
        // whose centre lands on a display cell must be the one reorient puts
        // in that cell.
        let placements: [[f64; 4]; 8] = [
            [1.0, 0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0, -1.0],
            [0.0, 1.0, 1.0, 0.0],
            [0.0, 1.0, -1.0, 0.0],
            [0.0, -1.0, 1.0, 0.0],
            [0.0, -1.0, -1.0, 0.0],
        ];
        for [a, b, c, d] in placements {
            // Moves the image to non-negative display coordinates.
            let shift = [a, b, c, d, 10.0, 10.0];
            let place = Orientation::of(shift);
            let (pixels, width, _) = reorient(SAMPLE.to_vec(), 3, 2, 1, place);
            // The centres lie one unit apart; the least ones are the first cell.
            let centres: Vec<(f64, f64, u8)> = (0..2usize)
                .flat_map(|j| (0..3usize).map(move |i| (i, j)))
                .map(|(i, j)| {
                    let (x, y) = apply(shift, i as f64 + 0.5, j as f64 + 0.5);
                    (x, y, SAMPLE[j * 3 + i])
                })
                .collect();
            let least_x = centres.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
            let least_y = centres.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
            for (x, y, pixel) in centres {
                let column = (x - least_x).round() as usize;
                let row = (y - least_y).round() as usize;
                assert_eq!(pixels[row * width + column], pixel, "{shift:?}");
            }
        }
    }

    #[test]
    fn a_photo_over_the_limit_is_scaled_to_fit_and_keeps_its_shape() {
        assert_eq!(photo_output_size(100, 50, 5000), (100, 50, false));
        assert_eq!(photo_output_size(100, 50, 4999), (99, 50, true));
        let (width, height, scaled) = photo_output_size(20_000, 10_000, 100_000_000);
        assert!(scaled && u64::from(width) * u64::from(height) <= 100_000_000);
        assert!((f64::from(width) / f64::from(height) - 2.0).abs() < 0.001);
        // Whatever the shape, the result fits and is not empty.
        for (w, h) in [(1, 200_000), (200_000, 1), (7, 9_999_991), (65_537, 1_531)] {
            let (width, height, scaled) = photo_output_size(w, h, 100_000);
            assert!(scaled);
            assert!(width >= 1 && height >= 1);
            assert!(u64::from(width) * u64::from(height) <= 100_000, "{w}x{h}");
        }
    }

    /// A listed image of `px_width` × `px_height` pixels placed by
    /// `pixel_to_display`.
    fn listed(px_width: u32, px_height: u32, pixel_to_display: Matrix) -> ListedImage {
        let frame = rect(0.0, 0.0, 1.0, 1.0);
        ListedImage {
            placed: PlacedImage {
                image_index: 0,
                path: vec![0],
                unit_to_page: IDENTITY,
                px_width,
                px_height,
            },
            info: EmbeddedImage {
                image_index: 0,
                rect_pt: frame,
                px_width,
                px_height,
                native_size: None,
            },
            frame,
            pixel_to_display,
        }
    }

    fn native_size_of(image: &ListedImage, max_pixels: u64) -> Option<(u32, u32, bool)> {
        image
            .native_size(max_pixels)
            .map(|size| (size.width, size.height, size.scaled_down))
    }

    #[test]
    fn a_photo_is_written_in_the_orientation_of_its_placement_not_of_its_frame() {
        const LIMIT: u64 = 100_000_000;
        // 61 × 43 pixels (wider than tall), placed into a frame by each of the
        // ways that keep the pixel grid. Only a quarter turn swaps the sides,
        // whatever the shape of the frame.
        let stretched = |width_pt: f64, height_pt: f64| {
            (
                // Upright: columns along x, rows along y.
                [width_pt / 61.0, 0.0, 0.0, height_pt / 43.0, 0.0, 0.0],
                // Quarter-turned: columns along y, rows along x.
                [0.0, height_pt / 61.0, width_pt / 43.0, 0.0, 0.0, 0.0],
            )
        };
        for (width_pt, height_pt) in [
            // An ordinary wide frame, and a tall one.
            (200.0, 150.0),
            (150.0, 200.0),
            // Stretched far from the image's shape: the frame is taller than
            // wide while the upright image is wider than tall, and the other
            // way round for the quarter turn.
            (50.0, 200.0),
            (200.0, 50.0),
            // Nearly square on both counts.
            (100.4, 100.0),
            (99.6, 100.0),
        ] {
            let (upright, turned) = stretched(width_pt, height_pt);
            for flip in [[1.0, 1.0], [-1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]] {
                let flipped = |m: Matrix| {
                    [
                        m[0] * flip[0],
                        m[1] * flip[1],
                        m[2] * flip[0],
                        m[3] * flip[1],
                        0.0,
                        0.0,
                    ]
                };
                assert_eq!(
                    native_size_of(&listed(61, 43, flipped(upright)), LIMIT),
                    Some((61, 43, false)),
                    "upright {width_pt} x {height_pt} {flip:?}"
                );
                assert_eq!(
                    native_size_of(&listed(61, 43, flipped(turned)), LIMIT),
                    Some((43, 61, false)),
                    "turned {width_pt} x {height_pt} {flip:?}"
                );
            }
        }
    }

    #[test]
    fn a_scaled_down_photo_is_turned_after_it_is_scaled() {
        let wide = (14_142, 7_071);
        let upright = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let turned = [0.0, 1.0, 1.0, 0.0, 0.0, 0.0];
        assert_eq!(
            native_size_of(&listed(20_000, 10_000, upright), 100_000_000),
            Some((wide.0, wide.1, true))
        );
        assert_eq!(
            native_size_of(&listed(20_000, 10_000, turned), 100_000_000),
            Some((wide.1, wide.0, true))
        );
        // The limit decides, not the constant.
        assert_eq!(
            native_size_of(&listed(20_000, 10_000, upright), 400_000_000),
            Some((20_000, 10_000, false))
        );
    }

    #[test]
    fn a_photo_that_is_not_on_the_pixel_grid_has_no_native_size() {
        let sheared = [120.0, 40.0, -30.0, 90.0, 0.0, 0.0];
        assert_eq!(native_size_of(&listed(61, 43, sheared), 100_000_000), None);
    }

    #[test]
    fn the_grid_matrix_pulls_every_edge_in_by_the_inset() {
        // 10 × 10 pixels shown at 1 pt each from (5, 5).
        let pixel_to_display = [1.0, 0.0, 0.0, 1.0, 5.0, 5.0];
        let grid = photo_grid_matrix(pixel_to_display, 10, 10, 10, 10).unwrap();
        let (left, top) = apply(grid, 5.0, 5.0);
        let (right, bottom) = apply(grid, 15.0, 15.0);
        for (got, want) in [
            (left, PHOTO_EDGE_INSET),
            (top, PHOTO_EDGE_INSET),
            (right, 10.0 - PHOTO_EDGE_INSET),
            (bottom, 10.0 - PHOTO_EDGE_INSET),
        ] {
            assert!((got - want).abs() < 1e-9, "{got} against {want}");
        }
        // Drawn smaller, the grid is the output's, and the inset too.
        let small = photo_grid_matrix(pixel_to_display, 10, 10, 5, 5).unwrap();
        let (right, _) = apply(small, 15.0, 15.0);
        assert!((right - (5.0 - PHOTO_EDGE_INSET)).abs() < 1e-9);
        assert!(photo_grid_matrix([0.0; 6], 10, 10, 10, 10).is_none());
    }

    #[test]
    fn only_pixels_the_photo_covers_count_when_the_page_differs() {
        let white = [[255, 255, 255, 255]; 4].concat();
        let black = [
            // Covered, covered, covered, not covered (shows the background).
            [255, 255, 255, 255],
            [255, 255, 255, 255],
            [255, 255, 255, 255],
            [0, 0, 0, 255],
        ]
        .concat();
        let tolerance = ON_TOP_COLOR_TOLERANCE;
        let page = [
            // Unchanged, changed by exactly the tolerance, changed by one
            // more in one channel only, changed a lot but not covered.
            [255, 255, 255, 255],
            [255 - tolerance, 255, 255, 255],
            [255, 255, 255 - tolerance - 1, 255],
            [0, 0, 0, 255],
        ]
        .concat();
        assert_eq!(count_covered_pixels(&page, &white, &black), 1);
    }

    #[test]
    fn the_touched_box_leaves_out_only_plain_background() {
        const PLAIN_WHITE: [u8; 4] = [255, 255, 255, 255];
        const PLAIN_BLACK: [u8; 4] = [0, 0, 0, 255];
        // 3 × 2 pixels; only the middle of the top row and the last of the
        // bottom row are touched: one half covered, one translucent.
        let mut white = [PLAIN_WHITE; 6];
        let mut black = [PLAIN_BLACK; 6];
        white[1] = [128, 128, 128, 255];
        black[1] = [128, 128, 128, 255];
        white[5] = [200, 200, 200, 255];
        black[5] = [30, 30, 30, 255];
        let box_of =
            |white: &[[u8; 4]], black: &[[u8; 4]]| touched_box(&white.concat(), &black.concat(), 3);
        assert_eq!(
            box_of(&white, &black),
            Some(PixelBox {
                left: 1,
                top: 0,
                right: 3,
                bottom: 2
            })
        );
        assert_eq!(box_of(&[PLAIN_WHITE; 6], &[PLAIN_BLACK; 6]), None);
        // A pixel that only the black render shows is touched too.
        let mut only_black = [PLAIN_BLACK; 6];
        only_black[3] = [10, 0, 0, 255];
        assert_eq!(
            box_of(&[PLAIN_WHITE; 6], &only_black),
            Some(PixelBox {
                left: 0,
                top: 1,
                right: 1,
                bottom: 2
            })
        );
    }

    fn grid_of(width: u32, height: u32, matrix: Matrix) -> Grid {
        Grid {
            width,
            height,
            matrix,
            scaled_down: false,
        }
    }

    #[test]
    fn the_page_covers_the_pixels_it_touches_even_in_part() {
        // A 100 × 50 pt page at eight pixels to the point, its corner at
        // pixel (10.5, 3) of a grid that reaches well past it.
        let grid = grid_of(2000, 1000, [8.0, 0.0, 0.0, 8.0, 10.5, 3.0]);
        assert_eq!(
            page_pixels(&grid, 100.0, 50.0),
            Some(PixelBox {
                left: 10,
                top: 3,
                right: 811,
                bottom: 403
            })
        );
        // The grid ends inside the page: the box is cut to the grid.
        let short = grid_of(500, 1000, [8.0, 0.0, 0.0, 8.0, -4.0, 3.0]);
        assert_eq!(page_pixels(&short, 100.0, 50.0).unwrap().right, 500);
    }

    #[test]
    fn a_page_edge_on_a_pixel_boundary_does_not_take_in_the_next_pixel() {
        // The edges lie a hair inside the boundaries, as a mapped edge does.
        let grid = grid_of(2000, 1000, [8.0, 0.0, 0.0, 8.0, 10.0006, 3.0006]);
        assert_eq!(
            page_pixels(&grid, 100.0, 50.0),
            Some(PixelBox {
                left: 10,
                top: 3,
                right: 810,
                bottom: 403
            })
        );
    }

    #[test]
    fn the_page_is_found_in_a_grid_turned_a_quarter_or_flipped() {
        // Page x runs down the grid and page y runs across it, reversed.
        let turned = grid_of(400, 800, [0.0, 8.0, -8.0, 0.0, 400.0, 0.0]);
        assert_eq!(
            page_pixels(&turned, 100.0, 50.0),
            Some(PixelBox {
                left: 0,
                top: 0,
                right: 400,
                bottom: 800
            })
        );
        let flipped = grid_of(1000, 1000, [-8.0, 0.0, 0.0, 8.0, 900.0, 0.0]);
        let found = page_pixels(&flipped, 100.0, 50.0).unwrap();
        assert_eq!((found.left, found.right), (100, 900));
    }

    #[test]
    fn a_page_the_grid_does_not_reach_covers_no_pixel() {
        let beside = grid_of(100, 100, [8.0, 0.0, 0.0, 8.0, -800.0, 0.0]);
        assert_eq!(page_pixels(&beside, 100.0, 50.0), None);
        let past = grid_of(100, 100, [8.0, 0.0, 0.0, 8.0, 100.0, 0.0]);
        assert_eq!(page_pixels(&past, 100.0, 50.0), None);
    }

    #[test]
    fn boxes_meet_in_the_pixels_they_share() {
        let pixels = |left, top, right, bottom| PixelBox {
            left,
            top,
            right,
            bottom,
        };
        assert_eq!(
            intersect(pixels(0, 0, 10, 10), pixels(4, 2, 20, 8)),
            Some(pixels(4, 2, 10, 8))
        );
        assert_eq!(intersect(pixels(0, 0, 10, 10), pixels(10, 0, 20, 10)), None);
    }

    #[test]
    fn cropping_keeps_the_box_and_all_of_a_pixel() {
        // 4 × 3 pixels of two bytes each, numbered in reading order.
        let source: Vec<u8> = (0..12u8).flat_map(|p| [p, p + 100]).collect();
        let window = PixelBox {
            left: 1,
            top: 1,
            right: 3,
            bottom: 3,
        };
        let (cropped, width, height) = crop(source.clone(), 4, 3, 2, Some(window));
        assert_eq!((width, height), (2, 2));
        assert_eq!(cropped, [5, 105, 6, 106, 9, 109, 10, 110]);

        let whole = PixelBox {
            left: 0,
            top: 0,
            right: 4,
            bottom: 3,
        };
        assert_eq!(crop(source.clone(), 4, 3, 2, Some(whole)).0, source);
        assert_eq!(crop(source.clone(), 4, 3, 2, None).0, source);
    }

    fn pdfium_library_dir() -> PathBuf {
        let os_dir = if cfg!(windows) {
            "windows/bin"
        } else {
            "linux/lib"
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/pdfium")
            .join(os_dir)
    }

    /// The only test here that loads pdfium, which is not thread safe.
    #[test]
    fn a_photo_over_the_limit_is_written_smaller_in_the_same_orientation() {
        let library = Pdfium::pdfium_platform_library_name_at_path(&pdfium_library_dir());
        let pdfium = Pdfium::new(Pdfium::bind_to_library(&library).expect("pdfium should load"));
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/crop_align.pdf");
        let doc = pdfium
            .load_pdf_from_file(&fixture, None)
            .expect("crop_align.pdf");

        let render = |page: u32, image_index: u32, max_pixels: u64| {
            let images = page_images(&doc, page, max_pixels).unwrap();
            let image = images
                .iter()
                .find(|i| i.image_index == image_index)
                .unwrap();
            let request = RegionRequest {
                page,
                rect: image.rect_pt,
                image_index: Some(image_index),
                dpi: 150,
                format: RenderFormat::Png,
                color: RenderColor::Color,
                photo_native: true,
                photo_only: true,
            };
            match render_region(&doc, &request, max_pixels).unwrap() {
                (
                    Response::RenderRegion {
                        width,
                        height,
                        scaled_down,
                    },
                    bytes,
                ) => {
                    // Nothing hides these images, so the listing says what is written.
                    let listed = image.native_size.unwrap();
                    assert_eq!(
                        (listed.width, listed.height, listed.scaled_down),
                        (width, height, scaled_down)
                    );
                    (
                        width,
                        height,
                        scaled_down,
                        image::load_from_memory(&bytes).unwrap(),
                    )
                }
                other => panic!("{other:?}"),
            }
        };

        // 61 × 43 = 2623 pixels.
        let (width, height, scaled_down, _) = render(1, 0, 2623);
        assert_eq!((width, height, scaled_down), (61, 43, false));

        let (width, height, scaled_down, picture) = render(1, 0, 1000);
        assert!(scaled_down);
        assert!(u64::from(width) * u64::from(height) <= 1000);
        assert_eq!((picture.width(), picture.height()), (width, height));
        assert!((f64::from(width) / f64::from(height) - 61.0 / 43.0).abs() < 0.1);
        // The picture still runs red to the right and green downwards.
        let rgb = picture.to_rgb8();
        let (right, bottom) = (width - 1, height - 1);
        assert!(rgb.get_pixel(0, 0)[0] < 40 && rgb.get_pixel(right, 0)[0] > 215);
        assert!(rgb.get_pixel(0, 0)[1] < 40 && rgb.get_pixel(0, bottom)[1] > 215);

        // Image 3 is turned a quarter: its written size is the turned one,
        // and the page turned a further quarter swaps it back.
        let (width, height, _, _) = render(1, 3, 1000);
        assert!(width < height, "{width} x {height}");
        let (width, height, _, _) = render(2, 3, 1000);
        assert!(width > height, "{width} x {height}");
    }
}
