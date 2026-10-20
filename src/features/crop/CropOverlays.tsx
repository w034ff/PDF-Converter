import { getTranslations } from "../../i18n";
import type { Candidate, PageImage } from "../../ipc";
import { useAppState } from "../../state";
import { rectToCssBox } from "./cropView";

export interface CropOverlaysProps {
  images: readonly PageImage[];
  candidates: readonly Candidate[];
  /** Whether the candidates are drawn; the frames of embedded images always are. */
  showCandidates: boolean;
  scale: number;
}

/**
 * What is drawn over the page (design §10.1): a dotted frame round each
 * embedded image, which is emphasized under the pointer, and a thin line with
 * the kind's word round each candidate. They only show where things are:
 * choosing an area from them comes with the areas themselves.
 *
 * The candidates come first, so the frame of a photo that is also a candidate
 * stays under the pointer.
 */
export function CropOverlays({
  images,
  candidates,
  showCandidates,
  scale,
}: CropOverlaysProps) {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  return (
    <div className="crop-overlay">
      {showCandidates &&
        candidates.map((candidate, index) => (
          <div
            key={index}
            className="crop-candidate"
            data-testid="crop-candidate"
            data-kind={candidate.kind}
            style={rectToCssBox(candidate.rectPt, scale)}
          >
            <span className="crop-tag">
              {t.crop.candidateKinds[candidate.kind]}
            </span>
          </div>
        ))}
      {images.map((image) => (
        <div
          key={image.imageIndex}
          className="crop-photo-frame"
          data-testid="crop-photo-frame"
          style={rectToCssBox(image.rectPt, scale)}
        />
      ))}
    </div>
  );
}
