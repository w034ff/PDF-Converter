//! Settings kept in `settings.json` and the output folders (design §6.5, §6.7).
//!
//! Loading, validating and writing take the settings folder as an argument so
//! they can run without a Tauri app.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use pdfconv_worker::{DEFAULT_RENDER_DPI, is_render_dpi_in_range};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::jobs::{PageOrientationChoice, PageSizeChoice, RenderColorChoice, RenderFormatChoice};

/// The `schemaVersion` this build reads and writes (design §6.7).
pub const SCHEMA_VERSION: u32 = 1;

/// Name of the settings file inside the settings folder.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

const DEFAULT_OUTPUT: OutputMode = OutputMode::Merge;
const DEFAULT_PAGE_SIZE: PageSizeChoice = PageSizeChoice::Fit;
const DEFAULT_ORIENTATION: PageOrientationChoice = PageOrientationChoice::Auto;
const DEFAULT_PDF_A: bool = false;
const DEFAULT_FORMAT: RenderFormatChoice = RenderFormatChoice::Png;
const DEFAULT_RENDER_COLOR_MODE: RenderColorChoice = RenderColorChoice::Color;

/// The UI language the user chose (design §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Ja,
    En,
}

/// Whether images become one PDF or one PDF each (design §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Merge,
    Each,
}

/// The conversion an output folder belongs to (design §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OutputKind {
    ImagesToPdf,
    PdfToImages,
}

/// What the frontend may know of an output folder: its name, never its path
/// (design §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputDirLabel {
    pub dir_label: String,
}

/// The "images to PDF" options the user can change (design §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImagesToPdfOptions {
    pub output: OutputMode,
    pub page_size: PageSizeChoice,
    pub orientation: PageOrientationChoice,
    pub pdf_a: bool,
}

/// The "PDF to images" options the user can change (design §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PdfToImagesOptions {
    pub format: RenderFormatChoice,
    pub color_mode: RenderColorChoice,
    pub dpi: u32,
}

/// Argument of `save_settings`: [`Settings`] without the folders, which only
/// `pick_output_dir` changes (design §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub language: Option<Language>,
    pub images_to_pdf: ImagesToPdfOptions,
    pub pdf_to_images: PdfToImagesOptions,
}

impl SettingsInput {
    /// Checks the values the type cannot rule out.
    ///
    /// # Errors
    ///
    /// `InvalidParams` if `dpi` is not within [`pdfconv_worker::RENDER_DPI_MIN`]..=[`pdfconv_worker::RENDER_DPI_MAX`].
    pub fn validate(&self) -> Result<(), IpcError> {
        if is_render_dpi_in_range(self.pdf_to_images.dpi) {
            Ok(())
        } else {
            Err(IpcError::from_code(ErrorCode::InvalidParams))
        }
    }
}

/// The "images to PDF" part of [`Settings`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImagesToPdfSettings {
    pub output: OutputMode,
    pub page_size: PageSizeChoice,
    pub orientation: PageOrientationChoice,
    pub pdf_a: bool,
    pub output_dir: Option<OutputDirLabel>,
}

/// The "PDF to images" part of [`Settings`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PdfToImagesSettings {
    pub format: RenderFormatChoice,
    pub color_mode: RenderColorChoice,
    pub dpi: u32,
    pub output_dir: Option<OutputDirLabel>,
}

/// Answer of `get_settings` (design §6.7). It holds no path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: Option<Language>,
    pub images_to_pdf: ImagesToPdfSettings,
    pub pdf_to_images: PdfToImagesSettings,
}

/// The "images to PDF" part of the settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagesToPdfFile {
    #[serde(flatten)]
    pub options: ImagesToPdfOptions,
    #[serde(serialize_with = "serialize_dir")]
    pub output_dir: Option<PathBuf>,
}

/// The "PDF to images" part of the settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfToImagesFile {
    #[serde(flatten)]
    pub options: PdfToImagesOptions,
    #[serde(serialize_with = "serialize_dir")]
    pub output_dir: Option<PathBuf>,
}

/// What `settings.json` holds (design §6.7). Unlike [`Settings`] it has the
/// folders' paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsFile {
    pub schema_version: u32,
    pub language: Option<Language>,
    pub images_to_pdf: ImagesToPdfFile,
    pub pdf_to_images: PdfToImagesFile,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            language: None,
            images_to_pdf: ImagesToPdfFile {
                options: ImagesToPdfOptions {
                    output: DEFAULT_OUTPUT,
                    page_size: DEFAULT_PAGE_SIZE,
                    orientation: DEFAULT_ORIENTATION,
                    pdf_a: DEFAULT_PDF_A,
                },
                output_dir: None,
            },
            pdf_to_images: PdfToImagesFile {
                options: PdfToImagesOptions {
                    format: DEFAULT_FORMAT,
                    color_mode: DEFAULT_RENDER_COLOR_MODE,
                    dpi: DEFAULT_RENDER_DPI,
                },
                output_dir: None,
            },
        }
    }
}

/// Writes a folder as a string. A path that is not valid Unicode cannot be
/// written as one, so it becomes `null`: the folder is then used in this
/// session but not remembered, and the file stays writable.
fn serialize_dir<S: Serializer>(dir: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error> {
    dir.as_deref().and_then(Path::to_str).serialize(serializer)
}

/// Reads `key` of `object` as a `T`; `None` if it is missing or not a `T`.
fn field<T: DeserializeOwned>(object: Option<&Map<String, Value>>, key: &str) -> Option<T> {
    T::deserialize(object?.get(key)?).ok()
}

/// Reads `settings.json` text. Every item that is missing, of the wrong type
/// or unknown goes back to its default and the others stay (design §6.7).
/// Text that is not JSON, or whose `schemaVersion` is not [`SCHEMA_VERSION`],
/// gives the defaults for everything.
///
/// Folders are returned as written; [`load_settings`] checks they exist.
pub fn parse_settings_json(text: &str) -> SettingsFile {
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
        return SettingsFile::default();
    };
    if root.get("schemaVersion").and_then(Value::as_u64) != Some(u64::from(SCHEMA_VERSION)) {
        return SettingsFile::default();
    }
    let images = root.get("imagesToPdf").and_then(Value::as_object);
    let pdfs = root.get("pdfToImages").and_then(Value::as_object);
    SettingsFile {
        schema_version: SCHEMA_VERSION,
        language: field(Some(&root), "language"),
        images_to_pdf: ImagesToPdfFile {
            options: ImagesToPdfOptions {
                output: field(images, "output").unwrap_or(DEFAULT_OUTPUT),
                page_size: field(images, "pageSize").unwrap_or(DEFAULT_PAGE_SIZE),
                orientation: field(images, "orientation").unwrap_or(DEFAULT_ORIENTATION),
                pdf_a: field(images, "pdfA").unwrap_or(DEFAULT_PDF_A),
            },
            output_dir: field(images, "outputDir"),
        },
        pdf_to_images: PdfToImagesFile {
            options: PdfToImagesOptions {
                format: field(pdfs, "format").unwrap_or(DEFAULT_FORMAT),
                color_mode: field(pdfs, "colorMode").unwrap_or(DEFAULT_RENDER_COLOR_MODE),
                dpi: field(pdfs, "dpi")
                    .filter(|dpi| is_render_dpi_in_range(*dpi))
                    .unwrap_or(DEFAULT_RENDER_DPI),
            },
            output_dir: field(pdfs, "outputDir"),
        },
    }
}

/// Reads the settings in `config_dir`. A missing or unreadable file gives the
/// defaults, and a saved folder that is not an absolute path to a folder
/// becomes `None` (design §6.7): a relative one would resolve against
/// wherever the app was started from.
pub fn load_settings(config_dir: &Path) -> SettingsFile {
    let mut file = std::fs::read_to_string(config_dir.join(SETTINGS_FILE_NAME))
        .map(|text| parse_settings_json(&text))
        .unwrap_or_default();
    for dir in [
        &mut file.images_to_pdf.output_dir,
        &mut file.pdf_to_images.output_dir,
    ] {
        if dir
            .as_deref()
            .is_some_and(|dir| !dir.is_absolute() || !dir.is_dir())
        {
            *dir = None;
        }
    }
    file
}

fn write_failed(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::WriteFailed, error.to_string())
}

/// Writes `file` as `settings.json` in `config_dir`, creating the folder if
/// needed.
///
/// The content goes to a temporary file in the same folder first and is then
/// renamed over `settings.json`, so a crash in between leaves the old file,
/// not a half-written one. A failure leaves no temporary file.
///
/// # Errors
///
/// `WriteFailed` if the folder or the file cannot be written.
pub fn save_settings_to_dir(config_dir: &Path, file: &SettingsFile) -> Result<(), IpcError> {
    std::fs::create_dir_all(config_dir).map_err(write_failed)?;
    let json = serde_json::to_vec_pretty(file).map_err(write_failed)?;
    let mut temp = tempfile::NamedTempFile::new_in(config_dir).map_err(write_failed)?;
    temp.write_all(&json).map_err(write_failed)?;
    temp.as_file().sync_all().map_err(write_failed)?;
    temp.persist(config_dir.join(SETTINGS_FILE_NAME))
        .map_err(|e| write_failed(e.error))?;
    Ok(())
}

#[derive(Debug, Default)]
struct Stored {
    /// Where the file is written; `None` before [`restore_settings`] ran, in
    /// which case nothing is written.
    config_dir: Option<PathBuf>,
    file: SettingsFile,
    /// Counts the changes to `file`, so a write can tell whether the file
    /// already has the latest of them.
    changes: u64,
    /// The highest `revision` [`SettingsStore::update`] took from the current
    /// page, so a call that arrives late cannot bring older settings back.
    revision: u64,
}

/// The one copy of the settings in memory, and the writes of it to the file
/// (design §6.7).
///
/// A change only touches memory, under a lock held for no longer than the
/// copy, because `save_settings` runs on the main thread. [`Self::flush`]
/// writes the latest settings under a second lock, so writes neither overlap
/// nor put older content over newer.
#[derive(Debug, Default)]
pub struct SettingsStore {
    inner: Mutex<Stored>,
    /// The value of `changes` that the file has; held while writing.
    written: Mutex<u64>,
}

impl SettingsStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Stored> {
        self.inner.lock().expect("settings lock")
    }

    /// A copy of what is held now.
    pub fn file(&self) -> SettingsFile {
        self.lock().file.clone()
    }

    /// Replaces the held settings with `file`, read from `config_dir`, where
    /// later changes are written.
    fn restore(&self, config_dir: PathBuf, file: SettingsFile) {
        *self.lock() = Stored {
            config_dir: Some(config_dir),
            file,
            ..Stored::default()
        };
        *self.written.lock().unwrap_or_else(PoisonError::into_inner) = 0;
    }

    /// Takes `input` over in memory, folders excluded, unless `revision` is
    /// not higher than one already taken from this page; such a call was
    /// overtaken by a newer one and is ignored. The file is written by a
    /// later [`Self::flush`].
    ///
    /// # Errors
    ///
    /// `InvalidParams` for a value [`SettingsInput::validate`] rejects; nothing
    /// changes then.
    pub fn update(&self, revision: u64, input: &SettingsInput) -> Result<(), IpcError> {
        input.validate()?;
        let mut stored = self.lock();
        if revision <= stored.revision {
            return Ok(());
        }
        stored.revision = revision;
        stored.file.language = input.language;
        stored.file.images_to_pdf.options = input.images_to_pdf;
        stored.file.pdf_to_images.options = input.pdf_to_images;
        stored.changes += 1;
        Ok(())
    }

    /// Forgets the highest `revision` taken, because a new page counts from
    /// the start again.
    pub fn start_page(&self) {
        self.lock().revision = 0;
    }

    /// Writes the held settings unless the file already has them. A write
    /// under way finishes first, so this also waits for it.
    ///
    /// # Errors
    ///
    /// `WriteFailed` if the file cannot be written. The settings stay in
    /// memory and the next call tries again.
    pub fn flush(&self) -> Result<(), IpcError> {
        // A write that panicked left the file as it was; the count still says
        // what the file has.
        let mut written = self.written.lock().unwrap_or_else(PoisonError::into_inner);
        let (changes, config_dir, file) = {
            let stored = self.lock();
            (
                stored.changes,
                stored.config_dir.clone(),
                stored.file.clone(),
            )
        };
        if changes == *written {
            return Ok(());
        }
        write(config_dir.as_deref(), &file)?;
        *written = changes;
        Ok(())
    }

    /// Puts `dir` in the held settings and writes the file. The folder stays
    /// in memory if the write fails.
    ///
    /// # Errors
    ///
    /// `WriteFailed` if the file cannot be written.
    pub fn record_output_dir(&self, kind: OutputKind, dir: &Path) -> Result<(), IpcError> {
        self.set_output_dir(kind, Some(dir.to_path_buf()))
    }

    /// Forgets the folder of `kind` in the held settings and writes the file.
    /// The folder stays forgotten in memory if the write fails.
    ///
    /// # Errors
    ///
    /// `WriteFailed` if the file cannot be written.
    pub fn clear_output_dir(&self, kind: OutputKind) -> Result<(), IpcError> {
        self.set_output_dir(kind, None)
    }

    fn set_output_dir(&self, kind: OutputKind, dir: Option<PathBuf>) -> Result<(), IpcError> {
        {
            let mut stored = self.lock();
            let slot = match kind {
                OutputKind::ImagesToPdf => &mut stored.file.images_to_pdf.output_dir,
                OutputKind::PdfToImages => &mut stored.file.pdf_to_images.output_dir,
            };
            *slot = dir;
            stored.changes += 1;
        }
        self.flush()
    }
}

fn write(config_dir: Option<&Path>, file: &SettingsFile) -> Result<(), IpcError> {
    config_dir.map_or(Ok(()), |dir| save_settings_to_dir(dir, file))
}

/// Loads the settings in `config_dir` into `state`: the held settings, and the
/// output folders that still exist. Called once at startup, before the state
/// is shared (design §6.7).
pub fn restore_settings(state: &AppState, config_dir: &Path) {
    let file = load_settings(config_dir);
    *output_dir_slot(state, OutputKind::ImagesToPdf)
        .lock()
        .expect("output_dir lock") = file.images_to_pdf.output_dir.clone();
    *output_dir_slot(state, OutputKind::PdfToImages)
        .lock()
        .expect("output_dir lock") = file.pdf_to_images.output_dir.clone();
    state.settings.restore(config_dir.to_path_buf(), file);
}

fn output_dir_slot(state: &AppState, kind: OutputKind) -> &Arc<Mutex<Option<PathBuf>>> {
    match kind {
        OutputKind::ImagesToPdf => &state.images_output_dir,
        OutputKind::PdfToImages => &state.pdfs_output_dir,
    }
}

/// The name of the last component of `dir`, or the whole path for one that has
/// none (a drive or the root).
pub fn dir_label(dir: &Path) -> String {
    dir.file_name().map_or_else(
        || dir.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn label_of(slot: &Mutex<Option<PathBuf>>) -> Option<OutputDirLabel> {
    slot.lock()
        .expect("output_dir lock")
        .as_deref()
        .map(|dir| OutputDirLabel {
            dir_label: dir_label(dir),
        })
}

/// The settings as `get_settings` returns them. The folders are the ones
/// conversions will use, taken from the state.
pub fn get_settings_internal(state: &AppState) -> Settings {
    let file = state.settings.file();
    Settings {
        language: file.language,
        images_to_pdf: ImagesToPdfSettings {
            output: file.images_to_pdf.options.output,
            page_size: file.images_to_pdf.options.page_size,
            orientation: file.images_to_pdf.options.orientation,
            pdf_a: file.images_to_pdf.options.pdf_a,
            output_dir: label_of(&state.images_output_dir),
        },
        pdf_to_images: PdfToImagesSettings {
            format: file.pdf_to_images.options.format,
            color_mode: file.pdf_to_images.options.color_mode,
            dpi: file.pdf_to_images.options.dpi,
            output_dir: label_of(&state.pdfs_output_dir),
        },
    }
}

/// Makes `dir`, which the user picked, the output folder of `kind` and writes
/// the settings.
///
/// Returns the label to show, and whether the settings file could be written.
/// The folder is in the state either way, so a settings file that cannot be
/// written does not stop conversions from starting (design §6.7).
pub fn apply_picked_dir(
    state: &AppState,
    kind: OutputKind,
    dir: PathBuf,
) -> (OutputDirLabel, Result<(), IpcError>) {
    let label = OutputDirLabel {
        dir_label: dir_label(&dir),
    };
    let persisted = state.settings.record_output_dir(kind, &dir);
    *output_dir_slot(state, kind)
        .lock()
        .expect("output_dir lock") = Some(dir);
    (label, persisted)
}

/// Puts the output folder of `kind` back to "not chosen", in the state and in
/// the settings file (design §6.5).
///
/// The folder is forgotten in the state even if the file cannot be written.
///
/// # Errors
///
/// `WriteFailed` if the settings file cannot be written.
pub fn clear_output_dir(state: &AppState, kind: OutputKind) -> Result<(), IpcError> {
    let persisted = state.settings.clear_output_dir(kind);
    *output_dir_slot(state, kind)
        .lock()
        .expect("output_dir lock") = None;
    persisted
}
