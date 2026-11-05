//! Wires the window to the model: pointer → caret, bands, keys, clipboard, search.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::model::{Flat, OcrPage, Rect, Sel};
use crate::{AppWindow, Band, panel};

pub const PAGES: [&str; 4] = ["h1_two_columns", "v2_two_tiers", "v4_dialogue", "m1_mixed"];

pub fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

#[derive(Default)]
pub struct State {
    pub page: Option<OcrPage>,
    pub flat: Flat,
    pub sel: Sel,
    pub vblock: Option<usize>,
    pub panel: Flat,
    pub panel_sel: Sel,
    /// The last pointer press was in the panel: Ctrl+A and Ctrl+C act there.
    pub in_panel: bool,
    clipboard: Option<arboard::Clipboard>,
}

fn bands(rects: Vec<Rect>) -> ModelRc<Band> {
    let v: Vec<Band> = rects
        .into_iter()
        .map(|r| Band { x: r.x0 as f32, y: r.y0 as f32, w: (r.x1 - r.x0) as f32, h: (r.y1 - r.y0) as f32 })
        .collect();
    ModelRc::new(VecModel::from(v))
}

fn load_image(name: &str) -> slint::Image {
    let img = image::open(fixtures().join(format!("{name}.png"))).unwrap().to_rgba8();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height());
    slint::Image::from_rgba8(buf)
}

impl State {
    fn open(&mut self, app: &AppWindow, i: usize) {
        let json = std::fs::read_to_string(fixtures().join(format!("{}.json", PAGES[i]))).unwrap();
        let page: OcrPage = serde_json::from_str(&json).unwrap();
        self.flat = Flat::from_page(&page);
        app.set_page_w_pt(page.width_pt as f32);
        app.set_page_h_pt(page.height_pt as f32);
        app.set_page_image(load_image(PAGES[i]));
        app.set_page_index(i as i32);
        self.page = Some(page);
        self.sel = Sel::default();
        self.vblock = None;
        self.panel = Flat::default();
        self.panel_sel = Sel::default();
        app.set_panel_glyphs(ModelRc::default());
        app.set_hit_bands(ModelRc::default());
        self.sync(app);
    }

    fn sync(&mut self, app: &AppWindow) {
        let (a, b) = self.sel.range();
        app.set_sel_bands(bands(self.flat.bands(a, b)));
        let (a, b) = self.panel_sel.range();
        app.set_panel_bands(bands(self.panel.bands(a, b)));
        let (a, b) = self.sel.range();
        app.set_status(format!("{} 字を選択", b - a).into());
    }

    /// Shows the vertical block under the selection's focus in the panel.
    fn follow_focus(&mut self, app: &AppWindow) {
        let n = self.flat.glyphs.len();
        if n == 0 {
            return;
        }
        let block = self.flat.glyphs[self.sel.focus.min(n - 1)].block;
        let vertical = self.page.as_ref().is_some_and(|p| p.blocks[block].orientation == "vertical");
        if vertical && self.vblock != Some(block) {
            let (flat, shown) = panel::build(&self.flat, block);
            self.panel = flat;
            self.panel_sel = Sel::default();
            self.vblock = Some(block);
            app.set_panel_glyphs(ModelRc::new(VecModel::from(shown)));
        }
    }

    fn pointer(&mut self, panel: bool, kind: i32, x: f32, y: f32, shift: bool) {
        let flat = if panel { &self.panel } else { &self.flat };
        let c = flat.caret_at(x as f64, y as f64);
        let sel = if panel { &mut self.panel_sel } else { &mut self.sel };
        match kind {
            0 if shift => sel.focus = c,
            0 => *sel = Sel { anchor: c, focus: c },
            _ => sel.focus = c,
        }
        if kind == 0 {
            self.in_panel = panel;
        }
    }

    pub fn selected_text(&self) -> String {
        let (flat, sel) = if self.in_panel { (&self.panel, self.panel_sel) } else { (&self.flat, self.sel) };
        let (a, b) = sel.range();
        flat.text(a, b)
    }

    fn copy(&mut self) {
        let text = self.selected_text();
        if text.is_empty() {
            return;
        }
        // arboard serves the X11 selection from a thread while the Clipboard lives, so keep it.
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        if let Some(c) = self.clipboard.as_mut() {
            let _ = c.set_text(text);
        }
    }
}

pub fn install(app: &AppWindow) -> Rc<RefCell<State>> {
    let state = Rc::new(RefCell::new(State::default()));
    let names: Vec<SharedString> = PAGES.iter().map(|&s| s.into()).collect();
    app.set_page_names(ModelRc::new(VecModel::from(names)));
    let weak = app.as_weak();
    let st = state.clone();
    app.on_page_changed(move |i| st.borrow_mut().open(&weak.unwrap(), i as usize));
    let (weak, st) = (app.as_weak(), state.clone());
    app.on_page_pointer(move |kind, x, y, shift| {
        let app = weak.unwrap();
        let mut s = st.borrow_mut();
        s.pointer(false, kind, x, y, shift);
        s.follow_focus(&app);
        s.sync(&app);
    });
    let (weak, st) = (app.as_weak(), state.clone());
    app.on_panel_pointer(move |kind, x, y, shift| {
        let mut s = st.borrow_mut();
        s.pointer(true, kind, x, y, shift);
        s.sync(&weak.unwrap());
    });
    let (weak, st) = (app.as_weak(), state.clone());
    app.on_select_all(move || {
        let mut s = st.borrow_mut();
        if s.in_panel {
            s.panel_sel = Sel { anchor: 0, focus: s.panel.glyphs.len() };
        } else {
            s.sel = Sel { anchor: 0, focus: s.flat.glyphs.len() };
        }
        s.sync(&weak.unwrap());
    });
    let st = state.clone();
    app.on_copy(move || st.borrow_mut().copy());
    let (weak, st) = (app.as_weak(), state.clone());
    app.on_search(move |q| {
        let s = st.borrow();
        let rects = s.flat.search(&q).into_iter().flat_map(|(a, b)| s.flat.bands(a, b)).collect();
        weak.unwrap().set_hit_bands(bands(rects));
    });
    state.borrow_mut().open(app, 0);
    let _ = app.get_hit_bands().row_count();
    state
}
