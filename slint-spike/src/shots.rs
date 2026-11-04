//! A scripted run for the screenshots: it waits for the first page, then
//! drives the window with real pointer and key events and saves snapshots.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};
use slint_crop_spike::drive::{click, drag};
use slint_crop_spike::edit::Kind;
use slint_crop_spike::{AppWindow, app};

type Step = Box<dyn FnOnce(&AppWindow, &PathBuf)>;

fn key(app: &AppWindow, text: SharedString) {
    app.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window().dispatch_event(WindowEvent::KeyReleased { text });
}

fn snapshot(app: &AppWindow, dir: &PathBuf, name: &str) {
    let t = Instant::now();
    let Ok(buf) = app.window().take_snapshot() else {
        eprintln!("snapshot {name} failed");
        return;
    };
    let ms = t.elapsed().as_millis();
    let img = image::RgbaImage::from_raw(buf.width(), buf.height(), buf.as_bytes().to_vec()).unwrap();
    img.save(dir.join(name)).unwrap();
    eprintln!("saved {name} ({}×{}, snapshot {ms} ms)", buf.width(), buf.height());
}

fn log_state(label: &str) {
    app::with(|c| {
        let areas: Vec<_> = c.editor.areas.iter().map(|a| (a.id, a.kind, a.rect.x, a.rect.y, a.rect.w, a.rect.h)).collect();
        eprintln!("{label}: selected={:?} areas={areas:?}", c.editor.selected);
    });
}

pub fn start(app: &AppWindow, dir: PathBuf) {
    std::fs::create_dir_all(&dir).unwrap();
    let mut steps: VecDeque<Step> = VecDeque::new();
    steps.push_back(Box::new(|app, dir| {
        app.invoke_fit_width();
        snapshot(app, dir, "01-open.png");
    }));
    // Click a photo candidate to adopt it, then draw a range by hand.
    steps.push_back(Box::new(|app, _| {
        let photo = app::with(|c| c.candidates().iter().find(|c| c.kind == Kind::Photo).map(|c| c.rect)).flatten();
        if let Some(r) = photo {
            click(app, (r.x + r.w / 2.0, r.y + r.h / 2.0));
        }
        drag(app, (48.0, 42.0), (262.0, 84.0));
        log_state("after adopt + draw");
    }));
    // A manual range over the chart candidate, then a drag that starts inside
    // the overlap: the range must move and the candidate must not be adopted.
    steps.push_back(Box::new(|app, _| {
        let chart = app::with(|c| c.candidates().iter().find(|c| c.kind == Kind::Chart || c.kind == Kind::Figure).map(|c| c.rect)).flatten();
        let r = chart.unwrap_or(slint_crop_spike::edit::Rect { x: 320.0, y: 190.0, w: 210.0, h: 160.0 });
        drag(app, (r.x + 40.0, r.y + 30.0), (r.x + 150.0, r.y + 110.0));
        log_state("after drawing over the candidate");
        drag(app, (r.x + 80.0, r.y + 60.0), (r.x + 60.0, r.y + 75.0));
        log_state("after dragging inside the overlap");
        // Resize with the bottom-right handle.
        let sel = app::with(|c| c.editor.selected.and_then(|id| c.editor.area(id)).map(|a| a.rect)).flatten();
        if let Some(s) = sel {
            drag(app, (s.x + s.w, s.y + s.h), (s.x + s.w + 40.0, s.y + s.h + 25.0));
        }
        key(app, SharedString::from(Key::RightArrow));
        log_state("after resize + arrow key");
    }));
    steps.push_back(Box::new(|app, dir| snapshot(app, dir, "02-crop-edit.png")));
    steps.push_back(Box::new(|app, _| {
        app.set_zoom(2.5);
        app.set_view_x(-260.0);
        app.set_view_y(-380.0);
    }));
    steps.push_back(Box::new(|app, dir| snapshot(app, dir, "03-crop-zoom.png")));
    steps.push_back(Box::new(|app, _| {
        app.set_tab(1);
        app.invoke_fit_width();
        app.set_view_y(0.0);
    }));
    // Select across the lines of the single TextInput, then copy and paste.
    steps.push_back(Box::new(|app, _| {
        drag(app, (120.0, 100.0), (300.0, 136.0));
        app.window().dispatch_event(WindowEvent::KeyPressed { text: SharedString::from(Key::Control) });
        app.window().dispatch_event(WindowEvent::KeyPressed { text: "c".into() });
        app.window().dispatch_event(WindowEvent::KeyReleased { text: "c".into() });
        app.window().dispatch_event(WindowEvent::KeyReleased { text: SharedString::from(Key::Control) });
    }));
    steps.push_back(Box::new(|app, dir| snapshot(app, dir, "04-ocr-select.png")));
    steps.push_back(Box::new(|app, _| {
        // Paste into the LineEdit to show what was copied.
        app.invoke_focus_ime_line();
        app.window().dispatch_event(WindowEvent::KeyPressed { text: SharedString::from(Key::Control) });
        app.window().dispatch_event(WindowEvent::KeyPressed { text: "v".into() });
        app.window().dispatch_event(WindowEvent::KeyReleased { text: "v".into() });
        app.window().dispatch_event(WindowEvent::KeyReleased { text: SharedString::from(Key::Control) });
        eprintln!("pasted: {:?}", app.get_ime_line());
    }));
    steps.push_back(Box::new(|app, dir| snapshot(app, dir, "04b-ocr-pasted.png")));
    steps.push_back(Box::new(|app, _| {
        app.set_zoom(2.0);
        app.set_view_x(-120.0);
        app.set_view_y(-120.0);
    }));
    steps.push_back(Box::new(|app, dir| {
        snapshot(app, dir, "05-ocr-zoom.png");
        let _ = slint::quit_event_loop();
    }));

    let steps = Rc::new(RefCell::new(steps));
    let weak = app.as_weak();
    let started = Rc::new(RefCell::new(false));
    app::with(move |c| {
        c.on_page_ready = Some(Box::new(move || {
            if std::mem::replace(&mut *started.borrow_mut(), true) {
                return;
            }
            let timer = Rc::new(slint::Timer::default());
            let (weak, steps, dir, keep) = (weak.clone(), steps.clone(), dir.clone(), timer.clone());
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(700), move || {
                let _ = &keep;
                let Some(app) = weak.upgrade() else { return };
                let next = steps.borrow_mut().pop_front();
                if let Some(step) = next {
                    step(&app, &dir);
                }
            });
        }));
    });
}
