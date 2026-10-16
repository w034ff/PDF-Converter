//! The worker's side of cropping (design §4.6, §5.1): a page's embedded
//! images, whether something covers one, and rendering one range of a page.
//!
//! Ranges are rectangles in points in the orientation the page is shown in
//! (after `/Rotate`, origin top-left). pdfium-render's matrix is applied after
//! pdfium's own page-to-bitmap matrix, so the matrices built here first undo or
//! measure that one ([`display_matrix`]) instead of rebuilding it from
//! `/Rotate` and `CropBox`.

use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfMatrix, PdfPage, PdfPageContentRegenerationStrategy, PdfPageObject,
    PdfPageObjectsCommon, PdfPoints, PdfRenderConfig,
};

use crate::is_render_dpi_in_range;
use crate::protocol::{EmbeddedImage, RectPt, RenderColor, RenderFormat, Response};
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

/// How far outside the page a range may reach, in points (design §4.6).
pub const RANGE_OUTSIDE_TOLERANCE_PT: f64 = 0.5;

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

/// An image that [`page_images`] lists, with its placement.
struct ListedImage {
    placed: PlacedImage,
    info: EmbeddedImage,
    /// Image pixel (column, row from the top) → shown page, 1 pt = 1.
    pixel_to_display: Matrix,
}

/// The images [`page_images`] lists on a page, with their placements.
fn listed_images(page: &PdfPage<'_>) -> Result<Vec<ListedImage>, Response> {
    let display = display_matrix(page)?;
    let page_width = f64::from(page.width().value);
    let page_height = f64::from(page.height().value);
    let mut listed = Vec::new();
    for placed in walk_images(page) {
        if placed.px_width < MIN_PHOTO_SIDE_PX || placed.px_height < MIN_PHOTO_SIDE_PX {
            continue;
        }
        let unit_to_display = then(placed.unit_to_page, display);
        let Some(rect_pt) = frame_on_page(unit_to_display, page_width, page_height) else {
            continue;
        };
        if (rect_pt.x1 - rect_pt.x0).min(rect_pt.y1 - rect_pt.y0) < MIN_PHOTO_SIDE_PT {
            continue;
        }
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
            rect_pt,
            px_width: placed.px_width,
            px_height: placed.px_height,
            native: is_quarter_turn_placement(pixel_to_display),
        };
        listed.push(ListedImage {
            placed,
            info,
            pixel_to_display,
        });
    }
    Ok(listed)
}

/// The embedded images of page `page` (1-based), in drawing order (design
/// §4.6).
pub(crate) fn page_images(
    doc: &PdfDocument<'_>,
    page: u32,
) -> Result<Vec<EmbeddedImage>, Response> {
    let page = load_page(doc, page)?;
    let listed = listed_images(&page)?;
    Ok(listed.into_iter().map(|image| image.info).collect())
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
    let (out_width, out_height) = if orientation.transposed {
        (height, width)
    } else {
        (width, height)
    };
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

/// A page with everything but one image removed (design §4.6, 写真だけを書き出す).
///
/// The page is loaded again so that the open one stays whole. Its content
/// regeneration is stopped before anything is removed: otherwise
/// pdfium-render rewrites the page content on each change and the reloaded
/// page then draws nothing.
fn isolated_page<'a>(
    doc: &'a PdfDocument<'_>,
    page_number: u32,
    path: &[usize],
) -> Result<PdfPage<'a>, Response> {
    let mut page = load_page(doc, page_number)?;
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    let failed = |e: pdfium_render::prelude::PdfiumError| {
        error_response(CODE_PDF_OPEN_FAILED, Some(format!("{e:?}")))
    };
    for index in (0..page.objects().len()).rev() {
        if index != path[0] {
            page.objects_mut()
                .remove_object_at_index(index)
                .map_err(failed)?;
        }
    }
    // Only the kept object is left, at index 0.
    let mut parent = page.objects().get(0).map_err(failed)?;
    for &keep in &path[1..] {
        let PdfPageObject::XObjectForm(form) = &mut parent else {
            return Err(error_response(CODE_PDF_OPEN_FAILED, None));
        };
        for index in (0..form.len()).rev() {
            if index != keep {
                form.remove_object_at_index(index).map_err(failed)?;
            }
        }
        let kept = form.get(0).map_err(failed)?;
        parent = kept;
    }
    // `parent` borrows `page`; it must end before `page` is moved out.
    drop(parent);
    Ok(page)
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

/// The listed image with `image_index`, or `InvalidParams` when `PageImages`
/// does not list one.
fn find_listed(listed: Vec<ListedImage>, image_index: u32) -> Result<ListedImage, Response> {
    listed
        .into_iter()
        .find(|image| image.info.image_index == image_index)
        .ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))
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
    let image = find_listed(listed_images(&page)?, image_index)?;

    let (width, height, matrix) = if image.info.native {
        let (width, height, _) =
            photo_output_size(image.placed.px_width, image.placed.px_height, check_pixels);
        let matrix = photo_grid_matrix(
            image.pixel_to_display,
            image.placed.px_width,
            image.placed.px_height,
            width,
            height,
        )
        .ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))?;
        (width, height, matrix)
    } else {
        let frame = image.info.rect_pt;
        let (frame_width, frame_height) = (frame.x1 - frame.x0, frame.y1 - frame.y0);
        let density =
            frame_density(&image).min((check_pixels as f64 / (frame_width * frame_height)).sqrt());
        let width = (frame_width * density).round().max(1.0) as u32;
        let height = (frame_height * density).round().max(1.0) as u32;
        (
            width,
            height,
            [
                density,
                0.0,
                0.0,
                density,
                -frame.x0 * density,
                -frame.y0 * density,
            ],
        )
    };

    let isolated = isolated_page(doc, page_number, &image.placed.path)?;
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
    Ok(count_covered_pixels(&whole, &photo_on_white, &photo_on_black) >= ON_TOP_MIN_PIXELS)
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
        Some(index) => Some(find_listed(listed_images(&page)?, index)?),
        None => None,
    };
    let own_pixels = photo
        .as_ref()
        .filter(|image| request.photo_native && image.info.native);
    if own_pixels.is_none() && !is_render_dpi_in_range(request.dpi) {
        return Err(error_response(CODE_INVALID_PARAMS, None));
    }

    // Everything but the photo is left out of a photo-only render, and the
    // background stays transparent where the format can keep that.
    let photo_only = photo.is_some() && request.photo_only;
    let isolated = match (&photo, photo_only) {
        (Some(image), true) => Some(isolated_page(doc, request.page, &image.placed.path)?),
        _ => None,
    };
    let source = isolated.as_ref().unwrap_or(&page);
    let transparent =
        photo_only && matches!(request.format, RenderFormat::Png | RenderFormat::Webp);
    let background = if transparent {
        PdfColor::new(0, 0, 0, 0)
    } else {
        PdfColor::WHITE
    };
    let gray = request.color == RenderColor::Gray;

    let (width, height, matrix, scaled_down, orientation) = match own_pixels {
        Some(image) => {
            let (px_width, px_height) = (image.placed.px_width, image.placed.px_height);
            let (width, height, scaled_down) = photo_output_size(px_width, px_height, max_pixels);
            let matrix =
                photo_grid_matrix(image.pixel_to_display, px_width, px_height, width, height)
                    .ok_or_else(|| error_response(CODE_INVALID_PARAMS, None))?;
            (
                width,
                height,
                matrix,
                scaled_down,
                Orientation::of(image.pixel_to_display),
            )
        }
        None => {
            let scale = f64::from(request.dpi) / 72.0;
            let width = ((rect.x1 - rect.x0) * scale).round().max(1.0) as u64;
            let height = ((rect.y1 - rect.y0) * scale).round().max(1.0) as u64;
            if width.saturating_mul(height) > max_pixels {
                return Err(error_response(CODE_RENDER_TOO_LARGE, None));
            }
            (
                width as u32,
                height as u32,
                [scale, 0.0, 0.0, scale, -rect.x0 * scale, -rect.y0 * scale],
                false,
                Orientation {
                    transposed: false,
                    flip_x: false,
                    flip_y: false,
                },
            )
        }
    };

    let rgba = render_rgba(&Draw {
        page: source,
        width,
        height,
        matrix,
        background,
        gray,
        annotations: !photo_only,
    })?;
    let (pixels, color_type) = pixels_from_rgba(rgba, request.color, transparent);
    let channels = color_type.bits_per_pixel() as usize / 8;
    let (pixels, out_width, out_height) = reorient(
        pixels,
        width as usize,
        height as usize,
        channels,
        orientation,
    );
    let (out_width, out_height) = (out_width as u32, out_height as u32);
    let bytes = encode_pixels(&pixels, out_width, out_height, color_type, request.format)?;
    Ok((
        Response::RenderRegion {
            width: out_width,
            height: out_height,
            scaled_down,
        },
        bytes,
    ))
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
            let images = page_images(&doc, page).unwrap();
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
                ) => (
                    width,
                    height,
                    scaled_down,
                    image::load_from_memory(&bytes).unwrap(),
                ),
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
