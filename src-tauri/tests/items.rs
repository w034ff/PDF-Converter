//! Integration tests for the lists of images and PDFs (design §6.1, §7.1):
//! adding files and folders, sorting a drop, and thumbnails. They use the
//! real worker through the pool, like `pool.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use pdf_converter_lib::AppState;
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::items::{
    AddResult, ImageFormatName, ImageItem, PdfItem, Skipped, add_dropped, add_images,
    add_images_from_folder, add_pdfs, add_pdfs_from_folder, make_thumbnail, sort_dropped,
    unlock_pdf,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_core::{THUMBNAIL_SIDE, probe};
use pdfconv_worker::WORKER_FLAG;

#[path = "../../crates/core/tests/support/fixture_passwords.rs"]
mod fixture_passwords;
use fixture_passwords::{ENCRYPTED_CAFE_USER_PASSWORD, ENCRYPTED_USER_PASSWORD, OWNER_PASSWORD};

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

fn state() -> AppState {
    let config = WorkerPoolConfig::new(
        env!("CARGO_BIN_EXE_pdf-converter"),
        [
            WORKER_FLAG.to_string(),
            library_dir().to_string_lossy().into_owned(),
        ]
        .into_iter()
        .chain(layout_args()),
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

#[test]
fn wrong_password_returns_password_protected_and_leaves_item_unchanged() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    let err = unlock_pdf(&state, id, "wrong-password").expect_err("wrong password");
    assert_eq!(err.code, ErrorCode::PasswordProtected);

    let entry = state.items.get(id).expect("entry exists");
    assert_eq!(
        entry.error.as_ref().map(|e| e.code),
        Some(ErrorCode::PasswordProtected)
    );
    assert_eq!(entry.page_count, 0);
    assert!(entry.password.is_none());

    // Can still be unlocked afterwards with the correct password
    let res = unlock_pdf(&state, id, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    assert_eq!(res.items.len(), 1);
    assert_eq!(res.items[0].page_count, 1);
    assert_eq!(res.items[0].error, None);
}

#[test]
fn correct_password_populates_metadata_and_enables_thumbnail() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    let res = unlock_pdf(&state, id, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    assert_eq!(res.items.len(), 1);
    let item = &res.items[0];
    assert_eq!(item.id, id);
    assert_eq!(item.page_count, 1);
    assert!(item.first_page_size_pt.is_some());
    assert_eq!(item.error, None);

    let entry = state.items.get(id).expect("entry exists");
    assert_eq!(entry.page_count, 1);
    assert_eq!(entry.error, None);

    let png = make_thumbnail(&state, id, None).expect("thumbnail generated");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn unlock_propagates_password_to_matching_protected_items_in_id_order() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let copy1 = put(dir.path(), "encrypted.pdf", "copy1.pdf");
    let copy2 = put(dir.path(), "encrypted.pdf", "copy2.pdf");
    let cafe = put(dir.path(), "encrypted_cafe.pdf", "cafe.pdf");
    let shapes = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let corrupt = put(dir.path(), "corrupt.pdf", "corrupt.pdf");

    let state = state();
    let added = add_pdfs(&state, &[copy1, copy2, cafe, shapes, corrupt]).added;
    assert_eq!(added.len(), 5);

    let id1 = added[0].id;
    let id2 = added[1].id;
    let id_cafe = added[2].id;
    let id_shapes = added[3].id;
    let id_corrupt = added[4].id;

    let res = unlock_pdf(&state, id1, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    assert_eq!(res.items.len(), 2);
    assert_eq!(res.items[0].id, id1);
    assert_eq!(res.items[1].id, id2);
    assert_eq!(res.items[0].page_count, 1);
    assert_eq!(res.items[1].page_count, 1);
    assert_eq!(res.items[0].error, None);
    assert_eq!(res.items[1].error, None);

    // Other items are unaffected
    let cafe_entry = state.items.get(id_cafe).expect("cafe exists");
    assert_eq!(
        cafe_entry.error.as_ref().map(|e| e.code),
        Some(ErrorCode::PasswordProtected)
    );
    assert_eq!(cafe_entry.page_count, 0);

    let shapes_entry = state.items.get(id_shapes).expect("shapes exists");
    assert_eq!(shapes_entry.error, None);
    assert_eq!(shapes_entry.page_count, 3);

    let corrupt_entry = state.items.get(id_corrupt).expect("corrupt exists");
    assert_eq!(
        corrupt_entry.error.as_ref().map(|e| e.code),
        Some(ErrorCode::PdfOpenFailed)
    );
}

#[test]
fn unlock_invalid_params_and_unknown_handle() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let enc_path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let shapes_path = put(dir.path(), "shapes.pdf", "shapes.pdf");
    let img_path = put(dir.path(), "photo.jpg", "photo.jpg");

    let state = state();
    let enc_id = add_pdfs(&state, &[enc_path]).added[0].id;
    let shapes_id = add_pdfs(&state, &[shapes_path]).added[0].id;
    let img_id = add_images(&state, &[img_path]).added[0].id;

    // Empty password
    let err_empty = unlock_pdf(&state, enc_id, "").expect_err("empty password rejected");
    assert_eq!(err_empty.code, ErrorCode::InvalidParams);

    // Target not PasswordProtected: shapes.pdf
    let err_shapes =
        unlock_pdf(&state, shapes_id, ENCRYPTED_USER_PASSWORD).expect_err("shapes rejected");
    assert_eq!(err_shapes.code, ErrorCode::InvalidParams);

    // Target not a PDF: image
    let err_img = unlock_pdf(&state, img_id, ENCRYPTED_USER_PASSWORD).expect_err("image rejected");
    assert_eq!(err_img.code, ErrorCode::InvalidParams);

    // Unknown handle
    let err_unknown =
        unlock_pdf(&state, 999_999, ENCRYPTED_USER_PASSWORD).expect_err("unknown handle rejected");
    assert_eq!(err_unknown.code, ErrorCode::UnknownHandle);

    // Once unlocked, trying to unlock again gives InvalidParams
    unlock_pdf(&state, enc_id, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    let err_already =
        unlock_pdf(&state, enc_id, ENCRYPTED_USER_PASSWORD).expect_err("already unlocked rejected");
    assert_eq!(err_already.code, ErrorCode::InvalidParams);
}

#[test]
fn unlock_fails_with_conversion_running() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    state.is_running.store(true, Ordering::SeqCst);
    let err = unlock_pdf(&state, id, ENCRYPTED_USER_PASSWORD).expect_err("conversion running");
    assert_eq!(err.code, ErrorCode::ConversionRunning);
}

#[test]
fn nul_byte_password_returns_password_protected_without_crashing_worker() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    let err = unlock_pdf(&state, id, "fixture\0user").expect_err("nul byte rejected");
    assert_eq!(err.code, ErrorCode::PasswordProtected);

    // Worker remains usable and immediately unlocks with valid password
    let res = unlock_pdf(&state, id, ENCRYPTED_USER_PASSWORD).expect("unlock succeeds");
    assert_eq!(res.items.len(), 1);
    assert_eq!(res.items[0].page_count, 1);
}

#[test]
fn whitespace_padded_password_fails_to_unlock() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let state = state();
    let id = add_pdfs(&state, &[path]).added[0].id;

    let padded = format!("  {}  ", ENCRYPTED_USER_PASSWORD);
    let err = unlock_pdf(&state, id, &padded).expect_err("padded password fails");
    assert_eq!(err.code, ErrorCode::PasswordProtected);

    let entry = state.items.get(id).expect("entry exists");
    assert_eq!(
        entry.error.as_ref().map(|e| e.code),
        Some(ErrorCode::PasswordProtected)
    );
}

#[test]
fn owner_password_and_cafe_password_unlock() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let enc_path = put(dir.path(), "encrypted.pdf", "encrypted.pdf");
    let cafe_path = put(dir.path(), "encrypted_cafe.pdf", "cafe.pdf");
    let state = state();

    let enc_id = add_pdfs(&state, &[enc_path]).added[0].id;
    let cafe_id = add_pdfs(&state, &[cafe_path]).added[0].id;

    // café user password unlocks only encrypted_cafe.pdf
    let cafe_res =
        unlock_pdf(&state, cafe_id, ENCRYPTED_CAFE_USER_PASSWORD).expect("cafe password unlocks");
    assert_eq!(cafe_res.items.len(), 1);
    assert_eq!(cafe_res.items[0].page_count, 1);

    // Owner password unlocks encrypted.pdf (cafe is already unlocked)
    let enc_res = unlock_pdf(&state, enc_id, OWNER_PASSWORD).expect("owner password unlocks");
    assert_eq!(enc_res.items.len(), 1);
    assert_eq!(enc_res.items[0].page_count, 1);
}

#[test]
fn tiff_items_report_format_and_page_count() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let g4_path = put(dir.path(), "scan_g4.tif", "scan.tif");
    let corrupt_path = put(dir.path(), "corrupt_page2.tif", "corrupt.tiff");
    let state = state();

    let result = add_images(&state, &[g4_path, corrupt_path]);
    assert_eq!(result.added.len(), 2);

    let item0 = &result.added[0];
    assert_eq!(item0.name, "scan.tif");
    assert_eq!(item0.format, Some(ImageFormatName::Tiff));
    assert_eq!(item0.page_count, 3);
    assert_eq!(item0.error, None);

    let json0 = serde_json::to_value(item0).expect("serializing scan.tif");
    assert_eq!(json0["format"], "tiff");
    assert_eq!(json0["pageCount"], 3);

    let item1 = &result.added[1];
    assert_eq!(item1.name, "corrupt.tiff");
    assert_eq!(item1.format, Some(ImageFormatName::Tiff));
    assert_eq!(item1.page_count, 2);
    assert_eq!(item1.error, None);

    let json1 = serde_json::to_value(item1).expect("serializing corrupt.tiff");
    assert_eq!(json1["format"], "tiff");
    assert_eq!(json1["pageCount"], 2);
}

#[test]
fn tiff_error_items_detail_and_codes() {
    let dir = tempfile::tempdir().expect("creating a temporary folder");
    let too_many = put(dir.path(), "too_many_pages.tif", "too_many.tif");
    let too_large = put(dir.path(), "too_large_page2.tif", "too_large.tif");
    let cmyk = put(dir.path(), "cmyk.tif", "cmyk.tif");
    let palette = put(dir.path(), "palette.tif", "palette.tif");
    let state = state();

    let result = add_images(&state, &[too_many, too_large, cmyk, palette]);
    assert_eq!(result.added.len(), 4);

    let err_many = result.added[0]
        .error
        .as_ref()
        .expect("too many pages error");
    assert_eq!(err_many.code, ErrorCode::TooManyPages);
    assert_eq!(err_many.detail.as_deref(), Some("1,000"));
    assert_eq!(result.added[0].page_count, 0);

    let err_large = result.added[1].error.as_ref().expect("too large error");
    assert_eq!(err_large.code, ErrorCode::TooLarge);
    assert_eq!(result.added[1].page_count, 0);

    let err_cmyk = result.added[2].error.as_ref().expect("cmyk error");
    assert_eq!(err_cmyk.code, ErrorCode::UnsupportedFormat);
    assert_eq!(result.added[2].page_count, 0);

    let err_palette = result.added[3].error.as_ref().expect("palette error");
    assert_eq!(err_palette.code, ErrorCode::UnsupportedFormat);
    assert_eq!(result.added[3].page_count, 0);
}
