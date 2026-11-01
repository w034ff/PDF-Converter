import { useLayoutEffect, useState, type RefObject } from "react";

/** The size of a scrolling area's content box; zero until it is measured. */
export interface Room {
  width: number;
  height: number;
}

export function useRoom(scrollRef: RefObject<HTMLElement | null>): Room {
  const [room, setRoom] = useState<Room>({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (element === null) {
      return;
    }
    setRoom({ width: element.clientWidth, height: element.clientHeight });
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries.at(-1);
      if (entry !== undefined) {
        const { width, height } = entry.contentRect;
        setRoom({ width, height });
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [scrollRef]);
  return room;
}
