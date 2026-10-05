//! Page sizing and image placement calculations (design §4.2).

use image::metadata::Orientation;

use crate::probe::ImageInfo;

/// Standard A4 page width in points (210 mm).
pub const A4_WIDTH_PT: f32 = 595.28;

/// Standard A4 page height in points (297 mm).
pub const A4_HEIGHT_PT: f32 = 841.89;

/// Margin around an A4 page in millimeters (design §4.2).
pub const A4_MARGIN_MM: f32 = 10.0;

/// Points per inch in PDF coordinate space (72 pt = 1 inch).
pub const POINTS_PER_INCH: f32 = 72.0;

/// Millimeters per inch (25.4 mm = 1 inch).
pub const MM_PER_INCH: f32 = 25.4;

/// Maximum page side length in points (design §4.2).
///
/// Corresponds to 200 inches (14,400 pt), Acrobat's page limit.
pub const MAX_PAGE_SIDE_PT: f32 = 14400.0;

/// Page size specification for image-to-PDF conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    /// Fits the page to the image dimensions at its native resolution (no margins).
    Fit,
    /// Places the image onto an A4 page with margins, scaled to fit and centered.
    A4,
}

/// A 2D rectangle in points with origin at the top-left corner (Y-down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// X coordinate of the top-left corner.
    pub x: f32,
    /// Y coordinate of the top-left corner.
    pub y: f32,
    /// Width of the rectangle.
    pub width: f32,
    /// Height of the rectangle.
    pub height: f32,
}

/// Calculated page layout result for placing an image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageLayout {
    /// Total width of the PDF page in points.
    pub page_width: f32,
    /// Total height of the PDF page in points.
    pub page_height: f32,
    /// Rectangle in points occupied by the oriented image.
    pub image_rect: Rect,
}

/// Returns whether the given EXIF orientation swaps width and height (design §4.2).
#[must_use]
pub fn orientation_swaps_dimensions(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// Calculates the page size and image placement rectangle (design §4.2).
///
/// This is a pure function. Coordinates use top-left origin with Y pointing down.
#[must_use]
pub fn calculate_layout(info: &ImageInfo, page_size: PageSize) -> PageLayout {
    let swaps = orientation_swaps_dimensions(info.orientation);
    let (display_w, display_h) = if swaps {
        (info.height as f32, info.width as f32)
    } else {
        (info.width as f32, info.height as f32)
    };

    match page_size {
        PageSize::Fit => {
            let raw_w = (display_w * POINTS_PER_INCH) / info.dpi as f32;
            let raw_h = (display_h * POINTS_PER_INCH) / info.dpi as f32;
            let max_side = raw_w.max(raw_h);
            let scale = if max_side > MAX_PAGE_SIDE_PT {
                MAX_PAGE_SIDE_PT / max_side
            } else {
                1.0
            };
            let page_w = raw_w * scale;
            let page_h = raw_h * scale;

            PageLayout {
                page_width: page_w,
                page_height: page_h,
                image_rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: page_w,
                    height: page_h,
                },
            }
        }
        PageSize::A4 => {
            // Images with display width > display height use landscape orientation,
            // otherwise portrait orientation (design §4.2).
            let is_landscape = display_w > display_h;
            let (page_w, page_h) = if is_landscape {
                (A4_HEIGHT_PT, A4_WIDTH_PT)
            } else {
                (A4_WIDTH_PT, A4_HEIGHT_PT)
            };

            let margin_pt = A4_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;
            let avail_w = page_w - 2.0 * margin_pt;
            let avail_h = page_h - 2.0 * margin_pt;

            let scale = (avail_w / display_w).min(avail_h / display_h);
            let img_w = display_w * scale;
            let img_h = display_h * scale;

            let x = margin_pt + (avail_w - img_w) / 2.0;
            let y = margin_pt + (avail_h - img_h) / 2.0;

            PageLayout {
                page_width: page_w,
                page_height: page_h,
                image_rect: Rect {
                    x,
                    y,
                    width: img_w,
                    height: img_h,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::ImageFormat;

    fn make_info(width: u32, height: u32, orientation: Orientation, dpi: u32) -> ImageInfo {
        ImageInfo {
            format: ImageFormat::Png,
            width,
            height,
            orientation,
            dpi,
        }
    }

    #[test]
    fn fit_calculates_pt_for_96_and_300_dpi() {
        // 96 dpi: 1 px = 72 / 96 = 0.75 pt
        let info_96 = make_info(192, 96, Orientation::NoTransforms, 96);
        let layout_96 = calculate_layout(&info_96, PageSize::Fit);
        assert!((layout_96.page_width - 144.0).abs() < 1e-4);
        assert!((layout_96.page_height - 72.0).abs() < 1e-4);
        assert_eq!(layout_96.image_rect.x, 0.0);
        assert_eq!(layout_96.image_rect.y, 0.0);
        assert_eq!(layout_96.image_rect.width, layout_96.page_width);
        assert_eq!(layout_96.image_rect.height, layout_96.page_height);

        // 300 dpi: 1 px = 72 / 300 = 0.24 pt
        let info_300 = make_info(300, 600, Orientation::NoTransforms, 300);
        let layout_300 = calculate_layout(&info_300, PageSize::Fit);
        assert!((layout_300.page_width - 72.0).abs() < 1e-4);
        assert!((layout_300.page_height - 144.0).abs() < 1e-4);
    }

    #[test]
    fn a4_chooses_landscape_or_portrait() {
        // Wider than tall -> Landscape (width = 841.89, height = 595.28)
        let landscape_img = make_info(1200, 800, Orientation::NoTransforms, 96);
        let layout_l = calculate_layout(&landscape_img, PageSize::A4);
        assert_eq!(layout_l.page_width, A4_HEIGHT_PT);
        assert_eq!(layout_l.page_height, A4_WIDTH_PT);

        // Taller than wide -> Portrait (width = 595.28, height = 841.89)
        let portrait_img = make_info(800, 1200, Orientation::NoTransforms, 96);
        let layout_p = calculate_layout(&portrait_img, PageSize::A4);
        assert_eq!(layout_p.page_width, A4_WIDTH_PT);
        assert_eq!(layout_p.page_height, A4_HEIGHT_PT);

        // Square image (w == h) -> Portrait
        let square_img = make_info(500, 500, Orientation::NoTransforms, 96);
        let layout_s = calculate_layout(&square_img, PageSize::A4);
        assert_eq!(layout_s.page_width, A4_WIDTH_PT);
        assert_eq!(layout_s.page_height, A4_HEIGHT_PT);
    }

    #[test]
    fn a4_applies_margin_and_centers() {
        assert_eq!(A4_MARGIN_MM, 10.0);
        let expected_margin_pt = 10.0 * POINTS_PER_INCH / MM_PER_INCH;
        let img = make_info(400, 200, Orientation::NoTransforms, 96);
        let layout = calculate_layout(&img, PageSize::A4);

        // Landscape page
        let avail_w = A4_HEIGHT_PT - 2.0 * expected_margin_pt;
        let avail_h = A4_WIDTH_PT - 2.0 * expected_margin_pt;

        // Image aspect ratio: 400 / 200 = 2.0.
        // Avail aspect: avail_w / avail_h ~ (841.89 - 56.69) / (595.28 - 56.69) = 785.2 / 538.59 ~ 1.458.
        // Limited by width.
        assert!((layout.image_rect.width - avail_w).abs() < 1e-4);
        assert!((layout.image_rect.x - expected_margin_pt).abs() < 1e-4);
        // Centered vertically
        let expected_y = expected_margin_pt + (avail_h - layout.image_rect.height) / 2.0;
        assert!((layout.image_rect.y - expected_y).abs() < 1e-4);
    }

    #[test]
    fn a4_scales_up_small_images_and_scales_down_large_images() {
        let margin_pt = A4_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;

        // Very small image (10x20 px)
        let small = make_info(10, 20, Orientation::NoTransforms, 96);
        let layout_small = calculate_layout(&small, PageSize::A4);
        let avail_h = A4_HEIGHT_PT - 2.0 * margin_pt;
        assert!((layout_small.image_rect.height - avail_h).abs() < 1e-3);

        // Very large image (10000x20000 px)
        let large = make_info(10000, 20000, Orientation::NoTransforms, 96);
        let layout_large = calculate_layout(&large, PageSize::A4);
        assert!((layout_large.image_rect.height - avail_h).abs() < 1e-3);
    }

    #[test]
    fn fit_scales_down_when_exceeding_max_page_side() {
        // Image producing raw dimension > MAX_PAGE_SIDE_PT (14,400 pt)
        // At 72 dpi, 20,000 px = 20,000 pt.
        let huge = make_info(
            20000,
            10000,
            Orientation::NoTransforms,
            POINTS_PER_INCH as u32,
        );
        let layout = calculate_layout(&huge, PageSize::Fit);
        assert!((layout.page_width - MAX_PAGE_SIDE_PT).abs() < 1e-2);
        assert!((layout.page_height - 7200.0).abs() < 1e-2);
        assert_eq!(layout.image_rect.x, 0.0);
        assert_eq!(layout.image_rect.y, 0.0);
        assert!((layout.image_rect.width - MAX_PAGE_SIDE_PT).abs() < 1e-2);
        assert!((layout.image_rect.height - 7200.0).abs() < 1e-2);
    }

    #[test]
    fn orientation_swaps_dimensions_cases() {
        assert!(!orientation_swaps_dimensions(Orientation::NoTransforms));
        assert!(!orientation_swaps_dimensions(Orientation::FlipHorizontal));
        assert!(!orientation_swaps_dimensions(Orientation::Rotate180));
        assert!(!orientation_swaps_dimensions(Orientation::FlipVertical));
        assert!(orientation_swaps_dimensions(Orientation::Rotate90FlipH));
        assert!(orientation_swaps_dimensions(Orientation::Rotate90));
        assert!(orientation_swaps_dimensions(Orientation::Rotate270FlipH));
        assert!(orientation_swaps_dimensions(Orientation::Rotate270));
    }

    #[test]
    fn handles_all_eight_orientations() {
        // Base dimensions: 160 x 96
        let orientations = [
            (Orientation::NoTransforms, false),
            (Orientation::FlipHorizontal, false),
            (Orientation::Rotate180, false),
            (Orientation::FlipVertical, false),
            (Orientation::Rotate90FlipH, true),
            (Orientation::Rotate90, true),
            (Orientation::Rotate270FlipH, true),
            (Orientation::Rotate270, true),
        ];

        for (orient, swaps) in orientations {
            assert_eq!(orientation_swaps_dimensions(orient), swaps);
            let info = make_info(160, 96, orient, POINTS_PER_INCH as u32);
            let layout = calculate_layout(&info, PageSize::Fit);

            if swaps {
                assert_eq!(layout.page_width, 96.0, "failed on {orient:?}");
                assert_eq!(layout.page_height, 160.0, "failed on {orient:?}");
            } else {
                assert_eq!(layout.page_width, 160.0, "failed on {orient:?}");
                assert_eq!(layout.page_height, 96.0, "failed on {orient:?}");
            }
        }
    }
}
