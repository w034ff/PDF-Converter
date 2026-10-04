//! Spike: write one image per PDF page with krilla, keeping JPEG bytes as-is.
//!
//! Usage: write-bench <out.pdf> <image>...
//! Page size: 1 px = 0.75 pt (96 dpi). EXIF orientation is applied with a
//! transform so the JPEG data itself is not touched.

use image::ImageDecoder;
use image::metadata::Orientation;
use krilla::Document;
use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;

const PT_PER_PX: f32 = 0.75;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut doc = Document::new();
    for path in &args[2..] {
        let bytes = std::fs::read(path).unwrap();
        let mut decoder = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let (w, h) = decoder.dimensions();
        let ext = path.rsplit('.').next().unwrap().to_ascii_lowercase();
        let img = match ext.as_str() {
            "jpg" | "jpeg" => Image::from_jpeg(bytes.clone().into(), true),
            "png" => Image::from_png(bytes.clone().into(), true),
            "webp" => Image::from_webp(bytes.clone().into(), true),
            _ => {
                let rgba = image::DynamicImage::from_decoder(decoder).unwrap().to_rgba8();
                Ok(Image::from_rgba8(rgba.into_raw(), w, h))
            }
        }
        .unwrap();
        let (iw, ih) = (w as f32 * PT_PER_PX, h as f32 * PT_PER_PX);
        let swaps = matches!(
            orientation,
            Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
        );
        let (pw, ph) = if swaps { (ih, iw) } else { (iw, ih) };
        // Maps the unrotated image box (0,0)-(iw,ih) onto the page. PDF page
        // coordinates in krilla have y pointing down, like the image.
        let t = match orientation {
            Orientation::NoTransforms => Transform::from_row(1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            Orientation::FlipHorizontal => Transform::from_row(-1.0, 0.0, 0.0, 1.0, iw, 0.0),
            Orientation::Rotate180 => Transform::from_row(-1.0, 0.0, 0.0, -1.0, iw, ih),
            Orientation::FlipVertical => Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, ih),
            Orientation::Rotate90FlipH => Transform::from_row(0.0, 1.0, 1.0, 0.0, 0.0, 0.0),
            Orientation::Rotate90 => Transform::from_row(0.0, 1.0, -1.0, 0.0, ih, 0.0),
            Orientation::Rotate270FlipH => Transform::from_row(0.0, -1.0, -1.0, 0.0, ih, iw),
            Orientation::Rotate270 => Transform::from_row(0.0, -1.0, 1.0, 0.0, 0.0, iw),
        };
        eprintln!("{path}: {w}x{h} {orientation:?} -> page {pw}x{ph} pt");
        let mut page = doc.start_page_with(PageSettings::from_wh(pw, ph).unwrap());
        let mut surface = page.surface();
        surface.push_transform(&t);
        surface.draw_image(img, Size::from_wh(iw, ih).unwrap());
        surface.pop();
        surface.finish();
        page.finish();
    }
    std::fs::write(&args[1], doc.finish().unwrap()).unwrap();
}
