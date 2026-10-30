//! Application entry point.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use pdfconv_worker::WORKER_FLAG;
use pdfconv_worker::system_memory;
use tauri::webview::PageLoadEvent;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

pub mod commands;
pub mod crop;
pub mod crop_export;
pub mod decode_budget;
pub mod error;
pub mod items;
pub mod jobs;
pub mod layout_files;
pub mod pdfium;
pub mod settings;
pub mod worker_pool;

use crop::{CropState, Dropped};
use decode_budget::{DecodeBudget, budget_for};
use items::ItemTable;
use settings::SettingsStore;
use worker_pool::{WorkerPool, WorkerPoolConfig};

/// Name of the event that reports what a drop added (design §7.2).
pub const ITEMS_DROPPED_EVENT: &str = "items-dropped";

/// Name of the event that reports what a drop in the crop tab opened (design §7.2).
pub const CROP_DROPPED_EVENT: &str = "crop-dropped";

/// State shared by the commands and the drop handler.
#[derive(Clone)]
pub struct AppState {
    /// The paths behind the IDs the frontend knows (design §1).
    pub items: Arc<ItemTable>,
    /// The PDF of the crop tab, kept apart from the lists (design §6.10).
    pub crop: Arc<CropState>,
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
    /// Output folder for crop exports (design §6.5).
    pub crop_output_dir: Arc<Mutex<Option<PathBuf>>>,
    /// The settings in memory and the file they are written to (design §6.7).
    pub settings: Arc<SettingsStore>,
    /// The memory that reading image pixels in this process may use at once (design §5.3).
    pub decode_budget: Arc<DecodeBudget>,
}

impl AppState {
    /// An empty state whose PDFs are opened by `pool`.
    pub fn new(pool: WorkerPool) -> Self {
        Self {
            items: Arc::new(ItemTable::new()),
            crop: Arc::new(CropState::default()),
            pool,
            is_running: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            images_output_dir: Arc::new(Mutex::new(None)),
            pdfs_output_dir: Arc::new(Mutex::new(None)),
            crop_output_dir: Arc::new(Mutex::new(None)),
            settings: Arc::new(SettingsStore::default()),
            decode_budget: Arc::new(DecodeBudget::new(budget_for(
                system_memory::installed_memory(),
            ))),
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
            let onnxruntime = layout_files::onnxruntime_file(app.handle())?;
            let model = layout_files::model_file(app.handle())?;
            let config = WorkerPoolConfig::new(
                exe,
                [
                    OsString::from(WORKER_FLAG),
                    library_dir.into_os_string(),
                    onnxruntime.into_os_string(),
                    model.into_os_string(),
                ],
            );
            let state = AppState::new(WorkerPool::new(config));
            // Without a settings folder the app still starts, with the
            // defaults, and keeps changes in memory (design §6.7).
            if let Ok(config_dir) = app.path().app_config_dir() {
                settings::restore_settings(&state, &config_dir);
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_about,
            commands::get_settings,
            commands::save_settings,
            commands::pick_output_dir,
            commands::add_images,
            commands::add_pdfs,
            commands::remove_items,
            commands::get_thumbnail,
            commands::check_page_range,
            commands::save_merged_pdf,
            commands::start_images_to_pdfs,
            commands::start_pdfs_to_images,
            commands::unlock_pdf,
            commands::open_crop_pdf,
            commands::activate_crop_pdf,
            commands::discard_crop_pdf,
            commands::unlock_crop_pdf,
            commands::set_drop_target,
            commands::list_page_images,
            commands::check_on_top,
            commands::detect_layout,
            commands::start_detect_all,
            commands::start_crop_export,
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
                    // The only way an emit fails is that the window is gone.
                    match crop::handle_drop(&state, &paths) {
                        Dropped::Items(dropped) => {
                            #[cfg(debug_assertions)]
                            eprintln!("[debug] DragDrop: {}", describe_drop(&dropped));
                            let _ = window.emit(ITEMS_DROPPED_EVENT, &dropped);
                        }
                        Dropped::Crop(dropped) => {
                            let _ = window.emit(CROP_DROPPED_EVENT, &dropped);
                        }
                    }
                });
            }
        })
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                let state = webview.state::<AppState>();
                state.settings.start_page();
                state.crop.start_page();
            }
        })
        .build(tauri::generate_context!())
        .expect("Tauri application should start")
        .run(|app, event| {
            // `Exit` rather than `CloseRequested`, because a Windows logoff
            // or shutdown sends only `Exit` (design §6.7). A failed write is
            // dropped as in `save_settings`.
            if let tauri::RunEvent::Exit = event {
                let _ = app.state::<AppState>().settings.flush();
            }
        });
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
