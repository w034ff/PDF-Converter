//! A worker that dies in the middle of a crop export (design §5.2, §6.10).
//!
//! The only way to make a real worker die between two ranges is to kill its
//! process from outside, which finds it among the children of this process. It
//! is the only test here, so that no other test's worker is among them, and it
//! needs `/proc` and `kill`.

#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pdf_converter_lib::AppState;
use pdf_converter_lib::crop::{activate_crop_pdf, open_pdf};
use pdf_converter_lib::crop_export::{CropRegionInput, start_crop_export_internal};
use pdf_converter_lib::error::ErrorCode;
use pdf_converter_lib::jobs::{
    JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, RenderColorChoice,
    RenderFormatChoice,
};
use pdf_converter_lib::settings::CropOptions;
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_worker::WORKER_FLAG;
use tempfile::TempDir;

/// Kills every worker process that is a child of this process.
fn kill_workers() -> usize {
    let me = std::process::id();
    let mut killed = 0;
    for entry in fs::read_dir("/proc").expect("reading /proc").flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // The name in the second field may hold spaces and parentheses; the
        // fields after its closing one are the state and then the parent.
        let Some(after_name) = stat.rfind(')').map(|end| &stat[end + 1..]) else {
            continue;
        };
        let parent = after_name
            .split_whitespace()
            .nth(1)
            .and_then(|field| field.parse::<u32>().ok());
        let is_worker = fs::read(entry.path().join("cmdline"))
            .is_ok_and(|cmdline| String::from_utf8_lossy(&cmdline).contains(WORKER_FLAG));
        if parent == Some(me) && is_worker {
            let status = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status()
                .expect("running kill");
            assert!(status.success());
            killed += 1;
        }
    }
    killed
}

fn library_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdfium/linux/lib")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures")
        .join(name)
}

#[test]
fn a_worker_that_dies_fails_the_next_range_and_the_rest_is_not_tried() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config = WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
            manifest_dir
                .join("onnxruntime/linux/libonnxruntime.so")
                .to_string_lossy()
                .into_owned(),
            manifest_dir
                .join("layout-model/PP-DocLayout_plus-L-int8.onnx")
                .to_string_lossy()
                .into_owned(),
        ],
    )
    .with_max_workers(1);
    let state = AppState::new(WorkerPool::new(config));
    let pdf = open_pdf(&state, &fixture("shapes.pdf"));
    activate_crop_pdf(&state, pdf.id).expect("activating the pending PDF");
    let out = TempDir::new().expect("creating the output folder");
    *state.crop_output_dir.lock().expect("output folder lock") = Some(out.path().to_path_buf());

    // Whole pages, so that a range takes long enough for the kill to land
    // before it is done.
    let regions: Vec<CropRegionInput> = (0u32..)
        .zip(&pdf.pages)
        .map(|(index, page)| CropRegionInput {
            key: u64::from(index) + 1,
            page: index + 1,
            rect_pt: [
                0.0,
                0.0,
                f64::from(page.width_pt),
                f64::from(page.height_pt),
            ],
            image_index: None,
        })
        .collect();
    assert_eq!(regions.len(), 3);

    let items = Arc::new(Mutex::new(Vec::<JobItemPayload>::new()));
    let finished = Arc::new(Mutex::new(None::<JobFinishedPayload>));
    let killed = Arc::new(Mutex::new(0));
    let callbacks = JobCallbacks {
        on_progress: |_| {},
        on_item: {
            let items = Arc::clone(&items);
            let killed = Arc::clone(&killed);
            move |item: JobItemPayload| {
                if item.id == 1 {
                    *killed.lock().expect("killed lock") += kill_workers();
                }
                items.lock().expect("items lock").push(item);
            }
        },
        on_finished: {
            let finished = Arc::clone(&finished);
            move |payload| *finished.lock().expect("finished lock") = Some(payload)
        },
    };
    let options = CropOptions {
        format: RenderFormatChoice::Png,
        color_mode: RenderColorChoice::Color,
        dpi: 300,
        photo_native: true,
        photo_only: false,
    };
    start_crop_export_internal(&state, pdf.id, &regions, options, callbacks).expect("starting");

    let started = Instant::now();
    let finished = loop {
        if let Some(payload) = finished.lock().expect("finished lock").clone() {
            break payload;
        }
        assert!(started.elapsed() < Duration::from_secs(120), "no end");
        std::thread::sleep(Duration::from_millis(10));
    };

    assert_eq!(*killed.lock().expect("killed lock"), 1, "one worker died");
    let items = items.lock().expect("items lock").clone();
    let seen: Vec<(u64, JobItemStatus)> = items.iter().map(|i| (i.id, i.status)).collect();
    assert_eq!(seen, [(1, JobItemStatus::Ok), (2, JobItemStatus::Failed)]);
    assert_eq!(
        items[1].error.as_ref().map(|error| error.code),
        Some(ErrorCode::WorkerCrashed)
    );
    assert_eq!(
        finished,
        JobFinishedPayload {
            succeeded: 1,
            failed: 1,
            no_pages: 0,
            unprocessed: 1,
            cancelled: false,
        }
    );
    assert!(!state.is_running.load(Ordering::SeqCst));
    let names: Vec<String> = fs::read_dir(out.path())
        .expect("listing the output folder")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["shapes_p1_1.png"]);
}
