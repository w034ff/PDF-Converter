import { onJobFinished, onJobItem, onJobProgress } from "../../ipc";
import { useAppDispatch } from "../../state";
import { useIpcEvent } from "../shared/useIpcEvent";

/**
 * Feeds `job-progress`, `job-item` and `job-finished` (design §7.2) into the
 * job state. Mount it once, above both screens: the events carry no screen,
 * and a conversion keeps running when the user switches tabs.
 */
export function useJobEvents(): void {
  const dispatch = useAppDispatch();

  useIpcEvent(onJobProgress, (progress) =>
    dispatch({ type: "JOB_PROGRESS", progress }),
  );
  useIpcEvent(onJobItem, (item) => dispatch({ type: "JOB_ITEM", item }));
  useIpcEvent(onJobFinished, (finished) =>
    dispatch({ type: "JOB_FINISHED", finished }),
  );
}
