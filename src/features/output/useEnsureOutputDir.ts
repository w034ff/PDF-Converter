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
        // Each tab keeps the folder of its own conversion.
        switch (tab) {
          case "imagesToPdf":
            dispatch({ type: "SET_IMAGES_OUTPUT_DIR", outputDir: picked });
            break;
          case "pdfToImages":
            dispatch({ type: "SET_PDFS_OUTPUT_DIR", outputDir: picked });
            break;
          case "crop":
            dispatch({ type: "SET_CROP_OUTPUT_DIR", outputDir: picked });
            break;
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
