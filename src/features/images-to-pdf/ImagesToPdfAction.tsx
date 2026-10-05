import { useMemo } from "react";
import { getTranslations } from "../../i18n";
import { useAppState, type JobTarget } from "../../state";
import { useJobRunner } from "../job/useJobRunner";

/**
 * Bottom bar action button for Images to PDF ("PDF を保存" or "変換を開始").
 * Dispatches conversion requests through `useJobRunner` (design §6.2, §6.5).
 */
export function ImagesToPdfAction() {
  const { imagesToPdf, language } = useAppState();
  const runner = useJobRunner();
  const t = getTranslations(language.language);

  const isMerge = imagesToPdf.output === "merge";

  const validTargets: JobTarget[] = useMemo(
    () =>
      imagesToPdf.items
        .filter((item) => item.error === null)
        .map((item) => ({ id: item.id, name: item.name })),
    [imagesToPdf.items],
  );

  const isDisabled =
    validTargets.length === 0 || (!isMerge && imagesToPdf.outputDir === null);

  const handleClick = () => {
    if (isMerge) {
      void runner.saveMergedPdf(validTargets, imagesToPdf.pageSize);
    } else {
      void runner.startImagesToPdfs(validTargets, imagesToPdf.pageSize);
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
