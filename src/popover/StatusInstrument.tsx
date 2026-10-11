// The core motif (reference 05, UI prompt §6.5): a dot hanging on a thin ink
// line from a tick on the frame's top edge. Two ticks are labelled HY2 and
// REALITY. Static in U2.1; the draw / travel motion comes in U2.4.
import type { Protocol } from "../api/bindings";
import type { Tone } from "./view";

const MODULE = 24;
/** Tick columns inside the 12-column frame (x = col × 24). */
export const TICK_X: Record<Protocol, number> = { hysteria2: 8 * MODULE, vless_reality: 11 * MODULE };
const LABEL: Record<Protocol, string> = { hysteria2: "HY2", vless_reality: "REALITY" };
const DROP = 30; // line length when dropped
const R = 5; // dot radius

const FILL: Record<Tone, string> = {
  ink: "var(--color-ink)",
  mustard: "var(--color-mustard)",
  cobalt: "var(--color-cobalt)",
  vermilion: "var(--color-vermilion)",
  ring: "var(--color-paper)",
};

export function StatusInstrument({ tick, tone, dropped, width = 288, height = 48 }: { tick: Protocol; tone: Tone; dropped: boolean; width?: number; height?: number }) {
  const x = TICK_X[tick] + 0.5;
  const lineEnd = dropped ? DROP : 0;
  const cy = lineEnd + R + 2;
  // Outline for the ring (DIRECT) and mustard (contrast, D13).
  const stroke = tone === "ring" ? "var(--color-ink)" : tone === "mustard" ? "var(--color-ink)" : "none";
  const strokeWidth = tone === "ring" ? 1.5 : 1;

  return (
    <svg width={width} height={height} className="absolute inset-0" aria-hidden>
      {(Object.keys(TICK_X) as Protocol[]).map((p) => (
        <g key={p}>
          {/* short inner mark continuing the frame tick */}
          <line x1={TICK_X[p] + 0.5} x2={TICK_X[p] + 0.5} y1={0} y2={4} stroke="var(--color-ink)" strokeWidth={1} />
          <text
            x={TICK_X[p] - 5}
            y={11}
            textAnchor="end"
            className="label"
            style={{ fontSize: 9, letterSpacing: "0.14em" }}
            fill={p === tick ? "var(--color-ink)" : "var(--color-ink-muted)"}
          >
            {LABEL[p]}
          </text>
        </g>
      ))}
      {lineEnd > 0 && <line x1={x} x2={x} y1={0} y2={lineEnd} stroke="var(--color-ink)" strokeWidth={1} />}
      <circle cx={x} cy={cy} r={R} fill={FILL[tone]} stroke={stroke} strokeWidth={strokeWidth} />
    </svg>
  );
}
