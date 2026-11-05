mod pages;
mod typeset;
mod types;

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => pages::generate(&root(), &PathBuf::from(&args[2])),
        _ => eprintln!("usage: textlayer-spike gen <ocr-spike dir> | ..."),
    }
}
