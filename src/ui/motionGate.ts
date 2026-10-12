// Whether small UI animations (number rolls, swaps) may run. Off while the
// popover is hidden/parked or the page is not visible: animations pause there,
// and exiting elements would pile up (e.g. a digit per second for hours).
import { createContext, useContext, useSyncExternalStore } from "react";

export const MotionGate = createContext(true);

export function usePageVisible() {
  return useSyncExternalStore(
    (cb) => {
      document.addEventListener("visibilitychange", cb);
      return () => document.removeEventListener("visibilitychange", cb);
    },
    () => document.visibilityState === "visible",
  );
}

/** True when it is safe to animate right now. */
export function useMotionOk() {
  return useContext(MotionGate) && usePageVisible();
}
