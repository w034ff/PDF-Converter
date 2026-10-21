import { useEffect } from "react";
import { onLayoutPage } from "../../ipc";
import { useAppDispatch } from "../../state";

/**
 * Feeds `layout-page` (design §7.2), the answer of "全ページの候補" for one
 * page, into the crop state. Mount it once, above the screens: the events
 * come whichever tab is shown.
 */
export function useCropLayoutPage(): void {
  const dispatch = useAppDispatch();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void onLayoutPage(({ id, page, candidates, error }) => {
      dispatch({ type: "CROP_LAYOUT_PAGE", id, page, candidates, error });
    }).then((stop) => {
      // `listen` resolves after the component may already have unmounted.
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [dispatch]);
}
