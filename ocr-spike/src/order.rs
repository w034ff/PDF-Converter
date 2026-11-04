//! Item 5: reading order for lines from the detector.
//!
//! Rules (written for Japanese pages that mix vertical and horizontal text):
//!
//! 1. **Orientation**: a line is vertical when its crop is at least 1.5 times
//!    taller than wide (the rule the recogniser already uses), horizontal when
//!    at least 1.5 times wider than tall. Lines in between (one or two
//!    characters) take the page's majority orientation.
//! 2. **Blocks**: two lines of the same orientation and similar thickness
//!    (column width or line height within [`SAME_SIZE_RATIO`]) join a block when
//!    they sit side by side across the reading direction (gap at most
//!    [`BLOCK_GAP`] × thickness and overlapping by at least [`MIN_OVERLAP`] of
//!    the shorter one), or follow each other along it (same column / row and a
//!    gap at most [`BLOCK_GAP`] × thickness). A heading in a larger size or a
//!    tier (段) separated by a wider gap starts a new block.
//! 3. **Inside a block**: vertical lines form columns (pieces overlapping
//!    horizontally by half their width), read right to left, each top to
//!    bottom; horizontal lines form rows read top to bottom, each left to right.
//! 4. **Between blocks**: blocks whose vertical extents overlap form a band;
//!    bands are read top to bottom. Inside a band, a band of mostly vertical
//!    blocks is read right to left (ties on the right edge, as with tiers under
//!    one tall title, go top first), otherwise left to right.

pub const VERTICAL_RATIO: f64 = 1.5;
pub const SAME_SIZE_RATIO: f64 = 1.3;
pub const BLOCK_GAP: f64 = 1.0;
pub const MIN_OVERLAP: f64 = 0.3;

#[derive(Clone, Copy, Debug)]
pub struct Item {
    /// `[x0, y0, x1, y1]`
    pub bbox: [f64; 4],
}

#[derive(Clone, Debug)]
pub struct Block {
    pub vertical: bool,
    /// Indices into the input, in reading order.
    pub lines: Vec<usize>,
    pub bbox: [f64; 4],
}

fn w(b: &[f64; 4]) -> f64 {
    b[2] - b[0]
}
fn h(b: &[f64; 4]) -> f64 {
    b[3] - b[1]
}
fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}
fn gap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a0.max(b0) - a1.min(b1)).max(0.0)
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn find(&mut self, i: usize) -> usize {
        if self.0[i] != i {
            let r = self.find(self.0[i]);
            self.0[i] = r;
        }
        self.0[i]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a] = b;
        }
    }
}

pub fn orientations(items: &[Item]) -> Vec<bool> {
    let decided: Vec<Option<bool>> = items
        .iter()
        .map(|i| {
            let r = h(&i.bbox) / w(&i.bbox).max(1e-9);
            if r >= VERTICAL_RATIO {
                Some(true)
            } else if r <= 1.0 / VERTICAL_RATIO {
                Some(false)
            } else {
                None
            }
        })
        .collect();
    let v = decided.iter().filter(|d| **d == Some(true)).count();
    let hz = decided.iter().filter(|d| **d == Some(false)).count();
    decided.iter().map(|d| d.unwrap_or(v > hz)).collect()
}

pub fn reading_order(items: &[Item]) -> Vec<Block> {
    let n = items.len();
    let vert = orientations(items);
    let thick = |i: usize| if vert[i] { w(&items[i].bbox) } else { h(&items[i].bbox) };
    let mut dsu = Dsu((0..n).collect());
    for i in 0..n {
        for j in i + 1..n {
            if vert[i] != vert[j] {
                continue;
            }
            let (ti, tj) = (thick(i), thick(j));
            if ti.max(tj) > SAME_SIZE_RATIO * ti.min(tj) {
                continue;
            }
            let t = ti.max(tj);
            let (a, b) = (&items[i].bbox, &items[j].bbox);
            // (across, along) axes: for vertical text across is x, along is y.
            let (ac, al) = if vert[i] { ((0, 2), (1, 3)) } else { ((1, 3), (0, 2)) };
            let along_ov = overlap(a[al.0], a[al.1], b[al.0], b[al.1]);
            let along_min = (a[al.1] - a[al.0]).min(b[al.1] - b[al.0]);
            let across_gap = gap(a[ac.0], a[ac.1], b[ac.0], b[ac.1]);
            let across_ov = overlap(a[ac.0], a[ac.1], b[ac.0], b[ac.1]);
            let side_by_side = across_gap <= BLOCK_GAP * t && along_ov >= MIN_OVERLAP * along_min;
            let along_gap = gap(a[al.0], a[al.1], b[al.0], b[al.1]);
            let in_sequence = across_ov >= 0.5 * ti.min(tj) && along_gap <= BLOCK_GAP * t;
            if side_by_side || in_sequence {
                dsu.union(i, j);
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..n {
        groups.entry(dsu.find(i)).or_default().push(i);
    }
    let mut blocks: Vec<Block> = groups
        .into_values()
        .map(|g| {
            let vertical = vert[g[0]];
            let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
            for &i in &g {
                let b = items[i].bbox;
                bbox = [bbox[0].min(b[0]), bbox[1].min(b[1]), bbox[2].max(b[2]), bbox[3].max(b[3])];
            }
            Block { vertical, lines: order_in_block(items, g, vertical), bbox }
        })
        .collect();

    // Bands of blocks that overlap vertically.
    let m = blocks.len();
    let mut bd = Dsu((0..m).collect());
    for i in 0..m {
        for j in i + 1..m {
            let (a, b) = (&blocks[i].bbox, &blocks[j].bbox);
            if overlap(a[1], a[3], b[1], b[3]) >= MIN_OVERLAP * h(a).min(h(b)) {
                bd.union(i, j);
            }
        }
    }
    let mut bands: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..m {
        bands.entry(bd.find(i)).or_default().push(i);
    }
    let mut bands: Vec<Vec<usize>> = bands.into_values().collect();
    bands.sort_by(|a, b| {
        let ta = a.iter().map(|&i| blocks[i].bbox[1]).fold(f64::MAX, f64::min);
        let tb = b.iter().map(|&i| blocks[i].bbox[1]).fold(f64::MAX, f64::min);
        ta.total_cmp(&tb)
    });
    let mut order = Vec::new();
    for mut band in bands {
        let nv = band.iter().filter(|&&i| blocks[i].vertical).count();
        if nv * 2 > band.len() {
            // Right to left; right edges within a column width count as equal.
            let tol = 0.5 * band.iter().map(|&i| w(&blocks[i].bbox)).fold(f64::MAX, f64::min);
            band.sort_by(|&a, &b| {
                let (ba, bb) = (&blocks[a].bbox, &blocks[b].bbox);
                if (ba[2] - bb[2]).abs() <= tol { ba[1].total_cmp(&bb[1]) } else { bb[2].total_cmp(&ba[2]) }
            });
        } else {
            band.sort_by(|&a, &b| {
                let (ba, bb) = (&blocks[a].bbox, &blocks[b].bbox);
                ba[0].total_cmp(&bb[0])
            });
        }
        order.extend(band);
    }
    let mut taken: Vec<Option<Block>> = blocks.drain(..).map(Some).collect();
    order.into_iter().map(|i| taken[i].take().unwrap()).collect()
}

fn order_in_block(items: &[Item], mut g: Vec<usize>, vertical: bool) -> Vec<usize> {
    // Group into columns (vertical) or rows (horizontal).
    let (ac, al) = if vertical { ((0, 2), (1, 3)) } else { ((1, 3), (0, 2)) };
    let centre = |i: usize| (items[i].bbox[ac.0] + items[i].bbox[ac.1]) / 2.0;
    g.sort_by(|&a, &b| centre(a).total_cmp(&centre(b)));
    let mut lanes: Vec<Vec<usize>> = Vec::new();
    for i in g {
        let b = items[i].bbox;
        let fits = lanes.last().is_some_and(|lane: &Vec<usize>| {
            lane.iter().any(|&j| {
                let c = items[j].bbox;
                let thin = (b[ac.1] - b[ac.0]).min(c[ac.1] - c[ac.0]);
                overlap(b[ac.0], b[ac.1], c[ac.0], c[ac.1]) >= 0.5 * thin
            })
        });
        if fits {
            lanes.last_mut().unwrap().push(i);
        } else {
            lanes.push(vec![i]);
        }
    }
    if vertical {
        lanes.reverse(); // right to left
    }
    lanes
        .into_iter()
        .flat_map(|mut lane| {
            lane.sort_by(|&a, &b| items[a].bbox[al.0].total_cmp(&items[b].bbox[al.0]));
            lane
        })
        .collect()
}
