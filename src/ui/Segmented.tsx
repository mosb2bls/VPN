// Segmented control: an ink block slides under the selected option (motion
// moment 5). Keyboard: arrow keys move the selection (radio group).
import type { KeyboardEvent } from "react";
import { motion } from "motion/react";
import { segmentTimeline } from "../design/motion";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  /** Option not available (e.g. TUN on Windows); `title` says why. */
  disabled?: boolean;
  title?: string;
}

export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  disabled = false,
}: {
  label: string;
  options: SegmentedOption<T>[];
  value: T;
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  const idx = Math.max(0, options.findIndex((o) => o.value === value));
  const onKey = (e: KeyboardEvent) => {
    const d = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    // Next enabled option in that direction (skips e.g. TUN on Windows).
    for (let step = 1; step < options.length; step++) {
      const o = options[(idx + d * step + options.length * step) % options.length];
      if (!o.disabled) return onChange(o.value);
    }
  };
  return (
    <div
      role="radiogroup"
      aria-label={label}
      aria-disabled={disabled || undefined}
      onKeyDown={onKey}
      className="relative grid h-[22px] border border-ink"
      style={{ gridTemplateColumns: `repeat(${options.length}, 1fr)` }}
    >
      {/* Moves by transform (x in % of its own width = one option). */}
      <motion.span
        aria-hidden
        className="absolute inset-y-0 left-0 bg-ink"
        style={{ width: `${100 / options.length}%` }}
        initial={false}
        animate={{ x: `${idx * 100}%` }}
        transition={segmentTimeline.slide}
      />
      {options.map((o, i) => {
        const on = i === idx;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on ? 0 : -1}
            disabled={disabled || o.disabled}
            title={o.title}
            onClick={() => onChange(o.value)}
            className={`label relative z-10 border-l border-ink first:border-l-0 transition-colors duration-base ease-enter ${
              on ? "text-paper" : "text-ink hover:bg-ink/5"
            } focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt disabled:opacity-40`}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
