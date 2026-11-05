mod check;
mod fonts;
mod krilla_write;
mod layout;
mod lopdf_write;
mod overlay;
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

/// Image-only PDF (krilla) for one page, then the text layer added with lopdf.
fn lopdf_pages(args: &[String], out: &Path) -> Vec<(String, PathBuf)> {
    let kind = match arg(args, "--font") {
        Some("glyphless") => lopdf_write::LFont::Glyphless,
        Some("glyphless-empty") => lopdf_write::LFont::GlyphlessEmpty,
        _ => lopdf_write::LFont::Real,
    };
    let fit = parse_fit(arg(args, "--fit"));
    let pos = if fit == Fit::LineTj { lopdf_write::Positioning::Tj } else { lopdf_write::Positioning::Plain };
    let vmode = parse_vmode(arg(args, "--vmode"), VMode::IdentityV);
    let real = real_font();
    std::fs::create_dir_all(out).unwrap();
    PAGES
        .iter()
        .map(|name| {
            let ocr = fixture(name);
            let (image, png) = page_image(name);
            let p = krilla_write::PageIn { image: &image, png, size: (ocr.width_pt, ocr.height_pt), runs: vec![] };
            let base = krilla_write::image_only(&[p], krilla::SerializeSettings::default(), None);
            let runs = layout::runs(&ocr, fit, vmode, 0.0);
            let doc = lopdf_write::add_text_layer(&base, &[runs], kind, pos, &real);
            let path = out.join(format!("{name}.pdf"));
            std::fs::write(&path, lopdf_write::save(doc)).unwrap();
            (name.to_string(), path)
        })
        .collect()
}

/// Item 6: size and write time for `n` pages cycling through PAGES.
fn bench(n: usize) {
    let real = real_font();
    let names: Vec<&str> = PAGES.iter().cycle().take(n).copied().collect();
    let data: Vec<(OcrPage, Vec<u8>, bool)> = names.iter().map(|nm| { let (i, p) = page_image(nm); (fixture(nm), i, p) }).collect();
    let pages = |fit: Fit, vmode: VMode| -> Vec<krilla_write::PageIn<'_>> {
        data.iter().map(|(o, i, p)| krilla_write::PageIn { image: i, png: *p, size: (o.width_pt, o.height_pt), runs: layout::runs(o, fit, vmode, 0.0) }).collect()
    };
    let chars: usize = data.iter().map(|d| d.0.chars().count()).sum();
    let t = std::time::Instant::now();
    let base = krilla_write::image_only(&pages(Fit::Char, VMode::IdentityV), krilla::SerializeSettings::default(), None);
    let base_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("| {n} | {chars} | 画像だけ（krilla） | {} | — | {base_ms:.0} ms |", base.len());
    for (label, font) in [("krilla 本物 char", krilla_write::FontKind::Real), ("krilla 字形なし char", krilla_write::FontKind::Glyphless)] {
        let p = pages(Fit::Char, VMode::UprightChars);
        let t = std::time::Instant::now();
        let b = krilla_write::write(&p, font, krilla_write::Invisible::Transparent, krilla::SerializeSettings::default(), None, &real);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("| {n} | {chars} | {label} | {} | +{} | {ms:.0} ms（画像込み） |", b.len(), b.len() as i64 - base.len() as i64);
    }
    for (label, font, fit) in [
        ("lopdf 本物 char", lopdf_write::LFont::Real, Fit::Char),
        ("lopdf 字形なし char", lopdf_write::LFont::Glyphless, Fit::Char),
        ("lopdf 本物 line", lopdf_write::LFont::Real, Fit::Line),
        ("lopdf 字形なし line", lopdf_write::LFont::Glyphless, Fit::Line),
    ] {
        let runs: Vec<Vec<layout::Run>> = data.iter().map(|d| layout::runs(&d.0, fit, VMode::IdentityV, 0.0)).collect();
        let t = std::time::Instant::now();
        let b = lopdf_write::save(lopdf_write::add_text_layer(&base, &runs, font, lopdf_write::Positioning::Plain, &real));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("| {n} | {chars} | {label} | {} | +{} | {ms:.0} ms（読み込みと保存込み） |", b.len(), b.len() as i64 - base.len() as i64);
    }
}

fn pdfa_settings() -> (krilla::SerializeSettings, krilla::metadata::Metadata) {
    use krilla::configure::{Archival, ConfigurationBuilder};
    let configuration = ConfigurationBuilder::new().with_archival_validator(Archival::A2_B).finish().unwrap();
    // As crates/core/src/pdf_write.rs: PDF/A needs a creation date (fixed here).
    let date = krilla::metadata::DateTime::new(2026).month(11).day(5).hour(0).minute(0).second(0).utc_offset_hour(0).utc_offset_minute(0);
    let meta = krilla::metadata::Metadata::new().creator("textlayer-spike".into()).creation_date(date);
    (krilla::SerializeSettings { configuration, ..krilla::SerializeSettings::default() }, meta)
}

/// AES-256 (V5, R6) as crates/core/src/pdf_encrypt.rs, with its /Length
/// entries (poppler cannot decrypt without the crypt filter's /Length) and
/// its /Extensions entry.
fn encrypt(bytes: &[u8], password: &str) -> Vec<u8> {
    use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
    use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
    let mut doc = lopdf::Document::load_mem(bytes).unwrap();
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).unwrap();
    let cf: std::sync::Arc<dyn CryptFilter> = std::sync::Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: std::collections::BTreeMap::from([(b"StdCF".to_vec(), cf)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: password,
        user_password: password,
        permissions: Permissions::all(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let id = doc.trailer.get(b"Encrypt").and_then(lopdf::Object::as_reference).unwrap();
    let e = doc.get_object_mut(id).and_then(lopdf::Object::as_dict_mut).unwrap();
    e.set("Length", 256);
    e.get_mut(b"CF").and_then(lopdf::Object::as_dict_mut).unwrap().get_mut(b"StdCF").and_then(lopdf::Object::as_dict_mut).unwrap().set("Length", 32);
    let root_id = doc.trailer.get(b"Root").and_then(lopdf::Object::as_reference).unwrap();
    let adbe = lopdf::dictionary! { "BaseVersion" => "1.7", "ExtensionLevel" => 8 };
    doc.get_object_mut(root_id).and_then(lopdf::Object::as_dict_mut).unwrap().set("Extensions", lopdf::dictionary! { "ADBE" => adbe });
    lopdf_write::save(doc)
}

/// Item 7: PDF/A-2b documents (three pages: horizontal, vertical, turned
/// mixed) from both paths, and AES-256 copies of the lopdf ones.
fn pdfa() {
    let real = real_font();
    let out = root().join("out/pdfa");
    std::fs::create_dir_all(&out).unwrap();
    let names = ["h1_horizontal", "t1_vertical", "m1_mixed_rot2"];
    let data: Vec<(OcrPage, Vec<u8>, bool)> = names.iter().map(|nm| { let (i, p) = page_image(nm); (fixture(nm), i, p) }).collect();
    let pages = |vmode: VMode, with_runs: bool| -> Vec<krilla_write::PageIn<'_>> {
        data.iter()
            .map(|(o, i, p)| krilla_write::PageIn { image: i, png: *p, size: (o.width_pt, o.height_pt), runs: if with_runs { layout::runs(o, Fit::Char, vmode, 0.0) } else { vec![] } })
            .collect()
    };
    let mut files = Vec::new();
    for (label, font) in [("krilla_real", krilla_write::FontKind::Real), ("krilla_glyphless", krilla_write::FontKind::Glyphless)] {
        let (settings, meta) = pdfa_settings();
        let mut doc = krilla::Document::new_with(settings);
        doc.set_metadata(meta);
        krilla_write::write_into(&mut doc, &pages(VMode::UprightChars, true), font, krilla_write::Invisible::Transparent, &real);
        match doc.finish() {
            Ok(b) => {
                let path = out.join(format!("{label}.pdf"));
                std::fs::write(&path, b).unwrap();
                println!("{label}: krilla finished");
                files.push((label.to_string(), path));
            }
            Err(e) => println!("{label}: krilla refused: {e:?}"),
        }
    }
    let (settings, meta) = pdfa_settings();
    let base = krilla_write::image_only(&pages(VMode::IdentityV, false), settings, Some(meta));
    std::fs::write(out.join("image_only.pdf"), &base).unwrap();
    files.push(("image_only".into(), out.join("image_only.pdf")));
    let runs: Vec<Vec<layout::Run>> = data.iter().map(|d| layout::runs(&d.0, Fit::Char, VMode::IdentityV, 0.0)).collect();
    for (label, font) in [("lopdf_real", lopdf_write::LFont::Real), ("lopdf_glyphless", lopdf_write::LFont::Glyphless), ("lopdf_glyphless_empty", lopdf_write::LFont::GlyphlessEmpty)] {
        let b = lopdf_write::save(lopdf_write::add_text_layer(&base, &runs, font, lopdf_write::Positioning::Plain, &real));
        let path = out.join(format!("{label}.pdf"));
        std::fs::write(&path, &b).unwrap();
        files.push((label.to_string(), path));
        let enc = encrypt(&b, "spike");
        let epath = out.join(format!("{label}_aes256.pdf"));
        std::fs::write(&epath, enc).unwrap();
    }
    // Text read back from the encrypted copies.
    let pdfium = check::pdfium();
    println!("{}", check::header());
    for label in ["lopdf_real", "lopdf_glyphless"] {
        for suffix in ["", "_aes256"] {
            let path = out.join(format!("{label}{suffix}.pdf"));
            for (k, nm) in names.iter().enumerate() {
                let sc = check::score_page(&pdfium, &path, k, &fixture(nm), if suffix.is_empty() { None } else { Some("spike") });
                println!("{}", check::row(&format!("{label}{suffix} / {nm}"), &sc));
            }
        }
    }
    for (l, p) in files {
        println!("{l}: {}", p.display());
    }
}

/// Sample PDFs for checking by hand in viewers: the eight generated pages
/// (not ja_ref, whose image is not ours) in one document per variant.
fn samples() {
    let real = real_font();
    let out = root().join("samples");
    std::fs::create_dir_all(&out).unwrap();
    let names: Vec<&str> = PAGES.iter().copied().filter(|n| *n != "ja_ref").collect();
    let data: Vec<(OcrPage, Vec<u8>, bool)> = names.iter().map(|nm| { let (i, p) = page_image(nm); (fixture(nm), i, p) }).collect();
    let pages = |vmode: VMode, fit: Fit, with_runs: bool| -> Vec<krilla_write::PageIn<'_>> {
        data.iter()
            .map(|(o, i, p)| krilla_write::PageIn { image: i, png: *p, size: (o.width_pt, o.height_pt), runs: if with_runs { layout::runs(o, fit, vmode, 0.0) } else { vec![] } })
            .collect()
    };
    let base = krilla_write::image_only(&pages(VMode::IdentityV, Fit::Char, false), krilla::SerializeSettings::default(), None);
    for (label, fit) in [("A_lopdf_glyphless_char", Fit::Char), ("C_lopdf_glyphless_line", Fit::Line)] {
        let runs: Vec<Vec<layout::Run>> = data.iter().map(|d| layout::runs(&d.0, fit, VMode::IdentityV, 0.0)).collect();
        let b = lopdf_write::save(lopdf_write::add_text_layer(&base, &runs, lopdf_write::LFont::Glyphless, lopdf_write::Positioning::Plain, &real));
        std::fs::write(out.join(format!("{label}.pdf")), b).unwrap();
    }
    let b = krilla_write::write(&pages(VMode::UprightChars, Fit::Char, true), krilla_write::FontKind::Real, krilla_write::Invisible::Transparent, krilla::SerializeSettings::default(), None, &real);
    std::fs::write(out.join("B_krilla_real_char.pdf"), b).unwrap();
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
        Some("lopdf") => {
            let out = root().join("out").join(arg(&args, "--out").unwrap_or("lopdf"));
            let files = lopdf_pages(&args, &out);
            println!("{}", score_all(&files));
        }
        Some("overlay") => {
            // overlay <dir under out/> <page> [scale]
            let pdf = root().join("out").join(&args[2]).join(format!("{}.pdf", args[3]));
            let png = root().join("out").join(&args[2]).join(format!("{}.png", args[3]));
            let scale = args.get(4).map_or(2.0, |s| s.parse().unwrap());
            overlay::draw(&check::pdfium(), &pdf, &fixture(&args[3]), &png, scale);
            println!("{}", png.display());
        }
        Some("bench") => {
            println!("| ページ | 字 | 書き方 | 大きさ (B) | 画像だけとの差 (B) | 時間 |\n| ---: | ---: | --- | ---: | ---: | ---: |");
            for n in [1, 10, 100] {
                bench(n);
            }
        }
        Some("pdfa") => pdfa(),
        Some("samples") => samples(),
        Some("dump") => check::dump(&check::pdfium(), Path::new(&args[2]), 0),
        _ => eprintln!("usage: textlayer-spike gen|krilla|lopdf|dump ..."),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_in_real_font() {
        let real = super::real_font();
        for name in super::PAGES {
            let ocr = super::fixture(name);
            let missing: Vec<String> = ocr.chars().flat_map(|c| c.text.chars()).filter(|c| real.gid(*c) == 0).map(|c| format!("{c} U+{:04X}", c as u32)).collect();
            println!("{name}: {missing:?}");
        }
    }
}
