//! Item 5: synthetic vertical-writing pages with known reading order.
//!
//! The text is public-domain prose (Akutagawa, Soseki, Kenji Miyazawa). The
//! Aozora Bunko site and its GitHub mirror are not reachable from the cloud
//! sandbox, so the passages were typed in; the ground truth is whatever is
//! drawn here, so small differences from the printed editions do not matter.

use std::collections::HashMap;
use std::path::Path;

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, point};

pub const RASHOMON: &str = "ある日の暮方の事である。一人の下人が、羅生門の下で雨やみを待っていた。広い門の下には、この男のほかに誰もいない。ただ、所々丹塗の剥げた、大きな円柱に、蟋蟀が一匹とまっている。羅生門が、朱雀大路にある以上は、この男のほかにも、雨やみをする市女笠や揉烏帽子が、もう二三人はありそうなものである。それが、この男のほかには誰もいない。";
pub const NEKO: &str = "吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。吾輩はここで始めて人間というものを見た。しかもあとで聞くとそれは書生という人間中で一番獰悪な種族であったそうだ。この書生というのは時々我々を捕えて煮て食うという話である。しかしその当時は何という考もなかったから別段恐しいとも思わなかった。ただ彼の掌に載せられてスーと持ち上げられた時何だかフワフワした感じがあったばかりである。";
pub const BOTCHAN: &str = "親譲りの無鉄砲で小供の時から損ばかりしている。小学校に居る時分学校の二階から飛び降りて一週間ほど腰を抜かした事がある。なぜそんな無闇をしたと聞く人があるかも知れぬ。別段深い理由でもない。新築の二階から首を出していたら、同級生の一人が冗談に、いくら威張っても、そこから飛び降りる事は出来まい。弱虫やーい。と囃したからである。小使に負ぶさって帰って来た時、おやじが大きな眼をして二階ぐらいから飛び降りて腰を抜かす奴があるかと云ったから、この次は抜かさずに飛んで見せますと答えた。";
pub const CHUMON: &str = "二人の若い紳士が、すっかりイギリスの兵隊のかたちをして、ぴかぴかする鉄砲をかついで、白熊のような犬を二疋つれて、だいぶ山奥の、木の葉のかさかさしたとこを、こんなことを云いながら、あるいておりました。「ぜんたい、ここらの山は怪しからんね。鳥も獣も一疋も居やがらん。なんでも構わないから、早くタンタアーンと、やって見たいもんだなあ。」「鹿の黄いろな横っ腹なんぞに、二三発お見舞もうしたら、ずいぶん痛快だろうねえ。くるくるまわって、それからどたっと倒れるだろうねえ。」";

pub const PAGE_W: u32 = 1654;
pub const PAGE_H: u32 = 2339;

/// One block of the ground truth, in reading order.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GtBlock {
    pub vertical: bool,
    pub lines: Vec<String>,
    /// Each line's `[x0, y0, x1, y1]` in page pixels.
    pub line_boxes: Vec<[f64; 4]>,
    /// `[x0, y0, x1, y1]` in page pixels.
    pub bbox: [f64; 4],
    /// Vertical item 1: each character's em box `[x0, y0, x1, y1]`, per line.
    /// On the four item-5 pages the glyphs sit lower than `line_boxes` say
    /// (they were drawn with the hhea ascent); these boxes are where the em
    /// boxes really are.
    #[serde(default)]
    pub char_boxes: Vec<Vec<[f64; 4]>>,
}

pub struct Typesetter<'a> {
    font: FontRef<'a>,
    cmap: HashMap<u32, u16>,
    vert: HashMap<u32, u16>,
    /// (hhea ascent − OS/2 typo ascender) / units per em: how far below the
    /// pen's top the em box starts when drawing with the hhea ascent.
    em_drop: f32,
    /// Vertical item 1: draw so that the em box starts at the pen's top.
    /// Off for the four item-5 pages, which must stay as they were.
    pub em_align: bool,
}

impl<'a> Typesetter<'a> {
    pub fn new(font_bytes: &'a [u8], vert_json: &Path) -> Self {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(vert_json).unwrap()).unwrap();
        let read = |k: &str| -> HashMap<u32, u16> {
            v[k].as_object()
                .unwrap()
                .iter()
                .map(|(cp, g)| (cp.parse().unwrap(), g.as_u64().unwrap() as u16))
                .collect()
        };
        let font = FontRef::try_from_slice(font_bytes).unwrap();
        let m = &v["metrics"];
        let em_drop = if m.is_object() {
            let upm = m["upm"].as_f64().unwrap() as f32;
            (font.ascent_unscaled() - m["typo_ascender"].as_f64().unwrap() as f32) / upm
        } else {
            0.18
        };
        Self { font, cmap: read("cmap"), vert: read("vert"), em_drop, em_align: false }
    }

    /// Top of the em box of a glyph drawn with its pen at `top`.
    pub fn em_top(&self, top: f32, size: f32) -> f32 {
        if self.em_align { top } else { top + self.em_drop * size }
    }

    fn glyph(&self, img: &mut image::GrayImage, gid: u16, size: f32, left: f32, top: f32) {
        let scale = PxScale::from(size);
        let ascent = self.font.as_scaled(scale).ascent();
        let ascent = if self.em_align { ascent - self.em_drop * size } else { ascent };
        let g = GlyphId(gid).with_scale_and_position(scale, point(left, top + ascent));
        if let Some(o) = self.font.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|x, y, c| {
                let (px, py) = (b.min.x as i32 + x as i32, b.min.y as i32 + y as i32);
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    let p = img.get_pixel_mut(px as u32, py as u32);
                    p.0[0] = p.0[0].min((255.0 * (1.0 - c)) as u8);
                }
            });
        }
    }

    /// Draws one column top-down with the font's vertical forms.
    pub fn column(&self, img: &mut image::GrayImage, text: &str, size: f32, left: f32, top: f32) {
        for (i, ch) in text.chars().enumerate() {
            let cp = ch as u32;
            let gid = self.vert.get(&cp).or_else(|| self.cmap.get(&cp)).copied().unwrap_or(0);
            self.glyph(img, gid, size, left, top + i as f32 * size);
        }
    }

    /// Draws one horizontal line; returns its width.
    pub fn row(&self, img: &mut image::GrayImage, text: &str, size: f32, left: f32, top: f32) -> f32 {
        let xs = self.row_advances(text, size, left);
        for (ch, x) in text.chars().zip(&xs) {
            let gid = self.cmap.get(&(ch as u32)).copied().unwrap_or(0);
            self.glyph(img, gid, size, x.0, top);
        }
        xs.last().map_or(0.0, |x| x.1 - left)
    }

    /// Each character's horizontal span on a line starting at `left`.
    pub fn row_advances(&self, text: &str, size: f32, left: f32) -> Vec<(f32, f32)> {
        let scaled = self.font.as_scaled(PxScale::from(size));
        let mut x = left;
        text.chars()
            .map(|ch| {
                let gid = self.cmap.get(&(ch as u32)).copied().unwrap_or(0);
                let start = x;
                x += scaled.h_advance(GlyphId(gid));
                (start, x)
            })
            .collect()
    }
}

fn split(text: &str, per: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(per).map(|c| c.iter().collect()).collect()
}

/// Lays `text` out as a vertical block whose right edge is at `right`, top at
/// `top`, `per` characters per column; returns the block.
pub fn vblock(ts: &Typesetter, img: &mut image::GrayImage, text: &str, size: f32, pitch: f32, right: f32, top: f32, per: usize) -> GtBlock {
    let cols = split(text, per);
    for (i, c) in cols.iter().enumerate() {
        ts.column(img, c, size, right - size - i as f32 * pitch, top);
    }
    let left = right - size - (cols.len() - 1) as f32 * pitch;
    let line_boxes = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let l = (right - size - i as f32 * pitch) as f64;
            [l, top as f64, l + size as f64, (top + c.chars().count() as f32 * size) as f64]
        })
        .collect();
    let char_boxes = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let l = (right - size - i as f32 * pitch) as f64;
            (0..c.chars().count())
                .map(|k| {
                    let t = ts.em_top(top + k as f32 * size, size) as f64;
                    [l, t, l + size as f64, t + size as f64]
                })
                .collect()
        })
        .collect();
    GtBlock {
        vertical: true,
        lines: cols,
        line_boxes,
        bbox: [left as f64, top as f64, right as f64, (top + per as f32 * size) as f64],
        char_boxes,
    }
}

pub fn hline(ts: &Typesetter, img: &mut image::GrayImage, text: &str, size: f32, left: f32, top: f32) -> GtBlock {
    let w = ts.row(img, text, size, left, top);
    let bbox = [left as f64, top as f64, (left + w) as f64, (top + size) as f64];
    let t = ts.em_top(top, size) as f64;
    let char_boxes = vec![ts.row_advances(text, size, left).iter().map(|&(a, b)| [a as f64, t, b as f64, t + size as f64]).collect()];
    GtBlock { vertical: false, lines: vec![text.into()], line_boxes: vec![bbox], bbox, char_boxes }
}

/// The four test pages: (name, image, blocks in reading order).
pub fn pages(ts: &Typesetter) -> Vec<(String, image::GrayImage, Vec<GtBlock>)> {
    let blank = || image::GrayImage::from_pixel(PAGE_W, PAGE_H, image::Luma([255]));
    let (r, top) = (PAGE_W as f32 - 140.0, 160.0);
    let mut out = Vec::new();

    // 1: a title column on the right, then one body block.
    let mut img = blank();
    let mut b = vec![vblock(ts, &mut img, "羅生門", 72.0, 100.0, r, top + 80.0, 3)];
    b.push(vblock(ts, &mut img, "芥川龍之介", 40.0, 60.0, r - 100.0, top + 1500.0, 5));
    b.push(vblock(ts, &mut img, RASHOMON, 44.0, 70.0, r - 260.0, top, 45));
    out.push(("v1_title_body".into(), img, b));

    // 2: a horizontal running head, two tiers (段組み), a page number.
    let mut img = blank();
    let mut b = vec![hline(ts, &mut img, "吾輩は猫である", 32.0, 140.0, 80.0)];
    let chars: Vec<char> = NEKO.chars().collect();
    let half = chars.len() / 2 / 22 * 22 + 22;
    let (upper, lower): (String, String) = (chars[..half].iter().collect(), chars[half..].iter().collect());
    b.push(vblock(ts, &mut img, &upper, 40.0, 64.0, r, top, 22));
    b.push(vblock(ts, &mut img, &lower, 40.0, 64.0, r, top + 22.0 * 40.0 + 160.0, 22));
    b.push(hline(ts, &mut img, "二十三", 32.0, PAGE_W as f32 / 2.0 - 48.0, PAGE_H as f32 - 120.0));
    out.push(("v2_two_tiers".into(), img, b));

    // 3: three tiers of short columns with a vertical title.
    let mut img = blank();
    let mut b = vec![vblock(ts, &mut img, "坊っちゃん", 64.0, 90.0, r, top, 5)];
    let per = 14;
    let chars: Vec<char> = BOTCHAN.chars().collect();
    let cols_per_tier = 6;
    for (t, part) in chars.chunks(per * cols_per_tier).enumerate() {
        let s: String = part.iter().collect();
        b.push(vblock(ts, &mut img, &s, 38.0, 62.0, r - 140.0, top + t as f32 * (per as f32 * 38.0 + 120.0), per));
    }
    out.push(("v3_three_tiers".into(), img, b));

    // 4: one wide block with dialogue in brackets, horizontal running head.
    let mut img = blank();
    let mut b = vec![hline(ts, &mut img, "注文の多い料理店", 30.0, PAGE_W as f32 - 140.0 - 8.0 * 30.0, 80.0)];
    b.push(vblock(ts, &mut img, CHUMON, 46.0, 76.0, r, top, 42));
    out.push(("v4_dialogue".into(), img, b));
    out
}
