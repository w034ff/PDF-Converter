import { useId, useRef, useState, type FormEvent } from "react";
import { ModalDialog } from "../../components";
import { formatMessage, getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import {
  hasOnlyAllowedCharacters,
  isOutputPasswordTooLong,
  OUTPUT_PASSWORD_MAX_LENGTH,
} from "./outputPassword";
// The card, the fields and the toggle are those of the dialog that asks for a PDF's password.
import "../pdf-to-images/pdfToImages.css";
import "./ImagesToPdf.css";

/** How one line of the conditions list stands (design §10.1). */
type ConditionState = "pending" | "met" | "failed";

const CONDITION_SYMBOLS: Record<ConditionState, string> = {
  pending: "○",
  met: "✓",
  failed: "✕",
};

function charactersCondition(password: string): ConditionState {
  if (password === "") {
    return "pending";
  }
  return hasOnlyAllowedCharacters(password) ? "met" : "failed";
}

/**
 * The match is not judged while the confirmation is still a start of the password, so typing
 * does not turn it red on the way.
 */
function matchCondition(
  password: string,
  confirmation: string,
): ConditionState {
  if (confirmation === "") {
    return "pending";
  }
  if (confirmation === password) {
    return "met";
  }
  return password.startsWith(confirmation) ? "pending" : "failed";
}

export interface OutputPasswordDialogProps {
  isOpen: boolean;
  /** "merge" asks for one PDF's password; "each" for the password of `count` PDFs. */
  mode: "merge" | "each";
  /** The number of PDFs "each" will save, as the footer says it. */
  count: number;
  onCancel: () => void;
  /** Called with the password, which the dialog no longer holds once it closes. */
  onConfirm: (password: string) => void;
  returnFocusRef?: React.RefObject<HTMLElement | null>;
}

interface ConditionProps {
  id: string;
  state: ConditionState;
  label: string;
  metText: string;
  notMetText: string;
}

function Condition({ id, state, label, metText, notMetText }: ConditionProps) {
  return (
    <li id={id} className={`output-password-condition is-${state}`}>
      <span aria-hidden="true" className="output-password-symbol">
        {CONDITION_SYMBOLS[state]}
      </span>
      {label}
      {state !== "pending" && (
        <span className="output-password-sr-only">
          {state === "met" ? metText : notMetText}
        </span>
      )}
    </li>
  );
}

function OutputPasswordDialogInner({
  mode,
  count,
  onCancel,
  onConfirm,
  returnFocusRef,
}: Omit<OutputPasswordDialogProps, "isOpen">) {
  const { language } = useAppState();
  const t = getTranslations(language.language).imagesToPdf.passwordDialog;
  const ids = useId();
  const titleId = `${ids}-title`;
  const passwordId = `${ids}-password`;
  const confirmId = `${ids}-confirm`;
  const charactersId = `${ids}-characters`;
  const matchId = `${ids}-match`;
  const tooLongId = `${ids}-too-long`;

  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const passwordRef = useRef<HTMLInputElement>(null);

  const characters = charactersCondition(password);
  const match = matchCondition(password, confirmation);
  const tooLong = isOutputPasswordTooLong(password);
  const canConfirm = characters === "met" && match === "met" && !tooLong;

  const passwordDescribedBy = [
    characters === "failed" ? charactersId : null,
    tooLong ? tooLongId : null,
  ].filter((id) => id !== null);
  const confirmDescribedBy = match === "failed" ? [matchId] : [];

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (canConfirm) {
      onConfirm(password);
    }
  }

  const notice =
    mode === "each"
      ? formatMessage(count === 1 ? t.eachNoticeSingle : t.eachNoticeMultiple, {
          count,
        })
      : null;
  const inputType = showPassword ? "text" : "password";

  return (
    <ModalDialog
      isOpen={true}
      onClose={onCancel}
      titleId={titleId}
      dialogClassName="modal-dialog password-dialog"
      initialFocusRef={passwordRef}
      returnFocusRef={returnFocusRef}
    >
      <div className="password-dialog-header">
        <h2 id={titleId} className="modal-title">
          {t.title}
        </h2>
        {notice !== null && <div className="hint">{notice}</div>}
      </div>
      <form onSubmit={handleSubmit} className="password-dialog-form">
        <div className="password-dialog-body">
          <label htmlFor={passwordId} className="label">
            {t.passwordLabel}
          </label>
          <div className="password-dialog-field">
            <input
              id={passwordId}
              ref={passwordRef}
              type={inputType}
              autoComplete="off"
              value={password}
              aria-describedby={passwordDescribedBy.join(" ") || undefined}
              onChange={(event) => setPassword(event.target.value)}
              className="settings-control password-dialog-input"
            />
            <button
              type="button"
              className="password-dialog-toggle"
              aria-pressed={showPassword}
              onClick={() => setShowPassword((previous) => !previous)}
            >
              {showPassword ? t.hidePassword : t.showPassword}
            </button>
          </div>
          {tooLong && (
            <div id={tooLongId} className="error-text">
              {formatMessage(t.tooLong, { max: OUTPUT_PASSWORD_MAX_LENGTH })}
            </div>
          )}
        </div>
        <div className="password-dialog-body">
          <label htmlFor={confirmId} className="label">
            {t.confirmLabel}
          </label>
          <input
            id={confirmId}
            type={inputType}
            autoComplete="off"
            value={confirmation}
            aria-describedby={confirmDescribedBy.join(" ") || undefined}
            onChange={(event) => setConfirmation(event.target.value)}
            className="settings-control password-dialog-input output-password-confirm"
          />
        </div>
        <ul className="output-password-conditions">
          <Condition
            id={charactersId}
            state={characters}
            label={t.conditionCharacters}
            metText={t.conditionMet}
            notMetText={t.conditionNotMet}
          />
          <Condition
            id={matchId}
            state={match}
            label={t.conditionMatch}
            metText={t.conditionMet}
            notMetText={t.conditionNotMet}
          />
        </ul>
        <div className="hint">
          {mode === "each" ? t.forgetWarningEach : t.forgetWarningMerge}
        </div>
        <div className="password-dialog-actions">
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            {t.cancel}
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={!canConfirm}
          >
            {mode === "each" ? t.confirmEach : t.confirmMerge}
          </button>
        </div>
      </form>
    </ModalDialog>
  );
}

/**
 * Asks for the password of the PDFs about to be written (design §6.2, §10.1). The password
 * lives in the open dialog only: closing it, either way, drops it.
 */
export function OutputPasswordDialog({
  isOpen,
  ...props
}: OutputPasswordDialogProps) {
  if (!isOpen) {
    return null;
  }
  return <OutputPasswordDialogInner {...props} />;
}
