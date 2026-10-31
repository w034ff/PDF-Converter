//! The lists of images and PDFs: the table of IDs, adding files and folders,
//! sorting what is dropped, and thumbnails (design §6.1, §7.1).
//!
//! Paths stay in this table; the frontend only sees IDs (design §1).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use pdfconv_core::{
    IMAGE_EXTENSIONS, ImageFormat, ImageInfo, THUMBNAIL_SIDE, orientation_swaps_dimensions, probe,
    thumbnail_png,
};
use pdfconv_worker::PAGE_VIEW_MAX_SIDE;
use pdfconv_worker::protocol::{PageDimensions, Password, PasswordError, Request, Response};
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::crop;
use crate::decode_budget::DECODE_MEMORY_FACTOR_THUMBNAIL;
use crate::error::{ErrorCode, IpcError};
use crate::organize;
use crate::worker_pool::PooledWorker;

/// The extension of a PDF, without the dot (design §6.1).
pub const PDF_EXTENSION: &str = "pdf";

/// The first character of the name of a hidden file (design §6.1).
const HIDDEN_PREFIX: char = '.';

/// Which list an item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Image,
    Pdf,
}

/// The kind of file that `path` names by its extension, ignoring case.
pub fn kind_of_extension(path: &Path) -> Option<ItemKind> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    if extension == PDF_EXTENSION {
        Some(ItemKind::Pdf)
    } else if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        Some(ItemKind::Image)
    } else {
        None
    }
}

/// What the table keeps for one item.
#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: ItemKind,
    /// The path as it was chosen; files are read through it.
    pub path: PathBuf,
    /// The normalized path, which decides whether two items are the same file.
    pub canonical: PathBuf,
    /// Pages of a PDF; 0 for an image and for a PDF that failed to open.
    pub page_count: u32,
    /// Bytes of the pixels that reading an image produces (`ImageInfo::decoded_bytes`); 0 for a
    /// PDF and for an image that failed to probe. The decode budget is borrowed from this
    /// (design §5.3).
    pub decoded_bytes: u64,
    /// Why the file could not be read, if it could not.
    pub error: Option<IpcError>,
    /// Password entered by the user, if any (design §6.8).
    pub password: Option<Password>,
}

impl Entry {
    /// Builds the [`Request::Open`] for this entry, including its password (design §6.8).
    pub fn open_request(&self) -> Request {
        Request::Open {
            path: self.path.clone(),
            password: self.password.clone(),
        }
    }

    /// Whether this entry is a PDF whose open failed due to a missing password (design §6.8).
    pub fn is_password_protected(&self) -> bool {
        self.kind == ItemKind::Pdf
            && self.error.as_ref().map(|e| e.code) == Some(ErrorCode::PasswordProtected)
    }
}

#[derive(Debug, Default)]
struct Table {
    entries: HashMap<u64, Entry>,
    by_canonical: HashMap<PathBuf, u64>,
}

/// The table that maps IDs to files (design §1, §6.1). An ID is a number that
/// only grows, so the ID of a removed item is never given to another file.
/// The crop PDFs (design §6.10) take their IDs from the same count but are not
/// kept here.
#[derive(Debug, Default)]
pub struct ItemTable {
    table: Mutex<Table>,
    last_id: AtomicU64,
}

impl ItemTable {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Table> {
        self.table
            .lock()
            .expect("the item table is never locked across a panic")
    }

    /// Whether a file with this normalized path is in a list already.
    pub fn contains(&self, canonical: &Path) -> bool {
        self.lock().by_canonical.contains_key(canonical)
    }

    /// Adds `entry` and returns its ID, or `None` if the same file is in a
    /// list already (so two additions at once cannot add a file twice).
    pub fn insert(&self, entry: Entry) -> Option<u64> {
        let mut table = self.lock();
        if table.by_canonical.contains_key(&entry.canonical) {
            return None;
        }
        let id = self.next_id();
        table.by_canonical.insert(entry.canonical.clone(), id);
        table.entries.insert(id, entry);
        Some(id)
    }

    /// Takes the next ID without putting anything in the table, so that a
    /// crop PDF (design §6.10) gets an ID that no list item has or will have.
    pub fn next_id(&self) -> u64 {
        self.last_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn get(&self, id: u64) -> Option<Entry> {
        self.lock().entries.get(&id).cloned()
    }

    /// Removes the items with these IDs; an ID that is not in the table is ignored.
    pub fn remove(&self, ids: &[u64]) {
        let mut table = self.lock();
        for id in ids {
            if let Some(entry) = table.entries.remove(id) {
                table.by_canonical.remove(&entry.canonical);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Updates an entry's page count, error, and password after an unlock attempt (design §6.8).
    pub fn update_pdf_after_unlock(
        &self,
        id: u64,
        page_count: u32,
        error: Option<IpcError>,
        password: Password,
    ) -> bool {
        let mut table = self.lock();
        if let Some(entry) = table.entries.get_mut(&id) {
            entry.page_count = page_count;
            entry.error = error;
            entry.password = Some(password);
            true
        } else {
            false
        }
    }

    /// Finds other candidate PDF entries that are password protected and have
    /// no password yet, sorted by ID (design §6.8).
    pub fn candidates_for_unlock(&self, target_id: u64) -> Vec<(u64, PathBuf)> {
        let table = self.lock();
        let mut candidates: Vec<(u64, PathBuf)> = table
            .entries
            .iter()
            .filter(|&(id, entry)| {
                *id != target_id && entry.is_password_protected() && entry.password.is_none()
            })
            .map(|(id, entry)| (*id, entry.path.clone()))
            .collect();
        candidates.sort_by_key(|&(id, _)| id);
        candidates
    }
}

/// The image formats of design §4.1, as the frontend names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormatName {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
}

impl From<ImageFormat> for ImageFormatName {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::WebP => Self::Webp,
            ImageFormat::Bmp => Self::Bmp,
            ImageFormat::Tiff => Self::Tiff,
        }
    }
}

/// The size of a PDF page in points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PageSizePt {
    pub width_pt: f32,
    pub height_pt: f32,
}

/// An image in the list (design §7.1). When `error` is set, the file could not
/// be read: `width` and `height` are 0 and `format` is null.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImageItem {
    #[ts(type = "number")]
    pub id: u64,
    /// The file name, without the folder.
    pub name: String,
    /// Pixels as displayed: the sides are swapped when the EXIF orientation
    /// turns the image by a quarter.
    pub width: u32,
    pub height: u32,
    pub format: Option<ImageFormatName>,
    pub page_count: u32,
    /// Size of the file in bytes.
    #[ts(type = "number")]
    pub bytes: u64,
    pub error: Option<IpcError>,
}

/// A PDF in the list (design §7.1). When `error` is set, the file could not be
/// opened: the page count is 0 and the size of the first page is null.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PdfItem {
    #[ts(type = "number")]
    pub id: u64,
    /// The file name, without the folder.
    pub name: String,
    pub page_count: u32,
    pub first_page_size_pt: Option<PageSizePt>,
    /// Size of the file in bytes.
    #[ts(type = "number")]
    pub bytes: u64,
    pub error: Option<IpcError>,
}

/// What was left out of an addition (design §6.1, §7.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    /// Files whose extension the list does not take.
    pub unsupported: u32,
    /// Subfolders of a folder that was added.
    pub folders: u32,
    /// Files that are in a list already.
    pub duplicates: u32,
}

impl Skipped {
    pub(crate) fn merged(self, other: Self) -> Self {
        Self {
            unsupported: self.unsupported + other.unsupported,
            folders: self.folders + other.folders,
            duplicates: self.duplicates + other.duplicates,
        }
    }
}

/// The answer of `add_images` and `add_pdfs` (design §7.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AddResult<T> {
    pub added: Vec<T>,
    pub skipped: Skipped,
}

/// The payload of the `items-dropped` event (design §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ItemsDropped {
    pub images: Vec<ImageItem>,
    pub pdfs: Vec<PdfItem>,
    pub skipped: Skipped,
    pub error: Option<IpcError>,
}

/// The answer of `unlock_pdf` (design §6.8, §7.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnlockResult {
    pub items: Vec<PdfItem>,
}

/// What a folder holds, sorted into the lists (design §6.1).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FolderScan {
    pub images: Vec<PathBuf>,
    pub pdfs: Vec<PathBuf>,
    /// Files with another extension.
    pub unsupported: u32,
    /// Subfolders.
    pub folders: u32,
}

/// Looks at the entries directly in `dir` (design §6.1): hidden entries and
/// symbolic links are left out without being counted, subfolders and files of
/// another extension are counted, and the files that are left are sorted by
/// name so that the order of a list does not depend on the file system.
///
/// # Errors
///
/// Returns the I/O error when `dir` cannot be read.
pub fn scan_folder(dir: &Path) -> io::Result<FolderScan> {
    let mut scan = FolderScan::default();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if is_hidden(&entry.file_name()) {
            continue;
        }
        // `DirEntry::file_type` does not follow links, so a symbolic link is
        // neither a folder nor a file here and is left out without a count.
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            scan.folders += 1;
        } else if file_type.is_file() {
            match kind_of_extension(&entry.path()) {
                Some(ItemKind::Image) => scan.images.push(entry.path()),
                Some(ItemKind::Pdf) => scan.pdfs.push(entry.path()),
                None => scan.unsupported += 1,
            }
        }
    }
    scan.images.sort_by_key(|path| sort_key(path));
    scan.pdfs.sort_by_key(|path| sort_key(path));
    Ok(scan)
}

fn is_hidden(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with(HIDDEN_PREFIX)
}

fn sort_key(path: &Path) -> (String, PathBuf) {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    (name.to_lowercase(), path.to_path_buf())
}

/// A file that was added to a list.
struct Added<I> {
    id: u64,
    name: String,
    bytes: u64,
    loaded: Result<I, IpcError>,
}

/// Adds the files in `paths` that belong to `kind`; every other file counts as
/// unsupported. A file that cannot be read is added with its error, and a file
/// that is in a list already is not added.
fn add_files<I>(
    state: &AppState,
    kind: ItemKind,
    paths: &[PathBuf],
    load: impl Fn(&AppState, &Path) -> Result<I, IpcError>,
    page_count: impl Fn(&I) -> u32,
    decoded_bytes: impl Fn(&I) -> u64,
) -> (Vec<Added<I>>, Skipped) {
    let mut added = Vec::new();
    let mut skipped = Skipped::default();
    for path in paths {
        if kind_of_extension(path) != Some(kind) {
            skipped.unsupported += 1;
            continue;
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if state.items.contains(&canonical) {
            skipped.duplicates += 1;
            continue;
        }
        let loaded = load(state, path);
        let bytes = std::fs::metadata(path).map_or(0, |metadata| metadata.len());
        let entry = Entry {
            kind,
            path: path.clone(),
            canonical,
            page_count: loaded.as_ref().map_or(0, &page_count),
            decoded_bytes: loaded.as_ref().map_or(0, &decoded_bytes),
            error: loaded.as_ref().err().cloned(),
            password: None,
        };
        match state.items.insert(entry) {
            Some(id) => added.push(Added {
                id,
                name: file_name(path),
                bytes,
                loaded,
            }),
            None => skipped.duplicates += 1,
        }
    }
    (added, skipped)
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Adds the image files in `paths` to the list of images. Each file is probed
/// (design §4.1), which reads only its header.
pub fn add_images(state: &AppState, paths: &[PathBuf]) -> AddResult<ImageItem> {
    let (added, skipped) = add_files(
        state,
        ItemKind::Image,
        paths,
        |_, path| probe(path).map_err(IpcError::from),
        |info| info.page_count,
        |info| info.decoded_bytes,
    );
    let added = added
        .into_iter()
        .map(
            |Added {
                 id,
                 name,
                 bytes,
                 loaded,
             }| match loaded {
                Ok(info) => image_item(id, name, bytes, &info),
                Err(error) => ImageItem {
                    id,
                    name,
                    width: 0,
                    height: 0,
                    format: None,
                    page_count: 0,
                    bytes,
                    error: Some(error),
                },
            },
        )
        .collect();
    AddResult { added, skipped }
}

fn image_item(id: u64, name: String, bytes: u64, info: &ImageInfo) -> ImageItem {
    let (width, height) = if orientation_swaps_dimensions(info.orientation) {
        (info.height, info.width)
    } else {
        (info.width, info.height)
    };
    ImageItem {
        id,
        name,
        width,
        height,
        format: Some(info.format.into()),
        page_count: info.page_count,
        bytes,
        error: None,
    }
}

/// What opening a PDF in a worker told us.
struct PdfInfo {
    page_count: u32,
    first_page_size_pt: Option<PageSizePt>,
}

fn make_pdf_item(id: u64, name: String, bytes: u64, loaded: &Result<PdfInfo, IpcError>) -> PdfItem {
    match loaded {
        Ok(info) => PdfItem {
            id,
            name,
            page_count: info.page_count,
            first_page_size_pt: info.first_page_size_pt,
            bytes,
            error: None,
        },
        Err(error) => PdfItem {
            id,
            name,
            page_count: 0,
            first_page_size_pt: None,
            bytes,
            error: Some(error.clone()),
        },
    }
}

/// Adds the PDF files in `paths` to the list of PDFs. Each file is opened in a
/// worker (design §5.1), never in this process.
pub fn add_pdfs(state: &AppState, paths: &[PathBuf]) -> AddResult<PdfItem> {
    let (added, skipped) = add_files(
        state,
        ItemKind::Pdf,
        paths,
        |s, p| load_pdf(s, p, None),
        |info| info.page_count,
        |_| 0,
    );
    let added = added
        .into_iter()
        .map(
            |Added {
                 id,
                 name,
                 bytes,
                 loaded,
             }| make_pdf_item(id, name, bytes, &loaded),
        )
        .collect();
    AddResult { added, skipped }
}

/// Opens the PDF at `path` in a pooled worker and returns its page count and
/// the size of every page (design §5.1).
pub(crate) fn open_in_pool(
    state: &AppState,
    path: &Path,
    password: Option<&Password>,
) -> Result<(u32, Vec<PageDimensions>), IpcError> {
    let mut worker = state.pool.acquire()?;
    let req = Request::Open {
        path: path.to_path_buf(),
        password: password.cloned(),
    };
    let opened = send_open(&mut worker, &req)?;
    close_pdf(&mut worker);
    Ok(opened)
}

fn load_pdf(
    state: &AppState,
    path: &Path,
    password: Option<&Password>,
) -> Result<PdfInfo, IpcError> {
    let (page_count, pages) = open_in_pool(state, path, password)?;
    Ok(PdfInfo {
        page_count,
        first_page_size_pt: pages.first().map(|page| PageSizePt {
            width_pt: page.width_pt,
            height_pt: page.height_pt,
        }),
    })
}

pub(crate) fn send_open(
    worker: &mut PooledWorker,
    req: &Request,
) -> Result<(u32, Vec<PageDimensions>), IpcError> {
    match worker.send(req)? {
        (Response::Open { page_count, pages }, _) => Ok((page_count, pages)),
        _ => Err(unexpected_response()),
    }
}

fn load_pdf_item(
    state: &AppState,
    id: u64,
    path: &Path,
    password: Option<&Password>,
) -> (PdfItem, Result<PdfInfo, IpcError>) {
    let loaded = load_pdf(state, path, password);
    let name = file_name(path);
    let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
    let item = make_pdf_item(id, name, bytes, &loaded);
    (item, loaded)
}

/// Lets the worker release the file, which on Windows would otherwise stay
/// locked until the worker opens another PDF. A worker that cannot answer is
/// discarded by the pool, so the result is not needed.
pub(crate) fn close_pdf(worker: &mut PooledWorker) {
    let _ = worker.send(&Request::Close);
}

pub(crate) fn unexpected_response() -> IpcError {
    IpcError::new(
        ErrorCode::WorkerCrashed,
        "the worker answered with an unexpected message",
    )
}

/// Adds the image files directly in the folder `dir` (design §6.1). Subfolders
/// and PDFs are counted as skipped.
///
/// # Errors
///
/// Returns `ReadFailed` when the folder cannot be read.
pub fn add_images_from_folder(
    state: &AppState,
    dir: &Path,
) -> Result<AddResult<ImageItem>, IpcError> {
    let scan = scan_folder(dir).map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))?;
    let mut result = add_images(state, &scan.images);
    result.skipped = result.skipped.merged(Skipped {
        unsupported: scan.unsupported + count(scan.pdfs.len()),
        folders: scan.folders,
        duplicates: 0,
    });
    Ok(result)
}

/// Adds the PDF files directly in the folder `dir` (design §6.1). Subfolders
/// and images are counted as skipped.
///
/// # Errors
///
/// Returns `ReadFailed` when the folder cannot be read.
pub fn add_pdfs_from_folder(state: &AppState, dir: &Path) -> Result<AddResult<PdfItem>, IpcError> {
    let scan = scan_folder(dir).map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))?;
    let mut result = add_pdfs(state, &scan.pdfs);
    result.skipped = result.skipped.merged(Skipped {
        unsupported: scan.unsupported + count(scan.images.len()),
        folders: scan.folders,
        duplicates: 0,
    });
    Ok(result)
}

pub(crate) fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// What a drop holds, sorted by kind (design §6.1).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DropSorting {
    pub images: Vec<PathBuf>,
    pub pdfs: Vec<PathBuf>,
    pub unsupported: u32,
    pub folders: u32,
}

/// Sorts dropped paths by extension. A dropped folder is looked into as in
/// [`scan_folder`]; a file that was dropped itself is kept even if its name is
/// hidden.
pub fn sort_dropped(paths: &[PathBuf]) -> DropSorting {
    let mut sorting = DropSorting::default();
    for path in paths {
        if path.is_dir() {
            match scan_folder(path) {
                Ok(scan) => {
                    sorting.images.extend(scan.images);
                    sorting.pdfs.extend(scan.pdfs);
                    sorting.unsupported += scan.unsupported;
                    sorting.folders += scan.folders;
                }
                // Nothing of an unreadable folder is added, so it is counted
                // with the files that were left out.
                Err(_) => sorting.unsupported += 1,
            }
            continue;
        }
        match kind_of_extension(path) {
            Some(ItemKind::Image) => sorting.images.push(path.clone()),
            Some(ItemKind::Pdf) => sorting.pdfs.push(path.clone()),
            None => sorting.unsupported += 1,
        }
    }
    sorting
}

/// Adds what was dropped to both lists (design §6.1, §7.2).
pub fn add_dropped(state: &AppState, paths: &[PathBuf]) -> ItemsDropped {
    if state.is_running.load(Ordering::SeqCst) {
        return ItemsDropped {
            images: Vec::new(),
            pdfs: Vec::new(),
            skipped: Skipped::default(),
            error: Some(IpcError::from_code(ErrorCode::ConversionRunning)),
        };
    }
    let sorting = sort_dropped(paths);
    let images = add_images(state, &sorting.images);
    let pdfs = add_pdfs(state, &sorting.pdfs);
    let left_out = Skipped {
        unsupported: sorting.unsupported,
        folders: sorting.folders,
        duplicates: 0,
    };
    ItemsDropped {
        images: images.added,
        pdfs: pdfs.added,
        skipped: images.skipped.merged(pdfs.skipped).merged(left_out),
        error: None,
    }
}

/// Removes items from the table, rejecting with `ConversionRunning` during active conversion (design §6.1, §7.1).
pub fn remove_items(state: &AppState, ids: &[u64]) -> Result<(), IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    state.items.remove(ids);
    Ok(())
}

/// Makes the PNG thumbnail of an item (design §6.1, §7.1).
///
/// `id` is an item of a list, an organize PDF (design organize §4) or the open crop PDF
/// (design §6.10). `page` is
/// used for a PDF only; it defaults to 1. `max_side` is the length of the
/// longer side of a PDF page, up to `PAGE_VIEW_MAX_SIDE`; it defaults to
/// `THUMBNAIL_SIDE` and is not accepted for an image, whose thumbnail has a
/// fixed size.
///
/// # Errors
///
/// Returns `UnknownHandle` for an ID that is neither in the table, an organize PDF nor the
/// open crop PDF, the item's own error if it could not be read, and `InvalidParams`
/// when `page` is not a page of the PDF, `max_side` is 0 or above
/// `PAGE_VIEW_MAX_SIDE`, or `max_side` is given for an image.
pub fn make_thumbnail(
    state: &AppState,
    id: u64,
    page: Option<u32>,
    max_side: Option<u32>,
) -> Result<Vec<u8>, IpcError> {
    let Some(entry) = state.items.get(id) else {
        return match organize::thumbnail(state, id, page, max_side) {
            Some(thumbnail) => thumbnail,
            None => crop::crop_thumbnail(state, id, page, max_side),
        };
    };
    if let Some(error) = entry.error {
        return Err(error);
    }
    match entry.kind {
        ItemKind::Image => {
            if max_side.is_some() {
                return Err(IpcError::from_code(ErrorCode::InvalidParams));
            }
            // Borrowed before the file is read, because the file's own bytes are part of the
            // peak, and held until the PNG is made (design §5.3).
            let _permit = state
                .decode_budget
                .acquire(
                    entry
                        .decoded_bytes
                        .saturating_mul(DECODE_MEMORY_FACTOR_THUMBNAIL),
                    &|| false,
                )
                .expect("a wait that is never cancelled ends with a permit");
            let bytes = std::fs::read(&entry.path)
                .map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))?;
            Ok(thumbnail_png(&bytes)?)
        }
        ItemKind::Pdf => {
            let page = checked_page(page, entry.page_count)?;
            let max_side = checked_max_side(max_side)?;
            pdf_thumbnail(state, id, &entry.open_request(), page, max_side)
        }
    }
}

/// `page`, which defaults to 1, if it is a page of a PDF with `page_count` pages.
pub(crate) fn checked_page(page: Option<u32>, page_count: u32) -> Result<u32, IpcError> {
    let page = page.unwrap_or(1);
    if page == 0 || page > page_count {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }
    Ok(page)
}

/// `max_side`, which defaults to `THUMBNAIL_SIDE`, if the worker will take it.
/// The worker checks it too; checking here keeps a bad value from taking a
/// worker for a PDF that has to be opened first.
pub(crate) fn checked_max_side(max_side: Option<u32>) -> Result<u32, IpcError> {
    let max_side = max_side.unwrap_or(THUMBNAIL_SIDE);
    if max_side == 0 || max_side > PAGE_VIEW_MAX_SIDE {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }
    Ok(max_side)
}

/// Sends `request` for the PDF `pdf_id`, which `open` opens, in a pooled
/// worker that has it open already if there is one, and leaves it open for the
/// next request: opening a PDF of many pages takes far longer than drawing a
/// thumbnail of it (design §5.2). `pdf_id` must never name another file.
pub(crate) fn request_on_pdf(
    state: &AppState,
    pdf_id: u64,
    open: &Request,
    request: &Request,
) -> Result<(Response, Vec<u8>), IpcError> {
    let mut worker = state.pool.acquire_for_pdf(pdf_id)?;
    if !worker.use_held_pdf(pdf_id) {
        worker.set_held_pdf(None);
        send_open(&mut worker, open)?;
        worker.set_held_pdf(Some(pdf_id));
    }
    Ok(worker.send(request)?)
}

/// Renders `page` of the PDF `pdf_id`, which `open` opens, as a PNG, in a pooled worker.
pub(crate) fn pdf_thumbnail(
    state: &AppState,
    pdf_id: u64,
    open: &Request,
    page: u32,
    max_side: u32,
) -> Result<Vec<u8>, IpcError> {
    match request_on_pdf(state, pdf_id, open, &Request::Thumbnail { page, max_side })? {
        (Response::Thumbnail, png) => Ok(png),
        _ => Err(unexpected_response()),
    }
}

fn try_unlock(
    state: &AppState,
    id: u64,
    path: &Path,
    password: &Password,
) -> Result<PdfItem, IpcError> {
    let (item, loaded) = load_pdf_item(state, id, path, Some(password));
    if let Err(ref err) = loaded
        && matches!(
            err.code,
            ErrorCode::PasswordProtected | ErrorCode::WorkerCrashed | ErrorCode::WorkerTimeout
        )
    {
        return Err(err.clone());
    }
    let page_count = loaded.as_ref().map_or(0, |info| info.page_count);
    let error = loaded.as_ref().err().cloned();
    state
        .items
        .update_pdf_after_unlock(id, page_count, error, password.clone());
    Ok(item)
}

/// Unlocks a password-protected PDF and propagates the password to other
/// password-protected entries in the table (design §6.8, §7.1).
pub fn unlock_pdf(state: &AppState, id: u64, password: &str) -> Result<UnlockResult, IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    let target = state
        .items
        .get(id)
        .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;

    if !target.is_password_protected() {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }

    let password = match Password::new(password) {
        Ok(pw) => pw,
        Err(PasswordError::Empty) => return Err(IpcError::from_code(ErrorCode::InvalidParams)),
        Err(PasswordError::ContainsNul) => {
            return Err(IpcError::from_code(ErrorCode::PasswordProtected));
        }
    };

    let target_item = try_unlock(state, id, &target.path, &password)?;
    let mut unlocked_items = vec![target_item];

    for (cand_id, cand_path) in state.items.candidates_for_unlock(id) {
        if let Ok(cand_item) = try_unlock(state, cand_id, &cand_path, &password) {
            unlocked_items.push(cand_item);
        }
    }

    Ok(UnlockResult {
        items: unlocked_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(canonical: &str) -> Entry {
        Entry {
            kind: ItemKind::Image,
            path: PathBuf::from(canonical),
            canonical: PathBuf::from(canonical),
            page_count: 0,
            decoded_bytes: 0,
            error: None,
            password: None,
        }
    }

    #[test]
    fn ids_only_grow() {
        let table = ItemTable::new();
        let first = table.insert(entry("/a.png")).expect("a new file");
        let second = table.insert(entry("/b.png")).expect("a new file");
        table.remove(&[second]);
        let third = table.insert(entry("/c.png")).expect("a new file");
        assert!(first < second && second < third);
    }

    #[test]
    fn the_same_file_is_added_once_until_it_is_removed() {
        let table = ItemTable::new();
        let id = table.insert(entry("/a.png")).expect("a new file");
        assert!(table.contains(Path::new("/a.png")));
        assert_eq!(table.insert(entry("/a.png")), None);
        table.remove(&[id]);
        assert!(!table.contains(Path::new("/a.png")));
        assert!(table.insert(entry("/a.png")).is_some());
    }

    #[test]
    fn removing_ignores_ids_that_are_not_in_the_table() {
        let table = ItemTable::new();
        let id = table.insert(entry("/a.png")).expect("a new file");
        table.remove(&[id + 100, 0]);
        assert_eq!(table.len(), 1);
        table.remove(&[id, id]);
        assert!(table.is_empty());
        assert!(table.get(id).is_none());
    }

    #[test]
    fn the_extension_decides_the_kind_ignoring_case() {
        assert_eq!(kind_of_extension(Path::new("a.PNG")), Some(ItemKind::Image));
        assert_eq!(
            kind_of_extension(Path::new("dir/a.JpEg")),
            Some(ItemKind::Image)
        );
        assert_eq!(kind_of_extension(Path::new("a.Pdf")), Some(ItemKind::Pdf));
        assert_eq!(kind_of_extension(Path::new("a.txt")), None);
        assert_eq!(kind_of_extension(Path::new("pdf")), None);
        assert_eq!(kind_of_extension(Path::new("a.png.bak")), None);
    }
}
