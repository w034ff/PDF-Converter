//! Writes the test PDF of the spike: A4 pages with Japanese text, an embedded
//! photo, a vector bar chart and a table, at positions that vary per page.
//! Usage: gen-pdf [out.pdf] [pages]

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};

const W: f32 = 595.276;
const H: f32 = 841.89;

fn fill(r: u8, g: u8, b: u8) -> Fill {
    Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity: NormalizedF32::ONE,
        rule: Default::default(),
    }
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> krilla::geom::Path {
    let mut pb = PathBuilder::new();
    pb.push_rect(Rect::from_xywh(x, y, w, h).unwrap());
    pb.finish().unwrap()
}

fn photo(seed: u32) -> Image {
    let (w, h) = (480u32, 320u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let sky = fy < 0.55 + 0.08 * ((fx * 9.0 + seed as f32).sin());
            let (r, g, b) = if sky {
                (90 + (fx * 80.0) as u8, 150 + (fy * 60.0) as u8, 230)
            } else {
                (40 + (fy * 60.0) as u8, 120 + ((fx * 31.0 + fy * 17.0).sin() * 40.0) as u8, 60)
            };
            px.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Image::from_rgba8(px, w, h)
}

const PARAGRAPH: &str = "この文書は Slint の試作のために作った試験用の PDF です。図、グラフ、表、写真が各ページに入っています。";

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "testdata/sample-220.pdf".into());
    let pages: u32 = args.next().map(|s| s.parse().unwrap()).unwrap_or(220);
    let font_path = concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/IBMPlexSansJP-Regular.ttf");
    let font = Font::new(std::fs::read(font_path).unwrap().into(), 0).unwrap();

    let mut doc = Document::new();
    for n in 1..=pages {
        let mut page = doc.start_page_with(PageSettings::from_wh(W, H).unwrap());
        let mut s = page.surface();
        let shift = (n % 5) as f32 * 12.0;
        let black = fill(20, 20, 20);

        s.set_fill(Some(black.clone()));
        s.draw_text(Point::from_xy(56.0, 70.0), font.clone(), 20.0,
            &format!("第 {n} 章　図と表の例"), false, TextDirection::Auto);
        for i in 0..4 {
            s.draw_text(Point::from_xy(56.0, 105.0 + i as f32 * 18.0), font.clone(), 10.5,
                PARAGRAPH, false, TextDirection::Auto);
        }

        // Photo: an embedded image, so pdfium reports it as a photo range.
        s.push_transform(&Transform::from_translate(56.0 + shift, 190.0));
        s.draw_image(photo(n), Size::from_wh(240.0, 160.0).unwrap());
        s.pop();
        s.set_fill(Some(black.clone()));
        s.draw_text(Point::from_xy(56.0 + shift, 366.0), font.clone(), 9.0,
            &format!("写真 {n}-1　風景"), false, TextDirection::Auto);

        // Bar chart: vector paths only.
        let (cx, cy) = (330.0 - shift, 190.0);
        s.set_stroke(Some(Stroke { paint: rgb::Color::new(60, 60, 60).into(), width: 1.0, ..Default::default() }));
        s.set_fill(None);
        s.draw_path(&rect_path(cx, cy, 210.0, 160.0));
        s.set_stroke(None);
        for b in 0..6u32 {
            let v = ((n * 7 + b * 13) % 10 + 2) as f32 * 12.0;
            s.set_fill(Some(fill(70 + (b * 25) as u8, 110, 200 - (b * 20) as u8)));
            s.draw_path(&rect_path(cx + 15.0 + b as f32 * 32.0, cy + 150.0 - v, 22.0, v));
        }
        s.set_fill(Some(black.clone()));
        s.draw_text(Point::from_xy(cx, 366.0), font.clone(), 9.0,
            &format!("図 {n}-2　月ごとの件数"), false, TextDirection::Auto);

        // Table.
        let (tx, ty) = (56.0, 420.0 + shift);
        s.set_stroke(Some(Stroke { paint: rgb::Color::new(40, 40, 40).into(), width: 0.7, ..Default::default() }));
        s.set_fill(None);
        for r in 0..=6 {
            let mut pb = PathBuilder::new();
            pb.move_to(tx, ty + r as f32 * 22.0);
            pb.line_to(tx + 480.0, ty + r as f32 * 22.0);
            s.draw_path(&pb.finish().unwrap());
        }
        for c in 0..=4 {
            let mut pb = PathBuilder::new();
            pb.move_to(tx + c as f32 * 120.0, ty);
            pb.line_to(tx + c as f32 * 120.0, ty + 132.0);
            s.draw_path(&pb.finish().unwrap());
        }
        s.set_stroke(None);
        s.set_fill(Some(black.clone()));
        let heads = ["項目", "四月", "五月", "六月"];
        for r in 0..6 {
            for c in 0..4 {
                let text = if r == 0 { heads[c].to_string() } else if c == 0 { format!("品目 {r}") } else { format!("{}", (n as usize * 31 + r * 7 + c * 3) % 1000) };
                s.draw_text(Point::from_xy(tx + 8.0 + c as f32 * 120.0, ty + 15.0 + r as f32 * 22.0),
                    font.clone(), 10.0, &text, false, TextDirection::Auto);
            }
        }
        s.draw_text(Point::from_xy(tx, ty + 150.0), font.clone(), 9.0,
            &format!("表 {n}-3　四半期の数値"), false, TextDirection::Auto);
        for i in 0..8 {
            s.draw_text(Point::from_xy(56.0, 640.0 + i as f32 * 18.0), font.clone(), 10.5,
                PARAGRAPH, false, TextDirection::Auto);
        }
        s.draw_text(Point::from_xy(W / 2.0 - 10.0, H - 30.0), font.clone(), 9.0,
            &format!("{n}"), false, TextDirection::Auto);
        s.finish();
        page.finish();
    }
    let pdf = doc.finish().unwrap();
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&out, pdf).unwrap();
    eprintln!("wrote {out} ({pages} pages)");
}
