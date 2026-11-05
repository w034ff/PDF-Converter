//! Item 4: the two fonts the text layer can embed.
//!
//! - The real font: Noto Sans JP, subset with `subsetter` (the crate krilla
//!   uses). Advances come from its `hmtx` through `skrifa`.
//! - A glyphless font, written here from nothing: TrueType tables whose
//!   glyphs have no outline and an advance of 1000 (one em). Tesseract and
//!   OCRmyPDF embed the same kind of font; the text is decided by `ToUnicode`.
//!   `glyphless_ttf(&[])` gives the two-glyph font of the lopdf path (every CID
//!   maps to glyph 1 through `CIDToGIDMap`); with characters it gives one empty
//!   glyph per character and a `cmap`, which krilla needs because krilla
//!   decides `ToUnicode` per glyph and picks glyphs through the font's `cmap`.

use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};

pub const UPM: u16 = 1000;
pub const ASCENT: i16 = 880;
pub const DESCENT: i16 = -120;

pub struct RealFont {
    pub data: Vec<u8>,
}

impl RealFont {
    pub fn load(path: &std::path::Path) -> Self {
        Self { data: std::fs::read(path).unwrap() }
    }

    fn font(&self) -> skrifa::FontRef<'_> {
        skrifa::FontRef::new(&self.data).unwrap()
    }

    pub fn gid(&self, c: char) -> u16 {
        self.font().charmap().map(c).map_or(0, |g| g.to_u32() as u16)
    }

    pub fn upm(&self) -> f64 {
        self.font().head().map(|h| h.units_per_em() as f64).unwrap_or(1000.0)
    }

    /// Advance in 1000ths of the em.
    pub fn advance(&self, gid: u16) -> f64 {
        let f = self.font();
        let m = f.glyph_metrics(Size::unscaled(), LocationRef::default());
        m.advance_width(skrifa::GlyphId::new(gid as u32)).unwrap_or(1000.0) as f64 * 1000.0 / self.upm()
    }
}

use skrifa::raw::TableProvider;

fn checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.chunks(4) {
        let mut b = [0u8; 4];
        b[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(b));
    }
    sum
}

struct W(Vec<u8>);
impl W {
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
}

/// A TrueType font with `chars.len() + 2` glyphs: .notdef, a spare empty
/// glyph 1, then one empty glyph per character (gid = index + 2), all with an
/// advance of one em and no outline.
pub fn glyphless_ttf(chars: &[char]) -> Vec<u8> {
    let n = (chars.len() + 2) as u16;
    let name = "GlyphLessFont";
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

    let mut head = W(Vec::new());
    head.u32(0x0001_0000).u32(0x0001_0000).u32(0).u32(0x5F0F_3CF5).u16(0x000B).u16(UPM);
    head.bytes(&[0; 16]); // created, modified
    head.i16(0).i16(DESCENT).i16(UPM as i16).i16(ASCENT);
    head.u16(0).u16(8).i16(2).i16(0).i16(0); // macStyle, lowestRecPPEM, direction hint, short loca, glyph data format
    tables.push((*b"head", head.0));

    let mut hhea = W(Vec::new());
    hhea.u32(0x0001_0000).i16(ASCENT).i16(DESCENT).i16(0).u16(UPM).i16(0).i16(0).i16(UPM as i16);
    hhea.i16(1).i16(0).i16(0).bytes(&[0; 8]).i16(0).u16(n);
    tables.push((*b"hhea", hhea.0));

    let mut maxp = W(Vec::new());
    maxp.u32(0x0001_0000).u16(n);
    for v in [0u16, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
        maxp.u16(v);
    }
    tables.push((*b"maxp", maxp.0));

    let mut os2 = W(Vec::new());
    os2.u16(4).i16(UPM as i16).u16(400).u16(5).u16(0); // version, avg width, weight, width, fsType (installable)
    for v in [650i16, 600, 0, 75, 650, 600, 0, 350, 50, 300, 0] {
        os2.i16(v); // sub/superscript sizes and offsets, strikeout, family class
    }
    os2.bytes(&[0; 10]); // panose
    os2.u32(1).u32(0).u32(0).u32(0); // unicode ranges (Basic Latin bit only; not used by PDF readers)
    os2.bytes(b"NONE").u16(0x40).u16(0x20).u16(0xFFFF);
    os2.i16(ASCENT).i16(DESCENT).i16(0).u16(ASCENT as u16).u16((-DESCENT) as u16);
    os2.u32(1).u32(0).i16(500).i16(700).u16(0).u16(32).u16(0);
    tables.push((*b"OS/2", os2.0));

    let mut hmtx = W(Vec::new());
    for _ in 0..n {
        hmtx.u16(UPM).i16(0);
    }
    tables.push((*b"hmtx", hmtx.0));

    let mut loca = W(Vec::new());
    for _ in 0..=n {
        loca.u16(0);
    }
    tables.push((*b"loca", loca.0));
    tables.push((*b"glyf", Vec::new()));

    // cmap format 12 (platform 3, encoding 10).
    let mut sorted: Vec<(u32, u32)> = chars.iter().enumerate().map(|(i, c)| (*c as u32, i as u32 + 2)).collect();
    sorted.sort();
    let mut cmap = W(Vec::new());
    cmap.u16(0).u16(1).u16(3).u16(10).u32(12);
    cmap.u16(12).u16(0).u32(16 + 12 * sorted.len() as u32).u32(0).u32(sorted.len() as u32);
    for (c, g) in &sorted {
        cmap.u32(*c).u32(*c).u32(*g);
    }
    tables.push((*b"cmap", cmap.0));

    let records: Vec<(u16, String)> =
        vec![(1, name.into()), (2, "Regular".into()), (3, name.into()), (4, name.into()), (6, name.into())];
    let mut strings = Vec::new();
    let mut nm = W(Vec::new());
    nm.u16(0).u16(records.len() as u16).u16(6 + 12 * records.len() as u16);
    for (id, s) in &records {
        let utf16: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
        nm.u16(3).u16(1).u16(0x409).u16(*id).u16(utf16.len() as u16).u16(strings.len() as u16);
        strings.extend(utf16);
    }
    nm.bytes(&strings);
    tables.push((*b"name", nm.0));

    let mut post = W(Vec::new());
    post.u32(0x0003_0000).u32(0).i16(-100).i16(50).u32(1).u32(0).u32(0).u32(0).u32(0);
    tables.push((*b"post", post.0));

    tables.sort_by(|a, b| a.0.cmp(&b.0));
    let num = tables.len() as u16;
    let entry_selector = 15 - num.leading_zeros() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let mut out = W(Vec::new());
    out.u32(0x0001_0000).u16(num).u16(search_range).u16(entry_selector).u16(num * 16 - search_range);
    let mut offset = 12 + 16 * num as u32;
    let mut head_offset = 0;
    for (tag, data) in &tables {
        if tag == b"head" {
            head_offset = offset as usize;
        }
        out.bytes(tag).u32(checksum(data)).u32(offset).u32(data.len() as u32);
        offset += (data.len() as u32).div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.bytes(data);
        while out.0.len() % 4 != 0 {
            out.0.push(0);
        }
    }
    let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out.0));
    out.0[head_offset + 8..head_offset + 12].copy_from_slice(&adjust.to_be_bytes());
    out.0
}
