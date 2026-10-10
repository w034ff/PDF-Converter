import { useMemo } from "react";
import { getTranslations } from "../../i18n";
import { isJobActive, useAppState, type JobTarget } from "../../state";
import { useEnsureOutputDir } from "../output";
import { useJobRunner } from "../job/useJobRunner";

/**
 * Bottom bar action button for Images to PDF ("PDF を保存" or "変換を開始").
 * Dispatches conversion requests through `useJobRunner` (design §6.2, §6.5).
 */
export function ImagesToPdfAction() {
  const { imagesToPdf, language, job } = useAppState();
  const runner = useJobRunner();
  const { runWithOutputDir, isPicking } = useEnsureOutputDir("imagesToPdf");
  const t = getTranslations(language.language);

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

  const handleClick = () => {
    if (isMerge) {
      void runner.saveMergedPdf(
        validTargets,
        imagesToPdf.pageSize,
        imagesToPdf.a4Orientation,
      );
    } else {
      void runWithOutputDir(imagesToPdf.outputDir, () =>
        runner.startImagesToPdfs(
          validTargets,
          imagesToPdf.pageSize,
          imagesToPdf.a4Orientation,
        ),
      );
    }
  };

  return (
    <button
      type="button"
      className="btn btn-primary app-footer-action"
      disabled={isDisabled}
      onClick={handleClick}
    >
      {isMerge ? t.footer.savePdf : t.footer.startConversion}
    </button>
  );
}
