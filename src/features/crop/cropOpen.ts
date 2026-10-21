import type { Dispatch } from "react";
import {
  activateCropPdf,
  discardCropPdf,
  normalizeIpcError,
  openCropPdf,
  type CropDropped,
  type CropPdf,
} from "../../ipc";
import {
  unexportedAreaCount,
  type AppAction,
  type CropPendingReplace,
  type CropState,
} from "../../state";

/** Reads the crop state as it is now, which a callback that outlives a render needs. */
export type CropGetter = () => CropState;

/** Makes the pending PDF the open one, and reports a failure where the open notice goes. */
async function activatePending(
  dispatch: Dispatch<AppAction>,
  pdf: CropPdf,
): Promise<void> {
  try {
    await activateCropPdf(pdf.id);
    dispatch({ type: "CROP_PDF_ACTIVATED", pdf });
  } catch (failure: unknown) {
    dispatch({
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: normalizeIpcError(failure) },
    });
  }
}

/** Drops the pending PDF in Rust; a failure goes where the open notice goes. */
async function discardPending(
  dispatch: Dispatch<AppAction>,
  pdf: CropPdf,
): Promise<boolean> {
  try {
    await discardCropPdf(pdf.id);
    return true;
  } catch (failure: unknown) {
    dispatch({
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: normalizeIpcError(failure) },
    });
    return false;
  }
}

/**
 * Takes the pending PDF Rust opened (design §6.10, §10.1).
 *
 * A broken one (any error but `PasswordProtected`) is dropped and reported
 * without asking, and the open PDF stays, so a file that would not open
 * anyway does not cost the user their work. A PDF that opened, or that only
 * needs its password, becomes the open one at once, unless the open one has
 * areas that were never exported: the user is asked first
 * (`pendingReplace`, answered by `confirmReplace` and `cancelReplace`).
 *
 * A pending PDF that arrives while the question is open replaces the one
 * asked about, which Rust has already dropped; a broken one closes the
 * question.
 */
export async function receivePendingCropPdf(
  dispatch: Dispatch<AppAction>,
  pdf: CropPdf,
  source: CropPendingReplace["source"],
  getCrop: CropGetter,
): Promise<void> {
  const crop = getCrop();
  const { error } = pdf;
  if (error !== null && error.code !== "PasswordProtected") {
    if (crop.pendingReplace !== null) {
      dispatch({ type: "CROP_REPLACE_PENDING", pending: null });
    }
    if (await discardPending(dispatch, pdf)) {
      dispatch({ type: "CROP_OPEN_NOTICE", notice: { kind: "error", error } });
    }
    return;
  }
  if (unexportedAreaCount(crop) > 0) {
    dispatch({ type: "CROP_REPLACE_PENDING", pending: { pdf, source } });
    return;
  }
  await activatePending(dispatch, pdf);
}

/** "破棄して開く": the pending PDF replaces the open one and its areas. */
export async function confirmReplace(
  dispatch: Dispatch<AppAction>,
  pending: CropPendingReplace,
): Promise<void> {
  dispatch({ type: "CROP_REPLACE_PENDING", pending: null });
  await activatePending(dispatch, pending.pdf);
}

/** "キャンセル": the pending PDF is dropped, and the open one and its areas stay. */
export async function cancelReplace(
  dispatch: Dispatch<AppAction>,
  pending: CropPendingReplace,
): Promise<void> {
  dispatch({ type: "CROP_REPLACE_PENDING", pending: null });
  await discardPending(dispatch, pending.pdf);
}

/**
 * Opens the file dialog for one PDF and takes what it picks. Nothing changes
 * when the dialog is cancelled.
 */
export async function openCropPdfFromDialog(
  dispatch: Dispatch<AppAction>,
  getCrop: CropGetter,
): Promise<void> {
  dispatch({ type: "CROP_OPEN_NOTICE", notice: null });
  let pdf: CropPdf | null;
  try {
    pdf = await openCropPdf();
  } catch (failure: unknown) {
    dispatch({
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: normalizeIpcError(failure) },
    });
    return;
  }
  if (pdf !== null) {
    await receivePendingCropPdf(dispatch, pdf, "dialog", getCrop);
  }
}

/** Takes what a drop on the crop tab opened (`crop-dropped`, design §7.2). */
export async function receiveCropDropped(
  dispatch: Dispatch<AppAction>,
  dropped: CropDropped,
  getCrop: CropGetter,
): Promise<void> {
  if (dropped.error !== null) {
    dispatch({
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: dropped.error },
    });
  } else if (dropped.notOnePdf) {
    dispatch({ type: "CROP_OPEN_NOTICE", notice: { kind: "notOnePdf" } });
  } else if (dropped.cropPdf !== null) {
    await receivePendingCropPdf(dispatch, dropped.cropPdf, "drop", getCrop);
  }
}
