import { useCallback, useEffect, useRef, useState } from "react";
import { onItemsDropped, type IpcError, type ItemsDropped } from "../../ipc";
import { useAppDispatch, useAppState, type ActiveTab } from "../../state";
import { useIpcEvent } from "../shared/useIpcEvent";

/**
 * The tab to show after a drop (design §6.1): the list that received items,
 * or the current tab when both or neither did.
 */
export function tabAfterDrop(
  dropped: ItemsDropped,
  current: ActiveTab,
): ActiveTab {
  const gotImages = dropped.images.length > 0;
  const gotPdfs = dropped.pdfs.length > 0;
  if (gotImages && !gotPdfs) {
    return "imagesToPdf";
  }
  if (gotPdfs && !gotImages) {
    return "pdfToImages";
  }
  return current;
}

export interface DropError {
  error: IpcError | null;
  dismiss: () => void;
}

/**
 * Adds what Rust reports in `items-dropped` (design §7.2) to both lists and
 * switches to the tab that received it. Mount it once, above both screens:
 * one drop can fill both lists.
 *
 * Returns the error of the last drop (`ConversionRunning` during a
 * conversion), until it is dismissed or another drop arrives.
 */
export function useItemsDropped(): DropError {
  const dispatch = useAppDispatch();
  const { language } = useAppState();
  const [error, setError] = useState<IpcError | null>(null);

  // Two drops can arrive before the next render, so the tab is read through a
  // ref that the handler keeps up to date itself.
  const activeTab = useRef(language.activeTab);
  useEffect(() => {
    activeTab.current = language.activeTab;
  }, [language.activeTab]);

  useIpcEvent(onItemsDropped, (dropped) => {
    setError(dropped.error);
    dispatch({ type: "ADD_IMAGE_ITEMS", items: dropped.images });
    dispatch({ type: "ADD_PDF_ITEMS", items: dropped.pdfs });
    const tab = tabAfterDrop(dropped, activeTab.current);
    if (tab !== activeTab.current) {
      activeTab.current = tab;
      dispatch({ type: "SET_ACTIVE_TAB", tab });
    }
  });

  const dismiss = useCallback(() => setError(null), []);
  return { error, dismiss };
}
