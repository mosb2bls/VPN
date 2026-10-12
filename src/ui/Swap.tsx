// Cross-fades or slides content when `k` changes. Old and new content share one
// grid cell, so nothing jumps while both are on screen. `slideX` clips to its box.
import type { ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { swapTimeline as S } from "../design/motion";
import { useMotionOk } from "./motionGate";

export function Swap({
  k,
  children,
  variant = "fade",
  className = "",
}: {
  k: string;
  children: ReactNode;
  variant?: "fade" | "slideX";
  className?: string;
}) {
  const slide = variant === "slideX";
  const animate = useMotionOk();
  if (!animate)
    return (
      <span className={`inline-grid ${className}`}>
        <span className="min-w-0">{children}</span>
      </span>
    );
  return (
    <span className={`inline-grid ${slide ? "overflow-hidden" : ""} ${className}`}>
      <AnimatePresence initial={false}>
        <motion.span
          key={k}
          className="col-start-1 row-start-1 min-w-0"
          initial={slide ? { x: S.slideShift, opacity: 0 } : { opacity: 0 }}
          animate={slide ? { x: 0, opacity: 1, transition: S.slideIn } : { opacity: 1, transition: S.fade }}
          exit={slide ? { x: -S.slideShift, opacity: 0, transition: S.slideOut } : { opacity: 0, transition: S.fade }}
        >
          {children}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
