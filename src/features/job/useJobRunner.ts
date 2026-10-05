import { useMemo } from "react";
import {
  cancelJob,
  normalizeIpcError,
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
    ): Promise<T | undefined> {
      dispatch({ type: "JOB_STARTED", kind, targets });
      try {
        return await command(targets.map((target) => target.id));
      } catch (error: unknown) {
        dispatch({ type: "JOB_FAILED", error: normalizeIpcError(error) });
        return undefined;
      }
    }

    return {
      async saveMergedPdf(targets, pageSize) {
        const result = await run("imagesToPdf", targets, (ids) =>
          saveMergedPdf(ids, pageSize),
        );
        if (result === null) {
          // The save dialog was cancelled; nothing ran.
          dispatch({ type: "JOB_RESET" });
        } else if (result !== undefined) {
          dispatch({ type: "JOB_SAVED", savedName: result.savedName });
        }
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
