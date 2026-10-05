import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AddResult } from "./generated/AddResult";
import type { AddSource } from "./generated/AddSource";
import type { CheckPageRangeResult } from "./generated/CheckPageRangeResult";
import type { ErrorCode } from "./generated/ErrorCode";
import type { ImageItem } from "./generated/ImageItem";
import type { IpcError } from "./generated/IpcError";
import type { ItemsDropped } from "./generated/ItemsDropped";
import type { JobFinishedPayload } from "./generated/JobFinishedPayload";
import type { JobItemPayload } from "./generated/JobItemPayload";
import type { JobProgressPayload } from "./generated/JobProgressPayload";
import type { PageSizeChoice } from "./generated/PageSizeChoice";
import type { PdfItem } from "./generated/PdfItem";
import type { RenderFormatChoice } from "./generated/RenderFormatChoice";
import type { SaveMergedPdfResult } from "./generated/SaveMergedPdfResult";

export type { AboutInfo } from "./generated/AboutInfo";
export type { AddResult } from "./generated/AddResult";
export type { AddSource } from "./generated/AddSource";
export type { CheckPageRangeResult } from "./generated/CheckPageRangeResult";
export type { ErrorCode } from "./generated/ErrorCode";
export type { ImageFormatName } from "./generated/ImageFormatName";
export type { ImageItem } from "./generated/ImageItem";
export type { IpcError } from "./generated/IpcError";
export type { ItemsDropped } from "./generated/ItemsDropped";
export type { JobFinishedPayload } from "./generated/JobFinishedPayload";
export type { JobItemPayload } from "./generated/JobItemPayload";
export type { JobItemStatus } from "./generated/JobItemStatus";
export type { JobProgressPayload } from "./generated/JobProgressPayload";
export type { PageSizeChoice } from "./generated/PageSizeChoice";
export type { PageSizePt } from "./generated/PageSizePt";
export type { PdfItem } from "./generated/PdfItem";
export type { RenderFormatChoice } from "./generated/RenderFormatChoice";
export type { SaveMergedPdfResult } from "./generated/SaveMergedPdfResult";
export type { Skipped } from "./generated/Skipped";
export type { UnlistenFn };

/** Name of the event Rust emits after a drop (design §7.2). */
export const ITEMS_DROPPED_EVENT = "items-dropped";

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
 * Fetches the PNG thumbnail of an item. `page` (from 1) is used for a PDF
 * only and defaults to the first page.
 */
export async function getThumbnail(
  id: number,
  page?: number,
): Promise<ArrayBuffer> {
  return invokeWrapped<ArrayBuffer>("get_thumbnail", { id, page });
}

/** Subscribes to what a drop added to the lists (design §7.2). */
export async function onItemsDropped(
  handler: (payload: ItemsDropped) => void,
): Promise<UnlistenFn> {
  return listen<ItemsDropped>(ITEMS_DROPPED_EVENT, (event) => {
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
 * Resolves to `null` when the dialog was cancelled.
 */
export async function saveMergedPdf(
  ids: number[],
  pageSize: PageSizeChoice,
): Promise<SaveMergedPdfResult | null> {
  return invokeWrapped<SaveMergedPdfResult | null>("save_merged_pdf", {
    ids,
    pageSize,
  });
}

/** Starts batch conversion of images to individual PDF files in the background (design §6.2, §7.1). */
export async function startImagesToPdfs(
  ids: number[],
  pageSize: PageSizeChoice,
): Promise<void> {
  return invokeWrapped<void>("start_images_to_pdfs", { ids, pageSize });
}

/** Starts batch conversion of PDF pages to image files in the background (design §6.3, §7.1). */
export async function startPdfsToImages(
  ids: number[],
  range: string,
  format: RenderFormatChoice,
  dpi: number,
): Promise<void> {
  return invokeWrapped<void>("start_pdfs_to_images", {
    ids,
    range,
    format,
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
