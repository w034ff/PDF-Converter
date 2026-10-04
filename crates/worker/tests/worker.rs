//! Starts the standalone worker binary against the pdfium fetched by
//! `npm run pdfium:fetch` (design §8.1).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pdfconv_worker::client::{WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{Request, Response};

const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/pdfium")
        .join(os_dir)
}

fn start() -> WorkerProcess {
    WorkerProcess::spawn(
        env!("CARGO_BIN_EXE_pdfconv-worker").as_ref(),
        [library_dir()],
    )
    .expect("worker should start")
}

#[test]
fn loads_pdfium_and_answers_repeatedly() {
    let mut worker = start();
    for _ in 0..3 {
        let (response, body) = worker.request(&Request::Hello, ANSWER_TIMEOUT).unwrap();
        assert_eq!(response, Response::Hello);
        assert!(body.is_empty());
    }
}

#[test]
fn reports_a_missing_library_as_an_error_without_crashing() {
    let empty = std::env::temp_dir().join("pdfconv-no-pdfium-here");
    let mut worker =
        WorkerProcess::spawn(env!("CARGO_BIN_EXE_pdfconv-worker").as_ref(), [&empty]).unwrap();
    match worker.request(&Request::Hello, ANSWER_TIMEOUT) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PdfiumUnavailable"),
        other => panic!("expected PdfiumUnavailable, got {other:?}"),
    }
    // The worker is still alive and answers the next request the same way.
    assert!(matches!(
        worker.request(&Request::Hello, ANSWER_TIMEOUT),
        Err(WorkerError::Remote { .. })
    ));
}

#[test]
fn exits_when_the_main_process_closes_stdin() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pdfconv-worker"))
        .arg(library_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    let deadline = Instant::now() + ANSWER_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not exit after stdin closed"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut rest = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty());
}

#[cfg(feature = "test-hooks")]
mod hooks {
    use super::*;

    #[test]
    fn a_crash_is_reported_and_the_worker_is_not_used_again() {
        let mut worker = start();
        assert!(matches!(
            worker.request(&Request::CrashForTest, ANSWER_TIMEOUT),
            Err(WorkerError::Crashed)
        ));
        assert!(matches!(
            worker.request(&Request::Hello, ANSWER_TIMEOUT),
            Err(WorkerError::Crashed)
        ));
        // A fresh worker works after the crash.
        assert!(start().request(&Request::Hello, ANSWER_TIMEOUT).is_ok());
    }

    #[test]
    fn a_hang_times_out_and_kills_the_worker() {
        let mut worker = start();
        let started = Instant::now();
        assert!(matches!(
            worker.request(&Request::HangForTest, Duration::from_secs(1)),
            Err(WorkerError::Timeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(matches!(
            worker.request(&Request::Hello, ANSWER_TIMEOUT),
            Err(WorkerError::Crashed)
        ));
    }

    #[test]
    fn allocations_within_the_limit_succeed() {
        let mut worker = start();
        let (response, _) = worker
            .request(&Request::AllocateForTest { mebibytes: 256 }, ANSWER_TIMEOUT)
            .unwrap();
        assert_eq!(response, Response::Allocated);
    }

    #[test]
    fn exceeding_the_memory_limit_ends_only_the_worker() {
        let mut worker = start();
        // Load pdfium first so the limit is shown to cover a worker that
        // has pdfium in memory, as in real use.
        worker.request(&Request::Hello, ANSWER_TIMEOUT).unwrap();
        assert!(matches!(
            worker.request(
                &Request::AllocateForTest { mebibytes: 3072 },
                ANSWER_TIMEOUT
            ),
            Err(WorkerError::Crashed)
        ));
        assert!(start().request(&Request::Hello, ANSWER_TIMEOUT).is_ok());
    }
}
