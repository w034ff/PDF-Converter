//! Slint prototype of the organize tab (see REPORT.md).
//!
//! `slint-spike [file.pdf ...]` opens the window; `slint-spike --worker
//! <pdfium dir>` is the worker process the window starts for itself.

mod organize;
mod scenario;
mod thumbs;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use organize::{
    CELL_HEIGHT, CELL_WIDTH, Key, Organizer, ROW_PITCH, cell_origin, column_count, content_height,
    drag_scroll_step, drop_target, fitted_size, visible_range,
};
use pdfconv_worker::protocol::OrganizePage;
use slint::platform::Key as SKey;
use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};
use thumbs::{ThumbService, WorkerCommand};

slint::include_modules!();

/// Thumbnails kept after their cell left the screen (organize §4).
const THUMB_CACHE_PAGES: usize = 1000;
/// A press must move this far before it becomes a drag.
const DRAG_THRESHOLD: f32 = 4.0;
const AUTOSCROLL_TICK: Duration = Duration::from_millis(16);

struct Source {
    path: PathBuf,
    name: String,
    color: i32,
    pages: usize,
}

struct Press {
    index: Option<usize>,
    x: f32,
    y: f32,
}

struct Drag {
    keys: Vec<u32>,
    insert: usize,
    pointer_x: f32,
    /// Pointer position relative to the top of the visible grid, so that
    /// auto-scroll can recompute the content position without a mouse move.
    pointer_view_y: f32,
}

/// A rendered thumbnail and its turned copies, made on demand. The pixels are
/// turned here because the software renderer ignores `transform-rotation`.
struct Thumb {
    base: thumbs::ThumbPixels,
    turned: [Option<Image>; 4],
}

impl Thumb {
    fn image(&mut self, rotation: u16) -> Image {
        let slot = (rotation / 90 % 4) as usize;
        if let Some(img) = &self.turned[slot] {
            return img.clone();
        }
        let base = image::RgbaImage::from_raw(self.base.width(), self.base.height(), self.base.as_bytes().to_vec())
            .expect("the buffer has width * height pixels");
        let turned = match slot {
            1 => image::imageops::rotate90(&base),
            2 => image::imageops::rotate180(&base),
            3 => image::imageops::rotate270(&base),
            _ => base,
        };
        let img = Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
            turned.as_raw(),
            turned.width(),
            turned.height(),
        ));
        self.turned[slot] = Some(img.clone());
        img
    }
}

#[derive(Default)]
struct State {
    org: Organizer,
    sources: Vec<Source>,
    thumbs: HashMap<(u32, u32), Thumb>,
    thumb_lru: VecDeque<(u32, u32)>,
    /// Page index -> slot of the recycled cell pool.
    slots: HashMap<usize, usize>,
    free_slots: Vec<usize>,
    columns: usize,
    press: Option<Press>,
    drag: Option<Drag>,
}

pub struct Ctx {
    app: slint::Weak<AppWindow>,
    st: RefCell<State>,
    cells: Rc<VecModel<CellData>>,
    chips: Rc<VecModel<SourceChip>>,
    thumbs: ThumbService,
    command: WorkerCommand,
    autoscroll: slint::Timer,
}

thread_local! {
    static CTX: RefCell<Option<Rc<Ctx>>> = const { RefCell::new(None) };
}

fn with_ctx(f: impl FnOnce(&Rc<Ctx>)) {
    let ctx = CTX.with(|c| c.borrow().clone());
    if let Some(ctx) = ctx {
        f(&ctx);
    }
}

fn key_str(k: SKey) -> SharedString {
    k.into()
}

impl Ctx {
    fn app(&self) -> AppWindow {
        self.app.upgrade().expect("the window outlives the context")
    }

    fn scroll_y(&self) -> f32 {
        -self.app().get_viewport_y()
    }

    fn set_scroll_y(&self, y: f32) {
        let app = self.app();
        let st = self.st.borrow();
        let max = (content_height(st.org.pages.len(), st.columns.max(1)) - app.get_view_height()).max(0.0);
        app.set_viewport_y(-y.clamp(0.0, max));
    }

    /// Recomputes everything the grid draws. Only cells whose data changed
    /// are written to the model, so Slint redraws only those.
    fn refresh(&self) {
        let app = self.app();
        let mut st = self.st.borrow_mut();
        let st = &mut *st;
        let width = app.get_view_width();
        let height = app.get_view_height();
        let scroll = -app.get_viewport_y();
        let n = st.org.pages.len();
        st.columns = column_count(width);
        let cols = st.columns;
        app.set_content_height(content_height(n, cols));
        let range = visible_range(n, cols, scroll, height);

        // Free the slots of pages that left the range, then give a slot to
        // each page that entered it.
        let leaving: Vec<usize> = st.slots.keys().copied().filter(|i| !range.contains(i)).collect();
        for i in leaving {
            if let Some(s) = st.slots.remove(&i) {
                st.free_slots.push(s);
            }
        }
        let dragging: HashSet<u32> = st
            .drag
            .as_ref()
            .map(|d| d.keys.iter().copied().collect())
            .unwrap_or_default();
        let focus = st.org.focus;
        let mut visible = HashSet::new();
        let mut missing = Vec::new();
        for i in range.clone() {
            let slot = match st.slots.get(&i) {
                Some(&s) => s,
                None => {
                    let s = st.free_slots.pop().unwrap_or_else(|| {
                        self.cells.push(CellData {
                            index: -1,
                            ..Default::default()
                        });
                        self.cells.row_count() - 1
                    });
                    st.slots.insert(i, s);
                    s
                }
            };
            let p = st.org.pages[i];
            let id = (p.source, p.page);
            visible.insert(id);
            let image = st.thumbs.get_mut(&id).map(|t| t.image(p.rotation));
            if image.is_none() {
                missing.push(id);
            }
            let (x, y) = cell_origin(i, cols);
            let (fw, fh) = fitted_size(p.width_pt, p.height_pt, p.rotation);
            let src = &st.sources[p.source as usize];
            let data = CellData {
                index: i as i32,
                x,
                y,
                has_image: image.is_some(),
                image: image.unwrap_or_default(),
                page_number: p.page as i32 + 1,
                source_name: src.name.clone().into(),
                source_color: src.color,
                rotation: p.rotation as i32,
                selected: st.org.selected.contains(&p.key),
                focused: focus == Some(p.key),
                dragging: dragging.contains(&p.key),
                fit_width: fw,
                fit_height: fh,
            };
            if self.cells.row_data(slot).as_ref() != Some(&data) {
                self.cells.set_row_data(slot, data);
            }
        }
        for &s in &st.free_slots {
            if self.cells.row_data(s).is_some_and(|c| c.index >= 0) {
                self.cells.set_row_data(
                    s,
                    CellData {
                        index: -1,
                        ..Default::default()
                    },
                );
            }
        }
        self.thumbs.set_visible(visible, &missing);

        app.set_page_count(n as i32);
        app.set_selected_count(st.org.selected.len() as i32);
        if let Some(d) = &st.drag {
            app.set_dragging(true);
            app.set_drag_count(d.keys.len() as i32);
            let (_, lx, ly) = drop_target(d.pointer_x, d.pointer_view_y + scroll, cols, n);
            app.set_drop_line_visible(true);
            app.set_drop_line_x(lx);
            app.set_drop_line_y(ly);
            app.set_ghost_x(d.pointer_x);
            app.set_ghost_y(d.pointer_view_y + scroll);
        } else {
            app.set_dragging(false);
            app.set_drop_line_visible(false);
        }
    }

    fn refresh_chips(&self) {
        let st = self.st.borrow();
        let chips: Vec<SourceChip> = st
            .sources
            .iter()
            .map(|s| SourceChip {
                name: s.name.clone().into(),
                pages: s.pages as i32,
                color: s.color,
            })
            .collect();
        self.chips.set_vec(chips);
    }

    fn hit_test(&self, x: f32, y: f32) -> Option<usize> {
        let st = self.st.borrow();
        let cols = st.columns.max(1);
        let (x0, y0) = cell_origin(0, cols);
        if x < x0 || y < y0 {
            return None;
        }
        let col = ((x - x0) / organize::COLUMN_PITCH) as usize;
        let row = ((y - y0) / ROW_PITCH) as usize;
        let (cx, cy) = cell_origin(row * cols + col, cols);
        let index = row * cols + col;
        (col < cols && x < cx + CELL_WIDTH && y < cy + CELL_HEIGHT && index < st.org.pages.len())
            .then_some(index)
    }

    fn reveal(&self, index: usize) {
        let app = self.app();
        let cols = self.st.borrow().columns.max(1);
        let (_, y) = cell_origin(index, cols);
        let scroll = self.scroll_y();
        let view = app.get_view_height();
        let margin = organize::GRID_PADDING;
        if y - margin < scroll {
            self.set_scroll_y(y - margin);
        } else if y + CELL_HEIGHT + margin > scroll + view {
            self.set_scroll_y(y + CELL_HEIGHT + margin - view);
        }
    }

    fn pointer_down(self: &Rc<Self>, x: f32, y: f32, shift: bool, ctrl: bool) {
        let index = self.hit_test(x, y);
        {
            let mut st = self.st.borrow_mut();
            match index {
                Some(i) => st.org.press(i, shift, ctrl),
                None if !shift && !ctrl => st.org.clear_selection(),
                None => {}
            }
            st.press = Some(Press { index, x, y });
        }
        self.refresh();
    }

    fn pointer_move(self: &Rc<Self>, x: f32, y: f32) {
        let scroll = self.scroll_y();
        let start = {
            let mut st = self.st.borrow_mut();
            if let Some(d) = st.drag.as_mut() {
                d.pointer_x = x;
                d.pointer_view_y = y - scroll;
                false
            } else if let Some(p) = &st.press {
                let far = (x - p.x).hypot(y - p.y) > DRAG_THRESHOLD;
                let on_selected = p.index.is_some_and(|i| st.org.is_selected(i));
                if far && on_selected {
                    let keys = st.org.operated();
                    st.drag = Some(Drag {
                        keys,
                        insert: 0,
                        pointer_x: x,
                        pointer_view_y: y - scroll,
                    });
                    true
                } else {
                    return;
                }
            } else {
                return;
            }
        };
        if start {
            let weak = Rc::downgrade(self);
            self.autoscroll.start(slint::TimerMode::Repeated, AUTOSCROLL_TICK, move || {
                if let Some(ctx) = weak.upgrade() {
                    ctx.autoscroll_tick();
                }
            });
        }
        self.update_drop();
        self.refresh();
    }

    fn update_drop(&self) {
        let scroll = self.scroll_y();
        let mut st = self.st.borrow_mut();
        let (cols, n) = (st.columns.max(1), st.org.pages.len());
        if let Some(d) = st.drag.as_mut() {
            d.insert = drop_target(d.pointer_x, d.pointer_view_y + scroll, cols, n).0;
        }
    }

    fn autoscroll_tick(&self) {
        let view = self.app().get_view_height();
        let step = match &self.st.borrow().drag {
            Some(d) => drag_scroll_step(d.pointer_view_y, view),
            None => {
                self.autoscroll.stop();
                return;
            }
        };
        if step != 0.0 {
            self.set_scroll_y(self.scroll_y() + step);
            self.update_drop();
            self.refresh();
        }
    }

    fn pointer_up(self: &Rc<Self>, _x: f32, _y: f32, shift: bool, ctrl: bool) {
        self.autoscroll.stop();
        {
            let mut st = self.st.borrow_mut();
            let press = st.press.take();
            if let Some(d) = st.drag.take() {
                st.org.move_pages(&d.keys, d.insert);
            } else if let Some(i) = press.and_then(|p| p.index) {
                st.org.click_released(i, shift, ctrl);
            }
        }
        self.refresh();
    }

    fn grid_key(self: &Rc<Self>, text: &str, shift: bool, ctrl: bool, alt: bool) -> bool {
        let arrow = [
            (key_str(SKey::LeftArrow), Key::Left),
            (key_str(SKey::RightArrow), Key::Right),
            (key_str(SKey::UpArrow), Key::Up),
            (key_str(SKey::DownArrow), Key::Down),
            (key_str(SKey::Home), Key::Home),
            (key_str(SKey::End), Key::End),
        ]
        .into_iter()
        .find(|(s, _)| s.as_str() == text)
        .map(|(_, k)| k);
        let cols = self.st.borrow().columns.max(1);
        let mut reveal = None;
        let handled = {
            let mut st = self.st.borrow_mut();
            let org = &mut st.org;
            match (arrow, alt, ctrl) {
                (Some(k @ (Key::Left | Key::Right | Key::Up | Key::Down)), true, false) => {
                    let forward = matches!(k, Key::Right | Key::Down);
                    let step = if matches!(k, Key::Up | Key::Down) { cols } else { 1 };
                    org.move_by(forward, step);
                    reveal = org.focus_index();
                    true
                }
                (Some(k), false, false) => {
                    reveal = org.move_focus(k, cols, shift);
                    true
                }
                _ => {
                    let lower = text.to_lowercase();
                    if ctrl && !alt {
                        match lower.as_str() {
                            "a" => {
                                org.select_all();
                                true
                            }
                            "z" if shift => org.redo(),
                            "z" => org.undo(),
                            "y" => org.redo(),
                            _ => false,
                        }
                    } else if alt {
                        false
                    } else if text == " " {
                        org.toggle_focused();
                        true
                    } else if text == key_str(SKey::Escape).as_str() {
                        org.clear_selection();
                        true
                    } else if lower == "r" || lower == "l" {
                        org.rotate(lower == "r");
                        true
                    } else if text == key_str(SKey::Delete).as_str()
                        || text == key_str(SKey::Backspace).as_str()
                    {
                        org.delete();
                        reveal = org.focus_index();
                        true
                    } else {
                        false
                    }
                }
            }
        };
        if let Some(i) = reveal {
            self.refresh();
            self.reveal(i);
        }
        self.refresh();
        handled
    }

    fn on_thumb(&self, source: u32, page: u32, pixels: thumbs::ThumbPixels) {
        {
            let mut st = self.st.borrow_mut();
            let id = (source, page);
            st.thumbs.insert(
                id,
                Thumb {
                    base: pixels,
                    turned: Default::default(),
                },
            );
            st.thumb_lru.push_back(id);
            while st.thumb_lru.len() > THUMB_CACHE_PAGES {
                if let Some(old) = st.thumb_lru.pop_front() {
                    st.thumbs.remove(&old);
                }
            }
        }
        self.refresh();
    }

    /// Opens `paths` in the background and appends their pages.
    fn add_paths(self: &Rc<Self>, paths: Vec<PathBuf>) {
        for path in paths {
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
            {
                continue;
            }
            let command = self.command.clone();
            let weak = self.app.clone();
            std::thread::spawn(move || {
                let result = thumbs::open_pdf(&command, &path);
                let _ = weak.upgrade_in_event_loop(move |_| {
                    with_ctx(|ctx| ctx.on_opened(path, result));
                });
            });
        }
    }

    fn on_opened(&self, path: PathBuf, result: Result<Vec<pdfconv_worker::protocol::PageDimensions>, String>) {
        match result {
            Ok(pages) => {
                let id = self.thumbs.add_source(path.clone());
                let mut st = self.st.borrow_mut();
                let sizes: Vec<(f32, f32)> = pages.iter().map(|p| (p.width_pt, p.height_pt)).collect();
                st.sources.push(Source {
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path,
                    color: id as i32 % 6,
                    pages: sizes.len(),
                });
                st.org.add_source(id, &sizes);
            }
            Err(e) => self.app().set_status(format!("{}: {e}", path.display()).into()),
        }
        self.refresh_chips();
        self.refresh();
    }

    fn organize_request(&self) -> (Vec<PathBuf>, Vec<OrganizePage>) {
        let st = self.st.borrow();
        let sources = st.sources.iter().map(|s| s.path.clone()).collect();
        let pages = st
            .org
            .pages
            .iter()
            .map(|p| OrganizePage {
                source: p.source,
                page: p.page,
                rotation: p.rotation,
            })
            .collect();
        (sources, pages)
    }

    /// Writes the current order to `out` on a background thread.
    fn save_to(self: &Rc<Self>, out: PathBuf) {
        let (sources, pages) = self.organize_request();
        let total = pages.len();
        let command = self.command.clone();
        let weak = self.app.clone();
        std::thread::spawn(move || {
            let mut progress = |done: u32| {
                let _ = weak.upgrade_in_event_loop(move |app| {
                    app.set_status(format!("{done} / {total}").into());
                });
            };
            let result = thumbs::organize(&command, sources, pages, &out, &mut progress);
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_status(match result {
                    Ok(bytes) => format!("{} ({bytes} bytes)", out.display()).into(),
                    Err(e) => format!("save failed: {e}").into(),
                });
            });
        });
    }
}

fn pdfium_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PDFIUM_DIR") {
        return dir.into();
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/pdfium");
    if cfg!(windows) {
        root.join("windows/bin")
    } else {
        root.join("linux/lib")
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--worker")) {
        let dir = args.next().map(PathBuf::from).unwrap_or_else(pdfium_dir);
        // Layout detection is not used here; the paths are never opened.
        let none = Path::new("unused");
        if let Err(e) = pdfconv_worker::server::run(&dir, none, none) {
            eprintln!("worker failed: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let initial: Vec<PathBuf> = first.into_iter().chain(args).map(PathBuf::from).collect();

    let app = AppWindow::new()?;
    let _ = slint::select_bundled_translation("ja");

    let command = WorkerCommand {
        program: std::env::current_exe().map_err(|e| slint::PlatformError::Other(e.to_string()))?.into(),
        pdfium_dir: pdfium_dir(),
    };
    let weak = app.as_weak();
    let thumbs = ThumbService::start(command.clone(), move |source, page, result| {
        if let Ok(pixels) = result {
            let _ = weak.upgrade_in_event_loop(move |_| {
                with_ctx(|ctx| ctx.on_thumb(source, page, pixels));
            });
        }
    });
    let cells = Rc::new(VecModel::<CellData>::default());
    let chips = Rc::new(VecModel::<SourceChip>::default());
    app.set_cells(ModelRc::from(cells.clone()));
    app.set_chips(ModelRc::from(chips.clone()));
    let ctx = Rc::new(Ctx {
        app: app.as_weak(),
        st: RefCell::new(State::default()),
        cells,
        chips,
        thumbs,
        command,
        autoscroll: slint::Timer::default(),
    });
    CTX.with(|c| *c.borrow_mut() = Some(ctx.clone()));

    app.on_layout_changed(|| with_ctx(|c| c.refresh()));
    app.on_pointer_down(|x, y, s, c| with_ctx(|ctx| ctx.pointer_down(x, y, s, c)));
    app.on_pointer_move(|x, y| with_ctx(|ctx| ctx.pointer_move(x, y)));
    app.on_pointer_up(|x, y, s, c| with_ctx(|ctx| ctx.pointer_up(x, y, s, c)));
    app.on_grid_key(|text, shift, ctrl, alt| {
        let mut handled = false;
        with_ctx(|ctx| handled = ctx.grid_key(&text, shift, ctrl, alt));
        handled
    });
    app.on_rotate(|cw| {
        with_ctx(|ctx| {
            ctx.st.borrow_mut().org.rotate(cw);
            ctx.refresh();
        })
    });
    app.on_delete_selected(|| {
        with_ctx(|ctx| {
            ctx.st.borrow_mut().org.delete();
            ctx.refresh();
        })
    });
    app.on_language_changed(|lang| {
        let _ = slint::select_bundled_translation(lang.as_str());
    });
    {
        let weak = app.as_weak();
        app.on_add_pdfs(move || {
            let weak = weak.clone();
            let _ = slint::spawn_local(async move {
                let Some(app) = weak.upgrade() else { return };
                let dialog = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_parent(&app.window().window_handle());
                if let Some(files) = dialog.pick_files().await {
                    let paths = files.iter().map(|f| f.path().to_path_buf()).collect();
                    with_ctx(|ctx| ctx.add_paths(paths));
                }
            });
        });
    }
    {
        let weak = app.as_weak();
        app.on_save(move || {
            let weak = weak.clone();
            let _ = slint::spawn_local(async move {
                let Some(app) = weak.upgrade() else { return };
                let dialog = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_file_name("organized.pdf")
                    .set_parent(&app.window().window_handle());
                if let Some(file) = dialog.save_file().await {
                    let path = file.path().to_path_buf();
                    with_ctx(|ctx| ctx.save_to(path));
                }
            });
        });
    }

    // Files dropped from the OS: winit's events, since DropArea does not
    // receive them (see REPORT.md).
    {
        use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
        let weak = app.as_weak();
        app.window().on_winit_window_event(move |_, event| {
            let Some(app) = weak.upgrade() else {
                return EventResult::Propagate;
            };
            match event {
                WindowEvent::HoveredFile(_) => app.set_file_hover(true),
                WindowEvent::HoveredFileCancelled => app.set_file_hover(false),
                WindowEvent::DroppedFile(path) => {
                    app.set_file_hover(false);
                    let path = path.clone();
                    with_ctx(|ctx| ctx.add_paths(vec![path]));
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    ctx.add_paths(initial);
    if let Some(dir) = std::env::var_os("SPIKE_SCREENSHOTS") {
        scenario::run(ctx.clone(), PathBuf::from(dir));
    }
    app.run()
}
