import { useEffect } from "react";
import { onJobFinished, onJobItem, onJobProgress } from "../../ipc";
import { useAppDispatch } from "../../state";

/**
 * Feeds `job-progress`, `job-item` and `job-finished` (design §7.2) into the
 * job state. Mount it once, above both screens: the events carry no screen,
 * and a conversion keeps running when the user switches tabs.
 */
export function useJobEvents(): void {
  const dispatch = useAppDispatch();

  useEffect(() => {
    let disposed = false;
    const unlistens: (() => void)[] = [];

    // `listen` resolves after the component may already have unmounted, so a
    // late subscription is dropped as soon as it arrives.
    const keep = (unlisten: () => void) => {
      if (disposed) {
        unlisten();
      } else {
        unlistens.push(unlisten);
      }
    };

    void onJobProgress((progress) =>
      dispatch({ type: "JOB_PROGRESS", progress }),
    ).then(keep);
    void onJobItem((item) => dispatch({ type: "JOB_ITEM", item })).then(keep);
    void onJobFinished((finished) =>
      dispatch({ type: "JOB_FINISHED", finished }),
    ).then(keep);

    return () => {
      disposed = true;
      for (const unlisten of unlistens) {
        unlisten();
      }
    };
  }, [dispatch]);
}
