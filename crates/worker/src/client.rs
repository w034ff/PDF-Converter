//! The main-process side of one worker: starts it, sends requests and turns
//! a crash or a missing answer into an error (design §5.2). The pool that
//! decides how many workers run lives in `src-tauri` (design §5.2).

use std::ffi::OsStr;
use std::io::{self, BufReader, BufWriter};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{Request, Response, read_message, write_message};

/// Why a request to a worker failed.
#[derive(Debug)]
pub enum WorkerError {
    /// The worker process could not be started.
    Spawn(io::Error),
    /// The worker ended, or sent something unreadable, before answering.
    Crashed,
    /// The worker did not answer in time and was killed.
    Timeout,
    /// The worker answered with an error (a code of design §6.6).
    Remote {
        code: String,
        detail: Option<String>,
    },
    /// A source PDF of an `Organize` request could not be used (design
    /// organize §3). The worker is still usable.
    SourceFailed {
        code: String,
        detail: Option<String>,
        /// Index of the source in the request.
        source: u32,
    },
    /// The caller cancelled a streamed request and the worker was killed.
    Cancelled,
}

/// How long to wait for a worker to open a PDF or load pdfium (design §5.2).
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for a worker to render a page or thumbnail (design §5.2).
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a worker to detect a page's layout, including loading
/// onnxruntime and the model on the first `Detect` (design §5.2).
pub const DETECT_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait between two answers of an `Organize` request (design
/// §5.2). It bounds the gap between answers, not the whole request, whose
/// length grows with the output. The gap that matters is the longest stretch
/// without an answer: saving the copied pages and rewriting them with lopdf,
/// which must fit within it for an output of the largest size the main process
/// accepts.
pub const ORGANIZE_STEP_TIMEOUT: Duration = Duration::from_secs(120);

/// How often [`WorkerProcess::request_stream`] checks whether to cancel while
/// it waits for an answer; this bounds how late a cancellation takes effect.
pub const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// An intermediate answer of a streamed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamPart {
    /// Pages processed so far (`OrganizeProgress`).
    Progress { done: u32 },
    /// A part of the output (`OrganizeChunk`).
    Chunk(Vec<u8>),
}

type Message = io::Result<Option<(Response, Vec<u8>)>>;

/// One running worker process. Dropping it kills the process.
pub struct WorkerProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    responses: Receiver<Message>,
    /// Set once the process crashed or timed out; it is never asked again.
    dead: bool,
    #[cfg(windows)]
    _job: crate::windows_job::Job,
}

impl WorkerProcess {
    /// Starts `program` with `args`. On Windows the process is put in a job
    /// object with the memory limit and kill-on-close (design §5.3) before
    /// this returns, so no request runs outside the limit.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError::Spawn`] if the process or its job object
    /// cannot be created.
    pub fn spawn<I, S>(program: &OsStr, args: I) -> Result<Self, WorkerError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(WorkerError::Spawn)?;

        #[cfg(windows)]
        let job =
            match crate::windows_job::Job::with_memory_limit(crate::server::WORKER_MEMORY_LIMIT)
                .and_then(|job| job.assign(&child).map(|()| job))
            {
                Ok(job) => job,
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(WorkerError::Spawn(e));
                }
            };

        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(WorkerError::Spawn(io::Error::other(
                "worker pipes were not created",
            )));
        };
        let (sender, responses) = mpsc::channel::<Message>();
        // A blocking read cannot be given a timeout, so a thread reads and
        // the requester waits on the channel with one.
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let message = read_message(&mut reader);
                let finished = !matches!(message, Ok(Some(_)));
                if sender.send(message).is_err() || finished {
                    break;
                }
            }
        });

        Ok(Self {
            child,
            stdin: BufWriter::new(stdin),
            responses,
            dead: false,
            #[cfg(windows)]
            _job: job,
        })
    }

    /// Sends `request` and waits up to `timeout` for the answer.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Timeout`] if no answer came in time (the worker is
    /// killed), [`WorkerError::Crashed`] if the worker ended or its answer
    /// was unreadable, [`WorkerError::Remote`] if it answered with an error.
    /// After `Timeout` or `Crashed` every later request fails with `Crashed`.
    pub fn request(
        &mut self,
        request: &Request,
        timeout: Duration,
    ) -> Result<(Response, Vec<u8>), WorkerError> {
        if self.dead {
            return Err(WorkerError::Crashed);
        }
        if write_message(&mut self.stdin, request, &[]).is_err() {
            return Err(self.fail(WorkerError::Crashed));
        }
        match self.responses.recv_timeout(timeout) {
            Ok(Ok(Some((Response::Error { code, detail }, _)))) => {
                Err(WorkerError::Remote { code, detail })
            }
            Ok(Ok(Some(answer))) => Ok(answer),
            Ok(Ok(None) | Err(_)) | Err(RecvTimeoutError::Disconnected) => {
                Err(self.fail(WorkerError::Crashed))
            }
            Err(RecvTimeoutError::Timeout) => Err(self.fail(WorkerError::Timeout)),
        }
    }

    /// Sends `request` and passes every intermediate answer to `on_part`
    /// until the last answer arrives, which is returned.
    ///
    /// `step_timeout` limits the wait for each answer, not the whole request.
    /// `should_cancel` is checked every [`CANCEL_POLL_INTERVAL`] while waiting.
    ///
    /// # Errors
    ///
    /// [`WorkerError::Cancelled`] if `should_cancel` returned true,
    /// [`WorkerError::Timeout`] if an answer took longer than `step_timeout`,
    /// and [`WorkerError::Crashed`] if the worker ended or sent something
    /// unreadable; the worker is killed and never used again in these three
    /// cases. [`WorkerError::SourceFailed`] and [`WorkerError::Remote`] end the
    /// request but leave the worker usable.
    pub fn request_stream(
        &mut self,
        request: &Request,
        step_timeout: Duration,
        should_cancel: &dyn Fn() -> bool,
        on_part: &mut dyn FnMut(StreamPart),
    ) -> Result<Response, WorkerError> {
        if self.dead {
            return Err(WorkerError::Crashed);
        }
        if write_message(&mut self.stdin, request, &[]).is_err() {
            return Err(self.fail(WorkerError::Crashed));
        }
        loop {
            let deadline = Instant::now() + step_timeout;
            let message = loop {
                if should_cancel() {
                    return Err(self.fail(WorkerError::Cancelled));
                }
                let wait = deadline
                    .saturating_duration_since(Instant::now())
                    .min(CANCEL_POLL_INTERVAL);
                match self.responses.recv_timeout(wait) {
                    Ok(message) => break message,
                    Err(RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                        return Err(self.fail(WorkerError::Timeout));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(self.fail(WorkerError::Crashed));
                    }
                }
            };
            match message {
                Ok(Some((Response::OrganizeProgress { done }, _))) => {
                    on_part(StreamPart::Progress { done });
                }
                Ok(Some((Response::OrganizeChunk, body))) => on_part(StreamPart::Chunk(body)),
                Ok(Some((
                    Response::OrganizeFailed {
                        code,
                        detail,
                        source,
                    },
                    _,
                ))) => {
                    return Err(WorkerError::SourceFailed {
                        code,
                        detail,
                        source,
                    });
                }
                Ok(Some((Response::Error { code, detail }, _))) => {
                    return Err(WorkerError::Remote { code, detail });
                }
                Ok(Some((last, _))) => return Ok(last),
                Ok(None) | Err(_) => return Err(self.fail(WorkerError::Crashed)),
            }
        }
    }

    /// The operating system's id of the worker process.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Whether this worker process has crashed, timed out, or stopped.
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    fn fail(&mut self, error: WorkerError) -> WorkerError {
        self.dead = true;
        self.stop();
        error
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
