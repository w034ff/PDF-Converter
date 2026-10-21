import { useRef, useState } from "react";
import { useAppState } from "../../state";
import { CropMain } from "./CropMain";
import { CropReplaceDialog } from "./CropReplaceDialog";
import { CropSidePanel } from "./CropSidePanel";
import { CropThumbnails } from "./CropThumbnails";
import { useCropHistoryKeys } from "./useCropHistoryKeys";

/**
 * The body of the crop tab (design §10.1): the page thumbnails on the left,
 * the page in the middle and the areas and export settings on the right.
 * While no PDF is open only the middle shows, as a drop zone.
 */
export function CropScreen() {
  const { crop } = useAppState();
  const { pdf, pendingReplace } = crop;
  const [focusRequested, setFocusRequested] = useState(false);
  const openButtonRef = useRef<HTMLButtonElement>(null);
  useCropHistoryKeys();

  return (
    <>
      {pdf !== null && pdf.pageCount > 0 && <CropThumbnails pdf={pdf} />}
      <CropMain
        openButtonRef={openButtonRef}
        focusRequested={focusRequested}
        onUnlocked={() => setFocusRequested(true)}
        onFocused={() => setFocusRequested(false)}
      />
      {pdf !== null && <CropSidePanel pdf={pdf} />}
      {pendingReplace !== null && (
        <CropReplaceDialog
          pending={pendingReplace}
          // A file dropped on the page was not chosen with the button, so
          // the focus goes to the page, as after the password dialog.
          returnFocusRef={
            pendingReplace.source === "dialog" ? openButtonRef : undefined
          }
          onCancelled={() => {
            if (pendingReplace.source === "drop") {
              setFocusRequested(true);
            }
          }}
        />
      )}
    </>
  );
}
