import { useId } from "react";
import {
  formatErrorMessage,
  formatMessage,
  getTranslations,
  type Language,
  type Translations,
} from "../../i18n";
import type { CropPdf } from "../../ipc";
import {
  isJobActive,
  jobRowStatus,
  useAppDispatch,
  useAppState,
  type AppState,
  type CropArea,
} from "../../state";
import {
  CROP_PREVIEW_HEIGHT,
  CROP_PREVIEW_WIDTH,
  areaPreviewMaxSide,
  areasInListOrder,
  exportSize,
  exportSizeText,
  previewPlacement,
} from "./cropAreas";
import { useAreaPreview } from "./useAreaPreview";
import "./crop.css";

/** How a row says what became of its area in the last export. */
interface RowStatus {
  text: string;
  tone: "ok" | "failed" | "muted" | "neutral";
  reason: string | null;
}

/**
 * The status of the row of area `key` (design §10.1). A row the last export
 * included reads like the rows of the other tabs, but its success is
 * "exported", and "running" reads as waiting: Rust writes a page's areas in
 * position order, which the screen cannot map to its rows, so it does not
 * point at one. A row an earlier export wrote reads "exported" too.
 */
function rowStatus(
  state: Pick<AppState, "crop" | "job">,
  t: Translations,
  language: Language,
  key: number,
): RowStatus | null {
  const { crop, job } = state;
  const status =
    job.kind === "crop" && crop.lastJob === "export"
      ? jobRowStatus(job, key)
      : null;
  if (status === null) {
    return crop.exportedKeys.includes(key)
      ? { text: t.crop.ranges.exported, tone: "ok", reason: null }
      : null;
  }
  switch (status) {
    case "ok":
      return { text: t.crop.ranges.exported, tone: "ok", reason: null };
    case "running":
    case "waiting":
      return { text: t.job.rowStatus.waiting, tone: "muted", reason: null };
    case "unprocessed":
      return { text: t.job.rowStatus.unprocessed, tone: "muted", reason: null };
    case "cancelled":
    case "noPages":
      return { text: t.job.rowStatus[status], tone: "neutral", reason: null };
    case "failed":
    case "partial": {
      const error = job.results[key]?.error;
      return {
        text: t.job.rowStatus[status],
        tone: "failed",
        reason:
          error === undefined
            ? null
            : formatErrorMessage(error.code, error.detail, language),
      };
    }
  }
}

interface AreaPreviewProps {
  pdf: CropPdf;
  area: CropArea;
}

/** The area cut out of its page's picture, which fills the box of the row. */
function AreaPreview({ pdf, area }: AreaPreviewProps) {
  const pageSize = pdf.pages[area.page - 1];
  const { ref, src } = useAreaPreview(
    pdf.id,
    area.page,
    areaPreviewMaxSide(area, pageSize, window.devicePixelRatio),
  );
  const placement = previewPlacement(area.rectPt, pageSize);
  return (
    <span
      ref={ref}
      className="crop-area-preview"
      style={{ width: CROP_PREVIEW_WIDTH, height: CROP_PREVIEW_HEIGHT }}
    >
      <span className="crop-area-preview-window" style={placement.window}>
        {src !== null && (
          <img
            className="crop-area-preview-image"
            src={src}
            alt=""
            style={placement.image}
          />
        )}
      </span>
    </span>
  );
}

interface CropAreaRowProps {
  pdf: CropPdf;
  area: CropArea;
}

function CropAreaRow({ pdf, area }: CropAreaRowProps) {
  const state = useAppState();
  const dispatch = useAppDispatch();
  const detailsId = useId();
  const { language, crop, job } = state;
  const t = getTranslations(language.language);
  const busy = isJobActive(job);
  const kind = t.crop.areaKinds[area.kind];
  const names = { page: area.page, kind };

  const size = exportSize(area, crop.pageImages[area.page], crop);
  const status = rowStatus(state, t, language.language, area.key);
  const selected = crop.selectedKey === area.key;

  return (
    <li
      className={`crop-area-row${selected ? " is-selected" : ""}`}
      data-testid="crop-area-row"
      data-key={area.key}
    >
      <input
        type="checkbox"
        className="crop-area-check"
        checked={area.checked}
        disabled={busy}
        aria-label={formatMessage(t.crop.ranges.checkAria, names)}
        onChange={(event) =>
          dispatch({
            type: "CROP_AREA_CHECKED",
            key: area.key,
            checked: event.target.checked,
          })
        }
      />
      <button
        type="button"
        className="crop-area-main"
        aria-label={formatMessage(t.crop.ranges.showAria, names)}
        aria-describedby={detailsId}
        aria-current={selected ? "true" : undefined}
        onClick={() => {
          dispatch({ type: "CROP_PAGE_SELECTED", page: area.page });
          dispatch({ type: "CROP_AREA_SELECTED", key: area.key });
        }}
      >
        <AreaPreview pdf={pdf} area={area} />
        <span id={detailsId} className="crop-area-text">
          <span className="crop-area-head">
            <span className="mono crop-area-page">
              {formatMessage(t.crop.ranges.pageLabel, { page: area.page })}
            </span>
            <span className="crop-area-kind">{kind}</span>
            {area.onTop === true && (
              <span className="crop-badge crop-area-overlaid">
                {t.crop.ranges.overlaid}
              </span>
            )}
          </span>
          {size !== null && (
            <span className="mono crop-area-size">
              {exportSizeText(t, size)}
            </span>
          )}
          {status !== null && (
            <span className={`crop-area-status is-${status.tone}`}>
              {status.text}
            </span>
          )}
          {status !== null && status.reason !== null && (
            <span className="crop-area-reason">{status.reason}</span>
          )}
        </span>
      </button>
      <button
        type="button"
        className="icon-btn crop-area-remove"
        aria-label={formatMessage(t.crop.ranges.removeAria, names)}
        disabled={busy}
        onClick={() => dispatch({ type: "CROP_AREA_REMOVED", key: area.key })}
      >
        {t.crop.ranges.remove}
      </button>
    </li>
  );
}

/**
 * The rows of the areas (mockup `CropA`): by page and, within a page, in the
 * order they were added (design §10.1).
 */
export function CropAreaList({ pdf }: { pdf: CropPdf }) {
  const { crop } = useAppState();
  return (
    <ul className="crop-area-list">
      {areasInListOrder(crop.areas).map((area) => (
        <CropAreaRow key={area.key} pdf={pdf} area={area} />
      ))}
    </ul>
  );
}
