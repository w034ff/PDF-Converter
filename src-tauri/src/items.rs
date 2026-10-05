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
use pdfconv_worker::protocol::{PageDimensions, Request, Response};
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
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
    /// Why the file could not be read, if it could not.
    pub error: Option<IpcError>,
}

#[derive(Debug, Default)]
struct Table {
    entries: HashMap<u64, Entry>,
    by_canonical: HashMap<PathBuf, u64>,
}

/// The table that maps IDs to files (design §1, §6.1). An ID is a number that
/// only grows, so the ID of a removed item is never given to another file.
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
        let id = self.last_id.fetch_add(1, Ordering::Relaxed) + 1;
        table.by_canonical.insert(entry.canonical.clone(), id);
        table.entries.insert(id, entry);
        Some(id)
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
}

/// The image formats of design §4.1, as the frontend names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormatName {
    Png,
    Jpeg,
    Webp,
    Bmp,
}

impl From<ImageFormat> for ImageFormatName {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::WebP => Self::Webp,
            ImageFormat::Bmp => Self::Bmp,
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
    fn merged(self, other: Self) -> Self {
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
            error: loaded.as_ref().err().cloned(),
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

fn file_name(path: &Path) -> String {
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
             }| match loaded {
                Ok(info) => image_item(id, name, bytes, &info),
                Err(error) => ImageItem {
                    id,
                    name,
                    width: 0,
                    height: 0,
                    format: None,
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
        bytes,
        error: None,
    }
}

/// What opening a PDF in a worker told us.
struct PdfInfo {
    page_count: u32,
    first_page_size_pt: Option<PageSizePt>,
}

/// Adds the PDF files in `paths` to the list of PDFs. Each file is opened in a
/// worker (design §5.1), never in this process.
pub fn add_pdfs(state: &AppState, paths: &[PathBuf]) -> AddResult<PdfItem> {
    let (added, skipped) = add_files(state, ItemKind::Pdf, paths, load_pdf, |info| {
        info.page_count
    });
    let added = added
        .into_iter()
        .map(
            |Added {
                 id,
                 name,
                 bytes,
                 loaded,
             }| match loaded {
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
                    error: Some(error),
                },
            },
        )
        .collect();
    AddResult { added, skipped }
}

fn load_pdf(state: &AppState, path: &Path) -> Result<PdfInfo, IpcError> {
    let mut worker = state.pool.acquire()?;
    let (page_count, pages) = open_pdf(&mut worker, path)?;
    close_pdf(&mut worker);
    Ok(PdfInfo {
        page_count,
        first_page_size_pt: pages.first().map(|page| PageSizePt {
            width_pt: page.width_pt,
            height_pt: page.height_pt,
        }),
    })
}

fn open_pdf(
    worker: &mut PooledWorker,
    path: &Path,
) -> Result<(u32, Vec<PageDimensions>), IpcError> {
    match worker.send(&Request::Open {
        path: path.to_path_buf(),
    })? {
        (Response::Open { page_count, pages }, _) => Ok((page_count, pages)),
        _ => Err(unexpected_response()),
    }
}

/// Lets the worker release the file, which on Windows would otherwise stay
/// locked until the worker opens another PDF. A worker that cannot answer is
/// discarded by the pool, so the result is not needed.
fn close_pdf(worker: &mut PooledWorker) {
    let _ = worker.send(&Request::Close);
}

fn unexpected_response() -> IpcError {
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

fn count(len: usize) -> u32 {
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

/// Makes the PNG thumbnail of an item (design §6.1).
///
/// `page` is used for a PDF only; it defaults to 1.
///
/// # Errors
///
/// Returns `UnknownHandle` for an ID that is not in the table, the item's own
/// error if it could not be read, and `InvalidParams` when `page` is not a
/// page of the PDF.
pub fn make_thumbnail(state: &AppState, id: u64, page: Option<u32>) -> Result<Vec<u8>, IpcError> {
    let entry = state
        .items
        .get(id)
        .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
    if let Some(error) = entry.error {
        return Err(error);
    }
    match entry.kind {
        ItemKind::Image => {
            let bytes = std::fs::read(&entry.path)
                .map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))?;
            Ok(thumbnail_png(&bytes)?)
        }
        ItemKind::Pdf => {
            let page = page.unwrap_or(1);
            if page == 0 || page > entry.page_count {
                return Err(IpcError::from_code(ErrorCode::InvalidParams));
            }
            let mut worker = state.pool.acquire()?;
            open_pdf(&mut worker, &entry.path)?;
            let thumbnail = worker.send(&Request::Thumbnail {
                page,
                max_side: THUMBNAIL_SIDE,
            });
            close_pdf(&mut worker);
            match thumbnail? {
                (Response::Thumbnail, png) => Ok(png),
                _ => Err(unexpected_response()),
            }
        }
    }
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
            error: None,
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
