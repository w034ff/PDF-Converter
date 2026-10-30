//! Password protection of a written PDF (design §4.3.1).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{Dictionary, Document, Object};

use crate::error::PdfWriteError;

/// Longest password in characters. The viewers other than pdfium cut a password at 127 bytes
/// before checking it, while lopdf does not, so a longer one would not open (design §4.3.1).
pub const MAX_OUTPUT_PASSWORD_LEN: usize = 127;

/// The first and last characters of a password: the printable ASCII characters and the space.
/// lopdf normalizes a password (SASLprep) before using it, and the viewers do not, so any
/// other character would not open the PDF (design §4.3.1). The front end copies this rule.
pub const OUTPUT_PASSWORD_CHARS: std::ops::RangeInclusive<char> = ' '..='~';

/// Name of the crypt filter that covers both streams and strings.
const CRYPT_FILTER_NAME: &[u8] = b"StdCF";
/// Size in bytes of the file encryption key of AES-256.
const FILE_KEY_LEN: usize = 32;
/// The same size in bits, as the encryption dictionary writes it.
const FILE_KEY_BITS: i64 = 256;

/// Why a string cannot be the password of a written PDF (design §4.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputPasswordError {
    /// The password is empty.
    Empty,
    /// A character is outside U+0020–U+007E.
    InvalidCharacter,
    /// The password is longer than [`MAX_OUTPUT_PASSWORD_LEN`] characters.
    TooLong,
}

impl fmt::Display for OutputPasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the password is empty"),
            Self::InvalidCharacter => {
                write!(
                    f,
                    "the password has a character other than ASCII printable ones"
                )
            }
            Self::TooLong => write!(
                f,
                "the password is longer than {MAX_OUTPUT_PASSWORD_LEN} characters"
            ),
        }
    }
}

impl std::error::Error for OutputPasswordError {}

/// The password of a written PDF, which opens it as a reader and as its owner.
///
/// Only [`OutputPassword::new`] makes one, so every value follows the rules of design §4.3.1.
/// `Debug` and `Display` leave the value out.
#[derive(Clone, PartialEq, Eq)]
pub struct OutputPassword(String);

impl OutputPassword {
    /// Checks `password` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`OutputPasswordError::Empty`] for an empty string,
    /// [`OutputPasswordError::InvalidCharacter`] for a character outside U+0020–U+007E, and
    /// [`OutputPasswordError::TooLong`] for more than [`MAX_OUTPUT_PASSWORD_LEN`] characters.
    /// The checks run in that order, so a long password with a full-width character is an
    /// invalid character.
    pub fn new(password: String) -> Result<Self, OutputPasswordError> {
        if password.is_empty() {
            return Err(OutputPasswordError::Empty);
        }
        if !password.chars().all(|c| OUTPUT_PASSWORD_CHARS.contains(&c)) {
            return Err(OutputPasswordError::InvalidCharacter);
        }
        if password.len() > MAX_OUTPUT_PASSWORD_LEN {
            return Err(OutputPasswordError::TooLong);
        }
        Ok(Self(password))
    }
}

impl fmt::Debug for OutputPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OutputPassword(<hidden>)")
    }
}

impl fmt::Display for OutputPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<hidden>")
    }
}

fn write_failed(err: impl fmt::Display) -> PdfWriteError {
    PdfWriteError(err.to_string())
}

/// Encrypts a PDF written by [`crate::PdfWriter::finish`] with AES-256 (design §4.3.1).
///
/// `pdf` is taken by value so that it is freed once lopdf has read it. A PDF/A document must
/// not be passed: PDF/A forbids encryption.
///
/// # Errors
///
/// Returns [`PdfWriteError`] if lopdf cannot read or write the PDF or the OS cannot give random
/// bytes. `pdf` comes from krilla, so a failure means a bug, not a bad input.
pub fn encrypt_pdf(pdf: Vec<u8>, password: &OutputPassword) -> Result<Vec<u8>, PdfWriteError> {
    let mut doc = Document::load_mem(&pdf).map_err(write_failed)?;
    drop(pdf);
    encrypt_document(&mut doc, password)?;

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(write_failed)?;
    Ok(out)
}

/// Encrypts `doc` in place with AES-256, so that saving it writes the encrypted PDF (design
/// §4.3.1). [`encrypt_pdf`] and [`crate::finish_organized`] share it.
pub(crate) fn encrypt_document(
    doc: &mut Document,
    password: &OutputPassword,
) -> Result<(), PdfWriteError> {
    let mut key = [0u8; FILE_KEY_LEN];
    getrandom::fill(&mut key).map_err(write_failed)?;

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    // The owner password is the reader's, too: an empty one would let anyone open the PDF as
    // its owner with the empty password and take the encryption off.
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(CRYPT_FILTER_NAME.to_vec(), crypt_filter)]),
        file_encryption_key: &key,
        stream_filter: CRYPT_FILTER_NAME.to_vec(),
        string_filter: CRYPT_FILTER_NAME.to_vec(),
        owner_password: &password.0,
        user_password: &password.0,
        permissions: Permissions::all(),
    })
    .map_err(write_failed)?;
    doc.encrypt(&state).map_err(write_failed)?;

    add_key_length(doc)?;
    add_extensions(doc)
}

/// Writes `/Length 256` into the encryption dictionary and `/Length 32` into its `StdCF`.
/// lopdf writes neither, and poppler then takes the key for a short one and draws every page
/// broken. Either entry alone fixes it; both are written as qpdf does.
fn add_key_length(doc: &mut Document) -> Result<(), PdfWriteError> {
    let encrypt_id = doc
        .trailer
        .get(b"Encrypt")
        .and_then(Object::as_reference)
        .map_err(write_failed)?;
    let encrypt = doc
        .get_object_mut(encrypt_id)
        .and_then(Object::as_dict_mut)
        .map_err(write_failed)?;
    encrypt.set(b"Length", Object::Integer(FILE_KEY_BITS));
    let std_cf = encrypt
        .get_mut(b"CF")
        .and_then(Object::as_dict_mut)
        .and_then(|filters| filters.get_mut(CRYPT_FILTER_NAME))
        .and_then(Object::as_dict_mut)
        .map_err(write_failed)?;
    std_cf.set(b"Length", Object::Integer(FILE_KEY_LEN as i64));
    Ok(())
}

/// Declares in the catalog that the PDF 1.7 file uses AES-256 (Adobe extension level 8), as qpdf
/// does: krilla writes PDF 1.7, which has no AES-256 of its own.
fn add_extensions(doc: &mut Document) -> Result<(), PdfWriteError> {
    let mut adbe = Dictionary::new();
    adbe.set(b"BaseVersion", Object::Name(b"1.7".to_vec()));
    adbe.set(b"ExtensionLevel", Object::Integer(8));
    let mut extensions = Dictionary::new();
    extensions.set(b"ADBE", Object::Dictionary(adbe));

    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(write_failed)?;
    doc.get_object_mut(root_id)
        .and_then(Object::as_dict_mut)
        .map_err(write_failed)?
        .set(b"Extensions", Object::Dictionary(extensions));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(password: &str) -> Result<OutputPassword, OutputPasswordError> {
        OutputPassword::new(password.to_string())
    }

    #[test]
    fn empty_password_is_refused() {
        assert_eq!(new(""), Err(OutputPasswordError::Empty));
    }

    #[test]
    fn characters_outside_printable_ascii_are_refused() {
        for password in [
            "a\u{1f}b",
            "a\u{7f}b",
            "a\tb",
            "ＡＢＣ１２３",
            "a\u{3000}b",
            "パスワード",
            "ｱｲｳ",
            "é",
            "😀",
        ] {
            assert_eq!(
                new(password),
                Err(OutputPasswordError::InvalidCharacter),
                "{password:?}"
            );
        }
    }

    #[test]
    fn spaces_and_symbols_are_accepted() {
        for password in [
            " ",
            "   ",
            "U15 pass-word!",
            "~",
            "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        ] {
            assert!(new(password).is_ok(), "{password:?}");
        }
    }

    #[test]
    fn length_limit_is_127_characters() {
        assert!(new(&"a".repeat(MAX_OUTPUT_PASSWORD_LEN)).is_ok());
        assert_eq!(
            new(&"a".repeat(MAX_OUTPUT_PASSWORD_LEN + 1)),
            Err(OutputPasswordError::TooLong)
        );
    }

    #[test]
    fn long_password_with_a_wide_character_is_an_invalid_character() {
        let password = format!("{}あ", "a".repeat(MAX_OUTPUT_PASSWORD_LEN));
        assert_eq!(new(&password), Err(OutputPasswordError::InvalidCharacter));
    }

    #[test]
    fn debug_and_display_leave_the_value_out() {
        let password = new("s3cret-value").expect("valid password");
        for text in [format!("{password:?}"), format!("{password}")] {
            assert!(!text.contains("s3cret"), "{text}");
        }
    }
}
