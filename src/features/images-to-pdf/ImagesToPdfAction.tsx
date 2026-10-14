import { useMemo, useRef, useState } from "react";
import { getTranslations } from "../../i18n";
import { isJobActive, useAppState, type JobTarget } from "../../state";
import { useEnsureOutputDir } from "../output";
import { useJobRunner } from "../job/useJobRunner";
import { OutputPasswordDialog } from "./OutputPasswordDialog";

/**
 * Bottom bar action button for Images to PDF ("PDF を保存" or "変換を開始").
 * Dispatches conversion requests through `useJobRunner` (design §6.2, §6.5).
 */
export function ImagesToPdfAction() {
  const { imagesToPdf, language, job } = useAppState();
  const runner = useJobRunner();
  const { runWithOutputDir, isPicking } = useEnsureOutputDir("imagesToPdf");
  const t = getTranslations(language.language);

  const [isPasswordDialogOpen, setIsPasswordDialogOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const isMerge = imagesToPdf.output === "merge";
  const isBusy = isJobActive(job);

  const validTargets: JobTarget[] = useMemo(
    () =>
      imagesToPdf.items
        .filter((item) => item.error === null)
        .map((item) => ({ id: item.id, name: item.name })),
    [imagesToPdf.items],
  );

  const isDisabled =
    validTargets.length === 0 || isBusy || (!isMerge && isPicking);

  const start = (password: string | null) => {
    if (isMerge) {
      void runner.saveMergedPdf(
        validTargets,
        imagesToPdf.pageSize,
        imagesToPdf.orientation,
        imagesToPdf.pdfA,
        password,
      );
    } else {
      void runWithOutputDir(imagesToPdf.outputDir, () =>
        runner.startImagesToPdfs(
          validTargets,
          imagesToPdf.pageSize,
          imagesToPdf.orientation,
          imagesToPdf.pdfA,
          password,
        ),
      );
    }
  };

  const handleClick = () => {
    if (imagesToPdf.encrypt) {
      setIsPasswordDialogOpen(true);
    } else {
      start(null);
    }
  };

  // The password goes from the dialog to the command and nowhere else (design §6.9).
  const handleConfirm = (password: string) => {
    setIsPasswordDialogOpen(false);
    start(password);
  };

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn btn-primary app-footer-action"
        disabled={isDisabled}
        onClick={handleClick}
      >
        {isMerge ? t.footer.savePdf : t.footer.startConversion}
      </button>
      <OutputPasswordDialog
        isOpen={isPasswordDialogOpen}
        mode={isMerge ? "merge" : "each"}
        count={validTargets.length}
        onCancel={() => setIsPasswordDialogOpen(false)}
        onConfirm={handleConfirm}
        returnFocusRef={buttonRef}
      />
    </>
  );
}
