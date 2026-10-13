import { useRef, useState, type FormEvent, type RefObject } from "react";
import { ModalDialog } from "../../components";
import { formatErrorMessage, getTranslations } from "../../i18n";
import { normalizeIpcError, unlockPdf, type PdfItem } from "../../ipc";
import { useAppState } from "../../state";

export interface PasswordDialogProps {
  isOpen: boolean;
  item: PdfItem | null;
  onClose: () => void;
  onSuccess: (items: PdfItem[]) => void;
  triggerRef?: RefObject<HTMLElement | null>;
}

interface PasswordDialogInnerProps {
  item: PdfItem;
  onClose: () => void;
  onSuccess: (items: PdfItem[]) => void;
  triggerRef?: RefObject<HTMLElement | null>;
}

function PasswordDialogInner({
  item,
  onClose,
  onSuccess,
  triggerRef,
}: PasswordDialogInnerProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [errorText, setErrorText] = useState<string | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);

  function handleClose() {
    if (busy) {
      return;
    }
    onClose();
  }

  function handlePasswordChange(e: React.ChangeEvent<HTMLInputElement>) {
    if (errorText !== null) {
      setErrorText(null);
    }
    setPassword(e.target.value);
  }

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (password === "" || busy) {
      return;
    }
    setBusy(true);
    setErrorText(null);

    try {
      const result = await unlockPdf(item.id, password);
      onSuccess(result.items);
    } catch (err: unknown) {
      setBusy(false);
      const ipcErr = normalizeIpcError(err);
      if (ipcErr.code === "PasswordProtected") {
        setErrorText(t.pdfToImages.passwordDialog.incorrectPassword);
      } else {
        setErrorText(
          formatErrorMessage(ipcErr.code, ipcErr.detail, language.language),
        );
      }
      inputRef.current?.select();
      inputRef.current?.focus();
    }
  }

  return (
    <ModalDialog
      isOpen={true}
      onClose={handleClose}
      titleId="password-dialog-title"
      dialogClassName="modal-dialog password-dialog"
      initialFocusRef={inputRef}
      returnFocusRef={triggerRef}
    >
      <div className="modal-header">
        <div className="password-dialog-header-text">
          <h2 id="password-dialog-title" className="modal-title">
            {t.pdfToImages.passwordDialog.title}
          </h2>
          <div className="password-dialog-filename mono">{item.name}</div>
        </div>
      </div>
      <form onSubmit={handleSubmit} className="password-dialog-form">
        <div className="password-dialog-body">
          <label htmlFor="password-dialog-input" className="label">
            {t.pdfToImages.passwordDialog.passwordLabel}
          </label>
          <div className="password-dialog-input-row">
            <input
              id="password-dialog-input"
              ref={inputRef}
              type={showPassword ? "text" : "password"}
              autoComplete="off"
              value={password}
              disabled={busy}
              onChange={handlePasswordChange}
              className="settings-control password-dialog-input"
            />
            <button
              type="button"
              className="btn btn-ghost"
              aria-pressed={showPassword}
              disabled={busy}
              onClick={() => setShowPassword((prev) => !prev)}
            >
              {showPassword
                ? t.pdfToImages.passwordDialog.hidePassword
                : t.pdfToImages.passwordDialog.showPassword}
            </button>
          </div>
          {errorText !== null && (
            <div className="error-text" role="alert">
              {errorText}
            </div>
          )}
        </div>
        <div className="modal-footer">
          <div className="password-dialog-actions">
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={handleClose}
            >
              {t.pdfToImages.passwordDialog.cancel}
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={password === "" || busy}
            >
              {t.pdfToImages.passwordDialog.open}
            </button>
          </div>
        </div>
      </form>
    </ModalDialog>
  );
}

/**
 * Dialog for unlocking a password-protected PDF (design §10.1, §6.8).
 * Manages password input state locally and discards it on close.
 */
export function PasswordDialog({
  isOpen,
  item,
  onClose,
  onSuccess,
  triggerRef,
}: PasswordDialogProps) {
  if (!isOpen || item === null) {
    return null;
  }

  return (
    <PasswordDialogInner
      item={item}
      onClose={onClose}
      onSuccess={onSuccess}
      triggerRef={triggerRef}
    />
  );
}
