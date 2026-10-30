import { LockIcon } from "../../components";
import { formatErrorMessage, formatMessage, getTranslations } from "../../i18n";
import { isJobActive, useAppState, type OrganizePdfEntry } from "../../state";
import { pluralize } from "../job/jobSummary";
import "./organize.css";

export interface OrganizePdfChipsProps {
  onRemove: (pdf: OrganizePdfEntry) => void;
  /**
   * Opens the password dialog for a PDF that waits for one, given the button
   * pressed so the focus can go back to it.
   */
  onEnterPassword: (pdf: OrganizePdfEntry, button: HTMLElement) => void;
}

/**
 * The PDFs of the list, one chip each, in one row that scrolls sideways
 * (organize §4). A chip has the color of its PDF, the name, a key if the PDF
 * needed a password, and the number of pages the PDF has (not the number left
 * in the grid). A PDF that waits for its password is yellow with the button
 * to enter it, and one that cannot be read is red with the reason as its title.
 */
export function OrganizePdfChips({
  onRemove,
  onEnterPassword,
}: OrganizePdfChipsProps) {
  const { organize, job, language } = useAppState();
  const t = getTranslations(language.language).organize;
  const locked = isJobActive(job);

  return (
    <div role="list" aria-label={t.pdfListAria} className="organize-chips">
      {organize.pdfs.map((pdf) => {
        const waiting = pdf.error?.code === "PasswordProtected";
        const broken = pdf.error !== null && !waiting;
        return (
          <div
            key={pdf.id}
            role="listitem"
            className={`organize-chip${waiting ? " is-waiting" : ""}${broken ? " is-broken" : ""}`}
            title={
              broken && pdf.error !== null
                ? formatErrorMessage(
                    pdf.error.code,
                    pdf.error.detail,
                    language.language,
                  )
                : undefined
            }
          >
            <span
              className="organize-dot"
              style={{
                background: `var(--organize-source-${pdf.colorIndex + 1})`,
              }}
            />
            <span className="organize-chip-name">{pdf.name}</span>
            {pdf.encrypted && (
              <span
                role="img"
                aria-label={t.chip.passwordProtected}
                className="organize-chip-lock"
              >
                <LockIcon size={12} />
              </span>
            )}
            {waiting ? (
              <button
                type="button"
                className="organize-chip-password"
                disabled={locked}
                onClick={(event) => onEnterPassword(pdf, event.currentTarget)}
              >
                {t.chip.enterPassword}
              </button>
            ) : broken ? (
              <span className="organize-chip-error">
                <span aria-hidden="true">✕ </span>
                {t.chip.unreadable}
              </span>
            ) : (
              <span className="mono">
                {pluralize(
                  pdf.pageCount,
                  t.chip.pagesSingle,
                  t.chip.pagesMultiple,
                )}
              </span>
            )}
            <button
              type="button"
              className="organize-chip-remove"
              aria-label={formatMessage(t.chip.removeAria, { name: pdf.name })}
              disabled={locked}
              onClick={() => onRemove(pdf)}
            >
              ✕
            </button>
          </div>
        );
      })}
    </div>
  );
}
