//! Slint version of the OCR text selection spike.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

const PAGES: [&str; 4] = ["h1_two_columns", "v2_two_tiers", "v4_dialogue", "m1_mixed"];

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

fn load_image(name: &str) -> slint::Image {
    let img = image::open(fixtures().join(format!("{name}.png"))).unwrap().to_rgba8();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height());
    slint::Image::from_rgba8(buf)
}

fn main() {
    let app = AppWindow::new().unwrap();
    let names: Vec<SharedString> = PAGES.iter().map(|&s| s.into()).collect();
    app.set_page_names(ModelRc::new(VecModel::from(names)));
    let open = {
        let weak = app.as_weak();
        move |i: usize| {
            let app = weak.unwrap();
            app.set_page_image(load_image(PAGES[i]));
        }
    };
    open(0);
    app.on_page_changed(move |i| open(i as usize));
    app.run().unwrap();
}
