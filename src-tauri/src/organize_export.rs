//! Making the outputs of an organize and saving them (design organize §3, §4, §6; design §6.5).
//!
//! Everything that can be refused is refused before anything is made, and every name is decided
//! then, so a request is taken or refused as a whole.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use pdfconv_core::naming::{resolve_organize_names, split_stem_and_ext};
use pdfconv_worker::client::{StreamPart, WorkerError};
use pdfconv_worker::protocol::{OrganizePage, OrganizeSource, Password, Request, Response};
use tempfile::NamedTempFile;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::items::{file_name, unexpected_response};
use crate::jobs::{
    self, JobCallbacks, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    SaveMergedPdfResult,
};
use crate::organize::{
    OrganizePageInput, Source, check_output_password, check_parts, check_sizes, ids_of,
};
use crate::settings::OutputKind;
use crate::worker_pool::WorkerPoolError;

/// One output, ready to be made.
#[derive(Debug, Clone)]
struct OutputPlan {
    /// The PDFs the output takes pages from, in the order they first appear in it.
    sources: Vec<OrganizeSource>,
    /// The organize ID of each of `sources`.
    source_ids: Vec<u64>,
    /// The pages, in output order.
    pages: Vec<OrganizePage>,
    /// The name the output is saved as, and the `current` of its progress.
    name: String,
}

impl OutputPlan {
    /// The output made of `part`, whose PDFs `check_parts` has found in `sources`.
    fn new(part: &[OrganizePageInput], sources: &HashMap<u64, Source>, name: String) -> Self {
        let mut plan = Self {
            sources: Vec::new(),
            source_ids: Vec::new(),
            pages: Vec::with_capacity(part.len()),
            name,
        };
        for page in part {
            let index = match plan.source_ids.iter().position(|id| *id == page.id) {
                Some(index) => index,
                None => {
                    let source = sources
                        .get(&page.id)
                        .expect("check_parts has found every PDF of the pages");
                    plan.sources.push(OrganizeSource {
                        path: source.path.clone(),
                        password: source.password.clone(),
                    });
                    plan.source_ids.push(page.id);
                    plan.source_ids.len() - 1
                }
            };
            plan.pages.push(OrganizePage {
                source: u32::try_from(index).unwrap_or(u32::MAX),
                page: page.page - 1,
                rotation: page.rotation,
            });
        }
        plan
    }

    fn page_count(&self) -> u32 {
        u32::try_from(self.pages.len()).unwrap_or(u32::MAX)
    }
}

/// Why an output was not made.
#[derive(Debug)]
pub enum OutputStop {
    /// The user cancelled.
    Cancelled,
    /// The output failed.
    Failed {
        error: IpcError,
        /// The organize PDF behind the failure, when the worker could not use one of them.
        source_id: Option<u64>,
        /// Whether the failure was in writing the file here. The folder is the same for every
        /// output, so none of the rest is tried (design §6.5).
        writing: bool,
    },
}

impl OutputStop {
    fn failed(error: IpcError) -> Self {
        Self::Failed {
            error,
            source_id: None,
            writing: false,
        }
    }

    fn writing(error: IpcError) -> Self {
        Self::Failed {
            error,
            source_id: None,
            writing: true,
        }
    }
}

fn write_failure(error: &std::io::Error) -> IpcError {
    IpcError::new(ErrorCode::WriteFailed, error.to_string())
}

/// The producer written into the document information of every output.
fn creator() -> String {
    format!("PDF Converter {}", env!("CARGO_PKG_VERSION"))
}

/// Builds the output `plan` in a pooled worker and writes what it sends to `sink` as it arrives
/// (design organize §3, §6). `on_pages_done` gets the number of pages done in this output.
///
/// A write to `sink` that fails stops the worker, because nothing it would send can be kept.
///
/// # Errors
///
/// [`OutputStop::Cancelled`] when `cancel_flag` is set, and [`OutputStop::Failed`] with the error
/// of the worker (`WorkerCrashed`, `WorkerTimeout`, a source PDF it could not use with its
/// `source_id`, or its own error code), of the bytes not adding up (`WorkerCrashed`), or of
/// `sink` (`WriteFailed`).
fn write_output(
    state: &AppState,
    plan: &OutputPlan,
    output_password: Option<&Password>,
    sink: &mut dyn Write,
    on_pages_done: &dyn Fn(u32),
) -> Result<(), OutputStop> {
    let mut worker = state
        .pool
        .acquire()
        .map_err(|error| OutputStop::failed(error.into()))?;
    let request = Request::Organize {
        sources: plan.sources.clone(),
        pages: plan.pages.clone(),
        creator: creator(),
        output_password: output_password.cloned(),
    };
    let step_timeout = state.pool.timeout_for(&request);

    let write_error: RefCell<Option<IpcError>> = RefCell::new(None);
    let mut written: u64 = 0;
    let should_cancel =
        || state.cancel_flag.load(Ordering::SeqCst) || write_error.borrow().is_some();
    let mut on_part = |part: StreamPart| match part {
        StreamPart::Progress { done } => on_pages_done(done),
        StreamPart::Chunk(bytes) => {
            if write_error.borrow().is_some() {
                return;
            }
            match sink.write_all(&bytes) {
                Ok(()) => written += u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                Err(error) => *write_error.borrow_mut() = Some(write_failure(&error)),
            }
        }
    };
    let answer = worker.request_stream(&request, step_timeout, &should_cancel, &mut on_part);
    // A dead worker (crashed, timed out, cancelled) is not given back to the pool when this
    // guard goes.
    drop(worker);

    match answer {
        Ok(Response::Organized { bytes }) if bytes == written => sink
            .flush()
            .map_err(|error| OutputStop::writing(write_failure(&error))),
        Ok(Response::Organized { bytes }) => Err(OutputStop::failed(IpcError::new(
            ErrorCode::WorkerCrashed,
            format!("the worker announced {bytes} bytes and {written} were written"),
        ))),
        Ok(_) => Err(OutputStop::failed(unexpected_response())),
        Err(WorkerError::Cancelled) => match write_error.take() {
            Some(error) => Err(OutputStop::writing(error)),
            None => Err(OutputStop::Cancelled),
        },
        Err(WorkerError::SourceFailed {
            code,
            detail,
            source,
        }) => Err(OutputStop::Failed {
            error: IpcError::from_worker(&code, detail.as_deref()),
            source_id: usize::try_from(source)
                .ok()
                .and_then(|index| plan.source_ids.get(index))
                .copied(),
            writing: false,
        }),
        Err(error) => Err(OutputStop::failed(WorkerPoolError::from(error).into())),
    }
}

/// Builds the output in a temporary file in `output_dir` and returns it finished, to be given
/// its name (design §6.5). The file goes when it is dropped, so a failure or a cancel leaves
/// nothing behind.
fn make_output_file(
    state: &AppState,
    plan: &OutputPlan,
    output_password: Option<&Password>,
    output_dir: &Path,
    on_pages_done: &dyn Fn(u32),
) -> Result<NamedTempFile, OutputStop> {
    let mut temp = jobs::create_output_temp(output_dir)
        .map_err(|error| OutputStop::writing(write_failure(&error)))?;
    write_output(state, plan, output_password, &mut temp, on_pages_done)?;
    Ok(temp)
}

/// Runs the checks that come before an organize is started (design organize §3, §4). Returns the
/// password the worker takes and the PDFs the pages come from.
fn check(
    state: &AppState,
    parts: &[&[OrganizePageInput]],
    password: Option<String>,
) -> Result<(Option<Password>, HashMap<u64, Source>), IpcError> {
    let password = check_output_password(password)?;
    let sources = state.organize.sources(&ids_of(parts));
    check_parts(parts, &sources)?;
    check_sizes(parts, &sources)?;
    Ok((password, sources))
}

/// A single PDF to save: checked, and ready to be made.
pub struct SingleOutput {
    plan: OutputPlan,
    password: Option<Password>,
    default_name: String,
}

impl SingleOutput {
    /// The file name the save dialog starts with: the name of the PDF of the first page, as a
    /// `.pdf` (design organize §4).
    pub fn default_name(&self) -> &str {
        &self.default_name
    }

    /// Builds the output and writes it to `sink`, to try a failing write (design organize §6).
    ///
    /// # Errors
    ///
    /// As the building of an output: see [`OutputStop`].
    pub fn write_to(&self, state: &AppState, sink: &mut dyn Write) -> Result<(), OutputStop> {
        write_output(state, &self.plan, self.password.as_ref(), sink, &|_| {})
    }
}

/// Marks a conversion as running and checks the pages and the password of a single PDF, before
/// the save dialog opens (design organize §3, §4). The guard puts `is_running` back when it is
/// dropped; a refusal drops it, so nothing is left running.
///
/// # Errors
///
/// `ConversionRunning` during a conversion. `InvalidParams` for no page, a `rotation` other than
/// 0, 90, 180 or 270, a page that its PDF does not have, the same page twice, more pages than
/// `MAX_PDF_PAGES`, and a password that breaks the rule of design §4.3.1. `UnknownHandle` for a PDF
/// that is not in the organize list, the error of a PDF that could not be opened, and
/// `OutputTooLarge` when the estimate is over `MAX_ORGANIZE_OUTPUT_BYTES`.
pub fn begin_single(
    state: &AppState,
    pages: &[OrganizePageInput],
    password: Option<String>,
) -> Result<(jobs::RunningGuard, SingleOutput), IpcError> {
    let guard = jobs::begin_conversion(state)?;
    let (password, sources) = check(state, &[pages], password)?;
    let first = pages
        .first()
        .and_then(|page| sources.get(&page.id))
        .expect("check_parts has found that there is a first page and its PDF");
    let (stem, _) = split_stem_and_ext(&first.name);
    let default_name = format!("{stem}.pdf");
    let output = SingleOutput {
        plan: OutputPlan::new(pages, &sources, default_name.clone()),
        password,
        default_name,
    };
    Ok((guard, output))
}

/// Makes the single PDF and saves it as `dest_path`, which the user picked in the save dialog
/// and may be one of the sources: the output is written in the folder first and put in place
/// when it is done (design organize §4). The result is reported like any job: one
/// `job-item`, then `job-finished`.
///
/// # Errors
///
/// `WriteFailed` when `dest_path` has no folder. A PDF that is not made is not an error: it is
/// the `job-item`, and `saved_name` is `None`.
pub fn run_save_organized_pdf<FProg, FItem, FFin>(
    state: &AppState,
    output: &SingleOutput,
    dest_path: &Path,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<SaveMergedPdfResult, IpcError>
where
    FProg: Fn(JobProgressPayload),
    FItem: Fn(JobItemPayload),
    FFin: FnOnce(JobFinishedPayload),
{
    let JobCallbacks {
        on_progress,
        on_item,
        on_finished,
    } = callbacks;
    let output_dir = dest_path
        .parent()
        .ok_or_else(|| IpcError::from_code(ErrorCode::WriteFailed))?;
    let saved_name = file_name(dest_path);
    let total = output.plan.page_count();
    let progress = |done: u32| {
        on_progress(JobProgressPayload {
            done,
            total,
            current: Some(saved_name.clone()),
        });
    };
    progress(0);

    let made = if state.cancel_flag.load(Ordering::SeqCst) {
        Err(OutputStop::Cancelled)
    } else {
        make_output_file(
            state,
            &output.plan,
            output.password.as_ref(),
            output_dir,
            &progress,
        )
        .and_then(|temp| {
            temp.persist(dest_path)
                .map(drop)
                .map_err(|error| OutputStop::writing(write_failure(&error.error)))
        })
    };
    match made {
        Ok(()) => {
            on_item(JobItemPayload {
                id: 1,
                status: JobItemStatus::Ok,
                outputs: vec![saved_name.clone()],
                error: None,
                failed_pages: None,
                source_id: None,
            });
            on_finished(finished(1, 0, 0, false));
            Ok(SaveMergedPdfResult {
                saved_name: Some(saved_name),
            })
        }
        Err(OutputStop::Cancelled) => {
            on_finished(finished(0, 0, 1, true));
            Ok(SaveMergedPdfResult { saved_name: None })
        }
        Err(OutputStop::Failed {
            error, source_id, ..
        }) => {
            on_item(failed_item(1, error, source_id));
            on_finished(finished(0, 1, 0, false));
            Ok(SaveMergedPdfResult { saved_name: None })
        }
    }
}

fn finished(succeeded: u32, failed: u32, unprocessed: u32, cancelled: bool) -> JobFinishedPayload {
    JobFinishedPayload {
        succeeded,
        failed,
        no_pages: 0,
        unprocessed,
        cancelled,
    }
}

fn failed_item(id: u64, error: IpcError, source_id: Option<u64>) -> JobItemPayload {
    JobItemPayload {
        id,
        status: JobItemStatus::Failed,
        outputs: Vec::new(),
        error: Some(error),
        failed_pages: None,
        source_id,
    }
}

/// The outputs of an organize that is saved in parts, everything decided.
struct SplitPlan {
    output_dir: PathBuf,
    outputs: Vec<OutputPlan>,
    password: Option<Password>,
    /// The names in the folder and the planned ones, in lower case, for `persist_unique` to pick
    /// numbers from when a name is taken in the meantime.
    taken_lower: HashSet<String>,
}

/// Starts making `parts`, one output each, in the background as a conversion (design organize
/// §4, §6; design §6.5, §7.1). The outputs are made one at a time, in the order of `parts`; each
/// gets a `job-item` whose `id` is its number from 1.
///
/// # Errors
///
/// Nothing starts, and `is_running` is as it was, with:
///
/// - `ConversionRunning` during a conversion;
/// - `InvalidParams` for what [`begin_single`] refuses in the pages of one part, and for no part or
///   an empty part, a page used in two parts, and when the output folder of organize is not
///   chosen;
/// - `UnknownHandle`, and the error of a PDF that could not be opened;
/// - `OutputTooLarge` for the first output over the limit;
/// - `OutputDirMissing` and `OutputDirNotWritable` as [`jobs::check_output_dir`], and
///   `WriteFailed` if the folder cannot be read.
pub fn start_organize_split_internal<FProg, FItem, FFin>(
    state: &AppState,
    parts: &[Vec<OrganizePageInput>],
    password: Option<String>,
    callbacks: JobCallbacks<FProg, FItem, FFin>,
) -> Result<(), IpcError>
where
    FProg: Fn(JobProgressPayload) + Send + Sync + 'static,
    FItem: Fn(JobItemPayload) + Send + Sync + 'static,
    FFin: FnOnce(JobFinishedPayload) + Send + Sync + 'static,
{
    let guard = jobs::begin_conversion(state)?;
    let slices: Vec<&[OrganizePageInput]> = parts.iter().map(Vec::as_slice).collect();
    let (password, sources) = check(state, &slices, password)?;

    let output_dir = state
        .organize_output_dir
        .lock()
        .expect("output_dir lock")
        .clone()
        .ok_or_else(|| IpcError::from_code(ErrorCode::InvalidParams))?;
    jobs::check_output_dir(state, OutputKind::Organize, &output_dir)?;
    let existing = jobs::list_existing_files(&output_dir)?;

    let stems: Vec<String> = slices
        .iter()
        .map(|part| {
            let first = part
                .first()
                .and_then(|page| sources.get(&page.id))
                .expect("check_parts has found that every part has a first page and its PDF");
            split_stem_and_ext(&first.name).0.to_owned()
        })
        .collect();
    let names = resolve_organize_names(&stems, &existing);
    let taken_lower = existing
        .iter()
        .chain(names.iter())
        .map(|name| name.to_lowercase())
        .collect();
    let outputs = slices
        .iter()
        .zip(names)
        .map(|(part, name)| OutputPlan::new(part, &sources, name))
        .collect();
    let plan = SplitPlan {
        output_dir,
        outputs,
        password,
        taken_lower,
    };

    let JobCallbacks {
        on_progress,
        on_item,
        on_finished,
    } = callbacks;
    let failed_if_it_panics = u32::try_from(parts.len()).unwrap_or(u32::MAX);
    jobs::spawn_conversion(
        state,
        guard,
        on_finished,
        failed_if_it_panics,
        move |state, finish| {
            let finished = run_split(state, &plan, &on_progress, &on_item);
            finish(finished);
            Ok(())
        },
    );
    Ok(())
}

/// Makes the outputs of `plan` and reports them. Returns the end of the job.
fn run_split(
    state: &AppState,
    plan: &SplitPlan,
    on_progress: &dyn Fn(JobProgressPayload),
    on_item: &dyn Fn(JobItemPayload),
) -> JobFinishedPayload {
    let total: u32 = plan.outputs.iter().map(OutputPlan::page_count).sum();
    let progress = |done: u32, name: &str| {
        on_progress(JobProgressPayload {
            done,
            total,
            current: Some(name.to_owned()),
        });
    };
    let taken_lower = Mutex::new(plan.taken_lower.clone());
    // The PDFs a source failure was met on, and the failure, so that no later output is tried
    // with one (design organize §6).
    let mut failed_sources: Vec<(u64, IpcError)> = Vec::new();
    let (mut succeeded, mut failed, mut reached, mut cancelled) = (0u32, 0u32, 0u32, false);
    let mut pages_before = 0u32;

    if let Some(first) = plan.outputs.first() {
        progress(0, &first.name);
    }
    for (index, output) in plan.outputs.iter().enumerate() {
        if state.cancel_flag.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        let number = u64::try_from(index + 1).unwrap_or(u64::MAX);
        let known_failure = output.source_ids.iter().find_map(|id| {
            failed_sources
                .iter()
                .find(|(failed_id, _)| failed_id == id)
                .map(|(failed_id, error)| (*failed_id, error.clone()))
        });
        let made = match known_failure {
            Some((source_id, error)) => Err(OutputStop::Failed {
                error,
                source_id: Some(source_id),
                writing: false,
            }),
            None => {
                let base = pages_before;
                make_output_file(
                    state,
                    output,
                    plan.password.as_ref(),
                    &plan.output_dir,
                    &|done| progress(base.saturating_add(done), &output.name),
                )
                .and_then(|temp| {
                    jobs::persist_unique(temp, &plan.output_dir, &output.name, &taken_lower)
                        .map_err(OutputStop::writing)
                })
            }
        };
        let ends_job = match made {
            Ok(saved_name) => {
                succeeded += 1;
                on_item(JobItemPayload {
                    id: number,
                    status: JobItemStatus::Ok,
                    outputs: vec![saved_name],
                    error: None,
                    failed_pages: None,
                    source_id: None,
                });
                false
            }
            Err(OutputStop::Cancelled) => {
                cancelled = true;
                break;
            }
            Err(OutputStop::Failed {
                error,
                source_id,
                writing,
            }) => {
                if let Some(id) = source_id
                    && !failed_sources.iter().any(|(failed_id, _)| *failed_id == id)
                {
                    failed_sources.push((id, error.clone()));
                }
                failed += 1;
                on_item(failed_item(number, error, source_id));
                writing
            }
        };
        reached += 1;
        if ends_job {
            break;
        }
        pages_before = pages_before.saturating_add(output.page_count());
        progress(pages_before, &output.name);
    }

    let outputs = u32::try_from(plan.outputs.len()).unwrap_or(u32::MAX);
    finished(succeeded, failed, outputs - reached, cancelled)
}
