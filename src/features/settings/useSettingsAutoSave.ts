import { useEffect, useRef } from "react";
import { saveSettings, type SettingsInput } from "../../ipc";
import { useAppState } from "../../state";

function sameSettings(a: SettingsInput, b: SettingsInput): boolean {
  return (
    a.language === b.language &&
    a.imagesToPdf.output === b.imagesToPdf.output &&
    a.imagesToPdf.pageSize === b.imagesToPdf.pageSize &&
    a.imagesToPdf.orientation === b.imagesToPdf.orientation &&
    a.imagesToPdf.pdfA === b.imagesToPdf.pdfA &&
    a.imagesToPdf.encrypt === b.imagesToPdf.encrypt &&
    a.pdfToImages.format === b.pdfToImages.format &&
    a.pdfToImages.colorMode === b.pdfToImages.colorMode &&
    a.pdfToImages.dpi === b.pdfToImages.dpi &&
    a.crop.format === b.crop.format &&
    a.crop.colorMode === b.crop.colorMode &&
    a.crop.dpi === b.crop.dpi &&
    a.crop.photoNative === b.crop.photoNative &&
    a.crop.photoOnly === b.crop.photoOnly
  );
}

/**
 * Saves the settings with `save_settings` as soon as they change (design
 * §6.7), so closing the app right after a change keeps it. Each call carries
 * a higher revision, so Rust keeps the latest settings even if the calls
 * arrive out of order. What the app started with, or what was sent last, is
 * not sent again. Output folders are not part of it: `pick_output_dir` saves
 * those itself.
 *
 * A rejected save is dropped: the settings still apply to this session, and
 * keeping them for the next one is a Should requirement (FR-10) that must not
 * get in the way of converting.
 */
export function useSettingsAutoSave(): void {
  const { language, imagesToPdf, pdfToImages, crop } = useAppState();
  const preference = language.preference;
  const { output, pageSize, orientation, pdfA, encrypt } = imagesToPdf;
  const { format, colorMode, dpi } = pdfToImages;
  const {
    format: cropFormat,
    colorMode: cropColorMode,
    dpi: cropDpi,
    photoNative,
    photoOnly,
  } = crop;

  const sentRef = useRef<SettingsInput>({
    language: preference,
    imagesToPdf: { output, pageSize, orientation, pdfA, encrypt },
    pdfToImages: { format, colorMode, dpi },
    crop: {
      format: cropFormat,
      colorMode: cropColorMode,
      dpi: cropDpi,
      photoNative,
      photoOnly,
    },
  });
  const revisionRef = useRef(0);

  useEffect(() => {
    const next: SettingsInput = {
      language: preference,
      imagesToPdf: { output, pageSize, orientation, pdfA, encrypt },
      pdfToImages: { format, colorMode, dpi },
      crop: {
        format: cropFormat,
        colorMode: cropColorMode,
        dpi: cropDpi,
        photoNative,
        photoOnly,
      },
    };
    if (sameSettings(next, sentRef.current)) {
      return;
    }
    sentRef.current = next;
    revisionRef.current += 1;
    saveSettings(revisionRef.current, next).catch(() => {
      // Dropped on purpose; see the doc comment.
    });
  }, [
    preference,
    output,
    pageSize,
    orientation,
    pdfA,
    encrypt,
    format,
    colorMode,
    dpi,
    cropFormat,
    cropColorMode,
    cropDpi,
    photoNative,
    photoOnly,
  ]);
}
