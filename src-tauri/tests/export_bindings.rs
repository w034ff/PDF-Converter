//! Writes the TypeScript types of the IPC to `src/ipc/generated/`. CI runs
//! this test and fails if the committed files differ from what it writes.

use std::path::Path;

use pdf_converter_lib::commands::{AboutInfo, AddSource};
use pdf_converter_lib::error::{ErrorCode, IpcError};
use pdf_converter_lib::items::{
    AddResult, ImageFormatName, ImageItem, ItemsDropped, PageSizePt, PdfItem, Skipped,
};
use pdf_converter_lib::jobs::{
    CheckPageRangeResult, JobFinishedPayload, JobItemPayload, JobItemStatus, JobProgressPayload,
    PageSizeChoice, RenderFormatChoice, SaveMergedPdfResult,
};
use ts_rs::{Config, TS};

#[test]
fn export_typescript_bindings() {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/generated");
    std::fs::create_dir_all(&out_dir).expect("creating the export directory");
    let cfg = Config::new().with_out_dir(&out_dir);

    ErrorCode::export_all(&cfg).expect("exporting ErrorCode");
    IpcError::export_all(&cfg).expect("exporting IpcError");
    AboutInfo::export_all(&cfg).expect("exporting AboutInfo");
    AddSource::export_all(&cfg).expect("exporting AddSource");
    ImageFormatName::export_all(&cfg).expect("exporting ImageFormatName");
    PageSizePt::export_all(&cfg).expect("exporting PageSizePt");
    ImageItem::export_all(&cfg).expect("exporting ImageItem");
    PdfItem::export_all(&cfg).expect("exporting PdfItem");
    Skipped::export_all(&cfg).expect("exporting Skipped");
    AddResult::<ImageItem>::export_all(&cfg).expect("exporting AddResult");
    ItemsDropped::export_all(&cfg).expect("exporting ItemsDropped");
    PageSizeChoice::export_all(&cfg).expect("exporting PageSizeChoice");
    RenderFormatChoice::export_all(&cfg).expect("exporting RenderFormatChoice");
    JobItemStatus::export_all(&cfg).expect("exporting JobItemStatus");
    JobProgressPayload::export_all(&cfg).expect("exporting JobProgressPayload");
    JobItemPayload::export_all(&cfg).expect("exporting JobItemPayload");
    JobFinishedPayload::export_all(&cfg).expect("exporting JobFinishedPayload");
    CheckPageRangeResult::export_all(&cfg).expect("exporting CheckPageRangeResult");
    SaveMergedPdfResult::export_all(&cfg).expect("exporting SaveMergedPdfResult");
}
