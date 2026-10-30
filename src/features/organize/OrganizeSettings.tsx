import { useId } from "react";
import {
  LockIcon,
  SegmentedControl,
  type SegmentedOption,
} from "../../components";
import { getTranslations } from "../../i18n";
import type { OrganizeOutput } from "../../ipc";
import { isJobActive, useAppDispatch, useAppState } from "../../state";
import { OutputDirField } from "../output";
import "./organize.css";

/**
 * The settings panel of the organize tab (mockups OrganizeSingle,
 * OrganizeSplit and OrganizeSplit960): how the pages are saved, and for the
 * save in parts where the breaks are and which folder they go to; whether the
 * saved PDFs get a password, a notice that the passwords of the PDFs put in
 * are not kept, and what the saved PDF is like.
 */
export function OrganizeSettings() {
  const { organize, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const checkId = useId();
  const hasEncrypted = organize.pdfs.some((pdf) => pdf.encrypted);
  const locked = isJobActive(job);
  const isSplit = organize.output === "split";
  const noPages = organize.pages.length === 0;

  const outputOptions: readonly SegmentedOption<OrganizeOutput>[] = [
    { value: "single", label: t.organize.settings.outputSingle },
    { value: "split", label: t.organize.settings.outputSplit },
  ];

  return (
    <div className="organize-settings">
      <div className="organize-settings-field">
        <span className="label">{t.organize.settings.outputTitle}</span>
        <SegmentedControl
          label={t.organize.settings.outputTitle}
          options={outputOptions}
          value={organize.output}
          onChange={(output) =>
            dispatch({ type: "SET_ORGANIZE_OUTPUT", output })
          }
          disabled={locked}
        />
        <span className="hint">
          {isSplit
            ? t.organize.settings.outputSplitHint
            : t.organize.settings.outputSingleHint}
        </span>
      </div>
      {isSplit && (
        <>
          <div className="organize-settings-field">
            <span className="label">{t.organize.settings.breaksTitle}</span>
            <span className="hint">{t.organize.settings.breaksHint}</span>
            <div className="organize-settings-buttons">
              <button
                type="button"
                className="btn btn-ghost"
                disabled={noPages || locked}
                onClick={() => dispatch({ type: "ORGANIZE_BREAKS_EACH_PAGE" })}
              >
                {t.organize.settings.breaksEachPage}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                disabled={noPages || locked}
                onClick={() => dispatch({ type: "ORGANIZE_BREAKS_EACH_PDF" })}
              >
                {t.organize.settings.breaksEachPdf}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                disabled={noPages || locked || organize.breaks.size === 0}
                onClick={() => dispatch({ type: "ORGANIZE_BREAKS_CLEARED" })}
              >
                {t.organize.settings.breaksClear}
              </button>
            </div>
          </div>
          <OutputDirField
            kind="organize"
            value={organize.outputDir}
            onChange={(outputDir) =>
              dispatch({ type: "SET_ORGANIZE_OUTPUT_DIR", outputDir })
            }
            disabled={locked}
          />
        </>
      )}
      <div className="organize-settings-field">
        <label className="organize-check" htmlFor={checkId}>
          <input
            id={checkId}
            type="checkbox"
            checked={organize.encrypt}
            disabled={locked}
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
