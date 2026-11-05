//! Item 4: the path for an existing PDF. Adds an invisible text layer
//! (`3 Tr`) to the pages of a PDF with lopdf 0.46.0: one more content stream
//! per page and Type0 fonts built here by hand.
//!
//! Fonts (one `CIDFont`, two `Type0` fonts over it: `Identity-H` for
//! horizontal runs, `Identity-V` for vertical ones):
//! - `Real`: Noto Sans JP subset with `subsetter`; the CFF outlines are
//!   embedded as `CIDFontType0` / `FontFile3` (`CIDFontType0C`). CID = the
//!   glyph id in the subset; `W` from the font's advances.
//! - `Glyphless` (Tesseract's and OCRmyPDF's way): a two-glyph TrueType font
//!   (`CIDFontType2` / `FontFile2`), CID = the character's code (UCS-2),
//!   `CIDToGIDMap` sends every CID to glyph 1, `DW 1000`. The font is the
//!   same whatever the text; only `ToUnicode` grows with the characters.
//!   `GlyphlessEmpty` is the same with an empty glyph 1 (no outline).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::fonts::{ASCENT, DESCENT, RealFont, glyphless_ttf_with};
use crate::layout::Run;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LFont {
    Real,
    Glyphless,
    GlyphlessEmpty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Positioning {
    /// One `Tj` per run (with `Tz`/`Tc` fitting the run's length).
    Plain,
    /// `TJ` with an adjustment before each glyph to put it at `Run::starts`.
    Tj,
}

/// The fonts added to the document and how a character is encoded.
struct Fonts {
    horizontal: ObjectId,
    vertical: ObjectId,
    /// Character → (2-byte code, advance in 1000ths of the em).
    code: BTreeMap<char, (u16, f64)>,
}

fn to_unicode(map: &BTreeMap<u16, char>) -> Vec<u8> {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<_> = map.iter().collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (code, c) in chunk {
            let mut buf = [0u16; 2];
            let hex: String = c.encode_utf16(&mut buf).iter().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{code:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

fn stream(doc: &mut Document, dict: Dictionary, data: Vec<u8>) -> ObjectId {
    let mut st = Stream::new(dict, data);
    let _ = st.compress();
    doc.add_object(st)
}

fn add_fonts(doc: &mut Document, chars: &BTreeSet<char>, kind: LFont, real: &RealFont) -> Fonts {
    let mut code = BTreeMap::new();
    let mut unicode = BTreeMap::new();
    let descriptor = |name: &str, file_key: &str, file: ObjectId| {
        dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(name.as_bytes().to_vec()),
            "Flags" => 4,
            "FontBBox" => vec![0.into(), (DESCENT as i64).into(), 1000.into(), (ASCENT as i64).into()],
            "ItalicAngle" => 0,
            "Ascent" => ASCENT as i64,
            "Descent" => DESCENT as i64,
            "CapHeight" => 700,
            "StemV" => 80,
            file_key => file,
        }
    };
    let (cid_font, base) = match kind {
        LFont::Real => {
            let mut remapper = subsetter::GlyphRemapper::new();
            let mut by_gid: BTreeMap<u16, char> = BTreeMap::new();
            for &c in chars {
                let g = real.gid(c);
                remapper.remap(g);
                by_gid.entry(g).or_insert(c);
            }
            let sub = subsetter::subset(&real.data, 0, &remapper).unwrap();
            let sub_ref = skrifa::FontRef::new(&sub).unwrap();
            use skrifa::raw::TableProvider;
            let cff = sub_ref.table_data(skrifa::Tag::new(b"CFF ")).map(|d| d.as_bytes().to_vec());
            let mut widths = Vec::new();
            for &c in chars {
                let g = real.gid(c);
                let cid = remapper.get(g).unwrap();
                let adv = real.advance(g);
                code.insert(c, (cid, adv));
                if by_gid[&g] == c {
                    unicode.insert(cid, c);
                }
            }
            let mut cids: Vec<(u16, f64)> = code.values().copied().collect::<BTreeSet<_>>().into_iter().map(|(a, b)| (a, b)).collect();
            cids.sort_by_key(|x| x.0);
            cids.dedup_by_key(|x| x.0);
            for (cid, adv) in cids {
                widths.push(Object::Integer(cid as i64));
                widths.push(Object::Array(vec![Object::Real(adv as f32)]));
            }
            let name = "AAAAAA+NotoSansJP-Regular";
            let (subtype, file_key, file) = match cff {
                Some(cff) => ("CIDFontType0", "FontFile3", stream(doc, dictionary! { "Subtype" => "CIDFontType0C" }, cff)),
                None => ("CIDFontType2", "FontFile2", stream(doc, dictionary! {}, sub.clone())),
            };
            let fd = doc.add_object(descriptor(name, file_key, file));
            let mut d = dictionary! {
                "Type" => "Font",
                "Subtype" => subtype,
                "BaseFont" => Object::Name(name.as_bytes().to_vec()),
                "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
                "FontDescriptor" => fd,
                "DW" => 1000,
                "W" => widths,
                "DW2" => vec![(ASCENT as i64).into(), (-1000).into()],
            };
            if subtype == "CIDFontType2" {
                d.set("CIDToGIDMap", "Identity");
            }
            (doc.add_object(d), name)
        }
        LFont::Glyphless | LFont::GlyphlessEmpty => {
            // CID = the UCS-2 code; characters outside the BMP (and in the
            // private use area) get private-use codes from U+F0000 down.
            let mut next_private = 0xF8FFu16;
            for &c in chars {
                let cp = c as u32;
                let cid = if cp <= 0xFFFF && !(0xE000..=0xF8FF).contains(&cp) {
                    cp as u16
                } else {
                    next_private -= 1;
                    next_private
                };
                code.insert(c, (cid, 1000.0));
                unicode.insert(cid, c);
            }
            let ttf = glyphless_ttf_with(&[], kind == LFont::Glyphless);
            let file = stream(doc, dictionary! { "Length1" => ttf.len() as i64 }, ttf);
            let name = "GlyphLessFont";
            let fd = doc.add_object(descriptor(name, "FontFile2", file));
            let mut map = vec![0u8; 65536 * 2];
            for cid in 1..65536usize {
                map[cid * 2 + 1] = 1;
            }
            let map = stream(doc, dictionary! {}, map);
            let d = dictionary! {
                "Type" => "Font",
                "Subtype" => "CIDFontType2",
                "BaseFont" => name,
                "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
                "FontDescriptor" => fd,
                "DW" => 1000,
                "DW2" => vec![(ASCENT as i64).into(), (-1000).into()],
                "CIDToGIDMap" => map,
            };
            (doc.add_object(d), name)
        }
    };
    let tu = stream(doc, dictionary! {}, to_unicode(&unicode));
    let type0 = |doc: &mut Document, enc: &str| {
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => Object::Name(format!("{base}-{enc}").into_bytes()),
            "Encoding" => Object::Name(enc.as_bytes().to_vec()),
            "DescendantFonts" => vec![cid_font.into()],
            "ToUnicode" => tu,
        })
    };
    let horizontal = type0(doc, "Identity-H");
    let vertical = type0(doc, "Identity-V");
    Fonts { horizontal, vertical, code }
}

fn f(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// The content stream of one page's runs, in PDF space (y up) on a page `h` high.
fn content(runs: &[Run], h: f64, fonts: &Fonts, pos: Positioning) -> Vec<u8> {
    let mut s = String::from("BT\n3 Tr\n");
    for r in runs {
        let codes: Vec<(u16, f64)> = r.text.iter().map(|c| fonts.code[c]).collect();
        let (ox, oy) = (r.origin[0], h - r.origin[1]);
        let a = -r.angle;
        let (c, sn) = (a.cos(), a.sin());
        let hex = |cs: &[(u16, f64)]| cs.iter().map(|(c, _)| format!("{c:04X}")).collect::<String>();
        if r.vertical {
            let n = codes.len().max(1) as f64;
            let tc = r.size - r.length / n;
            let _ = writeln!(s, "/TLV {} Tf 100 Tz {} Tc {} {} {} {} {} {} Tm <{}> Tj", f(r.size), f(tc), f(c), f(sn), f(-sn), f(c), f(ox), f(oy), hex(&codes));
        } else {
            let natural: f64 = codes.iter().map(|(_, w)| w / 1000.0 * r.size).sum();
            let th = if natural > 0.0 { r.length / natural } else { 1.0 };
            let _ = write!(s, "/TLH {} Tf {} Tz 0 Tc {} {} {} {} {} {} Tm ", f(r.size), f(th * 100.0), f(c), f(sn), f(-sn), f(c), f(ox), f(oy));
            match (&r.starts, pos) {
                (Some(starts), Positioning::Tj) => {
                    // Pen after glyph k (in text space, before Tz) should be at starts[k + 1] / th.
                    s.push('[');
                    let mut pen = 0.0;
                    for (k, (code, w)) in codes.iter().enumerate() {
                        let want = starts[k] / th;
                        let adj = (pen - want) / r.size * 1000.0;
                        if adj.abs() > 0.5 {
                            let _ = write!(s, "{} ", f(adj));
                        }
                        let _ = write!(s, "<{code:04X}>");
                        pen = want + w / 1000.0 * r.size;
                    }
                    s.push_str("] TJ\n");
                }
                _ => {
                    let _ = writeln!(s, "<{}> Tj", hex(&codes));
                }
            }
        }
    }
    s.push_str("ET\n");
    s.into_bytes()
}

/// Adds the runs of each page (by page index) to `pdf`.
pub fn add_text_layer(pdf: &[u8], pages: &[Vec<Run>], kind: LFont, pos: Positioning, real: &RealFont) -> Document {
    let mut doc = Document::load_mem(pdf).unwrap();
    let chars: BTreeSet<char> = pages.iter().flat_map(|p| p.iter().flat_map(|r| r.text.iter().copied())).collect();
    let fonts = add_fonts(&mut doc, &chars, kind, real);
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for (runs, &page_id) in pages.iter().zip(&page_ids) {
        if runs.is_empty() {
            continue;
        }
        let h = page_height(&doc, page_id);
        // Keep the page's own graphics state away from the text: q … Q around the old content.
        let page = doc.get_dictionary(page_id).unwrap();
        let mut list: Vec<Object> = match page.get(b"Contents") {
            Ok(Object::Reference(id)) => vec![Object::Reference(*id)],
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![],
        };
        let open = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
        let mut body = b"Q\n".to_vec();
        body.extend(content(runs, h, &fonts, pos));
        let close = stream(&mut doc, dictionary! {}, body);
        list.insert(0, open.into());
        list.push(close.into());
        doc.get_dictionary_mut(page_id).unwrap().set("Contents", list);

        let res = doc.get_or_create_resources(page_id).unwrap();
        let res = match res {
            Object::Dictionary(d) => d,
            _ => panic!("resources"),
        };
        let mut font_dict = match res.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        font_dict.set("TLH", fonts.horizontal);
        font_dict.set("TLV", fonts.vertical);
        // A Font entry that is a reference would need resolving; krilla writes none for an image page.
        let res = match doc.get_or_create_resources(page_id).unwrap() {
            Object::Dictionary(d) => d,
            _ => unreachable!(),
        };
        res.set("Font", font_dict);
    }
    doc
}

fn page_height(doc: &Document, page_id: ObjectId) -> f64 {
    let page = doc.get_dictionary(page_id).unwrap();
    let mb = page.get(b"MediaBox").unwrap().as_array().unwrap();
    let n = |o: &Object| o.as_float().map(|v| v as f64).or_else(|_| o.as_i64().map(|v| v as f64)).unwrap();
    n(&mb[3]) - n(&mb[1])
}

pub fn save(mut doc: Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}
