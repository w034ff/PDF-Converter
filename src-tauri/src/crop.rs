//! The PDF the crop tab works on: opening it, the pending PDF that waits for
//! the screen to confirm a switch, the questions the tab asks about a page,
//! and the analysis worker (design §5.2, §6.10, §7.1).
//!
//! Crop PDFs are kept apart from the lists of `items.rs`: the table refuses a
//! second copy of a file, which would keep a PDF that is in the list from being
//! cropped, and the commands of the lists must not act on a crop PDF.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(feature = "test-hooks")]
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "test-hooks")]
use std::time::Duration;

use pdfconv_worker::client::WorkerProcess;
use pdfconv_worker::protocol::{
    CandidateKind as WorkerCandidateKind, EmbeddedImage, LayoutCandidate, Password, PasswordError,
    RectPt, Request, Response,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::items::{
    ItemKind, ItemsDropped, PageSizePt, add_dropped, checked_max_side, checked_page, file_name,
    kind_of_extension, open_in_pool, pdf_thumbnail, request_on_pdf, unexpected_response,
};
use crate::jobs::{self, FinishJob, JobFinishedPayload, JobProgressPayload};
use crate::worker_pool::{WorkerPool, WorkerPoolError};

/// Name of the event that carries the candidates of one page of
/// `start_detect_all` (design §7.2).
pub const LAYOUT_PAGE_EVENT: &str = "layout-page";

/// Where the next drop goes: the lists of the other tabs or the crop tab
/// (design §6.10). It is a value of its own rather than a flag so that a third
/// place can be added.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum DropTarget {
    #[default]
    Lists,
    Crop,
}

/// A PDF of the crop tab (design §7.1). When `error` is set, the PDF could not
/// be opened (or still needs its password): the page count is 0 and `pages` is
/// empty.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CropPdf {
    #[ts(type = "number")]
    pub id: u64,
    /// The file name, without the folder.
    pub name: String,
    pub page_count: u32,
    /// The size of every page, in the orientation the page is shown in.
    pub pages: Vec<PageSizePt>,
    pub error: Option<IpcError>,
}

/// An embedded image of a page (design §7.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PageImage {
    pub image_index: u32,
    /// `[x0, y0, x1, y1]` in points, with the origin at the top left of the
    /// page as shown (design §4.6).
    pub rect_pt: [f64; 4],
    pub px_width: u32,
    pub px_height: u32,
    /// Whether the image can be written in its own pixels.
    pub native: bool,
}

/// What a layout candidate is (design §4.7, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum CandidateKind {
    Photo,
    Figure,
    Chart,
    Table,
}

/// A region that layout detection suggests cropping (design §7.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub kind: CandidateKind,
    pub score: f32,
    /// `[x0, y0, x1, y1]`, as in [`PageImage`].
    pub rect_pt: [f64; 4],
    /// The embedded image behind a photo; `null` for the other kinds.
    pub image_index: Option<u32>,
}

/// The payload of the `layout-page` event: what `start_detect_all` found on
/// one page, or why it could not (design §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPagePayload {
    /// The crop PDF the page belongs to.
    #[ts(type = "number")]
    pub id: u64,
    pub page: u32,
    /// Empty when `error` is set.
    pub candidates: Vec<Candidate>,
    pub error: Option<IpcError>,
}

/// The payload of the `crop-dropped` event (design §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CropDropped {
    /// The pending PDF, if the drop was one PDF file.
    pub crop_pdf: Option<CropPdf>,
    /// Whether the drop was anything but one PDF file, so nothing was opened.
    pub not_one_pdf: bool,
    pub error: Option<IpcError>,
}

/// What a drop is turned into, for the event that carries it.
#[derive(Debug, Clone, PartialEq)]
pub enum Dropped {
    Items(ItemsDropped),
    Crop(CropDropped),
}

fn rect_array(rect: &RectPt) -> [f64; 4] {
    [rect.x0, rect.y0, rect.x1, rect.y1]
}

impl From<&EmbeddedImage> for PageImage {
    fn from(image: &EmbeddedImage) -> Self {
        Self {
            image_index: image.image_index,
            rect_pt: rect_array(&image.rect_pt),
            px_width: image.px_width,
            px_height: image.px_height,
            native: image.native,
        }
    }
}

impl From<WorkerCandidateKind> for CandidateKind {
    fn from(kind: WorkerCandidateKind) -> Self {
        match kind {
            WorkerCandidateKind::Photo => Self::Photo,
            WorkerCandidateKind::Figure => Self::Figure,
            WorkerCandidateKind::Chart => Self::Chart,
            WorkerCandidateKind::Table => Self::Table,
        }
    }
}

impl From<&LayoutCandidate> for Candidate {
    fn from(candidate: &LayoutCandidate) -> Self {
        Self {
            kind: candidate.kind.into(),
            score: candidate.score,
            rect_pt: rect_array(&candidate.rect_pt),
            image_index: candidate.image_index,
        }
    }
}

/// What is kept for one crop PDF.
#[derive(Debug, Clone)]
struct CropEntry {
    id: u64,
    path: PathBuf,
    name: String,
    page_count: u32,
    pages: Vec<PageSizePt>,
    error: Option<IpcError>,
    password: Option<Password>,
}

impl CropEntry {
    fn to_ipc(&self) -> CropPdf {
        CropPdf {
            id: self.id,
            name: self.name.clone(),
            page_count: self.page_count,
            pages: self.pages.clone(),
            error: self.error.clone(),
        }
    }

    fn open_request(&self) -> Request {
        Request::Open {
            path: self.path.clone(),
            password: self.password.clone(),
        }
    }

    fn is_password_protected(&self) -> bool {
        self.error.as_ref().map(|error| error.code) == Some(ErrorCode::PasswordProtected)
    }
}

/// The PDF the tab shows, with its analysis worker. The worker is shared so
/// that a request still waiting for it keeps it alive after the PDF has been
/// replaced, and the last holder ends it.
struct OpenPdf {
    entry: CropEntry,
    analyzer: Arc<Analyzer>,
}

#[derive(Default)]
struct Slots {
    open: Option<OpenPdf>,
    pending: Option<CropEntry>,
}

/// What a question about the open PDF needs, taken under the lock so that the
/// lock is not held while a worker answers.
struct Snapshot {
    open: Request,
    name: String,
    page_count: u32,
    analyzer: Arc<Analyzer>,
}

/// What a crop export needs of the open PDF (design §6.10).
pub(crate) struct ExportTarget {
    pub(crate) open: Request,
    pub(crate) name: String,
    pub(crate) page_count: u32,
    pub(crate) pages: Vec<PageSizePt>,
}

/// The crop tab's part of [`AppState`]: the open PDF, the pending PDF, and
/// where a drop goes.
#[derive(Default)]
pub struct CropState {
    slots: Mutex<Slots>,
    drop_target: Mutex<DropTarget>,
    /// How many analysis workers were started, across PDFs.
    #[cfg(feature = "test-hooks")]
    analysis_spawns: Arc<AtomicUsize>,
}

impl CropState {
    fn lock(&self) -> MutexGuard<'_, Slots> {
        self.slots
            .lock()
            .expect("the crop state is never locked across a panic")
    }

    /// Where the next drop goes.
    pub fn drop_target(&self) -> DropTarget {
        *self
            .drop_target
            .lock()
            .expect("the drop target is never locked across a panic")
    }

    /// Sets where the next drop goes (design §6.10).
    pub fn set_drop_target(&self, target: DropTarget) {
        *self
            .drop_target
            .lock()
            .expect("the drop target is never locked across a panic") = target;
    }

    /// Goes back to the lists: a new page starts on a tab that is not the crop
    /// tab until the screen says otherwise.
    pub fn start_page(&self) {
        self.set_drop_target(DropTarget::default());
    }

    /// Holds `entry` as the pending PDF, dropping the one before.
    fn hold(&self, entry: CropEntry) {
        let previous = self.lock().pending.replace(entry);
        drop(previous);
    }

    fn activate(&self, id: u64, pool: &WorkerPool) -> Result<(), IpcError> {
        let replaced = {
            let mut slots = self.lock();
            let entry = slots
                .pending
                .take_if(|pending| pending.id == id)
                .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
            slots.open.replace(OpenPdf {
                entry,
                analyzer: Arc::new(Analyzer::new(
                    pool.clone(),
                    #[cfg(feature = "test-hooks")]
                    Arc::clone(&self.analysis_spawns),
                )),
            })
        };
        // The last holder of the old worker ends it; that waits for the
        // process, which does not belong under the lock.
        drop(replaced);
        Ok(())
    }

    fn discard(&self, id: u64) {
        let discarded = self.lock().pending.take_if(|pending| pending.id == id);
        drop(discarded);
    }

    /// Reads the open PDF `id` under the lock, if it can be asked about.
    ///
    /// # Errors
    ///
    /// `UnknownHandle` unless `id` is the open PDF, and the PDF's own error if
    /// it could not be opened or needs its password.
    fn read_usable<T>(&self, id: u64, read: impl FnOnce(&OpenPdf) -> T) -> Result<T, IpcError> {
        let slots = self.lock();
        let open = slots
            .open
            .as_ref()
            .filter(|open| open.entry.id == id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if let Some(error) = &open.entry.error {
            return Err(error.clone());
        }
        Ok(read(open))
    }

    /// The open PDF `id`, ready to be asked about.
    ///
    /// # Errors
    ///
    /// As [`Self::read_usable`].
    fn snapshot(&self, id: u64) -> Result<Snapshot, IpcError> {
        self.read_usable(id, |open| Snapshot {
            open: open.entry.open_request(),
            name: open.entry.name.clone(),
            page_count: open.entry.page_count,
            analyzer: Arc::clone(&open.analyzer),
        })
    }

    /// The open PDF `id`, ready to be exported.
    ///
    /// # Errors
    ///
    /// As [`Self::read_usable`].
    pub(crate) fn export_target(&self, id: u64) -> Result<ExportTarget, IpcError> {
        self.read_usable(id, |open| ExportTarget {
            open: open.entry.open_request(),
            name: open.entry.name.clone(),
            page_count: open.entry.page_count,
            pages: open.entry.pages.clone(),
        })
    }

    /// The path of the open PDF `id` if it waits for its password.
    fn path_to_unlock(&self, id: u64) -> Result<PathBuf, IpcError> {
        let slots = self.lock();
        let open = slots
            .open
            .as_ref()
            .filter(|open| open.entry.id == id)
            .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
        if !open.entry.is_password_protected() {
            return Err(IpcError::from_code(ErrorCode::InvalidParams));
        }
        Ok(open.entry.path.clone())
    }

    /// Records what opening the open PDF `id` with `password` gave. `None` if
    /// `id` is not the open PDF any more.
    fn unlocked(
        &self,
        id: u64,
        opened: Result<(u32, Vec<PageSizePt>), IpcError>,
        password: Password,
    ) -> Option<CropPdf> {
        let mut slots = self.lock();
        let entry = &mut slots
            .open
            .as_mut()
            .filter(|open| open.entry.id == id)?
            .entry;
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

    /// How many analysis workers were started.
    #[cfg(feature = "test-hooks")]
    pub fn analysis_spawn_count(&self) -> usize {
        self.analysis_spawns.load(Ordering::SeqCst)
    }

    /// Sends `request` to the running analysis worker of the open PDF `id`,
    /// to test how it is handled when it crashes or hangs.
    ///
    /// # Errors
    ///
    /// `UnknownHandle` unless `id` is the open PDF, `InvalidParams` while no
    /// analysis worker is running, and the error of the request otherwise.
    #[cfg(feature = "test-hooks")]
    pub fn send_to_analysis_worker(
        &self,
        id: u64,
        request: &Request,
        timeout: Duration,
    ) -> Result<(), IpcError> {
        let analyzer = {
            let slots = self.lock();
            let open = slots
                .open
                .as_ref()
                .filter(|open| open.entry.id == id)
                .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))?;
            Arc::clone(&open.analyzer)
        };
        analyzer.send_for_test(request, timeout)
    }
}

/// The worker of one open PDF that runs the layout analysis (design §5.2).
/// It is not in the pool, because a worker that has loaded the model takes
/// about 0.5 GB and more of them would add up.
struct Analyzer {
    pool: WorkerPool,
    state: Mutex<AnalyzerState>,
    /// The pages that have been analyzed without an error, by
    /// [`Self::detect`]. It is not in `state`, which is locked while a worker
    /// answers, so that asking for it never waits for an analysis. It lives and
    /// dies with the analyzer, that is with the PDF it belongs to.
    analyzed: Mutex<BTreeSet<u32>>,
    #[cfg(feature = "test-hooks")]
    spawns: Arc<AtomicUsize>,
}

#[derive(Default)]
struct AnalyzerState {
    /// The worker, if one is running. It is taken out while a request is
    /// being answered and put back when it can be used again.
    worker: Option<Running>,
    /// Set once the worker answered `LayoutUnavailable`. The worker cannot
    /// tell a model that failed to load, which must not be loaded a second
    /// time, from an analysis that failed, so both are final until the PDF is
    /// opened again (design §5.2).
    unavailable: Option<IpcError>,
}

struct Running {
    process: WorkerProcess,
    /// Whether the worker has the PDF open.
    opened: bool,
}

/// Puts the worker back in `state` or lets it go, by how the request ended: a
/// crashed or timed-out worker is gone and the next request starts another;
/// after `LayoutUnavailable` the error is remembered and no other worker is
/// started; any other error leaves the worker as it was.
fn settle<T>(
    state: &mut AnalyzerState,
    running: Running,
    result: Result<T, WorkerPoolError>,
) -> Result<T, IpcError> {
    match result {
        Ok(value) => {
            state.worker = Some(running);
            Ok(value)
        }
        Err(error) => {
            let error = IpcError::from(error);
            match error.code {
                ErrorCode::LayoutUnavailable => state.unavailable = Some(error.clone()),
                ErrorCode::WorkerCrashed | ErrorCode::WorkerTimeout => {}
                _ => state.worker = Some(running),
            }
            Err(error)
        }
    }
}

impl Analyzer {
    fn new(pool: WorkerPool, #[cfg(feature = "test-hooks")] spawns: Arc<AtomicUsize>) -> Self {
        Self {
            pool,
            state: Mutex::new(AnalyzerState::default()),
            analyzed: Mutex::new(BTreeSet::new()),
            #[cfg(feature = "test-hooks")]
            spawns,
        }
    }

    fn lock(&self) -> MutexGuard<'_, AnalyzerState> {
        self.state
            .lock()
            .expect("the analysis worker is never locked across a panic")
    }

    /// Detects the layout of `page`, starting the worker and opening the PDF
    /// with `open` first if that has not happened. Requests wait for each
    /// other: the lock is held while the worker answers.
    fn detect(&self, open: &Request, page: u32) -> Result<Vec<Candidate>, IpcError> {
        let mut state = self.lock();
        if let Some(error) = &state.unavailable {
            return Err(error.clone());
        }
        let mut running = match state.worker.take() {
            Some(running) => running,
            None => self.start()?,
        };
        let candidates = self.exchange(&mut running, open, page);
        let candidates = settle(&mut state, running, candidates)?;
        self.analyzed
            .lock()
            .expect("the analyzed pages are never locked across a panic")
            .insert(page);
        Ok(candidates.iter().map(Candidate::from).collect())
    }

    /// The pages of 1..=`page_count` that [`Self::detect`] has not answered.
    fn pending_pages(&self, page_count: u32) -> Vec<u32> {
        let analyzed = self
            .analyzed
            .lock()
            .expect("the analyzed pages are never locked across a panic");
        (1..=page_count)
            .filter(|page| !analyzed.contains(page))
            .collect()
    }

    fn start(&self) -> Result<Running, IpcError> {
        let process = self.pool.spawn_dedicated()?;
        #[cfg(feature = "test-hooks")]
        self.spawns.fetch_add(1, Ordering::SeqCst);
        Ok(Running {
            process,
            opened: false,
        })
    }

    fn exchange(
        &self,
        running: &mut Running,
        open: &Request,
        page: u32,
    ) -> Result<Vec<LayoutCandidate>, WorkerPoolError> {
        if !running.opened {
            let timeout = self.pool.timeout_for(open);
            match running.process.request(open, timeout)? {
                (Response::Open { .. }, _) => running.opened = true,
                _ => return Err(WorkerPoolError::WorkerCrashed),
            }
        }
        let detect = Request::Detect { page };
        let timeout = self.pool.timeout_for(&detect);
        match running.process.request(&detect, timeout)? {
            (Response::Detect { candidates }, _) => Ok(candidates),
            _ => Err(WorkerPoolError::WorkerCrashed),
        }
    }

    #[cfg(feature = "test-hooks")]
    fn send_for_test(&self, request: &Request, timeout: Duration) -> Result<(), IpcError> {
        let mut state = self.lock();
        let Some(mut running) = state.worker.take() else {
            return Err(IpcError::from_code(ErrorCode::InvalidParams));
        };
        let result = running
            .process
            .request(request, timeout)
            .map(|_| ())
            .map_err(WorkerPoolError::from);
        settle(&mut state, running, result)
    }
}

fn refuse_while_running(state: &AppState) -> Result<(), IpcError> {
    if state.is_running.load(Ordering::SeqCst) {
        return Err(IpcError::from_code(ErrorCode::ConversionRunning));
    }
    Ok(())
}

/// Opens the PDF at `path` and holds it as the pending PDF, dropping the one
/// before; the open PDF is not touched (design §6.10). A PDF that cannot be
/// opened is held with its error.
pub fn open_pdf(state: &AppState, path: &Path) -> CropPdf {
    let id = state.items.next_id();
    let (page_count, pages, error) = match open_in_pool(state, path, None) {
        Ok((page_count, pages)) => (page_count, page_sizes(&pages), None),
        Err(error) => (0, Vec::new(), Some(error)),
    };
    let entry = CropEntry {
        id,
        path: path.to_path_buf(),
        name: file_name(path),
        page_count,
        pages,
        error,
        password: None,
    };
    let held = entry.to_ipc();
    state.crop.hold(entry);
    held
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

/// Makes the pending PDF `id` the open PDF and ends the analysis worker of the
/// one before, without waiting for a request that is using it (design §6.10).
///
/// # Errors
///
/// `ConversionRunning` during a conversion, and `UnknownHandle` unless `id` is
/// the pending PDF.
pub fn activate_crop_pdf(state: &AppState, id: u64) -> Result<(), IpcError> {
    refuse_while_running(state)?;
    state.crop.activate(id, &state.pool)
}

/// Drops the pending PDF if it is `id`; any other `id` is ignored.
pub fn discard_crop_pdf(state: &AppState, id: u64) {
    state.crop.discard(id);
}

/// Opens the open PDF `id` again with `password` (design §6.8, §6.10). No other
/// PDF is tried.
///
/// # Errors
///
/// `ConversionRunning` during a conversion, `UnknownHandle` unless `id` is the
/// open PDF, `InvalidParams` unless it waits for its password or when the
/// password is empty, `PasswordProtected` for a wrong password or one with a
/// NUL, and `WorkerCrashed` or `WorkerTimeout` when the worker failed. Then
/// nothing changes.
pub fn unlock_crop_pdf(state: &AppState, id: u64, password: &str) -> Result<CropPdf, IpcError> {
    refuse_while_running(state)?;
    let path = state.crop.path_to_unlock(id)?;
    let password = match Password::new(password) {
        Ok(password) => password,
        Err(PasswordError::Empty) => return Err(IpcError::from_code(ErrorCode::InvalidParams)),
        Err(PasswordError::ContainsNul) => {
            return Err(IpcError::from_code(ErrorCode::PasswordProtected));
        }
    };
    let opened = open_in_pool(state, &path, Some(&password));
    if let Err(error) = &opened
        && matches!(
            error.code,
            ErrorCode::PasswordProtected | ErrorCode::WorkerCrashed | ErrorCode::WorkerTimeout
        )
    {
        return Err(error.clone());
    }
    let opened = opened.map(|(page_count, pages)| (page_count, page_sizes(&pages)));
    state
        .crop
        .unlocked(id, opened, password)
        .ok_or_else(|| IpcError::from_code(ErrorCode::UnknownHandle))
}

/// The thumbnail of a page of the open PDF `id`; see
/// [`crate::items::make_thumbnail`].
pub(crate) fn crop_thumbnail(
    state: &AppState,
    id: u64,
    page: Option<u32>,
    max_side: Option<u32>,
) -> Result<Vec<u8>, IpcError> {
    let snapshot = state.crop.snapshot(id)?;
    let page = checked_page(page, snapshot.page_count)?;
    let max_side = checked_max_side(max_side)?;
    pdf_thumbnail(state, &snapshot.open, page, max_side)
}

/// The embedded images of a page of the open PDF `id` (design §5.1, §7.1).
///
/// # Errors
///
/// `UnknownHandle` unless `id` is the open PDF, its own error if it could not
/// be opened, and `InvalidParams` when `page` is not a page of it.
pub fn list_page_images(state: &AppState, id: u64, page: u32) -> Result<Vec<PageImage>, IpcError> {
    let snapshot = state.crop.snapshot(id)?;
    let page = checked_page(Some(page), snapshot.page_count)?;
    match request_on_pdf(state, &snapshot.open, &Request::PageImages { page })? {
        (Response::PageImages { images }, _) => Ok(images.iter().map(PageImage::from).collect()),
        _ => Err(unexpected_response()),
    }
}

/// Whether anything is drawn over the embedded image `image_index` of a page
/// of the open PDF `id` (design §5.1, §7.1).
///
/// # Errors
///
/// As [`list_page_images`], and the worker's `InvalidParams` when the page has
/// no such image.
pub fn check_on_top(
    state: &AppState,
    id: u64,
    page: u32,
    image_index: u32,
) -> Result<bool, IpcError> {
    let snapshot = state.crop.snapshot(id)?;
    let page = checked_page(Some(page), snapshot.page_count)?;
    let request = Request::ImageOnTop { page, image_index };
    match request_on_pdf(state, &snapshot.open, &request)? {
        (Response::ImageOnTop { on_top }, _) => Ok(on_top),
        _ => Err(unexpected_response()),
    }
}

/// The candidates for cropping on a page of the open PDF `id`, from the
/// analysis worker (design §5.2, §7.1). It is allowed during a conversion,
/// because the worker is not the pool's.
///
/// # Errors
///
/// As [`list_page_images`], and `LayoutUnavailable` when the layout analysis
/// cannot run, which it keeps saying until the PDF is opened again.
pub fn detect_layout(state: &AppState, id: u64, page: u32) -> Result<Vec<Candidate>, IpcError> {
    let snapshot = state.crop.snapshot(id)?;
    let page = checked_page(Some(page), snapshot.page_count)?;
    snapshot.analyzer.detect(&snapshot.open, page)
}

/// What `start_detect_all` reports through: the progress, the answer for each
/// page (`layout-page`) and the end of the job.
pub struct DetectAllCallbacks<FProg, FPage, FFin> {
    pub on_progress: FProg,
    pub on_page: FPage,
    pub on_finished: FFin,
}

/// Starts the layout analysis of every page of the open PDF `id` that has not
/// been analyzed, in the background, as a conversion (design §6.10, §7.1).
///
/// The pages go through the analysis worker of the PDF one at a time, in page
/// order, taking turns with [`detect_layout`] for its lock. A page that
/// `detect_layout` answered, or an earlier run did, is left out: the screen
/// has its candidates. `LayoutUnavailable` ends the run, since the worker
/// keeps answering it; any other error fails the page and the run goes on.
///
/// # Errors
///
/// `ConversionRunning` during a conversion, `UnknownHandle` unless `id` is the
/// open PDF, and its own error if it could not be opened. Nothing starts then.
pub fn start_detect_all_internal<FProg, FPage, FFin>(
    state: &AppState,
    id: u64,
    callbacks: DetectAllCallbacks<FProg, FPage, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + 'static,
    FPage: Fn(LayoutPagePayload) + Send + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let guard = jobs::begin_conversion(state)?;
    let snapshot = state.crop.snapshot(id)?;
    let pages = snapshot.analyzer.pending_pages(snapshot.page_count);
    let failed_if_it_panics = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    let DetectAllCallbacks {
        on_progress,
        on_page,
        on_finished,
    } = callbacks;
    jobs::spawn_conversion(
        state,
        guard,
        on_finished,
        failed_if_it_panics,
        move |state, finish| {
            run_detect_all(
                &state.cancel_flag,
                id,
                &snapshot,
                &pages,
                &on_progress,
                &on_page,
                finish,
            );
            Ok(())
        },
    );
    Ok(())
}

fn run_detect_all(
    cancel_flag: &AtomicBool,
    id: u64,
    snapshot: &Snapshot,
    pages: &[u32],
    on_progress: &dyn Fn(JobProgressPayload),
    on_page: &dyn Fn(LayoutPagePayload),
    finish: FinishJob,
) {
    let total = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    let progress = |done: u32| {
        on_progress(JobProgressPayload {
            done,
            total,
            current: Some(snapshot.name.clone()),
        });
    };
    let (mut succeeded, mut failed, mut reached, mut cancelled) = (0, 0, 0, false);
    if total > 0 {
        progress(0);
    }
    for &page in pages {
        if cancel_flag.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        let answer = snapshot.analyzer.detect(&snapshot.open, page);
        reached += 1;
        let (payload, unavailable) = match answer {
            Ok(candidates) => {
                succeeded += 1;
                let payload = LayoutPagePayload {
                    id,
                    page,
                    candidates,
                    error: None,
                };
                (payload, false)
            }
            Err(error) => {
                failed += 1;
                let unavailable = error.code == ErrorCode::LayoutUnavailable;
                let payload = LayoutPagePayload {
                    id,
                    page,
                    candidates: Vec::new(),
                    error: Some(error),
                };
                (payload, unavailable)
            }
        };
        on_page(payload);
        progress(reached);
        if unavailable {
            break;
        }
    }
    finish(JobFinishedPayload {
        succeeded,
        failed,
        no_pages: 0,
        unprocessed: total - reached,
        cancelled,
    });
}

/// What a drop in the crop tab opens (design §6.10, §7.2): the one PDF file
/// that was dropped, as the pending PDF. Anything else opens nothing.
pub fn crop_dropped(state: &AppState, paths: &[PathBuf]) -> CropDropped {
    if refuse_while_running(state).is_err() {
        return CropDropped {
            crop_pdf: None,
            not_one_pdf: false,
            error: Some(IpcError::from_code(ErrorCode::ConversionRunning)),
        };
    }
    match paths {
        [path] if !path.is_dir() && kind_of_extension(path) == Some(ItemKind::Pdf) => CropDropped {
            crop_pdf: Some(open_pdf(state, path)),
            not_one_pdf: false,
            error: None,
        },
        _ => CropDropped {
            crop_pdf: None,
            not_one_pdf: true,
            error: None,
        },
    }
}

/// Handles a drop according to where drops go (design §6.10).
pub fn handle_drop(state: &AppState, paths: &[PathBuf]) -> Dropped {
    match state.crop.drop_target() {
        DropTarget::Lists => Dropped::Items(add_dropped(state, paths)),
        DropTarget::Crop => Dropped::Crop(crop_dropped(state, paths)),
    }
}
