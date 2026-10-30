import { useId } from "react";
import { LockIcon } from "../../components";
import { getTranslations } from "../../i18n";
import { isJobActive, useAppDispatch, useAppState } from "../../state";
import "./organize.css";

/**
 * The settings panel of the organize tab (mockups OrganizeEmpty and
 * OrganizeSingle): whether the saved PDF gets a password, a notice that the
 * passwords of the PDFs put in are not kept, and what the saved PDF is like.
 * The choice between one PDF and parts comes with the save in parts.
 */
export function OrganizeSettings() {
  const { organize, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const checkId = useId();
  const hasEncrypted = organize.pdfs.some((pdf) => pdf.encrypted);

  return (
    <div className="organize-settings">
      <div className="organize-settings-field">
        <label className="organize-check" htmlFor={checkId}>
          <input
            id={checkId}
            type="checkbox"
            checked={organize.encrypt}
            disabled={isJobActive(job)}
            onChange={(event) =>
              dispatch({
                type: "SET_ORGANIZE_ENCRYPT",
                encrypt: event.target.checked,
              })
            }
          />
          {t.organize.settings.password}
        </label>
        {hasEncrypted && (
          <div className="organize-settings-notice" role="note">
            <span
              role="img"
              aria-label={t.organize.chip.passwordProtected}
              className="organize-settings-notice-icon"
            >
              <LockIcon size={12} />
            </span>
            <span className="hint">{t.organize.settings.encryptedNotice}</span>
          </div>
        )}
      </div>
      <div className="organize-settings-about">
        <span className="label">{t.organize.settings.aboutTitle}</span>
        <span className="hint">{t.organize.settings.about}</span>
      </div>
    </div>
  );
}
