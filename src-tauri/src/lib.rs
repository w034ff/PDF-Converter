//! Application entry point.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use pdfconv_worker::WORKER_FLAG;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

pub mod commands;
pub mod error;
pub mod items;
pub mod jobs;
pub mod pdfium;
pub mod worker_pool;

use items::ItemTable;
use worker_pool::{WorkerPool, WorkerPoolConfig};

/// Name of the event that reports what a drop added (design §7.2).
pub const ITEMS_DROPPED_EVENT: &str = "items-dropped";

/// State shared by the commands and the drop handler.
#[derive(Clone)]
pub struct AppState {
    /// The paths behind the IDs the frontend knows (design §1).
    pub items: Arc<ItemTable>,
    /// The worker processes that open PDFs (design §5.2).
    pub pool: WorkerPool,
    /// Flag indicating whether a conversion job is currently executing (design §6.5, §7.1).
    pub is_running: Arc<AtomicBool>,
    /// Shared cancellation flag checked during conversion jobs (design §6.5).
    pub cancel_flag: Arc<AtomicBool>,
    /// Output folder for images to PDF conversion (design §6.5).
    pub images_output_dir: Arc<Mutex<Option<PathBuf>>>,
    /// Output folder for PDF to images conversion (design §6.5).
    pub pdfs_output_dir: Arc<Mutex<Option<PathBuf>>>,
}

impl AppState {
    /// An empty state whose PDFs are opened by `pool`.
    pub fn new(pool: WorkerPool) -> Self {
        Self {
            items: Arc::new(ItemTable::new()),
            pool,
            is_running: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            images_output_dir: Arc::new(Mutex::new(None)),
            pdfs_output_dir: Arc::new(Mutex::new(None)),
        }
    }
}

/// Starts the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to start, which leaves no window to report the error in.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let exe = std::env::current_exe()?;
            let library_dir = pdfium::library_dir(app.handle())?;
            let config = WorkerPoolConfig::new(
                exe,
                [OsString::from(WORKER_FLAG), library_dir.into_os_string()],
            );
            app.manage(AppState::new(WorkerPool::new(config)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_about,
            commands::add_images,
            commands::add_pdfs,
            commands::remove_items,
            commands::get_thumbnail,
            commands::check_page_range,
            commands::save_merged_pdf,
            commands::start_images_to_pdfs,
            commands::start_pdfs_to_images,
            commands::cancel_job,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let state = window.state::<AppState>().inner().clone();
                let paths = paths.clone();
                let window = window.clone();
                // Probing and opening PDFs take long enough to freeze the
                // window if they ran in the event handler.
                tauri::async_runtime::spawn_blocking(move || {
                    if state.is_running.load(std::sync::atomic::Ordering::SeqCst) {
                        let dropped = items::ItemsDropped {
                            images: Vec::new(),
                            pdfs: Vec::new(),
                            skipped: items::Skipped::default(),
                            error: Some(crate::error::IpcError::from_code(
                                crate::error::ErrorCode::ConversionRunning,
                            )),
                        };
                        let _ = window.emit(ITEMS_DROPPED_EVENT, &dropped);
                        return;
                    }
                    let dropped = items::add_dropped(&state, &paths);
                    #[cfg(debug_assertions)]
                    eprintln!("[debug] DragDrop: {}", describe_drop(&dropped));
                    // The only way this fails is that the window is gone.
                    let _ = window.emit(ITEMS_DROPPED_EVENT, &dropped);
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("Tauri application should start");
}

/// How a drop was sorted, for the terminal of a debug build.
#[cfg(debug_assertions)]
fn describe_drop(dropped: &items::ItemsDropped) -> String {
    let images: Vec<&str> = dropped.images.iter().map(|i| i.name.as_str()).collect();
    let pdfs: Vec<&str> = dropped.pdfs.iter().map(|p| p.name.as_str()).collect();
    format!(
        "images={} {images:?}, pdfs={} {pdfs:?}, skipped={:?}",
        images.len(),
        pdfs.len(),
        dropped.skipped,
    )
}
