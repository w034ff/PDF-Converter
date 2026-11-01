import { forwardRef, type ReactNode } from "react";
import { getTranslations } from "../../i18n";
import { isJobActive, useAppDispatch, useAppState } from "../../state";
import { pluralize } from "../job/jobSummary";
import "./organize.css";

export interface OrganizeBandProps {
  onShowKeyHints: () => void;
  /** The delete button goes out of use with the focus on it. */
  onDelete: () => void;
  /** What works on the list of PDFs, at the end of the band. */
  actions: ReactNode;
}

function RotateLeftIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4.2 16.5a9 9 0 1 0 1.8-11.2" />
      <path d="M3 4v5h5" />
    </svg>
  );
}

function RotateRightIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M19.8 16.5a9 9 0 1 1-1.8-11.2" />
      <path d="M21 4v5h-5" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M6 6l1 14h10l1-14" />
    </svg>
  );
}

/**
 * The band above the grid (mockup OrganizeSingle): how many pages there are
 * or are selected, the buttons that turn or delete the selected pages, and
 * the one that lists the keys. The buttons stay when nothing is selected, and
 * are pressed in then, so the grid under the band does not move. Where the
 * band is narrow they keep their icons alone, with their names as tooltips.
 */
export const OrganizeBand = forwardRef<HTMLButtonElement, OrganizeBandProps>(
  function OrganizeBand({ onShowKeyHints, onDelete, actions }, hintsRef) {
    const { organize, job, language } = useAppState();
    const dispatch = useAppDispatch();
    const t = getTranslations(language.language).organize.band;
    const selectedCount = organize.selected.size;
    const idle = selectedCount === 0 || isJobActive(job);
    const keys = [...organize.selected];

    return (
      <div className="organize-band">
        <span className="organize-band-count">
          {selectedCount === 0
            ? pluralize(
                organize.pages.length,
                t.countNoneSingle,
                t.countNoneMultiple,
              )
            : pluralize(
                selectedCount,
                t.countSelectedSingle,
                t.countSelectedMultiple,
              )}
        </span>
        <button
          type="button"
          className="btn btn-ghost organize-band-button"
          aria-label={t.rotateLeft}
          title={t.rotateLeft}
          disabled={idle}
          onClick={() =>
            dispatch({
              type: "ORGANIZE_PAGES_ROTATED",
              keys,
              direction: "left",
            })
          }
        >
          <RotateLeftIcon />
          <span className="organize-band-label">{t.rotateLeft}</span>
        </button>
        <button
          type="button"
          className="btn btn-ghost organize-band-button"
          aria-label={t.rotateRight}
          title={t.rotateRight}
          disabled={idle}
          onClick={() =>
            dispatch({
              type: "ORGANIZE_PAGES_ROTATED",
              keys,
              direction: "right",
            })
          }
        >
          <RotateRightIcon />
          <span className="organize-band-label">{t.rotateRight}</span>
        </button>
        <button
          type="button"
          className="btn btn-ghost organize-band-button"
          aria-label={t.delete}
          title={t.delete}
          disabled={idle}
          onClick={() => {
            onDelete();
            dispatch({ type: "ORGANIZE_PAGES_DELETED", keys });
          }}
        >
          <TrashIcon />
          <span className="organize-band-label">{t.delete}</span>
        </button>
        <div className="organize-band-actions">{actions}</div>
        <button
          ref={hintsRef}
          type="button"
          className="icon-btn organize-band-hints"
          aria-label={t.keyHints}
          title={t.keyHintsTitle}
          onClick={onShowKeyHints}
        >
          ?
        </button>
      </div>
    );
  },
);
