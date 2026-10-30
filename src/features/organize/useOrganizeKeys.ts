import { useEffect } from "react";
import { useAppDispatch } from "../../state";
import { historyKeyOf, isModalOpen, takesText } from "../shortcuts/keyboard";

export interface OrganizeKeysOptions {
  /** Opens the list of the keys (`?`). */
  onHelp: () => void;
}

/**
 * The keys of the organize tab that work wherever the focus is (organize §4,
 * design §10.1): Ctrl+Z undoes, Ctrl+Y or Ctrl+Shift+Z redoes, and `?` lists
 * the keys. None of them work in a field that takes text nor while a dialog
 * is open, and a drag in progress stops them before they get here.
 */
export function useOrganizeKeys({ onHelp }: OrganizeKeysOptions) {
  const dispatch = useAppDispatch();
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || takesText(event.target) || isModalOpen()) {
        return;
      }
      const direction = historyKeyOf(event);
      if (direction !== null) {
        event.preventDefault();
        dispatch({
          type: direction === "undo" ? "ORGANIZE_UNDO" : "ORGANIZE_REDO",
        });
        return;
      }
      if (event.key === "?" && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        onHelp();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [dispatch, onHelp]);
}
