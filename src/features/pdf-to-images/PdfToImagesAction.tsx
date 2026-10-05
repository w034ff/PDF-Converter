import { getTranslations } from "../../i18n";
import { isJobActive, useAppState } from "../../state";
import { useJobRunner } from "../job/useJobRunner";

/**
 * Bottom bar action button for PDF to Images ("変換を開始", design §6.3, §6.5).
 */
export function PdfToImagesAction() {
  const { language, pdfToImages, job } = useAppState();
  const runner = useJobRunner();
  const t = getTranslations(language.language);

  // Targets are error-free items in list order (design §6.1, §6.3)
  const targets = pdfToImages.items
    .filter((item) => item.error === null)
    .map((item) => ({ id: item.id, name: item.name }));

  const hasTargets = targets.length > 0;
  const hasOutputDir = pdfToImages.outputDir !== null;
  const isBusy = isJobActive(job);

  const isRangeValid =
    pdfToImages.pageSelection === "all" ||
    (!pdfToImages.rangeChecking &&
      pdfToImages.rangeError === null &&
      pdfToImages.rangeResult !== null);

  const disabled = !hasTargets || !hasOutputDir || isBusy || !isRangeValid;

  async function handleStart() {
    if (disabled) {
      return;
    }
    // When "all", range is 1-{maxPages} across targets (design §6.3)
    let range: string;
    if (pdfToImages.pageSelection === "all") {
      const maxPages = pdfToImages.items
        .filter((item) => item.error === null)
        .reduce((max, item) => Math.max(max, item.pageCount), 0);
      range = `1-${maxPages}`;
    } else {
      range = pdfToImages.rangeText;
    }

    await runner.startPdfsToImages(
      targets,
      range,
      pdfToImages.format,
      pdfToImages.dpi,
    );
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
