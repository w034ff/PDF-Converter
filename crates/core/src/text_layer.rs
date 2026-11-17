//! The invisible text layer that OCR adds to a PDF (design ocr §9), so that a
//! viewer can search, select and copy the text of a scanned page.
//!
//! Every character is placed on its own in the quad OCR found for it, in text
//! rendering mode 3 (invisible): horizontal text fits its width with `Tz`,
//! vertical text uses the `Identity-V` writing mode. One glyphless font serves
//! every character: its glyph is an em square that is never drawn, and
//! `ToUnicode` decides the text. A character the font does not need to know
//! cannot fall back to `.notdef`, and the font is about 1.5 KB whatever the
//! text.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::error::{PdfWriteError, write_failed};
use crate::ocr::{OcrPage, Orientation, Quad};

/// The glyphless font's units per em, ascent and descent.
const UNITS_PER_EM: u16 = 1000;
const ASCENT: i16 = 880;
const DESCENT: i16 = -120;

/// The part of a horizontal character's height below its baseline: the
/// font's descent over its height.
const DESCENT_FRACTION: f64 = 0.12;

/// Characters at most this many points high or wide are left out: there is
/// nothing to select.
const MIN_CHAR_SIZE_PT: f64 = 0.01;

/// Consecutive characters whose directions differ by less than this move
/// with `Td` from the previous one instead of a new `Tm`.
const SAME_ANGLE_RAD: f64 = 0.2 * std::f64::consts::PI / 180.0;

/// The highest code a character outside the Basic Multilingual Plane, or in
/// the private use area, is given; such characters count down from here.
const PRIVATE_CODE_TOP: u16 = 0xF8FF;
const PRIVATE_CODE_BOTTOM: u16 = 0xE000;

/// Adds the text of `pages` to `doc`: each entry is the index of a page
/// (counted from 0) and that page's OCR result, in points on the page as
/// shown. Pages without characters are left as they are.
///
/// # Errors
///
/// Returns [`PdfWriteError`] when a page index is out of range, a page has no
/// usable page box, an OCR result has no size, or there are more characters
/// outside the Basic Multilingual Plane than the font has codes for.
pub fn add_text_layer(
    doc: &mut Document,
    pages: &[(usize, &OcrPage)],
) -> Result<(), PdfWriteError> {
    let chars: BTreeSet<char> = pages
        .iter()
        .flat_map(|(_, page)| page_chars(page))
        .map(|(c, _, _)| c)
        .collect();
    if chars.is_empty() {
        return Ok(());
    }
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let codes = codes(&chars)?;
    let fonts = add_fonts(doc, &codes);
    for (index, page) in pages {
        let page_id = *page_ids
            .get(*index)
            .ok_or_else(|| write_failed(format!("the PDF has no page {}", index + 1)))?;
        let placed = page_chars(page);
        if placed.is_empty() {
            continue;
        }
        let frame = Frame::of(doc, page_id, page)?;
        let names = add_font_resources(doc, page_id, &fonts)?;
        let layer = content(&placed, &frame, &codes, &names);
        wrap_contents(doc, page_id, layer)?;
    }
    Ok(())
}

/// The page's characters in reading order: each with its quad and whether it
/// is read top to bottom. A run of several characters in one `OcrChar` shares
/// its quad out along the reading direction.
fn page_chars(page: &OcrPage) -> Vec<(char, Quad, bool)> {
    let mut out = Vec::new();
    for line in page.blocks.iter().flat_map(|b| &b.lines) {
        let vertical = line.orientation == Orientation::Vertical;
        for c in line.words.iter().flat_map(|w| &w.chars) {
            let text: Vec<char> = c.text.chars().filter(|c| !c.is_control()).collect();
            let n = text.len() as f64;
            for (k, ch) in text.into_iter().enumerate() {
                let part = |t: f64| {
                    let lerp = |a: [f64; 2], b: [f64; 2]| {
                        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                    };
                    (lerp(c.quad[0], c.quad[1]), lerp(c.quad[3], c.quad[2]))
                };
                let ((a0, d0), (a1, d1)) = (part(k as f64 / n), part((k + 1) as f64 / n));
                out.push((ch, [a0, a1, d1, d0], vertical));
            }
        }
    }
    out
}

/// The two-byte code of each character: its own code in the Basic
/// Multilingual Plane, a code of the private use area for the others.
fn codes(chars: &BTreeSet<char>) -> Result<BTreeMap<char, u16>, PdfWriteError> {
    let mut next_private = PRIVATE_CODE_TOP;
    let mut codes = BTreeMap::new();
    for &c in chars {
        let code = match u16::try_from(u32::from(c)) {
            Ok(code) if !(PRIVATE_CODE_BOTTOM..=PRIVATE_CODE_TOP).contains(&code) => code,
            _ => {
                if next_private < PRIVATE_CODE_BOTTOM {
                    return Err(write_failed("too many characters outside the BMP"));
                }
                next_private -= 1;
                next_private + 1
            }
        };
        codes.insert(c, code);
    }
    Ok(codes)
}

/// The document's two fonts: horizontal (`Identity-H`) and vertical
/// (`Identity-V`) over one glyphless CID font.
struct Fonts {
    horizontal: ObjectId,
    vertical: ObjectId,
}

fn compressed(doc: &mut Document, dict: Dictionary, data: Vec<u8>) -> ObjectId {
    let mut stream = Stream::new(dict, data);
    // A stream that does not shrink stays as it is.
    let _ = stream.compress();
    doc.add_object(stream)
}

fn add_fonts(doc: &mut Document, codes: &BTreeMap<char, u16>) -> Fonts {
    const NAME: &str = "GlyphLessFont";
    let font_program = glyphless_ttf();
    let file = compressed(
        doc,
        dictionary! { "Length1" => font_program.len() as i64 },
        font_program,
    );
    let descriptor = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => NAME,
        "Flags" => 4,
        "FontBBox" => vec![0.into(), i64::from(DESCENT).into(), i64::from(UNITS_PER_EM).into(), i64::from(ASCENT).into()],
        "ItalicAngle" => 0,
        "Ascent" => i64::from(ASCENT),
        "Descent" => i64::from(DESCENT),
        "CapHeight" => 700,
        "StemV" => 80,
        "FontFile2" => file,
    });
    // Every code but 0 draws glyph 1, the em square.
    let mut map = vec![0_u8; 65536 * 2];
    for pair in map.as_chunks_mut::<2>().0.iter_mut().skip(1) {
        pair[1] = 1;
    }
    let map = compressed(doc, Dictionary::new(), map);
    let cid_font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => NAME,
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
        "FontDescriptor" => descriptor,
        "DW" => i64::from(UNITS_PER_EM),
        "DW2" => vec![i64::from(ASCENT).into(), (-i64::from(UNITS_PER_EM)).into()],
        "CIDToGIDMap" => map,
    });
    let to_unicode = compressed(doc, Dictionary::new(), to_unicode(codes));
    let mut type0 = |encoding: &str| {
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => Object::Name(format!("{NAME}-{encoding}").into_bytes()),
            "Encoding" => Object::Name(encoding.as_bytes().to_vec()),
            "DescendantFonts" => vec![cid_font.into()],
            "ToUnicode" => to_unicode,
        })
    };
    Fonts {
        horizontal: type0("Identity-H"),
        vertical: type0("Identity-V"),
    }
}

/// The `ToUnicode` CMap from each code back to its character.
fn to_unicode(codes: &BTreeMap<char, u16>) -> Vec<u8> {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let by_code: BTreeMap<u16, char> = codes.iter().map(|(c, code)| (*code, *c)).collect();
    let entries: Vec<(&u16, &char)> = by_code.iter().collect();
    // A CMap section holds at most 100 entries.
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (code, c) in chunk {
            let mut units = [0_u16; 2];
            let hex: String = c
                .encode_utf16(&mut units)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            let _ = writeln!(s, "<{code:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

/// Where the page as shown lies in its user space.
struct Frame {
    /// The visible page box: the crop box within the media box.
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// Clockwise degrees the page is shown turned by: 0, 90, 180 or 270.
    rotate: i64,
    /// User space units per point of the OCR result, across and down the
    /// page as shown. A page with `UserUnit` gets the factor here, whichever
    /// way the renderer that OCR saw counted it.
    sx: f64,
    sy: f64,
}

/// A page attribute, inherited from the page tree when the page lacks it.
fn inherited<'a>(doc: &'a Document, page_id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    // The depth of a page tree is small; the limit only stops a cycle.
    for _ in 0..64 {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").and_then(Object::as_reference).ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

fn number(doc: &Document, object: &Object) -> Option<f64> {
    match resolve(doc, object) {
        Object::Integer(v) => Some(*v as f64),
        Object::Real(v) => Some(f64::from(*v)),
        _ => None,
    }
}

/// A rectangle attribute as `[x0, y0, x1, y1]` with `x0 < x1` and `y0 < y1`.
fn rectangle(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<[f64; 4]> {
    let array = resolve(doc, inherited(doc, page_id, key)?)
        .as_array()
        .ok()?;
    let values: Vec<f64> = array.iter().filter_map(|v| number(doc, v)).collect();
    let [a, b, c, d] = values[..] else {
        return None;
    };
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

impl Frame {
    fn of(doc: &Document, page_id: ObjectId, page: &OcrPage) -> Result<Self, PdfWriteError> {
        let media = rectangle(doc, page_id, b"MediaBox")
            .ok_or_else(|| write_failed("a page has no MediaBox"))?;
        let [x0, y0, x1, y1] = match rectangle(doc, page_id, b"CropBox") {
            Some(crop) => [
                crop[0].max(media[0]),
                crop[1].max(media[1]),
                crop[2].min(media[2]),
                crop[3].min(media[3]),
            ],
            None => media,
        };
        let rotate = inherited(doc, page_id, b"Rotate")
            .and_then(|r| number(doc, r))
            .map_or(0, |r| (r as i64).rem_euclid(360) / 90 * 90);
        let (w, h) = (x1 - x0, y1 - y0);
        let (shown_w, shown_h) = if rotate % 180 == 0 { (w, h) } else { (h, w) };
        if w <= 0.0 || h <= 0.0 || page.width_pt <= 0.0 || page.height_pt <= 0.0 {
            return Err(write_failed("a page or its OCR result has no size"));
        }
        Ok(Self {
            x0,
            y0,
            x1,
            y1,
            rotate,
            sx: shown_w / page.width_pt,
            sy: shown_h / page.height_pt,
        })
    }

    /// A point of the page as shown (origin at the top-left corner, y down)
    /// in user space.
    fn to_user(&self, p: [f64; 2]) -> [f64; 2] {
        let (u, v) = (p[0] * self.sx, p[1] * self.sy);
        match self.rotate {
            // Turned clockwise: the shown top-left corner is the box's
            // bottom-left one, and the shown x runs up the box.
            90 => [self.x0 + v, self.y0 + u],
            180 => [self.x1 - u, self.y0 + v],
            270 => [self.x1 - v, self.y1 - u],
            _ => [self.x0 + u, self.y1 - v],
        }
    }
}

/// The names the page's resources give the two fonts.
struct FontNames {
    horizontal: String,
    vertical: String,
}

/// Gives the page its own resources with the two fonts added under names it
/// does not use yet. Inherited or shared resources are copied into the page,
/// so that other pages keep theirs.
fn add_font_resources(
    doc: &mut Document,
    page_id: ObjectId,
    fonts: &Fonts,
) -> Result<FontNames, PdfWriteError> {
    let mut resources = match inherited(doc, page_id, b"Resources").map(|r| resolve(doc, r)) {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    let mut font_dict = match resources.get(b"Font").ok().map(|f| resolve(doc, f)) {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    let mut free = |prefix: &str| {
        let name = (0..)
            .map(|n| {
                if n == 0 {
                    prefix.to_owned()
                } else {
                    format!("{prefix}{n}")
                }
            })
            .find(|name| !font_dict.has(name.as_bytes()))
            .unwrap_or_else(|| prefix.to_owned());
        font_dict.set(name.as_bytes(), Object::Null);
        name
    };
    let names = FontNames {
        horizontal: free("OcrH"),
        vertical: free("OcrV"),
    };
    font_dict.set(names.horizontal.as_bytes(), fonts.horizontal);
    font_dict.set(names.vertical.as_bytes(), fonts.vertical);
    resources.set("Font", font_dict);
    doc.get_dictionary_mut(page_id)
        .map_err(write_failed)?
        .set("Resources", resources);
    Ok(names)
}

/// Sets the page's contents to its old contents inside `q … Q`, so that a
/// graphics state they leave behind does not reach the text, then `layer`.
fn wrap_contents(
    doc: &mut Document,
    page_id: ObjectId,
    layer: Vec<u8>,
) -> Result<(), PdfWriteError> {
    let page = doc.get_dictionary(page_id).map_err(write_failed)?;
    let mut contents: Vec<Object> = match page.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(parts)) => parts.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Ok(Object::Array(parts)) => parts.clone(),
        _ => Vec::new(),
    };
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut body = b"Q\n".to_vec();
    body.extend(layer);
    let close = compressed(doc, Dictionary::new(), body);
    contents.insert(0, open.into());
    contents.push(close.into());
    doc.get_dictionary_mut(page_id)
        .map_err(write_failed)?
        .set("Contents", contents);
    Ok(())
}

/// A number with at most `decimals` decimals and no trailing zeros.
fn num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        &s
    };
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn mean(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn length(v: [f64; 2]) -> f64 {
    v[0].hypot(v[1])
}

/// How one character is drawn, in user space.
struct Placement {
    vertical: bool,
    /// Horizontal: the start of the baseline. Vertical: the top centre of
    /// the character, the glyph's vertical origin.
    origin: [f64; 2],
    /// The direction of the text space's x axis.
    angle: f64,
    size: f64,
    /// Horizontal scaling in percent.
    scale: f64,
}

/// Where a character in `quad` (corners from the start of the reading
/// direction, on the page as shown) goes in user space.
fn placement(frame: &Frame, quad: &Quad, vertical: bool) -> Option<Placement> {
    let [a, b, c, d] = quad.map(|p| frame.to_user(p));
    if vertical {
        // a, b, c, d: top-right, bottom-right, bottom-left, top-left.
        let across = mean(sub(a, d), sub(b, c));
        let size = length(across);
        (size > MIN_CHAR_SIZE_PT).then(|| Placement {
            vertical,
            origin: mean(a, d),
            angle: across[1].atan2(across[0]),
            size,
            scale: 100.0,
        })
    } else {
        // a, b, c, d: top-left, top-right, bottom-right, bottom-left.
        let along = mean(sub(b, a), sub(c, d));
        let up = mean(sub(a, d), sub(b, c));
        let (width, size) = (length(along), length(up));
        (size > MIN_CHAR_SIZE_PT && width > MIN_CHAR_SIZE_PT).then(|| Placement {
            vertical,
            origin: [
                d[0] + up[0] * DESCENT_FRACTION,
                d[1] + up[1] * DESCENT_FRACTION,
            ],
            angle: along[1].atan2(along[0]),
            size,
            // The glyph is one em wide.
            scale: 100.0 * width / size,
        })
    }
}

/// The page's text layer, one character at a time. Operators that did not
/// change since the previous character are left out, and a character at the
/// previous one's angle moves with `Td` in its text space instead of a new
/// `Tm`; this halved the layer in the survey.
fn content(
    chars: &[(char, Quad, bool)],
    frame: &Frame,
    codes: &BTreeMap<char, u16>,
    names: &FontNames,
) -> Vec<u8> {
    let mut s = String::from("BT\n3 Tr\n");
    let mut font: Option<(bool, String)> = None;
    let mut scale = String::from("100");
    // The angle and origin of the current text line matrix.
    let mut line: Option<(f64, [f64; 2])> = None;
    for (c, quad, vertical) in chars {
        let (Some(code), Some(p)) = (codes.get(c), placement(frame, quad, *vertical)) else {
            continue;
        };
        let size = num(p.size, 3);
        if font.as_ref() != Some(&(p.vertical, size.clone())) {
            let name = if p.vertical {
                &names.vertical
            } else {
                &names.horizontal
            };
            let _ = write!(s, "/{name} {size} Tf ");
            font = Some((p.vertical, size));
        }
        let tz = num(p.scale, 3);
        if tz != scale {
            let _ = write!(s, "{tz} Tz ");
            scale = tz;
        }
        match line {
            Some((angle, [px, py])) if (angle - p.angle).abs() < SAME_ANGLE_RAD => {
                // The move in the previous text space, rounded as written so
                // that rounding does not add up along a line.
                let (dx, dy) = (p.origin[0] - px, p.origin[1] - py);
                let (cos, sin) = (angle.cos(), angle.sin());
                let round = |v: f64| (v * 100.0).round() / 100.0;
                let (tx, ty) = (round(dx * cos + dy * sin), round(-dx * sin + dy * cos));
                let _ = write!(s, "{} {} Td ", num(tx, 2), num(ty, 2));
                line = Some((angle, [px + tx * cos - ty * sin, py + tx * sin + ty * cos]));
            }
            _ => {
                let (cos, sin) = (p.angle.cos(), p.angle.sin());
                let _ = write!(
                    s,
                    "{} {} {} {} {} {} Tm ",
                    num(cos, 4),
                    num(sin, 4),
                    num(-sin, 4),
                    num(cos, 4),
                    num(p.origin[0], 2),
                    num(p.origin[1], 2)
                );
                line = Some((p.angle, p.origin));
            }
        }
        let _ = writeln!(s, "<{code:04X}> Tj");
    }
    s.push_str("ET\n");
    s.into_bytes()
}

/// Big-endian table data.
struct Table(Vec<u8>);

impl Table {
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

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0_u32, |sum, chunk| {
        let mut word = [0_u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// A TrueType font of two glyphs, `.notdef` and an em square, both one em
/// wide (design ocr §9). The square is never drawn, since the text is
/// invisible; without an outline, pdfium (the app's own and Chrome's and
/// Edge's) dropped the characters when it took the text out.
fn glyphless_ttf() -> Vec<u8> {
    const GLYPHS: u16 = 2;
    const NAME: &str = "GlyphLessFont";
    let em = UNITS_PER_EM as i16;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

    let mut head = Table(Vec::new());
    head.u32(0x0001_0000)
        .u32(0x0001_0000)
        .u32(0)
        .u32(0x5F0F_3CF5)
        .u16(0x000B)
        .u16(UNITS_PER_EM);
    head.bytes(&[0; 16]); // created, modified
    head.i16(0).i16(DESCENT).i16(em).i16(ASCENT);
    // macStyle, lowestRecPPEM, font direction hint, long loca offsets, glyph data format.
    head.u16(0).u16(8).i16(2).i16(1).i16(0);
    tables.push((*b"head", head.0));

    let mut hhea = Table(Vec::new());
    hhea.u32(0x0001_0000)
        .i16(ASCENT)
        .i16(DESCENT)
        .i16(0)
        .u16(UNITS_PER_EM)
        .i16(0)
        .i16(0)
        .i16(em);
    hhea.i16(1).i16(0).i16(0).bytes(&[0; 8]).i16(0).u16(GLYPHS);
    tables.push((*b"hhea", hhea.0));

    let mut maxp = Table(Vec::new());
    maxp.u32(0x0001_0000).u16(GLYPHS);
    for v in [4_u16, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
        maxp.u16(v);
    }
    tables.push((*b"maxp", maxp.0));

    let mut os2 = Table(Vec::new());
    // Version, average width, weight, width class, fsType (installable).
    os2.u16(4).i16(em).u16(400).u16(5).u16(0);
    // Sub- and superscript sizes and offsets, strikeout, family class.
    for v in [650_i16, 600, 0, 75, 650, 600, 0, 350, 50, 300, 0] {
        os2.i16(v);
    }
    os2.bytes(&[0; 10]); // panose
    os2.u32(1).u32(0).u32(0).u32(0); // Unicode ranges
    os2.bytes(b"NONE").u16(0x40).u16(0x20).u16(0xFFFF);
    os2.i16(ASCENT)
        .i16(DESCENT)
        .i16(0)
        .u16(ASCENT.unsigned_abs())
        .u16(DESCENT.unsigned_abs());
    os2.u32(1).u32(0).i16(500).i16(700).u16(0).u16(32).u16(0);
    tables.push((*b"OS/2", os2.0));

    let mut hmtx = Table(Vec::new());
    for _ in 0..GLYPHS {
        hmtx.u16(UNITS_PER_EM).i16(0);
    }
    tables.push((*b"hmtx", hmtx.0));

    // The em square from (0, descent) to (em, ascent): one contour of four
    // on-curve points.
    let mut square = Table(Vec::new());
    square
        .i16(1)
        .i16(0)
        .i16(DESCENT)
        .i16(em)
        .i16(ASCENT)
        .u16(3)
        .u16(0);
    square.bytes(&[0x01; 4]);
    for dx in [0, em, 0, -em] {
        square.i16(dx);
    }
    for dy in [DESCENT, 0, ASCENT - DESCENT, 0] {
        square.i16(dy);
    }
    let mut loca = Table(Vec::new());
    loca.u32(0).u32(0).u32(square.0.len() as u32);
    tables.push((*b"loca", loca.0));
    tables.push((*b"glyf", square.0));

    // An empty format 12 cmap: the PDF picks the glyph through CIDToGIDMap.
    let mut cmap = Table(Vec::new());
    cmap.u16(0).u16(1).u16(3).u16(10).u32(12);
    cmap.u16(12).u16(0).u32(16).u32(0).u32(0);
    tables.push((*b"cmap", cmap.0));

    let records: [(u16, &str); 5] = [(1, NAME), (2, "Regular"), (3, NAME), (4, NAME), (6, NAME)];
    let mut strings = Vec::new();
    let mut name = Table(Vec::new());
    name.u16(0)
        .u16(records.len() as u16)
        .u16(6 + 12 * records.len() as u16);
    for (id, text) in records {
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        name.u16(3)
            .u16(1)
            .u16(0x409)
            .u16(id)
            .u16(utf16.len() as u16)
            .u16(strings.len() as u16);
        strings.extend(utf16);
    }
    name.bytes(&strings);
    tables.push((*b"name", name.0));

    let mut post = Table(Vec::new());
    post.u32(0x0003_0000)
        .u32(0)
        .i16(-100)
        .i16(50)
        .u32(1)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0);
    tables.push((*b"post", post.0));

    tables.sort_by_key(|table| table.0);
    let count = tables.len() as u16;
    let entry_selector = 15 - count.leading_zeros() as u16;
    let search_range = (1_u16 << entry_selector) * 16;
    let mut out = Table(Vec::new());
    out.u32(0x0001_0000)
        .u16(count)
        .u16(search_range)
        .u16(entry_selector)
        .u16(count * 16 - search_range);
    let mut offset = 12 + 16 * u32::from(count);
    let mut head_offset = 0;
    for (tag, data) in &tables {
        if tag == b"head" {
            head_offset = offset as usize;
        }
        out.bytes(tag)
            .u32(checksum(data))
            .u32(offset)
            .u32(data.len() as u32);
        offset += (data.len() as u32).div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.bytes(data);
        while !out.0.len().is_multiple_of(4) {
            out.0.push(0);
        }
    }
    let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(checksum(&out.0));
    out.0[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    out.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::{OcrBlock, OcrChar, OcrLine, OcrWord, quad_bbox};

    fn page(lines: Vec<(Orientation, Vec<(&str, Quad)>)>) -> OcrPage {
        let lines = lines
            .into_iter()
            .map(|(orientation, chars)| {
                let quad = chars[0].1;
                OcrLine {
                    quad,
                    bbox: quad_bbox(&quad),
                    orientation,
                    text: chars.iter().map(|c| c.0).collect(),
                    confidence: 1.0,
                    words: vec![OcrWord {
                        quad,
                        text: String::new(),
                        confidence: 1.0,
                        chars: chars
                            .into_iter()
                            .map(|(text, quad)| OcrChar {
                                quad,
                                text: text.to_owned(),
                                confidence: 1.0,
                            })
                            .collect(),
                    }],
                }
            })
            .collect::<Vec<_>>();
        OcrPage {
            width_pt: 200.0,
            height_pt: 100.0,
            engine: "test".into(),
            blocks: vec![OcrBlock {
                bbox: quad_bbox(&lines[0].quad),
                orientation: lines[0].orientation,
                lines,
            }],
        }
    }

    fn frame(rotate: i64) -> Frame {
        Frame {
            x0: 10.0,
            y0: 20.0,
            x1: 210.0,
            y1: 120.0,
            rotate,
            sx: 1.0,
            sy: 1.0,
        }
    }

    #[test]
    fn the_shown_top_left_corner_follows_the_rotation() {
        assert_eq!(frame(0).to_user([0.0, 0.0]), [10.0, 120.0]);
        assert_eq!(frame(90).to_user([0.0, 0.0]), [10.0, 20.0]);
        assert_eq!(frame(180).to_user([0.0, 0.0]), [210.0, 20.0]);
        assert_eq!(frame(270).to_user([0.0, 0.0]), [210.0, 120.0]);
        // One point right and two down on the page as shown.
        assert_eq!(frame(90).to_user([1.0, 2.0]), [12.0, 21.0]);
        assert_eq!(frame(270).to_user([1.0, 2.0]), [208.0, 119.0]);
    }

    #[test]
    fn a_horizontal_character_sits_on_its_baseline_and_fills_its_width() {
        let quad = [[10.0, 10.0], [15.0, 10.0], [15.0, 20.0], [10.0, 20.0]];
        let Some(p) = placement(&frame(0), &quad, false) else {
            unreachable!("the character is large enough")
        };
        assert_eq!(p.size, 10.0);
        assert!((p.scale - 50.0).abs() < 1e-9);
        assert_eq!(p.angle, 0.0);
        // The bottom edge is at y = 120 - 20 = 100; the baseline 1.2 above it.
        assert!((p.origin[0] - 20.0).abs() < 1e-9 && (p.origin[1] - 101.2).abs() < 1e-9);
    }

    #[test]
    fn a_vertical_character_hangs_from_its_top_centre() {
        // Top-right first.
        let quad = [[20.0, 10.0], [20.0, 20.0], [10.0, 20.0], [10.0, 10.0]];
        let Some(p) = placement(&frame(0), &quad, true) else {
            unreachable!("the character is large enough")
        };
        assert!(p.vertical);
        assert_eq!(p.size, 10.0);
        assert_eq!(p.origin, [25.0, 110.0]);
    }

    #[test]
    fn codes_keep_the_bmp_and_move_the_rest_to_the_private_use_area() {
        let chars: BTreeSet<char> = ['A', '漢', '\u{E001}', '😀'].into_iter().collect();
        let codes = codes(&chars).unwrap();
        assert_eq!(codes[&'A'], 0x41);
        assert_eq!(codes[&'漢'], 0x6F22);
        let private: BTreeSet<u16> = [codes[&'\u{E001}'], codes[&'😀']].into();
        assert_eq!(private, [0xF8FE, 0xF8FF].into());
        let cmap = String::from_utf8(to_unicode(&codes)).unwrap();
        assert!(cmap.contains("<6F22> <6F22>"), "{cmap}");
        assert!(cmap.contains("<D83DDE00>"), "{cmap}");
    }

    #[test]
    fn numbers_lose_trailing_zeros() {
        assert_eq!(num(1.5, 3), "1.5");
        assert_eq!(num(2.0, 2), "2");
        assert_eq!(num(-0.0001, 2), "0");
        assert_eq!(num(100.0, 3), "100");
    }

    #[test]
    fn a_layer_moves_along_a_line_with_td() {
        let chars = vec![
            (
                "a",
                [[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]],
            ),
            (
                "b",
                [[20.0, 10.0], [30.0, 10.0], [30.0, 20.0], [20.0, 20.0]],
            ),
        ];
        let page = page(vec![(Orientation::Horizontal, chars)]);
        let placed = page_chars(&page);
        let codes = codes(&placed.iter().map(|c| c.0).collect()).unwrap();
        let names = FontNames {
            horizontal: "OcrH".into(),
            vertical: "OcrV".into(),
        };
        let layer = String::from_utf8(content(&placed, &frame(0), &codes, &names)).unwrap();
        assert_eq!(
            layer,
            "BT\n3 Tr\n/OcrH 10 Tf 1 0 0 1 20 101.2 Tm <0061> Tj\n10 0 Td <0062> Tj\nET\n"
        );
    }

    #[test]
    fn the_font_program_has_two_glyphs_and_a_valid_checksum() {
        let font = glyphless_ttf();
        assert_eq!(checksum(&font), 0xB1B0_AFBA);
        assert!(font.len() < 2048, "{}", font.len());
    }
}
