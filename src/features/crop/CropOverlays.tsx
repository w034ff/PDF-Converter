import { getTranslations } from "../../i18n";
import type { Candidate, PageImage } from "../../ipc";
import {
  useAppDispatch,
  useAppState,
  type CropArea,
  type NewArea,
} from "../../state";
import { rectToCssBox } from "./cropView";

export interface CropOverlaysProps {
  page: number;
  images: readonly PageImage[];
  candidates: readonly Candidate[];
  /** The areas of the page. */
  areas: readonly CropArea[];
  selectedKey: number | null;
  /** Whether the candidates are drawn; the frames of embedded images always are. */
  showCandidates: boolean;
  /** Whether the frames and candidates cannot be pressed to add an area (a conversion runs). */
  locked: boolean;
  scale: number;
}

/**
 * What is drawn over the page (design §10.1): a dotted frame round each
 * embedded image, which is emphasized under the pointer, a thin line with the
 * kind's word round each candidate, and the areas (an accent line with the
 * kind's word, and a tinted ground when selected). Pressing a frame or a
 * candidate adds the area it stands for, pressing an area selects it, and
 * pressing the page elsewhere selects none.
 *
 * From the bottom up: the candidates, the frames, the areas. So the frame of
 * a photo that is also a candidate stays under the pointer, and an area that
 * was made from a frame or a candidate covers it.
 */
export function CropOverlays({
  page,
  images,
  candidates,
  areas,
  selectedKey,
  showCandidates,
  locked,
  scale,
}: CropOverlaysProps) {
  const { language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);

  function add(area: NewArea) {
    if (!locked) {
      dispatch({ type: "CROP_AREA_ADDED", area });
    }
  }

  const pressable = locked ? "" : " is-pressable";
  return (
    <div
      className="crop-overlay"
      onClick={(event) => {
        // Only the page itself, not a frame or an area on it.
        if (event.target === event.currentTarget) {
          dispatch({ type: "CROP_AREA_SELECTED", key: null });
        }
      }}
    >
      {showCandidates &&
        candidates.map((candidate, index) => (
          <div
            key={index}
            className={`crop-candidate${pressable}`}
            data-testid="crop-candidate"
            data-kind={candidate.kind}
            style={rectToCssBox(candidate.rectPt, scale)}
            onClick={() =>
              add({
                page,
                kind: candidate.kind,
                rectPt: candidate.rectPt,
                imageIndex:
                  candidate.kind === "photo" ? candidate.imageIndex : null,
              })
            }
          >
            <span className="crop-tag">
              {t.crop.candidateKinds[candidate.kind]}
            </span>
          </div>
        ))}
      {images.map((image) => (
        <div
          key={image.imageIndex}
          className={`crop-photo-frame${pressable}`}
          data-testid="crop-photo-frame"
          style={rectToCssBox(image.rectPt, scale)}
          onClick={() =>
            add({
              page,
              kind: "photo",
              rectPt: image.rectPt,
              imageIndex: image.imageIndex,
            })
          }
        />
      ))}
      {areas.map((area) => (
        <div
          key={area.key}
          className={`crop-area${area.key === selectedKey ? " is-selected" : ""}`}
          data-testid="crop-area"
          data-key={area.key}
          data-kind={area.kind}
          data-selected={area.key === selectedKey}
          style={rectToCssBox(area.rectPt, scale)}
          onClick={() =>
            dispatch({ type: "CROP_AREA_SELECTED", key: area.key })
          }
        >
          <span className="crop-area-tag">{t.crop.areaKinds[area.kind]}</span>
        </div>
      ))}
    </div>
  );
}
