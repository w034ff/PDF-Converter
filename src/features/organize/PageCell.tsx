import { memo, type PointerEvent as ReactPointerEvent } from "react";
import { formatMessage, type Translations } from "../../i18n";
import type { PageSizePt } from "../../ipc";
import {
  keyOfPage,
  type OrganizePage,
  type OrganizePdfEntry,
} from "../../state";
import { useThumbnail } from "../items/useThumbnail";
import { fileStem } from "./pdfNames";
import {
  ORGANIZE_CELL_WIDTH,
  ORGANIZE_THUMBNAIL_DELAY_MS,
  thumbnailBox,
} from "./organizeGrid";

/** Stands in for the size of a page of a PDF that is gone, which no cell is drawn for. */
const UNKNOWN_PAGE_SIZE: PageSizePt = { widthPt: 1, heightPt: 1 };

export interface PageCellProps {
  page: OrganizePage;
  /** The place in the grid, from 0. */
  index: number;
  count: number;
  pdf: OrganizePdfEntry | undefined;
  selected: boolean;
  /** Whether Tab stops here (a roving tabindex: one cell does). */
  tabStop: boolean;
  left: number;
  top: number;
  t: Translations["organize"];
  onPointerDown: (
    event: ReactPointerEvent<HTMLDivElement>,
    page: OrganizePage,
  ) => void;
}

/**
 * One page of the grid (mockup OrganizeSingle): its thumbnail turned the way
 * the page is, the angle if it is, the PDF it comes from by a dot and its
 * name, and the page number in that PDF. The picture is asked for after
 * `ORGANIZE_THUMBNAIL_DELAY_MS`, and not at all if the cell is gone by then.
 */
export const PageCell = memo(function PageCell({
  page,
  index,
  count,
  pdf,
  selected,
  tabStop,
  left,
  top,
  t,
  onPointerDown,
}: PageCellProps) {
  const { ref, src } = useThumbnail(page.id, page.page, {
    delayMs: ORGANIZE_THUMBNAIL_DELAY_MS,
  });
  const box = thumbnailBox(
    pdf?.pages[page.page - 1] ?? UNKNOWN_PAGE_SIZE,
    page.rotation,
  );
  const name = pdf?.name ?? "";
  const label = formatMessage(
    page.rotation === 0 ? t.grid.cellAria : t.grid.cellAriaRotated,
    { name, page: page.page, degrees: page.rotation },
  );

  return (
    <div
      role="option"
      aria-selected={selected}
      aria-setsize={count}
      aria-posinset={index + 1}
      aria-label={label}
      tabIndex={tabStop ? 0 : -1}
      data-page-key={keyOfPage(page)}
      className={`organize-cell${selected ? " is-selected" : ""}`}
      style={{ left, top, width: ORGANIZE_CELL_WIDTH }}
      onPointerDown={(event) => onPointerDown(event, page)}
    >
      <div className="organize-cell-box" ref={ref}>
        <div
          className="organize-thumb"
          style={{ width: box.width, height: box.height }}
        >
          {src !== null && (
            <img
              className="organize-thumb-image"
              src={src}
              alt=""
              draggable={false}
              style={{
                width: box.pictureWidth,
                height: box.pictureHeight,
                transform: `translate(-50%, -50%) rotate(${page.rotation}deg)`,
              }}
            />
          )}
        </div>
        {page.rotation !== 0 && (
          <span className="mono organize-cell-angle">
            {formatMessage(t.grid.rotation, { degrees: page.rotation })}
          </span>
        )}
        {selected && (
          <span className="organize-cell-check" aria-hidden="true">
            ✓
          </span>
        )}
      </div>
      <div className="organize-cell-caption" aria-hidden="true">
        <span
          className="organize-dot"
          style={{
            background: `var(--organize-source-${(pdf?.colorIndex ?? 0) + 1})`,
          }}
        />
        <span className="organize-cell-page">
          {formatMessage(t.grid.pageLabel, { page: page.page })}
        </span>
        <span className="mono organize-cell-name">{fileStem(name)}</span>
      </div>
    </div>
  );
});
