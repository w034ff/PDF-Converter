import { useRef, useState } from "react";
import {
  ChevronDownIcon,
  DropZone,
  ErrorDisplay,
  MenuButton,
  NoticeDisplay,
} from "../../components";
import { formatErrorMessage, getTranslations } from "../../i18n";
import {
  addOrganizePdfs,
  normalizeIpcError,
  removeOrganizePdfs,
  unlockOrganizePdf,
  type AddSource,
} from "../../ipc";
import {
  isJobActive,
  useAppDispatch,
  useAppState,
  type OrganizePdfEntry,
} from "../../state";
import { formatSkippedSummary } from "../pdf-to-images/pdfUtils";
import { PasswordDialog } from "../pdf-to-images/PasswordDialog";
import { KeyHintsDialog } from "../shortcuts/KeyHintsDialog";
import { OrganizeBand } from "./OrganizeBand";
import { OrganizeClearDialog } from "./OrganizeClearDialog";
import { OrganizePdfChips } from "./OrganizePdfChips";
import { noticeOf } from "./organizeNotice";
import { PageGrid } from "./PageGrid";
import { useOrganizeKeys } from "./useOrganizeKeys";
import "./organize.css";

/**
 * The main area of the organize tab (mockups OrganizeEmpty and
 * OrganizeSingle): the drop zone while there is no PDF, then the PDFs as
 * chips, the band over the grid, and the grid of the pages.
 */
export function OrganizeView() {
  const { organize, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const locked = isJobActive(job);

  const [hintsOpen, setHintsOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const [passwordFor, setPasswordFor] = useState<OrganizePdfEntry | null>(null);
  const hintsButtonRef = useRef<HTMLButtonElement>(null);
  const clearButtonRef = useRef<HTMLButtonElement>(null);
  const passwordTriggerRef = useRef<HTMLElement | null>(null);

  const openHints = () => setHintsOpen(true);
  useOrganizeKeys({ onHelp: openHints });

  async function handleAdd(source: AddSource) {
    try {
      const result = await addOrganizePdfs(source);
      if (result !== null) {
        dispatch({ type: "ORGANIZE_PDFS_ADDED", items: result.added });
        dispatch({
          type: "ORGANIZE_NOTICE",
          notice: noticeOf(result.skipped, null),
        });
      }
    } catch (error: unknown) {
      dispatch({
        type: "ORGANIZE_NOTICE",
        notice: noticeOf(null, normalizeIpcError(error)),
      });
    }
  }

  async function removeFromRust(ids: number[]): Promise<boolean> {
    try {
      await removeOrganizePdfs(ids);
      return true;
    } catch (error: unknown) {
      dispatch({
        type: "ORGANIZE_NOTICE",
        notice: noticeOf(null, normalizeIpcError(error)),
      });
      return false;
    }
  }

  async function handleRemove(pdf: OrganizePdfEntry) {
    if (await removeFromRust([pdf.id])) {
      dispatch({ type: "ORGANIZE_PDF_REMOVED", id: pdf.id });
    }
  }

  async function clearAll() {
    setClearOpen(false);
    if (await removeFromRust(organize.pdfs.map((pdf) => pdf.id))) {
      dispatch({ type: "ORGANIZE_CLEARED" });
    }
  }

  function handleClearClick() {
    if (organize.edited) {
      setClearOpen(true);
    } else {
      void clearAll();
    }
  }

  const notice = organize.notice;
  const skippedText =
    notice?.skipped == null ? null : formatSkippedSummary(t, notice.skipped);
  const bands = (
    <>
      {notice?.error != null && (
        <ErrorDisplay
          message={formatErrorMessage(
            notice.error.code,
            notice.error.detail,
            language.language,
          )}
          onDismiss={() =>
            dispatch({
              type: "ORGANIZE_NOTICE",
              notice:
                notice.skipped === null
                  ? null
                  : { skipped: notice.skipped, error: null },
            })
          }
          dismissLabel={t.errors.dismiss}
        />
      )}
      {skippedText !== null && (
        <NoticeDisplay message={skippedText} icon="info" />
      )}
    </>
  );

  if (organize.pdfs.length === 0) {
    return (
      <div className="organize-view">
        {bands}
        <DropZone
          title={t.organize.dropZone.title}
          description={t.organize.dropZone.description}
          addFilesLabel={t.organize.dropZone.addPdfs}
          addFolderLabel={t.organize.dropZone.addFolder}
          onAddFiles={() => void handleAdd("files")}
          onAddFolder={() => void handleAdd("folder")}
          icon={<OrganizeDropIcon />}
        />
      </div>
    );
  }

  return (
    <div className="organize-view">
      {bands}
      <div className="organize-toolbar">
        <OrganizePdfChips
          onRemove={(pdf) => void handleRemove(pdf)}
          onEnterPassword={(pdf, button) => {
            passwordTriggerRef.current = button;
            setPasswordFor(pdf);
          }}
        />
        <div className="organize-toolbar-actions">
          <MenuButton
            className="btn btn-ghost organize-add-menu"
            label={t.organize.addMenu.label}
            menuLabel={t.organize.addMenu.label}
            disabled={locked}
            items={[
              {
                id: "files",
                label: t.organize.addMenu.files,
                onSelect: () => void handleAdd("files"),
              },
              {
                id: "folder",
                label: t.organize.addMenu.folder,
                onSelect: () => void handleAdd("folder"),
              },
            ]}
          >
            {t.organize.addMenu.label}
            <ChevronDownIcon />
          </MenuButton>
          <button
            ref={clearButtonRef}
            type="button"
            className="btn btn-ghost"
            disabled={locked}
            onClick={handleClearClick}
          >
            {t.organize.clearAll}
          </button>
        </div>
      </div>
      <OrganizeBand ref={hintsButtonRef} onShowKeyHints={openHints} />
      {organize.pages.length === 0 ? (
        <div className="organize-grid-empty">
          <span className="label">{t.organize.grid.empty}</span>
          <span className="hint">{t.organize.grid.emptyHint}</span>
        </div>
      ) : (
        <PageGrid />
      )}
      {hintsOpen && (
        <KeyHintsDialog
          hints={t.organize.keyHints}
          titleId="organize-key-hints-title"
          onClose={() => setHintsOpen(false)}
          returnFocusRef={hintsButtonRef}
        />
      )}
      {clearOpen && (
        <OrganizeClearDialog
          onCancel={() => setClearOpen(false)}
          onConfirm={() => void clearAll()}
          returnFocusRef={clearButtonRef}
        />
      )}
      {passwordFor !== null && (
        <PasswordDialog
          name={passwordFor.name}
          unlock={(password) => unlockOrganizePdf(passwordFor.id, password)}
          onClose={() => setPasswordFor(null)}
          onSuccess={(result) => {
            dispatch({ type: "ORGANIZE_PDFS_UNLOCKED", items: result.items });
            setPasswordFor(null);
          }}
          triggerRef={passwordTriggerRef}
        />
      )}
    </div>
  );
}

/** The mark of the empty drop zone (mockup OrganizeEmpty): three pages to arrange. */
function OrganizeDropIcon() {
  return (
    <svg
      className="drop-zone-icon"
      width="48"
      height="48"
      viewBox="0 0 24 24"
      fill="none"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="3" y="4" width="7" height="9" rx="1" />
      <rect x="14" y="4" width="7" height="9" rx="1" />
      <rect x="8.5" y="15" width="7" height="6" rx="1" />
    </svg>
  );
}
