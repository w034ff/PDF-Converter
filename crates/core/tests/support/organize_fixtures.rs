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
