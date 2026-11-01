//! The check of an image's pixels in a worker, before the image is written into a PDF
//! (design §6.2).

use std::path::Path;

use pdfconv_worker::client::WorkerError;
use pdfconv_worker::protocol::{Request, Response};

use crate::error::IpcError;
use crate::items::unexpected_response;
use crate::worker_pool::{WorkerPool, WorkerPoolError};

/// Why the check of an image did not pass.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ImageCheckStop {
    /// The conversion was cancelled while the worker was reading the image.
    Cancelled,
    /// The image failed with this error, and the conversion goes on with the next image.
    Failed(IpcError),
}

/// Has a worker read every pixel of the image at `path`, so that a decoder that stops or
/// crashes is contained by the worker's limits (design §5.3, §6.2).
///
/// The worker is borrowed for this image only. One that crashed, timed out or was stopped by
/// `should_cancel` is not given back to the pool.
///
/// # Errors
///
/// [`ImageCheckStop::Failed`] with the code the worker answered (`DecodeFailed` and the like),
/// `WorkerCrashed` or `WorkerTimeout`; [`ImageCheckStop::Cancelled`] when `should_cancel` returned
/// true while waiting for the worker's answer.
pub(super) fn check_image(
    pool: &WorkerPool,
    path: &Path,
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), ImageCheckStop> {
    let mut worker = pool
        .acquire()
        .map_err(|error| ImageCheckStop::Failed(error.into()))?;
    let request = Request::CheckImage {
        path: path.to_path_buf(),
    };
    let step_timeout = pool.timeout_for(&request);
    let answer = worker.request_stream(&request, step_timeout, should_cancel, &mut |_| {});
    drop(worker);

    match answer {
        Ok(Response::ImageChecked) => Ok(()),
        Ok(_) | Err(WorkerError::SourceFailed { .. }) => {
            Err(ImageCheckStop::Failed(unexpected_response()))
        }
        Err(WorkerError::Cancelled) => Err(ImageCheckStop::Cancelled),
        Err(error) => Err(ImageCheckStop::Failed(WorkerPoolError::from(error).into())),
    }
}
