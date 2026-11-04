//! Output names are unique (case-insensitively), never an existing name, and keep the input order.
#![no_main]
use std::collections::HashSet;

use libfuzzer_sys::fuzz_target;
use pdfconv_core::resolve_image_to_pdf_names;

fuzz_target!(|input: (Vec<String>, Vec<String>)| {
    let (mut inputs, existing) = input;
    inputs.truncate(64);
    let existing: HashSet<String> = existing.into_iter().take(64).collect();
    let names = resolve_image_to_pdf_names(&inputs, &existing);
    assert_eq!(names.len(), inputs.len());
    let existing_lower: HashSet<String> = existing.iter().map(|s| s.to_lowercase()).collect();
    let mut seen = HashSet::new();
    for name in &names {
        let lower = name.to_lowercase();
        assert!(!existing_lower.contains(&lower), "{name:?} is an existing name");
        assert!(seen.insert(lower), "{name:?} given twice");
    }
});
