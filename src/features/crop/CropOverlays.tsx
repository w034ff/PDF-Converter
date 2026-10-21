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
  /** Takes the focus to the view of the page, where it goes once an area is removed by key. */
  focusView: () => void;
}

/** Where the focus is to go once the areas have been drawn again. */
type FocusTarget = number | "view";

/**
 * What is drawn over the page (design §10.1): a dotted frame round each
 * embedded image, which is emphasized under the pointer, a thin line with the
 * kind's word round each candidate (without the word when an area lies in
 * its place), and the areas (an accent line with the
 * kind's word, and a tinted ground when selected). Pressing a frame or a
 * candidate adds the area it stands for, pressing an area selects it, and
 * pressing the page elsewhere selects none. Dragging draws an area, or moves
 * or resizes the selected one by its handles; the frames, candidates and
 * areas are buttons the keyboard can use as well.
 *
 * From the bottom up: the candidates, the frames, the areas, the handles. So
 * the frame of a photo that is also a candidate stays under the pointer, an
 * area that was made from a frame or a candidate covers it, and an area can
 * be resized from under another one.
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
  focusView,
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
    overlayRef,
    onDraw: (rectPt) => dispatch({ type: "CROP_AREA_DRAWN", page, rectPt }),
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
      focusView();
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
    if (!locked) {
      dispatch({ type: "CROP_AREA_ADDED", area });
    }
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
  const selectedArea = areas.find((area) => area.key === selectedKey);
  const pxPerPt = CSS_PX_PER_PT * scale;
  const pressable = locked ? "" : " is-pressable";
  return (
    <div
      ref={overlayRef}
      data-testid="crop-overlay"
      className={`crop-overlay${editable ? " is-editable" : ""}${
        preview !== null ? " is-dragging" : ""
      }`}
      style={preview !== null ? { cursor: preview.cursor } : undefined}
      {...drag.overlayProps}
      onClick={(event) => {
        // Only the page itself, not a frame or an area on it.
        if (event.target === event.currentTarget) {
          dispatch({ type: "CROP_AREA_SELECTED", key: null });
        }
      }}
    >
      {showCandidates &&
        candidates.map((candidate, index) => {
          const area: NewArea = {
            page,
            kind: candidate.kind,
            rectPt: candidate.rectPt,
            imageIndex:
              candidate.kind === "photo" ? candidate.imageIndex : null,
          };
          // An area in its place, such as the one made from it, puts its
          // own word where the candidate's would be.
          const madeArea = findDuplicate(areas, area) !== undefined;
          return (
            <div
              key={index}
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
              onKeyDown={(event) =>
                activate(event, () => addFromKeyboard(area))
              }
            >
              {!madeArea && (
                <span className="crop-tag">
                  {t.crop.candidateKinds[candidate.kind]}
                </span>
              )}
            </div>
          );
        })}
      {images.map((image) => {
        const area: NewArea = {
          page,
          kind: "photo",
          rectPt: image.rectPt,
          imageIndex: image.imageIndex,
        };
        return (
          <div
            key={image.imageIndex}
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
      })}
      {areas.map((area) => (
        <div
          key={area.key}
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
          onPointerDown={(event) =>
            drag.beginMove(event, area.key, area.rectPt)
          }
          onClick={() =>
            dispatch({ type: "CROP_AREA_SELECTED", key: area.key })
          }
          onKeyDown={(event) => handleAreaKeyDown(event, area)}
        >
          <span className="crop-area-tag">{t.crop.areaKinds[area.kind]}</span>
        </div>
      ))}
      {preview !== null && preview.key === null && (
        <div
          className="crop-area crop-area-draft"
          data-testid="crop-draft"
          style={rectToCssBox(preview.rectPt, scale)}
        />
      )}
      {editable &&
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
