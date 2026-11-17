//! The reading order of a page's lines (design ocr §4, 読み順), written for
//! Japanese pages that mix vertical and horizontal text.
//!
//! 1. **Blocks**: two lines of the same orientation and a similar thickness
//!    (column width or line height within [`SAME_SIZE_RATIO`]) join a block
//!    when they sit side by side across the reading direction (a gap of at
//!    most [`BLOCK_GAP`] × the thickness, overlapping along it by at least
//!    [`MIN_OVERLAP`] of the shorter one), or follow each other along it (the
//!    same column or row and a gap of at most [`BLOCK_GAP`] × the thickness).
//!    A heading in a larger size or a tier set off by a wider gap starts a new
//!    block.
//! 2. **Inside a block**: vertical lines form columns read right to left, each
//!    top to bottom; horizontal lines form rows read top to bottom, each left
//!    to right.
//! 3. **Between blocks**: blocks that overlap vertically form a band; bands
//!    are read top to bottom. A band of mostly vertical blocks is read right
//!    to left (on equal right edges, as with tiers under one tall title, the
//!    upper first). A band of mostly horizontal blocks is read by rows across
//!    its blocks, so that a form's column of keys is not read before its
//!    values; but when every horizontal block of more than one line in it is
//!    a column of text (see [`is_text_column`]), as on a page set in two
//!    columns, each block is read whole and columns side by side left to
//!    right.

use std::collections::BTreeMap;

/// Two lines join a block only when the thicker is at most this many times
/// the thinner.
const SAME_SIZE_RATIO: f64 = 1.3;

/// The largest gap between two lines of a block, in line thicknesses.
const BLOCK_GAP: f64 = 1.0;

/// Lines side by side, and blocks in one band, overlap by at least this part
/// of the shorter one.
const MIN_OVERLAP: f64 = 0.3;

/// A line of a column of text, its last line apart, is at least this many
/// times as long as it is thick.
const COLUMN_MIN_LINE_LENGTH: f64 = 8.0;

/// A line of a column of text, its last line apart, is at least this part of
/// the column's longest line.
const COLUMN_MIN_FILL: f64 = 0.85;

/// A line's box `[x0, y0, x1, y1]` and whether it is vertical.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Item {
    pub bbox: [f64; 4],
    pub vertical: bool,
}

/// Lines read together.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Block {
    pub vertical: bool,
    /// Indices into the items, in reading order.
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

fn union_box(boxes: impl Iterator<Item = [f64; 4]>) -> [f64; 4] {
    boxes.fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |u, b| {
        [
            u[0].min(b[0]),
            u[1].min(b[1]),
            u[2].max(b[2]),
            u[3].max(b[3]),
        ]
    })
}

struct DisjointSets(Vec<usize>);

impl DisjointSets {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut i = i;
        while self.0[i] != root {
            let next = self.0[i];
            self.0[i] = root;
            i = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a] = b;
        }
    }

    /// The sets, each in increasing order, ordered by their smallest member.
    fn groups(&mut self) -> Vec<Vec<usize>> {
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..self.0.len() {
            let root = self.find(i);
            groups.entry(root).or_default().push(i);
        }
        let mut groups: Vec<Vec<usize>> = groups.into_values().collect();
        groups.sort_by_key(|g| g[0]);
        groups
    }
}

/// The blocks of `items` in reading order.
pub(crate) fn reading_order(items: &[Item]) -> Vec<Block> {
    let mut blocks = blocks(items);

    let mut bands = DisjointSets::new(blocks.len());
    for i in 0..blocks.len() {
        for j in i + 1..blocks.len() {
            let (a, b) = (&blocks[i].bbox, &blocks[j].bbox);
            if overlap(a[1], a[3], b[1], b[3]) >= MIN_OVERLAP * h(a).min(h(b)) {
                bands.union(i, j);
            }
        }
    }
    let mut bands = bands.groups();
    let top = |band: &Vec<usize>, blocks: &[Block]| {
        band.iter()
            .map(|&i| blocks[i].bbox[1])
            .fold(f64::MAX, f64::min)
    };
    bands.sort_by(|a, b| top(a, &blocks).total_cmp(&top(b, &blocks)));

    let mut taken: Vec<Option<Block>> = blocks.drain(..).map(Some).collect();
    let mut out = Vec::new();
    for band in bands {
        let band: Vec<Block> = band.iter().filter_map(|&i| taken[i].take()).collect();
        let vertical = band.iter().filter(|b| b.vertical).count();
        if vertical * 2 > band.len() {
            out.extend(vertical_band(band));
        } else {
            out.extend(horizontal_band(items, band));
        }
    }
    out
}

/// Groups `items` into blocks (rule 1), each in reading order (rule 2).
fn blocks(items: &[Item]) -> Vec<Block> {
    let thick = |i: usize| {
        if items[i].vertical {
            w(&items[i].bbox)
        } else {
            h(&items[i].bbox)
        }
    };
    let mut sets = DisjointSets::new(items.len());
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            if items[i].vertical != items[j].vertical {
                continue;
            }
            let (ti, tj) = (thick(i), thick(j));
            if ti.max(tj) > SAME_SIZE_RATIO * ti.min(tj) {
                continue;
            }
            let t = ti.max(tj);
            let (a, b) = (&items[i].bbox, &items[j].bbox);
            // (across, along): for vertical text across is x and along is y.
            let (ac, al) = if items[i].vertical {
                ((0, 2), (1, 3))
            } else {
                ((1, 3), (0, 2))
            };
            let along_overlap = overlap(a[al.0], a[al.1], b[al.0], b[al.1]);
            let along_shorter = (a[al.1] - a[al.0]).min(b[al.1] - b[al.0]);
            let side_by_side = gap(a[ac.0], a[ac.1], b[ac.0], b[ac.1]) <= BLOCK_GAP * t
                && along_overlap >= MIN_OVERLAP * along_shorter;
            let in_sequence = overlap(a[ac.0], a[ac.1], b[ac.0], b[ac.1]) >= 0.5 * ti.min(tj)
                && gap(a[al.0], a[al.1], b[al.0], b[al.1]) <= BLOCK_GAP * t;
            if side_by_side || in_sequence {
                sets.union(i, j);
            }
        }
    }
    sets.groups()
        .into_iter()
        .map(|group| {
            let vertical = items[group[0]].vertical;
            Block {
                vertical,
                bbox: union_box(group.iter().map(|&i| items[i].bbox)),
                lines: lanes(items, group, vertical)
                    .into_iter()
                    .flatten()
                    .collect(),
            }
        })
        .collect()
}

/// A band of mostly vertical blocks, right to left; right edges within half
/// the narrowest block count as equal, and then the upper block comes first.
fn vertical_band(mut band: Vec<Block>) -> Vec<Block> {
    let tolerance = 0.5 * band.iter().map(|b| w(&b.bbox)).fold(f64::MAX, f64::min);
    band.sort_by(|a, b| {
        if (a.bbox[2] - b.bbox[2]).abs() <= tolerance {
            a.bbox[1].total_cmp(&b.bbox[1])
        } else {
            b.bbox[2].total_cmp(&a.bbox[2])
        }
    });
    band
}

/// Whether a horizontal block is a column of text rather than part of a form
/// or a table: it has two lines or more, and every line but the last is long
/// for its height and nearly as long as the longest line.
fn is_text_column(items: &[Item], block: &Block) -> bool {
    let Some((_, body)) = block.lines.split_last() else {
        return false;
    };
    if block.vertical || body.is_empty() {
        return false;
    }
    let longest = block
        .lines
        .iter()
        .map(|&i| w(&items[i].bbox))
        .fold(0.0, f64::max);
    body.iter().all(|&i| {
        let b = &items[i].bbox;
        w(b) >= COLUMN_MIN_LINE_LENGTH * h(b) && w(b) >= COLUMN_MIN_FILL * longest
    })
}

/// A band of mostly horizontal blocks (rule 3), each unit placed by its top:
/// a vertical block, a row of lines across the other blocks, or, when the
/// band is set in columns, a block or a group of columns side by side, read
/// left to right.
fn horizontal_band(items: &[Item], band: Vec<Block>) -> Vec<Block> {
    let multi_line = |b: &Block| !b.vertical && b.lines.len() > 1;
    let in_columns = band.iter().any(multi_line)
        && band
            .iter()
            .filter(|b| multi_line(b))
            .all(|b| is_text_column(items, b));
    let mut units: Vec<(f64, Vec<Block>)> = Vec::new();
    let mut columns: Vec<Block> = Vec::new();
    let mut row_lines: Vec<usize> = Vec::new();
    // The block each line of `row_lines` came from.
    let mut origin: BTreeMap<usize, usize> = BTreeMap::new();
    for (index, block) in band.into_iter().enumerate() {
        if block.vertical {
            units.push((block.bbox[1], vec![block]));
        } else if !in_columns {
            origin.extend(block.lines.iter().map(|&line| (line, index)));
            row_lines.extend(block.lines);
        } else if multi_line(&block) {
            columns.push(block);
        } else {
            units.push((block.bbox[1], vec![block]));
        }
    }
    // A run of rows that are each one line of the same block, as the lines of
    // a paragraph are, stays one block.
    let mut rows: Vec<(Option<usize>, Block)> = Vec::new();
    for row in lanes(items, row_lines, false) {
        let from = match row.as_slice() {
            [line] => origin.get(line).copied(),
            _ => None,
        };
        let bbox = union_box(row.iter().map(|&i| items[i].bbox));
        match rows.last_mut() {
            Some((Some(previous), block)) if from == Some(*previous) => {
                block.lines.extend(row);
                block.bbox = union_box([block.bbox, bbox].into_iter());
            }
            _ => rows.push((
                from,
                Block {
                    vertical: false,
                    lines: row,
                    bbox,
                },
            )),
        }
    }
    units.extend(
        rows.into_iter()
            .map(|(_, block)| (block.bbox[1], vec![block])),
    );
    let mut side_by_side = DisjointSets::new(columns.len());
    for i in 0..columns.len() {
        for j in i + 1..columns.len() {
            let (a, b) = (&columns[i].bbox, &columns[j].bbox);
            if overlap(a[1], a[3], b[1], b[3]) >= MIN_OVERLAP * h(a).min(h(b)) {
                side_by_side.union(i, j);
            }
        }
    }
    let mut taken: Vec<Option<Block>> = columns.into_iter().map(Some).collect();
    for group in side_by_side.groups() {
        let mut group: Vec<Block> = group.iter().filter_map(|&i| taken[i].take()).collect();
        group.sort_by(|a, b| a.bbox[0].total_cmp(&b.bbox[0]));
        let top = group.iter().map(|b| b.bbox[1]).fold(f64::MAX, f64::min);
        units.push((top, group));
    }
    units.sort_by(|a, b| a.0.total_cmp(&b.0));
    units.into_iter().flat_map(|(_, blocks)| blocks).collect()
}

/// Columns (vertical, right to left) or rows (horizontal, top to bottom),
/// each in reading order.
fn lanes(items: &[Item], mut group: Vec<usize>, vertical: bool) -> Vec<Vec<usize>> {
    let (ac, al) = if vertical {
        ((0, 2), (1, 3))
    } else {
        ((1, 3), (0, 2))
    };
    let centre = |i: usize| (items[i].bbox[ac.0] + items[i].bbox[ac.1]) / 2.0;
    group.sort_by(|&a, &b| centre(a).total_cmp(&centre(b)));
    let mut lanes: Vec<Vec<usize>> = Vec::new();
    for i in group {
        let b = items[i].bbox;
        let fits = lanes.last().is_some_and(|lane| {
            lane.iter().any(|&j| {
                let c = items[j].bbox;
                let thinner = (b[ac.1] - b[ac.0]).min(c[ac.1] - c[ac.0]);
                overlap(b[ac.0], b[ac.1], c[ac.0], c[ac.1]) >= 0.5 * thinner
            })
        });
        match lanes.last_mut() {
            Some(lane) if fits => lane.push(i),
            _ => lanes.push(vec![i]),
        }
    }
    if vertical {
        lanes.reverse();
    }
    for lane in &mut lanes {
        lane.sort_by(|&a, &b| items[a].bbox[al.0].total_cmp(&items[b].bbox[al.0]));
    }
    lanes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn horizontal(x0: f64, y0: f64, x1: f64, y1: f64) -> Item {
        Item {
            bbox: [x0, y0, x1, y1],
            vertical: false,
        }
    }

    fn vertical(x0: f64, y0: f64, x1: f64, y1: f64) -> Item {
        Item {
            bbox: [x0, y0, x1, y1],
            vertical: true,
        }
    }

    fn order(items: &[Item]) -> Vec<Vec<usize>> {
        reading_order(items).into_iter().map(|b| b.lines).collect()
    }

    #[test]
    fn vertical_columns_go_right_to_left_and_a_heading_first() {
        let items = [
            // Body columns, 20 wide with 10 between them.
            vertical(100.0, 100.0, 120.0, 500.0),
            vertical(130.0, 100.0, 150.0, 500.0),
            vertical(160.0, 100.0, 180.0, 400.0),
            // A heading column, 40 wide, to the right.
            vertical(220.0, 100.0, 260.0, 300.0),
        ];
        assert_eq!(order(&items), [vec![3], vec![2, 1, 0]]);
    }

    #[test]
    fn two_tiers_of_vertical_text_are_read_upper_tier_first() {
        let items = [
            vertical(100.0, 500.0, 120.0, 800.0),
            vertical(130.0, 500.0, 150.0, 800.0),
            vertical(100.0, 100.0, 120.0, 400.0),
            vertical(130.0, 100.0, 150.0, 400.0),
        ];
        assert_eq!(order(&items), [vec![3, 2], vec![1, 0]]);
    }

    #[test]
    fn two_columns_of_horizontal_text_are_read_column_by_column() {
        let mut items = Vec::new();
        for column in 0..2 {
            let x0 = 50.0 + column as f64 * 300.0;
            for line in 0..4 {
                let y0 = 100.0 + line as f64 * 30.0;
                let x1 = if line == 3 { x0 + 120.0 } else { x0 + 250.0 };
                items.push(horizontal(x0, y0, x1, y0 + 20.0));
            }
        }
        assert_eq!(order(&items), [vec![0, 1, 2, 3], vec![4, 5, 6, 7]]);
    }

    #[test]
    fn a_form_is_read_key_then_value() {
        // Keys at x 50–110, values at 200–400, rows 30 apart.
        let mut items = Vec::new();
        for row in 0..3 {
            let y0 = 100.0 + row as f64 * 30.0;
            items.push(horizontal(50.0, y0, 110.0, y0 + 20.0));
        }
        for row in 0..3 {
            let y0 = 100.0 + row as f64 * 30.0;
            items.push(horizontal(200.0, y0, 400.0, y0 + 20.0));
        }
        assert_eq!(order(&items), [vec![0, 3], vec![1, 4], vec![2, 5]]);
    }

    #[test]
    fn the_lines_of_a_paragraph_in_a_form_stay_one_block() {
        // A paragraph of uneven lines, then a key and its value.
        let items = [
            horizontal(50.0, 100.0, 400.0, 120.0),
            horizontal(50.0, 130.0, 250.0, 150.0),
            horizontal(50.0, 160.0, 380.0, 180.0),
            horizontal(50.0, 190.0, 110.0, 210.0),
            horizontal(200.0, 190.0, 400.0, 210.0),
        ];
        assert_eq!(order(&items), [vec![0, 1, 2], vec![3, 4]]);
    }

    #[test]
    fn short_lines_are_not_a_column() {
        let items = [
            horizontal(0.0, 0.0, 100.0, 20.0),
            horizontal(0.0, 30.0, 100.0, 50.0),
        ];
        let block = Block {
            vertical: false,
            lines: vec![0, 1],
            bbox: [0.0, 0.0, 100.0, 50.0],
        };
        assert!(!is_text_column(&items, &block));
    }
}
