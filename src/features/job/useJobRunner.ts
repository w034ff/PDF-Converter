import { useMemo } from "react";
import {
  cancelJob,
  normalizeIpcError,
  type IpcError,
  saveMergedPdf,
  startImagesToPdfs,
  startPdfsToImages,
  type PageSizeChoice,
  type RenderFormatChoice,
} from "../../ipc";
import { useAppDispatch, type ActiveTab, type JobTarget } from "../../state";

export interface JobRunner {
  /** Starts "1 つの PDF": opens the save dialog, then merges `targets` in order. */
  saveMergedPdf(targets: JobTarget[], pageSize: PageSizeChoice): Promise<void>;
  /** Starts "1 枚ずつ" for `targets` (design §6.2). */
  startImagesToPdfs(
    targets: JobTarget[],
    pageSize: PageSizeChoice,
  ): Promise<void>;
  /** Starts PDF → images for `targets` (design §6.3). */
  startPdfsToImages(
    targets: JobTarget[],
    range: string,
    format: RenderFormatChoice,
    dpi: number,
  ): Promise<void>;
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
              : { type: "SET_PDFS_OUTPUT_DIR", outputDir: null },
          );
        }
        dispatch({ type: "JOB_FAILED", error: ipcError });
        return { error: ipcError };
      }
    }

    return {
      async saveMergedPdf(targets, pageSize) {
        const outcome = await run("imagesToPdf", targets, (ids) =>
          saveMergedPdf(ids, pageSize),
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
      async startImagesToPdfs(targets, pageSize) {
        await run("imagesToPdf", targets, (ids) =>
          startImagesToPdfs(ids, pageSize),
        );
      },
      async startPdfsToImages(targets, range, format, dpi) {
        await run("pdfToImages", targets, (ids) =>
          startPdfsToImages(ids, range, format, dpi),
        );
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
