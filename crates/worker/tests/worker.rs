//! Starts the standalone worker binary against the pdfium fetched by
//! `npm run pdfium:fetch` (design §8.1).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "../../core/tests/support/fixture_passwords.rs"]
mod fixture_passwords;

use pdfconv_worker::client::{RENDER_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::{Password, RenderColor, RenderFormat, Request, Response};
use pdfconv_worker::{RENDER_DPI_MAX, RENDER_DPI_MIN};

const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

fn open_req(path: impl Into<PathBuf>) -> Request {
    Request::Open {
        path: path.into(),
        password: None,
    }
}

/// Tolerance for pixel color differences in PDF -> Image render tests (design §11.2).
const COLOR_TOLERANCE_RENDER: i16 = 32;

/// Tolerance for pixel difference between grayscale render and BT.601 luma (design §11.2).
const COLOR_TOLERANCE_GRAY: i16 = 8;

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
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
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
fn renders_page_in_all_formats_and_color_modes() {
    let mut worker = start();
    worker
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open should succeed");

    for format in [RenderFormat::Png, RenderFormat::Jpeg, RenderFormat::Webp] {
        for color in [RenderColor::Color, RenderColor::Gray] {
            let (resp, body) = worker
                .request(
                    &Request::Render {
                        page: 1,
                        dpi: 150,
                        format,
                        color,
                    },
                    ANSWER_TIMEOUT,
                )
                .unwrap_or_else(|e| panic!("Render {format:?} {color:?} failed: {e:?}"));
            assert_eq!(resp, Response::Render);

            let expected_format = match format {
                RenderFormat::Png => image::ImageFormat::Png,
                RenderFormat::Jpeg => image::ImageFormat::Jpeg,
                RenderFormat::Webp => image::ImageFormat::WebP,
            };
            assert_eq!(
                image::guess_format(&body).expect("guess format should succeed"),
                expected_format,
                "Format should match for {format:?} {color:?}"
            );

            let img = image::load_from_memory(&body)
                .unwrap_or_else(|e| panic!("Decoding {format:?} {color:?} failed: {e:?}"));
            // A4 at 150 dpi: 595.28 / 72 * 150 = 1240.17 -> 1240, 841.89 / 72 * 150 = 1753.94 -> 1754
            assert_eq!((img.width(), img.height()), (1240, 1754));

            match color {
                RenderColor::Color => {
                    assert_eq!(img.color(), image::ColorType::Rgb8);
                    let [r, g, b] = img.to_rgb8().get_pixel(0, 0).0;
                    if format == RenderFormat::Jpeg {
                        assert!(r >= 250 && g >= 250 && b >= 250);
                    } else {
                        assert_eq!([r, g, b], [255, 255, 255]);
                    }
                }
                RenderColor::Gray => {
                    if format == RenderFormat::Webp {
                        // WebP bitstream only supports RGB/RGBA; verify all channels are equal and match white corner
                        assert_eq!(img.color(), image::ColorType::Rgb8);
                        let [r, g, b] = img.to_rgb8().get_pixel(0, 0).0;
                        assert_eq!([r, g, b], [255, 255, 255]);
                    } else {
                        assert_eq!(img.color(), image::ColorType::L8);
                        let l = img.to_luma8().get_pixel(0, 0).0[0];
                        if format == RenderFormat::Jpeg {
                            assert!(l >= 250);
                        } else {
                            assert_eq!(l, 255);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn generates_thumbnail_with_correct_dimensions_and_white_background() {
    let mut worker = start();
    worker
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
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

    // 1. encrypted.pdf without password -> PasswordProtected
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("encrypted.pdf"),
            password: None,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PasswordProtected"),
        other => panic!("expected PasswordProtected, got {other:?}"),
    }

    // 2. encrypted.pdf with wrong password -> PasswordProtected
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("encrypted.pdf"),
            password: Some(Password::new("wrong-password").unwrap()),
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PasswordProtected"),
        other => panic!("expected PasswordProtected, got {other:?}"),
    }

    // 3. encrypted.pdf with whitespace padded user password -> PasswordProtected
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("encrypted.pdf"),
            password: Some(
                Password::new(format!(" {} ", fixture_passwords::ENCRYPTED_USER_PASSWORD)).unwrap(),
            ),
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PasswordProtected"),
        other => panic!("expected PasswordProtected, got {other:?}"),
    }

    // 4. encrypted.pdf with user password -> opens successfully (page_count: 1)
    let (resp, _) = worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("encrypted.pdf"),
                password: Some(Password::new(fixture_passwords::ENCRYPTED_USER_PASSWORD).unwrap()),
            },
            ANSWER_TIMEOUT,
        )
        .expect("encrypted.pdf should open with user password");
    assert!(matches!(resp, Response::Open { page_count: 1, .. }));

    // 5. encrypted.pdf with owner password -> opens successfully (page_count: 1)
    let (resp, _) = worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("encrypted.pdf"),
                password: Some(Password::new(fixture_passwords::OWNER_PASSWORD).unwrap()),
            },
            ANSWER_TIMEOUT,
        )
        .expect("encrypted.pdf should open with owner password");
    assert!(matches!(resp, Response::Open { page_count: 1, .. }));

    // 6. encrypted_cafe.pdf with "cafe" (ASCII without accent) -> PasswordProtected
    match worker.request(
        &Request::Open {
            path: fixtures_dir().join("encrypted_cafe.pdf"),
            password: Some(Password::new("cafe").unwrap()),
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PasswordProtected"),
        other => panic!("expected PasswordProtected, got {other:?}"),
    }

    // 7. encrypted_cafe.pdf with "café" (UTF-8) -> opens successfully (page_count: 1)
    let (resp, _) = worker
        .request(
            &Request::Open {
                path: fixtures_dir().join("encrypted_cafe.pdf"),
                password: Some(
                    Password::new(fixture_passwords::ENCRYPTED_CAFE_USER_PASSWORD).unwrap(),
                ),
            },
            ANSWER_TIMEOUT,
        )
        .expect("encrypted_cafe.pdf should open with café");
    assert!(matches!(resp, Response::Open { page_count: 1, .. }));

    // 8. Render encrypted_cafe.pdf page 1 and check blue rectangle
    let (resp_render, body_render) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Png,
                color: RenderColor::Color,
            },
            ANSWER_TIMEOUT,
        )
        .expect("Render of unlocked PDF should succeed");
    assert_eq!(resp_render, Response::Render);
    assert!(!body_render.is_empty());
    let img = image::load_from_memory(&body_render)
        .expect("rendered image should decode")
        .to_rgb8();
    let has_blue_pixel = img.pixels().any(|p| p[0] < 80 && p[1] < 120 && p[2] > 150);
    assert!(
        has_blue_pixel,
        "rendered page should contain the blue rectangle"
    );

    // 9. restricted.pdf has permission restrictions only -> opens successfully
    let (resp, _) = worker
        .request(
            &open_req(fixtures_dir().join("restricted.pdf")),
            ANSWER_TIMEOUT,
        )
        .expect("restricted.pdf should open without error");
    assert!(matches!(resp, Response::Open { page_count: 1, .. }));

    // 10. corrupt.pdf is truncated -> PdfOpenFailed
    match worker.request(
        &open_req(fixtures_dir().join("corrupt.pdf")),
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "PdfOpenFailed"),
        other => panic!("expected PdfOpenFailed, got {other:?}"),
    }

    // 11. Missing file -> ReadFailed
    match worker.request(
        &open_req(fixtures_dir().join("non_existent_file.pdf")),
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "ReadFailed"),
        other => panic!("expected ReadFailed, got {other:?}"),
    }
}

#[test]
fn rejects_thumbnail_exceeding_max_render_pixels() {
    let mut worker = start();
    worker
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open should succeed");

    // Thumbnail with max_side = 200_000 produces ~2.8e10 pixels (> 100_000_000 MAX_RENDER_PIXELS)
    match worker.request(
        &Request::Thumbnail {
            page: 1,
            max_side: 200_000,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "RenderTooLarge"),
        other => panic!("expected RenderTooLarge, got {other:?}"),
    }
}

#[test]
fn rejects_render_exceeding_max_render_pixels() {
    let mut worker = start();
    worker
        .request(
            &open_req(fixtures_dir().join("mixed_sizes.pdf")),
            ANSWER_TIMEOUT,
        )
        .expect("Open should succeed");

    // Page 2 of mixed_sizes.pdf is 3000x3000 pt. At 300 dpi, it produces ~156M pixels (> 100_000_000 MAX_RENDER_PIXELS).
    match worker.request(
        &Request::Render {
            page: 2,
            dpi: 300,
            format: RenderFormat::Png,
            color: RenderColor::Color,
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
            color: RenderColor::Color,
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
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open should succeed");

    // Page 0 -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 0,
            dpi: 150,
            format: RenderFormat::Png,
            color: RenderColor::Color,
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
            color: RenderColor::Color,
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
            color: RenderColor::Color,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // DPI RENDER_DPI_MIN - 1 (35) -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 1,
            dpi: RENDER_DPI_MIN - 1,
            format: RenderFormat::Png,
            color: RenderColor::Color,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // DPI RENDER_DPI_MAX + 1 (1001) -> InvalidParams
    match worker.request(
        &Request::Render {
            page: 1,
            dpi: RENDER_DPI_MAX + 1,
            format: RenderFormat::Png,
            color: RenderColor::Color,
        },
        ANSWER_TIMEOUT,
    ) {
        Err(WorkerError::Remote { code, .. }) => assert_eq!(code, "InvalidParams"),
        other => panic!("expected InvalidParams, got {other:?}"),
    }

    // RENDER_DPI_MIN succeeds. RENDER_DPI_MAX is rendered by
    // renders_a4_at_the_maximum_dpi, which needs a release build.
    let (resp_min, _) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: RENDER_DPI_MIN,
                format: RenderFormat::Png,
                color: RenderColor::Color,
            },
            ANSWER_TIMEOUT,
        )
        .expect("RENDER_DPI_MIN should succeed");
    assert_eq!(resp_min, Response::Render);

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
            color: RenderColor::Color,
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
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open shapes.pdf should succeed");

    for page_num in 1..=3 {
        let (resp, body) = worker
            .request(
                &Request::Render {
                    page: page_num,
                    dpi: 150,
                    format: RenderFormat::Png,
                    color: RenderColor::Color,
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
fn quality_pdf_to_images_grayscale_and_webp() {
    let mut worker = start();
    worker
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open shapes.pdf should succeed");

    for page_num in 1..=3 {
        // 1. Render color PNG
        let (resp_color, body_color) = worker
            .request(
                &Request::Render {
                    page: page_num,
                    dpi: 150,
                    format: RenderFormat::Png,
                    color: RenderColor::Color,
                },
                ANSWER_TIMEOUT,
            )
            .expect("Render color PNG should succeed");
        assert_eq!(resp_color, Response::Render);
        let color_dynamic = image::load_from_memory(&body_color).expect("color PNG should decode");
        assert_eq!(color_dynamic.color(), image::ColorType::Rgb8);
        let color_img = color_dynamic.to_rgb8();

        // 2. Render grayscale PNG
        let (resp_gray, body_gray) = worker
            .request(
                &Request::Render {
                    page: page_num,
                    dpi: 150,
                    format: RenderFormat::Png,
                    color: RenderColor::Gray,
                },
                ANSWER_TIMEOUT,
            )
            .expect("Render gray PNG should succeed");
        assert_eq!(resp_gray, Response::Render);
        let gray_dynamic = image::load_from_memory(&body_gray).expect("gray PNG should decode");
        assert_eq!(gray_dynamic.color(), image::ColorType::L8);
        let gray_img = gray_dynamic.to_luma8();

        assert_eq!(color_img.dimensions(), gray_img.dimensions());
        let (w, h) = color_img.dimensions();
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = color_img.get_pixel(x, y).0;
                let expected_luma = (0.299 * (r as f64) + 0.587 * (g as f64) + 0.114 * (b as f64))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let actual_luma = gray_img.get_pixel(x, y).0[0];
                let diff = (expected_luma as i16 - actual_luma as i16).abs();
                assert!(
                    diff <= COLOR_TOLERANCE_GRAY,
                    "Page {page_num} at ({x}, {y}): diff {diff} > tolerance {COLOR_TOLERANCE_GRAY} (expected {expected_luma}, got {actual_luma})"
                );
            }
        }

        // 3. Render color WebP and compare with color PNG
        let (resp_webp_color, body_webp_color) = worker
            .request(
                &Request::Render {
                    page: page_num,
                    dpi: 150,
                    format: RenderFormat::Webp,
                    color: RenderColor::Color,
                },
                ANSWER_TIMEOUT,
            )
            .expect("Render color WebP should succeed");
        assert_eq!(resp_webp_color, Response::Render);
        let webp_color_dynamic =
            image::load_from_memory(&body_webp_color).expect("WebP color should decode");
        assert_eq!(webp_color_dynamic.color(), image::ColorType::Rgb8);
        assert_eq!(
            webp_color_dynamic.to_rgb8().into_raw(),
            color_img.clone().into_raw(),
            "Page {page_num} WebP color pixels must match PNG color pixels exactly"
        );

        // 4. Render grayscale WebP and compare with grayscale PNG
        let (resp_webp_gray, body_webp_gray) = worker
            .request(
                &Request::Render {
                    page: page_num,
                    dpi: 150,
                    format: RenderFormat::Webp,
                    color: RenderColor::Gray,
                },
                ANSWER_TIMEOUT,
            )
            .expect("Render gray WebP should succeed");
        assert_eq!(resp_webp_gray, Response::Render);
        let webp_gray_dynamic =
            image::load_from_memory(&body_webp_gray).expect("WebP gray should decode");
        assert_eq!(webp_gray_dynamic.color(), image::ColorType::Rgb8);
        assert_eq!(
            webp_gray_dynamic.to_luma8().into_raw(),
            gray_img.clone().into_raw(),
            "Page {page_num} WebP gray pixels must match PNG gray pixels exactly"
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
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open shapes.pdf should succeed");

    let start_time = Instant::now();
    let (resp, body) = worker
        .request(
            &Request::Render {
                page: 1,
                dpi: 150,
                format: RenderFormat::Png,
                color: RenderColor::Color,
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

/// The maximum resolution is chosen so that an A4 page renders within the
/// worker's memory limit and render timeout (design §4.4). An unoptimized
/// build encodes the 97-megapixel page too slowly for that timeout.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "rendering at the maximum resolution is only fast enough in release builds"
)]
fn renders_a4_at_the_maximum_dpi() {
    let mut worker = start();
    worker
        .request(&open_req(fixtures_dir().join("shapes.pdf")), ANSWER_TIMEOUT)
        .expect("Open shapes.pdf should succeed");

    for format in [RenderFormat::Png, RenderFormat::Jpeg, RenderFormat::Webp] {
        let (resp, body) = worker
            .request(
                &Request::Render {
                    page: 1,
                    dpi: RENDER_DPI_MAX,
                    format,
                    color: RenderColor::Color,
                },
                RENDER_TIMEOUT,
            )
            .unwrap_or_else(|e| panic!("{format:?} at RENDER_DPI_MAX should succeed: {e:?}"));
        assert_eq!(resp, Response::Render);
        let dimensions = image::ImageReader::new(std::io::Cursor::new(&body))
            .with_guessed_format()
            .expect("guess format")
            .into_dimensions()
            .expect("read dimensions");
        assert_eq!(dimensions, (8268, 11693), "{format:?}");
    }
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
