import { useState } from "react";
import { useAppState } from "../../state";
import { CropMain } from "./CropMain";
import { CropSidePanel } from "./CropSidePanel";
import { CropThumbnails } from "./CropThumbnails";

/**
 * The body of the crop tab (design §10.1): the page thumbnails on the left,
 * the page in the middle and the areas and export settings on the right.
 * While no PDF is open only the middle shows, as a drop zone.
 */
export function CropScreen() {
  const { crop } = useAppState();
  const { pdf } = crop;
  const [focusRequested, setFocusRequested] = useState(false);

  return (
    <>
      {pdf !== null && pdf.pageCount > 0 && <CropThumbnails pdf={pdf} />}
      <CropMain
        focusRequested={focusRequested}
        onUnlocked={() => setFocusRequested(true)}
        onFocused={() => setFocusRequested(false)}
      />
      {pdf !== null && <CropSidePanel />}
    </>
  );
}
