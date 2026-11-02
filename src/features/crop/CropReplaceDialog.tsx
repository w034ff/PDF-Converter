import type { RefObject } from "react";
import { ConfirmDialog } from "../../components";
import { formatMessage, getTranslations } from "../../i18n";
import {
  unexportedAreaCount,
  useAppDispatch,
  useAppState,
  type CropPendingReplace,
} from "../../state";
import { cancelReplace, confirmReplace } from "./cropOpen";

export interface CropReplaceDialogProps {
  pending: CropPendingReplace;
  /**
   * Where the focus goes when the dialog closes; without it, to what had it
   * when the dialog opened.
   */
  returnFocusRef?: RefObject<HTMLElement | null>;
  /** Called when the user cancels, after the pending PDF is dropped. */
  onCancelled: () => void;
}

/**
 * Asks whether to open another PDF in place of the open one while it has
 * areas that were never exported (design §6.10, §10.1).
 */
export function CropReplaceDialog({
  pending,
  returnFocusRef,
  onCancelled,
}: CropReplaceDialogProps) {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const count = unexportedAreaCount(crop);

  function handleCancel() {
    onCancelled();
    void cancelReplace(dispatch, pending);
  }

  return (
    <ConfirmDialog
      title={t.crop.replaceDialog.title}
      detail={pending.pdf.name}
      cancelLabel={t.crop.replaceDialog.cancel}
      confirmLabel={t.crop.replaceDialog.discard}
      onCancel={handleCancel}
      onConfirm={() => void confirmReplace(dispatch, pending)}
      returnFocusRef={returnFocusRef}
    >
      {formatMessage(
        count === 1
          ? t.crop.replaceDialog.countSingle
          : t.crop.replaceDialog.countMultiple,
        { count },
      )}
    </ConfirmDialog>
  );
}
