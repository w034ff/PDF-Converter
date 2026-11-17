//! The outer borders of the text regions of the detector's binary map
//! (design ocr §4, 検出), standing in for `cv2.findContours`.
//!
//! Only the minimum-area rectangle of each border is used afterwards, and it
//! depends only on the border's convex hull, so a region is given as its
//! border pixels in any order: the pixels of an 8-connected region that touch
//! the background or the edge on one of their four sides. Holes are left out:
//! their boxes cover the low probabilities inside them and fall below the
//! box threshold, and a hole too small for that falls below the minimum side.

use super::geom::Pt;

/// The border pixels of each 8-connected region of `true` in `mask`
/// (`w` × `h`), regions in the order their top-left pixel comes in a raster
/// scan.
pub(crate) fn region_borders(mask: &[bool], w: usize, h: usize) -> Vec<Vec<Pt>> {
    let mut seen = vec![false; mask.len()];
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    let on = |x: isize, y: isize| {
        x >= 0
            && y >= 0
            && (x as usize) < w
            && (y as usize) < h
            && mask[y as usize * w + x as usize]
    };
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut border = Vec::new();
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            if !(on(x - 1, y) && on(x + 1, y) && on(x, y - 1) && on(x, y + 1)) {
                border.push([x as f64, y as f64]);
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx != 0 || dy != 0) && on(x + dx, y + dy) {
                        let j = (y + dy) as usize * w + (x + dx) as usize;
                        if !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
        }
        regions.push(border);
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(rows: &[&str]) -> (Vec<bool>, usize, usize) {
        let w = rows[0].len();
        let cells = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        (cells, w, rows.len())
    }

    #[test]
    fn diagonal_neighbours_join_and_inner_pixels_are_not_border() {
        let (m, w, h) = mask(&[
            "#.....", //
            ".#....", //
            "...###", //
            "...###", //
            "...###", //
        ]);
        let regions = region_borders(&m, w, h);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].len(), 2);
        // The 3×3 block keeps its eight edge pixels; the centre is inside.
        assert_eq!(regions[1].len(), 8);
        assert!(!regions[1].contains(&[4.0, 3.0]));
    }

    #[test]
    fn an_empty_map_has_no_regions() {
        let (m, w, h) = mask(&["...", "..."]);
        assert!(region_borders(&m, w, h).is_empty());
    }
}
