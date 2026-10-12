// Odometer-style number display (motion moment 4). Characters are matched from
// the right so units ("MB/s", "ms") keep their place; a changed digit rolls,
// any other changed character cross-fades. Use with `tabular` figures.
import { useEffect, useRef } from "react";
import { AnimatePresence, motion } from "motion/react";
import { rollTimeline as R } from "../design/motion";
import { useMotionOk } from "./motionGate";

const num = (s: string) => {
  const m = /-?\d+(?:\.\d+)?/.exec(s);
  if (!m) return null;
  let v = Number(m[0]);
  if (/GB/.test(s)) v *= 1e9;
  else if (/MB/.test(s)) v *= 1e6;
  else if (/KB/.test(s)) v *= 1e3;
  return v;
};

export function Rolling({ value, className = "" }: { value: string; className?: string }) {
  // Last committed value (updated after commit, so double renders don't lose it).
  const prev = useRef(value);
  useEffect(() => {
    prev.current = value;
  }, [value]);
  // Direction: +1 rolls up (value grew), -1 rolls down.
  const a = num(prev.current);
  const b = num(value);
  const dir = a !== null && b !== null && b < a ? -1 : 1;

  const animate = useMotionOk();
  if (!animate) return <span className={className}>{value}</span>;
  const chars = [...value];
  return (
    <span className={`inline-flex ${className}`} aria-label={value}>
      {chars.map((ch, i) => {
        const fromRight = chars.length - i; // stable slot identity
        const digit = /\d/.test(ch);
        return (
          <span key={fromRight} aria-hidden className="inline-grid overflow-hidden">
            <AnimatePresence initial={false} custom={dir}>
              <motion.span
                key={ch}
                custom={dir}
                className="col-start-1 row-start-1 whitespace-pre"
                variants={{
                  enter: (d: number) => (digit ? { y: d > 0 ? R.shift : `-${R.shift}`, opacity: 0 } : { opacity: 0 }),
                  center: { y: 0, opacity: 1 },
                  exit: (d: number) => (digit ? { y: d > 0 ? `-${R.shift}` : R.shift, opacity: 0 } : { opacity: 0 }),
                }}
                initial="enter"
                animate="center"
                exit="exit"
                transition={R.roll}
              >
                {ch}
              </motion.span>
            </AnimatePresence>
          </span>
        );
      })}
    </span>
  );
}
