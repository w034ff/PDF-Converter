//! Strings that `organize_meta.pdf` carries and an organized output must not (design §11.1).

#![allow(
    dead_code,
    reason = "constants are shared across test files where each file uses a subset"
)]

/// `/Author` of `organize_meta.pdf`.
pub const META_AUTHOR: &str = "fixture-author-7f3a";
/// `/Title` of `organize_meta.pdf`.
pub const META_TITLE: &str = "fixture-title-2c9d";
/// `/Creator` of `organize_meta.pdf`.
pub const META_CREATOR: &str = "fixture-creator-4b6e";
/// `/Producer` of `organize_meta.pdf`.
pub const META_PRODUCER: &str = "fixture-producer-81be";
/// The digits of `/CreationDate` of `organize_meta.pdf`.
pub const META_CREATION_DIGITS: &str = "20190807060504";
/// Text inside the XMP packet of `organize_meta.pdf`.
pub const META_XMP: &str = "fixture-xmp-5d1e";

/// Pages of `organize_links.pdf`.
pub const LINKS_PAGES: u32 = 4;
/// The title of a bookmark of `organize_links.pdf` that is outside ASCII.
pub const LINKS_NON_ASCII_TITLE: &str = "第 3 節";
/// The URL of the link on page 1 of `organize_links.pdf`.
pub const LINKS_PAGE_URL: &str = "https://example.com/p1";
/// The URL of the "Website" bookmark of `organize_links.pdf`.
pub const LINKS_BOOKMARK_URL: &str = "https://example.com/site";
