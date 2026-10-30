//! The PDFs of the organize tab: adding, removing, unlocking, dropping and thumbnails, and the
//! checks and the size estimate that come before an output is made (design organize §2, §4).
//!
//! Organize PDFs are kept apart from the lists of `items.rs`, as the crop PDFs are (design
//! §6.10): the same file may be in a list and here, and the commands of the lists do not act on
//! them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};

use pdfconv_core::PdfConformance;
use pdfconv_worker::MAX_PDF_PAGES;
use pdfconv_worker::protocol::{Password, PasswordError, Request};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::items::{
    ItemKind, PageSizePt, Skipped, checked_max_side, checked_page, count, file_name,
    kind_of_extension, open_in_pool, pdf_thumbnail, scan_folder, sort_dropped,
};
use crate::jobs::output_password;

/// The largest estimated size of one organized output (design organize §2).
///
/// The worker's memory for one output is about 2.8 times the output, and `WORKER_MEMORY_LIMIT`
/// of 2 GiB leaves room for an output of about 600 MB. This is a round number below that.
pub const MAX_ORGANIZE_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;

/// Bytes in the "MB" of the `detail` of `OutputTooLarge`.
const BYTES_PER_MB: u64 = 1024 * 1024;

/// A PDF of the organize tab (design organize §4). When `error` is set, the PDF could not be
/// opened (or still needs its password): the page count is 0 and `pages` is empty.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePdf {
    #[ts(type = "number")]
    pub id: u64,
    /// The file name, without the folder.
    pub name: String,
    pub page_count: u32,
    /// The size of every page, in the orientation the page is shown in.
    pub pages: Vec<PageSizePt>,
    /// The size of the file when it was added. The size estimate of an output is made from it
    /// (design organize §2).
    #[ts(type = "number")]
    pub file_bytes: u64,
    /// Whether opening the PDF needed a password. It stays `true` after the PDF is unlocked.
    pub encrypted: bool,
    pub error: Option<IpcError>,
}

/// The payload of the `organize-dropped` event (design §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeDropped {
    pub added: Vec<OrganizePdf>,
    pub skipped: Skipped,
    pub error: Option<IpcError>,
}

/// The answer of `add_organize_pdfs` (design organize §4).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeAdded {
    pub added: Vec<OrganizePdf>,
    pub skipped: Skipped,
}

/// The answer of `unlock_organize_pdf` (design §6.8, organize §4).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeUnlocked {
    pub items: Vec<OrganizePdf>,
}

/// One page of an output, as the screen sends it (design organize §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePageInput {
    /// The organize PDF the page belongs to.
    #[ts(type = "number")]
    pub id: u64,
    /// The page, from 1, as the screen shows it.
    pub page: u32,
    /// Clockwise degrees to turn the page by: 0, 90, 180 or 270.
    pub rotation: u16,
}

/// What is kept for one organize PDF.
#[derive(Debug, Clone)]
struct OrganizeEntry {
    id: u64,
    path: PathBuf,
    /// The normalized path, which decides whether two entries are the same file.
    canonical: PathBuf,
    name: String,
    page_count: u32,
    pages: Vec<PageSizePt>,
    file_bytes: u64,
    encrypted: bool,
    error: Option<IpcError>,
    password: Option<Password>,
}

impl OrganizeEntry {
    fn to_ipc(&self) -> OrganizePdf {
        OrganizePdf {
            id: self.id,
            name: self.name.clone(),
            page_count: self.page_count,
            pages: self.pages.clone(),
            file_bytes: self.file_bytes,
            encrypted: self.encrypted,
            error: self.error.clone(),
        }
    }

    fn is_password_protected(&self) -> bool {
        self.error.as_ref().map(|error| error.code) == Some(ErrorCode::PasswordProtected)
    }

    fn open_request(&self) -> Request {
        Request::Open {
            path: self.path.clone(),
            password: self.password.clone(),
        }
    }
}

/// What the checks and the requests need of one organize PDF.
#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub(crate) path: PathBuf,
    pub(crate) password: Option<Password>,
    pub(crate) name: String,
    pub(crate) page_count: u32,
    pub(crate) file_bytes: u64,
    pub(crate) error: Option<IpcError>,
}

/// The organize tab's part of [`AppState`]: its PDFs, in the order they were added.
#[derive(Debug, Default)]
pub struct OrganizeState {
    entries: Mutex<Vec<OrganizeEntry>>,
}

impl OrganizeState {
    fn lock(&self) -> MutexGuard<'_, Vec<OrganizeEntry>> {
        self.entries
            .lock()
            .expect("the organize PDFs are never locked across a panic")
    }

    fn contains(&self, canonical: &Path) -> bool {
        self.lock().iter().any(|entry| entry.canonical == canonical)
    }

    /// Adds `entry` unless the same file is here already (so two additions at once cannot add a
    /// file twice). Returns whether it was added.
    fn insert(&self, entry: OrganizeEntry) -> bool {
        let mut entries = self.lock();
        if entries
            .iter()
            .any(|known| known.canonical == entry.canonical)
        {
            return false;
        }
        entries.push(entry);
        true
    }

    /// Removes the PDFs with these IDs; an ID that is not here is ignored.
    fn remove(&self, ids: &[u64]) {
        self.lock().retain(|entry| !ids.contains(&entry.id));
    }

    /// Removes the PDF whose file is `canonical` and returns its ID; `None` if no PDF is that
    /// file.
    pub(crate) fn remove_file(&self, canonical: &Path) -> Option<u64> {
        let mut entries = self.lock();
        let index = entries
            .iter()
            .position(|entry| entry.canonical == canonical)?;
        Some(entries.remove(index).id)
    }

    fn get(&self, id: u64) -> Option<OrganizeEntry> {
        self.lock().iter().find(|entry| entry.id == id).cloned()
    }

    /// Makes the file of the PDF `id` count as `bytes` for the estimate, so that a test can reach
    /// the size limit without a file that large. Returns whether the PDF is here.
    #[cfg(feature = "test-hooks")]
    pub fn set_file_bytes_for_test(&self, id: u64, bytes: u64) -> bool {
        match self.lock().iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.file_bytes = bytes;
                true
            }
            None => false,
        }
    }

    /// The PDFs among `ids` that are here.
    pub(crate) fn sources(&self, ids: &[u64]) -> HashMap<u64, Source> {
        self.lock()
            .iter()
            .filter(|entry| ids.contains(&entry.id))
            .map(|entry| {
                (
                    entry.id,
                    Source {
                        path: entry.path.clone(),
                        password: entry.password.clone(),
                        name: entry.name.clone(),
                        page_count: entry.page_count,
                        file_bytes: entry.file_bytes,
                        error: entry.error.clone(),
                    },
                )
            })
            .collect()
    }

    /// The PDFs that wait for a password and have none yet, other than `target`, by ID.
    fn candidates_for_unlock(&self, target: u64) -> Vec<(u64, PathBuf)> {
        let mut candidates: Vec<(u64, PathBuf)> = self
            .lock()
            .iter()
            .filter(|entry| {
                entry.id != target && entry.is_password_protected() && entry.password.is_none()
            })
            .map(|entry| (entry.id, entry.path.clone()))
            .collect();
        candidates.sort_by_key(|&(id, _)| id);
        candidates
    }

    /// Records what opening `id` with `password` gave. `None` if `id` is not here any more.
    fn unlocked(
        &self,
        id: u64,
        opened: Result<(u32, Vec<PageSizePt>), IpcError>,
        password: Password,
    ) -> Option<OrganizePdf> {
        let mut entries = self.lock();
        let entry = entries.iter_mut().find(|entry| entry.id == id)?;
        match opened {
            Ok((page_count, pages)) => {
                entry.page_count = page_count;
                entry.pages = pages;
                entry.error = None;
            }
            Err(error) => {
                entry.page_count = 0;
                entry.pages = Vec::new();
                entry.error = Some(error);
            }
        }
        entry.password = Some(password);
        Some(entry.to_ipc())
    }
}

fn refuse_while_running(state: &AppState) -> Result<(), IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    Ok(())
}

fn page_sizes(pages: &[pdfconv_worker::protocol::PageDimensions]) -> Vec<PageSizePt> {
    pages
        .iter()
        .map(|page| PageSizePt {
            width_pt: page.width_pt,
            height_pt: page.height_pt,
        })
        .collect()
}

/// Adds the PDF files in `paths` to the organize list, one after another (design organize §4).
/// A file that cannot be read or opened is added with its error, and a file that is here
/// already is counted in `skipped.duplicates`.
fn add_files(state: &AppState, paths: &[PathBuf]) -> OrganizeAdded {
    let mut added = Vec::new();
    let mut skipped = Skipped::default();
    for path in paths {
        if kind_of_extension(path) != Some(ItemKind::Pdf) {
            skipped.unsupported += 1;
            continue;
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if state.organize.contains(&canonical) {
            skipped.duplicates += 1;
            continue;
        }
        let entry = open_entry(state, path, canonical);
        let pdf = entry.to_ipc();
        if state.organize.insert(entry) {
            added.push(pdf);
        } else {
            skipped.duplicates += 1;
        }
    }
    OrganizeAdded { added, skipped }
}

/// Reads the size of the file and opens it in a pooled worker, never in this process.
fn open_entry(state: &AppState, path: &Path, canonical: PathBuf) -> OrganizeEntry {
    let id = state.items.next_id();
    let mut entry = OrganizeEntry {
        id,
        path: path.to_path_buf(),
        canonical,
        name: file_name(path),
        page_count: 0,
        pages: Vec::new(),
        file_bytes: 0,
        encrypted: false,
        error: None,
        password: None,
    };
    match std::fs::metadata(path) {
        Err(_) => entry.error = Some(IpcError::from_code(ErrorCode::ReadFailed)),
        Ok(metadata) => {
            entry.file_bytes = metadata.len();
            match open_in_pool(state, path, None) {
                Ok((page_count, pages)) => {
                    entry.page_count = page_count;
                    entry.pages = page_sizes(&pages);
                }
                Err(error) => {
                    entry.encrypted = error.code == ErrorCode::PasswordProtected;
                    entry.error = Some(error);
                }
            }
        }
    }
    entry
}

/// Adds the PDF files in `paths` to the organize list.
///
/// # Errors
///
/// `ConversionRunning` during a conversion.
pub fn add_pdfs(state: &AppState, paths: &[PathBuf]) -> Result<OrganizeAdded, IpcError> {
    refuse_while_running(state)?;
    Ok(add_files(state, paths))
}

/// Adds the PDF files directly in the folder `dir` to the organize list, as `add_pdfs_from_folder`
/// does for the list: subfolders and other files are counted as skipped.
///
/// # Errors
///
/// `ConversionRunning` during a conversion, and `ReadFailed` when the folder cannot be read.
pub fn add_pdfs_from_folder(state: &AppState, dir: &Path) -> Result<OrganizeAdded, IpcError> {
    refuse_while_running(state)?;
    let scan = scan_folder(dir).map_err(|_| IpcError::from_code(ErrorCode::ReadFailed))?;
    let mut result = add_files(state, &scan.pdfs);
    result.skipped = result.skipped.merged(Skipped {
        unsupported: scan.unsupported + count(scan.images.len()),
        folders: scan.folders,
        duplicates: 0,
    });
    Ok(result)
}

/// What a drop in the organize tab adds (design organize §4, design §7.2): the PDFs in it, and the
/// PDFs in the folders in it. Nothing is added during a conversion.
pub fn add_dropped(state: &AppState, paths: &[PathBuf]) -> OrganizeDropped {
    if let Err(error) = refuse_while_running(state) {
        return OrganizeDropped {
            added: Vec::new(),
            skipped: Skipped::default(),
            error: Some(error),
        };
    }
    let sorting = sort_dropped(paths);
    let result = add_files(state, &sorting.pdfs);
    let left_out = Skipped {
        unsupported: sorting.unsupported + count(sorting.images.len()),
        folders: sorting.folders,
        duplicates: 0,
    };
    OrganizeDropped {
        added: result.added,
        skipped: result.skipped.merged(left_out),
        error: None,
    }
}

/// Removes organize PDFs. An ID that is not here is ignored.
///
/// # Errors
///
/// `ConversionRunning` during a conversion.
pub fn remove_pdfs(state: &AppState, ids: &[u64]) -> Result<(), IpcError> {
    refuse_while_running(state)?;
    state.organize.remove(ids);
    Ok(())
}

/// Opens `path` with `password` and records the result for `id`, unless the password is wrong
/// or the worker failed, which leave the PDF as it is (design §6.8).
fn try_unlock(
    state: &AppState,
    id: u64,
    path: &Path,
    password: &Password,
) -> Result<Option<OrganizePdf>, IpcError> {
    let opened = open_in_pool(state, path, Some(password));
    if let Err(error) = &opened
        && matches!(
            error.code,
            ErrorCode::PasswordProtected | ErrorCode::WorkerCrashed | ErrorCode::WorkerTimeout
        )
    {
        return Err(error.clone());
    }
    let opened = opened.map(|(page_count, pages)| (page_count, page_sizes(&pages)));
    Ok(state.organize.unlocked(id, opened, password.clone()))
}

/// Opens the organize PDF `id` again with `password`, then tries the same password on the other
/// PDFs that wait for one (design §6.8). The answer has `id` first and the others by ID. The
/// lists and the crop tab are not tried.
///
/// # Errors
///
/// `ConversionRunning` during a conversion, `UnknownHandle` for an ID that is not here,
/// `InvalidParams` unless it waits for its password or when the password is empty,
/// `PasswordProtected` for a wrong password or one with a NUL, and `WorkerCrashed` or
/// `WorkerTimeout` when the worker failed. Then nothing changes.
pub fn unlock_pdf(state: &AppState, id: u64, password: &str) -> Result<OrganizeUnlocked, IpcError> {
    refuse_while_running(state)?;
    let target = state
        .organize
        .get(id)
        .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
    if !target.is_password_protected() {
        return Err(IpcError::from_code(ErrorCode::InvalidParams));
    }
    let password = match Password::new(password) {
        Ok(password) => password,
        Err(PasswordError::Empty) => return Err(IpcError::from_code(ErrorCode::InvalidParams)),
        Err(PasswordError::ContainsNul) => {
            return Err(IpcError::from_code(ErrorCode::PasswordProtected));
        }
    };

    let first = try_unlock(state, id, &target.path, &password)?
        .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
    let mut items = vec![first];
    for (candidate, path) in state.organize.candidates_for_unlock(id) {
        if let Ok(Some(item)) = try_unlock(state, candidate, &path, &password) {
            items.push(item);
        }
    }
    Ok(OrganizeUnlocked { items })
}

/// The PNG thumbnail of a page of the organize PDF `id`; see [`crate::items::make_thumbnail`].
/// `None` if `id` is not an organize PDF.
pub(crate) fn thumbnail(
    state: &AppState,
    id: u64,
    page: Option<u32>,
    max_side: Option<u32>,
) -> Option<Result<Vec<u8>, IpcError>> {
    let entry = state.organize.get(id)?;
    Some(thumbnail_of(state, &entry, page, max_side))
}

fn thumbnail_of(
    state: &AppState,
    entry: &OrganizeEntry,
    page: Option<u32>,
    max_side: Option<u32>,
) -> Result<Vec<u8>, IpcError> {
    if let Some(error) = &entry.error {
        return Err(error.clone());
    }
    let page = checked_page(page, entry.page_count)?;
    let max_side = checked_max_side(max_side)?;
    pdf_thumbnail(state, &entry.open_request(), page, max_side)
}

/// The estimated size of one output: for each PDF in it, `ceil(file_bytes × used_pages ÷
/// page_count)`, added up (design organize §2). `sources` has `(file_bytes, used_pages,
/// page_count)` of each PDF the output takes pages from. The product is made in 128 bits, and a
/// sum that does not fit in 64 bits stays at `u64::MAX`. The screen makes the same estimate from
/// the same `fileBytes`, so that it and the refusal of [`check_estimate`] cannot disagree.
pub fn estimate_output_bytes(sources: impl IntoIterator<Item = (u64, u32, u32)>) -> u64 {
    sources
        .into_iter()
        .map(|(file_bytes, used_pages, page_count)| {
            if page_count == 0 {
                return 0;
            }
            let bytes =
                (u128::from(file_bytes) * u128::from(used_pages)).div_ceil(u128::from(page_count));
            u64::try_from(bytes).unwrap_or(u64::MAX)
        })
        .fold(0, u64::saturating_add)
}

/// Refuses an output whose estimate is over [`MAX_ORGANIZE_OUTPUT_BYTES`]; exactly the limit is
/// let through.
///
/// # Errors
///
/// `OutputTooLarge`, with the estimate and the limit in MB (1,048,576 bytes) as `detail`: the
/// estimate rounded up, so that one just over the limit does not read as the limit.
pub fn check_estimate(estimate: u64) -> Result<(), IpcError> {
    if estimate <= MAX_ORGANIZE_OUTPUT_BYTES {
        return Ok(());
    }
    Err(IpcError::new(
        ErrorCode::OutputTooLarge,
        format!(
            "{} MB / {} MB",
            estimate.div_ceil(BYTES_PER_MB),
            MAX_ORGANIZE_OUTPUT_BYTES / BYTES_PER_MB
        ),
    ))
}

/// Checks the password of the output against the rule of design §4.3.1 (the PDF is not PDF/A,
/// so a password is allowed) and makes the one the worker takes.
///
/// # Errors
///
/// `InvalidParams` for a password that breaks the rule.
pub(crate) fn check_output_password(
    password: Option<String>,
) -> Result<Option<Password>, IpcError> {
    output_password(PdfConformance::Standard, password.clone())?;
    // The rule excludes what `Password` refuses (empty, NUL), so the second check cannot fail.
    password
        .map(Password::new)
        .transpose()
        .map_err(|_| IpcError::from_code(ErrorCode::InvalidParams))
}

/// Checks the pages of the outputs against the PDFs they name, before anything is made and before
/// the size is estimated. `parts` are the outputs; a single PDF is one part.
///
/// # Errors
///
/// `InvalidParams` for no output, an output without a page, a `rotation` other than 0, 90, 180
/// or 270, a `page` of 0 or past the end of its PDF, the same `(id, page)` twice (also across
/// outputs), and an output of more than [`MAX_PDF_PAGES`] pages, so that the app can open what
/// it made. `UnknownHandle` for a PDF that is not here, and the PDF's own error for one that
/// could not be opened.
pub(crate) fn check_parts(
    parts: &[&[OrganizePageInput]],
    sources: &HashMap<u64, Source>,
) -> Result<(), IpcError> {
    let invalid = || IpcError::from_code(ErrorCode::InvalidParams);
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err(invalid());
    }
    let max_pages = usize::try_from(MAX_PDF_PAGES).unwrap_or(usize::MAX);
    let mut seen = HashSet::new();
    for part in parts {
        for page in *part {
            if !matches!(page.rotation, 0 | 90 | 180 | 270) {
                return Err(invalid());
            }
            let source = sources
                .get(&page.id)
                .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
            if let Some(error) = &source.error {
                return Err(error.clone());
            }
            if page.page == 0 || page.page > source.page_count || !seen.insert((page.id, page.page))
            {
                return Err(invalid());
            }
        }
        if part.len() > max_pages {
            return Err(invalid());
        }
    }
    Ok(())
}

/// The estimate of the output made of `pages` (design organize §2).
pub(crate) fn estimate_part(pages: &[OrganizePageInput], sources: &HashMap<u64, Source>) -> u64 {
    let mut used: Vec<(u64, u32)> = Vec::new();
    for page in pages {
        match used.iter_mut().find(|(id, _)| *id == page.id) {
            Some((_, count)) => *count += 1,
            None => used.push((page.id, 1)),
        }
    }
    estimate_output_bytes(used.into_iter().filter_map(|(id, count)| {
        sources
            .get(&id)
            .map(|source| (source.file_bytes, count, source.page_count))
    }))
}

/// Refuses the first of `parts`, in order, whose estimate is over the limit.
///
/// # Errors
///
/// `OutputTooLarge` as [`check_estimate`], with the estimate of that output.
pub(crate) fn check_sizes(
    parts: &[&[OrganizePageInput]],
    sources: &HashMap<u64, Source>,
) -> Result<(), IpcError> {
    parts
        .iter()
        .try_for_each(|part| check_estimate(estimate_part(part, sources)))
}

/// The IDs of the PDFs that `parts` take pages from.
pub(crate) fn ids_of(parts: &[&[OrganizePageInput]]) -> Vec<u64> {
    let mut ids: Vec<u64> = parts
        .iter()
        .flat_map(|part| part.iter().map(|page| page.id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    fn source(id: u64, page_count: u32, file_bytes: u64) -> (u64, Source) {
        let source = Source {
            path: PathBuf::from(format!("{id}.pdf")),
            password: None,
            name: format!("{id}.pdf"),
            page_count,
            file_bytes,
            error: None,
        };
        (id, source)
    }

    fn sources(list: impl IntoIterator<Item = (u64, Source)>) -> HashMap<u64, Source> {
        list.into_iter().collect()
    }

    fn page(id: u64, page: u32) -> OrganizePageInput {
        OrganizePageInput {
            id,
            page,
            rotation: 0,
        }
    }

    #[test]
    fn an_estimate_is_rounded_up_for_each_pdf() {
        // 10 bytes over 3 pages: 1 page is 3.33.. bytes, 2 pages are 6.66..
        assert_eq!(estimate_output_bytes([(10, 1, 3)]), 4);
        assert_eq!(estimate_output_bytes([(10, 2, 3)]), 7);
        assert_eq!(estimate_output_bytes([(10, 3, 3)]), 10);
        // Each PDF is rounded on its own: 4 + 4, not ceil(6.66..).
        assert_eq!(estimate_output_bytes([(10, 1, 3), (10, 1, 3)]), 8);
    }

    #[test]
    fn an_estimate_of_a_whole_pdf_is_its_size_and_of_no_page_is_nothing() {
        assert_eq!(estimate_output_bytes([(1234, 7, 7)]), 1234);
        assert_eq!(estimate_output_bytes([(1234, 0, 7)]), 0);
        assert_eq!(estimate_output_bytes([(1234, 1, 0)]), 0);
        assert_eq!(estimate_output_bytes([]), 0);
    }

    /// The same rows are in the screen's test of `estimateOutputBytes`
    /// (`src/features/organize/estimate.test.ts`): `(file_bytes, used_pages, page_count)` of each
    /// PDF, then the estimate. A row changed here is changed there.
    #[test]
    fn the_estimates_of_the_table_the_screen_tests_too() {
        type Row = (&'static [(u64, u32, u32)], u64);
        let table: [Row; 6] = [
            (&[(1000, 1, 3)], 334),
            (&[(1000, 3, 3)], 1000),
            (&[(1000, 0, 3)], 0),
            (&[(10, 1, 4), (7, 2, 3)], 8),
            (&[(500, 2, 0)], 0),
            (&[(9_007_199_254_740_991, 3, 4)], 6_755_399_441_055_744),
        ];
        for (sources, expected) in table {
            assert_eq!(
                estimate_output_bytes(sources.iter().copied()),
                expected,
                "{sources:?}"
            );
        }
    }

    #[test]
    fn an_estimate_is_made_in_128_bits_and_saturates_at_the_top() {
        // 2^63 × 4 does not fit in 64 bits, but ÷ 4 does.
        let big = 1u64 << 63;
        assert_eq!(estimate_output_bytes([(big, 4, 4)]), big);
        // The product fits in 128 bits and the result does not fit in 64.
        assert_eq!(estimate_output_bytes([(u64::MAX, 2, 1)]), u64::MAX);
        // A sum past the top stays there.
        assert_eq!(
            estimate_output_bytes([(u64::MAX, 1, 1), (10, 1, 1)]),
            u64::MAX
        );
    }

    #[test]
    fn the_limit_itself_is_let_through_and_one_byte_more_is_refused() {
        assert!(check_estimate(MAX_ORGANIZE_OUTPUT_BYTES).is_ok());
        assert!(check_estimate(0).is_ok());
        let error = check_estimate(MAX_ORGANIZE_OUTPUT_BYTES + 1).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutputTooLarge);
    }

    #[test]
    fn the_detail_has_the_estimate_rounded_up_and_the_limit_in_mb() {
        let detail = |estimate: u64| check_estimate(estimate).unwrap_err().detail;
        // 512 MB + 1 byte reads 513, not the limit's own 512.
        assert_eq!(
            detail(MAX_ORGANIZE_OUTPUT_BYTES + 1).as_deref(),
            Some("513 MB / 512 MB")
        );
        assert_eq!(detail(613 * MB - 1).as_deref(), Some("613 MB / 512 MB"));
        assert_eq!(detail(613 * MB).as_deref(), Some("613 MB / 512 MB"));
        assert_eq!(detail(613 * MB + 1).as_deref(), Some("614 MB / 512 MB"));
    }

    #[test]
    fn the_first_output_over_the_limit_is_the_one_that_is_refused() {
        // 600 MB and 700 MB of one-page files.
        let sources = sources([
            source(1, 1, 100 * MB),
            source(2, 1, 600 * MB),
            source(3, 1, 700 * MB),
        ]);
        let parts: [&[OrganizePageInput]; 3] = [&[page(1, 1)], &[page(3, 1)], &[page(2, 1)]];
        let error = check_sizes(&parts, &sources).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutputTooLarge);
        assert_eq!(error.detail.as_deref(), Some("700 MB / 512 MB"));
        assert!(check_sizes(&parts[..1], &sources).is_ok());
    }

    #[test]
    fn an_output_is_estimated_from_the_pages_it_uses_of_each_pdf() {
        let sources = sources([source(1, 10, 1000), source(2, 4, 400)]);
        let pages = [page(1, 1), page(2, 2), page(1, 5), page(1, 9)];
        // 3 of 10 pages of 1000 bytes, 1 of 4 pages of 400 bytes.
        assert_eq!(estimate_part(&pages, &sources), 300 + 100);
    }

    #[test]
    fn a_part_is_checked_for_the_limit_on_its_own_not_on_the_total() {
        let sources = sources([source(1, 2, 600 * MB)]);
        // Each page is 300 MB: one page at a time is below the limit, both are not.
        assert!(check_sizes(&[&[page(1, 1)], &[page(1, 2)]], &sources).is_ok());
        assert!(check_sizes(&[&[page(1, 1), page(1, 2)]], &sources).is_err());
    }

    fn check(parts: &[&[OrganizePageInput]], sources: &HashMap<u64, Source>) -> ErrorCode {
        check_parts(parts, sources)
            .expect_err("the pages should be refused")
            .code
    }

    #[test]
    fn nothing_to_make_is_refused() {
        let sources = sources([source(1, 3, 10)]);
        assert_eq!(check(&[], &sources), ErrorCode::InvalidParams);
        assert_eq!(check(&[&[]], &sources), ErrorCode::InvalidParams);
        assert_eq!(
            check(&[&[page(1, 1)], &[]], &sources),
            ErrorCode::InvalidParams
        );
    }

    #[test]
    fn only_quarter_turns_are_rotations() {
        let sources = sources([source(1, 3, 10)]);
        for rotation in [0, 90, 180, 270] {
            let pages = [OrganizePageInput {
                id: 1,
                page: 1,
                rotation,
            }];
            assert!(check_parts(&[&pages], &sources).is_ok(), "{rotation}");
        }
        for rotation in [1, 45, 91, 360, 450] {
            let pages = [OrganizePageInput {
                id: 1,
                page: 1,
                rotation,
            }];
            assert_eq!(check(&[&pages], &sources), ErrorCode::InvalidParams);
        }
    }

    #[test]
    fn a_page_must_be_a_page_of_its_pdf() {
        let sources = sources([source(1, 3, 10)]);
        assert!(check_parts(&[&[page(1, 1), page(1, 3)]], &sources).is_ok());
        assert_eq!(check(&[&[page(1, 0)]], &sources), ErrorCode::InvalidParams);
        assert_eq!(check(&[&[page(1, 4)]], &sources), ErrorCode::InvalidParams);
    }

    #[test]
    fn the_same_page_twice_is_refused_also_across_outputs() {
        let sources = sources([source(1, 3, 10), source(2, 3, 10)]);
        assert_eq!(
            check(&[&[page(1, 1), page(1, 1)]], &sources),
            ErrorCode::InvalidParams
        );
        assert_eq!(
            check(&[&[page(1, 1)], &[page(2, 1), page(1, 1)]], &sources),
            ErrorCode::InvalidParams
        );
        // The same page number of another PDF is another page.
        assert!(check_parts(&[&[page(1, 1)], &[page(2, 1)]], &sources).is_ok());
    }

    #[test]
    fn an_unknown_pdf_and_a_pdf_with_an_error_are_refused_with_their_own_codes() {
        let mut broken = source(2, 0, 10);
        broken.1.error = Some(IpcError::from_code(ErrorCode::PdfOpenFailed));
        let sources = sources([source(1, 3, 10), broken]);
        assert_eq!(check(&[&[page(9, 1)]], &sources), ErrorCode::UnknownHandle);
        assert_eq!(
            check(&[&[page(1, 1), page(2, 1)]], &sources),
            ErrorCode::PdfOpenFailed
        );
    }

    #[test]
    fn one_output_may_have_as_many_pages_as_the_app_opens_and_no_more() {
        let sources = sources([source(1, MAX_PDF_PAGES + 1, 10)]);
        let pages = |count: u32| -> Vec<OrganizePageInput> {
            (1..=count).map(|number| page(1, number)).collect()
        };
        let at_the_limit = pages(MAX_PDF_PAGES);
        assert!(check_parts(&[&at_the_limit], &sources).is_ok());
        let over = pages(MAX_PDF_PAGES + 1);
        assert_eq!(check(&[&over], &sources), ErrorCode::InvalidParams);
    }

    #[test]
    fn the_page_limit_is_for_each_output_and_not_for_the_total() {
        let sources = sources([source(1, MAX_PDF_PAGES + 1, 10)]);
        let first: Vec<OrganizePageInput> = (1..=MAX_PDF_PAGES).map(|n| page(1, n)).collect();
        let second = [page(1, MAX_PDF_PAGES + 1)];
        assert!(check_parts(&[&first, &second], &sources).is_ok());
    }

    #[test]
    fn the_password_of_the_output_follows_the_rule_of_the_writer() {
        assert!(check_output_password(None).unwrap().is_none());
        assert!(
            check_output_password(Some("secret".into()))
                .unwrap()
                .is_some()
        );
        for bad in ["", "日本語", "a\0b", &"x".repeat(1000)] {
            assert_eq!(
                check_output_password(Some(bad.into())).unwrap_err().code,
                ErrorCode::InvalidParams,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_ids_of_the_pages_are_listed_once_each() {
        let parts: [&[OrganizePageInput]; 2] = [&[page(3, 1), page(1, 1)], &[page(3, 2)]];
        assert_eq!(ids_of(&parts), [1, 3]);
    }
}
