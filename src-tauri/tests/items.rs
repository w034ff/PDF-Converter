//! Integration tests for the lists of images and PDFs (design §6.1, §7.1):
//! adding files and folders, sorting a drop, and thumbnails. They use the
//! real worker through the pool, like `pool.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pdf_converter_lib::AppState;
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::items::{
    AddResult, ImageItem, PdfItem, Skipped, add_dropped, add_images, add_images_from_folder,
    add_pdfs, add_pdfs_from_folder, make_thumbnail, sort_dropped,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_core::{THUMBNAIL_SIDE, probe};
use pdfconv_worker::WORKER_FLAG;

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

fn state() -> AppState {
    let config = WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ],
    )
    .with_max_workers(2)
    .with_open_timeout(Duration::from_secs(30))
    .with_render_timeout(Duration::from_secs(60));
    AppState::new(WorkerPool::new(config))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures")
        .join(name)
}

/// Copies the fixture `name` into `dir` as `as_name`.
fn put(dir: &Path, name: &str, as_name: &str) -> PathBuf {
    let path = dir.join(as_name);
    fs::copy(fixture(name), &path).expect("copying a fixture");
    path
}

fn names<T>(items: &[T], name: impl Fn(&T) -> &str) -> Vec<String> {
    items.iter().map(|item| name(item).to_owned()).collect()
}

fn image_names(result: &AddResult<ImageItem>) -> Vec<String> {
    names(&result.added, |item| &item.name)
}

fn pdf_names(result: &AddResult<PdfItem>) -> Vec<String> {
    names(&result.added, |item| &item.name)
}

const NO_SKIPS: Skipped = Skipped {
    unsupported: 0,
    folders: 0,
    duplicates: 0,
};

/// A folder with two images, a PDF, a text file, a subfolder with an image in
/// it, and a hidden image.
fn mixed_folder() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    put(dir.path(), "photo.jpg", "b.JPG");
    put(dir.path(), "logo_alpha.png", "a.png");
    put(dir.path(), "shapes.pdf", "doc.pdf");
    fs::write(dir.path().join("notes.txt"), "text").expect("writing a file");
    put(dir.path(), "photo.jpg", ".hidden.png");
    fs::create_dir(dir.path().join("sub")).expect("creating a subfolder");
    put(&dir.path().join("sub"), "photo.jpg", "nested.png");
    dir
}

fn error_code(error: &Option<IpcError>) -> Option<ErrorCode> {
    error.as_ref().map(|error| error.code)
}

#[test]
fn a_folder_is_read_one_level_deep() {
    let dir = mixed_folder();
    let state = state();

    let result = add_images_from_folder(&state, dir.path()).expect("reading the folder");

    assert_eq!(image_names(&result), ["a.png", "b.JPG"]);
    // doc.pdf and notes.txt are not images; `sub` is a folder; the hidden file
    // is neither added nor counted.
    assert_eq!(
        result.skipped,
        Skipped {
            unsupported: 2,
            folders: 1,
            duplicates: 0,
        }
    );
    assert_eq!(state.items.len(), 2);
}

#[test]
fn a_folder_of_pdfs_counts_what_it_leaves_out() {
    let dir = mixed_folder();
    let state = state();

    let result = add_pdfs_from_folder(&state, dir.path()).expect("reading the folder");

    assert_eq!(pdf_names(&result), ["doc.pdf"]);
    assert_eq!(
        result.skipped,
        Skipped {
            unsupported: 3,
            folders: 1,
            duplicates: 0,
        }
    );
}

#[test]
fn a_folder_that_cannot_be_read_is_a_read_error() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let error = add_images_from_folder(&state(), &dir.path().join("missing"))
        .expect_err("a folder that is not there");
    assert_eq!(error.code, ErrorCode::ReadFailed);
}

#[test]
fn hidden_files_are_added_when_they_are_chosen_directly() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let hidden = put(dir.path(), "photo.jpg", ".chosen.jpg");
    let state = state();

    let result = add_images(&state, &[hidden]);

    assert_eq!(image_names(&result), [".chosen.jpg"]);
    assert_eq!(result.skipped, NO_SKIPS);
}

#[test]
fn files_already_in_the_list_are_counted_as_duplicates() {
    let dir = mixed_folder();
    let state = state();
    add_images_from_folder(&state, dir.path()).expect("reading the folder");

    let again = add_images_from_folder(&state, dir.path()).expect("reading the folder");

    assert!(again.added.is_empty());
    assert_eq!(
        again.skipped,
        Skipped {
            unsupported: 2,
            folders: 1,
            duplicates: 2,
        }
    );
    assert_eq!(state.items.len(), 2);
}

#[test]
fn the_same_file_under_another_spelling_is_a_duplicate() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "photo.jpg", "a.jpg");
    fs::create_dir(dir.path().join("sub")).expect("creating a subfolder");
    let roundabout = dir.path().join("sub").join("..").join("a.jpg");
    let state = state();

    let result = add_images(&state, &[path, roundabout]);

    assert_eq!(image_names(&result), ["a.jpg"]);
    assert_eq!(result.skipped.duplicates, 1);
}

#[test]
fn a_removed_file_can_be_added_again() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "photo.jpg", "a.jpg");
    let state = state();
    let first = add_images(&state, std::slice::from_ref(&path));
    let id = first.added[0].id;

    state.items.remove(&[id, id + 1000]);
    let second = add_images(&state, &[path]);

    assert_eq!(second.skipped, NO_SKIPS);
    assert_eq!(second.added.len(), 1);
    assert_ne!(second.added[0].id, id);
}

#[test]
fn an_image_item_describes_the_file() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "photo.jpg", "photo.jpg");
    let state = state();

    let result = add_images(&state, std::slice::from_ref(&path));

    let item = &result.added[0];
    assert_eq!(
        (item.name.as_str(), item.width, item.height),
        ("photo.jpg", 480, 320)
    );
    assert_eq!(
        item.bytes,
        fs::metadata(&path).expect("reading the size").len()
    );
    assert_eq!(item.error, None);
    let json = serde_json::to_value(item).expect("serializing the item");
    assert_eq!(json["format"], "jpeg");
    assert!(json.get("path").is_none());
}

#[test]
fn an_image_size_follows_the_exif_orientation() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let swapping = ["rotate90.jpg", "rotate270.jpg", "rotate90_flip_h.jpg"];
    let keeping = ["rotate0.jpg", "rotate180.jpg", "flip_h.jpg", "flip_v.jpg"];
    let state = state();

    for name in swapping.iter().chain(keeping.iter()) {
        let stored = probe(&fixture(name)).expect("probing the fixture");
        assert_ne!(stored.width, stored.height, "{name} must not be square");
        let path = put(dir.path(), name, name);

        let item = &add_images(&state, &[path]).added[0];

        let expected = if swapping.contains(name) {
            (stored.height, stored.width)
        } else {
            (stored.width, stored.height)
        };
        assert_eq!((item.width, item.height), expected, "{name}");
    }
}

#[test]
fn damaged_images_are_added_with_the_reason() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    // Cut inside the IHDR chunk, so that the header cannot be read.
    let full = fs::read(fixture("logo_alpha.png")).expect("reading a fixture");
    fs::write(dir.path().join("cut.png"), &full[..20]).expect("writing a file");
    fs::write(dir.path().join("text.png"), "not an image").expect("writing a file");
    // Only the pixel data of this one is damaged, so probing accepts it.
    put(dir.path(), "corrupt.png", "corrupt.png");
    put(dir.path(), "photo.jpg", "good.jpg");
    let state = state();

    let result = add_images_from_folder(&state, dir.path()).expect("reading the folder");

    let by_name = |name: &str| {
        result
            .added
            .iter()
            .find(|item| item.name == name)
            .unwrap_or_else(|| panic!("{name} should be in the list"))
    };
    assert_eq!(result.added.len(), 4);
    assert_eq!(
        error_code(&by_name("cut.png").error),
        Some(ErrorCode::DecodeFailed)
    );
    assert_eq!(
        error_code(&by_name("text.png").error),
        Some(ErrorCode::UnsupportedFormat)
    );
    assert_eq!(by_name("corrupt.png").error, None);
    assert_eq!(by_name("good.jpg").error, None);
    let failed = by_name("cut.png");
    assert_eq!((failed.width, failed.height, failed.format), (0, 0, None));
}

#[test]
fn pdfs_are_opened_in_a_worker_and_failures_are_added_with_the_reason() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    put(dir.path(), "shapes.pdf", "shapes.pdf");
    put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    put(dir.path(), "corrupt.pdf", "corrupt.pdf");
    let state = state();

    let result = add_pdfs_from_folder(&state, dir.path()).expect("reading the folder");

    assert_eq!(
        pdf_names(&result),
        ["corrupt.pdf", "encrypted.pdf", "shapes.pdf"]
    );
    let [corrupt, encrypted, shapes] = &result.added[..] else {
        panic!("three PDFs should have been added");
    };
    assert_eq!(error_code(&corrupt.error), Some(ErrorCode::PdfOpenFailed));
    assert_eq!(
        error_code(&encrypted.error),
        Some(ErrorCode::PasswordProtected)
    );
    assert_eq!((corrupt.page_count, encrypted.page_count), (0, 0));
    assert_eq!(shapes.error, None);
    assert_eq!(shapes.page_count, 3);
    let size = shapes
        .first_page_size_pt
        .expect("the size of the first page");
    assert!(size.width_pt > 0.0 && size.height_pt > 0.0);
    let json = serde_json::to_value(shapes).expect("serializing the item");
    assert!(json.get("pageCount").is_some() && json.get("firstPageSizePt").is_some());
}

#[test]
fn a_file_of_another_kind_is_unsupported() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let image = put(dir.path(), "photo.jpg", "a.jpg");
    let pdf = put(dir.path(), "shapes.pdf", "a.pdf");
    let state = state();

    let images = add_images(&state, &[image.clone(), pdf.clone()]);
    let pdfs = add_pdfs(&state, &[image, pdf]);

    assert_eq!((images.added.len(), images.skipped.unsupported), (1, 1));
    assert_eq!((pdfs.added.len(), pdfs.skipped.unsupported), (1, 1));
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_neither_added_nor_counted() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let target = put(dir.path(), "photo.jpg", "a.jpg");
    fs::create_dir(dir.path().join("sub")).expect("creating a subfolder");
    std::os::unix::fs::symlink(&target, dir.path().join("link.jpg")).expect("making a link");
    std::os::unix::fs::symlink(dir.path().join("sub"), dir.path().join("linked-folder"))
        .expect("making a link");
    std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("broken.jpg"))
        .expect("making a link");
    let state = state();

    let result = add_images_from_folder(&state, dir.path()).expect("reading the folder");

    assert_eq!(image_names(&result), ["a.jpg"]);
    // Only the real subfolder is counted; a counted link would show up here.
    assert_eq!(
        result.skipped,
        Skipped {
            unsupported: 0,
            folders: 1,
            duplicates: 0,
        }
    );
}

#[test]
fn a_drop_is_sorted_by_kind() {
    let dir = mixed_folder();
    let loose = tempfile::tempdir().expect("creating a temporary folder");
    let image = put(loose.path(), "photo.jpg", "loose.png");
    let hidden = put(loose.path(), "photo.jpg", ".hidden-but-chosen.jpg");
    let pdf = put(loose.path(), "shapes.pdf", "loose.PDF");
    let other = loose.path().join("readme.md");
    fs::write(&other, "text").expect("writing a file");

    let sorting = sort_dropped(&[
        image.clone(),
        pdf.clone(),
        other,
        hidden.clone(),
        dir.path().to_path_buf(),
    ]);

    let folder_file = |name: &str| dir.path().join(name);
    assert_eq!(
        sorting.images,
        [image, hidden, folder_file("a.png"), folder_file("b.JPG")]
    );
    assert_eq!(sorting.pdfs, [pdf, folder_file("doc.pdf")]);
    // readme.md and notes.txt; the subfolder `sub` of the dropped folder.
    assert_eq!((sorting.unsupported, sorting.folders), (2, 1));
}

#[test]
fn a_drop_adds_to_both_lists() {
    let dir = mixed_folder();
    let loose = tempfile::tempdir().expect("creating a temporary folder");
    let image = put(loose.path(), "photo.jpg", "loose.jpg");
    let pdf = put(loose.path(), "shapes.pdf", "loose.pdf");
    let state = state();

    let dropped = add_dropped(
        &state,
        &[
            image.clone(),
            image,
            pdf,
            dir.path().to_path_buf(),
            loose.path().join("readme.md"),
        ],
    );

    assert_eq!(
        names(&dropped.images, |item| &item.name),
        ["loose.jpg", "a.png", "b.JPG"]
    );
    assert_eq!(
        names(&dropped.pdfs, |item| &item.name),
        ["loose.pdf", "doc.pdf"]
    );
    assert_eq!(
        dropped.skipped,
        Skipped {
            // notes.txt, and readme.md, which is not there but would not be a supported type.
            unsupported: 2,
            folders: 1,
            duplicates: 1,
        }
    );
    let json = serde_json::to_value(&dropped).expect("serializing the payload");
    assert!(json["images"].is_array() && json["pdfs"].is_array());
}

/// The width and height in the IHDR chunk of a PNG.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let read = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().expect("four bytes"));
    (read(16), read(20))
}

#[test]
fn an_image_thumbnail_is_a_png_with_the_longer_side_fixed() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "photo.jpg", "photo.jpg");
    let state = state();
    let id = add_images(&state, &[path]).added[0].id;

    let png = make_thumbnail(&state, id, None).expect("making the thumbnail");
    assert_eq!(png_size(&png), (THUMBNAIL_SIDE, 107));

    // `page` is for PDFs only.
    let png = make_thumbnail(&state, id, Some(99)).expect("making the thumbnail");
    assert_eq!(png_size(&png), (THUMBNAIL_SIDE, 107));
}

#[test]
fn a_pdf_thumbnail_is_made_by_the_worker() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    for page in [None, Some(1), Some(3)] {
        let png = make_thumbnail(&state, id, page).expect("making the thumbnail");
        let (width, height) = png_size(&png);
        assert_eq!(width.max(height), THUMBNAIL_SIDE, "page {page:?}");
    }
}

#[test]
fn a_page_that_is_not_in_the_pdf_is_invalid() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    for page in [0, 4, u32::MAX] {
        let error = make_thumbnail(&state, id, Some(page)).expect_err("a page out of range");
        assert_eq!(
            error,
            IpcError::from_code(ErrorCode::InvalidParams),
            "page {page}"
        );
    }
}

#[test]
fn thumbnails_of_unknown_and_failed_items_are_errors() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let encrypted = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let cut = dir.path().join("cut.png");
    let full = fs::read(fixture("logo_alpha.png")).expect("reading a fixture");
    fs::write(&cut, &full[..20]).expect("writing a file");
    let state = state();
    let pdf_id = add_pdfs(&state, &[encrypted]).added[0].id;
    let image_id = add_images(&state, &[cut]).added[0].id;

    assert_eq!(
        make_thumbnail(&state, 12345, None).expect_err("an ID that is not in the table"),
        IpcError::from_code(ErrorCode::UnknownHandle)
    );
    assert_eq!(
        make_thumbnail(&state, pdf_id, None)
            .expect_err("a PDF that did not open")
            .code,
        ErrorCode::PasswordProtected
    );
    assert_eq!(
        make_thumbnail(&state, image_id, None)
            .expect_err("an image that did not load")
            .code,
        ErrorCode::DecodeFailed
    );

    state.items.remove(&[pdf_id]);
    assert_eq!(
        make_thumbnail(&state, pdf_id, None)
            .expect_err("a removed item")
            .code,
        ErrorCode::UnknownHandle
    );
}

#[test]
fn a_pdf_that_became_damaged_after_it_was_added_fails_with_a_reason() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let state = state();
    let id = add_pdfs(&state, std::slice::from_ref(&path)).added[0].id;

    fs::write(&path, "no longer a PDF").expect("overwriting the file");

    assert_eq!(
        make_thumbnail(&state, id, None)
            .expect_err("a PDF that is damaged now")
            .code,
        ErrorCode::PdfOpenFailed
    );
    // The worker that answered with the error is still usable.
    let other = put(dir.path(), "shapes.pdf", "other.pdf");
    assert_eq!(add_pdfs(&state, &[other]).added[0].error, None);
}

#[test]
fn the_table_is_shared_by_clones_of_the_state() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "photo.jpg", "a.jpg");
    let state = state();
    let clone = state.clone();

    add_images(&clone, &[path]);

    assert_eq!(state.items.len(), 1);
}
