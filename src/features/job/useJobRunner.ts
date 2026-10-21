import { useMemo } from "react";
import {
  cancelJob,
  normalizeIpcError,
  type CropRegionInput,
  type IpcError,
  type PageOrientationChoice,
  saveMergedPdf,
  startCropExport,
  startDetectAll,
  startImagesToPdfs,
  startPdfsToImages,
  type PageSizeChoice,
  type RenderColorChoice,
  type RenderFormatChoice,
} from "../../ipc";
import { useAppDispatch, type ActiveTab, type JobTarget } from "../../state";

export interface JobRunner {
  /** Starts "1 つの PDF": opens the save dialog, then merges `targets` in order. */
  saveMergedPdf(
    targets: JobTarget[],
    pageSize: PageSizeChoice,
    orientation: PageOrientationChoice,
    pdfA: boolean,
    password: string | null,
  ): Promise<void>;
  /** Starts "1 枚ずつ" for `targets` (design §6.2). */
  startImagesToPdfs(
    targets: JobTarget[],
    pageSize: PageSizeChoice,
    orientation: PageOrientationChoice,
    pdfA: boolean,
    password: string | null,
  ): Promise<void>;
  /** Starts PDF → images for `targets` (design §6.3). */
  startPdfsToImages(
    targets: JobTarget[],
    range: string,
    format: RenderFormatChoice,
    colorMode: RenderColorChoice,
    dpi: number,
  ): Promise<void>;
  /**
   * Starts writing `regions` of the crop PDF `id` (design §6.10). `targets`
   * are the areas, each under its `key`.
   */
  startCropExport(
    id: number,
    targets: JobTarget[],
    regions: CropRegionInput[],
    format: RenderFormatChoice,
    colorMode: RenderColorChoice,
    dpi: number,
    photoNative: boolean,
    photoOnly: boolean,
  ): Promise<void>;
  /** Starts the layout analysis of every page of the crop PDF `id` (design §6.10). */
  startDetectAll(id: number): Promise<void>;
  /** Shows "キャンセル中…" at once and asks the backend to stop (design §6.5). */
  cancel(): Promise<void>;
}

/**
 * Starts and cancels conversions through the job state, so both screens
 * report progress, results and failures the same way.
 *
 * The job is marked as started before the command is sent: the backend can
 * emit `job-progress` before the command's promise resolves, and those
 * events are dropped unless a job is running.
 */
export function useJobRunner(): JobRunner {
  const dispatch = useAppDispatch();

  return useMemo(() => {
    async function run<T>(
      kind: ActiveTab,
      targets: JobTarget[],
      command: (ids: number[]) => Promise<T>,
    ): Promise<{ result: T } | { error: IpcError }> {
      dispatch({ type: "JOB_STARTED", kind, targets });
      try {
        return { result: await command(targets.map((target) => target.id)) };
      } catch (error: unknown) {
        const ipcError = normalizeIpcError(error);
        if (ipcError.code === "OutputDirMissing") {
          // Rust has forgotten the folder, so the field reads "未選択" and
          // the next start asks for a folder (`useEnsureOutputDir`). This
          // comes before JOB_FAILED because changing a setting drops a
          // result that is already there.
          dispatch(
            kind === "imagesToPdf"
              ? { type: "SET_IMAGES_OUTPUT_DIR", outputDir: null }
              : kind === "crop"
                ? { type: "SET_CROP_OUTPUT_DIR", outputDir: null }
                : { type: "SET_PDFS_OUTPUT_DIR", outputDir: null },
          );
        }
        dispatch({ type: "JOB_FAILED", error: ipcError });
        return { error: ipcError };
      }
    }

    return {
      async saveMergedPdf(targets, pageSize, orientation, pdfA, password) {
        const outcome = await run("imagesToPdf", targets, (ids) =>
          saveMergedPdf(ids, pageSize, orientation, pdfA, password),
        );
        if ("error" in outcome) {
          return;
        }
        if (outcome.result === null) {
          // The save dialog was cancelled; nothing ran.
          dispatch({ type: "JOB_RESET" });
        } else if (outcome.result.savedName !== null) {
          dispatch({ type: "JOB_SAVED", savedName: outcome.result.savedName });
        }
        // Otherwise the job ran but wrote no PDF (it was cancelled, or no
        // image could be added). `job-finished` has already set the result,
        // and the banner and rows keep showing it.
      },
      async startImagesToPdfs(targets, pageSize, orientation, pdfA, password) {
        await run("imagesToPdf", targets, (ids) =>
          startImagesToPdfs(ids, pageSize, orientation, pdfA, password),
        );
      },
      async startPdfsToImages(targets, range, format, colorMode, dpi) {
        await run("pdfToImages", targets, (ids) =>
          startPdfsToImages(ids, range, format, colorMode, dpi),
        );
      },
      async startCropExport(
        id,
        targets,
        regions,
        format,
        colorMode,
        dpi,
        photoNative,
        photoOnly,
      ) {
        dispatch({ type: "CROP_EXPORT_STARTED" });
        await run("crop", targets, () =>
          startCropExport(
            id,
            regions,
            format,
            colorMode,
            dpi,
            photoNative,
            photoOnly,
          ),
        );
      },
      async startDetectAll(id) {
        // The areas of the pages already analyzed are added before Rust
        // starts, in the same step that marks this as the last conversion.
        dispatch({ type: "CROP_DETECT_ALL_STARTED" });
        await run("crop", [], () => startDetectAll(id));
      },
      async cancel() {
        dispatch({ type: "JOB_CANCEL_REQUESTED" });
        try {
          await cancelJob();
        } catch {
          // cancel_job only raises a flag. If the call itself fails, the job
          // runs to its end, and job-finished still reports how it ended.
        }
      },
    };
  }, [dispatch]);
}
