import { useState } from "react";
import {
  normalizeIpcError,
  pickOutputDir,
  type IpcError,
  type OutputDirLabel,
} from "../../ipc";
import { useAppDispatch, type ActiveTab } from "../../state";

export interface EnsureOutputDirResult {
  /**
   * Calls `onConfirmed` once a folder is chosen. It may resolve with the
   * error that refused the start; `OutputDirMissing` means the folder was
   * gone, and the folder dialog then opens as if none had been chosen.
   */
  runWithOutputDir: (
    currentDir: OutputDirLabel | null,
    onConfirmed: () => Promise<IpcError | null | void> | void,
  ) => Promise<void>;
  isPicking: boolean;
}

/**
 * Ensures an output directory is selected before starting a conversion (design §6.5).
 * If no directory is chosen, or the chosen one has gone, opens the folder
 * dialog first.
 */
export function useEnsureOutputDir(tab: ActiveTab): EnsureOutputDirResult {
  const [isPicking, setIsPicking] = useState(false);
  const dispatch = useAppDispatch();

  const runWithOutputDir = async (
    currentDir: OutputDirLabel | null,
    onConfirmed: () => Promise<IpcError | null | void> | void,
  ) => {
    if (isPicking) {
      return;
    }

    const pickAndConfirm = async () => {
      setIsPicking(true);
      try {
        const picked = await pickOutputDir(tab);
        if (picked !== null) {
          if (tab === "imagesToPdf") {
            dispatch({ type: "SET_IMAGES_OUTPUT_DIR", outputDir: picked });
          } else {
            dispatch({ type: "SET_PDFS_OUTPUT_DIR", outputDir: picked });
          }
          // A folder that was just chosen and is already gone is reported
          // as it is, not asked for again.
          await onConfirmed();
        }
      } catch (err: unknown) {
        dispatch({
          type: "JOB_NOT_STARTED",
          kind: tab,
          error: normalizeIpcError(err),
        });
      } finally {
        setIsPicking(false);
      }
    };

    if (currentDir === null) {
      await pickAndConfirm();
      return;
    }
    const refused = await onConfirmed();
    if (refused?.code === "OutputDirMissing") {
      await pickAndConfirm();
    }
  };

  return { runWithOutputDir, isPicking };
}
