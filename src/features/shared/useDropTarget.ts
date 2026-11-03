import { useEffect } from "react";
import { setDropTarget, type DropTarget } from "../../ipc";
import type { ActiveTab } from "../../state";

/** Where a drop goes while `tab` is shown. */
function dropTargetOf(tab: ActiveTab): DropTarget {
  switch (tab) {
    case "crop":
      return "crop";
    case "organize":
      return "organize";
    case "imagesToPdf":
    case "pdfToImages":
      return "lists";
  }
}

/**
 * Tells Rust where a drop goes, each time the tab changes and once at the
 * start (design §6.10): the crop tab opens a PDF, the organize tab fills its
 * own list, and the others fill the lists.
 *
 * A failure is dropped: Rust keeps the target it had, and the next tab change
 * says it again.
 */
export function useDropTarget(tab: ActiveTab): void {
  const target = dropTargetOf(tab);
  useEffect(() => {
    setDropTarget(target).catch(() => {
      // Dropped on purpose; see the doc comment.
    });
  }, [target]);
}
