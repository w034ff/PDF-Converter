import { formatMessage, type Translations } from "../../i18n";
import {
  keyOfPage,
  useAppDispatch,
  type OrganizePage,
  type OrganizePdfEntry,
} from "../../state";
import {
  ORGANIZE_PAGE_BOX_SIDE,
  breakButtonBox,
  breakGap,
  breakMarkLeft,
  cellOffset,
  dropLineLeft,
} from "./organizeGrid";
import { fileStem } from "./pdfNames";

/** How far below the top of a break's line its ✂ mark is (mockup OrganizeSplit). */
const BREAK_MARK_TOP = 50;

interface PageBreakProps {
  /** The page before the break. */
  before: OrganizePage;
  /** The page the break is before. */
  after: OrganizePage;
  /** The index of `after` in the grid (1 or more). */
  index: number;
  columns: number;
  roomWidth: number;
  isBreak: boolean;
  disabled: boolean;
  pdfById: ReadonlyMap<number, OrganizePdfEntry>;
  t: Translations["organize"];
}

/**
 * The place that presses the break before a page, and its line and mark when
 * it is there. The nodes are the grid's own, not in a box of their own.
 */
export function PageBreak({
  before,
  after,
  index,
  columns,
  roomWidth,
  isBreak,
  disabled,
  pdfById,
  t,
}: PageBreakProps) {
  const dispatch = useAppDispatch();
  const key = keyOfPage(after);
  const { row, slot } = breakGap(index, columns);
  const top = cellOffset(row * columns, columns).top;
  const box = breakButtonBox(slot, roomWidth);
  return (
    <>
      <button
        type="button"
        className="organize-break-button"
        aria-pressed={isBreak}
        aria-label={breakLabel(before, after, pdfById, t)}
        tabIndex={-1}
        disabled={disabled}
        style={{
          left: box.left,
          top,
          width: box.width,
          height: ORGANIZE_PAGE_BOX_SIDE,
        }}
        // The press must not take the focus from the page the keys work on.
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => dispatch({ type: "ORGANIZE_BREAK_TOGGLED", key })}
      />
      {isBreak && (
        <>
          <div
            className="organize-break-line"
            aria-hidden="true"
            style={{
              left: dropLineLeft(slot, roomWidth),
              top,
              height: ORGANIZE_PAGE_BOX_SIDE,
            }}
          />
          <span
            className="organize-break-mark"
            aria-hidden="true"
            style={{
              left: breakMarkLeft(slot, roomWidth),
              top: top + BREAK_MARK_TOP,
            }}
          >
            <ScissorsIcon />
          </span>
        </>
      )}
    </>
  );
}

/** Page numbers alone cannot tell the pages of two PDFs apart, so those also get their PDFs' names. */
function breakLabel(
  before: OrganizePage,
  after: OrganizePage,
  pdfById: ReadonlyMap<number, OrganizePdfEntry>,
  t: Translations["organize"],
): string {
  const numbers = { before: before.page, after: after.page };
  if (before.id === after.id) {
    return formatMessage(t.grid.breakAria, numbers);
  }
  return formatMessage(t.grid.breakAriaAcrossPdfs, {
    ...numbers,
    beforeName: fileStem(pdfById.get(before.id)?.name ?? ""),
    afterName: fileStem(pdfById.get(after.id)?.name ?? ""),
  });
}

/** The ✂ of the mark on a break's line (mockup OrganizeSplit). */
function ScissorsIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="6" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M20 4L8.1 15.9" />
      <path d="M14.5 14.5L20 20" />
      <path d="M8.1 8.1L12 12" />
    </svg>
  );
}
