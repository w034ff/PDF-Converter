//! The worker side: answers requests on stdin/stdout until stdin closes
//! (design §5). This is the only code that loads pdfium.

use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use pdfium_render::prelude::Pdfium;

use crate::protocol::{Request, Response, read_message, write_message};

/// How much memory one worker may use (design §5.3). Allocations beyond it
/// fail and end the worker, which the main process reports as a crash.
pub const WORKER_MEMORY_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Error code for a pdfium library that cannot be loaded or used.
const CODE_PDFIUM_UNAVAILABLE: &str = "PdfiumUnavailable";

/// Runs the worker loop. `library_dir` is the folder holding the bundled
/// pdfium library; pdfium is loaded on the first request that needs it.
///
/// # Errors
///
/// Returns an I/O error when stdin or stdout fails. A clean end of stdin
/// (the main process closed the pipe or exited) returns `Ok(())`.
pub fn run(library_dir: &Path) -> io::Result<()> {
    apply_memory_limit()?;
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    let mut state = State {
        library_dir: library_dir.to_path_buf(),
        pdfium: None,
    };
    while let Some((request, _body)) = read_message::<_, Request>(&mut input)? {
        let response = state.handle(request);
        write_message(&mut output, &response, &[])?;
    }
    Ok(())
}

struct State {
    library_dir: PathBuf,
    pdfium: Option<Pdfium>,
}

impl State {
    fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::Hello => match self.pdfium() {
                Ok(pdfium) => match pdfium.create_new_pdf() {
                    Ok(_) => Response::Hello,
                    Err(e) => unavailable(&e),
                },
                Err(response) => response,
            },
            #[cfg(feature = "test-hooks")]
            Request::AllocateForTest { mebibytes } => {
                const MIB: usize = 1024 * 1024;
                let size = usize::try_from(mebibytes)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(MIB);
                // Writing every page makes the operating system commit the
                // memory, so the limit is hit here rather than lazily later.
                let mut block = vec![0u8; size];
                for byte in block.iter_mut().step_by(4096) {
                    *byte = 1;
                }
                std::hint::black_box(&block);
                Response::Allocated
            }
            #[cfg(feature = "test-hooks")]
            Request::CrashForTest => std::process::abort(),
            #[cfg(feature = "test-hooks")]
            Request::HangForTest => loop {
                std::thread::park();
            },
        }
    }

    fn pdfium(&mut self) -> Result<&Pdfium, Response> {
        if self.pdfium.is_none() {
            let path = Pdfium::pdfium_platform_library_name_at_path(&self.library_dir);
            let bindings = Pdfium::bind_to_library(&path).map_err(|e| unavailable(&e))?;
            self.pdfium = Some(Pdfium::new(bindings));
        }
        self.pdfium
            .as_ref()
            .ok_or_else(|| unavailable(&"pdfium was not stored"))
    }
}

fn unavailable(error: &dyn std::fmt::Debug) -> Response {
    Response::Error {
        code: CODE_PDFIUM_UNAVAILABLE.into(),
        detail: Some(format!("{error:?}")),
    }
}

#[cfg(target_os = "linux")]
fn apply_memory_limit() -> io::Result<()> {
    // RLIMIT_AS caps the address space; it must be set before pdfium is
    // loaded so that pdfium's own allocations are covered too.
    rlimit::setrlimit(
        rlimit::Resource::AS,
        WORKER_MEMORY_LIMIT,
        WORKER_MEMORY_LIMIT,
    )
}

#[cfg(windows)]
fn apply_memory_limit() -> io::Result<()> {
    // On Windows the main process puts the worker in a job object with the
    // limit before sending the first request (crate::windows_job).
    Ok(())
}
