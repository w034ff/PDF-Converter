//! A scripted run for the headless cloud machine: drives the window through
//! `Window::dispatch_event` (the same path as real mouse and keyboard input),
//! takes screenshots with `Window::take_snapshot`, and prints timings.
//!
//! `SPIKE_SCREENSHOTS=<dir> SPIKE_PDF=<file> slint-spike`

use std::cell::Cell;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, Model};

use crate::organize::{CELL_HEIGHT, CELL_WIDTH, cell_origin, drop_target};
use crate::{Ctx, thumbs};

struct Sleep {
    done: Rc<Cell<bool>>,
    duration: Duration,
    started: bool,
}

impl Future for Sleep {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.done.get() {
            return Poll::Ready(());
        }
        if !self.started {
            self.started = true;
            let done = self.done.clone();
            let waker = cx.waker().clone();
            slint::Timer::single_shot(self.duration, move || {
                done.set(true);
                waker.wake();
            });
        }
        Poll::Pending
    }
}

fn sleep(ms: u64) -> Sleep {
    Sleep {
        done: Rc::new(Cell::new(false)),
        duration: Duration::from_millis(ms),
        started: false,
    }
}

struct Driver {
    ctx: Rc<Ctx>,
    dir: PathBuf,
}

impl Driver {
    fn app(&self) -> crate::AppWindow {
        self.ctx.app()
    }

    fn shot(&self, name: &str) {
        let app = self.app();
        match app.window().take_snapshot() {
            Ok(buf) => {
                let img = image::RgbaImage::from_raw(buf.width(), buf.height(), buf.as_bytes().to_vec())
                    .expect("the buffer has width * height pixels");
                let path = self.dir.join(format!("{name}.png"));
                img.save(&path).expect("screenshot directory is writable");
                println!("screenshot {}", path.display());
            }
            Err(e) => println!("snapshot failed: {e}"),
        }
    }

    /// Window position of the centre of the cell at `index`.
    fn cell_center(&self, index: usize) -> LogicalPosition {
        let cols = self.ctx.st.borrow().columns.max(1);
        let (x, y) = cell_origin(index, cols);
        self.content_to_window(x + CELL_WIDTH / 2.0, y + CELL_HEIGHT / 2.0 - 10.0)
    }

    fn content_to_window(&self, x: f32, y: f32) -> LogicalPosition {
        let app = self.app();
        LogicalPosition::new(
            app.get_grid_origin_x() + x,
            app.get_grid_origin_y() + y - self.ctx.scroll_y(),
        )
    }

    fn dispatch(&self, e: WindowEvent) {
        self.app().window().dispatch_event(e);
    }

    fn key(&self, k: impl Into<slint::SharedString> + Clone) {
        self.dispatch(WindowEvent::KeyPressed { text: k.clone().into() });
        self.dispatch(WindowEvent::KeyReleased { text: k.into() });
    }

    fn modifier(&self, k: Key, down: bool) {
        let text = k.into();
        self.dispatch(if down {
            WindowEvent::KeyPressed { text }
        } else {
            WindowEvent::KeyReleased { text }
        });
    }

    fn click(&self, pos: LogicalPosition, modifier: Option<Key>) {
        if let Some(m) = modifier {
            self.modifier(m, true);
        }
        self.dispatch(WindowEvent::PointerMoved { position: pos });
        self.dispatch(WindowEvent::PointerPressed {
            position: pos,
            button: PointerEventButton::Left,
        });
        self.dispatch(WindowEvent::PointerReleased {
            position: pos,
            button: PointerEventButton::Left,
        });
        if let Some(m) = modifier {
            self.modifier(m, false);
        }
    }

    async fn drag_to(&self, from: LogicalPosition, to: LogicalPosition, release: bool) {
        self.dispatch(WindowEvent::PointerMoved { position: from });
        self.dispatch(WindowEvent::PointerPressed {
            position: from,
            button: PointerEventButton::Left,
        });
        let steps = 12;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let p = LogicalPosition::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
            self.dispatch(WindowEvent::PointerMoved { position: p });
            sleep(16).await;
        }
        if release {
            self.release(to);
        }
    }

    fn release(&self, at: LogicalPosition) {
        self.dispatch(WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Left,
        });
    }

    /// Waits until every drawn cell has its thumbnail; returns the time.
    async fn wait_thumbs(&self, label: &str) -> Duration {
        let start = Instant::now();
        loop {
            let cells = &self.ctx.cells;
            let drawn: Vec<_> = cells.iter().filter(|c| c.index >= 0).collect();
            if !drawn.is_empty() && drawn.iter().all(|c| c.has_image) {
                break;
            }
            if start.elapsed() > Duration::from_secs(60) {
                println!("{label}: thumbnails did not all arrive");
                break;
            }
            sleep(20).await;
        }
        let t = start.elapsed();
        println!(
            "{label}: {} drawn cells (pool {}) of {} pages filled in {} ms",
            self.ctx.cells.iter().filter(|c| c.index >= 0).count(),
            self.ctx.cells.row_count(),
            self.ctx.st.borrow().org.pages.len(),
            t.as_millis()
        );
        t
    }

    fn order(&self, n: usize) -> Vec<u32> {
        self.ctx.st.borrow().org.pages.iter().take(n).map(|p| p.page + 1).collect()
    }
}

/// Loads the PDF, selects pages 2-4, moves the focus to page 5 with the
/// keyboard and then stays open, for `scripts/a11y-dump.sh` to read the
/// accessibility tree over AT-SPI.
pub fn hold_for_a11y(ctx: Rc<Ctx>, pdf: PathBuf) {
    let d = Driver { ctx, dir: PathBuf::new() };
    let _ = slint::spawn_local(async move {
        d.app().window().set_size(LogicalSize::new(1100.0, 760.0));
        sleep(300).await;
        d.ctx.add_paths(vec![pdf]);
        while d.ctx.st.borrow().org.pages.is_empty() {
            sleep(10).await;
        }
        d.wait_thumbs("a11y").await;
        d.click(d.cell_center(1), None);
        d.click(d.cell_center(3), Some(Key::Shift));
        d.modifier(Key::Control, true);
        d.key(Key::RightArrow);
        d.modifier(Key::Control, false);
        d.key(Key::RightArrow);
        sleep(300).await;
        println!("a11y: ready");
    });
}

pub fn run(ctx: Rc<Ctx>, dir: PathBuf) {
    std::fs::create_dir_all(&dir).expect("screenshot directory can be created");
    let pdf = std::env::var_os("SPIKE_PDF").map(PathBuf::from);
    let d = Driver { ctx, dir };
    let _ = slint::spawn_local(async move {
        let app = d.app();
        app.window().set_size(LogicalSize::new(1100.0, 760.0));
        sleep(600).await;
        d.shot("00-empty-ja");

        let Some(pdf) = pdf else {
            slint::quit_event_loop().ok();
            return;
        };
        let start = Instant::now();
        d.ctx.add_paths(vec![pdf.clone()]);
        while d.ctx.st.borrow().org.pages.is_empty() {
            sleep(10).await;
        }
        println!("open: {} ms", start.elapsed().as_millis());
        d.wait_thumbs("first screen").await;
        sleep(100).await;
        d.shot("01-grid-ja");

        // Click, Shift+click, Ctrl+click.
        d.click(d.cell_center(1), None);
        d.click(d.cell_center(3), Some(Key::Shift));
        d.click(d.cell_center(8), Some(Key::Control));
        sleep(100).await;
        println!("selected after clicks: {:?}", d.ctx.st.borrow().org.operated().len());
        d.shot("02-selection");

        // R rotates the selection.
        d.key("r");
        sleep(100).await;
        d.shot("03-rotated");

        // Drag the 4 selected pages into the gap between the 12th and 13th.
        let cols = d.ctx.st.borrow().columns;
        let (gx, gy) = cell_origin(12, cols);
        let target = d.content_to_window(gx - 8.0, gy + 60.0);
        d.drag_to(d.cell_center(2), target, false).await;
        sleep(100).await;
        let (insert, _, _) = drop_target(gx - 8.0, gy + 60.0, cols, d.ctx.st.borrow().org.pages.len());
        println!("drag over gap {insert}");
        d.shot("04-dragging");
        d.release(target);
        sleep(100).await;
        println!("order after drop: {:?}", d.order(16));
        d.shot("05-dropped");

        // Keyboard: Escape, Home, Right twice, Shift+Right, Alt+Right.
        d.key(Key::Escape);
        d.key(Key::Home);
        d.key(Key::RightArrow);
        d.key(Key::RightArrow);
        d.modifier(Key::Shift, true);
        d.key(Key::RightArrow);
        d.modifier(Key::Shift, false);
        d.modifier(Key::Alt, true);
        d.key(Key::RightArrow);
        d.modifier(Key::Alt, false);
        sleep(100).await;
        println!("order after Alt+Right: {:?}", d.order(8));
        d.shot("06-keyboard");
        d.modifier(Key::Control, true);
        d.key("z");
        d.modifier(Key::Control, false);
        println!("order after Ctrl+Z: {:?}", d.order(8));

        // Drag to the bottom edge and hold: the grid scrolls by itself.
        d.key(Key::Escape);
        d.click(d.cell_center(0), None);
        let view_bottom = app.get_grid_origin_y() + app.get_view_height() - 6.0;
        let to = LogicalPosition::new(app.get_grid_origin_x() + 300.0, view_bottom);
        d.drag_to(d.cell_center(0), to, false).await;
        let before = d.ctx.scroll_y();
        sleep(1500).await;
        println!("auto-scroll moved {} px in 1.5 s", d.ctx.scroll_y() - before);
        d.shot("07-autoscroll");
        d.release(to);
        sleep(50).await;

        // Jump far down, as with the scrollbar: only the cells in view exist.
        let full = app.get_content_height();
        d.ctx.set_scroll_y(full * 0.6);
        d.ctx.refresh();
        d.wait_thumbs("jump to 60%").await;
        d.shot("08-scrolled");
        // A quick sweep through the whole grid: the frames stay cheap since
        // only ~40 cells exist.
        let sweep = Instant::now();
        let mut frames = 0;
        let mut y = 0.0;
        while y < full {
            d.ctx.set_scroll_y(y);
            d.ctx.refresh();
            y += 120.0;
            frames += 1;
        }
        println!(
            "sweep: {frames} scroll steps refreshed in {} ms (model work only)",
            sweep.elapsed().as_millis()
        );

        // Narrow window: fewer columns.
        app.window().set_size(LogicalSize::new(720.0, 700.0));
        d.ctx.set_scroll_y(0.0);
        sleep(300).await;
        d.ctx.refresh();
        d.wait_thumbs("narrow").await;
        println!("columns at 720 px: {}", d.ctx.st.borrow().columns);
        d.shot("09-narrow");

        // English.
        app.window().set_size(LogicalSize::new(1100.0, 760.0));
        let _ = slint::select_bundled_translation("en");
        app.set_language("en".into());
        sleep(300).await;
        d.click(d.cell_center(5), None);
        sleep(100).await;
        d.shot("10-english");

        // Dark.
        app.global::<crate::T>().set_force_dark(true);
        sleep(200).await;
        d.shot("11-dark");

        // The overlay shown while the OS drags a file over the window
        // (winit's HoveredFile; set directly since Xvfb has no drag source).
        app.global::<crate::T>().set_force_dark(false);
        let _ = slint::select_bundled_translation("ja");
        app.set_language("ja".into());
        app.set_file_hover(true);
        sleep(200).await;
        d.shot("12-file-hover");
        app.set_file_hover(false);

        // The file dialog: on this machine there is no XDG desktop portal, so
        // rfd must give up without blocking the event loop.
        let t = Instant::now();
        app.invoke_add_pdfs();
        sleep(1500).await;
        println!(
            "add-pdfs dialog: event loop still running after {} ms",
            t.elapsed().as_millis()
        );

        // Save the current order and read it back.
        let out = d.dir.join("../target/organized.pdf");
        let expected: Vec<u32> = d.order(usize::MAX);
        let (sources, pages) = d.ctx.organize_request();
        let command = d.ctx.command.clone();
        let out2 = out.clone();
        let t = Instant::now();
        let handle = std::thread::spawn(move || thumbs::organize(&command, sources, pages, &out2, &mut |_| {}));
        while !handle.is_finished() {
            sleep(20).await;
        }
        match handle.join().expect("the save thread does not panic") {
            Ok(bytes) => {
                println!("saved {} pages, {bytes} bytes in {} ms", expected.len(), t.elapsed().as_millis());
                match thumbs::open_pdf(&d.ctx.command, &out) {
                    Ok(pages) => println!("reopened: {} pages", pages.len()),
                    Err(e) => println!("reopen failed: {e}"),
                }
            }
            Err(e) => println!("save failed: {e}"),
        }
        slint::quit_event_loop().ok();
    });
}
