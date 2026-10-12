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
  // Call every hook unconditionally: `a() && b()` would skip b's hook when a is false,
  // React would see a different hook count, throw, and unmount the window (blank popover).
  const gate = useContext(MotionGate);
  const visible = usePageVisible();
  return gate && visible;
}
