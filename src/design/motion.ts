// Motion tokens (UI prompt §7.1, documented in docs/UI.md). Smooth, calm,
// mechanical: no bounce, no overshoot, no idle animation.
import type { Transition } from "motion/react";

/** Seconds (Motion's unit). CSS equivalents live in tokens.css. */
export const duration = {
  fast: 0.12, // hover, press
  base: 0.2, // state changes, toggles
  slow: 0.32, // sheets, content changes
  draw: 0.48, // line drawing, frame
} as const;

export const ease = {
  enter: [0.22, 1, 0.36, 1],
  exit: [0.4, 0, 1, 1],
  loop: [0.65, 0, 0.35, 1],
} as const satisfies Record<string, [number, number, number, number]>;

/** List stagger per item (s); stop staggering after `staggerMax` items. */
export const stagger = 0.035;
export const staggerMax = 6;
export const staggerDelay = (i: number) => Math.min(i, staggerMax) * stagger;

/** The only spring allowed: critically damped, no overshoot. */
export const spring: Transition = { type: "spring", bounce: 0, duration: duration.slow };

export const t = {
  fast: { duration: duration.fast, ease: ease.enter },
  base: { duration: duration.base, ease: ease.enter },
  slow: { duration: duration.slow, ease: ease.enter },
  draw: { duration: duration.draw, ease: ease.enter },
  exit: { duration: duration.fast, ease: ease.exit },
} satisfies Record<string, Transition>;
