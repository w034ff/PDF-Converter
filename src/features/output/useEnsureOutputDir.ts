import { useState } from "react";
import {
  normalizeIpcError,
  pickOutputDir,
  type OutputDirLabel,
} from "../../ipc";
import { useAppDispatch, type ActiveTab } from "../../state";

export interface EnsureOutputDirResult {
  runWithOutputDir: (
    currentDir: OutputDirLabel | null,
    onConfirmed: () => Promise<void> | void,
  ) => Promise<void>;
  isPicking: boolean;
}

/**
 * Ensures an output directory is selected before starting a conversion (design §6.5).
 * If no directory is chosen, opens the folder dialog first.
 */
export function useEnsureOutputDir(tab: ActiveTab): EnsureOutputDirResult {
  const [isPicking, setIsPicking] = useState(false);
  const dispatch = useAppDispatch();

  const runWithOutputDir = async (
    currentDir: OutputDirLabel | null,
    onConfirmed: () => Promise<void> | void,
  ) => {
    if (isPicking) {
      return;
    }
    if (currentDir !== null) {
      await onConfirmed();
      return;
    }

    setIsPicking(true);
    try {
      const picked = await pickOutputDir(tab);
      if (picked !== null) {
        if (tab === "imagesToPdf") {
          dispatch({ type: "SET_IMAGES_OUTPUT_DIR", outputDir: picked });
        } else {
          dispatch({ type: "SET_PDFS_OUTPUT_DIR", outputDir: picked });
        }
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

  return { runWithOutputDir, isPicking };
}
