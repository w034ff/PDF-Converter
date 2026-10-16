//! The picture and the placements of `crop_align.pdf` (design §11.1).
//!
//! `gen_fixtures` writes the PDF from these values and the worker tests derive
//! their expectations from the same ones, so the pixels of the picture and the
//! frames it is placed in are defined once.

#![allow(
    dead_code,
    reason = "constants are shared across test files where each file uses a subset"
)]

/// An affine matrix as the PDF `cm` operator takes it: `[a b c d e f]`,
/// x' = a·x + c·y + e, y' = b·x + d·y + f.
pub type Matrix = [f64; 6];

/// Width and height of the picture in pixels. Odd and different, so that no
/// flip, turn or transpose maps the picture onto itself.
pub const PICTURE_WIDTH: u32 = 61;
pub const PICTURE_HEIGHT: u32 = 43;

/// The picture's pixel at column `x`, row `y` (row 0 on top): red follows the
/// column, green the row, blue both, so every pixel of every orientation
/// can be told apart.
pub fn picture_pixel(x: u32, y: u32) -> [u8; 3] {
    [
        (x * 255 / (PICTURE_WIDTH - 1)) as u8,
        (y * 255 / (PICTURE_HEIGHT - 1)) as u8,
        ((x * 7 + y * 13) % 256) as u8,
    ]
}

/// The picture as the bytes of an RGB image stream, row by row.
pub fn picture_rgb() -> Vec<u8> {
    (0..PICTURE_HEIGHT)
        .flat_map(|y| (0..PICTURE_WIDTH).flat_map(move |x| picture_pixel(x, y)))
        .collect()
}

/// The soft mask's value at column `x`, row `y`: transparent at the left,
/// graded over the first columns and opaque from column 32 on, with a
/// row-dependent step so the grade is not constant down a column.
pub fn soft_mask_alpha(x: u32, y: u32) -> u8 {
    (x * 8 + y).min(255) as u8
}

/// The soft mask as the bytes of a gray image stream, row by row.
pub fn soft_mask_gray() -> Vec<u8> {
    (0..PICTURE_HEIGHT)
        .flat_map(|y| (0..PICTURE_WIDTH).map(move |x| soft_mask_alpha(x, y)))
        .collect()
}

/// Size of every page's `MediaBox` in pt.
pub const MEDIA_BOX_PT: (f64, f64) = (595.276, 841.89);

/// `CropBox` of every page: [left, bottom, right, top]. Away from the origin,
/// so that a wrong origin shows.
pub const CROP_BOX_PT: [f64; 4] = [10.3, 7.7, 590.1, 830.2];

/// The `/Rotate` of pages 1, 2 and 3.
pub const PAGE_ROTATIONS: [u32; 3] = [0, 90, 270];

/// A fractional position and scale, no turn.
pub const FRACTIONAL: Matrix = [173.37, 0.0, 0.0, 115.58, 101.23, 603.71];
/// Mirrored left to right.
pub const MIRRORED: Matrix = [-173.37, 0.0, 0.0, 115.58, 510.0, 453.21];
/// Turned a quarter, counter-clockwise on the page.
pub const TURNED: Matrix = [0.0, 150.5, -106.1, 0.0, 250.7, 303.3];
/// Turned and sheared: not placed on the pixel grid by any flip or quarter turn.
pub const SHEARED: Matrix = [120.0, 40.0, -30.0, 90.0, 400.5, 250.25];
/// The picture with a soft mask.
pub const SOFT_MASKED: Matrix = [100.4, 0.0, 0.0, 70.2, 60.3, 150.7];
/// A frame under `MIN_PHOTO_SIDE_PT`.
pub const SMALL_FRAME: Matrix = [11.0, 0.0, 0.0, 7.6, 300.1, 60.3];
/// A frame that a clip path cuts.
pub const CLIPPED: Matrix = [150.3, 0.0, 0.0, 100.7, 220.1, 80.6];
/// The clip path of [`CLIPPED`]: x, y, width, height.
pub const CLIP_RECT: [f64; 4] = [250.0, 100.0, 80.0, 60.0];
/// A 2 × 2 pixel image, below `MIN_PHOTO_SIDE_PX`.
pub const TINY_IMAGE: Matrix = [30.2, 0.0, 0.0, 30.2, 101.5, 760.5];
/// The filled rectangle over [`FRACTIONAL`]: x, y, width, height.
pub const OVERLAY_RECT: [f64; 4] = [150.0, 640.0, 60.0, 40.0];

/// The filled rectangle inside [`SHEARED`]: x, y, width, height. Its image is
/// not placed on the pixel grid, so it is checked by its frame.
pub const SHEAR_OVERLAY_RECT: [f64; 4] = [430.0, 300.0, 30.0, 30.0];
/// The filled rectangle over the left, only partly opaque, columns of
/// [`SOFT_MASKED`]: x, y, width, height. It changes the page there, but the
/// image does not cover those pixels.
pub const PARTIAL_OVERLAY_RECT: [f64; 4] = [62.0, 160.0, 40.0, 55.0];

/// The stamp annotation's `Rect`, over the top-right of [`FRACTIONAL`], and
/// the matrix of the picture in its appearance stream.
pub const ANNOTATION_RECT: [f64; 4] = [220.0, 690.0, 270.0, 715.0];
pub const ANNOTATION_IMAGE: Matrix = [100.0, 0.0, 0.0, 60.0, 0.0, 0.0];

/// The picture inside two nested forms: the `cm` before the outer form's
/// `Do`, the outer form's `/Matrix`, the `cm` before the inner form's `Do`
/// (inside the outer form), the inner form's `/Matrix` and the `cm` of the
/// picture inside the inner form.
pub const OUTER_FORM_CM: Matrix = [1.0, 0.0, 0.0, 1.0, 400.5, 690.25];
pub const OUTER_FORM_MATRIX: Matrix = [0.8, 0.0, 0.0, 0.8, 30.5, 20.25];
pub const INNER_FORM_CM: Matrix = [0.9, 0.0, 0.0, 0.9, 20.25, 10.5];
pub const INNER_FORM_MATRIX: Matrix = [1.1, 0.0, 0.0, 1.1, 5.0, 5.5];
pub const NESTED_IMAGE: Matrix = [100.2, 0.0, 0.0, 70.7, 0.0, 0.0];

/// `m` applied to the point (`x`, `y`).
pub fn apply(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// `first` applied, then `then` (PDF's row-vector convention).
pub fn then(first: Matrix, then: Matrix) -> Matrix {
    [
        first[0] * then[0] + first[1] * then[2],
        first[0] * then[1] + first[1] * then[3],
        first[2] * then[0] + first[3] * then[2],
        first[2] * then[1] + first[3] * then[3],
        first[4] * then[0] + first[5] * then[2] + then[4],
        first[4] * then[1] + first[5] * then[3] + then[5],
    ]
}

/// Where the picture in the nested forms sits on the page: its matrix through
/// the inner form, the outer form and the page, in the order the PDF
/// specification applies them (a form's `/Matrix`, then the `cm` in force at
/// its `Do`).
pub fn nested_image_on_page() -> Matrix {
    let mut m = NESTED_IMAGE;
    for next in [
        INNER_FORM_MATRIX,
        INNER_FORM_CM,
        OUTER_FORM_MATRIX,
        OUTER_FORM_CM,
    ] {
        m = then(m, next);
    }
    m
}

/// A picture placed on the page that `PageImages` lists.
#[derive(Debug, Clone, Copy)]
pub struct ListedImage {
    /// The `imageIndex`: the number of images drawn before it, listed or not.
    pub image_index: u32,
    /// Unit square → page, in user space.
    pub matrix: Matrix,
    /// Whether the placement is a flip or quarter turn only.
    pub native: bool,
    pub has_soft_mask: bool,
}

/// The images `PageImages` lists on each page, in drawing order. Drawn but not
/// listed: index 1 ([`TINY_IMAGE`]), index 6 ([`SMALL_FRAME`]) and the
/// annotation's picture.
pub fn listed_images() -> [ListedImage; 7] {
    let image = |image_index, matrix, native, has_soft_mask| ListedImage {
        image_index,
        matrix,
        native,
        has_soft_mask,
    };
    [
        image(0, FRACTIONAL, true, false),
        image(2, MIRRORED, true, false),
        image(3, TURNED, true, false),
        image(4, SHEARED, false, false),
        image(5, SOFT_MASKED, true, true),
        image(7, CLIPPED, true, false),
        image(8, nested_image_on_page(), true, false),
    ]
}

/// Where a point of the page (user space) is in the shown page of page
/// `page_index` (0-based): origin top-left, y down, `CropBox` and `/Rotate`
/// applied.
pub fn to_shown_page(page_index: usize, x: f64, y: f64) -> (f64, f64) {
    let [left, bottom, right, top] = CROP_BOX_PT;
    let (u, v) = (x - left, y - bottom);
    let (width, height) = (right - left, top - bottom);
    match PAGE_ROTATIONS[page_index] {
        0 => (u, height - v),
        // The page turns clockwise: its left edge becomes the top.
        90 => (v, u),
        // The page turns counter-clockwise: its left edge becomes the bottom.
        270 => (height - v, width - u),
        other => unreachable!("fixture page rotation {other}"),
    }
}
