//! The check of an embedded ICC profile before krilla gets it (design §4.1.1, §4.3).

/// Length of the fixed header of an ICC profile, followed by the tag count.
const ICC_HEADER_LEN: usize = 128;

/// Whether `data` starts with an ICC profile header: the `acsp` signature, and a declared size
/// that holds the header and the tag count and is within `data` (padding after it is allowed).
///
/// krilla checks only the version and colour space bytes, then embeds the bytes unread as an
/// `ICCBased` colour space, so junk with the right colour space bytes would go into the PDF,
/// and into a PDF/A, as a broken colour space.
pub(crate) fn has_profile_header(data: &[u8]) -> bool {
    let declared_size = data
        .get(0..4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_be_bytes)
        .and_then(|size| usize::try_from(size).ok());
    declared_size.is_some_and(|size| (ICC_HEADER_LEN + 4..=data.len()).contains(&size))
        && data.get(36..40) == Some(b"acsp".as_slice())
}

/// A profile header of `len` bytes with `color_space`; the rest is zero. For tests.
#[cfg(test)]
pub(crate) fn test_profile(len: usize, color_space: &[u8; 4]) -> Vec<u8> {
    let mut data = vec![0u8; len];
    data[0..4].copy_from_slice(&u32::try_from(len).unwrap_or(u32::MAX).to_be_bytes());
    data[16..20].copy_from_slice(color_space);
    data[36..40].copy_from_slice(b"acsp");
    data
}
