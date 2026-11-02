import { onCropDropped } from "../../ipc";
import { useAppDispatch } from "../../state";
import { useIpcEvent } from "../shared/useIpcEvent";
import { receiveCropDropped } from "./cropOpen";
import { useLatestCrop } from "./useLatestCrop";

/**
 * Opens what Rust reports in `crop-dropped` (design §7.2). Mount it once,
 * above the screens: a drop can arrive while another tab is shown, when Rust
 * has not yet been told the target changed.
 */
export function useCropDropped(): void {
  const dispatch = useAppDispatch();
  const getCrop = useLatestCrop();

  useIpcEvent(onCropDropped, (dropped) => {
    void receiveCropDropped(dispatch, dropped, getCrop);
  });
}
