import { useRef, useState, type FormEvent, type RefObject } from "react";
import { ModalDialog } from "../../components";
import { formatErrorMessage, getTranslations } from "../../i18n";
import { normalizeIpcError } from "../../ipc";
import { useAppState } from "../../state";

export interface PasswordDialogProps<T> {
  /** The file name shown under the title. */
  name: string;
  /**
   * Tries `password` on the file. It resolves with what the caller needs to
   * show the unlocked file, and rejects with `PasswordProtected` for a wrong
   * password (design §6.8).
   */
  unlock: (password: string) => Promise<T>;
  onClose: () => void;
  onSuccess: (result: T) => void;
  triggerRef?: RefObject<HTMLElement | null>;
}

/**
 * Dialog for unlocking a password-protected PDF (design §10.1, §6.8), shared
 * by the lists (`unlock_pdf`) and the crop tab (`unlock_crop_pdf`), which
 * differ in the command and in what it returns. It is open while it is
 * rendered; it keeps the password input to itself and drops it on unmount.
 */
export function PasswordDialog<T>({
  name,
  unlock,
  onClose,
  onSuccess,
  triggerRef,
}: PasswordDialogProps<T>) {
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
      const result = await unlock(password);
      onSuccess(result);
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
      <div className="password-dialog-header">
        <h2 id="password-dialog-title" className="modal-title">
          {t.pdfToImages.passwordDialog.title}
        </h2>
        <div className="password-dialog-filename mono">{name}</div>
      </div>
      <form onSubmit={handleSubmit} className="password-dialog-form">
        <div className="password-dialog-body">
          <label htmlFor="password-dialog-input" className="label">
            {t.pdfToImages.passwordDialog.passwordLabel}
          </label>
          <div className="password-dialog-field">
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
              className="password-dialog-toggle"
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
      </form>
    </ModalDialog>
  );
}
