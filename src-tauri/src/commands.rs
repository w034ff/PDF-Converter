//! IPC commands (design §7.1).

use serde::Serialize;
use tauri::AppHandle;

use pdfconv_worker::WORKER_FLAG;
use pdfconv_worker::client::{OPEN_TIMEOUT, WorkerError, WorkerProcess};
use pdfconv_worker::protocol::Request;

/// Answer of [`get_about`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub version: String,
    pub pdfium_version: String,
    /// Whether a worker started and could use pdfium. Until the about
    /// screen (T13) exists this is how an installed build shows that the
    /// bundled pdfium works.
    pub pdfium_ready: bool,
    pub pdfium_error: Option<String>,
}

/// Returns the app and pdfium versions after checking that a worker can
/// start and load pdfium.
#[tauri::command]
pub async fn get_about(app: AppHandle) -> AboutInfo {
    let version = app.package_info().version.to_string();
    let check = tauri::async_runtime::spawn_blocking(move || check_worker(&app)).await;
    let pdfium_error = match check {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e),
        Err(e) => Some(e.to_string()),
    };
    AboutInfo {
        version,
        pdfium_version: crate::pdfium::release(),
        pdfium_ready: pdfium_error.is_none(),
        pdfium_error,
    }
}

fn check_worker(app: &AppHandle) -> Result<(), String> {
    let library_dir = crate::pdfium::library_dir(app).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut worker = WorkerProcess::spawn(
        exe.as_os_str(),
        [WORKER_FLAG.as_ref(), library_dir.as_os_str()],
    )
    .map_err(|e| format!("{e:?}"))?;
    match worker.request(&Request::Hello, OPEN_TIMEOUT) {
        Ok(_) => Ok(()),
        Err(WorkerError::Remote { code, detail }) => {
            Err(format!("{code}: {}", detail.unwrap_or_default()))
        }
        Err(e) => Err(format!("{e:?}")),
    }
}
