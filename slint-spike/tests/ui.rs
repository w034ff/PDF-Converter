//! Drives the real window with pointer events on Slint's testing backend.

use slint::ComponentHandle;
use slint_crop_spike::drive::{click, drag};
use slint_crop_spike::edit::{Kind, Rect};
use slint_crop_spike::{AppWindow, app};

const CHART: Rect = Rect { x: 100.0, y: 100.0, w: 200.0, h: 150.0 };

fn setup() -> AppWindow {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    app::install(&app, 150);
    app.window().set_size(slint::LogicalSize::new(1280.0, 820.0));
    app.set_zoom(1.0);
    app::with(|c| c.set_candidates_for_test(vec![(CHART, Kind::Chart)]));
    app
}

fn areas() -> Vec<(Kind, Rect)> {
    app::with(|c| c.editor.areas.iter().map(|a| (a.kind, a.rect)).collect()).unwrap()
}

#[test]
fn click_on_candidate_adopts_it() {
    let app = setup();
    click(&app, (200.0, 200.0));
    assert_eq!(areas(), vec![(Kind::Chart, CHART)]);
}

#[test]
fn drag_on_empty_page_draws_a_range() {
    let app = setup();
    drag(&app, (400.0, 400.0), (500.0, 460.0));
    assert_eq!(areas(), vec![(Kind::Manual, Rect { x: 400.0, y: 400.0, w: 100.0, h: 60.0 })]);
}

/// The case that failed in the React app: a manual range over a candidate
/// must take a drag that starts where the two overlap.
#[test]
fn manual_range_over_candidate_takes_the_drag() {
    let app = setup();
    app::with(|c| c.add_area(Rect { x: 150.0, y: 120.0, w: 100.0, h: 80.0 }, Kind::Manual));
    app::with(|c| c.editor.selected = None);
    drag(&app, (200.0, 150.0), (230.0, 170.0));
    assert_eq!(areas(), vec![(Kind::Manual, Rect { x: 180.0, y: 140.0, w: 100.0, h: 80.0 })]);
}

#[test]
fn handle_resizes_the_selected_range() {
    let app = setup();
    app::with(|c| c.add_area(Rect { x: 400.0, y: 400.0, w: 100.0, h: 80.0 }, Kind::Manual));
    drag(&app, (500.0, 480.0), (540.0, 500.0));
    assert_eq!(areas(), vec![(Kind::Manual, Rect { x: 400.0, y: 400.0, w: 140.0, h: 100.0 })]);
}

#[test]
fn zoom_keeps_page_coordinates() {
    let app = setup();
    app.set_zoom(2.0);
    drag(&app, (110.0, 300.0), (160.0, 330.0));
    assert_eq!(areas(), vec![(Kind::Manual, Rect { x: 110.0, y: 300.0, w: 50.0, h: 30.0 })]);
}
