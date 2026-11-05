//! Item 3: the image → PDF path. The page image and an invisible text layer
//! written with krilla 0.8.2's public API.
//!
//! What krilla cannot do (see REPORT.md, item 3):
//! - text rendering mode 3: `Surface::draw_glyphs` only fills and/or strokes
//!   (`TextRenderingMode::{Fill, Stroke, FillStroke}` in `content.rs`), so the
//!   text is filled with opacity 0 (an ExtGState with `/ca 0`), or drawn
//!   first and covered by the image (`Invisible::UnderImage`);
//! - vertical writing mode: krilla always writes `Identity-H` and a
//!   horizontal `WMode`, so vertical lines are upright characters placed one
//!   by one, or the column turned 90°;
//! - `Tz`/`Tc`: a line is stretched with a transform instead.

use std::collections::BTreeSet;

use krilla::Document;
use krilla::geom::{Point, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::text::{Font, GlyphId, KrillaGlyph};

use crate::fonts::{RealFont, glyphless_ttf};
use crate::layout::Run;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    Real,
    Glyphless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invisible {
    /// Filled with opacity 0, above the image.
    Transparent,
    /// Filled black, under the image.
    UnderImage,
}

pub struct PageIn<'a> {
    pub image: &'a [u8],
    pub png: bool,
    pub size: (f64, f64),
    pub runs: Vec<Run>,
}

pub fn write(
    pages: &[PageIn],
    font_kind: FontKind,
    invisible: Invisible,
    settings: krilla::SerializeSettings,
    metadata: Option<krilla::metadata::Metadata>,
    real: &RealFont,
) -> Vec<u8> {
    let mut doc = Document::new_with(settings);
    if let Some(m) = metadata {
        doc.set_metadata(m);
    }
    write_into(&mut doc, pages, font_kind, invisible, real);
    doc.finish().unwrap()
}

pub fn write_into(doc: &mut Document, pages: &[PageIn], font_kind: FontKind, invisible: Invisible, real: &RealFont) {
    // The glyphless font has one glyph per character of the whole document.
    let chars: Vec<char> =
        pages.iter().flat_map(|p| p.runs.iter().flat_map(|r| r.text.iter().copied())).collect::<BTreeSet<_>>().into_iter().collect();
    let (font, gid_of): (Font, Box<dyn Fn(char) -> (u32, f64)>) = match font_kind {
        FontKind::Real => (
            Font::new(real.data.clone().into(), 0).unwrap(),
            Box::new(|c| {
                let g = real.gid(c);
                (g as u32, real.advance(g))
            }),
        ),
        FontKind::Glyphless => {
            let index: std::collections::HashMap<char, u32> = chars.iter().enumerate().map(|(i, c)| (*c, i as u32 + 2)).collect();
            (Font::new(glyphless_ttf(&chars).into(), 0).unwrap(), Box::new(move |c| (index[&c], 1000.0)))
        }
    };
    for p in pages {
        let (w, h) = (p.size.0 as f32, p.size.1 as f32);
        let mut page = doc.start_page_with(PageSettings::new(Size::from_wh(w, h).unwrap()));
        let mut surface = page.surface();
        let img = if p.png {
            Image::from_png(p.image.to_vec().into(), false).unwrap()
        } else {
            Image::from_jpeg(p.image.to_vec().into(), false).unwrap()
        };
        if invisible == Invisible::Transparent {
            surface.draw_image(img.clone(), Size::from_wh(w, h).unwrap());
            surface.set_fill(Some(Fill { opacity: NormalizedF32::ZERO, ..Fill::default() }));
        } else {
            surface.set_fill(Some(Fill::default()));
        }
        for r in &p.runs {
            assert!(!r.vertical, "krilla has no vertical writing mode");
            let text: String = r.text.iter().collect();
            // The line is stretched to its length (sx, like Tz); within it each
            // glyph advances to the next character's start (like TJ), in the
            // stretched space.
            let natural: f64 = r.text.iter().map(|c| gid_of(*c).1 / 1000.0 * r.size).sum();
            let sx = if natural > 0.0 { r.length / natural } else { 1.0 };
            let mut glyphs = Vec::new();
            let mut byte = 0;
            for (k, c) in r.text.iter().enumerate() {
                let (gid, adv) = gid_of(*c);
                let len = c.len_utf8();
                let x_advance = match &r.starts {
                    Some(s) if k + 1 < s.len() => ((s[k + 1] - s[k]) / (r.size * sx)) as f32,
                    Some(s) => (((r.length - s[k]).max(0.0)) / (r.size * sx)) as f32,
                    None => (adv / 1000.0) as f32,
                };
                glyphs.push(KrillaGlyph {
                    glyph_id: GlyphId::new(gid),
                    text_range: byte..byte + len,
                    x_advance,
                    x_offset: 0.0,
                    y_offset: 0.0,
                    y_advance: 0.0,
                    location: None,
                });
                byte += len;
            }
            let (c, s) = (r.angle.cos(), r.angle.sin());
            // translate(origin) · rotate(angle) · scale(sx, 1)
            let t = Transform::from_row(
                (c * sx) as f32,
                (s * sx) as f32,
                (-s) as f32,
                c as f32,
                r.origin[0] as f32,
                r.origin[1] as f32,
            );
            surface.push_transform(&t);
            surface.draw_glyphs(Point::from_xy(0.0, 0.0), &glyphs, font.clone(), &text, r.size as f32, false);
            surface.pop();
        }
        if invisible == Invisible::UnderImage {
            surface.draw_image(img, Size::from_wh(w, h).unwrap());
        }
        surface.finish();
        page.finish();
    }
}

/// The image alone, the input of the lopdf path and the size baseline.
pub fn image_only(pages: &[PageIn], settings: krilla::SerializeSettings, metadata: Option<krilla::metadata::Metadata>) -> Vec<u8> {
    let mut doc = Document::new_with(settings);
    if let Some(m) = metadata {
        doc.set_metadata(m);
    }
    for p in pages {
        let (w, h) = (p.size.0 as f32, p.size.1 as f32);
        let mut page = doc.start_page_with(PageSettings::new(Size::from_wh(w, h).unwrap()));
        let mut surface = page.surface();
        let img = if p.png {
            Image::from_png(p.image.to_vec().into(), false).unwrap()
        } else {
            Image::from_jpeg(p.image.to_vec().into(), false).unwrap()
        };
        surface.draw_image(img, Size::from_wh(w, h).unwrap());
        surface.finish();
        page.finish();
    }
    doc.finish().unwrap()
}
