#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let Ok(set) = pdfconv_core::parse_page_range(text) else { return };
    for page_count in [0u32, 1, 7, 100] {
        let pages = set.pages_within(page_count);
        assert_eq!(pages.len() as u32, set.count_within(page_count), "{text:?} {page_count}");
        assert!(pages.windows(2).all(|w| w[0] < w[1]), "not sorted/unique: {text:?}");
        assert!(pages.iter().all(|&p| p >= 1 && p <= page_count), "out of range: {text:?}");
    }
});
