//! Integration tests for the settings file and the output folders (work-plan
//! T09, design §6.5, §6.7).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pdf_converter_lib::AppState;
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::jobs::{
    PageOrientationChoice, PageSizeChoice, RenderColorChoice, RenderFormatChoice,
};
use pdf_converter_lib::settings::{
    CropOptions, ImagesToPdfOptions, Language, OutputKind, OutputMode, PdfToImagesOptions,
    SETTINGS_FILE_NAME, SettingsFile, SettingsInput, apply_picked_dir, dir_label,
    get_settings_internal, load_settings, restore_settings, save_settings_to_dir,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_worker::{DEFAULT_RENDER_DPI, RENDER_DPI_MAX, RENDER_DPI_MIN};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A state whose pool is never used, so no worker is started.
fn state() -> AppState {
    AppState::new(WorkerPool::new(WorkerPoolConfig::new(
        "unused-worker",
        Vec::<String>::new(),
    )))
}

/// The `revision` the next [`save`] passes. One counter for all the tests is
/// enough: each store only needs higher numbers than it has seen.
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

/// What `save_settings` does, with the write it leaves to another thread made
/// at once.
fn save(state: &AppState, input: &SettingsInput) -> Result<(), IpcError> {
    let revision = NEXT_REVISION.fetch_add(1, Ordering::Relaxed);
    state.settings.update(revision, input)?;
    state.settings.flush()
}

fn write_settings(config_dir: &Path, json: &Value) {
    fs::write(config_dir.join(SETTINGS_FILE_NAME), json.to_string()).expect("writing settings");
}

fn new_dir(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir(&dir).expect("creating a folder");
    dir
}

/// A file whose items differ from the defaults, with folders that exist (the crop folder is set
/// by the tests that need one). `encrypt` stays off: PDF/A is on, and the two exclude each other.
fn customized(images_dir: &Path, pdfs_dir: &Path) -> Value {
    json!({
        "schemaVersion": 1,
        "language": "en",
        "imagesToPdf": {
            "output": "each",
            "pageSize": "a4",
            "orientation": "portrait",
            "pdfA": true,
            "encrypt": false,
            "outputDir": images_dir,
        },
        "pdfToImages": {
            "format": "jpeg",
            "colorMode": "gray",
            "dpi": 300,
            "outputDir": pdfs_dir,
        },
        "crop": {
            "format": "webp",
            "colorMode": "gray",
            "dpi": 72,
            "photoNative": false,
            "photoOnly": true,
            "outputDir": null,
        },
    })
}

fn defaults() -> SettingsFile {
    SettingsFile::default()
}

fn input() -> SettingsInput {
    SettingsInput {
        language: Some(Language::En),
        images_to_pdf: ImagesToPdfOptions {
            output: OutputMode::Each,
            page_size: PageSizeChoice::A4,
            orientation: PageOrientationChoice::Portrait,
            pdf_a: true,
            encrypt: false,
        },
        pdf_to_images: PdfToImagesOptions {
            format: RenderFormatChoice::Jpeg,
            color_mode: RenderColorChoice::Gray,
            dpi: 72,
        },
        crop: crop_options(),
    }
}

/// Crop options that differ from the defaults in every item.
fn crop_options() -> CropOptions {
    CropOptions {
        format: RenderFormatChoice::Webp,
        color_mode: RenderColorChoice::Gray,
        dpi: 72,
        photo_native: false,
        photo_only: true,
    }
}

fn leftovers(config_dir: &Path) -> Vec<String> {
    fs::read_dir(config_dir)
        .expect("listing the settings folder")
        .map(|entry| entry.expect("reading an entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| name != SETTINGS_FILE_NAME)
        .collect()
}

#[test]
fn the_defaults_are_those_of_the_design() {
    let file = defaults();
    assert_eq!(file.schema_version, 1);
    assert_eq!(file.language, None);
    assert_eq!(file.images_to_pdf.options.output, OutputMode::Merge);
    assert_eq!(file.images_to_pdf.options.page_size, PageSizeChoice::Fit);
    assert_eq!(
        file.images_to_pdf.options.orientation,
        PageOrientationChoice::Auto
    );
    assert!(!file.images_to_pdf.options.pdf_a);
    assert!(!file.images_to_pdf.options.encrypt);
    assert_eq!(file.images_to_pdf.output_dir, None);
    assert_eq!(file.pdf_to_images.options.format, RenderFormatChoice::Png);
    assert_eq!(
        file.pdf_to_images.options.color_mode,
        RenderColorChoice::Color
    );
    assert_eq!(file.pdf_to_images.options.dpi, 150);
    assert_eq!(file.pdf_to_images.options.dpi, DEFAULT_RENDER_DPI);
    assert_eq!(file.pdf_to_images.output_dir, None);
    assert_eq!(
        file.crop.options,
        CropOptions {
            format: RenderFormatChoice::Png,
            color_mode: RenderColorChoice::Color,
            dpi: 150,
            photo_native: true,
            photo_only: false,
        }
    );
    assert_eq!(file.crop.output_dir, None);
}

#[test]
fn a_missing_file_gives_the_defaults() {
    let temp = TempDir::new().expect("temp dir");
    assert_eq!(load_settings(temp.path()), defaults());
}

#[test]
fn what_is_saved_is_what_is_loaded() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let config_dir = temp.path().join("config");
    let mut file = defaults();
    file.language = Some(Language::Ja);
    file.images_to_pdf.options = input().images_to_pdf;
    file.pdf_to_images.options = input().pdf_to_images;
    file.images_to_pdf.output_dir = Some(images_dir);
    file.pdf_to_images.output_dir = Some(pdfs_dir);
    file.crop.options = crop_options();
    file.crop.output_dir = Some(new_dir(temp.path(), "crop-out"));

    save_settings_to_dir(&config_dir, &file).expect("saving");

    assert_eq!(load_settings(&config_dir), file);
}

#[test]
fn the_file_has_the_shape_of_the_design() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let config_dir = temp.path().join("config");
    let expected = customized(&images_dir, &pdfs_dir);

    save_settings_to_dir(&config_dir, &load_from(&config_dir, &expected)).expect("saving");

    let written: Value =
        serde_json::from_str(&fs::read_to_string(config_dir.join(SETTINGS_FILE_NAME)).unwrap())
            .expect("the file is JSON");
    assert_eq!(written, expected);
}

/// What loading `json` gives, with `config_dir` created for the file.
fn load_from(config_dir: &Path, json: &Value) -> SettingsFile {
    fs::create_dir_all(config_dir).expect("creating the settings folder");
    write_settings(config_dir, json);
    load_settings(config_dir)
}

#[test]
fn broken_json_gives_the_defaults_for_everything() {
    let temp = TempDir::new().expect("temp dir");
    for text in ["", "{", "not json", "[1, 2]", "null"] {
        fs::write(temp.path().join(SETTINGS_FILE_NAME), text).expect("writing settings");
        assert_eq!(load_settings(temp.path()), defaults(), "for {text:?}");
    }
}

#[test]
fn an_unknown_schema_version_gives_the_defaults_for_everything() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    for version in [json!(2), json!(0), json!("1"), json!(null), json!(1.5)] {
        let mut file = customized(&images_dir, &pdfs_dir);
        file["schemaVersion"] = version.clone();
        write_settings(temp.path(), &file);
        assert_eq!(load_settings(temp.path()), defaults(), "for {version}");
    }

    let mut file = customized(&images_dir, &pdfs_dir);
    file.as_object_mut()
        .expect("an object")
        .remove("schemaVersion");
    write_settings(temp.path(), &file);
    assert_eq!(load_settings(temp.path()), defaults());
}

/// Loads `customized` after `damage` changed it, and checks that exactly the
/// items `reset` names went back to their defaults.
fn assert_only_resets(damage: impl FnOnce(&mut Value), reset: impl FnOnce(&mut SettingsFile)) {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let mut json = customized(&images_dir, &pdfs_dir);
    damage(&mut json);
    write_settings(temp.path(), &json);

    let mut expected = load_from(
        &temp.path().join("intact"),
        &customized(&images_dir, &pdfs_dir),
    );
    assert_eq!(expected.language, Some(Language::En), "the intact file");
    assert_eq!(expected.pdf_to_images.options.dpi, 300, "the intact file");
    reset(&mut expected);

    assert_eq!(load_settings(temp.path()), expected);
}

#[test]
fn a_missing_folder_resets_only_that_folder() {
    assert_only_resets(
        |json| json["imagesToPdf"]["outputDir"] = json!("/no/such/folder/for/pdf-converter"),
        |file| file.images_to_pdf.output_dir = None,
    );
    assert_only_resets(
        |json| json["pdfToImages"]["outputDir"] = json!("/no/such/folder/for/pdf-converter"),
        |file| file.pdf_to_images.output_dir = None,
    );
}

#[test]
fn a_relative_folder_resets_the_folder() {
    // Tests run in the package folder, where `src` is a folder, so only the
    // path being relative can reset it.
    assert!(Path::new("src").is_dir(), "src is a folder from here");
    assert_only_resets(
        |json| json["imagesToPdf"]["outputDir"] = json!("src"),
        |file| file.images_to_pdf.output_dir = None,
    );
}

#[test]
fn a_file_in_place_of_a_folder_resets_the_folder() {
    let temp = TempDir::new().expect("temp dir");
    let not_a_dir = temp.path().join("file.txt");
    fs::write(&not_a_dir, "x").expect("writing a file");
    assert_only_resets(
        |json| json["imagesToPdf"]["outputDir"] = json!(not_a_dir),
        |file| file.images_to_pdf.output_dir = None,
    );
}

#[test]
fn a_dpi_out_of_range_or_wrong_type_resets_only_the_dpi() {
    for dpi in [
        json!(35),
        json!(1001),
        json!(0),
        json!(-150),
        json!(150.5),
        json!("150"),
        json!(1e12),
    ] {
        assert_only_resets(
            |json| json["pdfToImages"]["dpi"] = dpi.clone(),
            |file| file.pdf_to_images.options.dpi = DEFAULT_RENDER_DPI,
        );
    }
    for valid in [RENDER_DPI_MIN, RENDER_DPI_MAX, 600, 72, 150, 300] {
        assert_only_resets(
            |json| json["pdfToImages"]["dpi"] = json!(valid),
            |file| file.pdf_to_images.options.dpi = valid,
        );
    }
}

#[test]
fn an_unknown_color_mode_resets_only_the_color_mode() {
    for bad in [json!("sepia"), json!(123), json!(true), json!(null)] {
        assert_only_resets(
            |json| json["pdfToImages"]["colorMode"] = bad.clone(),
            |file| file.pdf_to_images.options.color_mode = RenderColorChoice::Color,
        );
    }
    assert_only_resets(
        |json| {
            json["pdfToImages"]
                .as_object_mut()
                .expect("section")
                .remove("colorMode");
        },
        |file| file.pdf_to_images.options.color_mode = RenderColorChoice::Color,
    );
    for valid in [RenderColorChoice::Color, RenderColorChoice::Gray] {
        assert_only_resets(
            |json| json["pdfToImages"]["colorMode"] = json!(valid),
            |file| file.pdf_to_images.options.color_mode = valid,
        );
    }
}

#[test]
fn an_unknown_output_resets_only_the_output() {
    assert_only_resets(
        |json| json["imagesToPdf"]["output"] = json!("zip"),
        |file| file.images_to_pdf.options.output = OutputMode::Merge,
    );
}

#[test]
fn an_unknown_page_size_resets_only_the_page_size() {
    assert_only_resets(
        |json| json["imagesToPdf"]["pageSize"] = json!("b5"),
        |file| file.images_to_pdf.options.page_size = PageSizeChoice::Fit,
    );
}

#[test]
fn an_unknown_orientation_resets_only_the_orientation() {
    assert_only_resets(
        |json| json["imagesToPdf"]["orientation"] = json!("diagonal"),
        |file| file.images_to_pdf.options.orientation = PageOrientationChoice::Auto,
    );
}

#[test]
fn a_missing_orientation_resets_only_the_orientation() {
    assert_only_resets(
        |json| {
            json["imagesToPdf"]
                .as_object_mut()
                .unwrap()
                .remove("orientation");
        },
        |file| file.images_to_pdf.options.orientation = PageOrientationChoice::Auto,
    );
}

#[test]
fn a_missing_pdf_a_resets_only_the_pdf_a() {
    assert_only_resets(
        |json| {
            json["imagesToPdf"].as_object_mut().unwrap().remove("pdfA");
        },
        |file| file.images_to_pdf.options.pdf_a = false,
    );
}

#[test]
fn a_pdf_a_of_the_wrong_type_resets_only_the_pdf_a() {
    for wrong in [json!("true"), json!(1), json!(null), json!([true])] {
        assert_only_resets(
            |json| json["imagesToPdf"]["pdfA"] = wrong.clone(),
            |file| file.images_to_pdf.options.pdf_a = false,
        );
    }
}

/// `customized` with `pdfA` off and `encrypt` on, as a file written after the user chose a password.
fn customized_with_password(images_dir: &Path, pdfs_dir: &Path) -> Value {
    let mut json = customized(images_dir, pdfs_dir);
    json["imagesToPdf"]["pdfA"] = json!(false);
    json["imagesToPdf"]["encrypt"] = json!(true);
    json
}

#[test]
fn a_file_with_encrypt_on_loads_it() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let file = load_from(
        temp.path(),
        &customized_with_password(&images_dir, &pdfs_dir),
    );
    assert!(file.images_to_pdf.options.encrypt);
    assert!(!file.images_to_pdf.options.pdf_a);
}

#[test]
fn a_missing_encrypt_means_no_password() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let mut json = customized_with_password(&images_dir, &pdfs_dir);
    json["imagesToPdf"]
        .as_object_mut()
        .expect("imagesToPdf object")
        .remove("encrypt");
    let file = load_from(temp.path(), &json);
    assert!(!file.images_to_pdf.options.encrypt);
    // The other items stay.
    assert_eq!(file.images_to_pdf.options.page_size, PageSizeChoice::A4);
    assert_eq!(file.pdf_to_images.options.dpi, 300);
}

#[test]
fn an_encrypt_of_the_wrong_type_resets_only_the_encrypt() {
    for wrong in [json!("true"), json!(1), json!(null), json!([true])] {
        let temp = TempDir::new().expect("temp dir");
        let images_dir = new_dir(temp.path(), "images-out");
        let pdfs_dir = new_dir(temp.path(), "pdfs-out");
        let mut json = customized_with_password(&images_dir, &pdfs_dir);
        json["imagesToPdf"]["encrypt"] = wrong.clone();
        let mut expected = load_from(
            &temp.path().join("intact"),
            &customized_with_password(&images_dir, &pdfs_dir),
        );
        expected.images_to_pdf.options.encrypt = false;
        assert_eq!(load_from(temp.path(), &json), expected, "for {wrong}");
    }
}

#[test]
fn a_file_with_pdf_a_and_encrypt_both_on_keeps_pdf_a_and_drops_encrypt() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let mut json = customized(&images_dir, &pdfs_dir);
    json["imagesToPdf"]["encrypt"] = json!(true);
    let file = load_from(temp.path(), &json);
    assert!(file.images_to_pdf.options.pdf_a);
    assert!(!file.images_to_pdf.options.encrypt);
    // The other items stay.
    assert_eq!(file.images_to_pdf.options.page_size, PageSizeChoice::A4);
    assert_eq!(file.pdf_to_images.options.dpi, 300);
}

#[test]
fn save_settings_keeps_encrypt() {
    let temp = TempDir::new().expect("temp dir");
    let state = state();
    restore_settings(&state, temp.path());
    let mut with_password = input();
    with_password.images_to_pdf.pdf_a = false;
    with_password.images_to_pdf.encrypt = true;
    save(&state, &with_password).expect("saving");
    assert!(load_settings(temp.path()).images_to_pdf.options.encrypt);
}

#[test]
fn save_settings_rejects_pdf_a_with_encrypt_and_changes_nothing() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    save(&state, &input()).expect("saving");
    let before = fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file");
    let held = state.settings.file();

    let mut both = input();
    both.images_to_pdf.pdf_a = true;
    both.images_to_pdf.encrypt = true;
    both.language = None;
    assert_eq!(
        save(&state, &both),
        Err(IpcError::from_code(ErrorCode::InvalidParams))
    );

    assert_eq!(
        fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file"),
        before
    );
    assert_eq!(state.settings.file(), held);
}

#[test]
fn settings_input_requires_encrypt() {
    let missing_encrypt = json!({
        "language": "ja",
        "imagesToPdf": {
            "output": "merge",
            "pageSize": "a4",
            "orientation": "auto",
            "pdfA": false
        },
        "pdfToImages": {
            "format": "png",
            "colorMode": "color",
            "dpi": 150
        }
    });
    assert!(serde_json::from_value::<SettingsInput>(missing_encrypt).is_err());
}

#[test]
fn an_unknown_format_resets_only_the_format() {
    assert_only_resets(
        |json| json["pdfToImages"]["format"] = json!("gif"),
        |file| file.pdf_to_images.options.format = RenderFormatChoice::Png,
    );
}

#[test]
fn an_unknown_language_resets_only_the_language() {
    for language in [json!("fr"), json!("JA"), json!(1), json!(null)] {
        assert_only_resets(
            |json| json["language"] = language.clone(),
            |file| file.language = None,
        );
    }
}

#[test]
fn an_item_of_the_wrong_type_resets_only_that_item() {
    assert_only_resets(
        |json| json["imagesToPdf"]["output"] = json!(1),
        |file| file.images_to_pdf.options.output = OutputMode::Merge,
    );
    assert_only_resets(
        |json| json["imagesToPdf"]["pageSize"] = json!(["a4"]),
        |file| file.images_to_pdf.options.page_size = PageSizeChoice::Fit,
    );
    assert_only_resets(
        |json| json["imagesToPdf"]["orientation"] = json!(true),
        |file| file.images_to_pdf.options.orientation = PageOrientationChoice::Auto,
    );
    assert_only_resets(
        |json| json["pdfToImages"]["format"] = json!({"png": true}),
        |file| file.pdf_to_images.options.format = RenderFormatChoice::Png,
    );
    assert_only_resets(
        |json| json["pdfToImages"]["colorMode"] = json!(123),
        |file| file.pdf_to_images.options.color_mode = RenderColorChoice::Color,
    );
    assert_only_resets(
        |json| json["pdfToImages"]["dpi"] = json!("300"),
        |file| file.pdf_to_images.options.dpi = DEFAULT_RENDER_DPI,
    );
    assert_only_resets(
        |json| json["imagesToPdf"]["outputDir"] = json!(7),
        |file| file.images_to_pdf.output_dir = None,
    );
    assert_only_resets(
        |json| json["pdfToImages"]["outputDir"] = json!(false),
        |file| file.pdf_to_images.output_dir = None,
    );
}

#[test]
fn a_missing_item_resets_only_that_item() {
    fn remove(json: &mut Value, section: &str, key: &str) {
        json[section]
            .as_object_mut()
            .expect("a section")
            .remove(key);
    }
    assert_only_resets(
        |json| remove(json, "imagesToPdf", "output"),
        |file| file.images_to_pdf.options.output = OutputMode::Merge,
    );
    assert_only_resets(
        |json| remove(json, "imagesToPdf", "pageSize"),
        |file| file.images_to_pdf.options.page_size = PageSizeChoice::Fit,
    );
    assert_only_resets(
        |json| remove(json, "imagesToPdf", "orientation"),
        |file| file.images_to_pdf.options.orientation = PageOrientationChoice::Auto,
    );
    assert_only_resets(
        |json| remove(json, "imagesToPdf", "outputDir"),
        |file| file.images_to_pdf.output_dir = None,
    );
    assert_only_resets(
        |json| remove(json, "pdfToImages", "format"),
        |file| file.pdf_to_images.options.format = RenderFormatChoice::Png,
    );
    assert_only_resets(
        |json| remove(json, "pdfToImages", "colorMode"),
        |file| file.pdf_to_images.options.color_mode = RenderColorChoice::Color,
    );
    assert_only_resets(
        |json| remove(json, "pdfToImages", "dpi"),
        |file| file.pdf_to_images.options.dpi = DEFAULT_RENDER_DPI,
    );
    assert_only_resets(
        |json| remove(json, "pdfToImages", "outputDir"),
        |file| file.pdf_to_images.output_dir = None,
    );
    assert_only_resets(
        |json| {
            json.as_object_mut().expect("an object").remove("language");
        },
        |file| file.language = None,
    );
}

#[test]
fn dev_settings_file_with_only_a4_orientation_resets_orientation_to_auto_and_preserves_other_fields()
 {
    let temp = TempDir::new().expect("temp dir");
    let dev_json = json!({
        "schemaVersion": 1,
        "language": "ja",
        "imagesToPdf": {
            "output": "each",
            "pageSize": "a4",
            "a4Orientation": "landscape",
            "outputDir": null,
        },
        "pdfToImages": {
            "format": "png",
            "dpi": 150,
            "outputDir": null,
        },
    });
    let file = load_from(temp.path(), &dev_json);
    assert_eq!(file.images_to_pdf.options.output, OutputMode::Each);
    assert_eq!(file.images_to_pdf.options.page_size, PageSizeChoice::A4);
    assert_eq!(
        file.images_to_pdf.options.orientation,
        PageOrientationChoice::Auto
    );
    assert!(!file.images_to_pdf.options.pdf_a);
}

#[test]
fn v0_1_0_settings_file_without_orientation_loads_with_auto_and_preserves_page_size() {
    let temp = TempDir::new().expect("temp dir");
    let v010_json = json!({
        "schemaVersion": 1,
        "language": "ja",
        "imagesToPdf": {
            "output": "each",
            "pageSize": "a4",
            "outputDir": null,
        },
        "pdfToImages": {
            "format": "png",
            "dpi": 150,
            "outputDir": null,
        },
    });
    let file = load_from(temp.path(), &v010_json);
    assert_eq!(file.images_to_pdf.options.output, OutputMode::Each);
    assert_eq!(file.images_to_pdf.options.page_size, PageSizeChoice::A4);
    assert_eq!(
        file.images_to_pdf.options.orientation,
        PageOrientationChoice::Auto
    );
}

#[test]
fn a_missing_or_malformed_section_resets_only_that_section() {
    for damage in [json!(null), json!("x"), json!([1])] {
        assert_only_resets(
            |json| json["imagesToPdf"] = damage.clone(),
            |file| file.images_to_pdf = defaults().images_to_pdf,
        );
        assert_only_resets(
            |json| json["pdfToImages"] = damage.clone(),
            |file| file.pdf_to_images = defaults().pdf_to_images,
        );
    }
    assert_only_resets(
        |json| {
            json.as_object_mut()
                .expect("an object")
                .remove("pdfToImages");
        },
        |file| file.pdf_to_images = defaults().pdf_to_images,
    );
}

#[test]
fn several_bad_items_reset_each_on_its_own() {
    assert_only_resets(
        |json| {
            json["imagesToPdf"]["output"] = json!("zip");
            json["pdfToImages"]["dpi"] = json!(35);
        },
        |file| {
            file.images_to_pdf.options.output = OutputMode::Merge;
            file.pdf_to_images.options.dpi = DEFAULT_RENDER_DPI;
        },
    );
}

#[test]
fn save_settings_takes_the_input_over_and_keeps_the_folders() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    let _ = apply_picked_dir(&state, OutputKind::ImagesToPdf, images_dir.clone());
    let _ = apply_picked_dir(&state, OutputKind::PdfToImages, pdfs_dir.clone());

    save(&state, &input()).expect("saving");

    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.language, Some(Language::En));
    assert_eq!(loaded.images_to_pdf.options, input().images_to_pdf);
    assert_eq!(loaded.pdf_to_images.options, input().pdf_to_images);
    assert_eq!(loaded.images_to_pdf.output_dir, Some(images_dir.clone()));
    assert_eq!(loaded.pdf_to_images.output_dir, Some(pdfs_dir.clone()));
    assert_eq!(state.settings.file(), loaded);
    assert_eq!(*state.images_output_dir.lock().unwrap(), Some(images_dir));
    assert_eq!(*state.pdfs_output_dir.lock().unwrap(), Some(pdfs_dir));
}

#[test]
fn save_settings_rejects_an_out_of_range_dpi_and_changes_nothing() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    save(&state, &input()).expect("saving");
    let before = fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file");
    let held = state.settings.file();

    for dpi in [0, 35, 1001, u32::MAX] {
        let mut bad = input();
        bad.language = None;
        bad.images_to_pdf.output = OutputMode::Merge;
        bad.pdf_to_images.dpi = dpi;
        assert_eq!(
            save(&state, &bad),
            Err(IpcError::from_code(ErrorCode::InvalidParams)),
            "for {dpi}"
        );
    }

    assert_eq!(
        fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file"),
        before
    );
    assert_eq!(state.settings.file(), held);
}

#[test]
fn save_settings_accepts_valid_dpis() {
    let temp = TempDir::new().expect("temp dir");
    let state = state();
    restore_settings(&state, temp.path());
    for dpi in [RENDER_DPI_MIN, RENDER_DPI_MAX, 600, 72, 150, 300] {
        let mut valid = input();
        valid.pdf_to_images.dpi = dpi;
        save(&state, &valid).expect("saving");
        assert_eq!(load_settings(temp.path()).pdf_to_images.options.dpi, dpi);
    }
}

#[test]
fn settings_input_requires_color_mode() {
    let images_dir = PathBuf::from("/tmp/images-out");
    let pdfs_dir = PathBuf::from("/tmp/pdfs-out");
    let mut bad_json = customized(&images_dir, &pdfs_dir);
    bad_json["pdfToImages"]
        .as_object_mut()
        .expect("pdfToImages object")
        .remove("colorMode");

    let result = serde_json::from_value::<SettingsInput>(bad_json);
    assert!(
        result.is_err(),
        "SettingsInput without colorMode must fail deserialization"
    );
}

#[test]
fn round_trips_webp_gray_and_custom_dpi() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    let mut custom = input();
    custom.pdf_to_images.format = RenderFormatChoice::Webp;
    custom.pdf_to_images.color_mode = RenderColorChoice::Gray;
    custom.pdf_to_images.dpi = 600;

    save(&state, &custom).expect("saving");

    let loaded = load_settings(&config_dir);
    assert_eq!(
        loaded.pdf_to_images.options.format,
        RenderFormatChoice::Webp
    );
    assert_eq!(
        loaded.pdf_to_images.options.color_mode,
        RenderColorChoice::Gray
    );
    assert_eq!(loaded.pdf_to_images.options.dpi, 600);
}

#[test]
fn reads_legacy_settings_file_without_color_mode() {
    let temp = TempDir::new().expect("temp dir");
    let legacy = json!({
        "schemaVersion": 1,
        "imagesToPdf": {
            "output": "merge",
            "pageSize": "fit",
            "a4Orientation": "auto",
            "outputDir": null,
        },
        "pdfToImages": {
            "format": "png",
            "dpi": 150,
            "outputDir": null,
        },
    });
    write_settings(temp.path(), &legacy);
    let loaded = load_settings(temp.path());
    assert_eq!(
        loaded.pdf_to_images.options.color_mode,
        RenderColorChoice::Color
    );
    assert_eq!(loaded.pdf_to_images.options.format, RenderFormatChoice::Png);
    assert_eq!(loaded.pdf_to_images.options.dpi, 150);
}

#[test]
fn a_write_leaves_no_temporary_file() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    save(&state, &input()).expect("saving");
    save(&state, &input()).expect("saving again");
    let _ = apply_picked_dir(&state, OutputKind::ImagesToPdf, temp.path().to_path_buf());

    assert_eq!(leftovers(&config_dir), Vec::<String>::new());
}

#[test]
fn a_write_replaces_the_file_that_is_there() {
    let temp = TempDir::new().expect("temp dir");
    write_settings(temp.path(), &json!({"schemaVersion": 1, "language": "ja"}));

    save_settings_to_dir(temp.path(), &defaults()).expect("saving");

    assert_eq!(load_settings(temp.path()), defaults());
}

#[test]
fn a_write_creates_the_settings_folder() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("not").join("yet").join("there");

    save_settings_to_dir(&config_dir, &defaults()).expect("saving");

    assert!(config_dir.join(SETTINGS_FILE_NAME).is_file());
}

#[test]
fn a_write_that_fails_is_write_failed() {
    let temp = TempDir::new().expect("temp dir");
    let a_file = temp.path().join("file");
    fs::write(&a_file, "x").expect("writing a file");

    let error = save_settings_to_dir(&a_file.join("config"), &defaults())
        .expect_err("a folder cannot be made inside a file");

    assert_eq!(error.code, ErrorCode::WriteFailed);
}

#[test]
fn the_state_gets_the_folders_that_exist_at_startup() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let missing = temp.path().join("gone");
    let config_dir = temp.path().join("config");
    fs::create_dir(&config_dir).expect("creating the settings folder");
    write_settings(
        &config_dir,
        &json!({
            "schemaVersion": 1,
            "imagesToPdf": { "outputDir": images_dir },
            "pdfToImages": { "outputDir": missing },
        }),
    );
    let state = state();

    restore_settings(&state, &config_dir);

    assert_eq!(*state.images_output_dir.lock().unwrap(), Some(images_dir));
    assert_eq!(*state.pdfs_output_dir.lock().unwrap(), None);
    let settings = get_settings_internal(&state);
    assert_eq!(
        settings.images_to_pdf.output_dir.map(|d| d.dir_label),
        Some("images-out".to_owned())
    );
    assert_eq!(settings.pdf_to_images.output_dir, None);
}

#[test]
fn the_state_gets_nothing_from_a_broken_file() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join(SETTINGS_FILE_NAME), "{").expect("writing settings");
    let state = state();

    restore_settings(&state, temp.path());

    assert_eq!(*state.images_output_dir.lock().unwrap(), None);
    assert_eq!(*state.pdfs_output_dir.lock().unwrap(), None);
    assert_eq!(state.settings.file(), defaults());
}

#[test]
fn get_settings_has_no_path() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let pdfs_dir = new_dir(temp.path(), "pdfs-out");
    let config_dir = temp.path().join("config");
    fs::create_dir(&config_dir).expect("creating the settings folder");
    write_settings(&config_dir, &customized(&images_dir, &pdfs_dir));
    let state = state();
    restore_settings(&state, &config_dir);

    let json = serde_json::to_value(get_settings_internal(&state)).expect("serializing");

    assert_eq!(
        json,
        json!({
            "language": "en",
            "imagesToPdf": {
                "output": "each",
                "pageSize": "a4",
                "orientation": "portrait",
                "pdfA": true,
                "encrypt": false,
                "outputDir": { "dirLabel": "images-out" },
            },
            "pdfToImages": {
                "format": "jpeg",
                "colorMode": "gray",
                "dpi": 300,
                "outputDir": { "dirLabel": "pdfs-out" },
            },
            "crop": {
                "format": "webp",
                "colorMode": "gray",
                "dpi": 72,
                "photoNative": false,
                "photoOnly": true,
                "outputDir": null,
            },
        })
    );
    assert!(!json.to_string().contains(temp.path().to_str().unwrap()));
}

#[test]
fn get_settings_of_the_defaults_has_no_folders() {
    let json = serde_json::to_value(get_settings_internal(&state())).expect("serializing");

    assert_eq!(
        json,
        json!({
            "language": null,
            "imagesToPdf": { "output": "merge", "pageSize": "fit", "orientation": "auto", "pdfA": false, "encrypt": false, "outputDir": null },
            "pdfToImages": { "format": "png", "colorMode": "color", "dpi": 150, "outputDir": null },
            "crop": { "format": "png", "colorMode": "color", "dpi": 150, "photoNative": true, "photoOnly": false, "outputDir": null },
        })
    );
}

#[test]
fn a_picked_folder_goes_to_the_state_and_the_file() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "for-pdfs");
    let pdfs_dir = new_dir(temp.path(), "for-images");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    let (label, persisted) = apply_picked_dir(&state, OutputKind::ImagesToPdf, images_dir.clone());

    assert_eq!(label.dir_label, "for-pdfs");
    assert_eq!(persisted, Ok(()));
    assert_eq!(
        *state.images_output_dir.lock().unwrap(),
        Some(images_dir.clone())
    );
    assert_eq!(*state.pdfs_output_dir.lock().unwrap(), None);
    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.images_to_pdf.output_dir, Some(images_dir.clone()));
    assert_eq!(loaded.pdf_to_images.output_dir, None);

    let (label, persisted) = apply_picked_dir(&state, OutputKind::PdfToImages, pdfs_dir.clone());

    assert_eq!(label.dir_label, "for-images");
    assert_eq!(persisted, Ok(()));
    assert_eq!(
        *state.images_output_dir.lock().unwrap(),
        Some(images_dir.clone())
    );
    assert_eq!(
        *state.pdfs_output_dir.lock().unwrap(),
        Some(pdfs_dir.clone())
    );
    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.images_to_pdf.output_dir, Some(images_dir));
    assert_eq!(loaded.pdf_to_images.output_dir, Some(pdfs_dir));
}

#[test]
fn a_picked_folder_keeps_the_other_settings() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    save(&state, &input()).expect("saving");

    let _ = apply_picked_dir(&state, OutputKind::PdfToImages, temp.path().to_path_buf());

    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.language, Some(Language::En));
    assert_eq!(loaded.images_to_pdf.options, input().images_to_pdf);
    assert_eq!(loaded.pdf_to_images.options, input().pdf_to_images);
}

#[test]
fn a_picked_folder_is_used_when_there_is_no_settings_folder_to_write_to() {
    let temp = TempDir::new().expect("temp dir");
    let state = state();

    let (label, persisted) =
        apply_picked_dir(&state, OutputKind::ImagesToPdf, temp.path().to_path_buf());

    assert_eq!(persisted, Ok(()));
    assert_eq!(
        *state.images_output_dir.lock().unwrap(),
        Some(temp.path().to_path_buf())
    );
    assert_eq!(
        label,
        get_settings_internal(&state)
            .images_to_pdf
            .output_dir
            .unwrap()
    );
}

#[cfg(unix)]
#[test]
fn a_picked_folder_is_used_even_when_the_file_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let picked = new_dir(temp.path(), "picked");
    let config_dir = new_dir(temp.path(), "config");
    let state = state();
    restore_settings(&state, &config_dir);
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500))
        .expect("making the settings folder read-only");

    let (label, persisted) = apply_picked_dir(&state, OutputKind::PdfToImages, picked.clone());
    let saved = save(&state, &input());
    let leftover_files = leftovers(&config_dir);

    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700))
        .expect("making the settings folder writable again");
    assert_eq!(label.dir_label, "picked");
    assert_eq!(
        persisted.expect_err("the write must fail").code,
        ErrorCode::WriteFailed
    );
    assert_eq!(*state.pdfs_output_dir.lock().unwrap(), Some(picked.clone()));
    assert_eq!(
        get_settings_internal(&state)
            .pdf_to_images
            .output_dir
            .map(|d| d.dir_label),
        Some("picked".to_owned())
    );
    assert_eq!(
        saved.expect_err("the write must fail").code,
        ErrorCode::WriteFailed
    );
    assert_eq!(leftover_files, Vec::<String>::new());
    assert!(!config_dir.join(SETTINGS_FILE_NAME).exists());

    // The folder was kept in memory, so the next successful write has it.
    save(&state, &input()).expect("saving once writable");
    assert_eq!(
        load_settings(&config_dir).pdf_to_images.output_dir,
        Some(picked)
    );
}

#[cfg(unix)]
#[test]
fn a_folder_whose_name_is_not_unicode_is_used_but_not_remembered() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let temp = TempDir::new().expect("temp dir");
    let picked = new_dir(temp.path(), "placeholder");
    let odd = temp.path().join(OsStr::from_bytes(b"odd-\xff-name"));
    fs::rename(&picked, &odd).expect("renaming");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    let (_, persisted) = apply_picked_dir(&state, OutputKind::ImagesToPdf, odd.clone());

    assert_eq!(persisted, Ok(()));
    assert_eq!(*state.images_output_dir.lock().unwrap(), Some(odd));
    assert_eq!(load_settings(&config_dir).images_to_pdf.output_dir, None);
    save(&state, &input()).expect("the file stays writable");
}

#[test]
fn the_label_is_the_name_of_the_folder() {
    assert_eq!(dir_label(Path::new("/home/user/Documents/out")), "out");
    assert_eq!(dir_label(Path::new("/home/user/Documents/out/")), "out");
    assert_eq!(dir_label(Path::new("/")), "/");
}

#[test]
fn seven_paper_sizes_and_orientations_roundtrip() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    let papers = [
        PageSizeChoice::A3,
        PageSizeChoice::A4,
        PageSizeChoice::A5,
        PageSizeChoice::JisB4,
        PageSizeChoice::JisB5,
        PageSizeChoice::Letter,
        PageSizeChoice::Legal,
    ];
    let orientations = [
        PageOrientationChoice::Auto,
        PageOrientationChoice::Portrait,
        PageOrientationChoice::Landscape,
    ];

    for paper in papers {
        for orientation in orientations {
            let mut inp = input();
            inp.images_to_pdf.page_size = paper;
            inp.images_to_pdf.orientation = orientation;

            save(&state, &inp).expect("saving settings");
            let loaded = load_settings(&config_dir);
            assert_eq!(loaded.images_to_pdf.options.page_size, paper);
            assert_eq!(loaded.images_to_pdf.options.orientation, orientation);
        }
    }
}

#[test]
fn save_settings_rejects_missing_orientation_and_unknown_page_size() {
    // Missing orientation
    let missing_orientation = json!({
        "language": "ja",
        "imagesToPdf": {
            "output": "merge",
            "pageSize": "a4",
            "pdfA": false,
            "encrypt": false
        },
        "pdfToImages": {
            "format": "png",
            "colorMode": "color",
            "dpi": 150
        }
    });
    assert!(serde_json::from_value::<SettingsInput>(missing_orientation).is_err());

    // Unknown pageSize ("b5")
    let unknown_page_size = json!({
        "language": "ja",
        "imagesToPdf": {
            "output": "merge",
            "pageSize": "b5",
            "orientation": "auto",
            "pdfA": false,
            "encrypt": false
        },
        "pdfToImages": {
            "format": "png",
            "colorMode": "color",
            "dpi": 150
        }
    });
    assert!(serde_json::from_value::<SettingsInput>(unknown_page_size).is_err());

    // Missing pdfA
    let missing_pdf_a = json!({
        "language": "ja",
        "imagesToPdf": {
            "output": "merge",
            "pageSize": "a4",
            "orientation": "auto",
            "encrypt": false
        },
        "pdfToImages": {
            "format": "png",
            "colorMode": "color",
            "dpi": 150
        }
    });
    assert!(serde_json::from_value::<SettingsInput>(missing_pdf_a).is_err());
}

#[test]
fn an_update_reaches_the_file_only_on_flush() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    state.settings.update(1, &input()).expect("updating");
    assert_eq!(state.settings.file().language, Some(Language::En));
    assert!(!config_dir.join(SETTINGS_FILE_NAME).exists());

    state.settings.flush().expect("flushing");
    assert_eq!(load_settings(&config_dir), state.settings.file());
}

#[test]
fn a_flush_with_nothing_new_does_not_write() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    state.settings.flush().expect("flushing");
    assert!(!config_dir.join(SETTINGS_FILE_NAME).exists());

    state.settings.update(1, &input()).expect("updating");
    state.settings.flush().expect("flushing");
    fs::remove_file(config_dir.join(SETTINGS_FILE_NAME)).expect("removing the file");
    state.settings.flush().expect("flushing again");
    assert!(!config_dir.join(SETTINGS_FILE_NAME).exists());
}

#[test]
fn an_update_older_than_one_taken_is_ignored() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    let mut older = input();
    older.pdf_to_images.dpi = 72;
    let mut newer = input();
    newer.pdf_to_images.dpi = 300;

    state.settings.update(2, &newer).expect("updating");
    state.settings.update(1, &older).expect("updating");
    state.settings.update(2, &older).expect("updating");
    state.settings.flush().expect("flushing");

    assert_eq!(load_settings(&config_dir).pdf_to_images.options.dpi, 300);
}

#[test]
fn a_new_page_counts_its_revisions_from_the_start() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    let mut first = input();
    first.pdf_to_images.dpi = 72;
    let mut after_reload = input();
    after_reload.pdf_to_images.dpi = 300;

    state.settings.update(5, &first).expect("updating");
    state.settings.start_page();
    state.settings.update(1, &after_reload).expect("updating");
    state.settings.flush().expect("flushing");

    assert_eq!(load_settings(&config_dir).pdf_to_images.options.dpi, 300);
}

#[cfg(unix)]
#[test]
fn a_failed_flush_is_tried_again_by_the_next() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let config_dir = new_dir(temp.path(), "config");
    let state = state();
    restore_settings(&state, &config_dir);
    state.settings.update(1, &input()).expect("updating");

    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500))
        .expect("making the settings folder read-only");
    let failed = state.settings.flush();
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700))
        .expect("making the settings folder writable again");

    assert_eq!(
        failed.expect_err("the write must fail").code,
        ErrorCode::WriteFailed
    );
    state.settings.flush().expect("flushing again");
    assert_eq!(load_settings(&config_dir), state.settings.file());
}

// ---- The crop settings (design §6.7) ----

#[test]
fn a_file_without_crop_gives_the_crop_defaults_and_keeps_the_rest() {
    // A file written before the crop tab existed.
    assert_only_resets(
        |json| {
            json.as_object_mut().expect("an object").remove("crop");
        },
        |file| file.crop = defaults().crop,
    );
}

#[test]
fn a_crop_that_is_not_an_object_gives_the_crop_defaults() {
    for wrong in [json!("png"), json!(1), json!(null), json!([1])] {
        assert_only_resets(
            |json| json["crop"] = wrong.clone(),
            |file| file.crop = defaults().crop,
        );
    }
}

#[test]
fn a_crop_dpi_out_of_range_or_wrong_type_resets_only_that_dpi() {
    for dpi in [
        json!(35),
        json!(1001),
        json!(0),
        json!(-150),
        json!(150.5),
        json!("150"),
    ] {
        assert_only_resets(
            |json| json["crop"]["dpi"] = dpi.clone(),
            |file| file.crop.options.dpi = DEFAULT_RENDER_DPI,
        );
    }
    for valid in [RENDER_DPI_MIN, RENDER_DPI_MAX, 600] {
        assert_only_resets(
            |json| json["crop"]["dpi"] = json!(valid),
            |file| file.crop.options.dpi = valid,
        );
    }
}

#[test]
fn an_unknown_crop_format_or_color_mode_resets_only_that_item() {
    for bad in [json!("gif"), json!("PNG"), json!(1), json!(null)] {
        assert_only_resets(
            |json| json["crop"]["format"] = bad.clone(),
            |file| file.crop.options.format = RenderFormatChoice::Png,
        );
    }
    for bad in [json!("sepia"), json!(1), json!(null)] {
        assert_only_resets(
            |json| json["crop"]["colorMode"] = bad.clone(),
            |file| file.crop.options.color_mode = RenderColorChoice::Color,
        );
    }
}

#[test]
fn a_crop_photo_option_of_the_wrong_type_resets_only_that_option() {
    for wrong in [json!("true"), json!(1), json!(null), json!([true])] {
        assert_only_resets(
            |json| json["crop"]["photoNative"] = wrong.clone(),
            |file| file.crop.options.photo_native = true,
        );
        assert_only_resets(
            |json| json["crop"]["photoOnly"] = wrong.clone(),
            |file| file.crop.options.photo_only = false,
        );
    }
}

#[test]
fn a_missing_crop_item_gives_its_default() {
    type Reset = fn(&mut SettingsFile);
    let cases: [(&str, Reset); 5] = [
        ("format", |file| {
            file.crop.options.format = RenderFormatChoice::Png;
        }),
        ("colorMode", |file| {
            file.crop.options.color_mode = RenderColorChoice::Color;
        }),
        ("dpi", |file| file.crop.options.dpi = DEFAULT_RENDER_DPI),
        ("photoNative", |file| file.crop.options.photo_native = true),
        ("photoOnly", |file| file.crop.options.photo_only = false),
    ];
    for (key, reset) in cases {
        assert_only_resets(
            |json| {
                json["crop"].as_object_mut().expect("section").remove(key);
            },
            reset,
        );
    }
}

#[test]
fn a_missing_crop_folder_resets_only_that_folder() {
    let temp = TempDir::new().expect("temp dir");
    let crop_dir = new_dir(temp.path(), "crop-out");
    let kept = load_from(
        &temp.path().join("kept"),
        &with_crop_dir(&temp, json!(crop_dir)),
    );
    assert_eq!(kept.crop.output_dir, Some(crop_dir));

    for bad in [
        json!("/no/such/folder/for/pdf-converter"),
        json!("src"),
        json!(1),
    ] {
        let loaded = load_from(&temp.path().join("bad"), &with_crop_dir(&temp, bad.clone()));
        let mut expected = kept.clone();
        expected.crop.output_dir = None;
        assert_eq!(loaded, expected, "for {bad}");
    }
}

/// `customized` with the crop folder `dir`, with its other folders in `temp`.
fn with_crop_dir(temp: &TempDir, dir: Value) -> Value {
    let images_dir = temp.path().join("images-out");
    let pdfs_dir = temp.path().join("pdfs-out");
    for dir in [&images_dir, &pdfs_dir] {
        fs::create_dir_all(dir).expect("creating a folder");
    }
    let mut json = customized(&images_dir, &pdfs_dir);
    json["crop"]["outputDir"] = dir;
    json
}

#[test]
fn the_state_gets_the_crop_folder_if_it_exists_and_nothing_if_it_does_not() {
    let temp = TempDir::new().expect("temp dir");
    let crop_dir = new_dir(temp.path(), "crop-out");
    let config_dir = new_dir(temp.path(), "config");
    write_settings(
        &config_dir,
        &json!({ "schemaVersion": 1, "crop": { "outputDir": crop_dir } }),
    );
    let with_folder = state();

    restore_settings(&with_folder, &config_dir);

    assert_eq!(*with_folder.crop_output_dir.lock().unwrap(), Some(crop_dir));
    assert_eq!(
        get_settings_internal(&with_folder)
            .crop
            .output_dir
            .map(|d| d.dir_label),
        Some("crop-out".to_owned())
    );

    write_settings(
        &config_dir,
        &json!({ "schemaVersion": 1, "crop": { "outputDir": temp.path().join("gone") } }),
    );
    let after_it_is_gone = state();

    restore_settings(&after_it_is_gone, &config_dir);

    assert_eq!(*after_it_is_gone.crop_output_dir.lock().unwrap(), None);
    assert_eq!(
        get_settings_internal(&after_it_is_gone).crop.output_dir,
        None
    );
}

#[test]
fn a_picked_crop_folder_goes_to_the_state_and_the_file_and_leaves_the_others() {
    let temp = TempDir::new().expect("temp dir");
    let crop_dir = new_dir(temp.path(), "for-crops");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    let (label, persisted) = apply_picked_dir(&state, OutputKind::Crop, crop_dir.clone());

    assert_eq!(label.dir_label, "for-crops");
    assert_eq!(persisted, Ok(()));
    assert_eq!(
        *state.crop_output_dir.lock().unwrap(),
        Some(crop_dir.clone())
    );
    assert_eq!(*state.images_output_dir.lock().unwrap(), None);
    assert_eq!(*state.pdfs_output_dir.lock().unwrap(), None);
    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.crop.output_dir, Some(crop_dir));
    assert_eq!(loaded.images_to_pdf.output_dir, None);
    assert_eq!(loaded.pdf_to_images.output_dir, None);
}

#[test]
fn save_settings_saves_the_crop_options_and_keeps_the_crop_folder() {
    let temp = TempDir::new().expect("temp dir");
    let crop_dir = new_dir(temp.path(), "crop-out");
    let config_dir = new_dir(temp.path(), "config");
    write_settings(
        &config_dir,
        &json!({ "schemaVersion": 1, "crop": { "outputDir": crop_dir } }),
    );
    let state = state();
    restore_settings(&state, &config_dir);
    assert_eq!(state.settings.file().crop.options, defaults().crop.options);

    save(&state, &input()).expect("saving");

    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.language, Some(Language::En));
    assert_eq!(loaded.crop.options, crop_options());
    assert_eq!(loaded.crop.output_dir, Some(crop_dir.clone()));
    assert_eq!(state.settings.file(), loaded);
    assert_eq!(*state.crop_output_dir.lock().unwrap(), Some(crop_dir));
}

#[test]
fn save_settings_rejects_an_out_of_range_crop_dpi_and_changes_nothing() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    save(&state, &input()).expect("saving");
    let before = fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file");
    let held = state.settings.file();

    for dpi in [0, 35, 1001, u32::MAX] {
        let mut bad = input();
        bad.language = None;
        bad.crop.photo_only = !bad.crop.photo_only;
        bad.crop.dpi = dpi;
        assert_eq!(
            save(&state, &bad),
            Err(IpcError::from_code(ErrorCode::InvalidParams)),
            "for {dpi}"
        );
    }

    assert_eq!(
        fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file"),
        before
    );
    assert_eq!(state.settings.file(), held);
}

#[test]
fn save_settings_accepts_valid_crop_dpis() {
    let temp = TempDir::new().expect("temp dir");
    let state = state();
    restore_settings(&state, temp.path());
    for dpi in [RENDER_DPI_MIN, RENDER_DPI_MAX, 600, 72, 150, 300] {
        let mut valid = input();
        valid.crop.dpi = dpi;
        save(&state, &valid).expect("saving");
        assert_eq!(load_settings(temp.path()).crop.options.dpi, dpi);
    }
}

#[test]
fn a_folder_that_is_gone_is_cleared_for_crops_without_touching_the_others() {
    let temp = TempDir::new().expect("temp dir");
    let images_dir = new_dir(temp.path(), "images-out");
    let crop_dir = new_dir(temp.path(), "crop-out");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    let _ = apply_picked_dir(&state, OutputKind::ImagesToPdf, images_dir.clone());
    let _ = apply_picked_dir(&state, OutputKind::Crop, crop_dir);

    pdf_converter_lib::settings::clear_output_dir(&state, OutputKind::Crop).expect("clearing");

    assert_eq!(*state.crop_output_dir.lock().unwrap(), None);
    assert_eq!(
        *state.images_output_dir.lock().unwrap(),
        Some(images_dir.clone())
    );
    let loaded = load_settings(&config_dir);
    assert_eq!(loaded.crop.output_dir, None);
    assert_eq!(loaded.images_to_pdf.output_dir, Some(images_dir));
}
