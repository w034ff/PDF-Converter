//! Talks to `pdfconv-worker` processes off the UI thread: opens PDFs,
//! renders thumbnails and writes organized PDFs. Results come back through
//! callbacks that the caller forwards to the Slint event loop.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use pdfconv_worker::client::{
    OPEN_TIMEOUT, ORGANIZE_STEP_TIMEOUT, RENDER_TIMEOUT, StreamPart, WorkerError, WorkerProcess,
};
use pdfconv_worker::protocol::{OrganizePage, OrganizeSource, PageDimensions, Request, Response};
use slint::{Rgba8Pixel, SharedPixelBuffer};

/// Workers rendering thumbnails in parallel.
pub const THUMB_WORKERS: usize = 2;

/// Thumbnails are rendered at twice the drawn size for high-DPI screens.
pub const THUMB_MAX_SIDE: u32 = 240;

/// How a worker is started: this executable with `--worker <pdfium dir>`.
#[derive(Clone)]
pub struct WorkerCommand {
    pub program: OsString,
    pub pdfium_dir: PathBuf,
}

impl WorkerCommand {
    pub fn spawn(&self) -> Result<WorkerProcess, WorkerError> {
        WorkerProcess::spawn(
            &self.program,
            [OsString::from("--worker"), self.pdfium_dir.clone().into_os_string()],
        )
    }
}

pub type ThumbPixels = SharedPixelBuffer<Rgba8Pixel>;

#[derive(Default)]
struct Queue {
    /// Requests in arrival order; the newest is served first.
    pending: Vec<(u32, u32)>,
    /// (source, page) currently on screen. A request whose page left the
    /// screen before a worker took it is dropped.
    wanted: HashSet<(u32, u32)>,
    sources: Vec<PathBuf>,
}

pub struct ThumbService {
    shared: Arc<(Mutex<Queue>, Condvar)>,
}

impl ThumbService {
    /// Starts [`THUMB_WORKERS`] threads, each owning one worker process.
    /// `on_done(source, page, pixels)` runs on a worker thread.
    pub fn start(
        command: WorkerCommand,
        on_done: impl Fn(u32, u32, Result<ThumbPixels, String>) + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let on_done = Arc::new(on_done);
        for _ in 0..THUMB_WORKERS {
            let shared = shared.clone();
            let on_done = on_done.clone();
            let command = command.clone();
            thread::spawn(move || thumb_loop(&command, &shared, &*on_done));
        }
        Self { shared }
    }

    pub fn add_source(&self, path: PathBuf) -> u32 {
        let mut q = self.shared.0.lock().unwrap();
        q.sources.push(path);
        (q.sources.len() - 1) as u32
    }

    /// Replaces the set of pages on screen and queues those not yet asked.
    /// `missing` are the visible pages without a thumbnail, in screen order.
    pub fn set_visible(&self, visible: HashSet<(u32, u32)>, missing: &[(u32, u32)]) {
        let (lock, cvar) = &*self.shared;
        let mut q = lock.lock().unwrap();
        q.pending.retain(|r| visible.contains(r));
        // Reversed so that the top of the screen, pushed last, is served first.
        for r in missing.iter().rev() {
            if !q.pending.contains(r) {
                q.pending.push(*r);
            }
        }
        q.wanted = visible;
        cvar.notify_all();
    }
}

fn thumb_loop(
    command: &WorkerCommand,
    shared: &(Mutex<Queue>, Condvar),
    on_done: &(dyn Fn(u32, u32, Result<ThumbPixels, String>) + Send + Sync),
) {
    let mut worker: Option<WorkerProcess> = None;
    let mut open_source: Option<u32> = None;
    loop {
        let (source, page, path) = {
            let (lock, cvar) = shared;
            let mut q = lock.lock().unwrap();
            loop {
                if let Some(r) = q.pending.pop() {
                    if q.wanted.contains(&r) {
                        let path = q.sources[r.0 as usize].clone();
                        // Another worker must not take the same page.
                        q.wanted.remove(&r);
                        break (r.0, r.1, path);
                    }
                    continue;
                }
                q = cvar.wait(q).unwrap();
            }
        };
        if worker.as_ref().is_none_or(|w| w.is_dead()) {
            worker = command.spawn().ok();
            open_source = None;
        }
        let Some(w) = worker.as_mut() else {
            on_done(source, page, Err("worker did not start".into()));
            continue;
        };
        let result = (|| {
            if open_source != Some(source) {
                w.request(&Request::Open { path, password: None }, OPEN_TIMEOUT)?;
                open_source = Some(source);
            }
            w.request(
                &Request::Thumbnail {
                    page: page + 1,
                    max_side: THUMB_MAX_SIDE,
                },
                RENDER_TIMEOUT,
            )
        })();
        let result = match result {
            Ok((Response::Thumbnail, png)) => decode_png(&png),
            Ok((other, _)) => Err(format!("unexpected answer {other:?}")),
            Err(e) => {
                open_source = None;
                Err(format!("{e:?}"))
            }
        };
        on_done(source, page, result);
    }
}

fn decode_png(png: &[u8]) -> Result<ThumbPixels, String> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    Ok(SharedPixelBuffer::clone_from_slice(
        img.as_raw(),
        img.width(),
        img.height(),
    ))
}

/// Opens `path` in a fresh worker and returns the page sizes. Blocking: call
/// it from a background thread.
pub fn open_pdf(command: &WorkerCommand, path: &Path) -> Result<Vec<PageDimensions>, String> {
    let mut w = command.spawn().map_err(|e| format!("{e:?}"))?;
    match w.request(
        &Request::Open {
            path: path.to_path_buf(),
            password: None,
        },
        OPEN_TIMEOUT,
    ) {
        Ok((Response::Open { pages, .. }, _)) => Ok(pages),
        Ok((other, _)) => Err(format!("unexpected answer {other:?}")),
        Err(e) => Err(format!("{e:?}")),
    }
}

/// Writes the organized PDF to `out` through the worker's `Organize`
/// request. `on_progress(done)` runs on the calling thread. Blocking.
pub fn organize(
    command: &WorkerCommand,
    sources: Vec<PathBuf>,
    pages: Vec<OrganizePage>,
    out: &Path,
    on_progress: &mut dyn FnMut(u32),
) -> Result<u64, String> {
    let mut w = command.spawn().map_err(|e| format!("{e:?}"))?;
    let tmp = out.with_extension("pdf.part");
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut write_error = None;
    let request = Request::Organize {
        sources: sources
            .into_iter()
            .map(|path| OrganizeSource {
                path,
                password: None,
            })
            .collect(),
        pages,
        creator: "PDF Converter slint-spike".into(),
        output_password: None,
    };
    let answer = w.request_stream(
        &request,
        ORGANIZE_STEP_TIMEOUT,
        &|| false,
        &mut |part| match part {
            StreamPart::Progress { done } => on_progress(done),
            StreamPart::Chunk(bytes) => {
                if write_error.is_none() {
                    if let Err(e) = file.write_all(&bytes) {
                        write_error = Some(e.to_string());
                    }
                }
            }
        },
    );
    drop(file);
    let result = match (answer, write_error) {
        (_, Some(e)) => Err(e),
        (Ok(Response::Organized { bytes }), None) => {
            std::fs::rename(&tmp, out).map_err(|e| e.to_string())?;
            Ok(bytes)
        }
        (Ok(other), None) => Err(format!("unexpected answer {other:?}")),
        (Err(e), None) => Err(format!("{e:?}")),
    };
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
