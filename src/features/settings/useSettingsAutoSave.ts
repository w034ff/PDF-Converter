import { useEffect, useRef } from "react";
import { saveSettings, type SettingsInput } from "../../ipc";
import { useAppState } from "../../state";

/**
 * How long the settings wait after a change before they are saved, so a run
 * of changes (stepping through the resolutions, say) is written once.
 */
export const SETTINGS_SAVE_DEBOUNCE_MS = 1000;

function sameSettings(a: SettingsInput, b: SettingsInput): boolean {
  return (
    a.language === b.language &&
    a.imagesToPdf.output === b.imagesToPdf.output &&
    a.imagesToPdf.pageSize === b.imagesToPdf.pageSize &&
    a.pdfToImages.format === b.pdfToImages.format &&
    a.pdfToImages.dpi === b.pdfToImages.dpi
  );
}

/**
 * Saves the settings with `save_settings` once they have stayed unchanged
 * for {@link SETTINGS_SAVE_DEBOUNCE_MS} (design §6.7). What the app started
 * with is not saved again. Output folders are not part of it: `pick_output_dir`
 * saves those itself.
 *
 * A failed save is dropped: the settings still apply to this session, and
 * keeping them for the next one is a Should requirement (FR-10) that must not
 * get in the way of converting.
 */
export function useSettingsAutoSave(): void {
  const { language, imagesToPdf, pdfToImages } = useAppState();
  const preference = language.preference;
  const { output, pageSize } = imagesToPdf;
  const { format, dpi } = pdfToImages;

  const savedRef = useRef<SettingsInput>({
    language: preference,
    imagesToPdf: { output, pageSize },
    pdfToImages: { format, dpi },
  });

  useEffect(() => {
    const next: SettingsInput = {
      language: preference,
      imagesToPdf: { output, pageSize },
      pdfToImages: { format, dpi },
    };
    if (sameSettings(next, savedRef.current)) {
      return;
    }
    const timer = setTimeout(() => {
      savedRef.current = next;
      saveSettings(next).catch(() => {
        // Dropped on purpose; see the doc comment.
      });
    }, SETTINGS_SAVE_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [preference, output, pageSize, format, dpi]);
}
