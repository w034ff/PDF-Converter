//! Page sizing and image placement calculations (design §4.2).

use image::metadata::Orientation;

use crate::probe::ImageInfo;

/// Margin around a page in millimeters for paper sizes (design §4.2).
pub const PAGE_MARGIN_MM: f32 = 10.0;

/// Points per inch in PDF coordinate space (72 pt = 1 inch).
pub const POINTS_PER_INCH: f32 = 72.0;

/// Millimeters per inch (25.4 mm = 1 inch).
pub const MM_PER_INCH: f32 = 25.4;

/// Maximum page side length in points (design §4.2).
///
/// Corresponds to 200 inches (14,400 pt), Acrobat's page limit.
pub const MAX_PAGE_SIDE_PT: f32 = 14400.0;

/// Converts millimeters to points in PDF coordinate space.
#[inline]
fn mm_to_pt(mm: f32) -> f32 {
    mm * POINTS_PER_INCH / MM_PER_INCH
}

/// Converts inches to points in PDF coordinate space.
#[inline]
fn in_to_pt(inch: f32) -> f32 {
    inch * POINTS_PER_INCH
}

/// Standard paper sizes for image-to-PDF conversion (design §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaperSize {
    /// ISO A3 (297 × 420 mm).
    A3,
    /// ISO A4 (210 × 297 mm).
    A4,
    /// ISO A5 (148 × 210 mm).
    A5,
    /// JIS B4 (257 × 364 mm).
    JisB4,
    /// JIS B5 (182 × 257 mm).
    JisB5,
    /// ANSI Letter (8.5 × 11 in).
    Letter,
    /// ANSI Legal (8.5 × 14 in).
    Legal,
}

impl PaperSize {
    /// Returns the portrait dimensions (width, height) in points (design §4.2).
    #[must_use]
    pub fn portrait_dimensions_pt(self) -> (f32, f32) {
        match self {
            Self::A3 => (mm_to_pt(297.0), mm_to_pt(420.0)),
            Self::A4 => (mm_to_pt(210.0), mm_to_pt(297.0)),
            Self::A5 => (mm_to_pt(148.0), mm_to_pt(210.0)),
            Self::JisB4 => (mm_to_pt(257.0), mm_to_pt(364.0)),
            Self::JisB5 => (mm_to_pt(182.0), mm_to_pt(257.0)),
            Self::Letter => (in_to_pt(8.5), in_to_pt(11.0)),
            Self::Legal => (in_to_pt(8.5), in_to_pt(14.0)),
        }
    }
}

/// Orientation choice for a paper page (design §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrientation {
    /// Chooses landscape if display width > display height, otherwise portrait.
    Auto,
    /// Always portrait.
    Portrait,
    /// Always landscape.
    Landscape,
}

/// Page size specification for image-to-PDF conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    /// Fits the page to the image dimensions at its native resolution (no margins).
    Fit,
    /// Places the image onto a paper page with margins, scaled to fit and centered.
    Paper(PaperSize, PageOrientation),
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
    let (oriented_dpi_x, oriented_dpi_y) = if swaps {
        (info.dpi_y as f32, info.dpi_x as f32)
    } else {
        (info.dpi_x as f32, info.dpi_y as f32)
    };

    let raw_w = (display_w * POINTS_PER_INCH) / oriented_dpi_x;
    let raw_h = (display_h * POINTS_PER_INCH) / oriented_dpi_y;

    match page_size {
        PageSize::Fit => {
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
        PageSize::Paper(paper, orientation) => {
            let (portrait_w, portrait_h) = paper.portrait_dimensions_pt();
            let is_landscape = match orientation {
                PageOrientation::Auto => raw_w > raw_h,
                PageOrientation::Portrait => false,
                PageOrientation::Landscape => true,
            };
            let (page_w, page_h) = if is_landscape {
                (portrait_h, portrait_w)
            } else {
                (portrait_w, portrait_h)
            };

            let margin_pt = mm_to_pt(PAGE_MARGIN_MM);
            let avail_w = page_w - 2.0 * margin_pt;
            let avail_h = page_h - 2.0 * margin_pt;

            let scale = (avail_w / raw_w).min(avail_h / raw_h);
            let img_w = raw_w * scale;
            let img_h = raw_h * scale;

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
        make_info_aniso(width, height, orientation, dpi, dpi)
    }

    fn make_info_aniso(
        width: u32,
        height: u32,
        orientation: Orientation,
        dpi_x: u32,
        dpi_y: u32,
    ) -> ImageInfo {
        ImageInfo {
            format: ImageFormat::Png,
            width,
            height,
            orientation,
            dpi_x,
            dpi_y,
            page_count: 1,
            decoded_bytes: 0,
        }
    }

    #[test]
    fn fit_calculates_pt_for_anisotropic_dpi() {
        // fax_standard.tif: 1728 x 1143 px, 204 x 98 dpi
        // raw_w = 1728 * 72 / 204 ≈ 610.0235 pt
        // raw_h = 1143 * 72 / 98 ≈ 839.7551 pt
        let info = make_info_aniso(1728, 1143, Orientation::NoTransforms, 204, 98);
        let layout = calculate_layout(&info, PageSize::Fit);
        assert!((layout.page_width - 609.88).abs() < 0.1);
        assert!((layout.page_height - 839.76).abs() < 0.1);

        // Auto orientation on PaperSize::A4 picks portrait because raw_w (610) < raw_h (840),
        // even though pixel width (1728) > pixel height (1143).
        let layout_a4 =
            calculate_layout(&info, PageSize::Paper(PaperSize::A4, PageOrientation::Auto));
        let (pw, ph) = PaperSize::A4.portrait_dimensions_pt();
        assert_eq!(layout_a4.page_width, pw);
        assert_eq!(layout_a4.page_height, ph);

        // Rotating by 90 degrees swaps dimensions and swaps dpi_x / dpi_y
        let rotated = make_info_aniso(1728, 1143, Orientation::Rotate90, 204, 98);
        let layout_rot = calculate_layout(&rotated, PageSize::Fit);
        // After Rotate90: display_w = 1143, display_h = 1728, oriented_dpi_x = 98, oriented_dpi_y = 204
        assert!((layout_rot.page_width - 839.76).abs() < 0.1);
        assert!((layout_rot.page_height - 609.88).abs() < 0.1);
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
    fn paper_portrait_dimensions_match_design_table() {
        // Verified against design §4.2 table (within 0.01 pt).
        let expected = [
            (PaperSize::A3, 841.89, 1190.55),
            (PaperSize::A4, 595.28, 841.89),
            (PaperSize::A5, 419.53, 595.28),
            (PaperSize::JisB4, 728.50, 1031.81),
            (PaperSize::JisB5, 515.91, 728.50),
            (PaperSize::Letter, 612.0, 792.0),
            (PaperSize::Legal, 612.0, 1008.0),
        ];

        let img_p = make_info(100, 200, Orientation::NoTransforms, 96);
        let img_l = make_info(200, 100, Orientation::NoTransforms, 96);

        for (paper, exp_w, exp_h) in expected {
            let (pw, ph) = paper.portrait_dimensions_pt();
            assert!(
                (pw - exp_w).abs() < 0.01,
                "{paper:?} portrait width {pw} differs from expected {exp_w}"
            );
            assert!(
                (ph - exp_h).abs() < 0.01,
                "{paper:?} portrait height {ph} differs from expected {exp_h}"
            );

            // Portrait layout
            let layout_p =
                calculate_layout(&img_p, PageSize::Paper(paper, PageOrientation::Portrait));
            assert_eq!(layout_p.page_width, pw);
            assert_eq!(layout_p.page_height, ph);

            // Landscape layout swaps width and height
            let layout_l =
                calculate_layout(&img_l, PageSize::Paper(paper, PageOrientation::Landscape));
            assert_eq!(layout_l.page_width, ph);
            assert_eq!(layout_l.page_height, pw);
        }
    }

    #[test]
    fn paper_margin_is_constant_across_sizes() {
        let expected_margin_pt = PAGE_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;
        let img = make_info(500, 500, Orientation::NoTransforms, 96);

        for paper in [PaperSize::A5, PaperSize::A4, PaperSize::A3] {
            let layout = calculate_layout(&img, PageSize::Paper(paper, PageOrientation::Portrait));
            let (pw, ph) = paper.portrait_dimensions_pt();
            let avail_w = pw - 2.0 * expected_margin_pt;
            let avail_h = ph - 2.0 * expected_margin_pt;
            let avail_min = avail_w.min(avail_h);

            assert!((layout.image_rect.width - avail_min).abs() < 1e-4);
            assert!((layout.image_rect.height - avail_min).abs() < 1e-4);
            // Margin remains the same distance
            let expected_x = expected_margin_pt + (avail_w - avail_min) / 2.0;
            let expected_y = expected_margin_pt + (avail_h - avail_min) / 2.0;
            assert!((layout.image_rect.x - expected_x).abs() < 1e-4);
            assert!((layout.image_rect.y - expected_y).abs() < 1e-4);
        }
    }

    #[test]
    fn paper_auto_chooses_landscape_or_portrait_a4_and_letter() {
        for paper in [PaperSize::A4, PaperSize::Letter] {
            let (portrait_w, portrait_h) = paper.portrait_dimensions_pt();

            // Wider than tall -> Landscape
            let landscape_img = make_info(1200, 800, Orientation::NoTransforms, 96);
            let layout_l = calculate_layout(
                &landscape_img,
                PageSize::Paper(paper, PageOrientation::Auto),
            );
            assert_eq!(layout_l.page_width, portrait_h);
            assert_eq!(layout_l.page_height, portrait_w);

            // Taller than wide -> Portrait
            let portrait_img = make_info(800, 1200, Orientation::NoTransforms, 96);
            let layout_p =
                calculate_layout(&portrait_img, PageSize::Paper(paper, PageOrientation::Auto));
            assert_eq!(layout_p.page_width, portrait_w);
            assert_eq!(layout_p.page_height, portrait_h);

            // Square image (w == h) -> Portrait
            let square_img = make_info(500, 500, Orientation::NoTransforms, 96);
            let layout_s =
                calculate_layout(&square_img, PageSize::Paper(paper, PageOrientation::Auto));
            assert_eq!(layout_s.page_width, portrait_w);
            assert_eq!(layout_s.page_height, portrait_h);
        }
    }

    #[test]
    fn paper_portrait_always_uses_portrait_dimensions_a4_and_letter() {
        let expected_margin_pt = PAGE_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;

        for paper in [PaperSize::A4, PaperSize::Letter] {
            let (portrait_w, portrait_h) = paper.portrait_dimensions_pt();
            let avail_w = portrait_w - 2.0 * expected_margin_pt;
            let avail_h = portrait_h - 2.0 * expected_margin_pt;

            // Landscape image on portrait page: width fills avail_w, centered vertically
            let landscape_img = make_info(1200, 800, Orientation::NoTransforms, 96);
            let layout_l = calculate_layout(
                &landscape_img,
                PageSize::Paper(paper, PageOrientation::Portrait),
            );
            assert_eq!(layout_l.page_width, portrait_w);
            assert_eq!(layout_l.page_height, portrait_h);
            assert!((layout_l.image_rect.width - avail_w).abs() < 1e-4);
            assert!((layout_l.image_rect.x - expected_margin_pt).abs() < 1e-4);
            let exp_h = avail_w * (800.0 / 1200.0);
            assert!((layout_l.image_rect.height - exp_h).abs() < 1e-4);
            let exp_y = expected_margin_pt + (avail_h - exp_h) / 2.0;
            assert!((layout_l.image_rect.y - exp_y).abs() < 1e-4);

            // Portrait image on portrait page
            let portrait_img = make_info(800, 1200, Orientation::NoTransforms, 96);
            let layout_p = calculate_layout(
                &portrait_img,
                PageSize::Paper(paper, PageOrientation::Portrait),
            );
            assert_eq!(layout_p.page_width, portrait_w);
            assert_eq!(layout_p.page_height, portrait_h);

            // Square image on portrait page
            let square_img = make_info(500, 500, Orientation::NoTransforms, 96);
            let layout_s = calculate_layout(
                &square_img,
                PageSize::Paper(paper, PageOrientation::Portrait),
            );
            assert_eq!(layout_s.page_width, portrait_w);
            assert_eq!(layout_s.page_height, portrait_h);
        }
    }

    #[test]
    fn paper_landscape_always_uses_landscape_dimensions_a4_and_letter() {
        let expected_margin_pt = PAGE_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;

        for paper in [PaperSize::A4, PaperSize::Letter] {
            let (portrait_w, portrait_h) = paper.portrait_dimensions_pt();
            let avail_w = portrait_h - 2.0 * expected_margin_pt;
            let avail_h = portrait_w - 2.0 * expected_margin_pt;

            // Portrait image on landscape page: height fills avail_h, centered horizontally
            let portrait_img = make_info(800, 1200, Orientation::NoTransforms, 96);
            let layout_p = calculate_layout(
                &portrait_img,
                PageSize::Paper(paper, PageOrientation::Landscape),
            );
            assert_eq!(layout_p.page_width, portrait_h);
            assert_eq!(layout_p.page_height, portrait_w);
            assert!((layout_p.image_rect.height - avail_h).abs() < 1e-4);
            assert!((layout_p.image_rect.y - expected_margin_pt).abs() < 1e-4);
            let exp_w = avail_h * (800.0 / 1200.0);
            assert!((layout_p.image_rect.width - exp_w).abs() < 1e-4);
            let exp_x = expected_margin_pt + (avail_w - exp_w) / 2.0;
            assert!((layout_p.image_rect.x - exp_x).abs() < 1e-4);

            // Landscape image on landscape page
            let landscape_img = make_info(1200, 800, Orientation::NoTransforms, 96);
            let layout_l = calculate_layout(
                &landscape_img,
                PageSize::Paper(paper, PageOrientation::Landscape),
            );
            assert_eq!(layout_l.page_width, portrait_h);
            assert_eq!(layout_l.page_height, portrait_w);

            // Square image on landscape page
            let square_img = make_info(500, 500, Orientation::NoTransforms, 96);
            let layout_s = calculate_layout(
                &square_img,
                PageSize::Paper(paper, PageOrientation::Landscape),
            );
            assert_eq!(layout_s.page_width, portrait_h);
            assert_eq!(layout_s.page_height, portrait_w);
        }
    }

    #[test]
    fn paper_applies_margin_and_centers() {
        assert_eq!(PAGE_MARGIN_MM, 10.0);
        let expected_margin_pt = 10.0 * POINTS_PER_INCH / MM_PER_INCH;
        let img = make_info(400, 200, Orientation::NoTransforms, 96);
        let layout = calculate_layout(&img, PageSize::Paper(PaperSize::A4, PageOrientation::Auto));

        let (portrait_w, portrait_h) = PaperSize::A4.portrait_dimensions_pt();
        // Landscape page
        let avail_w = portrait_h - 2.0 * expected_margin_pt;
        let avail_h = portrait_w - 2.0 * expected_margin_pt;

        assert!((layout.image_rect.width - avail_w).abs() < 1e-4);
        assert!((layout.image_rect.x - expected_margin_pt).abs() < 1e-4);
        // Centered vertically
        let expected_y = expected_margin_pt + (avail_h - layout.image_rect.height) / 2.0;
        assert!((layout.image_rect.y - expected_y).abs() < 1e-4);
    }

    #[test]
    fn paper_scales_up_small_images_and_scales_down_large_images_a4_and_letter() {
        let margin_pt = PAGE_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;

        for paper in [PaperSize::A4, PaperSize::Letter] {
            let (portrait_w, portrait_h) = paper.portrait_dimensions_pt();
            let avail_h = portrait_h - 2.0 * margin_pt;
            let avail_h_landscape = portrait_w - 2.0 * margin_pt;

            // Very small image (10x20 px) in Auto (portrait)
            let small = make_info(10, 20, Orientation::NoTransforms, 96);
            let layout_small =
                calculate_layout(&small, PageSize::Paper(paper, PageOrientation::Auto));
            assert!((layout_small.image_rect.height - avail_h).abs() < 1e-3);

            // Small image scaled up in Portrait and Landscape
            let layout_small_p =
                calculate_layout(&small, PageSize::Paper(paper, PageOrientation::Portrait));
            assert!((layout_small_p.image_rect.height - avail_h).abs() < 1e-3);
            let layout_small_l =
                calculate_layout(&small, PageSize::Paper(paper, PageOrientation::Landscape));
            assert!((layout_small_l.image_rect.height - avail_h_landscape).abs() < 1e-3);

            // Very large image (10000x20000 px)
            let large = make_info(10000, 20000, Orientation::NoTransforms, 96);
            let layout_large =
                calculate_layout(&large, PageSize::Paper(paper, PageOrientation::Auto));
            assert!((layout_large.image_rect.height - avail_h).abs() < 1e-3);

            let layout_large_p =
                calculate_layout(&large, PageSize::Paper(paper, PageOrientation::Portrait));
            assert!((layout_large_p.image_rect.height - avail_h).abs() < 1e-3);

            let layout_large_l =
                calculate_layout(&large, PageSize::Paper(paper, PageOrientation::Landscape));
            assert!((layout_large_l.image_rect.height - avail_h_landscape).abs() < 1e-3);
        }
    }

    #[test]
    fn paper_respects_exif_90_degree_rotation_dimensions_a4_and_letter() {
        // Raw pixels: 160 x 96 (wider than tall), but Rotate90 swaps display to 96 x 160 (taller than wide).
        let rotated_img = make_info(160, 96, Orientation::Rotate90, 96);
        let margin_pt = PAGE_MARGIN_MM * POINTS_PER_INCH / MM_PER_INCH;

        for paper in [PaperSize::A4, PaperSize::Letter] {
            let (portrait_w, portrait_h) = paper.portrait_dimensions_pt();

            // Auto uses display dimensions: 96 x 160 is portrait
            let layout_auto =
                calculate_layout(&rotated_img, PageSize::Paper(paper, PageOrientation::Auto));
            assert_eq!(layout_auto.page_width, portrait_w);
            assert_eq!(layout_auto.page_height, portrait_h);

            // Portrait: uses display dimensions
            let layout_p = calculate_layout(
                &rotated_img,
                PageSize::Paper(paper, PageOrientation::Portrait),
            );
            let avail_h_p = portrait_h - 2.0 * margin_pt;
            assert!((layout_p.image_rect.height - avail_h_p).abs() < 1e-3);

            // Landscape: uses display dimensions
            let layout_l = calculate_layout(
                &rotated_img,
                PageSize::Paper(paper, PageOrientation::Landscape),
            );
            let avail_h_l = portrait_w - 2.0 * margin_pt;
            assert!((layout_l.image_rect.height - avail_h_l).abs() < 1e-3);
        }
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
