//! Messages between the main process and the worker (design §5.1).
//!
//! One message is a 4-byte little-endian length, a JSON header of that length,
//! and then `bodyLength` raw bytes when the header says so. Images travel as
//! raw bytes instead of base64.

use std::fmt;
use std::io::{self, Read, Write};
use std::ops::Deref;
use std::path::PathBuf;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Upper bound on a header, so a corrupt length cannot make the reader
/// allocate gigabytes. Headers are small JSON objects.
pub const MAX_HEADER_BYTES: u32 = 1024 * 1024;

/// Upper bound on a body; a rendered page is far smaller (design §4.4 caps a
/// page at `MAX_RENDER_PIXELS`).
pub const MAX_BODY_BYTES: u64 = 1024 * 1024 * 1024;

/// How much of an organized PDF one `OrganizeChunk` carries at most (design
/// organize §3). The output is not sent as one body: that would hold it whole
/// in both processes and run into [`MAX_BODY_BYTES`]. One mebibyte keeps the
/// number of messages for an output of several hundred mebibytes in the
/// hundreds, not the hundreds of thousands.
pub const OUTPUT_CHUNK_BYTES: usize = 1024 * 1024;

/// Errors when constructing a [`Password`] (design §6.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordError {
    /// The password was empty.
    Empty,
    /// The password contained a NUL byte (`U+0000`).
    ContainsNul,
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "password cannot be empty"),
            Self::ContainsNul => write!(f, "password cannot contain NUL byte"),
        }
    }
}

impl std::error::Error for PasswordError {}

/// A validated password for opening encrypted PDFs (design §5.1, §6.8).
///
/// Cannot be empty and cannot contain NUL (`U+0000`). Whitespace and fullwidth
/// characters are preserved verbatim. In JSON, it serializes and deserializes
/// as a string. `Debug` redacts the value and `Display` is intentionally omitted
/// so that passwords are never printed to logs or terminal output.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Password(String);

impl Password {
    /// Validates and constructs a [`Password`] from `s`.
    pub fn new(s: impl Into<String>) -> Result<Self, PasswordError> {
        let s = s.into();
        Self::validate(&s)?;
        Ok(Self(s))
    }

    fn validate(s: &str) -> Result<(), PasswordError> {
        if s.is_empty() {
            return Err(PasswordError::Empty);
        }
        if s.contains('\0') {
            return Err(PasswordError::ContainsNul);
        }
        Ok(())
    }

    /// Borrows the password as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for Password {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Password {
    type Error = PasswordError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<Password> for String {
    fn from(p: Password) -> Self {
        p.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Password(<redacted>)")
    }
}

/// The image format to render a PDF page to (design §4.4, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormat {
    Png,
    Jpeg,
    Webp,
}

/// The color mode to render a PDF page in (design §4.4, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderColor {
    Color,
    Gray,
}

/// Width and height of a PDF page in points (1/72 inch) (design §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDimensions {
    pub width_pt: f32,
    pub height_pt: f32,
}

/// A rectangle on a page, in points, in the orientation the page is shown in:
/// the origin is the top-left corner and y grows downwards (design §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RectPt {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// One embedded image of a page (design §4.6, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedImage {
    /// Position among all the images met while walking the page in drawing
    /// order, including those left out of the list, so it does not change
    /// when the list is filtered.
    pub image_index: u32,
    /// The part of the image's frame that shows on the page, cut to the
    /// page: a clip or a mask can hide the rest (design §4.6).
    pub rect_pt: RectPt,
    pub px_width: u32,
    pub px_height: u32,
    /// The size the image is written at in its own pixels, or `None` when it
    /// is not placed by flips and quarter turns only and so cannot be. It is
    /// the whole image's: a clip or a mask that hides part of it makes the
    /// written image smaller (design §4.6).
    pub native_size: Option<NativeSize>,
}

/// The size of an image written in its own pixels, in the orientation it is
/// shown in, as `RenderRegion` answers it for an image that nothing hides
/// (design §4.6, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSize {
    pub width: u32,
    pub height: u32,
    /// Set when the image's pixels exceed `MAX_RENDER_PIXELS`, so that it is
    /// written smaller.
    pub scaled_down: bool,
}

/// What a layout candidate is (design §4.7, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateKind {
    /// An embedded image; the rectangle is the image's visible frame.
    Photo,
    /// A picture drawn with shapes rather than an embedded image.
    Figure,
    Chart,
    Table,
}

/// One region that layout detection suggests cropping (design §4.7, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutCandidate {
    pub kind: CandidateKind,
    /// The model's confidence in the box, also for a photo.
    pub score: f32,
    pub rect_pt: RectPt,
    /// The matched embedded image; set only for [`CandidateKind::Photo`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_index: Option<u32>,
}

/// One source PDF of an `Organize` request (design organize §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeSource {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Password>,
}

/// One page of an organized output, in output order (design organize §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePage {
    /// Index into the `sources` of the request.
    pub source: u32,
    /// Page of that source, counted from 0.
    pub page: u32,
    /// Clockwise degrees added to the page's own rotation: 0, 90, 180 or 270.
    pub rotation: u16,
}

/// A request from the main process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    /// Loads pdfium if needed and reports whether it works.
    Hello,
    /// Opens a PDF document at `path` (design §5.1, §6.8).
    Open {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<Password>,
    },
    /// Renders page `page` (1-based) at `dpi` in `format` and `color` (design §5.1).
    Render {
        page: u32,
        dpi: u32,
        format: RenderFormat,
        color: RenderColor,
    },
    /// Renders page `page` (1-based) with the longer side at `max_side` as PNG (design §5.1).
    Thumbnail { page: u32, max_side: u32 },
    /// Lists the embedded images of page `page` (design §4.6, §5.1).
    PageImages { page: u32 },
    /// Whether anything is drawn over the embedded image `image_index` of
    /// page `page` (design §4.6, §5.1).
    ImageOnTop { page: u32, image_index: u32 },
    /// Detects the photos, figures, charts and tables of page `page`
    /// (design §4.7, §5.1).
    Detect { page: u32 },
    /// Renders one range of page `page` (design §4.6, §5.1). `image_index`
    /// is set for a photo range; then `photo_native` writes the image's own
    /// pixels and `photo_only` leaves out everything but the image.
    RenderRegion {
        page: u32,
        rect_pt: RectPt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_index: Option<u32>,
        dpi: u32,
        format: RenderFormat,
        color: RenderColor,
        photo_native: bool,
        photo_only: bool,
    },
    /// Builds one organized PDF from pages of `sources` (design organize §3).
    /// `creator` is the producer written into the document information.
    /// The answer is several messages, not one: see [`Response::Organized`].
    Organize {
        sources: Vec<OrganizeSource>,
        pages: Vec<OrganizePage>,
        creator: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_password: Option<Password>,
    },
    /// Closes any currently opened PDF document (design §5.1).
    Close,
    /// Allocates and touches this much memory, to test the memory limit.
    #[cfg(feature = "test-hooks")]
    AllocateForTest { mebibytes: u64 },
    /// Aborts the worker, to test crash handling.
    #[cfg(feature = "test-hooks")]
    CrashForTest,
    /// Never answers, to test the timeout.
    #[cfg(feature = "test-hooks")]
    HangForTest,
}

/// A response from the worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Response {
    /// pdfium is loaded and could create a document.
    Hello,
    /// A PDF was opened successfully (design §5.1).
    Open {
        page_count: u32,
        pages: Vec<PageDimensions>,
    },
    /// A page was rendered; the image bytes are in the message body (design §5.1).
    Render,
    /// A thumbnail was rendered; the PNG bytes are in the message body (design §5.1).
    Thumbnail,
    /// The embedded images of the page (design §5.1).
    PageImages { images: Vec<EmbeddedImage> },
    /// Whether anything is drawn over the image (design §5.1).
    ImageOnTop { on_top: bool },
    /// The layout candidates of the page, highest score first (design §4.7).
    Detect { candidates: Vec<LayoutCandidate> },
    /// A range was rendered; the image bytes are in the message body.
    /// `width` and `height` are the written image's size; `scaled_down` is
    /// set when a photo had to be written smaller than its own pixels to stay
    /// within `MAX_RENDER_PIXELS` (design §4.6, §5.1).
    RenderRegion {
        width: u32,
        height: u32,
        scaled_down: bool,
    },
    /// The currently open PDF was closed (design §5.1).
    Close,
    /// Pages processed so far for an `Organize` request, counted over the
    /// whole output (design organize §3).
    OrganizeProgress { done: u32 },
    /// A part of the organized PDF, at most [`OUTPUT_CHUNK_BYTES`], in the
    /// message body (design organize §3).
    OrganizeChunk,
    /// The last answer to an `Organize` request: `bytes` is the total length
    /// of the chunks sent.
    Organized { bytes: u64 },
    /// A source PDF of an `Organize` request could not be used; `source` is
    /// its index in the request and `code` one of the codes of design §6.6.
    OrganizeFailed {
        code: String,
        detail: Option<String>,
        source: u32,
    },
    /// The request failed; `code` is one of the error codes of design §6.6.
    Error {
        code: String,
        detail: Option<String>,
    },
    /// A test allocation succeeded.
    #[cfg(feature = "test-hooks")]
    Allocated,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<T> {
    message: T,
    body_length: u64,
}

/// Writes one message with an optional body and flushes it.
///
/// # Errors
///
/// Returns the I/O error of the underlying writer, or `InvalidData` if the
/// header cannot be encoded or is larger than [`MAX_HEADER_BYTES`].
pub fn write_message<W: Write, T: Serialize>(
    writer: &mut W,
    message: &T,
    body: &[u8],
) -> io::Result<()> {
    let header = serde_json::to_vec(&Envelope {
        message,
        body_length: body.len() as u64,
    })
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let length = u32::try_from(header.len())
        .ok()
        .filter(|len| *len <= MAX_HEADER_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "header too large"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&header)?;
    writer.write_all(body)?;
    writer.flush()
}

/// Reads one message and its body.
///
/// Returns `Ok(None)` when the stream ends cleanly before a message starts,
/// which is how the worker learns that the main process has gone.
///
/// # Errors
///
/// Returns the I/O error of the underlying reader (`UnexpectedEof` when the
/// stream ends inside a message), or `InvalidData` for an oversized or
/// malformed header.
pub fn read_message<R: Read, T: DeserializeOwned>(
    reader: &mut R,
) -> io::Result<Option<(T, Vec<u8>)>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let length = u32::from_le_bytes(length);
    if length > MAX_HEADER_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header too large",
        ));
    }
    let mut header = vec![0u8; length as usize];
    reader.read_exact(&mut header)?;
    let envelope: Envelope<T> = serde_json::from_slice(&header)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if envelope.body_length > MAX_BODY_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }
    let mut body = Vec::new();
    reader
        .by_ref()
        .take(envelope.body_length)
        .read_to_end(&mut body)?;
    if body.len() as u64 != envelope.body_length {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "body ended early",
        ));
    }
    Ok(Some((envelope.message, body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_message_with_a_body() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &Request::Hello, b"abc").unwrap();
        let (message, body): (Request, Vec<u8>) =
            read_message(&mut buffer.as_slice()).unwrap().unwrap();
        assert_eq!(message, Request::Hello);
        assert_eq!(body, b"abc");
    }

    #[test]
    fn reads_consecutive_messages() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &Response::Hello, &[]).unwrap();
        let error = Response::Error {
            code: "PdfOpenFailed".into(),
            detail: None,
        };
        write_message(&mut buffer, &error, &[]).unwrap();
        let mut reader = buffer.as_slice();
        let first: (Response, Vec<u8>) = read_message(&mut reader).unwrap().unwrap();
        let second: (Response, Vec<u8>) = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(first.0, Response::Hello);
        assert_eq!(second.0, error);
    }

    #[test]
    fn clean_end_of_stream_is_none() {
        let result: Option<(Request, Vec<u8>)> = read_message(&mut [].as_slice()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn stream_ending_inside_a_body_is_an_error() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &Request::Hello, b"abcdef").unwrap();
        buffer.truncate(buffer.len() - 2);
        let result: io::Result<Option<(Request, Vec<u8>)>> = read_message(&mut buffer.as_slice());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn oversized_header_length_is_rejected_before_allocating() {
        let buffer = (MAX_HEADER_BYTES + 1).to_le_bytes();
        let result: io::Result<Option<(Request, Vec<u8>)>> = read_message(&mut buffer.as_slice());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn uses_camel_case_tags_on_the_wire() {
        let json = serde_json::to_string(&Response::Error {
            code: "X".into(),
            detail: Some("d".into()),
        })
        .unwrap();
        assert_eq!(json, r#"{"type":"error","code":"X","detail":"d"}"#);
    }

    #[test]
    fn serializes_request_and_response_fields_as_camel_case() {
        let req = Request::Thumbnail {
            page: 2,
            max_side: 160,
        };
        let req_json = serde_json::to_string(&req).unwrap();
        assert_eq!(req_json, r#"{"type":"thumbnail","page":2,"maxSide":160}"#);

        let render_req = Request::Render {
            page: 1,
            dpi: 150,
            format: RenderFormat::Jpeg,
            color: RenderColor::Color,
        };
        let render_json = serde_json::to_string(&render_req).unwrap();
        assert_eq!(
            render_json,
            r#"{"type":"render","page":1,"dpi":150,"format":"jpeg","color":"color"}"#
        );

        let render_webp_gray = Request::Render {
            page: 3,
            dpi: 600,
            format: RenderFormat::Webp,
            color: RenderColor::Gray,
        };
        let render_webp_gray_json = serde_json::to_string(&render_webp_gray).unwrap();
        assert_eq!(
            render_webp_gray_json,
            r#"{"type":"render","page":3,"dpi":600,"format":"webp","color":"gray"}"#
        );

        let resp = Response::Open {
            page_count: 1,
            pages: vec![PageDimensions {
                width_pt: 595.28,
                height_pt: 841.89,
            }],
        };
        let resp_json = serde_json::to_string(&resp).unwrap();
        assert!(resp_json.contains(r#""pageCount":1"#));
        assert!(resp_json.contains(r#""widthPt":595.28"#));
        assert!(resp_json.contains(r#""heightPt":841.89"#));
    }

    #[cfg(not(feature = "test-hooks"))]
    #[test]
    fn rejects_test_hooks_when_feature_is_disabled() {
        assert!(serde_json::from_str::<Request>(r#"{"type":"crashForTest"}"#).is_err());
        assert!(serde_json::from_str::<Request>(r#"{"type":"hangForTest"}"#).is_err());
        assert!(
            serde_json::from_str::<Request>(r#"{"type":"allocateForTest","mebibytes":10}"#)
                .is_err()
        );
    }

    #[test]
    fn password_rejects_empty_and_nul_both_directly_and_via_serde() {
        assert_eq!(Password::new(""), Err(PasswordError::Empty));
        assert!(serde_json::from_str::<Password>(r#""""#).is_err());

        let nul_str = "abc\0def";
        assert_eq!(Password::new(nul_str), Err(PasswordError::ContainsNul));
        assert!(serde_json::from_str::<Password>(r#""abc\u0000def""#).is_err());
    }

    #[test]
    fn password_preserves_whitespace_and_fullwidth_characters() {
        let spaced = "  secret  ";
        let pass_spaced = Password::new(spaced).unwrap();
        assert_eq!(pass_spaced.as_str(), spaced);
        assert_eq!(&*pass_spaced, spaced);

        let fullwidth = "パスワード　１２３";
        let pass_fullwidth = Password::new(fullwidth).unwrap();
        assert_eq!(pass_fullwidth.as_str(), fullwidth);
    }

    #[test]
    fn password_debug_and_request_open_debug_redact_value() {
        let pass = Password::new("my-secret-password-123").unwrap();
        let pass_debug = format!("{pass:?}");
        assert_eq!(pass_debug, "Password(<redacted>)");
        assert!(!pass_debug.contains("my-secret"));

        let req = Request::Open {
            path: PathBuf::from("doc.pdf"),
            password: Some(pass),
        };
        let req_debug = format!("{req:?}");
        assert!(req_debug.contains("Password(<redacted>)"));
        assert!(!req_debug.contains("my-secret"));
    }

    #[test]
    fn password_roundtrips_in_json() {
        let pass = Password::new("café").unwrap();
        let serialized = serde_json::to_string(&pass).unwrap();
        assert_eq!(serialized, r#""café""#);

        let deserialized: Password = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, pass);
        assert_eq!(deserialized.as_str(), "café");

        let req = Request::Open {
            path: PathBuf::from("doc.pdf"),
            password: Some(pass),
        };
        let req_json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            req_json,
            r#"{"type":"open","path":"doc.pdf","password":"café"}"#
        );
        let deserialized_req: Request = serde_json::from_str(&req_json).unwrap();
        assert_eq!(deserialized_req, req);

        let req_no_pass = Request::Open {
            path: PathBuf::from("doc.pdf"),
            password: None,
        };
        let req_no_pass_json = serde_json::to_string(&req_no_pass).unwrap();
        assert_eq!(req_no_pass_json, r#"{"type":"open","path":"doc.pdf"}"#);
        let deserialized_no_pass: Request = serde_json::from_str(&req_no_pass_json).unwrap();
        assert_eq!(deserialized_no_pass, req_no_pass);
    }

    #[test]
    fn crop_requests_use_the_names_of_design_5_1() {
        let region = Request::RenderRegion {
            page: 2,
            rect_pt: RectPt {
                x0: 1.5,
                y0: 2.0,
                x1: 30.0,
                y1: 40.25,
            },
            image_index: Some(3),
            dpi: 150,
            format: RenderFormat::Png,
            color: RenderColor::Gray,
            photo_native: true,
            photo_only: false,
        };
        let json = serde_json::to_string(&region).unwrap();
        assert_eq!(
            json,
            r#"{"type":"renderRegion","page":2,"rectPt":{"x0":1.5,"y0":2.0,"x1":30.0,"y1":40.25},"imageIndex":3,"dpi":150,"format":"png","color":"gray","photoNative":true,"photoOnly":false}"#
        );
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), region);

        // A range that is not a photo has no image index at all.
        let rectangle = r#"{"type":"renderRegion","page":1,"rectPt":{"x0":0.0,"y0":0.0,"x1":1.0,"y1":1.0},"dpi":72,"format":"jpeg","color":"color","photoNative":false,"photoOnly":false}"#;
        match serde_json::from_str::<Request>(rectangle).unwrap() {
            Request::RenderRegion { image_index, .. } => assert_eq!(image_index, None),
            other => panic!("{other:?}"),
        }

        assert_eq!(
            serde_json::to_string(&Request::PageImages { page: 4 }).unwrap(),
            r#"{"type":"pageImages","page":4}"#
        );
        assert_eq!(
            serde_json::to_string(&Request::ImageOnTop {
                page: 4,
                image_index: 7
            })
            .unwrap(),
            r#"{"type":"imageOnTop","page":4,"imageIndex":7}"#
        );
    }

    #[test]
    fn crop_responses_use_the_names_of_design_5_1() {
        let images = Response::PageImages {
            images: vec![EmbeddedImage {
                image_index: 2,
                rect_pt: RectPt {
                    x0: 1.0,
                    y0: 2.0,
                    x1: 3.0,
                    y1: 4.0,
                },
                px_width: 61,
                px_height: 43,
                native_size: Some(NativeSize {
                    width: 43,
                    height: 61,
                    scaled_down: false,
                }),
            }],
        };
        assert_eq!(
            serde_json::to_string(&images).unwrap(),
            r#"{"type":"pageImages","images":[{"imageIndex":2,"rectPt":{"x0":1.0,"y0":2.0,"x1":3.0,"y1":4.0},"pxWidth":61,"pxHeight":43,"nativeSize":{"width":43,"height":61,"scaledDown":false}}]}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::ImageOnTop { on_top: true }).unwrap(),
            r#"{"type":"imageOnTop","onTop":true}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::RenderRegion {
                width: 10,
                height: 20,
                scaled_down: false
            })
            .unwrap(),
            r#"{"type":"renderRegion","width":10,"height":20,"scaledDown":false}"#
        );
    }

    #[test]
    fn organize_messages_use_the_names_of_the_design() {
        let request = Request::Organize {
            sources: vec![OrganizeSource {
                path: PathBuf::from("a.pdf"),
                password: Some(Password::new("pw").unwrap()),
            }],
            pages: vec![OrganizePage {
                source: 0,
                page: 2,
                rotation: 90,
            }],
            creator: "PDF Converter 1".into(),
            output_password: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"organize","sources":[{"path":"a.pdf","password":"pw"}],"pages":[{"source":0,"page":2,"rotation":90}],"creator":"PDF Converter 1"}"#
        );
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        assert!(!format!("{request:?}").contains("\"pw\""));

        assert_eq!(
            serde_json::to_string(&Response::OrganizeProgress { done: 3 }).unwrap(),
            r#"{"type":"organizeProgress","done":3}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::OrganizeChunk).unwrap(),
            r#"{"type":"organizeChunk"}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Organized { bytes: 9 }).unwrap(),
            r#"{"type":"organized","bytes":9}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::OrganizeFailed {
                code: "PdfOpenFailed".into(),
                detail: None,
                source: 1
            })
            .unwrap(),
            r#"{"type":"organizeFailed","code":"PdfOpenFailed","detail":null,"source":1}"#
        );
    }

    #[test]
    fn detect_uses_the_names_of_design_5_1() {
        assert_eq!(
            serde_json::to_string(&Request::Detect { page: 3 }).unwrap(),
            r#"{"type":"detect","page":3}"#
        );
        let rect_pt = RectPt {
            x0: 1.0,
            y0: 2.0,
            x1: 3.0,
            y1: 4.0,
        };
        let detected = Response::Detect {
            candidates: vec![
                LayoutCandidate {
                    kind: CandidateKind::Photo,
                    score: 0.5,
                    rect_pt,
                    image_index: Some(2),
                },
                LayoutCandidate {
                    kind: CandidateKind::Figure,
                    score: 0.75,
                    rect_pt,
                    image_index: None,
                },
                LayoutCandidate {
                    kind: CandidateKind::Chart,
                    score: 0.25,
                    rect_pt,
                    image_index: None,
                },
                LayoutCandidate {
                    kind: CandidateKind::Table,
                    score: 1.0,
                    rect_pt,
                    image_index: None,
                },
            ],
        };
        let json = serde_json::to_string(&detected).unwrap();
        let rect = r#""rectPt":{"x0":1.0,"y0":2.0,"x1":3.0,"y1":4.0}"#;
        assert_eq!(
            json,
            format!(
                r#"{{"type":"detect","candidates":[{{"kind":"photo","score":0.5,{rect},"imageIndex":2}},{{"kind":"figure","score":0.75,{rect}}},{{"kind":"chart","score":0.25,{rect}}},{{"kind":"table","score":1.0,{rect}}}]}}"#
            )
        );
        assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), detected);
    }
}
