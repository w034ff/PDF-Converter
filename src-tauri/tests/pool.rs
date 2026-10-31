//! Integration tests for [`WorkerPool`] (design §5.2).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(feature = "test-hooks")]
use pdf_converter_lib::worker_pool::WorkerPoolError;
use pdf_converter_lib::worker_pool::{HELD_PDF_MAX_REQUESTS, WorkerPool, WorkerPoolConfig};
use pdfconv_worker::WORKER_FLAG;
use pdfconv_worker::protocol::{Request, Response};

fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdfium")
        .join(os_dir)
}

/// The onnxruntime library and the layout model fetched by
/// `npm run layout:fetch`, the worker's last two arguments (design §5.4).
fn layout_args() -> [String; 2] {
    let onnxruntime = if cfg!(windows) {
        "onnxruntime/windows/onnxruntime.dll"
    } else {
        "onnxruntime/linux/libonnxruntime.so"
    };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest_dir.join(onnxruntime),
        manifest_dir.join("layout-model/PP-DocLayout_plus-L-int8.onnx"),
    ]
    .map(|path| path.to_string_lossy().into_owned())
}

fn test_pool_config(max_workers: usize) -> WorkerPoolConfig {
    WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ]
        .into_iter()
        .chain(layout_args()),
    )
    .with_max_workers(max_workers)
    .with_open_timeout(Duration::from_secs(30))
    .with_render_timeout(Duration::from_secs(60))
}

#[test]
fn worker_count_does_not_exceed_limit_under_concurrent_load() {
    const POOL_LIMIT: usize = 2;
    const NUM_THREADS: usize = 8;
    const ITERATIONS_PER_THREAD: usize = 5;

    let pool = WorkerPool::new(test_pool_config(POOL_LIMIT));
    let active_ids = Arc::new(Mutex::new(HashSet::new()));
    let max_concurrent_observed = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..NUM_THREADS {
        let pool = pool.clone();
        let active_ids = Arc::clone(&active_ids);
        let max_concurrent_observed = Arc::clone(&max_concurrent_observed);

        handles.push(thread::spawn(move || {
            for _ in 0..ITERATIONS_PER_THREAD {
                let mut worker = pool.acquire().expect("acquire should succeed");
                let id = worker.id();

                {
                    let mut set = active_ids.lock().unwrap();
                    set.insert(id);
                    let count = set.len();
                    max_concurrent_observed.fetch_max(count, Ordering::SeqCst);
                    assert!(
                        count <= POOL_LIMIT,
                        "concurrent workers {count} exceeded limit {POOL_LIMIT}"
                    );
                }

                // Verify worker actually works
                let (resp, _) = worker
                    .request(&Request::Hello, Duration::from_secs(10))
                    .expect("Hello should succeed");
                assert_eq!(resp, Response::Hello);

                thread::sleep(Duration::from_millis(20));

                {
                    let mut set = active_ids.lock().unwrap();
                    set.remove(&id);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("thread should join cleanly");
    }

    let peak = max_concurrent_observed.load(Ordering::SeqCst);
    println!("Peak concurrent workers observed: {peak} (limit: {POOL_LIMIT})");
    assert!(peak <= POOL_LIMIT);
    assert!(peak >= 1);
}

#[cfg(feature = "test-hooks")]
fn acquire_with_timeout(
    pool: &WorkerPool,
    timeout: Duration,
) -> Result<pdf_converter_lib::worker_pool::PooledWorker, String> {
    let pool = pool.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let res = pool.acquire();
        let _ = tx.send(res);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(worker)) => Ok(worker),
        Ok(Err(e)) => Err(format!("acquire error: {e}")),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err("acquire timed out waiting for worker slot".into())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("acquire thread exited unexpectedly".into())
        }
    }
}

#[cfg(feature = "test-hooks")]
#[test]
fn recovers_after_worker_crashes() {
    let pool = WorkerPool::new(test_pool_config(1));

    let mut worker = pool.acquire().expect("acquire should succeed");
    let res = worker.request(&Request::CrashForTest, Duration::from_secs(10));
    assert_eq!(res.unwrap_err(), WorkerPoolError::WorkerCrashed);
    // Worker is dropped here and discarded by pool.
    drop(worker);

    // Next acquire should spawn a fresh worker within timeout (not block forever if slot leaked)
    let mut next_worker = acquire_with_timeout(&pool, Duration::from_secs(10))
        .expect("acquire after crash should succeed");
    let (resp, _) = next_worker
        .request(&Request::Hello, Duration::from_secs(10))
        .expect("Hello should succeed on fresh worker");
    assert_eq!(resp, Response::Hello);
}

#[cfg(feature = "test-hooks")]
#[test]
fn recovers_after_worker_times_out() {
    let pool = WorkerPool::new(test_pool_config(1));

    let mut worker = pool.acquire().expect("acquire should succeed");
    // Short timeout so the test runs quickly
    let res = worker.request(&Request::HangForTest, Duration::from_millis(300));
    assert_eq!(res.unwrap_err(), WorkerPoolError::WorkerTimeout);
    // Worker is dropped here and discarded by pool.
    drop(worker);

    // Next acquire should spawn a fresh worker within timeout (not block forever if slot leaked)
    let mut next_worker = acquire_with_timeout(&pool, Duration::from_secs(10))
        .expect("acquire after timeout should succeed");
    let (resp, _) = next_worker
        .request(&Request::Hello, Duration::from_secs(10))
        .expect("Hello should succeed on fresh worker");
    assert_eq!(resp, Response::Hello);
}

#[test]
fn the_about_check_starts_a_worker_that_loads_pdfium() {
    let pool = WorkerPool::new(test_pool_config(1));
    assert_eq!(pdf_converter_lib::commands::check_worker(&pool), Ok(()));
}

mod held_pdfs {
    use super::*;

    const THUMBNAIL: Request = Request::Thumbnail {
        page: 1,
        max_side: 32,
    };

    fn shapes_pdf() -> Request {
        Request::Open {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../crates/core/tests/fixtures/shapes.pdf"),
            password: None,
        }
    }

    fn pool_of_one(idle_time: Duration) -> WorkerPool {
        WorkerPool::new(test_pool_config(1).with_held_pdf_idle_time(idle_time))
    }

    /// Leaves a worker of `pool` holding the PDF `id`.
    fn hold(pool: &WorkerPool, id: u64) {
        let mut worker = pool.acquire_for_pdf(id).expect("acquire should succeed");
        worker.send(&shapes_pdf()).expect("the fixture should open");
        worker.set_held_pdf(Some(id));
    }

    /// Whether `worker` can draw without being asked to open a PDF first.
    fn has_a_pdf_open(worker: &mut pdf_converter_lib::worker_pool::PooledWorker) -> bool {
        worker.send(&THUMBNAIL).is_ok()
    }

    #[test]
    fn a_worker_is_lent_again_with_the_pdf_it_holds() {
        let pool = pool_of_one(Duration::from_secs(60));
        hold(&pool, 7);

        let mut worker = pool.acquire_for_pdf(7).expect("acquire should succeed");

        assert_eq!(worker.held_pdf(), Some(7));
        assert!(has_a_pdf_open(&mut worker));
    }

    #[test]
    fn a_worker_that_holds_the_pdf_is_preferred_to_the_others() {
        let pool =
            WorkerPool::new(test_pool_config(3).with_held_pdf_idle_time(Duration::from_secs(60)));
        let mut first = pool.acquire_for_pdf(7).expect("acquire should succeed");
        let mut second = pool.acquire_for_pdf(8).expect("acquire should succeed");
        let third = pool.acquire().expect("acquire should succeed");
        first.send(&shapes_pdf()).expect("the fixture should open");
        first.set_held_pdf(Some(7));
        second.send(&shapes_pdf()).expect("the fixture should open");
        second.set_held_pdf(Some(8));
        drop(first);
        drop(second);
        drop(third);

        assert_eq!(pool.acquire_for_pdf(7).unwrap().held_pdf(), Some(7));
        assert_eq!(pool.acquire_for_pdf(8).unwrap().held_pdf(), Some(8));
    }

    #[test]
    fn another_pdf_is_told_what_the_worker_holds() {
        let pool = pool_of_one(Duration::from_secs(60));
        hold(&pool, 7);

        assert_eq!(pool.acquire_for_pdf(8).unwrap().held_pdf(), Some(7));
    }

    #[test]
    fn a_plain_borrow_gets_a_worker_with_nothing_open() {
        let pool = pool_of_one(Duration::from_secs(60));
        hold(&pool, 7);

        let mut worker = pool.acquire().expect("acquire should succeed");

        assert_eq!(worker.held_pdf(), None);
        assert!(!has_a_pdf_open(&mut worker));
    }

    #[test]
    fn a_held_pdf_is_opened_afresh_after_its_share_of_requests() {
        let pool = pool_of_one(Duration::from_secs(60));
        hold(&pool, 7);

        for _ in 0..HELD_PDF_MAX_REQUESTS {
            let mut worker = pool.acquire_for_pdf(7).expect("acquire should succeed");
            assert!(worker.use_held_pdf(7));
        }

        let mut worker = pool.acquire_for_pdf(7).expect("acquire should succeed");
        assert!(!worker.use_held_pdf(7));
        worker.set_held_pdf(Some(7));
        assert!(worker.use_held_pdf(7));
    }

    #[test]
    fn a_held_pdf_is_closed_after_the_idle_time() {
        let pool = pool_of_one(Duration::from_millis(50));
        hold(&pool, 7);

        thread::sleep(Duration::from_millis(500));

        let mut worker = pool.acquire_for_pdf(7).expect("acquire should succeed");
        assert_eq!(worker.held_pdf(), None);
        assert!(!has_a_pdf_open(&mut worker));
    }

    #[test]
    fn a_held_pdf_is_closed_again_after_the_closer_has_ended() {
        let pool = pool_of_one(Duration::from_millis(50));
        hold(&pool, 7);
        thread::sleep(Duration::from_millis(500));
        hold(&pool, 8);
        thread::sleep(Duration::from_millis(500));

        assert_eq!(pool.acquire_for_pdf(8).unwrap().held_pdf(), None);
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn a_worker_that_crashed_is_not_lent_as_holding_its_pdf() {
        let pool = pool_of_one(Duration::from_secs(60));
        hold(&pool, 7);

        let mut worker = pool.acquire_for_pdf(7).expect("acquire should succeed");
        let _ = worker.request(&Request::CrashForTest, Duration::from_secs(10));
        drop(worker);

        assert_eq!(pool.acquire_for_pdf(7).unwrap().held_pdf(), None);
    }
}
