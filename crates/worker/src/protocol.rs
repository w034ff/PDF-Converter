//! Messages between the main process and the worker (design §5.1).
//!
//! One message is a 4-byte little-endian length, a JSON header of that length,
//! and then `bodyLength` raw bytes when the header says so. Images travel as
//! raw bytes instead of base64.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Upper bound on a header, so a corrupt length cannot make the reader
/// allocate gigabytes. Headers are small JSON objects.
pub const MAX_HEADER_BYTES: u32 = 1024 * 1024;

/// Upper bound on a body; a rendered page is far smaller (design §4.4 caps a
/// page at `MAX_RENDER_PIXELS`).
pub const MAX_BODY_BYTES: u64 = 1024 * 1024 * 1024;

/// A request from the main process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    /// Loads pdfium if needed and reports whether it works.
    Hello,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Response {
    /// pdfium is loaded and could create a document.
    Hello,
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
}
