//! The main-process side of one worker: starts it, sends requests and turns
//! a crash or a missing answer into an error (design §5.2). The pool that
//! decides how many workers run lives in `src-tauri` (design §5.2).

use std::ffi::OsStr;
use std::io::{self, BufReader, BufWriter};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

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
}

/// How long to wait for a worker to open a PDF or load pdfium (design §5.2).
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for a worker to render a page or thumbnail (design §5.2).
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(60);

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
