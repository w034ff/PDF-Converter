import { useEffect, useRef } from "react";
import type { UnlistenFn } from "../../ipc";

/**
 * Calls `handler` with each event of `subscribe`, one of the `on…` functions
 * of the IPC layer, while the component is mounted. It subscribes once, so no
 * event is missed between renders, and always calls the latest `handler`.
 */
export function useIpcEvent<T>(
  subscribe: (handler: (payload: T) => void) => Promise<UnlistenFn>,
  handler: (payload: T) => void,
): void {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;

    void subscribe((payload) => latest.current(payload)).then((stop) => {
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
  }, [subscribe]);
}
