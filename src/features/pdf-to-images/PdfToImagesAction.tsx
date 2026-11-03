import { getTranslations } from "../../i18n";
import {
  hasInvalidCustomDpi,
  isJobActive,
  maxPageCount,
  readablePdfs,
  useAppState,
} from "../../state";
import { useEnsureOutputDir } from "../output";
import { useJobRunner } from "../job/useJobRunner";

/**
 * Bottom bar action button for PDF to Images ("変換を開始", design §6.3, §6.5).
 */
export function PdfToImagesAction() {
  const { language, pdfToImages, job } = useAppState();
  const runner = useJobRunner();
  const { runWithOutputDir, isPicking } = useEnsureOutputDir("pdfToImages");
  const t = getTranslations(language.language);

  const targets = readablePdfs(pdfToImages).map((item) => ({
    id: item.id,
    name: item.name,
  }));

  const hasTargets = targets.length > 0;
  const isBusy = isJobActive(job);

  // A range that no page of any PDF falls in would convert nothing. A range
  // that only some PDFs lack pages for still starts; those rows read "対象の
  // ページなし" (FR-04).
  const isRangeValid =
    pdfToImages.pageSelection === "all" ||
    (!pdfToImages.rangeChecking &&
      pdfToImages.rangeError === null &&
      pdfToImages.rangeResult !== null &&
      pdfToImages.rangeResult.totalPages > 0);

  const isDpiValid = !hasInvalidCustomDpi(pdfToImages);

  const disabled =
    !hasTargets || isBusy || !isRangeValid || !isDpiValid || isPicking;

  async function handleStart() {
    if (disabled) {
      return;
    }
    await runWithOutputDir(pdfToImages.outputDir, async () => {
      const range =
        pdfToImages.pageSelection === "all"
          ? `1-${maxPageCount(pdfToImages)}`
          : pdfToImages.rangeText;

      await runner.startPdfsToImages(targets, range, pdfToImages);
    });
  }

  return (
    <button
      type="button"
      className="btn btn-primary app-footer-action"
      disabled={disabled}
      onClick={() => void handleStart()}
    >
      {t.footer.startConversion}
    </button>
  );
}
