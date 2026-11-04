//! The organize grid's state without any UI: pages, selection, focus,
//! reordering and the geometry of the grid. The Slint side only draws what
//! this module computes, so all of it can be unit tested.

use std::collections::HashSet;

/// Same sizes as `src/features/organize/organizeGrid.ts`.
pub const CELL_WIDTH: f32 = 124.0;
pub const PAGE_BOX: f32 = 124.0;
pub const PAGE_MAX_SIDE: f32 = 119.0;
pub const CELL_HEIGHT: f32 = PAGE_BOX + 6.0 + 18.0;
pub const COLUMN_GAP: f32 = 16.0;
pub const ROW_GAP: f32 = 18.0;
pub const COLUMN_PITCH: f32 = CELL_WIDTH + COLUMN_GAP;
pub const ROW_PITCH: f32 = CELL_HEIGHT + ROW_GAP;
/// Padding around the grid inside the scroll area.
pub const GRID_PADDING: f32 = 20.0;
pub const OVERSCAN_ROWS: usize = 2;
/// How far from the top / bottom edge a drag starts scrolling, and the
/// fastest step per tick at the edge itself.
pub const DRAG_SCROLL_EDGE: f32 = 40.0;
pub const DRAG_SCROLL_MAX_STEP: f32 = 24.0;
const MAX_HISTORY: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Page {
    /// Stable identity of the page across reorderings.
    pub key: u32,
    pub source: u32,
    /// 0-based page of the source.
    pub page: u32,
    /// Clockwise degrees: 0, 90, 180, 270.
    pub rotation: u16,
    pub width_pt: f32,
    pub height_pt: f32,
}

#[derive(Debug, Default, Clone)]
pub struct Organizer {
    pub pages: Vec<Page>,
    pub selected: HashSet<u32>,
    /// Where a Shift range starts.
    pub anchor: Option<u32>,
    /// The page the keyboard works on.
    pub focus: Option<u32>,
    next_key: u32,
    undo: Vec<Vec<Page>>,
    redo: Vec<Vec<Page>>,
}

pub fn column_count(width: f32) -> usize {
    let inner = width - 2.0 * GRID_PADDING;
    (((inner + COLUMN_GAP) / COLUMN_PITCH).floor() as isize).max(1) as usize
}

pub fn row_count(count: usize, columns: usize) -> usize {
    count.div_ceil(columns)
}

pub fn content_height(count: usize, columns: usize) -> f32 {
    let rows = row_count(count, columns);
    if rows == 0 {
        0.0
    } else {
        rows as f32 * CELL_HEIGHT + (rows - 1) as f32 * ROW_GAP + 2.0 * GRID_PADDING
    }
}

/// Top left of the cell at `index`, in content coordinates.
pub fn cell_origin(index: usize, columns: usize) -> (f32, f32) {
    let row = index / columns;
    let col = index % columns;
    (
        GRID_PADDING + col as f32 * COLUMN_PITCH,
        GRID_PADDING + row as f32 * ROW_PITCH,
    )
}

/// The indices of the cells to draw for a viewport at `scroll_y` of height
/// `view_height`, with [`OVERSCAN_ROWS`] above and below.
pub fn visible_range(
    count: usize,
    columns: usize,
    scroll_y: f32,
    view_height: f32,
) -> std::ops::Range<usize> {
    if count == 0 {
        return 0..0;
    }
    let top = (scroll_y - GRID_PADDING).max(0.0);
    let first_row = ((top / ROW_PITCH).floor() as usize).saturating_sub(OVERSCAN_ROWS);
    let last_row = ((top + view_height) / ROW_PITCH).ceil() as usize + OVERSCAN_ROWS;
    let start = (first_row * columns).min(count);
    let end = ((last_row + 1) * columns).min(count);
    start..end
}

/// The size a page is drawn at inside its square box, after rotation.
pub fn fitted_size(width_pt: f32, height_pt: f32, rotation: u16) -> (f32, f32) {
    let (w, h) = if rotation % 180 == 90 {
        (height_pt, width_pt)
    } else {
        (width_pt, height_pt)
    };
    let scale = PAGE_MAX_SIDE / w.max(h).max(1.0);
    (w * scale, h * scale)
}

/// Where a drop at content position (`x`, `y`) inserts: the gap before the
/// returned index, and where to draw the line (x of the line's centre, top of
/// the row). A pointer right of a row's last cell inserts at the end of that
/// row and draws the line there, not at the start of the next row.
pub fn drop_target(x: f32, y: f32, columns: usize, count: usize) -> (usize, f32, f32) {
    if count == 0 {
        return (0, GRID_PADDING, GRID_PADDING);
    }
    let rows = row_count(count, columns);
    let row = (((y - GRID_PADDING + ROW_GAP / 2.0) / ROW_PITCH).floor().max(0.0) as usize)
        .min(rows - 1);
    let cells_in_row = (count - row * columns).min(columns);
    let gap = (((x - GRID_PADDING + COLUMN_GAP / 2.0) / COLUMN_PITCH + 0.5).floor().max(0.0)
        as usize)
        .min(cells_in_row);
    let index = row * columns + gap;
    let line_x = GRID_PADDING + gap as f32 * COLUMN_PITCH - COLUMN_GAP / 2.0;
    let line_y = GRID_PADDING + row as f32 * ROW_PITCH;
    (index, line_x, line_y)
}

/// How far to scroll on one tick of a drag at viewport position `y`: negative
/// near the top, positive near the bottom, faster closer to the edge.
pub fn drag_scroll_step(y: f32, view_height: f32) -> f32 {
    if y < DRAG_SCROLL_EDGE {
        -DRAG_SCROLL_MAX_STEP * ((DRAG_SCROLL_EDGE - y.max(0.0)) / DRAG_SCROLL_EDGE)
    } else if y > view_height - DRAG_SCROLL_EDGE {
        DRAG_SCROLL_MAX_STEP * ((y.min(view_height) - (view_height - DRAG_SCROLL_EDGE)) / DRAG_SCROLL_EDGE)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl Organizer {
    /// Appends the pages of a newly opened source.
    pub fn add_source(&mut self, source: u32, sizes: &[(f32, f32)]) {
        for (page, &(w, h)) in sizes.iter().enumerate() {
            self.pages.push(Page {
                key: self.next_key,
                source,
                page: page as u32,
                rotation: 0,
                width_pt: w,
                height_pt: h,
            });
            self.next_key += 1;
        }
        if self.focus.is_none() {
            self.focus = self.pages.first().map(|p| p.key);
        }
    }

    pub fn index_of(&self, key: u32) -> Option<usize> {
        self.pages.iter().position(|p| p.key == key)
    }

    pub fn focus_index(&self) -> Option<usize> {
        self.focus.and_then(|k| self.index_of(k))
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.pages
            .get(index)
            .is_some_and(|p| self.selected.contains(&p.key))
    }

    /// A press on the cell at `index`. Pressing an already selected page with
    /// no modifier keeps the selection, so that the whole selection can be
    /// dragged; [`Self::click_released`] narrows it if no drag followed.
    pub fn press(&mut self, index: usize, shift: bool, ctrl: bool) {
        let Some(key) = self.pages.get(index).map(|p| p.key) else {
            return;
        };
        if shift {
            let anchor = self.anchor.and_then(|a| self.index_of(a)).unwrap_or(index);
            let (a, b) = (anchor.min(index), anchor.max(index));
            if !ctrl {
                self.selected.clear();
            }
            for p in &self.pages[a..=b] {
                self.selected.insert(p.key);
            }
        } else if ctrl {
            if !self.selected.remove(&key) {
                self.selected.insert(key);
            }
            self.anchor = Some(key);
        } else if !self.selected.contains(&key) {
            self.selected.clear();
            self.selected.insert(key);
            self.anchor = Some(key);
        }
        self.focus = Some(key);
    }

    /// The press at `index` ended without a drag.
    pub fn click_released(&mut self, index: usize, shift: bool, ctrl: bool) {
        if shift || ctrl {
            return;
        }
        if let Some(key) = self.pages.get(index).map(|p| p.key) {
            self.selected.clear();
            self.selected.insert(key);
            self.anchor = Some(key);
        }
    }

    /// The pages an operation works on: the selection, else the focused page.
    pub fn operated(&self) -> Vec<u32> {
        if self.selected.is_empty() {
            self.focus.into_iter().collect()
        } else {
            self.pages
                .iter()
                .filter(|p| self.selected.contains(&p.key))
                .map(|p| p.key)
                .collect()
        }
    }

    fn snapshot(&mut self) {
        self.undo.push(self.pages.clone());
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Moves the pages `keys` as one block, keeping their order, into the gap
    /// before index `insert` (counted in the current order). Returns whether
    /// the order changed.
    pub fn move_pages(&mut self, keys: &[u32], insert: usize) -> bool {
        let set: HashSet<u32> = keys.iter().copied().collect();
        let insert = insert.min(self.pages.len());
        let before = self.pages[..insert]
            .iter()
            .filter(|p| set.contains(&p.key))
            .count();
        let (moving, mut rest): (Vec<Page>, Vec<Page>) =
            self.pages.iter().partition(|p| set.contains(&p.key));
        if moving.is_empty() {
            return false;
        }
        let at = insert - before;
        let tail = rest.split_off(at);
        let new: Vec<Page> = rest.into_iter().chain(moving).chain(tail).collect();
        if new == self.pages {
            return false;
        }
        self.snapshot();
        self.pages = new;
        true
    }

    /// Alt+arrow: moves the operated pages by `step` places (1 for left /
    /// right, the column count for up / down).
    pub fn move_by(&mut self, forward: bool, step: usize) -> bool {
        let keys = self.operated();
        let indices: Vec<usize> = keys.iter().filter_map(|&k| self.index_of(k)).collect();
        let (Some(&first), Some(&last)) = (indices.iter().min(), indices.iter().max()) else {
            return false;
        };
        let insert = if forward {
            (last + 1 + step).min(self.pages.len())
        } else {
            first.saturating_sub(step)
        };
        self.move_pages(&keys, insert)
    }

    pub fn rotate(&mut self, clockwise: bool) {
        let keys: HashSet<u32> = self.operated().into_iter().collect();
        if keys.is_empty() {
            return;
        }
        self.snapshot();
        for p in self.pages.iter_mut().filter(|p| keys.contains(&p.key)) {
            p.rotation = if clockwise {
                (p.rotation + 90) % 360
            } else {
                (p.rotation + 270) % 360
            };
        }
    }

    pub fn delete(&mut self) {
        let keys: HashSet<u32> = self.operated().into_iter().collect();
        if keys.is_empty() {
            return;
        }
        let focus_index = self.focus_index().unwrap_or(0);
        self.snapshot();
        self.pages.retain(|p| !keys.contains(&p.key));
        self.selected.clear();
        let landing = focus_index.min(self.pages.len().saturating_sub(1));
        self.focus = self.pages.get(landing).map(|p| p.key);
        self.anchor = self.focus;
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.pages, prev));
                self.fix_focus();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.pages, next));
                self.fix_focus();
                true
            }
            None => false,
        }
    }

    fn fix_focus(&mut self) {
        let keys: HashSet<u32> = self.pages.iter().map(|p| p.key).collect();
        self.selected.retain(|k| keys.contains(k));
        if self.focus.is_none_or(|k| !keys.contains(&k)) {
            self.focus = self.pages.first().map(|p| p.key);
        }
    }

    pub fn select_all(&mut self) {
        self.selected = self.pages.iter().map(|p| p.key).collect();
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    pub fn toggle_focused(&mut self) {
        if let Some(k) = self.focus {
            if !self.selected.remove(&k) {
                self.selected.insert(k);
            }
            self.anchor = Some(k);
        }
    }

    /// Arrow keys, Home and End move the focus; with Shift they extend the
    /// selection from the anchor, without it they select the focused page.
    pub fn move_focus(&mut self, key: Key, columns: usize, shift: bool) -> Option<usize> {
        let n = self.pages.len();
        if n == 0 {
            return None;
        }
        let from = self.focus_index().unwrap_or(0);
        let to = match key {
            Key::Left => from.saturating_sub(1),
            Key::Right => (from + 1).min(n - 1),
            Key::Up => {
                if from >= columns {
                    from - columns
                } else {
                    from
                }
            }
            Key::Down => {
                if from + columns < n {
                    from + columns
                } else {
                    from
                }
            }
            Key::Home => 0,
            Key::End => n - 1,
        };
        let key = self.pages[to].key;
        if shift {
            let anchor = self.anchor.and_then(|a| self.index_of(a)).unwrap_or(from);
            let (a, b) = (anchor.min(to), anchor.max(to));
            self.selected = self.pages[a..=b].iter().map(|p| p.key).collect();
            if self.anchor.is_none() {
                self.anchor = Some(self.pages[from].key);
            }
        } else {
            self.selected.clear();
            self.selected.insert(key);
            self.anchor = Some(key);
        }
        self.focus = Some(key);
        Some(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn organizer(n: usize) -> Organizer {
        let mut o = Organizer::default();
        o.add_source(0, &vec![(595.0, 842.0); n]);
        o
    }

    fn order(o: &Organizer) -> Vec<u32> {
        o.pages.iter().map(|p| p.page).collect()
    }

    #[test]
    fn columns_follow_width() {
        assert_eq!(column_count(0.0), 1);
        assert_eq!(column_count(2.0 * GRID_PADDING + CELL_WIDTH), 1);
        assert_eq!(column_count(2.0 * GRID_PADDING + 2.0 * CELL_WIDTH + COLUMN_GAP), 2);
        assert_eq!(column_count(2.0 * GRID_PADDING + 2.0 * CELL_WIDTH + COLUMN_GAP - 1.0), 1);
    }

    #[test]
    fn visible_range_is_bounded() {
        let r = visible_range(240, 5, 0.0, 600.0);
        assert_eq!(r.start, 0);
        assert!(r.end < 60, "{r:?}");
        let r = visible_range(240, 5, 3000.0, 600.0);
        assert!(r.start > 50 && r.end - r.start < 60, "{r:?}");
        assert_eq!(visible_range(240, 5, 1e6, 600.0).end, 240);
    }

    #[test]
    fn click_shift_ctrl() {
        let mut o = organizer(10);
        o.press(2, false, false);
        o.click_released(2, false, false);
        o.press(5, true, false);
        assert_eq!(o.operated(), vec![2, 3, 4, 5]);
        o.press(8, false, true);
        assert_eq!(o.operated(), vec![2, 3, 4, 5, 8]);
        o.press(3, false, true);
        assert_eq!(o.operated(), vec![2, 4, 5, 8]);
        // A plain press on a selected page keeps the selection for a drag...
        o.press(4, false, false);
        assert_eq!(o.operated().len(), 4);
        // ...and narrows it when released without one.
        o.click_released(4, false, false);
        assert_eq!(o.operated(), vec![4]);
    }

    #[test]
    fn move_block_keeps_order() {
        let mut o = organizer(8);
        // Move pages 1 and 5 to the gap before index 7.
        assert!(o.move_pages(&[1, 5], 7));
        assert_eq!(order(&o), vec![0, 2, 3, 4, 6, 1, 5, 7]);
        // To the front.
        assert!(o.move_pages(&[4, 7], 0));
        assert_eq!(order(&o), vec![4, 7, 0, 2, 3, 6, 1, 5]);
        // Dropping a block into its own place changes nothing.
        assert!(!o.move_pages(&[4, 7], 1));
        assert!(o.undo());
        assert_eq!(order(&o), vec![0, 2, 3, 4, 6, 1, 5, 7]);
        assert!(o.redo());
        assert_eq!(order(&o), vec![4, 7, 0, 2, 3, 6, 1, 5]);
    }

    #[test]
    fn alt_arrows_move_by_one_or_a_row() {
        let mut o = organizer(10);
        o.press(2, false, false);
        assert!(o.move_by(true, 1));
        assert_eq!(order(&o), vec![0, 1, 3, 2, 4, 5, 6, 7, 8, 9]);
        assert!(o.move_by(true, 4));
        assert_eq!(order(&o), vec![0, 1, 3, 4, 5, 6, 7, 2, 8, 9]);
        assert!(o.move_by(false, 4));
        assert_eq!(order(&o), vec![0, 1, 3, 2, 4, 5, 6, 7, 8, 9]);
        o.move_focus(Key::Home, 4, false);
        assert!(!o.move_by(false, 1));
    }

    #[test]
    fn keyboard_focus_and_shift() {
        let mut o = organizer(10);
        o.move_focus(Key::Right, 4, false);
        o.move_focus(Key::Down, 4, true);
        assert_eq!(o.operated(), vec![1, 2, 3, 4, 5]);
        assert_eq!(o.move_focus(Key::Down, 4, false), Some(9));
        assert_eq!(o.move_focus(Key::Down, 4, false), Some(9));
        assert_eq!(o.move_focus(Key::End, 4, false), Some(9));
    }

    #[test]
    fn delete_rotate_undo() {
        let mut o = organizer(5);
        o.press(1, false, false);
        o.press(2, true, false);
        o.rotate(true);
        assert_eq!(o.pages[1].rotation, 90);
        o.rotate(false);
        o.rotate(false);
        assert_eq!(o.pages[2].rotation, 270);
        o.delete();
        assert_eq!(order(&o), vec![0, 3, 4]);
        assert_eq!(o.focus_index(), Some(2));
        o.undo();
        assert_eq!(order(&o), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn drop_target_gaps() {
        // 5 columns, 12 pages: rows of 5, 5, 2.
        let (cols, n) = (5, 12);
        let (x0, y0) = cell_origin(0, cols);
        // Left half of the first cell inserts before it.
        assert_eq!(drop_target(x0 + 10.0, y0 + 50.0, cols, n).0, 0);
        // Right half inserts after it.
        assert_eq!(drop_target(x0 + CELL_WIDTH - 10.0, y0 + 50.0, cols, n).0, 1);
        // Right of the first row's last cell: end of that row, line on its right.
        let (i, lx, ly) = drop_target(2000.0, y0 + 50.0, cols, n);
        assert_eq!(i, 5);
        assert_eq!(ly, y0);
        assert!(lx > cell_origin(4, cols).0);
        // Past the last, short row: the end.
        assert_eq!(drop_target(2000.0, 10_000.0, cols, n).0, 12);
    }

    #[test]
    fn drag_scroll_speeds_up_at_edges() {
        assert_eq!(drag_scroll_step(300.0, 600.0), 0.0);
        assert!(drag_scroll_step(5.0, 600.0) < drag_scroll_step(30.0, 600.0));
        assert!(drag_scroll_step(598.0, 600.0) > drag_scroll_step(570.0, 600.0));
        assert!(drag_scroll_step(598.0, 600.0) <= DRAG_SCROLL_MAX_STEP);
    }

    #[test]
    fn fitted_size_rotates() {
        let (w, h) = fitted_size(595.0, 842.0, 0);
        assert!(h == PAGE_MAX_SIDE && w < h);
        let (w, h) = fitted_size(595.0, 842.0, 90);
        assert!(w == PAGE_MAX_SIDE && h < w);
    }
}
