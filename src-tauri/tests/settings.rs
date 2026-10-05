//! Integration tests for the settings file and the output folders (work-plan
//! T09, design §6.5, §6.7).

use std::fs;
use std::path::{Path, PathBuf};

use pdf_converter_lib::AppState;
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::jobs::{PageSizeChoice, RenderFormatChoice};
use pdf_converter_lib::settings::{
    ImagesToPdfOptions, Language, OutputKind, OutputMode, PdfToImagesOptions, SETTINGS_FILE_NAME,
    SettingsFile, SettingsInput, apply_picked_dir, dir_label, get_settings_internal, load_settings,
    restore_settings, save_settings_to_dir,
};
use pdf_converter_lib::worker_pool::{WorkerPool, WorkerPoolConfig};
use pdfconv_worker::{DEFAULT_RENDER_DPI, DPI_CHOICES};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A state whose pool is never used, so no worker is started.
fn state() -> AppState {
    AppState::new(WorkerPool::new(WorkerPoolConfig::new(
        "unused-worker",
        Vec::<String>::new(),
    )))
}

fn write_settings(config_dir: &Path, json: &Value) {
    fs::write(config_dir.join(SETTINGS_FILE_NAME), json.to_string()).expect("writing settings");
}

fn new_dir(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir(&dir).expect("creating a folder");
    dir
}

/// A file whose items all differ from the defaults, with folders that exist.
fn customized(images_dir: &Path, pdfs_dir: &Path) -> Value {
    json!({
        "schemaVersion": 1,
        "language": "en",
        "imagesToPdf": {
            "output": "each",
            "pageSize": "a4",
            "outputDir": images_dir,
        },
        "pdfToImages": {
            "format": "jpeg",
            "dpi": 300,
            "outputDir": pdfs_dir,
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
        },
        pdf_to_images: PdfToImagesOptions {
            format: RenderFormatChoice::Jpeg,
            dpi: 72,
        },
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
    assert_eq!(file.images_to_pdf.output_dir, None);
    assert_eq!(file.pdf_to_images.options.format, RenderFormatChoice::Png);
    assert_eq!(file.pdf_to_images.options.dpi, 150);
    assert_eq!(file.pdf_to_images.options.dpi, DEFAULT_RENDER_DPI);
    assert_eq!(file.pdf_to_images.output_dir, None);
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
fn a_dpi_that_is_not_a_choice_resets_only_the_dpi() {
    for dpi in [json!(100), json!(0), json!(-150), json!(150.5), json!(1e12)] {
        assert_only_resets(
            |json| json["pdfToImages"]["dpi"] = dpi.clone(),
            |file| file.pdf_to_images.options.dpi = DEFAULT_RENDER_DPI,
        );
    }
    for choice in DPI_CHOICES {
        assert_only_resets(
            |json| json["pdfToImages"]["dpi"] = json!(choice),
            |file| file.pdf_to_images.options.dpi = choice,
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
        |json| json["imagesToPdf"]["pageSize"] = json!("letter"),
        |file| file.images_to_pdf.options.page_size = PageSizeChoice::Fit,
    );
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
        |json| json["pdfToImages"]["format"] = json!({"png": true}),
        |file| file.pdf_to_images.options.format = RenderFormatChoice::Png,
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
        |json| remove(json, "imagesToPdf", "outputDir"),
        |file| file.images_to_pdf.output_dir = None,
    );
    assert_only_resets(
        |json| remove(json, "pdfToImages", "format"),
        |file| file.pdf_to_images.options.format = RenderFormatChoice::Png,
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
            json["pdfToImages"]["dpi"] = json!(100);
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

    state.settings.save(&input()).expect("saving");

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
fn save_settings_rejects_a_dpi_that_is_not_a_choice_and_changes_nothing() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);
    state.settings.save(&input()).expect("saving");
    let before = fs::read(config_dir.join(SETTINGS_FILE_NAME)).expect("reading the file");
    let held = state.settings.file();

    for dpi in [0, 100, 151, 600, u32::MAX] {
        let mut bad = input();
        bad.language = None;
        bad.images_to_pdf.output = OutputMode::Merge;
        bad.pdf_to_images.dpi = dpi;
        assert_eq!(
            state.settings.save(&bad),
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
fn save_settings_accepts_every_dpi_choice() {
    let temp = TempDir::new().expect("temp dir");
    let state = state();
    restore_settings(&state, temp.path());
    for dpi in DPI_CHOICES {
        let mut valid = input();
        valid.pdf_to_images.dpi = dpi;
        state.settings.save(&valid).expect("saving");
        assert_eq!(load_settings(temp.path()).pdf_to_images.options.dpi, dpi);
    }
}

#[test]
fn a_write_leaves_no_temporary_file() {
    let temp = TempDir::new().expect("temp dir");
    let config_dir = temp.path().join("config");
    let state = state();
    restore_settings(&state, &config_dir);

    state.settings.save(&input()).expect("saving");
    state.settings.save(&input()).expect("saving again");
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
                "outputDir": { "dirLabel": "images-out" },
            },
            "pdfToImages": {
                "format": "jpeg",
                "dpi": 300,
                "outputDir": { "dirLabel": "pdfs-out" },
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
            "imagesToPdf": { "output": "merge", "pageSize": "fit", "outputDir": null },
            "pdfToImages": { "format": "png", "dpi": 150, "outputDir": null },
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
    state.settings.save(&input()).expect("saving");

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
    let saved = state.settings.save(&input());
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
    state.settings.save(&input()).expect("saving once writable");
    assert_eq!(
        load_settings(&config_dir).pdf_to_images.output_dir,
        Some(picked)
    );
}

#[cfg(unix)]
#[test]
fn a_failed_save_settings_leaves_the_held_settings_as_they_were() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let config_dir = new_dir(temp.path(), "config");
    let state = state();
    restore_settings(&state, &config_dir);
    let held = state.settings.file();
    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500))
        .expect("making the settings folder read-only");

    let saved = state.settings.save(&input());

    fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700))
        .expect("making the settings folder writable again");
    assert!(saved.is_err());
    assert_eq!(state.settings.file(), held);
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
    state
        .settings
        .save(&input())
        .expect("the file stays writable");
}

#[test]
fn the_label_is_the_name_of_the_folder() {
    assert_eq!(dir_label(Path::new("/home/user/Documents/out")), "out");
    assert_eq!(dir_label(Path::new("/home/user/Documents/out/")), "out");
    assert_eq!(dir_label(Path::new("/")), "/");
}
