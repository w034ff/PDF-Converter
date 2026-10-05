//! Integration tests for [`WorkerPool`] (design §5.2).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(feature = "test-hooks")]
use pdf_converter_lib::worker_pool::WorkerPoolError;
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
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

fn test_pool_config(max_workers: usize) -> WorkerPoolConfig {
    WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ],
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
