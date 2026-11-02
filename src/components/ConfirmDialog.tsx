import { useId, useRef, type ReactNode, type RefObject } from "react";
import { ModalDialog } from "./ModalDialog";
import "./ConfirmDialog.css";

export interface ConfirmDialogProps {
  title: string;
  /** What the question is about, such as a file name, under the title. */
  detail?: string;
  /** The question itself. */
  children: ReactNode;
  cancelLabel: string;
  confirmLabel: string;
  /** Called by the cancel button, Esc and a press outside the dialog. */
  onCancel: () => void;
  onConfirm: () => void;
  /**
   * Where the focus goes when the dialog closes; without it, to what had it
   * when the dialog opened.
   */
  returnFocusRef?: RefObject<HTMLElement | null>;
}

/**
 * Asks before something that cannot be undone (design §10.1). The focus starts
 * on the cancel button, so that holding Enter down does not lose any work.
 */
export function ConfirmDialog({
  title,
  detail,
  children,
  cancelLabel,
  confirmLabel,
  onCancel,
  onConfirm,
  returnFocusRef,
}: ConfirmDialogProps) {
  const titleId = useId();
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <ModalDialog
      isOpen={true}
      onClose={onCancel}
      titleId={titleId}
      dialogClassName="modal-dialog confirm-dialog"
      initialFocusRef={cancelRef}
      returnFocusRef={returnFocusRef}
    >
      <div className="confirm-dialog-header">
        <h2 id={titleId} className="modal-title">
          {title}
        </h2>
        {detail !== undefined && (
          <div className="confirm-dialog-detail mono">{detail}</div>
        )}
      </div>
      <p className="confirm-dialog-body">{children}</p>
      <div className="confirm-dialog-actions">
        <button
          ref={cancelRef}
          type="button"
          className="btn btn-ghost"
          onClick={onCancel}
        >
          {cancelLabel}
        </button>
        <button type="button" className="btn btn-primary" onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </ModalDialog>
  );
}
