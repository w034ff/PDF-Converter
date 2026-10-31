//! Worker process pool management (design §5.2).
//!
//! Maintains a pool of worker processes up to a dynamic limit based on
//! logical CPU cores. Workers are spawned lazily on demand. If a worker
//! crashes or times out, it is discarded and replaced on subsequent requests.

use std::ffi::OsString;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

use pdfconv_worker::client::{
    DETECT_TIMEOUT, OPEN_TIMEOUT, ORGANIZE_STEP_TIMEOUT, RENDER_TIMEOUT, WorkerError, WorkerProcess,
};
use pdfconv_worker::protocol::{Request, Response};

/// Maximum number of concurrent worker processes (design §5.2).
pub const MAX_WORKERS: usize = 4;

/// How long a worker keeps a PDF open with no request for it (design §5.2).
pub const HELD_PDF_IDLE_TIME: Duration = Duration::from_secs(2);

/// How many requests a held PDF serves before it is opened afresh. pdfium keeps
/// what it read of each page until the PDF closes, so a long scroll would
/// otherwise grow towards the worker's memory limit (design §5.2).
pub const HELD_PDF_MAX_REQUESTS: u32 = 200;

/// Errors returned by worker pool operations, corresponding to design §6.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerPoolError {
    /// Worker process crashed, died, or failed to spawn (design §6.6 `WorkerCrashed`).
    WorkerCrashed,
    /// Worker process did not answer in time (design §6.6 `WorkerTimeout`).
    WorkerTimeout,
    /// Worker answered with an error code and optional detail.
    Remote {
        code: String,
        detail: Option<String>,
    },
}

impl WorkerPoolError {
    /// The error code string matching design §6.6.
    pub fn code(&self) -> &str {
        match self {
            Self::WorkerCrashed => "WorkerCrashed",
            Self::WorkerTimeout => "WorkerTimeout",
            Self::Remote { code, .. } => code.as_str(),
        }
    }

    /// Additional details about the error, if any.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::WorkerCrashed | Self::WorkerTimeout => None,
            Self::Remote { detail, .. } => detail.as_deref(),
        }
    }
}

impl std::fmt::Display for WorkerPoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkerCrashed => write!(f, "WorkerCrashed"),
            Self::WorkerTimeout => write!(f, "WorkerTimeout"),
            Self::Remote { code, detail } => {
                if let Some(detail) = detail {
                    write!(f, "{code}: {detail}")
                } else {
                    write!(f, "{code}")
                }
            }
        }
    }
}

impl std::error::Error for WorkerPoolError {}

impl From<WorkerError> for WorkerPoolError {
    fn from(err: WorkerError) -> Self {
        match err {
            WorkerError::Spawn(_) | WorkerError::Crashed => Self::WorkerCrashed,
            WorkerError::Timeout => Self::WorkerTimeout,
            WorkerError::Remote { code, detail } => Self::Remote { code, detail },
            // Only `request_stream` returns these, and the pool sends single-answer requests.
            WorkerError::SourceFailed { code, detail, .. } => Self::Remote { code, detail },
            WorkerError::Cancelled => Self::WorkerCrashed,
        }
    }
}

/// Calculates the worker limit for a given logical CPU count: `max(1, min(logical_cpus - 1, MAX_WORKERS))` (design §5.2).
pub fn calculate_worker_limit(logical_cpus: usize) -> usize {
    logical_cpus.saturating_sub(1).clamp(1, MAX_WORKERS)
}

/// Calculates the default worker limit based on the system's available parallelism (design §5.2).
pub fn default_worker_limit() -> usize {
    let logical_cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    calculate_worker_limit(logical_cpus)
}

/// Configuration for creating a [`WorkerPool`].
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub max_workers: usize,
    pub open_timeout: Duration,
    pub render_timeout: Duration,
    pub detect_timeout: Duration,
    /// The longest wait between two answers of an `Organize` request, not for the whole
    /// request, whose length grows with the output (design organize §3).
    pub organize_step_timeout: Duration,
    /// How long an idle worker keeps a PDF open (design §5.2).
    pub held_pdf_idle_time: Duration,
}

impl WorkerPoolConfig {
    /// Creates a configuration with default worker limit and timeouts.
    pub fn new(
        program: impl Into<OsString>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            max_workers: default_worker_limit(),
            open_timeout: OPEN_TIMEOUT,
            render_timeout: RENDER_TIMEOUT,
            detect_timeout: DETECT_TIMEOUT,
            organize_step_timeout: ORGANIZE_STEP_TIMEOUT,
            held_pdf_idle_time: HELD_PDF_IDLE_TIME,
        }
    }

    /// Sets the maximum number of worker processes in the pool.
    pub fn with_max_workers(mut self, max_workers: usize) -> Self {
        self.max_workers = max_workers.max(1);
        self
    }

    /// Sets the timeout for opening PDFs or initial loading.
    pub fn with_open_timeout(mut self, timeout: Duration) -> Self {
        self.open_timeout = timeout;
        self
    }

    /// Sets the timeout for rendering pages and thumbnails.
    pub fn with_render_timeout(mut self, timeout: Duration) -> Self {
        self.render_timeout = timeout;
        self
    }

    /// Sets the timeout for detecting a page's layout, which the analysis
    /// worker uses as well (design §5.2).
    pub fn with_detect_timeout(mut self, timeout: Duration) -> Self {
        self.detect_timeout = timeout;
        self
    }

    /// Sets the longest wait between two answers of an `Organize` request.
    pub fn with_organize_step_timeout(mut self, timeout: Duration) -> Self {
        self.organize_step_timeout = timeout;
        self
    }

    /// Sets how long an idle worker keeps a PDF open.
    pub fn with_held_pdf_idle_time(mut self, time: Duration) -> Self {
        self.held_pdf_idle_time = time;
        self
    }

    /// How long to wait for the answer to `request` (design §5.2). Every
    /// request is listed, so that a new one cannot fall to a default value
    /// without the compiler noticing.
    pub fn timeout_for(&self, request: &Request) -> Duration {
        match request {
            Request::Hello | Request::Open { .. } | Request::Close => self.open_timeout,
            Request::Render { .. }
            | Request::Thumbnail { .. }
            | Request::PageImages { .. }
            | Request::ImageOnTop { .. }
            | Request::RenderRegion { .. } => self.render_timeout,
            Request::Detect { .. } => self.detect_timeout,
            // The wait between two answers of the stream, not for the whole request.
            Request::Organize { .. } => self.organize_step_timeout,
            #[cfg(feature = "test-hooks")]
            Request::AllocateForTest { .. } | Request::CrashForTest | Request::HangForTest => {
                self.open_timeout
            }
        }
    }
}

/// A worker waiting for a request.
struct IdleWorker {
    worker: WorkerProcess,
    /// The PDF it still has open, as given to [`PooledWorker::set_held_pdf`].
    held_pdf: Option<u64>,
    held_requests: u32,
    since: Instant,
}

struct PoolState {
    idle: Vec<IdleWorker>,
    total_workers: usize,
    shutting_down: bool,
    /// Whether [`close_idle_pdfs`] is running; it ends when no idle worker holds a PDF.
    closer_running: bool,
}

struct PoolInner {
    config: WorkerPoolConfig,
    state: Mutex<PoolState>,
    available: Condvar,
}

/// A pool of worker processes (design §5.2).
#[derive(Clone)]
pub struct WorkerPool {
    inner: Arc<PoolInner>,
}

impl WorkerPool {
    /// Creates a new worker pool with the given configuration.
    pub fn new(config: WorkerPoolConfig) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                config,
                state: Mutex::new(PoolState {
                    idle: Vec::new(),
                    total_workers: 0,
                    shutting_down: false,
                    closer_running: false,
                }),
                available: Condvar::new(),
            }),
        }
    }

    /// The maximum number of concurrent workers allowed in this pool.
    pub fn max_workers(&self) -> usize {
        self.inner.config.max_workers
    }

    /// The timeout for opening documents.
    pub fn open_timeout(&self) -> Duration {
        self.inner.config.open_timeout
    }

    /// The timeout for rendering pages or thumbnails.
    pub fn render_timeout(&self) -> Duration {
        self.inner.config.render_timeout
    }

    /// The timeout for detecting a page's layout.
    pub fn detect_timeout(&self) -> Duration {
        self.inner.config.detect_timeout
    }

    /// The longest wait between two answers of an `Organize` request.
    pub fn organize_step_timeout(&self) -> Duration {
        self.inner.config.organize_step_timeout
    }

    /// How long to wait for the answer to `request`; see
    /// [`WorkerPoolConfig::timeout_for`].
    pub fn timeout_for(&self, request: &Request) -> Duration {
        self.inner.config.timeout_for(request)
    }

    /// Starts a worker that belongs to the caller instead of the pool, with
    /// the pool's program and arguments. The pool neither counts nor reuses
    /// it, so it is not bound by the worker limit (design §5.2, the analysis
    /// worker).
    ///
    /// # Errors
    ///
    /// Returns [`WorkerPoolError::WorkerCrashed`] if the process cannot be
    /// started.
    pub fn spawn_dedicated(&self) -> Result<WorkerProcess, WorkerPoolError> {
        WorkerProcess::spawn(&self.inner.config.program, &self.inner.config.args)
            .map_err(WorkerPoolError::from)
    }

    /// Borrows a worker from the pool with no PDF open, blocking if all
    /// workers are in use.
    ///
    /// Spawns a new worker lazily if below the configured worker limit.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerPoolError::WorkerCrashed`] if the pool was shut down or
    /// if spawning the worker process fails.
    pub fn acquire(&self) -> Result<PooledWorker, WorkerPoolError> {
        loop {
            let mut worker = self.acquire_preferring(|_| false)?;
            if worker.held_pdf.take().is_none() {
                return Ok(worker);
            }
            if worker.send(&Request::Close).is_ok() {
                return Ok(worker);
            }
        }
    }

    /// Borrows a worker to use on the PDF `pdf_id`, one that still has it open
    /// if there is one; see [`PooledWorker::held_pdf`]. The worker may hold
    /// another PDF, which the next `Open` replaces.
    ///
    /// # Errors
    ///
    /// As [`WorkerPool::acquire`].
    pub fn acquire_for_pdf(&self, pdf_id: u64) -> Result<PooledWorker, WorkerPoolError> {
        self.acquire_preferring(|held| held == Some(pdf_id))
    }

    /// Borrows the last idle worker `preferred` accepts by the PDF it holds,
    /// else the last that holds none, else the last idle one, so that a PDF
    /// another request may want goes last; else a new one.
    fn acquire_preferring(
        &self,
        preferred: impl Fn(Option<u64>) -> bool,
    ) -> Result<PooledWorker, WorkerPoolError> {
        let mut state = self.inner.state.lock().expect("worker pool mutex poisoned");
        loop {
            if state.shutting_down {
                return Err(WorkerPoolError::WorkerCrashed);
            }
            let idle = &state.idle;
            let index = idle
                .iter()
                .rposition(|worker| preferred(worker.held_pdf))
                .or_else(|| idle.iter().rposition(|worker| worker.held_pdf.is_none()))
                .or(idle.len().checked_sub(1));
            if let Some(index) = index {
                let IdleWorker {
                    worker,
                    held_pdf,
                    held_requests,
                    ..
                } = state.idle.remove(index);
                return Ok(PooledWorker {
                    worker: Some(worker),
                    held_pdf,
                    held_requests,
                    pool: Arc::clone(&self.inner),
                });
            }
            if state.total_workers < self.inner.config.max_workers {
                state.total_workers += 1;
                drop(state);
                match WorkerProcess::spawn(&self.inner.config.program, &self.inner.config.args) {
                    Ok(worker) => {
                        return Ok(PooledWorker {
                            worker: Some(worker),
                            held_pdf: None,
                            held_requests: 0,
                            pool: Arc::clone(&self.inner),
                        });
                    }
                    Err(e) => {
                        let mut state =
                            self.inner.state.lock().expect("worker pool mutex poisoned");
                        state.total_workers -= 1;
                        self.inner.available.notify_one();
                        return Err(WorkerPoolError::from(e));
                    }
                }
            }
            state = self
                .inner
                .available
                .wait(state)
                .expect("worker pool condvar wait poisoned");
        }
    }

    /// Shuts down all idle workers in the pool and signals waiting threads.
    pub fn shutdown(&self) {
        let mut state = self.inner.state.lock().expect("worker pool mutex poisoned");
        state.shutting_down = true;
        state.idle.clear();
        self.inner.available.notify_all();
    }
}

/// An RAII guard for a worker borrowed from [`WorkerPool`].
///
/// When dropped, if the worker crashed or timed out, it is discarded.
/// Otherwise, it is returned to the pool for subsequent requests.
pub struct PooledWorker {
    worker: Option<WorkerProcess>,
    held_pdf: Option<u64>,
    held_requests: u32,
    pool: Arc<PoolInner>,
}

impl PooledWorker {
    /// The ID of the PDF this worker has open, as far as [`set_held_pdf`] was told.
    ///
    /// [`set_held_pdf`]: PooledWorker::set_held_pdf
    pub fn held_pdf(&self) -> Option<u64> {
        self.held_pdf
    }

    /// Says which PDF stays open in this worker when it goes back to the pool,
    /// until [`HELD_PDF_IDLE_TIME`] passes with no request. `pdf_id` must never
    /// name another file: a PDF stays readable as it was opened after its file
    /// is replaced.
    pub fn set_held_pdf(&mut self, pdf_id: Option<u64>) {
        self.held_pdf = pdf_id;
        self.held_requests = 0;
    }

    /// Whether one more request may use the PDF `pdf_id` this worker holds,
    /// which counts as that request. If not, the caller opens it.
    pub fn use_held_pdf(&mut self, pdf_id: u64) -> bool {
        let usable = self.held_pdf == Some(pdf_id) && self.held_requests < HELD_PDF_MAX_REQUESTS;
        if usable {
            self.held_requests += 1;
        }
        usable
    }

    /// Sends `request` to the worker with the specified timeout.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerPoolError::WorkerCrashed`] if the worker crashed or exited,
    /// [`WorkerPoolError::WorkerTimeout`] if it timed out, or
    /// [`WorkerPoolError::Remote`] if it answered with an error.
    pub fn request(
        &mut self,
        request: &Request,
        timeout: Duration,
    ) -> Result<(Response, Vec<u8>), WorkerPoolError> {
        self.worker
            .as_mut()
            .expect("pooled worker present")
            .request(request, timeout)
            .map_err(WorkerPoolError::from)
    }

    /// Sends `request` to the worker with the timeout that
    /// [`WorkerPoolConfig::timeout_for`] gives for its type.
    pub fn send(&mut self, request: &Request) -> Result<(Response, Vec<u8>), WorkerPoolError> {
        let timeout = self.pool.config.timeout_for(request);
        self.request(request, timeout)
    }
}

impl Deref for PooledWorker {
    type Target = WorkerProcess;

    fn deref(&self) -> &Self::Target {
        self.worker.as_ref().expect("pooled worker present")
    }
}

impl DerefMut for PooledWorker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.worker.as_mut().expect("pooled worker present")
    }
}

impl Drop for PooledWorker {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let mut state = self.pool.state.lock().expect("worker pool mutex poisoned");
            if worker.is_dead() || state.shutting_down {
                drop(worker);
                state.total_workers -= 1;
            } else {
                let held_pdf = self.held_pdf;
                state.idle.push(IdleWorker {
                    worker,
                    held_pdf,
                    held_requests: self.held_requests,
                    since: Instant::now(),
                });
                if held_pdf.is_some() && !state.closer_running {
                    state.closer_running = true;
                    let pool = Arc::downgrade(&self.pool);
                    std::thread::spawn(move || close_idle_pdfs(&pool));
                }
            }
            self.pool.available.notify_one();
        }
    }
}

/// Closes the PDFs that idle workers have held for `held_pdf_idle_time`, and
/// ends once no idle worker holds one. A worker being closed counts as
/// borrowed, so a request that wants it waits for the close.
fn close_idle_pdfs(pool: &Weak<PoolInner>) {
    loop {
        let Some(time) = pool.upgrade().map(|inner| inner.config.held_pdf_idle_time) else {
            return;
        };
        std::thread::sleep(time / 2);
        let Some(inner) = pool.upgrade() else {
            return;
        };
        let mut state = inner.state.lock().expect("worker pool mutex poisoned");
        let (expired, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut state.idle)
            .into_iter()
            .partition(|idle| idle.held_pdf.is_some() && idle.since.elapsed() >= time);
        state.idle = kept;
        if expired.is_empty() && state.idle.iter().all(|idle| idle.held_pdf.is_none()) {
            state.closer_running = false;
            return;
        }
        drop(state);
        for idle in expired {
            let mut worker = PooledWorker {
                worker: Some(idle.worker),
                held_pdf: None,
                held_requests: 0,
                pool: Arc::clone(&inner),
            };
            // A worker that fails here is dropped by `PooledWorker`'s `Drop`.
            let _ = worker.send(&Request::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use pdfconv_worker::protocol::{RectPt, RenderColor, RenderFormat};

    use super::*;

    const OPEN: Duration = Duration::from_secs(1);
    const RENDER: Duration = Duration::from_secs(2);
    const DETECT: Duration = Duration::from_secs(3);
    const ORGANIZE: Duration = Duration::from_secs(4);

    fn distinct_timeouts() -> WorkerPoolConfig {
        WorkerPoolConfig::new("worker", ["arg"])
            .with_open_timeout(OPEN)
            .with_render_timeout(RENDER)
            .with_detect_timeout(DETECT)
            .with_organize_step_timeout(ORGANIZE)
    }

    #[test]
    fn the_timeout_follows_the_kind_of_request() {
        let config = distinct_timeouts();
        let open = Request::Open {
            path: "a.pdf".into(),
            password: None,
        };
        let render = Request::Render {
            page: 1,
            dpi: 72,
            format: RenderFormat::Png,
            color: RenderColor::Color,
        };
        let region = Request::RenderRegion {
            page: 1,
            rect_pt: RectPt {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            image_index: None,
            dpi: 72,
            format: RenderFormat::Png,
            color: RenderColor::Color,
            photo_native: false,
            photo_only: false,
        };
        let cases = [
            (Request::Hello, OPEN),
            (open, OPEN),
            (Request::Close, OPEN),
            (render, RENDER),
            (
                Request::Thumbnail {
                    page: 1,
                    max_side: 10,
                },
                RENDER,
            ),
            (Request::PageImages { page: 1 }, RENDER),
            (
                Request::ImageOnTop {
                    page: 1,
                    image_index: 0,
                },
                RENDER,
            ),
            (region, RENDER),
            (Request::Detect { page: 1 }, DETECT),
            (
                Request::Organize {
                    sources: Vec::new(),
                    pages: Vec::new(),
                    creator: String::new(),
                    output_password: None,
                },
                ORGANIZE,
            ),
        ];
        for (request, expected) in cases {
            assert_eq!(config.timeout_for(&request), expected, "{request:?}");
        }
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn the_test_requests_wait_as_long_as_an_open() {
        let config = distinct_timeouts();
        for request in [
            Request::AllocateForTest { mebibytes: 1 },
            Request::CrashForTest,
            Request::HangForTest,
        ] {
            assert_eq!(config.timeout_for(&request), OPEN, "{request:?}");
        }
    }

    #[test]
    fn the_detect_timeout_defaults_to_the_workers_constant() {
        let config = WorkerPoolConfig::new("worker", ["arg"]);
        assert_eq!(config.detect_timeout, DETECT_TIMEOUT);
    }

    #[test]
    fn the_organize_step_timeout_defaults_to_the_workers_constant() {
        let config = WorkerPoolConfig::new("worker", ["arg"]);
        assert_eq!(config.organize_step_timeout, ORGANIZE_STEP_TIMEOUT);
    }

    #[test]
    fn calculates_limits_based_on_cpu_counts() {
        assert_eq!(calculate_worker_limit(0), 1);
        assert_eq!(calculate_worker_limit(1), 1);
        assert_eq!(calculate_worker_limit(2), 1);
        assert_eq!(calculate_worker_limit(3), 2);
        assert_eq!(calculate_worker_limit(4), 3);
        assert_eq!(calculate_worker_limit(5), 4);
        assert_eq!(calculate_worker_limit(8), 4);
        assert_eq!(calculate_worker_limit(64), 4);
    }

    #[test]
    fn maps_worker_errors_to_pool_errors() {
        let crashed = WorkerPoolError::from(WorkerError::Crashed);
        assert_eq!(crashed, WorkerPoolError::WorkerCrashed);
        assert_eq!(crashed.code(), "WorkerCrashed");
        assert_eq!(crashed.detail(), None);

        let timeout = WorkerPoolError::from(WorkerError::Timeout);
        assert_eq!(timeout, WorkerPoolError::WorkerTimeout);
        assert_eq!(timeout.code(), "WorkerTimeout");
        assert_eq!(timeout.detail(), None);

        let remote = WorkerPoolError::from(WorkerError::Remote {
            code: "PasswordProtected".into(),
            detail: Some("secret".into()),
        });
        assert_eq!(
            remote,
            WorkerPoolError::Remote {
                code: "PasswordProtected".into(),
                detail: Some("secret".into()),
            }
        );
        assert_eq!(remote.code(), "PasswordProtected");
        assert_eq!(remote.detail(), Some("secret"));
    }
}
