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

/**
 * Motion moment 1 — popover open (UI.md §5). Everything finishes by ~0.38 s:
 * paper fades in, the frame outline draws, ticks fade in left → right, corner
 * labels follow, content settles 6 px upward. Hide is a quick fade.
 */
export const openTimeline = {
  paper: { duration: duration.fast, ease: ease.enter },
  frame: { delay: 0.02, duration: 0.34, ease: ease.enter },
  ticks: { delay: 0.1, step: 0.014, duration: duration.fast, ease: ease.enter },
  labels: { delay: 0.16, duration: duration.base, ease: ease.enter },
  content: { delay: 0.1, duration: 0.28, ease: ease.enter },
  /** px the content settles upward. Kept out of `content`: a `y` key in a
   *  transition is read as a per-property override (→ default spring). */
  contentShift: 6,
  hide: { duration: duration.fast, ease: ease.exit },
} as const;

/**
 * Motion moment 2 — connect / disconnect (UI.md §5, D26). The state word is a
 * one-line counter: the old word slides up and out, the new one slides up
 * from below into its colour. The sub-line cross-fades; the primary button
 * cross-fades its label while its fill transitions (CSS, `duration-base`).
 */
export const stateTimeline = {
  wordIn: { duration: duration.slow, ease: ease.enter },
  wordOut: { duration: duration.fast + 0.04, ease: ease.exit },
  /** px travelled by the word (clipped by its 32 px line box). */
  wordShift: 14,
  sub: { duration: duration.fast, ease: ease.enter },
  label: { duration: duration.fast, ease: ease.enter },
} as const;

/**
 * Motion moment 3 — failover (UI.md §5, D26). The word's amber → blue changes
 * come from moment 2. Here: the node row slides sideways to the new node, and
 * the node label, transport corner label and last-event line cross-fade.
 */
export const swapTimeline = {
  slideIn: { duration: duration.slow, ease: ease.enter },
  slideOut: { duration: duration.fast + 0.04, ease: ease.exit },
  /** px the node row travels sideways. */
  slideShift: 16,
  fade: { duration: duration.base, ease: ease.enter },
} as const;

/**
 * Motion moment 4 — numbers (UI.md §5). Speeds, latency and session time roll
 * digit by digit like an odometer (up when the value grows), with tabular
 * figures so nothing shifts. Kept short: these change every second.
 */
export const rollTimeline = {
  roll: { duration: 0.18, ease: ease.enter },
  /** Fraction of the line height a digit travels (clipped). */
  shift: "70%",
} as const;
