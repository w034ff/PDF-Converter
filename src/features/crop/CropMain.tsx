import { useRef, type RefObject } from "react";
import {
  DropZone,
  ErrorDisplay,
  FolderIcon,
  NoticeDisplay,
} from "../../components";
import { formatErrorMessage, getTranslations } from "../../i18n";
import { unlockCropPdf, type CropPdf, type IpcError } from "../../ipc";
import { isJobActive, useAppDispatch, useAppState } from "../../state";
import { openCropPdfFromDialog } from "./cropOpen";
import { useLatestCrop } from "./useLatestCrop";
import { PasswordDialog } from "../pdf-to-images/PasswordDialog";
import { CropPagePane } from "./CropPagePane";
import "./crop.css";

function EmptyIcon() {
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
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <path d="M14 3v6h6" />
      <rect
        x="7.5"
        y="11.5"
        width="9"
        height="6"
        rx="0.5"
        strokeDasharray="2 1.5"
      />
    </svg>
  );
}

/** Why nothing opened, and that layout suggestions are not available. */
function CropNotices() {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const { openNotice } = crop;

  const notices = [];
  if (openNotice?.kind === "error") {
    notices.push(
      <ErrorDisplay
        key="open"
        message={formatErrorMessage(
          openNotice.error.code,
          openNotice.error.detail,
          language.language,
        )}
        onDismiss={() => dispatch({ type: "CROP_OPEN_NOTICE", notice: null })}
        dismissLabel={t.errors.dismiss}
      />,
    );
  } else if (openNotice?.kind === "notOnePdf") {
    notices.push(
      <NoticeDisplay key="open" icon="info" message={t.crop.notOnePdf} />,
    );
  }
  if (crop.layoutUnavailable) {
    notices.push(
      <NoticeDisplay
        key="layout"
        icon="info"
        message={formatErrorMessage(
          "LayoutUnavailable",
          null,
          language.language,
        )}
      />,
    );
  }
  if (notices.length === 0) {
    return null;
  }
  return <div className="crop-notices">{notices}</div>;
}

/**
 * The name of the open PDF and the button that opens another, at the left of
 * the toolbar (design §10.1). The name is cut short with an ellipsis and
 * shows whole on hover.
 */
export function CropFileControls({
  pdf,
  openButtonRef,
}: {
  pdf: CropPdf;
  openButtonRef: RefObject<HTMLButtonElement | null>;
}) {
  const { language, job } = useAppState();
  const dispatch = useAppDispatch();
  const getCrop = useLatestCrop();
  const t = getTranslations(language.language);

  return (
    <div className="crop-file">
      <span className="crop-file-name" title={pdf.name}>
        {pdf.name}
      </span>
      <button
        ref={openButtonRef}
        type="button"
        className="icon-btn crop-file-open"
        aria-label={t.crop.openAnother}
        title={t.crop.openAnother}
        disabled={isJobActive(job)}
        onClick={() => void openCropPdfFromDialog(dispatch, getCrop)}
      >
        <FolderIcon size={16} />
      </button>
    </div>
  );
}

/**
 * How the page shown is going, over the bottom left of the page: only while
 * an area is being drawn, its suggestions are being looked for or are hidden,
 * or something about it failed, so it takes no room the rest of the time
 * (design §10.1).
 */
function CropPageStatus({ pdf }: { pdf: CropPdf }) {
  const { language, crop } = useAppState();
  const t = getTranslations(language.language);
  const { page, layouts, pageImages, layoutUnavailable } = crop;

  const images = pageImages[page];
  const layout = layouts[page];
  const searching =
    pdf.pageCount > 0 &&
    !layoutUnavailable &&
    (layout === undefined || layout.status === "pending");
  const failures: IpcError[] = [];
  if (images?.status === "failed") {
    failures.push(images.error);
  }
  // `LayoutUnavailable` is said once above the page, not on every page.
  if (
    layout?.status === "failed" &&
    layout.error.code !== "LayoutUnavailable"
  ) {
    failures.push(layout.error);
  }
  // Said while hidden, so that a page without frames is not taken for a
  // failed analysis.
  const hidden = !crop.showCandidates && !layoutUnavailable;
  // Drawing has no control of its own on the screen, so this says it is on.
  // A conversion stops it (`JOB_STARTED`), so it is never on while one runs.
  const drawing = crop.tool === "draw";
  if (!drawing && !searching && !hidden && failures.length === 0) {
    return null;
  }

  return (
    <div className="crop-page-status">
      {drawing && <span>{t.crop.drawing}</span>}
      {searching && <span className="hint">{t.crop.searching}</span>}
      {hidden && <span className="hint">{t.crop.candidatesHidden}</span>}
      {failures.map((error, index) => (
        <span key={index} className="error-text">
          {formatErrorMessage(error.code, error.detail, language.language)}
        </span>
      ))}
    </div>
  );
}

export interface CropMainProps {
  /** The toolbar's "別の PDF を開く", where the focus returns after the replace question. */
  openButtonRef: RefObject<HTMLButtonElement | null>;
  /** Whether the page should take the focus (see `CropPagePane`). */
  focusRequested: boolean;
  /** Called when a password dialog unlocked the PDF. */
  onUnlocked: () => void;
  /** Called once the page has taken the focus. */
  onFocused: () => void;
}

/**
 * The middle of the crop tab (mockups `CropEmpty` and `CropA`): a drop zone
 * while no PDF is open, otherwise the toolbar and the page. A PDF that cannot
 * be shown keeps a toolbar of its name and the button to open another.
 */
export function CropMain({
  openButtonRef,
  focusRequested,
  onUnlocked,
  onFocused,
}: CropMainProps) {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const { pdf } = crop;
  const passwordButtonRef = useRef<HTMLButtonElement>(null);
  const getCrop = useLatestCrop();

  if (pdf === null) {
    return (
      <main className="crop-main crop-main-empty">
        <CropNotices />
        <DropZone
          title={t.crop.empty.title}
          description={t.crop.empty.description}
          addFilesLabel={t.crop.empty.open}
          onAddFiles={() => void openCropPdfFromDialog(dispatch, getCrop)}
          icon={<EmptyIcon />}
        />
      </main>
    );
  }

  const needsPassword = pdf.error?.code === "PasswordProtected";
  return (
    <main className="crop-main">
      <CropNotices />
      {pdf.error !== null && (
        <div className="crop-toolbar">
          <CropFileControls pdf={pdf} openButtonRef={openButtonRef} />
        </div>
      )}
      {needsPassword && (
        <div className="crop-notices">
          <NoticeDisplay
            message={formatErrorMessage(
              "PasswordProtected",
              null,
              language.language,
            )}
            action={
              <button
                ref={passwordButtonRef}
                type="button"
                className="btn btn-ghost"
                onClick={() =>
                  dispatch({ type: "CROP_PASSWORD_DIALOG", open: true })
                }
              >
                {t.pdfToImages.enterPassword}
              </button>
            }
          />
        </div>
      )}
      {pdf.error !== null && !needsPassword && (
        <div className="crop-notices">
          <ErrorDisplay
            message={formatErrorMessage(
              pdf.error.code,
              pdf.error.detail,
              language.language,
            )}
          />
        </div>
      )}
      {pdf.error === null && (
        <>
          <CropPagePane
            pdf={pdf}
            fileControls={
              <CropFileControls pdf={pdf} openButtonRef={openButtonRef} />
            }
            focusRequested={focusRequested}
            onFocused={onFocused}
          />
          <CropPageStatus pdf={pdf} />
        </>
      )}
      {crop.passwordDialogOpen && (
        <PasswordDialog
          name={pdf.name}
          unlock={(password) => unlockCropPdf(pdf.id, password)}
          onClose={() =>
            dispatch({ type: "CROP_PASSWORD_DIALOG", open: false })
          }
          onSuccess={(unlocked) => {
            onUnlocked();
            dispatch({ type: "CROP_PDF_UNLOCKED", pdf: unlocked });
          }}
          triggerRef={passwordButtonRef}
        />
      )}
    </main>
  );
}
