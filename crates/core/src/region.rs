//! Rectangles on a page and the pure arithmetic on them (design §4.6, §6.4).
//!
//! Nothing here depends on pdfium: the worker measures the page, this module
//! only orders and compares the rectangles it is given.

/// A rectangle on a page, in points, in the orientation the page is shown in
/// (after `/Rotate`): the origin is the top-left corner and y grows downwards
/// (design §4.6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl PageRect {
    #[must_use]
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    #[must_use]
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    fn area(&self) -> f64 {
        self.width().max(0.0) * self.height().max(0.0)
    }

    /// The area both rectangles cover, 0 when they only touch or are apart.
    fn intersection_area(&self, other: &Self) -> f64 {
        let width = self.x1.min(other.x1) - self.x0.max(other.x0);
        let height = self.y1.min(other.y1) - self.y0.max(other.y0);
        width.max(0.0) * height.max(0.0)
    }
}

/// How much of a detected photo box and an embedded image's frame must
/// coincide, as [`iou`], for the box to stand for that image (design §4.7).
pub const PHOTO_MATCH_IOU: f64 = 0.85;

/// Intersection over union of two rectangles: the area they share divided by
/// the area they cover together. 0 when they share nothing, 1 when they are
/// the same, and 0 when both are empty.
#[must_use]
pub fn iou(a: &PageRect, b: &PageRect) -> f64 {
    let shared = a.intersection_area(b);
    let union = a.area() + b.area() - shared;
    if union > 0.0 { shared / union } else { 0.0 }
}

/// Whether `rect` belongs to the row that `first` started: their vertical
/// overlap is at least half of the lower (shorter) of the two (design §6.4).
/// Exactly half counts as the same row.
fn is_same_row(first: &PageRect, rect: &PageRect) -> bool {
    let overlap = first.y1.min(rect.y1) - first.y0.max(rect.y0);
    let shorter = first.height().min(rect.height());
    overlap >= shorter * 0.5
}

/// The indices of `rects` in position order (design §6.4): rows from the top,
/// left to right within a row.
///
/// The rectangles are sorted by their top edge and cut into rows from the
/// first one down; a rectangle joins the current row when it overlaps the
/// row's first rectangle enough ([`is_same_row`]), otherwise it starts the
/// next row. Within a row they go by left edge. Rectangles whose sort keys
/// are equal keep the order they were given in.
#[must_use]
pub fn position_order(rects: &[PageRect]) -> Vec<usize> {
    let mut by_top: Vec<usize> = (0..rects.len()).collect();
    by_top.sort_by(|&a, &b| rects[a].y0.total_cmp(&rects[b].y0));

    let mut rows: Vec<Vec<usize>> = Vec::new();
    for index in by_top {
        match rows.last_mut() {
            Some(row) if is_same_row(&rects[row[0]], &rects[index]) => row.push(index),
            _ => rows.push(vec![index]),
        }
    }

    rows.into_iter()
        .flat_map(|mut row| {
            row.sort_by(|&a, &b| rects[a].x0.total_cmp(&rects[b].x0));
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PageRect {
        PageRect { x0, y0, x1, y1 }
    }

    #[test]
    fn iou_of_apart_touching_and_identical_rectangles() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(iou(&a, &rect(20.0, 20.0, 30.0, 30.0)), 0.0);
        assert_eq!(iou(&a, &rect(10.0, 0.0, 20.0, 10.0)), 0.0);
        assert_eq!(iou(&a, &a), 1.0);
    }

    #[test]
    fn iou_when_one_contains_the_other() {
        let outer = rect(0.0, 0.0, 10.0, 10.0);
        let inner = rect(0.0, 0.0, 5.0, 5.0);
        assert_eq!(iou(&outer, &inner), 0.25);
        assert_eq!(iou(&inner, &outer), 0.25);
    }

    #[test]
    fn iou_of_a_partial_overlap() {
        // The overlap is 5 × 10 = 50 of 100 + 100 - 50 = 150.
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 15.0, 10.0);
        assert!((iou(&a, &b) - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn iou_of_empty_rectangles_is_zero() {
        let empty = rect(3.0, 3.0, 3.0, 3.0);
        assert_eq!(iou(&empty, &empty), 0.0);
    }

    #[test]
    fn iou_separates_the_photo_match_threshold() {
        // 100 × 100 against a copy shifted by 7 and by 8 on one axis:
        // 93/107 ≈ 0.869 matches, 92/108 ≈ 0.852 still does, 91/109 ≈ 0.835 does not.
        let frame = rect(0.0, 0.0, 100.0, 100.0);
        let shifted = |dx: f64| rect(dx, 0.0, 100.0 + dx, 100.0);
        assert!(iou(&frame, &shifted(7.0)) >= PHOTO_MATCH_IOU);
        assert!(iou(&frame, &shifted(8.0)) >= PHOTO_MATCH_IOU);
        assert!(iou(&frame, &shifted(9.0)) < PHOTO_MATCH_IOU);
    }

    #[test]
    fn order_of_nothing_and_of_one() {
        assert!(position_order(&[]).is_empty());
        assert_eq!(position_order(&[rect(0.0, 0.0, 1.0, 1.0)]), [0]);
    }

    #[test]
    fn rows_go_top_to_bottom_and_left_to_right() {
        let rects = [
            rect(60.0, 100.0, 90.0, 120.0), // 0: second row, right
            rect(50.0, 0.0, 80.0, 20.0),    // 1: first row, right
            rect(0.0, 100.0, 30.0, 120.0),  // 2: second row, left
            rect(0.0, 0.0, 30.0, 20.0),     // 3: first row, left
        ];
        assert_eq!(position_order(&rects), [3, 1, 2, 0]);
    }

    #[test]
    fn a_rectangle_a_little_lower_stays_in_the_row() {
        // The left one starts 4 lower but overlaps 16 of the 20 height.
        let rects = [rect(0.0, 4.0, 30.0, 24.0), rect(50.0, 0.0, 80.0, 20.0)];
        assert_eq!(position_order(&rects), [0, 1]);
    }

    #[test]
    fn overlap_of_exactly_half_the_lower_rectangle_is_the_same_row() {
        // Both are 10 high; the left one starts 5 lower, so the overlap is 5.
        let rects = [rect(0.0, 5.0, 30.0, 15.0), rect(50.0, 0.0, 80.0, 10.0)];
        assert_eq!(position_order(&rects), [0, 1]);
    }

    #[test]
    fn overlap_below_half_the_lower_rectangle_starts_a_new_row() {
        // The overlap is 4.5 of the 10, so the left one comes second
        // although it is further left.
        let rects = [rect(0.0, 5.5, 30.0, 15.5), rect(50.0, 0.0, 80.0, 10.0)];
        assert_eq!(position_order(&rects), [1, 0]);
    }

    #[test]
    fn halves_are_measured_against_the_lower_of_the_two_heights() {
        // A tall rectangle (0..100) and a short one (10..20): the overlap 10
        // is all of the short one, so they share a row and go by left edge.
        let rects = [rect(50.0, 0.0, 90.0, 100.0), rect(0.0, 10.0, 30.0, 20.0)];
        assert_eq!(position_order(&rects), [1, 0]);
    }

    #[test]
    fn a_row_is_decided_against_its_first_rectangle() {
        // The third overlaps the second by half, but not the first (which
        // started the row), so it starts a new row.
        let rects = [
            rect(0.0, 0.0, 10.0, 10.0),
            rect(20.0, 5.0, 30.0, 15.0),
            rect(40.0, 10.0, 50.0, 20.0),
            rect(0.0, 11.0, 10.0, 21.0),
        ];
        assert_eq!(position_order(&rects), [0, 1, 3, 2]);
    }

    #[test]
    fn rectangles_with_the_same_top_and_left_keep_their_given_order() {
        let rects = [
            rect(10.0, 10.0, 50.0, 50.0),
            rect(10.0, 10.0, 20.0, 20.0),
            rect(10.0, 10.0, 90.0, 90.0),
        ];
        assert_eq!(position_order(&rects), [0, 1, 2]);
    }

    #[test]
    fn the_same_top_with_different_lefts_goes_by_left() {
        let rects = [
            rect(30.0, 10.0, 40.0, 20.0),
            rect(0.0, 10.0, 10.0, 20.0),
            rect(15.0, 10.0, 25.0, 20.0),
        ];
        assert_eq!(position_order(&rects), [1, 2, 0]);
    }
}
