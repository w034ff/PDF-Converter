//! Renders an SVG to a square PNG: icon-render <in.svg> <out.png> <size>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let size: u32 = a[3].parse().unwrap();
    let tree =
        resvg::usvg::Tree::from_data(&std::fs::read(&a[1]).unwrap(), &Default::default()).unwrap();
    let mut pix = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
    let s = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(s, s),
        &mut pix.as_mut(),
    );
    pix.save_png(&a[2]).unwrap();
}
