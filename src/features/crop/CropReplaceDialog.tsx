import { useRef, type RefObject } from "react";
import { ModalDialog } from "../../components";
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
 * areas that were never exported (design §6.10, §10.1). The focus starts on
 * "キャンセル", so that holding Enter down does not lose the areas. It looks
 * like the password dialog: one card without dividers.
 */
export function CropReplaceDialog({
  pending,
  returnFocusRef,
  onCancelled,
}: CropReplaceDialogProps) {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const count = unexportedAreaCount(crop);

  function handleCancel() {
    onCancelled();
    void cancelReplace(dispatch, pending);
  }

  return (
    <ModalDialog
      isOpen={true}
      onClose={handleCancel}
      titleId="crop-replace-title"
      dialogClassName="modal-dialog password-dialog"
      initialFocusRef={cancelRef}
      returnFocusRef={returnFocusRef}
    >
      <div className="password-dialog-header">
        <h2 id="crop-replace-title" className="modal-title">
          {t.crop.replaceDialog.title}
        </h2>
        <div className="password-dialog-filename mono">{pending.pdf.name}</div>
      </div>
      <p className="crop-replace-count">
        {formatMessage(
          count === 1
            ? t.crop.replaceDialog.countSingle
            : t.crop.replaceDialog.countMultiple,
          { count },
        )}
      </p>
      <div className="password-dialog-actions">
        <button
          ref={cancelRef}
          type="button"
          className="btn btn-ghost"
          onClick={handleCancel}
        >
          {t.crop.replaceDialog.cancel}
        </button>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => void confirmReplace(dispatch, pending)}
        >
          {t.crop.replaceDialog.discard}
        </button>
      </div>
    </ModalDialog>
  );
}
