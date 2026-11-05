//! Item 1: the input pages. (a) vertical and (d) mixed pages come from
//! ocr-spike's `gen-sets`; (b) a horizontal page and another mixed page are
//! drawn here with the same typesetter; (c) some are turned by 2°.

use std::path::Path;

use crate::typeset::{BOTCHAN, CHUMON, PAGE_H, PAGE_W, RASHOMON, Typesetter, hline, vblock};

fn blank() -> image::GrayImage {
    image::GrayImage::from_pixel(PAGE_W, PAGE_H, image::Luma([255]))
}

fn wrap(text: &str, per: usize) -> Vec<String> {
    let c: Vec<char> = text.chars().collect();
    c.chunks(per).map(|x| x.iter().collect()).collect()
}

/// (b) Horizontal page: title, author, body set solid in Noto Serif JP, a
/// line with Latin letters and digits, and a page number.
fn horizontal(mincho: &Typesetter, gothic: &Typesetter) -> image::GrayImage {
    let mut img = blank();
    hline(gothic, &mut img, "羅生門", 72.0, 140.0, 140.0);
    hline(gothic, &mut img, "芥川龍之介", 40.0, 140.0, 260.0);
    let mut y = 380.0;
    for l in wrap(RASHOMON, 30) {
        hline(mincho, &mut img, &l, 40.0, 140.0, y);
        y += 64.0;
    }
    y += 40.0;
    hline(gothic, &mut img, "PDF Converter 2.0（2026年11月5日）", 36.0, 140.0, y);
    y += 100.0;
    for l in wrap(BOTCHAN, 30).iter().take(9) {
        hline(mincho, &mut img, l, 40.0, 140.0, y);
        y += 64.0;
    }
    hline(gothic, &mut img, "― 12 ―", 32.0, PAGE_W as f32 / 2.0 - 60.0, PAGE_H as f32 - 120.0);
    img
}

/// (d) Mixed page: a horizontal heading and paragraph above a vertical block.
fn mixed(mincho: &Typesetter, gothic: &Typesetter) -> image::GrayImage {
    let mut img = blank();
    hline(gothic, &mut img, "注文の多い料理店", 64.0, 140.0, 140.0);
    let mut y = 280.0;
    for l in wrap("宮沢賢治の童話。二人の紳士が山奥で「山猫軒」という料理店に入る。", 30) {
        hline(mincho, &mut img, &l, 40.0, 140.0, y);
        y += 64.0;
    }
    vblock(mincho, &mut img, CHUMON, 44.0, 70.0, PAGE_W as f32 - 140.0, 520.0, 36);
    img
}

fn rotate(img: &image::GrayImage, deg: f32) -> image::GrayImage {
    imageproc::geometric_transformations::rotate_about_center(
        img,
        deg.to_radians(),
        imageproc::geometric_transformations::Interpolation::Bilinear,
        image::Luma([255]),
    )
}

pub fn generate(root: &Path, ocr_spike: &Path) {
    let fonts = root.join("data/fonts");
    let serif = std::fs::read(fonts.join("NotoSerifJP-Regular.otf")).unwrap();
    let plex = std::fs::read(fonts.join("IBMPlexSansJP-Regular.otf")).unwrap();
    let mut mincho = Typesetter::new(&serif, &fonts.join("noto_serif_vert.json"));
    mincho.em_align = true;
    mincho.true_em = true;
    let mut gothic = Typesetter::new(&plex, &fonts.join("plex_vert.json"));
    gothic.em_align = true;
    gothic.true_em = true;
    let out = root.join("data/pages");
    std::fs::create_dir_all(&out).unwrap();
    let h1 = horizontal(&mincho, &gothic);
    let m1 = mixed(&mincho, &gothic);
    let t1 = image::open(ocr_spike.join("data/val/t1_merosu_mincho.png")).unwrap().to_luma8();
    let v2 = image::open(ocr_spike.join("data/vertical/v2_two_tiers.png")).unwrap().to_luma8();
    let v4 = image::open(ocr_spike.join("data/vertical/v4_dialogue.png")).unwrap().to_luma8();
    let pages = [
        ("h1_horizontal", h1.clone()),
        ("h1_horizontal_rot2", rotate(&h1, 2.0)),
        ("t1_vertical", t1.clone()),
        ("t1_vertical_rot2", rotate(&t1, 2.0)),
        ("m1_mixed", m1.clone()),
        ("m1_mixed_rot2", rotate(&m1, -2.0)),
        ("v2_two_tiers", v2),
        ("v4_dialogue", v4),
    ];
    for (name, img) in pages {
        img.save(out.join(format!("{name}.png"))).unwrap();
        println!("{name}");
    }
    let ja = ocr_spike.join("data/ref/ja.jpg");
    if ja.exists() {
        std::fs::copy(&ja, out.join("ja_ref.jpg")).unwrap();
        println!("ja_ref");
    }
}
