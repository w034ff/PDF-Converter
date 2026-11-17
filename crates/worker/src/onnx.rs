//! The onnxruntime environment of a worker, shared by layout detection and OCR
//! (design §4.7, design ocr §6).
//!
//! onnxruntime allows one environment per process, so the worker creates it
//! once, on the first request that needs it, and every session is built on it.

use std::path::{Path, PathBuf};

use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

/// Creates the environment on first use and keeps it, or keeps why it could
/// not be created.
pub(crate) struct Runtime {
    library: PathBuf,
    state: State,
}

enum State {
    NotLoaded,
    Ready(Environment),
    /// Creating the environment failed once; it is never tried again. A
    /// second `ort::init_from` after one that failed to load the library
    /// brings the process down (design ocr §6).
    Unavailable(String),
}

impl Runtime {
    /// `library` is the onnxruntime library file; it is not touched until
    /// [`Runtime::environment`].
    pub(crate) fn new(library: &Path) -> Self {
        Self {
            library: library.to_path_buf(),
            state: State::NotLoaded,
        }
    }

    /// The environment, created on the first call.
    ///
    /// # Errors
    ///
    /// Why onnxruntime could not be loaded or its environment created, the
    /// same text on every call after the first failure.
    pub(crate) fn environment(&mut self) -> Result<&Environment, String> {
        if matches!(self.state, State::NotLoaded) {
            self.state = match create_environment(&self.library) {
                Ok(environment) => State::Ready(environment),
                Err(detail) => State::Unavailable(detail),
            };
        }
        match &self.state {
            State::Ready(environment) => Ok(environment),
            State::Unavailable(detail) => Err(detail.clone()),
            State::NotLoaded => Err("the environment was not stored".into()),
        }
    }
}

/// Loads onnxruntime from its full path, which keeps another copy on the
/// system's search path from being used (design §5.4).
fn create_environment(library: &Path) -> Result<Environment, String> {
    ort::init_from(library)
        .map_err(|e| e.to_string())?
        // The Windows build registers its event provider as Microsoft telemetry and emits
        // events for Windows to collect unless they are turned off here (NFR-01, design §8.5).
        .with_telemetry(false)
        .build()
        .map_err(|e| e.to_string())
}

/// Builds a session for `model` with the settings every model of the worker
/// uses (design §4.7, セッション), running one inference on at most
/// `max_threads` threads.
pub(crate) fn load_session(
    environment: &Environment,
    model: &Path,
    max_threads: usize,
) -> Result<Session, String> {
    let logical_cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let build = || -> ort::Result<Session> {
        Session::builder(environment)?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(intra_threads(max_threads, logical_cpus))?
            .with_inter_threads(1)?
            // Idle threads that spin slow inference down (design §4.7, セッション).
            .with_intra_op_spinning(false)?
            // The arena keeps the largest allocation it has made for the life of the session,
            // which counts against WORKER_MEMORY_LIMIT while the worker goes on to other
            // requests; freed memory goes back to the allocator instead (design §4.7, セッション).
            .with_execution_providers([ort::ep::CPU::default()
                .with_arena_allocator(false)
                .build()])?
            .commit_from_file(model)
    };
    build().map_err(|e| e.to_string())
}

/// Threads for one inference on a machine with `logical_cpus` logical CPUs:
/// at most `max_threads`, leaving one logical CPU free.
fn intra_threads(max_threads: usize, logical_cpus: usize) -> usize {
    max_threads.min(logical_cpus.saturating_sub(1)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intra_threads_leave_one_cpu_and_stay_within_the_maximum() {
        assert_eq!(intra_threads(6, 0), 1);
        assert_eq!(intra_threads(6, 1), 1);
        assert_eq!(intra_threads(6, 2), 1);
        assert_eq!(intra_threads(6, 4), 3);
        assert_eq!(intra_threads(6, 7), 6);
        assert_eq!(intra_threads(6, 32), 6);
    }
}
