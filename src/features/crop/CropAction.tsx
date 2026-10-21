import { getTranslations } from "../../i18n";
import type { CropRegionInput } from "../../ipc";
import { hasInvalidCustomDpi, isJobActive, useAppState } from "../../state";
import { useEnsureOutputDir } from "../output";
import { useJobRunner } from "../job/useJobRunner";
import { areasInListOrder } from "./cropAreas";

/**
 * The bottom bar's "書き出す" (design §6.10, §10.1). It writes the checked
 * areas of the open PDF into the crop output folder, asking for the folder
 * first if there is none.
 */
export function CropAction() {
  const { language, crop, job } = useAppState();
  const runner = useJobRunner();
  const { runWithOutputDir, isPicking } = useEnsureOutputDir("crop");
  const t = getTranslations(language.language);

  const { pdf } = crop;
  const checked = areasInListOrder(crop.areas).filter((area) => area.checked);
  const disabled =
    pdf === null ||
    pdf.error !== null ||
    checked.length === 0 ||
    isJobActive(job) ||
    isPicking ||
    hasInvalidCustomDpi(crop);

  async function handleStart() {
    if (disabled) {
      return;
    }
    await runWithOutputDir(crop.outputDir, async () => {
      const regions: CropRegionInput[] = checked.map((area) => ({
        key: area.key,
        page: area.page,
        rectPt: area.rectPt,
        imageIndex: area.kind === "photo" ? area.imageIndex : null,
      }));
      await runner.startCropExport(
        pdf.id,
        checked.map((area) => ({ id: area.key, name: pdf.name })),
        regions,
        crop.format,
        crop.colorMode,
        crop.dpi,
        crop.photoNative,
        crop.photoOnly,
      );
    });
  }

  return (
    <button
      type="button"
      className="btn btn-primary app-footer-action"
      disabled={disabled}
      onClick={() => void handleStart()}
    >
      {t.crop.footer.export}
    </button>
  );
}
