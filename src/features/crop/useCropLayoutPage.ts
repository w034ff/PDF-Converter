import { onLayoutPage } from "../../ipc";
import { useAppDispatch } from "../../state";
import { useIpcEvent } from "../shared/useIpcEvent";

/**
 * Feeds `layout-page` (design §7.2), the answer of "全ページの候補" for one
 * page, into the crop state. Mount it once, above the screens: the events
 * come whichever tab is shown.
 */
export function useCropLayoutPage(): void {
  const dispatch = useAppDispatch();

  useIpcEvent(onLayoutPage, ({ id, page, candidates, error }) => {
    dispatch({ type: "CROP_LAYOUT_PAGE", id, page, candidates, error });
  });
}
