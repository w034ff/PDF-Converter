import type { Dispatch } from "react";
import {
  activateCropPdf,
  discardCropPdf,
  normalizeIpcError,
  openCropPdf,
  type CropDropped,
  type CropPdf,
} from "../../ipc";
import type { AppAction } from "../../state";

/**
 * Takes the pending PDF Rust opened (design §6.10, §10.1). A PDF that opened,
 * or that only needs its password, becomes the open one. A broken one is
 * dropped and reported, and the open PDF stays, so a file that would not open
 * anyway does not cost the user their work. Nothing here confirms a
 * replacement yet: until there are ranges to lose, it is always safe to
 * replace.
 */
export async function receivePendingCropPdf(
  dispatch: Dispatch<AppAction>,
  pdf: CropPdf,
): Promise<void> {
  try {
    if (pdf.error === null || pdf.error.code === "PasswordProtected") {
      await activateCropPdf(pdf.id);
      dispatch({ type: "CROP_PDF_ACTIVATED", pdf });
    } else {
      await discardCropPdf(pdf.id);
      dispatch({
        type: "CROP_OPEN_NOTICE",
        notice: { kind: "error", error: pdf.error },
      });
    }
  } catch (failure: unknown) {
    dispatch({
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: normalizeIpcError(failure) },
    });
  }
}

/**
 * Opens the file dialog for one PDF and takes what it picks. Nothing changes
 * when the dialog is cancelled.
 */
export async function openCropPdfFromDialog(
  dispatch: Dispatch<AppAction>,
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
    await receivePendingCropPdf(dispatch, pdf);
  }
}

/** Takes what a drop on the crop tab opened (`crop-dropped`, design §7.2). */
export async function receiveCropDropped(
  dispatch: Dispatch<AppAction>,
  dropped: CropDropped,
): Promise<void> {
  if (dropped.error !== null) {
    dispatch({
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: dropped.error },
    });
  } else if (dropped.notOnePdf) {
    dispatch({ type: "CROP_OPEN_NOTICE", notice: { kind: "notOnePdf" } });
  } else if (dropped.cropPdf !== null) {
    await receivePendingCropPdf(dispatch, dropped.cropPdf);
  }
}
