//! Replays fixtures/scenarios/*.json (written by web/tests/gen-scenarios.mjs)
//! with real window events, compares the clipboard and saves snapshots.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::Value;
use slint::platform::Key;
use slint::{ComponentHandle, LogicalPosition};

use crate::app::{PAGES, State, fixtures};
use crate::{AppWindow, drive, panel};

const SENTINEL: &str = "<empty>";

fn pt(v: &Value) -> (f32, f32) {
    (v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32)
}

/// Window position of the panel glyph `ch` of line `line` (counted inside the block).
fn panel_point(app: &AppWindow, state: &Rc<RefCell<State>>, line: usize, ch: usize, dy: f32) -> LogicalPosition {
    let s = state.borrow();
    let first = s.panel.glyphs.first().map_or(0, |g| g.line);
    let g = s.panel.glyphs.iter().filter(|g| g.line == first + line).nth(ch).expect("panel glyph");
    let x = (g.bbox.x0 + g.bbox.x1) / 2.0;
    let y = g.bbox.y0 + (g.bbox.y1 - g.bbox.y0) * 0.3;
    LogicalPosition::new(app.get_panel_abs_x() + x as f32, app.get_panel_abs_y() + y as f32 + dy)
}

fn run(app: &AppWindow, state: &Rc<RefCell<State>>, out: &PathBuf) -> (usize, usize) {
    let mut clip = arboard::Clipboard::new().unwrap();
    let (mut pass, mut fail) = (0, 0);
    let mut report = Vec::new();
    for (i, name) in PAGES.iter().enumerate() {
        let json = std::fs::read_to_string(fixtures().join(format!("scenarios/{name}.json"))).unwrap();
        let scenarios: Vec<Value> = serde_json::from_str(&json).unwrap();
        // The Rust model must give the same whole-page text as the TypeScript one that wrote the scenarios.
        app.invoke_page_changed(i as i32);
        let rust_text = state.borrow().page.as_ref().unwrap().text();
        let ts_text = scenarios.iter().find(|s| s["name"] == "all").and_then(|s| s["copy"].as_str()).unwrap();
        assert_eq!(rust_text, ts_text, "{name}: Rust and TypeScript disagree on the page text");
        for sc in &scenarios {
            app.invoke_page_changed(i as i32);
            app.invoke_search("".into());
            clip.set_text(SENTINEL).unwrap();
            for st in sc["steps"].as_array().unwrap() {
                match st["op"].as_str().unwrap() {
                    "drag" => {
                        let pts: Vec<_> = st["pts"].as_array().unwrap().iter().map(pt).collect();
                        drive::press(app, drive::at(app, pts[0].0, pts[0].1));
                        for p in &pts[1..] {
                            drive::move_to(app, drive::at(app, p.0, p.1));
                        }
                        let l = pts[pts.len() - 1];
                        drive::release(app, drive::at(app, l.0, l.1));
                    }
                    "click" => {
                        let shift = st["shift"].as_bool().unwrap_or(false);
                        if shift {
                            drive::key_down(app, Key::Shift.into());
                        }
                        let p = pt(&st["pt"]);
                        drive::press(app, drive::at(app, p.0, p.1));
                        drive::release(app, drive::at(app, p.0, p.1));
                        if shift {
                            drive::key_up(app, Key::Shift.into());
                        }
                    }
                    "key" => {
                        let k = st["key"].as_str().unwrap();
                        drive::ctrl(app, k.chars().last().unwrap());
                    }
                    "search" => app.invoke_search(st["q"].as_str().unwrap().into()),
                    "panel-click" => {
                        let p = panel_point(app, state, st["line"].as_u64().unwrap() as usize, st["char"].as_u64().unwrap() as usize, 0.0);
                        drive::press(app, p);
                        drive::release(app, p);
                    }
                    "panel-drag" => {
                        let f = &st["from"];
                        let t = &st["to"];
                        let a = panel_point(app, state, f[0].as_u64().unwrap() as usize, f[1].as_u64().unwrap() as usize, -4.0);
                        let b = panel_point(app, state, t[0].as_u64().unwrap() as usize, t[1].as_u64().unwrap() as usize, 10.0);
                        drive::press(app, a);
                        for k in 1..=8 {
                            let f = k as f32 / 8.0;
                            drive::move_to(app, LogicalPosition::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f));
                        }
                        drive::release(app, b);
                    }
                    op => panic!("unknown op {op}"),
                }
            }
            let (ok, got) = if let Some(rects) = sc["rects"].as_array() {
                let got: Vec<_> = slint::Model::iter(&app.get_hit_bands()).map(|b| [b.x, b.y, b.w, b.h]).collect();
                let want: Vec<[f32; 4]> = rects
                    .iter()
                    .map(|r| {
                        let f = |k: &str| r[k].as_f64().unwrap() as f32;
                        [f("x0"), f("y0"), f("x1") - f("x0"), f("y1") - f("y0")]
                    })
                    .collect();
                let ok = got.len() == want.len() && got.iter().zip(&want).all(|(g, w)| g.iter().zip(w).all(|(a, b)| (a - b).abs() < 1.0));
                (ok, format!("{got:?}"))
            } else {
                let mut got = clip.get_text().unwrap_or_default();
                let want = sc["copy"].as_str().unwrap();
                if got == SENTINEL && want.is_empty() {
                    got.clear();
                }
                (got == want, got)
            };
            let sname = sc["name"].as_str().unwrap();
            println!("{} {name} {sname}", if ok { "PASS" } else { "FAIL" });
            if !ok {
                println!("   want {:?}\n   got  {:?}", sc["copy"].as_str().unwrap_or(""), got.chars().take(120).collect::<String>());
            }
            if ok { pass += 1 } else { fail += 1 }
            report.push(serde_json::json!({ "page": name, "name": sname, "ok": ok }));
            let shot = app.window().take_snapshot().unwrap();
            let img = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_bytes().to_vec()).unwrap();
            img.save(out.join(format!("{name}-{sname}.png"))).unwrap();
        }
    }
    std::fs::write(out.join("result.json"), serde_json::to_string_pretty(&report).unwrap()).unwrap();
    (pass, fail)
}

pub fn start(app: &AppWindow, state: Rc<RefCell<State>>, out: PathBuf, zoom: f32) {
    std::fs::create_dir_all(&out).unwrap();
    app.set_zoom(zoom);
    let (w, h) = if zoom > 1.0 { (2300.0, 2500.0) } else { (1300.0, 1250.0) };
    app.window().set_size(slint::LogicalSize::new(w, h));
    let weak = app.as_weak();
    let _ = panel::WIDTH;
    slint::Timer::single_shot(std::time::Duration::from_millis(500), move || {
        let app = weak.unwrap();
        let (pass, fail) = run(&app, &state, &out);
        println!("slint zoom {zoom}: {pass} pass, {fail} fail");
        slint::quit_event_loop().unwrap();
    });
}
