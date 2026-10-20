import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isPageSizeChoice } from "../state/imagesToPdf";
import { RENDER_DPI_MAX, RENDER_DPI_MIN } from "../state/pdfToImages";
import type { AddResult } from "./generated/AddResult";
import type { AddSource } from "./generated/AddSource";
import type { Candidate } from "./generated/Candidate";
import type { CheckPageRangeResult } from "./generated/CheckPageRangeResult";
import type { CropDropped } from "./generated/CropDropped";
import type { CropPdf } from "./generated/CropPdf";
import type { DropTarget } from "./generated/DropTarget";
import type { ErrorCode } from "./generated/ErrorCode";
import type { ImageItem } from "./generated/ImageItem";
import type { IpcError } from "./generated/IpcError";
import type { ItemsDropped } from "./generated/ItemsDropped";
import type { JobFinishedPayload } from "./generated/JobFinishedPayload";
import type { JobItemPayload } from "./generated/JobItemPayload";
import type { JobProgressPayload } from "./generated/JobProgressPayload";
import type { OutputDirLabel } from "./generated/OutputDirLabel";
import type { OutputKind } from "./generated/OutputKind";
import type { PageImage } from "./generated/PageImage";
import type { PageOrientationChoice } from "./generated/PageOrientationChoice";
import type { PageSizeChoice } from "./generated/PageSizeChoice";
import type { PageSizePt } from "./generated/PageSizePt";
import type { PdfItem } from "./generated/PdfItem";
import type { RenderColorChoice } from "./generated/RenderColorChoice";
import type { RenderFormatChoice } from "./generated/RenderFormatChoice";
import type { SaveMergedPdfResult } from "./generated/SaveMergedPdfResult";
import type { Settings } from "./generated/Settings";
import type { SettingsInput } from "./generated/SettingsInput";
import type { UnlockResult } from "./generated/UnlockResult";

export type { AboutInfo } from "./generated/AboutInfo";
export type { AddResult } from "./generated/AddResult";
export type { AddSource } from "./generated/AddSource";
export type { Candidate } from "./generated/Candidate";
export type { CandidateKind } from "./generated/CandidateKind";
export type { CheckPageRangeResult } from "./generated/CheckPageRangeResult";
export type { CropDropped } from "./generated/CropDropped";
export type { CropPdf } from "./generated/CropPdf";
export type { DropTarget } from "./generated/DropTarget";
export type { ErrorCode } from "./generated/ErrorCode";
export type { ImageFormatName } from "./generated/ImageFormatName";
export type { ImageItem } from "./generated/ImageItem";
export type { ImagesToPdfOptions } from "./generated/ImagesToPdfOptions";
export type { ImagesToPdfSettings } from "./generated/ImagesToPdfSettings";
export type { IpcError } from "./generated/IpcError";
export type { ItemsDropped } from "./generated/ItemsDropped";
export type { JobFinishedPayload } from "./generated/JobFinishedPayload";
export type { JobItemPayload } from "./generated/JobItemPayload";
export type { JobItemStatus } from "./generated/JobItemStatus";
export type { JobProgressPayload } from "./generated/JobProgressPayload";
export type { Language } from "./generated/Language";
export type { OutputDirLabel } from "./generated/OutputDirLabel";
export type { OutputKind } from "./generated/OutputKind";
export type { OutputMode } from "./generated/OutputMode";
export type { PageImage } from "./generated/PageImage";
export type { PageOrientationChoice } from "./generated/PageOrientationChoice";
export type { PageSizeChoice } from "./generated/PageSizeChoice";
export type { PageSizePt } from "./generated/PageSizePt";
export type { PdfItem } from "./generated/PdfItem";
export type { PdfToImagesOptions } from "./generated/PdfToImagesOptions";
export type { PdfToImagesSettings } from "./generated/PdfToImagesSettings";
export type { RenderColorChoice } from "./generated/RenderColorChoice";
export type { RenderFormatChoice } from "./generated/RenderFormatChoice";
export type { SaveMergedPdfResult } from "./generated/SaveMergedPdfResult";
export type { Settings } from "./generated/Settings";
export type { SettingsInput } from "./generated/SettingsInput";
export type { Skipped } from "./generated/Skipped";
export type { UnlockResult } from "./generated/UnlockResult";
export type { UnlistenFn };

/** Name of the event Rust emits after a drop (design §7.2). */
export const ITEMS_DROPPED_EVENT = "items-dropped";

/** Name of the event Rust emits after a drop in the crop tab (design §7.2). */
export const CROP_DROPPED_EVENT = "crop-dropped";

/** Name of the event Rust emits during conversion progress (design §7.2). */
export const JOB_PROGRESS_EVENT = "job-progress";

/** Name of the event Rust emits when an item completes (design §7.2). */
export const JOB_ITEM_EVENT = "job-item";

/** Name of the event Rust emits when a job finishes (design §7.2). */
export const JOB_FINISHED_EVENT = "job-finished";

const ERROR_CODES = {
  UnsupportedFormat: true,
  DecodeFailed: true,
  PdfOpenFailed: true,
  PasswordProtected: true,
  TooLarge: true,
  TooManyPages: true,
  RenderTooLarge: true,
  WorkerCrashed: true,
  WorkerTimeout: true,
  ReadFailed: true,
  WriteFailed: true,
  InvalidPageRange: true,
  ConversionRunning: true,
  UnknownHandle: true,
  InvalidParams: true,
  OutputDirMissing: true,
  OutputDirNotWritable: true,
  CmykNotAllowedInPdfA: true,
  LayoutUnavailable: true,
} satisfies Record<ErrorCode, true>;

/** Whether a value is one of the error codes of design §6.6. */
export function isErrorCode(value: unknown): value is ErrorCode {
  return (
    typeof value === "string" &&
    Object.prototype.hasOwnProperty.call(ERROR_CODES, value)
  );
}

/** Whether a value has the shape `{ code, detail }` of design §6.6. */
export function isIpcError(value: unknown): value is IpcError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  if (!("code" in value) || !("detail" in value)) {
    return false;
  }
  const { code, detail } = value;
  return isErrorCode(code) && (typeof detail === "string" || detail === null);
}

/**
 * Turns anything an IPC call rejected with into `{ code, detail }`. Tauri
 * rejects with a plain string when it cannot read the arguments, which design
 * §6.6 reports as `InvalidParams`.
 */
export function normalizeIpcError(error: unknown): IpcError {
  if (isIpcError(error)) {
    return error;
  }
  let detail: string;
  if (typeof error === "string") {
    detail = error;
  } else if (error instanceof Error) {
    detail = error.message;
  } else if (typeof error === "object" && error !== null) {
    try {
      detail = JSON.stringify(error);
    } catch {
      detail = String(error);
    }
  } else {
    detail = String(error);
  }
  return { code: "InvalidParams", detail };
}

async function invokeWrapped<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isOutputDirLabel(value: unknown): value is OutputDirLabel | null {
  return (
    value === null || (isRecord(value) && typeof value.dirLabel === "string")
  );
}

export function isPageOrientationChoice(
  value: unknown,
): value is PageOrientationChoice {
  return value === "auto" || value === "portrait" || value === "landscape";
}

export function isPageSizePt(value: unknown): value is PageSizePt {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.widthPt === "number" &&
    Number.isFinite(value.widthPt) &&
    typeof value.heightPt === "number" &&
    Number.isFinite(value.heightPt)
  );
}

export function isPdfItem(value: unknown): value is PdfItem {
  if (!isRecord(value)) {
    return false;
  }
  const { id, name, pageCount, firstPageSizePt, bytes, error } = value;
  return (
    typeof id === "number" &&
    Number.isInteger(id) &&
    typeof name === "string" &&
    typeof pageCount === "number" &&
    Number.isInteger(pageCount) &&
    (firstPageSizePt === null || isPageSizePt(firstPageSizePt)) &&
    typeof bytes === "number" &&
    Number.isInteger(bytes) &&
    (error === null || isIpcError(error))
  );
}

export function isUnlockResult(value: unknown): value is UnlockResult {
  if (!isRecord(value)) {
    return false;
  }
  const { items } = value;
  return Array.isArray(items) && items.every(isPdfItem);
}

function isRenderFormatChoice(value: unknown): value is RenderFormatChoice {
  return value === "png" || value === "jpeg" || value === "webp";
}

function isRenderColorChoice(value: unknown): value is RenderColorChoice {
  return value === "color" || value === "gray";
}

function isRenderDpi(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= RENDER_DPI_MIN &&
    value <= RENDER_DPI_MAX
  );
}

/** Whether a value has the shape of the `get_settings` answer (design §6.7). */
export function isSettings(value: unknown): value is Settings {
  if (!isRecord(value)) {
    return false;
  }
  const { language, imagesToPdf, pdfToImages, crop } = value;
  return (
    (language === "ja" || language === "en" || language === null) &&
    isRecord(imagesToPdf) &&
    (imagesToPdf.output === "merge" || imagesToPdf.output === "each") &&
    isPageSizeChoice(imagesToPdf.pageSize) &&
    isPageOrientationChoice(imagesToPdf.orientation) &&
    typeof imagesToPdf.pdfA === "boolean" &&
    typeof imagesToPdf.encrypt === "boolean" &&
    isOutputDirLabel(imagesToPdf.outputDir) &&
    isRecord(pdfToImages) &&
    isRenderFormatChoice(pdfToImages.format) &&
    isRenderColorChoice(pdfToImages.colorMode) &&
    isRenderDpi(pdfToImages.dpi) &&
    isOutputDirLabel(pdfToImages.outputDir) &&
    isRecord(crop) &&
    isRenderFormatChoice(crop.format) &&
    isRenderColorChoice(crop.colorMode) &&
    isRenderDpi(crop.dpi) &&
    typeof crop.photoNative === "boolean" &&
    typeof crop.photoOnly === "boolean" &&
    isOutputDirLabel(crop.outputDir)
  );
}

/**
 * Fetches the settings. Output folders come as names, never as paths (design
 * §6.7). Rejects with `InvalidParams` when the answer does not have that shape.
 */
export async function getSettings(): Promise<Settings> {
  const value = await invokeWrapped<unknown>("get_settings");
  if (!isSettings(value)) {
    throw normalizeIpcError("get_settings returned an unexpected value");
  }
  return value;
}

/**
 * Saves the settings except the output folders, which only `pickOutputDir`
 * changes (design §6.7). `revision` must be higher on each call from the page:
 * Rust ignores a call that arrives after a later one. Resolves once Rust holds
 * the settings; the file is written afterwards and a failed write is not
 * reported. Rejects with `InvalidParams` for a value Rust refuses.
 */
export async function saveSettings(
  revision: number,
  settings: SettingsInput,
): Promise<void> {
  return invokeWrapped<void>("save_settings", { revision, settings });
}

/**
 * Asks Rust to open a folder dialog and make the folder the output folder of
 * `kind`. Resolves to `null` when the dialog was cancelled.
 */
export async function pickOutputDir(
  kind: OutputKind,
): Promise<OutputDirLabel | null> {
  return invokeWrapped<OutputDirLabel | null>("pick_output_dir", { kind });
}

/**
 * Asks Rust to open a dialog for images or a folder and add the images in it.
 * Resolves to `null` when the dialog was cancelled.
 */
export async function addImages(
  source: AddSource,
): Promise<AddResult<ImageItem> | null> {
  return invokeWrapped<AddResult<ImageItem> | null>("add_images", { source });
}

/**
 * Asks Rust to open a dialog for PDFs or a folder and add the PDFs in it.
 * Resolves to `null` when the dialog was cancelled.
 */
export async function addPdfs(
  source: AddSource,
): Promise<AddResult<PdfItem> | null> {
  return invokeWrapped<AddResult<PdfItem> | null>("add_pdfs", { source });
}

/** Removes items from Rust's table; an ID that is not in it is ignored. */
export async function removeItems(ids: number[]): Promise<void> {
  return invokeWrapped<void>("remove_items", { ids });
}

/**
 * Unlocks a password-protected PDF and attempts unlocking other password-protected
 * PDFs in the table with the same password (design §6.8, §7.1).
 * Rejects with `InvalidParams` when the answer does not have that shape.
 */
export async function unlockPdf(
  id: number,
  password: string,
): Promise<UnlockResult> {
  const value = await invokeWrapped<unknown>("unlock_pdf", { id, password });
  if (!isUnlockResult(value)) {
    throw normalizeIpcError("unlock_pdf returned an unexpected value");
  }
  return value;
}

/**
 * Asks Rust to open a dialog for one PDF and hold it as the pending crop PDF
 * (design §6.10). Resolves to `null` when the dialog was cancelled. The open
 * crop PDF stays as it is until `activateCropPdf`.
 */
export async function openCropPdf(): Promise<CropPdf | null> {
  return invokeWrapped<CropPdf | null>("open_crop_pdf");
}

/**
 * Makes the pending crop PDF the open one and drops the one before with its
 * analysis worker. Rejects with `UnknownHandle` unless `id` is the pending PDF.
 */
export async function activateCropPdf(id: number): Promise<void> {
  return invokeWrapped<void>("activate_crop_pdf", { id });
}

/** Drops the pending crop PDF if it is `id`; any other ID is ignored. */
export async function discardCropPdf(id: number): Promise<void> {
  return invokeWrapped<void>("discard_crop_pdf", { id });
}

/** Drops the open crop PDF and the pending one. */
export async function closeCropPdf(): Promise<void> {
  return invokeWrapped<void>("close_crop_pdf");
}

/**
 * Opens the open crop PDF again with its password (design §6.8, §6.10). Rejects
 * with `PasswordProtected` for a wrong password.
 */
export async function unlockCropPdf(
  id: number,
  password: string,
): Promise<CropPdf> {
  return invokeWrapped<CropPdf>("unlock_crop_pdf", { id, password });
}

/** Tells Rust where a drop goes: the lists or the crop tab (design §6.10). */
export async function setDropTarget(target: DropTarget): Promise<void> {
  return invokeWrapped<void>("set_drop_target", { target });
}

/** Lists the embedded images of a page of the open crop PDF (design §7.1). */
export async function listPageImages(
  id: number,
  page: number,
): Promise<PageImage[]> {
  return invokeWrapped<PageImage[]>("list_page_images", { id, page });
}

/**
 * Whether anything is drawn over an embedded image of a page of the open crop
 * PDF (design §7.1).
 */
export async function checkOnTop(
  id: number,
  page: number,
  imageIndex: number,
): Promise<boolean> {
  return invokeWrapped<boolean>("check_on_top", { id, page, imageIndex });
}

/**
 * The candidates for cropping on a page of the open crop PDF (design §7.1).
 * Rejects with `LayoutUnavailable` when the layout analysis cannot run.
 */
export async function detectLayout(
  id: number,
  page: number,
): Promise<Candidate[]> {
  return invokeWrapped<Candidate[]>("detect_layout", { id, page });
}

/**
 * Fetches the PNG thumbnail of an item of a list or of the open crop PDF.
 * `page` (from 1) is used for a PDF only and defaults to the first page.
 * `maxSide` is the length of the longer side of a PDF page in pixels; it
 * defaults to the thumbnail size and is refused for an image.
 */
export async function getThumbnail(
  id: number,
  page?: number,
  maxSide?: number,
): Promise<ArrayBuffer> {
  return invokeWrapped<ArrayBuffer>("get_thumbnail", { id, page, maxSide });
}

/** Subscribes to what a drop added to the lists (design §7.2). */
export async function onItemsDropped(
  handler: (payload: ItemsDropped) => void,
): Promise<UnlistenFn> {
  return listen<ItemsDropped>(ITEMS_DROPPED_EVENT, (event) => {
    handler(event.payload);
  });
}

/** Subscribes to what a drop in the crop tab opened (design §7.2). */
export async function onCropDropped(
  handler: (payload: CropDropped) => void,
): Promise<UnlistenFn> {
  return listen<CropDropped>(CROP_DROPPED_EVENT, (event) => {
    handler(event.payload);
  });
}

/**
 * Validates a page range string and counts matching pages across the specified PDF items (design §7.1).
 */
export async function checkPageRange(
  text: string,
  ids: number[],
): Promise<CheckPageRangeResult> {
  return invokeWrapped<CheckPageRangeResult>("check_page_range", { text, ids });
}

/** Requests cancellation of the currently executing conversion job (design §6.5, §7.1). */
export async function cancelJob(): Promise<void> {
  return invokeWrapped<void>("cancel_job");
}

/**
 * Opens a save dialog to save selected images into a single merged PDF (design §6.2, §7.1).
 * Resolves to `null` when the dialog was cancelled, so nothing ran. Once the
 * conversion has run, `savedName` is `null` if no PDF was written.
 */
export async function saveMergedPdf(
  ids: number[],
  pageSize: PageSizeChoice,
  orientation: PageOrientationChoice,
  pdfA: boolean,
  password: string | null,
): Promise<SaveMergedPdfResult | null> {
  return invokeWrapped<SaveMergedPdfResult | null>("save_merged_pdf", {
    ids,
    pageSize,
    orientation,
    pdfA,
    password,
  });
}

/** Starts batch conversion of images to individual PDF files in the background (design §6.2, §7.1). */
export async function startImagesToPdfs(
  ids: number[],
  pageSize: PageSizeChoice,
  orientation: PageOrientationChoice,
  pdfA: boolean,
  password: string | null,
): Promise<void> {
  return invokeWrapped<void>("start_images_to_pdfs", {
    ids,
    pageSize,
    orientation,
    pdfA,
    password,
  });
}

/** Starts batch conversion of PDF pages to image files in the background (design §6.3, §7.1). */
export async function startPdfsToImages(
  ids: number[],
  range: string,
  format: RenderFormatChoice,
  colorMode: RenderColorChoice,
  dpi: number,
): Promise<void> {
  return invokeWrapped<void>("start_pdfs_to_images", {
    ids,
    range,
    format,
    colorMode,
    dpi,
  });
}

/** Subscribes to job progress updates (design §7.2). */
export async function onJobProgress(
  handler: (payload: JobProgressPayload) => void,
): Promise<UnlistenFn> {
  return listen<JobProgressPayload>(JOB_PROGRESS_EVENT, (event) => {
    handler(event.payload);
  });
}

/** Subscribes to individual item completion events (design §7.2). */
export async function onJobItem(
  handler: (payload: JobItemPayload) => void,
): Promise<UnlistenFn> {
  return listen<JobItemPayload>(JOB_ITEM_EVENT, (event) => {
    handler(event.payload);
  });
}

/** Subscribes to job finished events (design §7.2). */
export async function onJobFinished(
  handler: (payload: JobFinishedPayload) => void,
): Promise<UnlistenFn> {
  return listen<JobFinishedPayload>(JOB_FINISHED_EVENT, (event) => {
    handler(event.payload);
  });
}
