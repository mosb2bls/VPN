// Press-and-hold to confirm (motion moment 9). A vermilion bar fills along the
// bottom edge while held; the label stays ink on paper (D14). Releasing early
// runs the bar back. Works with pointer and with Space / Enter held down.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { animate, motion, useMotionValue } from "motion/react";
import { holdTimeline as H } from "../design/motion";

export function HoldButton({
  children,
  onConfirm,
  disabled = false,
  title,
}: {
  children: ReactNode;
  onConfirm: () => void;
  disabled?: boolean;
  title?: string;
}) {
  const progress = useMotionValue(0);
  const [holding, setHolding] = useState(false);
  const timer = useRef<number | null>(null);
  const anim = useRef<ReturnType<typeof animate> | null>(null);

  const start = () => {
    if (disabled || timer.current !== null) return;
    setHolding(true);
    anim.current?.stop();
    anim.current = animate(progress, 1, H.fill);
    // Confirm on a plain timer — never rely on an animation callback.
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setHolding(false);
      progress.set(0);
      onConfirm();
    }, H.holdMs);
  };
  const cancel = () => {
    if (timer.current === null) return;
    window.clearTimeout(timer.current);
    timer.current = null;
    setHolding(false);
    anim.current?.stop();
    anim.current = animate(progress, 0, H.release);
  };
  useEffect(() => () => void (timer.current !== null && window.clearTimeout(timer.current)), []);

  return (
    <button
      type="button"
      disabled={disabled}
      title={title}
      aria-label={typeof children === "string" ? `${children} (press and hold)` : undefined}
      onPointerDown={(e) => {
        // Capture keeps the hold if the pointer drifts off the button; never let it block the hold.
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          /* unknown pointer id — hold still works, release ends it */
        }
        start();
      }}
      onPointerUp={cancel}
      onPointerCancel={cancel}
      onKeyDown={(e) => {
        if ((e.key === " " || e.key === "Enter") && !e.repeat) {
          e.preventDefault();
          start();
        }
      }}
      onKeyUp={(e) => (e.key === " " || e.key === "Enter") && cancel()}
      onBlur={cancel}
      className="label relative inline-flex h-8 items-center overflow-hidden border border-ink px-3 text-ink transition-colors duration-fast select-none hover:bg-ink/5 disabled:border-ink/30 disabled:text-ink-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt"
    >
      <span className="relative">{holding ? "Keep holding…" : children}</span>
      <motion.span aria-hidden className="absolute inset-x-0 bottom-0 h-[3px] origin-left bg-vermilion" style={{ scaleX: progress }} />
    </button>
  );
}
