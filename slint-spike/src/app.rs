//! The controller between the window, the editing logic and the worker.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use slint::platform::Key;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

use crate::edit::{Area, Editor, Handle, Kind, NUDGE_PT, Rect};
use crate::pdf::{self, Candidate, Pdf, Rgba};
use crate::{AppWindow, AreaItem, CandidateItem, OcrLine, PageItem, VGlyphItem};

/// The resolution the range sizes in the list are given at, as in FR-11's default.
pub const EXPORT_DPI: f32 = 150.0;
/// The longer side of a page thumbnail in the list.
pub const THUMB_SIDE: u32 = 200;

pub fn to_image(img: &Rgba) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&img.pixels, img.width, img.height);
    Image::from_rgba8(buf)
}

/// What the page thread is asked to do.
enum Job {
    Page { page: u32, dpi: u32 },
}

pub struct Controller {
    app: slint::Weak<AppWindow>,
    pub editor: Editor,
    pub page: u32,
    pub page_w: f32,
    pub page_h: f32,
    page_sizes: Vec<(f32, f32)>,
    page_rgba: Option<Rgba>,
    candidates: Vec<Candidate>,
    previews: HashMap<u32, (Rect, Image)>,
    page_areas: Rc<VecModel<AreaItem>>,
    all_areas: Rc<VecModel<AreaItem>>,
    candidate_model: Rc<VecModel<CandidateItem>>,
    pages: Rc<VecModel<PageItem>>,
    jobs: Option<Sender<Job>>,
    pub dpi: u32,
    /// Called once a page and its candidates have arrived (screenshot runs).
    pub on_page_ready: Option<Box<dyn FnMut()>>,
}

thread_local! {
    static CONTROLLER: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

/// Runs `f` on the controller of this (UI) thread.
pub fn with<R>(f: impl FnOnce(&mut Controller) -> R) -> Option<R> {
    CONTROLLER.with(|c| c.borrow_mut().as_mut().map(f))
}

fn post(f: impl FnOnce(&mut Controller) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        with(f);
    });
}

const SAMPLE_TEXT: &str = "この文書は Slint の試作のために作った試験用の PDF です。図、グラフ、表、写真が各ページに入っています。";

/// Wires the callbacks of `app` and installs the controller on this thread.
pub fn install(app: &AppWindow, dpi: u32) {
    let c = Controller {
        app: app.as_weak(),
        editor: Editor::default(),
        page: 1,
        page_w: 595.276,
        page_h: 841.89,
        page_sizes: Vec::new(),
        page_rgba: None,
        candidates: Vec::new(),
        previews: HashMap::new(),
        page_areas: Rc::new(VecModel::default()),
        all_areas: Rc::new(VecModel::default()),
        candidate_model: Rc::new(VecModel::default()),
        pages: Rc::new(VecModel::default()),
        jobs: None,
        dpi,
        on_page_ready: None,
    };
    app.set_page_areas(ModelRc::from(c.page_areas.clone()));
    app.set_all_areas(ModelRc::from(c.all_areas.clone()));
    app.set_candidates(ModelRc::from(c.candidate_model.clone()));
    app.set_pages(ModelRc::from(c.pages.clone()));
    app.set_page_w(c.page_w);
    app.set_page_h(c.page_h);
    install_ocr_sample(app);
    CONTROLLER.with(|slot| *slot.borrow_mut() = Some(c));

    app.on_page_press(|x, y, cand| {
        with(|c| {
            c.editor.press_page(x, y, usize::try_from(cand).ok());
            c.refresh();
        });
    });
    app.on_area_press(|id, handle, x, y| {
        with(|c| {
            c.editor.press_area(id as u32, Handle::from_index(handle), x, y);
            c.refresh();
        });
    });
    app.on_pointer_move(|x, y| {
        with(|c| {
            let before = c.editor.areas.len();
            c.editor.pointer_move(c.page, x, y, c.page_w, c.page_h);
            if before != c.editor.areas.len() || c.editor.gesture != crate::edit::Gesture::Idle {
                c.refresh();
            }
        });
    });
    app.on_pointer_release(|| {
        with(|c| {
            if let Some(i) = c.editor.release()
                && let Some(cand) = c.candidates.get(i)
            {
                let (rect, kind) = (cand.rect, cand.kind);
                c.editor.add(c.page, rect, kind);
            }
            c.refresh();
        });
    });
    app.on_key(|text, shift, ctrl| with(|c| c.key(&text, shift, ctrl)).unwrap_or(false));
    app.on_undo(|| {
        with(|c| {
            c.editor.undo();
            c.refresh();
        });
    });
    app.on_redo(|| {
        with(|c| {
            c.editor.redo();
            c.refresh();
        });
    });
    app.on_delete_selected(|| {
        with(|c| {
            c.editor.delete_selected();
            c.refresh();
        });
    });
    app.on_select_area(|id| {
        with(|c| {
            let Some(page) = c.editor.area(id as u32).map(|a| a.page) else { return };
            c.editor.selected = Some(id as u32);
            if page != c.page {
                c.show_page(page);
            }
            c.refresh();
        });
    });
    app.on_toggle_export(|id, on| {
        with(|c| {
            if let Some(a) = c.editor.areas.iter_mut().find(|a| a.id == id as u32) {
                a.export = on;
            }
            c.refresh();
        });
    });
    app.on_select_page(|page| {
        with(|c| c.show_page(page as u32));
    });
    app.on_open_file(|| {
        // rfd's portal dialog blocks; run it off the UI thread.
        thread::spawn(|| {
            let picked = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file();
            if let Some(path) = picked {
                post(move |c| c.open(&path));
            }
        });
    });
}

fn install_ocr_sample(app: &AppWindow) {
    // The positions of the text that gen_pdf writes: four paragraph lines from
    // baseline 105 pt, 18 pt apart, 10.5 pt; the table cells below.
    app.set_ocr_block(SharedString::from([SAMPLE_TEXT; 4].join("\n")));
    app.set_ocr_block_x(56.0);
    app.set_ocr_block_y(105.0);
    app.set_ocr_block_w(500.0);
    app.set_ocr_block_h(76.0);
    let mut lines = Vec::new();
    for (i, t) in ["項目", "四月", "五月", "六月"].iter().enumerate() {
        // Page 1 of the test PDF: the table starts 12 pt lower than on page 5n.
        lines.push(OcrLine { x: 64.0 + i as f32 * 120.0, y: 447.0, w: 100.0, h: 18.0, text: (*t).into(), font_size: 10.0 });
    }
    app.set_ocr_lines(ModelRc::new(VecModel::from(lines)));
    let to_items = |text: &str, rows: usize| -> ModelRc<VGlyphItem> {
        let v: Vec<VGlyphItem> = crate::vtext::layout(text, rows)
            .into_iter()
            .map(|g| VGlyphItem { ch: g.ch.into(), col: g.col as i32, row: g.row as i32, rotate: g.rotate, shift: g.shift })
            .collect();
        ModelRc::new(VecModel::from(v))
    };
    app.set_vglyphs(to_items("縦書きの例です。「かぎ括弧」や（丸括弧）、長音のデータ、小さいっゃゅょ、英字 PDF も並べる。", 10));
    app.set_page_vglyphs(to_items("写真の説明を縦に書く。ページの上に重ねた例", 25));
}

impl Controller {
    fn app(&self) -> Option<AppWindow> {
        self.app.upgrade()
    }

    /// Opens `path` and starts the page and thumbnail threads.
    pub fn open(&mut self, path: &Path) {
        let Some(app) = self.app() else { return };
        app.set_file_name(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().into());
        app.set_status("開いています…".into());
        let path = path.to_path_buf();
        let (tx, rx) = mpsc::channel::<Job>();
        self.jobs = Some(tx);
        self.editor = Editor::default();
        self.previews.clear();
        let dpi = self.dpi;
        let page_path = path.clone();
        thread::spawn(move || {
            let mut pdf = match Pdf::open(&page_path) {
                Ok(p) => p,
                Err(e) => {
                    post(move |c| c.status(&format!("開けません: {e}")));
                    return;
                }
            };
            let sizes: Vec<(f32, f32)> = pdf.pages.iter().map(|p| (p.width_pt, p.height_pt)).collect();
            post(move |c| c.set_pages(sizes));
            spawn_thumbnails(page_path.clone());
            let _ = dpi;
            while let Ok(Job::Page { page, dpi }) = rx.recv() {
                match pdf.render(page, dpi) {
                    Ok((img, render_ms, decode_ms)) => {
                        let (w, h) = (img.width, img.height);
                        post(move |c| {
                            c.set_page_image(page, img);
                            c.status(&format!("p.{page}: {w}×{h} px（{dpi} dpi）、描画 {render_ms} ms、PNG の展開 {decode_ms} ms"));
                        });
                    }
                    Err(e) => post(move |c| c.status(&format!("描けません: {e}"))),
                }
                match pdf.candidates(page) {
                    Ok(cands) => post(move |c| c.set_candidates(page, cands)),
                    Err(e) => post(move |c| c.status(&format!("候補を出せません: {e}"))),
                }
            }
        });
    }

    fn status(&self, s: &str) {
        eprintln!("status: {s}");
        if let Some(app) = self.app() {
            app.set_status(s.into());
        }
    }

    fn set_pages(&mut self, sizes: Vec<(f32, f32)>) {
        let items: Vec<PageItem> = (1..=sizes.len() as i32).map(|page| PageItem { page, thumb: Image::default(), loaded: false }).collect();
        self.pages.set_vec(items);
        self.page_sizes = sizes;
        self.show_page(1);
    }

    pub fn show_page(&mut self, page: u32) {
        self.page = page;
        if let Some(&(w, h)) = self.page_sizes.get(page as usize - 1) {
            self.page_w = w;
            self.page_h = h;
        }
        self.page_rgba = None;
        self.candidates.clear();
        if let Some(app) = self.app() {
            app.set_current_page(page as i32);
            app.set_page_w(self.page_w);
            app.set_page_h(self.page_h);
            app.set_page_image(Image::default());
        }
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Job::Page { page, dpi: self.dpi });
        }
        self.refresh();
    }

    pub fn set_page_image(&mut self, page: u32, img: Rgba) {
        if page != self.page {
            return;
        }
        if let Some(app) = self.app() {
            let t = std::time::Instant::now();
            app.set_page_image(to_image(&img));
            eprintln!("page image {}×{} handed to Slint in {} ms", img.width, img.height, t.elapsed().as_millis());
        }
        self.page_rgba = Some(img);
        self.refresh();
    }

    pub fn set_candidates(&mut self, page: u32, cands: Vec<Candidate>) {
        if page != self.page {
            return;
        }
        self.candidates = cands;
        self.candidate_model.set_vec(Vec::new());
        self.refresh();
        if let Some(f) = self.on_page_ready.as_mut() {
            f();
        }
    }

    fn set_thumbnail(&mut self, page: u32, img: Rgba) {
        let i = page as usize - 1;
        if i < self.pages.row_count() {
            self.pages.set_row_data(i, PageItem { page: page as i32, thumb: to_image(&img), loaded: true });
        }
    }

    fn key(&mut self, text: &str, shift: bool, ctrl: bool) -> bool {
        let is = |k: Key| text == SharedString::from(k).as_str();
        let step = if shift { NUDGE_PT * 10.0 } else { NUDGE_PT };
        let (pw, ph) = (self.page_w, self.page_h);
        let handled = if is(Key::LeftArrow) {
            self.editor.nudge(-step, 0.0, pw, ph)
        } else if is(Key::RightArrow) {
            self.editor.nudge(step, 0.0, pw, ph)
        } else if is(Key::UpArrow) {
            self.editor.nudge(0.0, -step, pw, ph)
        } else if is(Key::DownArrow) {
            self.editor.nudge(0.0, step, pw, ph)
        } else if is(Key::Delete) || is(Key::Backspace) {
            self.editor.delete_selected()
        } else if is(Key::Escape) {
            self.editor.selected.take().is_some()
        } else if ctrl && (text == "z" || text == "Z") && !shift {
            self.editor.undo()
        } else if ctrl && (text == "y" || text == "Y" || ((text == "z" || text == "Z") && shift)) {
            self.editor.redo()
        } else {
            return false;
        };
        if handled {
            self.refresh();
        }
        true
    }

    fn item(&mut self, a: &Area) -> AreaItem {
        let preview = match (&self.page_rgba, a.page == self.page) {
            (Some(rgba), true) => {
                let cached = self.previews.get(&a.id).filter(|(r, _)| *r == a.rect).map(|(_, i)| i.clone());
                match cached {
                    Some(i) => i,
                    None => {
                        let img = pdf::crop(rgba, self.page_w, a.rect).map(|c| to_image(&c)).unwrap_or_default();
                        self.previews.insert(a.id, (a.rect, img.clone()));
                        img
                    }
                }
            }
            _ => self.previews.get(&a.id).map(|(_, i)| i.clone()).unwrap_or_default(),
        };
        let px = |pt: f32| (pt * EXPORT_DPI / 72.0).round() as i32;
        AreaItem {
            id: a.id as i32,
            page: a.page as i32,
            x: a.rect.x,
            y: a.rect.y,
            w: a.rect.w,
            h: a.rect.h,
            kind: a.kind.label().into(),
            selected: self.editor.selected == Some(a.id),
            export: a.export,
            preview,
            caption: format!("p.{} {}\n{} × {} px", a.page, a.kind.label(), px(a.rect.w), px(a.rect.h)).into(),
        }
    }

    /// Pushes the ranges to the window. Rows are updated in place when the
    /// count is unchanged: replacing the model would recreate the repeated
    /// elements, and the `TouchArea` holding the pointer grab with them.
    pub fn refresh(&mut self) {
        let mut on_page: Vec<Area> = self.editor.areas.iter().filter(|a| a.page == self.page).cloned().collect();
        on_page.sort_by_key(|a| a.id);
        let page_items: Vec<AreaItem> = on_page.iter().map(|a| self.item(a)).collect();
        sync(&self.page_areas, page_items);
        let mut all = self.editor.areas.clone();
        all.sort_by(|a, b| a.page.cmp(&b.page).then(a.rect.y.total_cmp(&b.rect.y)).then(a.rect.x.total_cmp(&b.rect.x)));
        let all_items: Vec<AreaItem> = all.iter().map(|a| self.item(a)).collect();
        sync(&self.all_areas, all_items);
        let candidate_items: Vec<CandidateItem> = self
            .candidates
            .iter()
            .map(|c| CandidateItem {
                x: c.rect.x,
                y: c.rect.y,
                w: c.rect.w,
                h: c.rect.h,
                kind: c.kind.label().into(),
                adopted: on_page.iter().any(|a| a.rect == c.rect),
            })
            .collect();
        sync(&self.candidate_model, candidate_items);
        if let Some(app) = self.app() {
            app.set_can_undo(self.editor.history.can_undo());
            app.set_can_redo(self.editor.history.can_redo());
            app.set_has_selection(self.editor.selected.is_some());
        }
    }

    /// Adds `kind` ranges directly (tests and screenshot runs).
    pub fn add_area(&mut self, rect: Rect, kind: Kind) -> u32 {
        let id = self.editor.add(self.page, rect, kind);
        self.refresh();
        id
    }

    pub fn set_candidates_for_test(&mut self, cands: Vec<(Rect, Kind)>) {
        let page = self.page;
        self.set_candidates(page, cands.into_iter().map(|(rect, kind)| Candidate { rect, kind, score: 1.0 }).collect());
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
}

fn sync<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    if model.row_count() == rows.len() {
        for (i, row) in rows.into_iter().enumerate() {
            if model.row_data(i).as_ref() != Some(&row) {
                model.set_row_data(i, row);
            }
        }
    } else {
        model.set_vec(rows);
    }
}

fn spawn_thumbnails(path: PathBuf) {
    thread::spawn(move || {
        let Ok(mut pdf) = Pdf::open(&path) else { return };
        for page in 1..=pdf.pages.len() as u32 {
            match pdf.thumbnail(page, THUMB_SIDE) {
                Ok(img) => post(move |c| c.set_thumbnail(page, img)),
                Err(_) => return,
            }
        }
    });
}
