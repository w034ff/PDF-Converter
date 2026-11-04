//! Pure editing logic of the crop ranges: what `src/features/crop/cropEdit.ts`
//! and `CropHistory` do in the React app. Coordinates are page points with the
//! origin at the top left (design §4.6).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn from_points(ax: f32, ay: f32, bx: f32, by: f32) -> Self {
        Rect { x: ax.min(bx), y: ay.min(by), w: (ax - bx).abs(), h: (ay - by).abs() }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Manual,
    Photo,
    Figure,
    Chart,
    Table,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Manual => "手で選んだもの",
            Kind::Photo => "写真",
            Kind::Figure => "図",
            Kind::Chart => "グラフ",
            Kind::Table => "表",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub id: u32,
    pub page: u32,
    pub rect: Rect,
    pub kind: Kind,
    pub export: bool,
}

/// Which part of a range the pointer holds. `0..=7` in the UI are the
/// handles, clockwise from the top-left corner; `8` is the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Body,
}

impl Handle {
    pub fn from_index(i: i32) -> Handle {
        match i {
            0 => Handle::TopLeft,
            1 => Handle::Top,
            2 => Handle::TopRight,
            3 => Handle::Right,
            4 => Handle::BottomRight,
            5 => Handle::Bottom,
            6 => Handle::BottomLeft,
            7 => Handle::Left,
            _ => Handle::Body,
        }
    }
}

/// The smallest side of a range, in points, so a handle cannot flip it.
pub const MIN_SIDE_PT: f32 = 4.0;
/// How far the pointer must move, in points, before a press becomes a drag.
pub const DRAG_THRESHOLD_PT: f32 = 3.0;
/// How far an arrow key moves a range; with Shift, ten times as far.
pub const NUDGE_PT: f32 = 1.0;

/// The rectangle after dragging `handle` of `start` by (`dx`, `dy`), kept
/// inside a page of `page_w` × `page_h` points.
pub fn drag_rect(start: Rect, handle: Handle, dx: f32, dy: f32, page_w: f32, page_h: f32) -> Rect {
    if handle == Handle::Body {
        let x = (start.x + dx).clamp(0.0, (page_w - start.w).max(0.0));
        let y = (start.y + dy).clamp(0.0, (page_h - start.h).max(0.0));
        return Rect { x, y, ..start };
    }
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.x + start.w, start.y + start.h);
    let moves_left = matches!(handle, Handle::TopLeft | Handle::Left | Handle::BottomLeft);
    let moves_right = matches!(handle, Handle::TopRight | Handle::Right | Handle::BottomRight);
    let moves_top = matches!(handle, Handle::TopLeft | Handle::Top | Handle::TopRight);
    let moves_bottom = matches!(handle, Handle::BottomLeft | Handle::Bottom | Handle::BottomRight);
    if moves_left {
        l = (l + dx).clamp(0.0, r - MIN_SIDE_PT);
    }
    if moves_right {
        r = (r + dx).clamp(l + MIN_SIDE_PT, page_w);
    }
    if moves_top {
        t = (t + dy).clamp(0.0, b - MIN_SIDE_PT);
    }
    if moves_bottom {
        b = (b + dy).clamp(t + MIN_SIDE_PT, page_h);
    }
    Rect { x: l, y: t, w: r - l, h: b - t }
}

/// The rectangle drawn from (`ax`, `ay`) to (`bx`, `by`), cut to the page.
pub fn drawn_rect(ax: f32, ay: f32, bx: f32, by: f32, page_w: f32, page_h: f32) -> Rect {
    let c = |v: f32, max: f32| v.clamp(0.0, max);
    Rect::from_points(c(ax, page_w), c(ay, page_h), c(bx, page_w), c(by, page_h))
}

/// Undo and redo over snapshots of all ranges, like `CropHistory`.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Vec<Area>>,
    redo: Vec<Vec<Area>>,
}

/// How many steps undo keeps.
pub const HISTORY_LIMIT: usize = 100;

impl History {
    /// Records `before`, the ranges as they were before a change.
    pub fn record(&mut self, before: Vec<Area>) {
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, current: Vec<Area>) -> Option<Vec<Area>> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }
    pub fn redo(&mut self, current: Vec<Area>) -> Option<Vec<Area>> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// What the pointer is doing on the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    Idle,
    /// Pressed on the page or a candidate; becomes `Drawing` once it moves
    /// past [`DRAG_THRESHOLD_PT`]. `candidate` is adopted on release if it
    /// never moved.
    Pressed { x: f32, y: f32, candidate: Option<usize> },
    Drawing { x: f32, y: f32, id: u32 },
    Editing { x: f32, y: f32, id: u32, handle: Handle, start: Rect, moved: bool },
}

/// All ranges of the document and the gesture in progress.
#[derive(Debug)]
pub struct Editor {
    pub areas: Vec<Area>,
    pub selected: Option<u32>,
    pub gesture: Gesture,
    pub history: History,
    next_id: u32,
    /// The ranges before the gesture in progress, recorded once it changes something.
    before_gesture: Option<Vec<Area>>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor { areas: Vec::new(), selected: None, gesture: Gesture::Idle, history: History::default(), next_id: 1, before_gesture: None }
    }
}

impl Editor {
    fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    fn area_mut(&mut self, id: u32) -> Option<&mut Area> {
        self.areas.iter_mut().find(|a| a.id == id)
    }
    pub fn area(&self, id: u32) -> Option<&Area> {
        self.areas.iter().find(|a| a.id == id)
    }
    /// Records the current ranges as one undo step.
    fn checkpoint(&mut self) {
        self.history.record(self.areas.clone());
    }

    pub fn add(&mut self, page: u32, rect: Rect, kind: Kind) -> u32 {
        self.checkpoint();
        let id = self.new_id();
        self.areas.push(Area { id, page, rect, kind, export: true });
        self.selected = Some(id);
        id
    }

    pub fn delete_selected(&mut self) -> bool {
        let Some(id) = self.selected else { return false };
        self.checkpoint();
        self.areas.retain(|a| a.id != id);
        self.selected = None;
        true
    }

    pub fn nudge(&mut self, dx: f32, dy: f32, page_w: f32, page_h: f32) -> bool {
        let Some(id) = self.selected else { return false };
        let Some(start) = self.area(id).map(|a| a.rect) else { return false };
        let moved = drag_rect(start, Handle::Body, dx, dy, page_w, page_h);
        if moved == start {
            return false;
        }
        self.checkpoint();
        if let Some(a) = self.area_mut(id) {
            a.rect = moved;
        }
        true
    }

    pub fn undo(&mut self) -> bool {
        match self.history.undo(self.areas.clone()) {
            Some(prev) => {
                self.areas = prev;
                self.drop_stale_selection();
                true
            }
            None => false,
        }
    }
    pub fn redo(&mut self) -> bool {
        match self.history.redo(self.areas.clone()) {
            Some(next) => {
                self.areas = next;
                self.drop_stale_selection();
                true
            }
            None => false,
        }
    }
    fn drop_stale_selection(&mut self) {
        if self.selected.is_some_and(|id| self.area(id).is_none()) {
            self.selected = None;
        }
    }

    /// The pointer went down on the empty page (`candidate` is `None`) or on a candidate.
    pub fn press_page(&mut self, x: f32, y: f32, candidate: Option<usize>) {
        self.selected = None;
        self.gesture = Gesture::Pressed { x, y, candidate };
    }

    /// The pointer went down on range `id`, on `handle`.
    pub fn press_area(&mut self, id: u32, handle: Handle, x: f32, y: f32) {
        let Some(start) = self.area(id).map(|a| a.rect) else { return };
        self.selected = Some(id);
        self.gesture = Gesture::Editing { x, y, id, handle, start, moved: false };
    }

    pub fn pointer_move(&mut self, page: u32, x: f32, y: f32, page_w: f32, page_h: f32) {
        match self.gesture.clone() {
            Gesture::Pressed { x: ax, y: ay, .. } => {
                if (x - ax).abs().max((y - ay).abs()) < DRAG_THRESHOLD_PT {
                    return;
                }
                self.before_gesture = Some(self.areas.clone());
                let id = self.new_id();
                let rect = drawn_rect(ax, ay, x, y, page_w, page_h);
                self.areas.push(Area { id, page, rect, kind: Kind::Manual, export: true });
                self.selected = Some(id);
                self.gesture = Gesture::Drawing { x: ax, y: ay, id };
            }
            Gesture::Drawing { x: ax, y: ay, id } => {
                let rect = drawn_rect(ax, ay, x, y, page_w, page_h);
                if let Some(a) = self.area_mut(id) {
                    a.rect = rect;
                }
            }
            Gesture::Editing { x: ax, y: ay, id, handle, start, moved } => {
                if !moved && (x - ax).abs().max((y - ay).abs()) < DRAG_THRESHOLD_PT {
                    return;
                }
                if !moved {
                    self.before_gesture = Some(self.areas.clone());
                }
                let rect = drag_rect(start, handle, x - ax, y - ay, page_w, page_h);
                if let Some(a) = self.area_mut(id) {
                    a.rect = rect;
                }
                self.gesture = Gesture::Editing { x: ax, y: ay, id, handle, start, moved: true };
            }
            Gesture::Idle => {}
        }
    }

    /// The pointer went up. Returns the candidate index to adopt, if the
    /// press was a click on a candidate.
    pub fn release(&mut self) -> Option<usize> {
        let gesture = std::mem::replace(&mut self.gesture, Gesture::Idle);
        if let Some(before) = self.before_gesture.take() {
            // A drawn range smaller than a handle is a slip, not a range.
            if let Gesture::Drawing { id, .. } = gesture
                && self.area(id).is_some_and(|a| a.rect.w < MIN_SIDE_PT || a.rect.h < MIN_SIDE_PT)
            {
                self.areas.retain(|a| a.id != id);
                self.selected = None;
                return None;
            }
            self.history.record(before);
        }
        match gesture {
            Gesture::Pressed { candidate, .. } => candidate,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PW: f32 = 595.0;
    const PH: f32 = 842.0;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn body_drag_stays_on_page() {
        assert_eq!(drag_rect(r(10., 10., 100., 50.), Handle::Body, -50., 900., PW, PH), r(0., 792., 100., 50.));
    }

    #[test]
    fn corner_drag_cannot_flip() {
        let out = drag_rect(r(10., 10., 100., 50.), Handle::TopLeft, 500., 500., PW, PH);
        assert_eq!(out, r(110. - MIN_SIDE_PT, 60. - MIN_SIDE_PT, MIN_SIDE_PT, MIN_SIDE_PT));
    }

    #[test]
    fn edge_drag_moves_one_side() {
        assert_eq!(drag_rect(r(10., 10., 100., 50.), Handle::Right, 20., 99., PW, PH), r(10., 10., 120., 50.));
    }

    #[test]
    fn draw_then_undo_redo() {
        let mut e = Editor::default();
        e.press_page(10., 10., None);
        e.pointer_move(1, 60., 80., PW, PH);
        assert_eq!(e.release(), None);
        assert_eq!(e.areas.len(), 1);
        assert_eq!(e.areas[0].rect, r(10., 10., 50., 70.));
        assert!(e.undo());
        assert!(e.areas.is_empty());
        assert!(e.redo());
        assert_eq!(e.areas.len(), 1);
    }

    #[test]
    fn click_on_candidate_adopts_it_and_drag_draws() {
        let mut e = Editor::default();
        e.press_page(10., 10., Some(2));
        e.pointer_move(1, 11., 11., PW, PH);
        assert_eq!(e.release(), Some(2));
        assert!(e.areas.is_empty());
        e.press_page(10., 10., Some(2));
        e.pointer_move(1, 40., 40., PW, PH);
        assert_eq!(e.release(), None);
        assert_eq!(e.areas.len(), 1);
    }

    #[test]
    fn move_is_one_undo_step() {
        let mut e = Editor::default();
        let id = e.add(1, r(10., 10., 50., 50.), Kind::Manual);
        e.press_area(id, Handle::Body, 20., 20.);
        for i in 0..10 {
            e.pointer_move(1, 20. + i as f32 * 5., 20., PW, PH);
        }
        e.release();
        assert_eq!(e.areas[0].rect.x, 55.);
        assert!(e.undo());
        assert_eq!(e.areas[0].rect.x, 10.);
        assert!(e.undo());
        assert!(e.areas.is_empty());
    }

    #[test]
    fn tiny_draw_is_dropped_without_history() {
        let mut e = Editor::default();
        e.press_page(10., 10., None);
        e.pointer_move(1, 13.5, 11., PW, PH);
        e.release();
        assert!(e.areas.is_empty());
        assert!(!e.history.can_undo());
    }

    #[test]
    fn nudge_and_delete() {
        let mut e = Editor::default();
        e.add(1, r(10., 10., 50., 50.), Kind::Manual);
        assert!(e.nudge(10. * NUDGE_PT, 0., PW, PH));
        assert_eq!(e.areas[0].rect.x, 20.);
        assert!(e.delete_selected());
        assert!(e.areas.is_empty());
        assert!(e.undo());
        assert_eq!(e.areas[0].rect.x, 20.);
    }
}
