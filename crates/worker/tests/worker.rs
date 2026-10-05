//! Starts the standalone worker binary against the pdfium fetched by
//! `npm run pdfium:fetch` (design §8.1).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pdfconv_worker::client::{WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{RenderFormat, Request, Response};
use pdfconv_worker::{MAX_PDF_PAGES, check_page_count};

const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// Tolerance for pixel color differences in PDF -> Image render tests (design §11.2).
const COLOR_TOLERANCE_RENDER: i16 = 32;

/// Maximum allowable ratio of mismatched pixels for render tests (design §11.2: 0.5%).
const MAX_MISMATCH_RATIO_RENDER: f64 = 0.005;

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

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures")
}

fn start() -> WorkerProcess {
    WorkerProcess::spawn(
        env!("CARGO_BIN_EXE_pdfconv-worker").as_ref(),
        [library_dir()],
    )
    .expect("worker should start")
}

fn compare_images(actual_bytes: &[u8], expected_bytes: &[u8]) -> f64 {
    let actual = image::load_from_memory(actual_bytes)
        .expect("actual image should decode")
        .to_rgb8();
    let expected = image::load_from_memory(expected_bytes)
        .expect("expected image should decode")
        .to_rgb8();

    assert_eq!(
        actual.dimensions(),
        expected.dimensions(),
        "dimensions must match"
    );

    let (width, height) = actual.dimensions();
    let total_pixels = (width as u64) * (height as u64);
    let mut mismatched = 0u64;

    for (p1, p2) in actual.pixels().zip(expected.pixels()) {
        let diff_r = (p1[0] as i16 - p2[0] as i16).abs();
        let diff_g = (p1[1] as i16 - p2[1] as i16).abs();
        let diff_b = (p1[2] as i16 - p2[2] as i16).abs();

        if diff_r > COLOR_TOLERANCE_RENDER
            || diff_g > COLOR_TOLERANCE_RENDER
            || diff_b > COLOR_TOLERANCE_RENDER
        {
            mismatched += 1;
        }
    }

    (mismatched as f64) / (total_pixels as f64)
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

#[test]
fn opens_shapes_pdf_and_checks_page_count_and_dimensions() {
    let mut worker = start();
    let (response, body) = worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open should succeed");
    assert!(body.is_empty());

    let (page_count, pages) = match response {
        Response::Open { page_count, pages } => (page_count, pages),
        other => panic!("expected Response::Open, got {other:?}"),
    };
    assert_eq!(page_count, 3);
    assert_eq!(pages.len(), 3);

    // Page 1: A4 portrait (595.28 x 841.89 pt)
    assert!((pages[0].width_pt - 595.28).abs() < 0.1);
    assert!((pages[0].height_pt - 841.89).abs() < 0.1);

    // Page 2: A4 landscape (841.89 x 595.28 pt)
    assert!((pages[1].width_pt - 841.89).abs() < 0.1);
    assert!((pages[1].height_pt - 595.28).abs() < 0.1);

    // Page 3: Letter (612.0 x 792.0 pt)
    assert!((pages[2].width_pt - 612.0).abs() < 0.1);
    assert!((pages[2].height_pt - 792.0).abs() < 0.1);
}

#[test]
fn renders_page_to_png_and_jpeg_with_white_background() {
    let mut worker = start();
    worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open should succeed");

    // Render Page 1 at 150 dpi in PNG
    let (resp_png, body_png) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Png,
            },
            ANSWER_TIMEOUT,
        )
        .expect("Render PNG should succeed");
    assert_eq!(resp_png, Response::Render);

    let png_img = image::load_from_memory(&body_png)
        .expect("body should be valid PNG")
        .to_rgb8();
    // A4 at 150 dpi: 595.28 / 72 * 150 = 1240.17 -> 1240, 841.89 / 72 * 150 = 1753.94 -> 1754
    assert_eq!(png_img.dimensions(), (1240, 1754));
    // Top-left corner (0, 0) has no shape, so background must be pure white (255, 255, 255)
    assert_eq!(png_img.get_pixel(0, 0).0, [255, 255, 255]);

    // Render Page 1 at 150 dpi in JPEG
    let (resp_jpeg, body_jpeg) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Jpeg,
            },
            ANSWER_TIMEOUT,
        )
        .expect("Render JPEG should succeed");
    assert_eq!(resp_jpeg, Response::Render);

    let jpeg_img = image::load_from_memory(&body_jpeg)
        .expect("body should be valid JPEG")
        .to_rgb8();
    assert_eq!(jpeg_img.dimensions(), (1240, 1754));
    let corner = jpeg_img.get_pixel(0, 0).0;
    // JPEG has lossy compression, allow small tolerance near 255
    assert!(corner[0] >= 250 && corner[1] >= 250 && corner[2] >= 250);
}

#[test]
fn generates_thumbnail_with_correct_dimensions_and_white_background() {
    let mut worker = start();
    worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open should succeed");

    let (resp, body) = worker
        .request(
            &Request::Thumbnail {
                page: 1,
                max_side: 160,
            },
            ANSWER_TIMEOUT,
        )
        .expect("Thumbnail should succeed");
    assert_eq!(resp, Response::Thumbnail);

    let thumb = image::load_from_memory(&body)
        .expect("thumbnail should be valid PNG")
        .to_rgb8();
    // Longer side (height) is 160. Shorter side: (160 * 595.28 / 841.89).round() = 113
    assert_eq!(thumb.dimensions(), (113, 160));
    assert_eq!(thumb.get_pixel(0, 0).0, [255, 255, 255]);
}

#[test]
fn handles_encrypted_restricted_and_corrupt_pdfs() {
    let mut worker = start();

    // encrypted.pdf requires user password -> PasswordProtected
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("encrypted.pdf"),
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PasswordProtected"),
        other => panic!("expected PasswordProtected, got {other:?}"),
    }

    // restricted.pdf has permission restrictions only -> opens successfully
    let (resp, _) = worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("restricted.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("restricted.pdf should open without error");
    assert!(matches!(resp, Response::Open { page_count: 1, .. }));

    // corrupt.pdf is truncated -> PdfOpenFailed
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("corrupt.pdf"),
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PdfOpenFailed"),
        other => panic!("expected PdfOpenFailed, got {other:?}"),
    }

    // Missing file -> ReadFailed
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("non_existent_file.pdf"),
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "ReadFailed"),
        other => panic!("expected ReadFailed, got {other:?}"),
    }
}

#[test]
fn checks_page_count_limit_purely() {
    assert!(check_page_count(MAX_PDF_PAGES).is_ok());
    let err = check_page_count(MAX_PDF_PAGES + 1).unwrap_err();
    assert_eq!(
        err,
        Response::Error {
            code: "TooManyPages".into(),
            detail: Some("10,000".into()),
        }
    );
}

#[test]
fn rejects_render_exceeding_max_render_pixels() {
    let mut worker = start();
    worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open should succeed");

    // Rendering A4 at 4000 dpi gives ~1.5 billion pixels (> 100_000_000 MAX_RENDER_PIXELS)
    match worker.request(
        &Request::Render {
            page: 1,
            dpi: 4000,
            format: RenderFormat::Png,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "RenderTooLarge"),
        other => panic!("expected RenderTooLarge, got {other:?}"),
    }
}

#[test]
fn rejects_invalid_parameters_and_closed_documents() {
    let mut worker = start();

    // Render before Open -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 1,
            dpi: 150,
            format: RenderFormat::Png,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // Thumbnail before Open -> InvalidParams
    match worker.request(
        &Request::Thumbnail {
            page: 1,
            max_side: 160,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open should succeed");

    // Page 0 -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 0,
            dpi: 150,
            format: RenderFormat::Png,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // Page 4 (out of bounds for 3-page PDF) -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 4,
            dpi: 150,
            format: RenderFormat::Png,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // DPI 0 -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 1,
            dpi: 0,
            format: RenderFormat::Png,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // max_side 0 -> InvalidParams
    match worker.request(
        &Request::Thumbnail {
            page: 1,
            max_side: 0,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // Close document
    let (resp_close, _) = worker
        .request(&Request::Close, ANSWER_TIMEOUT)
        .expect("Close should succeed");
    assert_eq!(resp_close, Response::Close);

    // Render after Close -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 1,
            dpi: 150,
            format: RenderFormat::Png,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }
}

#[test]
fn quality_pdf_to_images_shapes() {
    let mut worker = start();
    worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open shapes.pdf should succeed");

    for page_num in 1..=3 {
        let (resp, body) = worker
            .request(
                &Request::Render {
                    page: page_num,
                    dpi: 150,
                    format: RenderFormat::Png,
                },
                ANSWER_TIMEOUT,
            )
            .expect("Render should succeed");
        assert_eq!(resp, Response::Render);

        let expected_file = format!("shapes_150dpi_p{page_num}.png");
        let expected_bytes =
            std::fs::read(fixtures_dir().join(&expected_file)).expect("reading expected fixture");

        let ratio = compare_images(&body, &expected_bytes);
        println!(
            "Page {page_num} mismatch ratio: {:.4}% (limit: {:.4}%)",
            ratio * 100.0,
            MAX_MISMATCH_RATIO_RENDER * 100.0
        );
        assert!(
            ratio <= MAX_MISMATCH_RATIO_RENDER,
            "Page {page_num} mismatch ratio {ratio} exceeded limit {MAX_MISMATCH_RATIO_RENDER}"
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "performance benchmarks are only meaningful in release builds"
)]
fn performance_render_a4_150dpi() {
    let mut worker = start();
    worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("shapes.pdf"),
            },
            ANSWER_TIMEOUT,
        )
        .expect("Open shapes.pdf should succeed");

    let start_time = Instant::now();
    let (resp, body) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Png,
            },
            ANSWER_TIMEOUT,
        )
        .expect("Render should succeed");
    let elapsed = start_time.elapsed();

    assert_eq!(resp, Response::Render);
    assert!(!body.is_empty());
    println!("A4 150dpi PNG render elapsed time: {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(3),
        "Render took {elapsed:?}, exceeding 3s limit"
    );
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
