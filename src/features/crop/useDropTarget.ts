import { useEffect } from "react";
import { setDropTarget, type DropTarget } from "../../ipc";
import type { ActiveTab } from "../../state";

/**
 * Tells Rust where a drop goes, each time the tab changes and once at the
 * start (design §6.10): the crop tab opens a PDF, the others fill the lists.
 *
 * A failure is dropped: Rust keeps the target it had, and the next tab change
 * says it again.
 */
export function useDropTarget(tab: ActiveTab): void {
  const target: DropTarget = tab === "crop" ? "crop" : "lists";
  useEffect(() => {
    setDropTarget(target).catch(() => {
      // Dropped on purpose; see the doc comment.
    });
  }, [target]);
}
