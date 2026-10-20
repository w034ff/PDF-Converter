import { useRef } from "react";
import { DropZone, ErrorDisplay, NoticeDisplay } from "../../components";
import { formatErrorMessage, formatMessage, getTranslations } from "../../i18n";
import { unlockCropPdf, type CropPdf, type IpcError } from "../../ipc";
import { useAppDispatch, useAppState } from "../../state";
import { openCropPdfFromDialog } from "./cropOpen";
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
    notices.push(<NoticeDisplay key="open" message={t.crop.notOnePdf} />);
  }
  if (crop.layoutUnavailable) {
    notices.push(
      <NoticeDisplay
        key="layout"
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

/** The line above the page: file name, page number, how the page is going. */
function CropBand({ pdf }: { pdf: CropPdf }) {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
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

  return (
    <div className="crop-band">
      <span className="crop-band-name">{pdf.name}</span>
      {pdf.pageCount > 0 && (
        <span className="mono crop-band-counter">
          {formatMessage(t.crop.pageCounter, {
            page,
            total: pdf.pageCount,
          })}
        </span>
      )}
      {searching && <span className="hint">{t.crop.searching}</span>}
      {failures.map((error, index) => (
        <span key={index} className="error-text">
          {formatErrorMessage(error.code, error.detail, language.language)}
        </span>
      ))}
      <button
        type="button"
        className="btn btn-ghost crop-band-open"
        onClick={() => void openCropPdfFromDialog(dispatch)}
      >
        {t.crop.openAnother}
      </button>
    </div>
  );
}

export interface CropMainProps {
  /** Whether the page should take the focus (see `CropPagePane`). */
  focusRequested: boolean;
  /** Called when a password dialog unlocked the PDF. */
  onUnlocked: () => void;
  /** Called once the page has taken the focus. */
  onFocused: () => void;
}

/**
 * The middle of the crop tab (mockups `CropEmpty` and `CropA`): a drop zone
 * while no PDF is open, otherwise the PDF's band and its page.
 */
export function CropMain({
  focusRequested,
  onUnlocked,
  onFocused,
}: CropMainProps) {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const { pdf } = crop;
  const passwordButtonRef = useRef<HTMLButtonElement>(null);

  if (pdf === null) {
    return (
      <main className="crop-main crop-main-empty">
        <CropNotices />
        <DropZone
          title={t.crop.empty.title}
          description={t.crop.empty.description}
          addFilesLabel={t.crop.empty.open}
          onAddFiles={() => void openCropPdfFromDialog(dispatch)}
          icon={<EmptyIcon />}
        />
      </main>
    );
  }

  const needsPassword = pdf.error?.code === "PasswordProtected";
  return (
    <main className="crop-main">
      <CropNotices />
      <CropBand pdf={pdf} />
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
        <CropPagePane
          pdf={pdf}
          focusRequested={focusRequested}
          onFocused={onFocused}
        />
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
