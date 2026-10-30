import { useRef, type RefObject } from "react";
import { ModalDialog } from "../../components";
import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

export interface OrganizeClearDialogProps {
  onCancel: () => void;
  onConfirm: () => void;
  returnFocusRef: RefObject<HTMLElement | null>;
}

/**
 * Asks whether to remove every PDF after the pages were edited, which loses
 * their order and cannot be undone (organize §4, design §10.1). The focus
 * starts on "キャンセル", so that holding Enter down does not lose the work.
 */
export function OrganizeClearDialog({
  onCancel,
  onConfirm,
  returnFocusRef,
}: OrganizeClearDialogProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language).organize.clearDialog;
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <ModalDialog
      isOpen={true}
      onClose={onCancel}
      titleId="organize-clear-title"
      dialogClassName="modal-dialog password-dialog"
      initialFocusRef={cancelRef}
      returnFocusRef={returnFocusRef}
    >
      <div className="password-dialog-header">
        <h2 id="organize-clear-title" className="modal-title">
          {t.title}
        </h2>
      </div>
      <p className="crop-replace-count">{t.body}</p>
      <div className="password-dialog-actions">
        <button
          ref={cancelRef}
          type="button"
          className="btn btn-ghost"
          onClick={onCancel}
        >
          {t.cancel}
        </button>
        <button type="button" className="btn btn-primary" onClick={onConfirm}>
          {t.confirm}
        </button>
      </div>
    </ModalDialog>
  );
}
