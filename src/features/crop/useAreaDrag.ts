import {
  useEffect,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
  type RefObject,
} from "react";
import type { PageSizePt } from "../../ipc";
import type { CropTool, RectPt } from "../../state";
import {
  drawRect,
  handleCursor,
  isDrawnAreaBigEnough,
  moveRect,
  resizeRect,
  type HandleId,
  type PointPt,
} from "./cropEdit";
import { CROP_DRAG_THRESHOLD_PX, CSS_PX_PER_PT } from "./cropView";
import { historyKeyOf } from "./useCropKeys";

/** `button` of a press with the middle button (the wheel). */
const MIDDLE_BUTTON = 1;

type DragMode =
  | { kind: "pan"; scrollLeft: number; scrollTop: number }
  | { kind: "draw"; start: PointPt }
  | { kind: "move"; key: number; rect: RectPt }
  | { kind: "resize"; key: number; rect: RectPt; handle: HandleId };

interface Drag {
  pointerId: number;
  /** Where the press was, in client pixels. */
  originX: number;
  originY: number;
  /** How far the pointer is from there, in CSS pixels. */
  dx: number;
  dy: number;
  /** Whether the pointer moved `CROP_DRAG_THRESHOLD_PX`; once it did, the press is a drag for good. */
  passed: boolean;
  mode: DragMode;
  /** The page and scale the drag began on; it is called off when either changes. */
  page: number;
  scale: number;
}

/** What a drag in progress shows in place of the area it changes, or as a new area. */
export interface DragPreview {
  /** The `key` of the area being moved or resized; `null` for one being drawn. */
  key: number | null;
  rectPt: RectPt;
  cursor: string;
}

export interface UseAreaDragOptions {
  page: number;
  pageSize: PageSizePt;
  scale: number;
  /** Whether areas cannot be drawn or changed (a conversion runs). */
  locked: boolean;
  /** What a press and a drag do; "draw" draws wherever the press is. */
  tool: CropTool;
  overlayRef: RefObject<HTMLElement | null>;
  /** The scrolling view a drag of the page itself moves. */
  viewport: HTMLElement;
  /** Called on release with the rectangle of a drawn area that is big enough. */
  onDraw: (rectPt: RectPt) => void;
  /** Called on release with the rectangle a moved or resized area ends with. */
  onChange: (key: number, rectPt: RectPt) => void;
  /** Called when a press lands on an area, so it is selected at once. */
  onSelect: (key: number) => void;
}

export interface AreaDrag {
  preview: DragPreview | null;
  /** Whether the page is being dragged to scroll it. */
  panning: boolean;
  /** Spread on the element the page's overlay is drawn in. */
  overlayProps: {
    onPointerDownCapture: (event: PointerEvent<HTMLElement>) => void;
    onMouseDownCapture: (event: MouseEvent<HTMLElement>) => void;
    onPointerDown: (event: PointerEvent<HTMLElement>) => void;
    onPointerMove: (event: PointerEvent<HTMLElement>) => void;
    onPointerUp: (event: PointerEvent<HTMLElement>) => void;
    onPointerCancel: (event: PointerEvent<HTMLElement>) => void;
    onClickCapture: (event: MouseEvent<HTMLElement>) => void;
  };
  /** Press on an area: selects it and starts to move it. */
  beginMove: (
    event: PointerEvent<HTMLElement>,
    key: number,
    rectPt: RectPt,
  ) => void;
  /** Press on a handle of the selected area: starts to resize it. */
  beginResize: (
    event: PointerEvent<HTMLElement>,
    key: number,
    rectPt: RectPt,
    handle: HandleId,
  ) => void;
}

function cursorOf(mode: Exclude<DragMode, { kind: "pan" }>): string {
  switch (mode.kind) {
    case "draw":
      return "crosshair";
    case "move":
      return "move";
    case "resize":
      return handleCursor(mode.handle);
  }
}

/** The shape a drag gives an area; `null` for a drag of the page. */
function previewOf(drag: Drag, pageSize: PageSizePt): DragPreview | null {
  const pxPerPt = CSS_PX_PER_PT * drag.scale;
  const delta = { x: drag.dx / pxPerPt, y: drag.dy / pxPerPt };
  const { mode } = drag;
  switch (mode.kind) {
    case "pan":
      return null;
    case "draw":
      return {
        key: null,
        rectPt: drawRect(
          mode.start,
          { x: mode.start.x + delta.x, y: mode.start.y + delta.y },
          pageSize,
        ),
        cursor: cursorOf(mode),
      };
    case "move":
      return {
        key: mode.key,
        rectPt: moveRect(mode.rect, delta, pageSize),
        cursor: cursorOf(mode),
      };
    case "resize":
      return {
        key: mode.key,
        rectPt: resizeRect(mode.rect, mode.handle, delta, pageSize),
        cursor: cursorOf(mode),
      };
  }
}

/**
 * The pointer side of editing the areas of a page (design §10.1): a press
 * that stays under `CROP_DRAG_THRESHOLD_PX` is left to the `click` of what it
 * landed on, and one that goes further moves or resizes the area it landed
 * on, or else scrolls the page; with the "draw" tool it draws a new area
 * wherever it began. A drag with the middle button scrolls the page wherever
 * it began, over areas too, and with either tool. The shape of a drag in progress is only here; the state
 * changes once, on release.
 *
 * The pointer is captured by the element the press landed on, so the moves
 * and the release come however far the pointer goes, even off the page, and
 * the `click` of a press that stays a click still reaches that element, which
 * a capture by the overlay would take from the frame or candidate under it.
 */
export function useAreaDrag({
  page,
  pageSize,
  scale,
  locked,
  tool,
  overlayRef,
  viewport,
  onDraw,
  onChange,
  onSelect,
}: UseAreaDragOptions): AreaDrag {
  // The ref is what the handlers read, as several events can come before the
  // next render; the state is what the render shows.
  const dragRef = useRef<Drag | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  // Set when a drag ends, so the `click` the browser sends after it is dropped.
  const suppressClickRef = useRef(false);

  // Scrolling the page edits nothing, so a conversion does not stop it.
  function isCurrent(candidate: Drag): boolean {
    return (
      candidate.page === page &&
      candidate.scale === scale &&
      (candidate.mode.kind === "pan" || !locked)
    );
  }

  function setBoth(next: Drag | null) {
    dragRef.current = next;
    setDrag(next);
  }

  if (drag !== null && !isCurrent(drag)) {
    setDrag(null);
  }

  // A drag that was called off for good stays so: the page and scale it began
  // on may come back, and it must not wake up then.
  useEffect(() => {
    if (drag === null) {
      dragRef.current = null;
    }
  }, [drag]);

  const dragging = drag !== null && drag.passed && isCurrent(drag);

  // Esc calls a drag off. It listens on the window, ahead of everything, so
  // the same key does not also deselect the area; Ctrl+Z and Ctrl+Y wait.
  useEffect(() => {
    if (!dragging) {
      return;
    }
    function handleKeyDown(event: KeyboardEvent) {
      // The areas stay as they are until the drag ends.
      if (historyKeyOf(event) !== null) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      if (event.key !== "Escape") {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      suppressClickRef.current = true;
      dragRef.current = null;
      setDrag(null);
    }
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [dragging]);

  function begin(
    event: PointerEvent<HTMLElement>,
    mode: DragMode,
    captureOn: EventTarget = event.target,
  ) {
    if (captureOn instanceof Element) {
      captureOn.setPointerCapture(event.pointerId);
    }
    setBoth({
      pointerId: event.pointerId,
      originX: event.clientX,
      originY: event.clientY,
      dx: 0,
      dy: 0,
      passed: false,
      mode,
      page,
      scale,
    });
  }

  function canBegin(
    event: PointerEvent<HTMLElement>,
    evenLocked = false,
  ): boolean {
    const current = dragRef.current;
    return (
      event.button === 0 &&
      (evenLocked || !locked) &&
      (current === null || current.pointerId === event.pointerId)
    );
  }

  function beginPan(event: PointerEvent<HTMLElement>) {
    if (canBegin(event, true)) {
      begin(event, {
        kind: "pan",
        scrollLeft: viewport.scrollLeft,
        scrollTop: viewport.scrollTop,
      });
    }
  }

  // Taken before the area, handle or frame under the press sees it. Not
  // calling the default off would start the WebView's autoscroll.
  function beginMiddlePan(event: PointerEvent<HTMLElement>) {
    event.preventDefault();
    event.stopPropagation();
    const current = dragRef.current;
    if (current !== null && current.pointerId !== event.pointerId) {
      return;
    }
    // No click follows a middle press, so the overlay can hold the pointer.
    begin(
      event,
      {
        kind: "pan",
        scrollLeft: viewport.scrollLeft,
        scrollTop: viewport.scrollTop,
      },
      event.currentTarget,
    );
  }

  function beginDraw(event: PointerEvent<HTMLElement>) {
    const overlay = overlayRef.current;
    if (!canBegin(event) || overlay === null) {
      return;
    }
    const bounds = overlay.getBoundingClientRect();
    const pxPerPt = CSS_PX_PER_PT * scale;
    begin(event, {
      kind: "draw",
      start: {
        x: (event.clientX - bounds.left) / pxPerPt,
        y: (event.clientY - bounds.top) / pxPerPt,
      },
    });
  }

  function beginMove(
    event: PointerEvent<HTMLElement>,
    key: number,
    rect: RectPt,
  ) {
    // With "draw" the press draws, over the area as anywhere else.
    if (tool === "draw") {
      return;
    }
    // A press on an area is the area's, not the page's: it is not scrolled.
    event.stopPropagation();
    if (event.button !== 0) {
      return;
    }
    onSelect(key);
    if (canBegin(event)) {
      begin(event, { kind: "move", key, rect });
    }
  }

  function beginResize(
    event: PointerEvent<HTMLElement>,
    key: number,
    rect: RectPt,
    handle: HandleId,
  ) {
    event.stopPropagation();
    if (canBegin(event)) {
      begin(event, { kind: "resize", key, rect, handle });
    }
  }

  function handleMove(event: PointerEvent<HTMLElement>) {
    const current = dragRef.current;
    if (current === null || current.pointerId !== event.pointerId) {
      return;
    }
    if (!isCurrent(current)) {
      setBoth(null);
      return;
    }
    const dx = event.clientX - current.originX;
    const dy = event.clientY - current.originY;
    const passed =
      current.passed || Math.hypot(dx, dy) >= CROP_DRAG_THRESHOLD_PX;
    if (passed) {
      const { mode } = current;
      if (mode.kind === "pan") {
        viewport.scrollTo({
          left: mode.scrollLeft - dx,
          top: mode.scrollTop - dy,
        });
      }
      setBoth({ ...current, dx, dy, passed });
    } else {
      dragRef.current = { ...current, dx, dy };
    }
  }

  function handleUp(event: PointerEvent<HTMLElement>) {
    const current = dragRef.current;
    if (current === null || current.pointerId !== event.pointerId) {
      return;
    }
    setBoth(null);
    if (!current.passed || !isCurrent(current)) {
      return;
    }
    suppressClickRef.current = true;
    const preview = previewOf(current, pageSize);
    const { mode } = current;
    if (preview === null || mode.kind === "pan") {
      return;
    }
    if (mode.kind === "draw") {
      if (isDrawnAreaBigEnough(preview.rectPt)) {
        onDraw(preview.rectPt);
      }
    } else {
      onChange(mode.key, preview.rectPt);
    }
  }

  function handleCancel(event: PointerEvent<HTMLElement>) {
    if (dragRef.current?.pointerId === event.pointerId) {
      setBoth(null);
    }
  }

  return {
    preview: dragging ? previewOf(drag, pageSize) : null,
    panning: dragging && drag.mode.kind === "pan",
    overlayProps: {
      onPointerDownCapture: (event) => {
        if (event.button === MIDDLE_BUTTON) {
          beginMiddlePan(event);
          return;
        }
        suppressClickRef.current = false;
      },
      // For a WebView that sends the mouse event although the pointer event
      // called the default off.
      onMouseDownCapture: (event) => {
        if (event.button === MIDDLE_BUTTON) {
          event.preventDefault();
        }
      },
      onPointerDown: tool === "draw" ? beginDraw : beginPan,
      onPointerMove: handleMove,
      onPointerUp: handleUp,
      onPointerCancel: handleCancel,
      onClickCapture: (event) => {
        if (suppressClickRef.current) {
          suppressClickRef.current = false;
          event.stopPropagation();
        }
      },
    },
    beginMove,
    beginResize,
  };
}
