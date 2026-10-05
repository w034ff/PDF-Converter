//! Worker process pool management (design §5.2).
//!
//! Maintains a pool of worker processes up to a dynamic limit based on
//! logical CPU cores. Workers are spawned lazily on demand. If a worker
//! crashes or times out, it is discarded and replaced on subsequent requests.

use std::ffi::OsString;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use pdfconv_worker::client::{OPEN_TIMEOUT, RENDER_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{Request, Response};

/// Maximum number of concurrent worker processes (design §5.2).
pub const MAX_WORKERS: usize = 4;

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
}

struct PoolState {
    idle: Vec<WorkerProcess>,
    total_workers: usize,
    shutting_down: bool,
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

    /// Borrows a worker from the pool, blocking if all workers are in use.
    ///
    /// Spawns a new worker lazily if below the configured worker limit.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerPoolError::WorkerCrashed`] if the pool was shut down or
    /// if spawning the worker process fails.
    pub fn acquire(&self) -> Result<PooledWorker, WorkerPoolError> {
        let mut state = self.inner.state.lock().expect("worker pool mutex poisoned");
        loop {
            if state.shutting_down {
                return Err(WorkerPoolError::WorkerCrashed);
            }
            if let Some(worker) = state.idle.pop() {
                return Ok(PooledWorker {
                    worker: Some(worker),
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
    pool: Arc<PoolInner>,
}

impl PooledWorker {
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

    /// Sends `request` to the worker using the default timeout for the request type.
    pub fn send(&mut self, request: &Request) -> Result<(Response, Vec<u8>), WorkerPoolError> {
        let timeout = match request {
            Request::Render { .. } | Request::Thumbnail { .. } => self.pool.config.render_timeout,
            _ => self.pool.config.open_timeout,
        };
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
                state.idle.push(worker);
            }
            self.pool.available.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
