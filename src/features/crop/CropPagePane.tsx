import { useCallback, useEffect, useRef, useState } from "react";
import {
  MenuButton,
  SegmentedControl,
  type MenuButtonItem,
  type SegmentedOption,
} from "../../components";
import { formatMessage, getTranslations } from "../../i18n";
import type { CropPdf } from "../../ipc";
import {
  candidateAreas,
  isJobActive,
  useAppDispatch,
  useAppState,
  type CropTool,
  type CropViewSize,
} from "../../state";
import { useJobRunner } from "../job/useJobRunner";
import { CropKeyHints } from "./CropKeyHints";
import { CropOverlays } from "./CropOverlays";
import {
  CSS_PX_PER_PT,
  nextZoomPercent,
  pageImageMaxSide,
  scalePercent,
  viewScale,
  type ViewportSize,
} from "./cropView";
import { useCropKeys } from "./useCropKeys";
import { usePageImage } from "./usePageImage";
import "./crop.css";

type ViewKind = CropViewSize["kind"];

interface PageSurfaceProps {
  pdf: CropPdf;
  page: number;
  scale: number;
  /** The scrolling view the page is in. */
  viewport: HTMLElement;
}

/** The page at `scale` with its picture and what is drawn over it. */
function PageSurface({ pdf, page, scale, viewport }: PageSurfaceProps) {
  const { language, crop, job } = useAppState();
  const t = getTranslations(language.language);
  const size = pdf.pages[page - 1];
  const widthPx = size.widthPt * CSS_PX_PER_PT * scale;
  const heightPx = size.heightPt * CSS_PX_PER_PT * scale;
  const src = usePageImage(
    pdf.id,
    page,
    pageImageMaxSide(Math.max(widthPx, heightPx), window.devicePixelRatio),
  );
  const images = crop.pageImages[page];
  const layout = crop.layouts[page];
  const locked = isJobActive(job);

  return (
    <div
      className="crop-page"
      data-testid="crop-page"
      style={{ width: widthPx, height: heightPx }}
    >
      {src !== null && (
        <img
          className="crop-page-image"
          src={src}
          alt={formatMessage(t.crop.pageAlt, { name: pdf.name, page })}
        />
      )}
      <CropOverlays
        page={page}
        pageSize={size}
        images={images?.status === "done" ? images.value : []}
        candidates={layout?.status === "done" ? layout.value : []}
        areas={crop.areas.filter((area) => area.page === page)}
        selectedKey={crop.selectedKey}
        showCandidates={crop.showCandidates}
        locked={locked}
        scale={scale}
        tool={locked ? "select" : crop.tool}
        hiddenCandidates={crop.hiddenCandidates[page] ?? []}
        viewport={viewport}
      />
    </div>
  );
}

export interface CropPagePaneProps {
  pdf: CropPdf;
  /**
   * Whether the view should take the focus once it is shown: the control that
   * opened the password dialog is gone by then (design §10.1).
   */
  focusRequested: boolean;
  /** Called once the view has taken the focus. */
  onFocused: () => void;
}

/**
 * The toolbar and the page of the crop tab (mockup `CropA`): how big the page
 * is shown, whether the candidates are drawn, and the page itself in a view
 * that scrolls when the page is larger than it.
 */
export function CropPagePane({
  pdf,
  focusRequested,
  onFocused,
}: CropPagePaneProps) {
  const { language, crop, job } = useAppState();
  const dispatch = useAppDispatch();
  const runner = useJobRunner();
  const t = getTranslations(language.language);
  const { page, view } = crop;
  const size = pdf.pages[page - 1];
  const busy = isJobActive(job);

  const layout = crop.layouts[page];
  const pageCandidates = candidateAreas(
    page,
    layout?.status === "done" ? layout.value : [],
    crop.hiddenCandidates[page],
  );
  const addItems: MenuButtonItem[] = [
    {
      id: "thisPage",
      label: t.crop.addMenu.thisPage,
      disabled: pageCandidates.length === 0,
      onSelect: () =>
        dispatch({ type: "CROP_AREAS_ADDED", areas: pageCandidates }),
    },
    {
      id: "allPages",
      label: t.crop.addMenu.allPages,
      disabled: crop.layoutUnavailable,
      onSelect: () => void runner.startDetectAll(pdf.id),
    },
    {
      id: "wholePage",
      label: t.crop.addMenu.wholePage,
      onSelect: () =>
        dispatch({
          type: "CROP_AREA_ADDED",
          area: {
            page,
            kind: "manual",
            rectPt: [0, 0, size.widthPt, size.heightPt],
            imageIndex: null,
          },
        }),
    },
  ];
  const [viewport, setViewport] = useState<HTMLDivElement | null>(null);
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const attachViewport = useCallback((element: HTMLDivElement | null) => {
    viewportRef.current = element;
    setViewport(element);
  }, []);
  const hintsButtonRef = useRef<HTMLButtonElement>(null);
  // Where the list of keys was opened from, which the focus goes back to.
  const [hints, setHints] = useState<"button" | "key" | null>(null);
  const tool = busy ? "select" : crop.tool;
  const openHintsByKey = useCallback(() => setHints("key"), []);
  useCropKeys({ tool, canDraw: !busy, onHelp: openHintsByKey });
  const [room, setRoom] = useState<ViewportSize | null>(null);

  // The room is the content box, so the view's padding is not part of it.
  useEffect(() => {
    if (viewport === null) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const box = entries.at(-1)?.contentRect;
      if (box !== undefined) {
        setRoom({ width: box.width, height: box.height });
      }
    });
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [viewport]);

  useEffect(() => {
    if (viewport !== null && focusRequested) {
      viewport.focus();
      onFocused();
    }
  }, [viewport, focusRequested, onFocused]);

  const scale = viewScale(view, size, room);
  const percent = scale === null ? null : scalePercent(scale);

  // The wheel handler reads the percent through a ref: it is added once per
  // view element, and consecutive events before the next render must each
  // move one step on from the last.
  const percentRef = useRef(percent);
  useEffect(() => {
    percentRef.current = percent;
  }, [percent]);

  // A listener of its own, not `onWheel`: React's is passive, and a passive
  // listener cannot stop the WebView from zooming the whole window.
  useEffect(() => {
    if (viewport === null) {
      return;
    }
    function handleWheel(event: WheelEvent) {
      if (!event.ctrlKey) {
        return;
      }
      event.preventDefault();
      const current = percentRef.current;
      if (current === null || event.deltaY === 0) {
        return;
      }
      const next = nextZoomPercent(current, event.deltaY < 0 ? "in" : "out");
      if (next !== null) {
        percentRef.current = next;
        dispatch({
          type: "CROP_VIEW_SET",
          view: { kind: "zoom", percent: next },
        });
      }
    }
    viewport.addEventListener("wheel", handleWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", handleWheel);
  }, [viewport, dispatch]);

  function zoom(direction: "in" | "out") {
    if (percent === null) {
      return;
    }
    const next = nextZoomPercent(percent, direction);
    if (next !== null) {
      dispatch({
        type: "CROP_VIEW_SET",
        view: { kind: "zoom", percent: next },
      });
    }
  }

  const toolOptions: readonly SegmentedOption<CropTool>[] = [
    { value: "select", label: t.crop.toolbar.select },
    { value: "draw", label: t.crop.toolbar.draw },
  ];

  const sizeOptions: readonly SegmentedOption<"fitWidth" | "fitPage">[] = [
    { value: "fitWidth", label: t.crop.toolbar.fitWidth },
    { value: "fitPage", label: t.crop.toolbar.fitPage },
  ];

  return (
    <>
      <div className="crop-toolbar">
        <SegmentedControl<ViewKind>
          label={t.crop.toolbar.sizeLabel}
          options={sizeOptions}
          value={view.kind}
          onChange={(kind) => {
            if (kind !== "zoom") {
              dispatch({ type: "CROP_VIEW_SET", view: { kind } });
            }
          }}
          className="seg crop-size-seg"
        />
        <button
          type="button"
          className="icon-btn"
          aria-label={t.crop.toolbar.zoomOut}
          disabled={
            percent === null || nextZoomPercent(percent, "out") === null
          }
          onClick={() => zoom("out")}
        >
          −
        </button>
        <span
          className="mono crop-zoom-percent"
          data-testid="crop-zoom-percent"
        >
          {percent === null ? "" : `${percent}%`}
        </span>
        <button
          type="button"
          className="icon-btn"
          aria-label={t.crop.toolbar.zoomIn}
          disabled={percent === null || nextZoomPercent(percent, "in") === null}
          onClick={() => zoom("in")}
        >
          ＋
        </button>
        <label className="crop-candidates-toggle label">
          <input
            type="checkbox"
            checked={crop.showCandidates}
            onChange={(event) =>
              dispatch({
                type: "CROP_CANDIDATES_SHOWN",
                shown: event.target.checked,
              })
            }
          />
          {t.crop.toolbar.candidates}
        </label>
        <SegmentedControl<CropTool>
          label={t.crop.toolbar.toolLabel}
          options={toolOptions}
          value={tool}
          onChange={(next) => dispatch({ type: "CROP_TOOL_SET", tool: next })}
          disabled={busy}
          className="seg crop-tool-seg"
        />
        <MenuButton
          className="btn btn-ghost crop-add-areas"
          menuLabel={t.crop.addMenu.label}
          items={addItems}
          disabled={busy}
        >
          {t.crop.toolbar.addAreas}
        </MenuButton>
        <button
          ref={hintsButtonRef}
          type="button"
          className="icon-btn"
          aria-label={t.crop.toolbar.keyHints}
          title={t.crop.toolbar.keyHintsTitle}
          onClick={() => setHints("button")}
        >
          ?
        </button>
      </div>
      {hints !== null && (
        <CropKeyHints
          onClose={() => setHints(null)}
          returnFocusRef={hints === "button" ? hintsButtonRef : viewportRef}
        />
      )}

      <div
        className="crop-viewport"
        data-testid="crop-viewport"
        ref={attachViewport}
        tabIndex={-1}
        onKeyDown={(event) => {
          // A drag takes the key first, and so do the menu and dialogs,
          // which stop it; so does an area that removes itself. With "描く",
          // Esc goes back to "選ぶ" (`useCropKeys`) and keeps the selection.
          if (
            event.defaultPrevented ||
            crop.selectedKey === null ||
            (event.key === "Escape" && tool === "draw")
          ) {
            return;
          }
          if (event.key === "Escape") {
            dispatch({ type: "CROP_AREA_SELECTED", key: null });
            return;
          }
          // Pressing a frame or a candidate adds an area but leaves the focus
          // on what was pressed, so the key removes the selected area from
          // anywhere on the page.
          if (event.key === "Delete" || event.key === "Backspace") {
            event.preventDefault();
            if (!busy) {
              dispatch({ type: "CROP_AREA_REMOVED", key: crop.selectedKey });
              viewport?.focus();
            }
          }
        }}
      >
        {scale !== null && viewport !== null && (
          <PageSurface
            pdf={pdf}
            page={page}
            scale={scale}
            viewport={viewport}
          />
        )}
      </div>
    </>
  );
}
