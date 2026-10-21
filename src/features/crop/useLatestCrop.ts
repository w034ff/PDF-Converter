import { useCallback, useEffect, useRef } from "react";
import { useAppState } from "../../state";
import type { CropGetter } from "./cropOpen";

/**
 * A function that returns the crop state as it is when it is called, not as
 * it was when it was made: what opens a PDF waits for a dialog or for Rust
 * and then decides by the areas there are by that time.
 */
export function useLatestCrop(): CropGetter {
  const { crop } = useAppState();
  const latest = useRef(crop);
  useEffect(() => {
    latest.current = crop;
  });
  return useCallback(() => latest.current, []);
}
