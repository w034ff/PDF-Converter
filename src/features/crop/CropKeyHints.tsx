import type { RefObject } from "react";
import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import { KeyHintsDialog } from "../shortcuts/KeyHintsDialog";

export interface CropKeyHintsProps {
  onClose: () => void;
  /** Where the focus goes back to once the list is closed. */
  returnFocusRef: RefObject<HTMLElement | null>;
}

/** The keys of the crop tab, listed in a dialog (design §10.1). */
export function CropKeyHints({ onClose, returnFocusRef }: CropKeyHintsProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  return (
    <KeyHintsDialog
      hints={t.crop.keyHints}
      titleId="crop-key-hints-title"
      onClose={onClose}
      returnFocusRef={returnFocusRef}
    />
  );
}
