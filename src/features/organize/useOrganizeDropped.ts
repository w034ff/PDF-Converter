import { useEffect } from "react";
import { onOrganizeDropped } from "../../ipc";
import { useAppDispatch } from "../../state";
import { noticeOf } from "./organizeNotice";

/**
 * Adds what Rust reports in `organize-dropped` (design §7.2) as an addition
 * from the dialog does. Mount it once, above the screens: a drop can arrive
 * while another tab is shown, when Rust has not yet been told the target
 * changed.
 */
export function useOrganizeDropped(): void {
  const dispatch = useAppDispatch();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void onOrganizeDropped((dropped) => {
      dispatch({ type: "ORGANIZE_PDFS_ADDED", items: dropped.added });
      dispatch({
        type: "ORGANIZE_NOTICE",
        notice: noticeOf(dropped.skipped, dropped.error),
      });
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
