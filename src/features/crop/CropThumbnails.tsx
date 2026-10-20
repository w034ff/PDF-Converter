import { formatMessage, getTranslations } from "../../i18n";
import type { CropPdf } from "../../ipc";
import { useAppDispatch, useAppState } from "../../state";
import { useThumbnail } from "../items/useThumbnail";
import "./crop.css";

interface CropThumbnailProps {
  pdf: CropPdf;
  page: number;
  isCurrent: boolean;
  label: string;
}

function CropThumbnail({ pdf, page, isCurrent, label }: CropThumbnailProps) {
  const dispatch = useAppDispatch();
  const { ref, src } = useThumbnail(pdf.id, page);
  const size = pdf.pages[page - 1];

  return (
    <li>
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
        </span>
        <span className="mono crop-thumb-number">{page}</span>
      </button>
    </li>
  );
}

/**
 * The column of page thumbnails on the left of the crop tab (mockup
 * `CropA`): a click shows the page, and the page shown has an accent frame.
 * Each thumbnail is asked for once it comes into view (design §10.1).
 */
export function CropThumbnails({ pdf }: { pdf: CropPdf }) {
  const { language, crop } = useAppState();
  const t = getTranslations(language.language);

  return (
    <nav className="crop-thumbs" aria-label={t.crop.pagesAria}>
      <ul className="crop-thumb-list">
        {pdf.pages.map((_, index) => (
          <CropThumbnail
            key={index}
            pdf={pdf}
            page={index + 1}
            isCurrent={index + 1 === crop.page}
            label={formatMessage(t.crop.pageAria, { page: index + 1 })}
          />
        ))}
      </ul>
    </nav>
  );
}
