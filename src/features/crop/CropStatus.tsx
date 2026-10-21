import { formatMessage, getTranslations } from "../../i18n";
import { useAppState } from "../../state";

/** What the bottom bar says while nothing is converting (design §10.1). */
export function CropStatus() {
  const { language, crop } = useAppState();
  const t = getTranslations(language.language);
  if (crop.pdf === null) {
    return <span className="hint">{t.crop.footer.noPdf}</span>;
  }
  const count = crop.areas.filter((area) => area.checked).length;
  if (count === 0) {
    return <span className="hint">{t.crop.footer.noRanges}</span>;
  }
  const format = {
    png: t.pdfToImages.settings.formatPng,
    jpeg: t.pdfToImages.settings.formatJpeg,
    webp: t.pdfToImages.settings.formatWebp,
  }[crop.format];
  return (
    <span className="hint">
      {formatMessage(
        count === 1
          ? t.crop.footer.willExportSingle
          : t.crop.footer.willExportMultiple,
        { count, format },
      )}
    </span>
  );
}
