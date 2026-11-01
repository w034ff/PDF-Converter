import { memo, useMemo, useRef } from "react";
import { formatMessage, getTranslations, type Translations } from "../../i18n";
import type { CropPdf } from "../../ipc";
import { useAppDispatch, useAppState } from "../../state";
import { useThumbnail } from "../items/useThumbnail";
import { useRoom } from "../shared/useRoom";
import {
  LIST_THUMBNAIL_DELAY_MS,
  rowOffsets,
  useVirtualRows,
} from "../shared/virtualRows";
import { cropThumbnails } from "./cropImageCaches";
import "./crop.css";

/** Around and between the rows; `.crop-thumb-list` has it too. */
const CROP_THUMB_LIST_PADDING = 12;
const CROP_THUMB_LIST_GAP = 12;

/** Between a page and its number; `.crop-thumb` has it too. */
const CROP_THUMB_NUMBER_GAP = 4;

/** The number's line, which `.crop-thumb-number` fixes. */
const CROP_THUMB_NUMBER_HEIGHT = 16;

interface CropThumbnailProps {
  pdf: CropPdf;
  page: number;
  isCurrent: boolean;
  /** How many areas the page has, the unchecked ones among them. */
  areaCount: number;
  translations: Translations;
}

/** Memoised: a change of page must redraw two rows, not every page of the PDF. */
const CropThumbnail = memo(function CropThumbnail({
  pdf,
  page,
  isCurrent,
  areaCount,
  translations: t,
}: CropThumbnailProps) {
  const dispatch = useAppDispatch();
  const { ref, src } = useThumbnail(pdf.id, page, {
    delayMs: LIST_THUMBNAIL_DELAY_MS,
    cache: cropThumbnails,
  });
  const size = pdf.pages[page - 1];
  const label =
    areaCount === 0
      ? formatMessage(t.crop.pageAria, { page })
      : formatMessage(
          areaCount === 1
            ? t.crop.pageWithAreasSingle
            : t.crop.pageWithAreasMultiple,
          { page, count: areaCount },
        );

  return (
    <li data-virtual-row={page - 1}>
      <button
        type="button"
        className="crop-thumb"
        aria-label={label}
        aria-current={isCurrent ? "page" : undefined}
        onClick={() => dispatch({ type: "CROP_PAGE_SELECTED", page })}
      >
        <span
          ref={ref}
          className={`crop-thumb-frame${isCurrent ? " is-current" : ""}`}
          style={{ aspectRatio: `${size.widthPt} / ${size.heightPt}` }}
        >
          {src !== null && (
            <img className="crop-thumb-image" src={src} alt="" />
          )}
          {areaCount > 0 && (
            <span className="crop-badge crop-thumb-count" aria-hidden="true">
              {areaCount}
            </span>
          )}
        </span>
        <span className="mono crop-thumb-number">{page}</span>
      </button>
    </li>
  );
});

/** How many areas each page has; a page without any is not in the map. */
function countAreasByPage(areas: readonly { page: number }[]) {
  const counts = new Map<number, number>();
  for (const { page } of areas) {
    counts.set(page, (counts.get(page) ?? 0) + 1);
  }
  return counts;
}

/**
 * The column of page thumbnails on the left of the crop tab (mockup
 * `CropA`): a click shows the page, and the page shown has an accent frame.
 * A page with areas has their number at the top right. Each page keeps its
 * own shape, so the rows differ in height; only those near the view are drawn,
 * and each thumbnail is asked for once it comes into view (design §10.1).
 */
export function CropThumbnails({ pdf }: { pdf: CropPdf }) {
  const { language, crop } = useAppState();
  const t = getTranslations(language.language);
  const areaCounts = useMemo(() => countAreasByPage(crop.areas), [crop.areas]);

  const scrollRef = useRef<HTMLElement>(null);
  const room = useRoom(scrollRef);
  const offsets = useMemo(() => {
    const frameWidth = Math.max(room.width - 2 * CROP_THUMB_LIST_PADDING, 0);
    return rowOffsets(
      pdf.pages.length,
      (row) =>
        frameWidth * (pdf.pages[row].heightPt / pdf.pages[row].widthPt) +
        CROP_THUMB_NUMBER_GAP +
        CROP_THUMB_NUMBER_HEIGHT,
      CROP_THUMB_LIST_GAP,
    );
  }, [pdf.pages, room.width]);
  const { first, last, before, after, onScroll } = useVirtualRows(
    scrollRef,
    room,
    offsets,
  );

  const pages: number[] = [];
  for (let page = first + 1; page <= last + 1; page += 1) {
    pages.push(page);
  }

  return (
    <nav
      ref={scrollRef}
      className="crop-thumbs"
      aria-label={t.crop.pagesAria}
      tabIndex={-1}
      onScroll={onScroll}
    >
      <ul
        className="crop-thumb-list"
        style={{
          paddingTop: CROP_THUMB_LIST_PADDING + before,
          paddingBottom: CROP_THUMB_LIST_PADDING + after,
        }}
      >
        {pages.map((page) => (
          <CropThumbnail
            key={page}
            pdf={pdf}
            page={page}
            isCurrent={page === crop.page}
            areaCount={areaCounts.get(page) ?? 0}
            translations={t}
          />
        ))}
      </ul>
    </nav>
  );
}
