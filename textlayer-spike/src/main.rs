mod check;
mod fonts;
mod krilla_write;
mod layout;
mod pages;
mod typeset;
mod types;

use std::path::{Path, PathBuf};

use layout::{Fit, VMode};
use types::OcrPage;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

pub const PAGES: &[&str] = &[
    "h1_horizontal",
    "ja_ref",
    "t1_vertical",
    "v2_two_tiers",
    "v4_dialogue",
    "m1_mixed",
    "h1_horizontal_rot2",
    "t1_vertical_rot2",
    "m1_mixed_rot2",
];

pub fn page_image(name: &str) -> (Vec<u8>, bool) {
    let dir = root().join("data/pages");
    let png = dir.join(format!("{name}.png"));
    if png.exists() {
        (std::fs::read(png).unwrap(), true)
    } else {
        (std::fs::read(dir.join(format!("{name}.jpg"))).unwrap(), false)
    }
}

pub fn fixture(name: &str) -> OcrPage {
    OcrPage::load(&root().join(format!("fixtures/{name}.json")))
}

fn real_font() -> fonts::RealFont {
    fonts::RealFont::load(&root().join("data/fonts/NotoSansJP-Regular.otf"))
}

fn parse_fit(s: Option<&str>) -> Fit {
    match s {
        Some("char") => Fit::Char,
        Some("tj") => Fit::LineTj,
        _ => Fit::Line,
    }
}

fn parse_vmode(s: Option<&str>, default: VMode) -> VMode {
    match s {
        Some("v") => VMode::IdentityV,
        Some("upright") => VMode::UprightChars,
        Some("rot90") => VMode::Rot90,
        _ => default,
    }
}

/// Writes one PDF per page with krilla; returns the paths.
fn krilla_pages(args: &[String], out: &Path) -> Vec<(String, PathBuf)> {
    let font = match arg(args, "--font") {
        Some("glyphless") => krilla_write::FontKind::Glyphless,
        _ => krilla_write::FontKind::Real,
    };
    let invisible = match arg(args, "--invisible") {
        Some("under") => krilla_write::Invisible::UnderImage,
        _ => krilla_write::Invisible::Transparent,
    };
    let fit = parse_fit(arg(args, "--fit"));
    let vmode = parse_vmode(arg(args, "--vmode"), VMode::UprightChars);
    let real = real_font();
    std::fs::create_dir_all(out).unwrap();
    PAGES
        .iter()
        .map(|name| {
            let ocr = fixture(name);
            let (image, png) = page_image(name);
            let p = krilla_write::PageIn { image: &image, png, size: (ocr.width_pt, ocr.height_pt), runs: layout::runs(&ocr, fit, vmode, 0.0) };
            let bytes = krilla_write::write(&[p], font, invisible, krilla::SerializeSettings::default(), None, &real);
            let path = out.join(format!("{name}.pdf"));
            std::fs::write(&path, bytes).unwrap();
            (name.to_string(), path)
        })
        .collect()
}

fn score_all(files: &[(String, PathBuf)]) -> String {
    let pdfium = check::pdfium();
    let mut s = check::header() + "\n";
    for (name, path) in files {
        let sc = check::score_page(&pdfium, path, 0, &fixture(name), None);
        s += &check::row(name, &sc);
        s += "\n";
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => pages::generate(&root(), &PathBuf::from(&args[2])),
        Some("krilla") => {
            let out = root().join("out").join(arg(&args, "--out").unwrap_or("krilla"));
            let files = krilla_pages(&args, &out);
            println!("{}", score_all(&files));
        }
        Some("dump") => check::dump(&check::pdfium(), Path::new(&args[2]), 0),
        _ => eprintln!("usage: textlayer-spike gen|krilla|lopdf|dump ..."),
    }
}
