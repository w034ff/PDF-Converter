import {
  useEffect,
  useRef,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import { formatMessage, getTranslations } from "../../i18n";
import type { Candidate, PageImage, PageSizePt } from "../../ipc";
import {
  findDuplicate,
  useAppDispatch,
  useAppState,
  type CropArea,
  type CropTool,
  type NewArea,
  type RectPt,
} from "../../state";
import {
  HANDLE_IDS,
  arrowRect,
  handleCursor,
  handlePoint,
  isArrowKey,
} from "./cropEdit";
import {
  CROP_HANDLE_HIT_PX,
  CROP_HANDLE_SIZE_PX,
  CSS_PX_PER_PT,
  rectToCssBox,
} from "./cropView";
import { useAreaDrag } from "./useAreaDrag";

export interface CropOverlaysProps {
  page: number;
  pageSize: PageSizePt;
  images: readonly PageImage[];
  candidates: readonly Candidate[];
  /** The areas of the page. */
  areas: readonly CropArea[];
  selectedKey: number | null;
  /** Whether the candidates are drawn; the frames of embedded images always are. */
  showCandidates: boolean;
  /** Whether the areas cannot be added or changed (a conversion runs). */
  locked: boolean;
  scale: number;
  /** What a press and a drag do (design §10.1). */
  tool: CropTool;
  /** The indices of the page's candidates that were put aside as wrong. */
  hiddenCandidates: readonly number[];
  /** The scrolling view of the page: a drag of the page moves it, and the focus goes there once something is removed by key. */
  viewport: HTMLElement;
}

/** One of the things drawn over the page, in the order they are stacked. */
type Layer =
  | { kind: "candidate"; index: number; candidate: Candidate }
  | { kind: "frame"; image: PageImage }
  | { kind: "area"; area: CropArea };

function layerRect(layer: Layer): RectPt {
  switch (layer.kind) {
    case "candidate":
      return layer.candidate.rectPt;
    case "frame":
      return layer.image.rectPt;
    case "area":
      return layer.area.rectPt;
  }
}

function rectSize([x0, y0, x1, y1]: RectPt): number {
  return (x1 - x0) * (y1 - y0);
}

/**
 * The candidates, frames and areas from the biggest to the smallest, so the
 * smallest thing under the pointer takes the press (design §10.1): a wrong
 * candidate round the whole page, or an area over it, leaves what is inside
 * it to be pressed. The tab order follows. Things of the same size keep the
 * order of `layers`, which lists the areas last, so an area lies over the
 * frame or candidate it was made from.
 */
function stackLayers(layers: Layer[]): Layer[] {
  return layers
    .map((layer, order) => ({ layer, order }))
    .sort(
      (a, b) =>
        rectSize(layerRect(b.layer)) - rectSize(layerRect(a.layer)) ||
        a.order - b.order,
    )
    .map(({ layer }) => layer);
}

/** Where the focus is to go once the areas have been drawn again. */
type FocusTarget = number | "view";

/**
 * What is drawn over the page (design §10.1): a dotted frame round each
 * embedded image, which is emphasized under the pointer, a thin line with the
 * kind's word round each candidate (without the word when an area lies in
 * its place), and the areas (an accent line with the kind's word, and a
 * tinted ground when selected). With the "select" tool, pressing a frame or
 * a candidate adds the area it stands for, pressing an area selects it,
 * pressing the page elsewhere selects none, dragging an area or its handles
 * moves or resizes it, and dragging anything else scrolls the page. With
 * "draw", a drag anywhere draws an area. The frames, candidates and areas are
 * buttons the keyboard can use as well.
 *
 * They are stacked from the biggest up (`stackLayers`), and the handles of
 * the selected area lie over all of them, so it can be resized from under
 * another one.
 */
export function CropOverlays({
  page,
  pageSize,
  images,
  candidates,
  areas,
  selectedKey,
  showCandidates,
  locked,
  scale,
  tool,
  hiddenCandidates,
  viewport,
}: CropOverlaysProps) {
  const { language, crop } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const overlayRef = useRef<HTMLDivElement>(null);
  const focusTargetRef = useRef<FocusTarget | null>(null);

  const drag = useAreaDrag({
    page,
    pageSize,
    scale,
    locked,
    tool,
    overlayRef,
    viewport,
    // Drawing makes one area, then the page is for selecting again.
    onDraw: (rectPt) => {
      dispatch({ type: "CROP_AREA_DRAWN", page, rectPt });
      dispatch({ type: "CROP_TOOL_SET", tool: "select" });
    },
    onChange: (key, rectPt) =>
      dispatch({ type: "CROP_AREA_CHANGED", key, rectPt }),
    onSelect: (key) => dispatch({ type: "CROP_AREA_SELECTED", key }),
  });
  const { preview } = drag;

  /** Takes the focus to the target if it is drawn; true once it has. */
  function takeFocus(): boolean {
    const target = focusTargetRef.current;
    if (target === null) {
      return true;
    }
    if (target === "view") {
      viewport.focus();
      return true;
    }
    const element = overlayRef.current?.querySelector(
      `.crop-area[data-key="${target}"]`,
    );
    if (element instanceof HTMLElement) {
      element.focus();
      return true;
    }
    return false;
  }

  function moveFocusTo(target: FocusTarget) {
    focusTargetRef.current = target;
    if (takeFocus()) {
      focusTargetRef.current = null;
    }
  }

  // An area that was just added is not drawn yet when the key that added it
  // is handled; the focus follows once it is.
  useEffect(() => {
    takeFocus();
    focusTargetRef.current = null;
  });

  function add(area: NewArea) {
    if (!locked && tool === "select") {
      dispatch({ type: "CROP_AREA_ADDED", area });
    }
  }

  /**
   * Delete on a candidate removes the area in its place, which pressing the
   * candidate just added while the focus stayed on it, or else puts the
   * candidate aside as wrong (design §10.1).
   */
  function handleCandidateKeyDown(
    event: KeyboardEvent<HTMLElement>,
    index: number,
    area: NewArea,
  ) {
    if (event.target !== event.currentTarget) {
      return;
    }
    if (event.key !== "Delete" && event.key !== "Backspace") {
      activate(event, () => addFromKeyboard(area));
      return;
    }
    event.preventDefault();
    if (locked) {
      return;
    }
    const placed = findDuplicate(areas, area);
    dispatch(
      placed === undefined
        ? { type: "CROP_CANDIDATE_HIDDEN", page, index }
        : { type: "CROP_AREA_REMOVED", key: placed.key },
    );
    moveFocusTo("view");
  }

  function addFromKeyboard(area: NewArea) {
    if (locked) {
      return;
    }
    const target = findDuplicate(areas, area)?.key ?? crop.nextAreaKey;
    dispatch({ type: "CROP_AREA_ADDED", area });
    moveFocusTo(target);
  }

  function activate(
    event: KeyboardEvent<HTMLElement>,
    activateWith: () => void,
  ) {
    if (
      event.target !== event.currentTarget ||
      (event.key !== "Enter" && event.key !== " ")
    ) {
      return;
    }
    event.preventDefault();
    activateWith();
  }

  function handleAreaKeyDown(
    event: KeyboardEvent<HTMLElement>,
    area: CropArea,
  ) {
    if (event.target !== event.currentTarget) {
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      dispatch({ type: "CROP_AREA_SELECTED", key: area.key });
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      if (locked) {
        return;
      }
      dispatch({ type: "CROP_AREA_REMOVED", key: area.key });
      // Not to another area: its focus ring would read as a selection.
      moveFocusTo("view");
      return;
    }
    if (!isArrowKey(event.key) || event.ctrlKey || event.metaKey) {
      return;
    }
    // Stops the view from scrolling and, with Alt, WebView2 from going back.
    event.preventDefault();
    if (locked) {
      return;
    }
    const rectPt = arrowRect(
      area.rectPt,
      event.key,
      { shift: event.shiftKey, alt: event.altKey },
      pageSize,
    );
    if (rectPt.every((value, index) => value === area.rectPt[index])) {
      return;
    }
    if (selectedKey !== area.key) {
      dispatch({ type: "CROP_AREA_SELECTED", key: area.key });
    }
    dispatch({
      type: "CROP_AREA_CHANGED",
      key: area.key,
      rectPt,
      byKeyboard: true,
    });
  }

  function rectOf(area: CropArea): RectPt {
    return preview !== null && preview.key === area.key
      ? preview.rectPt
      : area.rectPt;
  }

  const editable = !locked;
  const drawing = editable && tool === "draw";
  const selectedArea = areas.find((area) => area.key === selectedKey);
  const pxPerPt = CSS_PX_PER_PT * scale;
  const pressable = locked ? "" : " is-pressable";
  const layers = stackLayers([
    ...(showCandidates
      ? candidates.flatMap((candidate, index): Layer[] =>
          hiddenCandidates.includes(index)
            ? []
            : [{ kind: "candidate", index, candidate }],
        )
      : []),
    ...images.map((image): Layer => ({ kind: "frame", image })),
    ...areas.map((area): Layer => ({ kind: "area", area })),
  ]);

  function renderCandidate(index: number, candidate: Candidate) {
    const area: NewArea = {
      page,
      kind: candidate.kind,
      rectPt: candidate.rectPt,
      imageIndex: candidate.kind === "photo" ? candidate.imageIndex : null,
    };
    // An area in its place, such as the one made from it, puts its own word
    // where the candidate's would be.
    const madeArea = findDuplicate(areas, area) !== undefined;
    return (
      <div
        key={`candidate-${index}`}
        className={`crop-candidate${pressable}`}
        data-testid="crop-candidate"
        data-kind={candidate.kind}
        role="button"
        tabIndex={locked ? -1 : 0}
        aria-label={formatMessage(t.crop.candidateAria, {
          kind: t.crop.candidateKinds[candidate.kind],
        })}
        style={rectToCssBox(candidate.rectPt, scale)}
        onClick={() => add(area)}
        onKeyDown={(event) => handleCandidateKeyDown(event, index, area)}
      >
        {!madeArea && (
          <span className="crop-tag">
            {t.crop.candidateKinds[candidate.kind]}
          </span>
        )}
      </div>
    );
  }

  function renderFrame(image: PageImage) {
    const area: NewArea = {
      page,
      kind: "photo",
      rectPt: image.rectPt,
      imageIndex: image.imageIndex,
    };
    return (
      <div
        key={`frame-${image.imageIndex}`}
        className={`crop-photo-frame${pressable}`}
        data-testid="crop-photo-frame"
        role="button"
        tabIndex={locked ? -1 : 0}
        aria-label={t.crop.photoFrameAria}
        style={rectToCssBox(image.rectPt, scale)}
        onClick={() => add(area)}
        onKeyDown={(event) => activate(event, () => addFromKeyboard(area))}
      />
    );
  }

  function renderArea(area: CropArea) {
    return (
      <div
        key={`area-${area.key}`}
        className={`crop-area${area.key === selectedKey ? " is-selected" : ""}`}
        data-testid="crop-area"
        data-key={area.key}
        data-kind={area.kind}
        data-selected={area.key === selectedKey}
        role="button"
        tabIndex={0}
        aria-label={formatMessage(t.crop.areaAria, {
          kind: t.crop.areaKinds[area.kind],
        })}
        aria-current={area.key === selectedKey ? "true" : undefined}
        style={rectToCssBox(rectOf(area), scale)}
        onPointerDown={(event) => drag.beginMove(event, area.key, area.rectPt)}
        onClick={() => {
          if (tool === "select") {
            dispatch({ type: "CROP_AREA_SELECTED", key: area.key });
          }
        }}
        onKeyDown={(event) => handleAreaKeyDown(event, area)}
      >
        <span className="crop-area-tag">{t.crop.areaKinds[area.kind]}</span>
      </div>
    );
  }

  const toolClass = `${editable ? " is-editable" : ""}${drawing ? " is-drawing" : ""}`;
  return (
    <div
      ref={overlayRef}
      data-testid="crop-overlay"
      className={`crop-overlay${toolClass}${
        preview !== null ? " is-dragging" : ""
      }${drag.panning ? " is-panning" : ""}`}
      style={preview !== null ? { cursor: preview.cursor } : undefined}
      {...drag.overlayProps}
      onClick={(event) => {
        // Only the page itself, not a frame or an area on it.
        if (event.target === event.currentTarget && tool === "select") {
          dispatch({ type: "CROP_AREA_SELECTED", key: null });
        }
      }}
    >
      {layers.map((layer) => {
        switch (layer.kind) {
          case "candidate":
            return renderCandidate(layer.index, layer.candidate);
          case "frame":
            return renderFrame(layer.image);
          case "area":
            return renderArea(layer.area);
        }
      })}
      {preview !== null && preview.key === null && (
        <div
          className="crop-area crop-area-draft"
          data-testid="crop-draft"
          style={rectToCssBox(preview.rectPt, scale)}
        />
      )}
      {editable &&
        !drawing &&
        selectedArea !== undefined &&
        HANDLE_IDS.map((handle) => {
          const point = handlePoint(rectOf(selectedArea), handle);
          return (
            <div
              key={handle}
              className="crop-handle"
              data-testid="crop-handle"
              data-handle={handle}
              style={{
                left: point.x * pxPerPt - CROP_HANDLE_HIT_PX / 2,
                top: point.y * pxPerPt - CROP_HANDLE_HIT_PX / 2,
                width: CROP_HANDLE_HIT_PX,
                height: CROP_HANDLE_HIT_PX,
                cursor: handleCursor(handle),
              }}
              onPointerDown={(event: PointerEvent<HTMLElement>) =>
                drag.beginResize(
                  event,
                  selectedArea.key,
                  selectedArea.rectPt,
                  handle,
                )
              }
            >
              <span
                className="crop-handle-mark"
                style={{
                  width: CROP_HANDLE_SIZE_PX,
                  height: CROP_HANDLE_SIZE_PX,
                }}
              />
            </div>
          );
        })}
    </div>
  );
}
