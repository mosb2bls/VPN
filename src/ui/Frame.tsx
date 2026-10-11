// The instrument frame (reference 02/05): paper with grain, an optional
// hairline module grid, a 1.5 px ink frame, tick marks on the top edge at each
// grid column, registration dashes outside the corners, and four corner
// metadata labels. With `animate`, it plays the open sequence (motion moment 1)
// on mount — re-key it to replay.
import type { CSSProperties, ReactNode } from "react";
import { motion } from "motion/react";
import { openTimeline as T } from "../design/motion";

export interface FrameGeometry {
  /** Outer size (the window or card). */
  width: number;
  height: number;
  /** Grid module in px. */
  module: number;
  cols: number;
  rows: number;
}

/** Frame rectangle centred in the outer box. */
export function frameRect(g: FrameGeometry) {
  const w = g.cols * g.module;
  const h = g.rows * g.module;
  const x = Math.round((g.width - w) / 2);
  const y = Math.round((g.height - h) / 2);
  return { x, y, w, h };
}

export interface FrameProps extends FrameGeometry {
  /** Corner labels: above the frame (tl, tr) and below it (bl, br). */
  tl?: ReactNode;
  tr?: ReactNode;
  bl?: ReactNode;
  br?: ReactNode;
  grid?: boolean;
  grain?: boolean;
  /** Play the open sequence on mount. */
  animate?: boolean;
  /** Content laid out inside the frame rectangle. */
  children?: ReactNode;
  className?: string;
}

const TICK = 5; // tick length above the top edge
const DASH = 12; // registration dash length
const DASH_GAP = 6; // gap between frame and dash
const LABEL_GAP = 12; // clear of the 5 px ticks, like the reference

export function Frame({ tl, tr, bl, br, grid = true, grain = true, animate = false, children, className = "", ...g }: FrameProps) {
  const { x, y, w, h } = frameRect(g);
  const cols = Array.from({ length: g.cols - 1 }, (_, i) => x + (i + 1) * g.module);
  const rows = Array.from({ length: g.rows - 1 }, (_, i) => y + (i + 1) * g.module);
  // `initial={false}` renders the end state immediately (no animation).
  const init = <V,>(v: V) => (animate ? v : false);
  const tickT = (i: number) => ({ ...T.ticks, delay: T.ticks.delay + i * T.ticks.step });
  const lastTick = T.ticks.delay + cols.length * T.ticks.step;

  return (
    <div
      className={`relative overflow-hidden text-ink ${grain ? "paper-grain" : "bg-paper"} ${className}`}
      style={{ width: g.width, height: g.height }}
    >
      <svg className="pointer-events-none absolute inset-0" width={g.width} height={g.height} aria-hidden>
        {grid && (
          <motion.g
            stroke="var(--color-grid)"
            strokeWidth={1}
            shapeRendering="crispEdges"
            initial={init({ opacity: 0 })}
            animate={{ opacity: 1 }}
            transition={T.labels}
          >
            {cols.map((cx) => (
              <line key={`c${cx}`} x1={cx + 0.5} x2={cx + 0.5} y1={y} y2={y + h} />
            ))}
            {rows.map((ry) => (
              <line key={`r${ry}`} x1={x} x2={x + w} y1={ry + 0.5} y2={ry + 0.5} />
            ))}
          </motion.g>
        )}
        <g stroke="var(--color-ink)" strokeWidth={1}>
          {cols.map((cx, i) => (
            <motion.line
              key={`t${cx}`}
              x1={cx + 0.5}
              x2={cx + 0.5}
              y1={y - TICK}
              y2={y}
              initial={init({ opacity: 0 })}
              animate={{ opacity: 1 }}
              transition={tickT(i)}
            />
          ))}
          {[y, y + h].map((ry) => (
            <motion.g key={`d${ry}`} initial={init({ opacity: 0 })} animate={{ opacity: 1 }} transition={{ ...T.ticks, delay: lastTick }}>
              <line x1={x - DASH_GAP - DASH} x2={x - DASH_GAP} y1={ry} y2={ry} />
              <line x1={x + w + DASH_GAP} x2={x + w + DASH_GAP + DASH} y1={ry} y2={ry} />
            </motion.g>
          ))}
        </g>
        {/* The outline draws from the top-left corner, clockwise, like a plotter. */}
        <motion.rect
          x={x}
          y={y}
          width={w}
          height={h}
          fill="none"
          stroke="var(--color-ink)"
          strokeWidth={1.5}
          initial={init({ pathLength: 0 })}
          animate={{ pathLength: 1 }}
          transition={T.frame}
        />
      </svg>

      <Corner animate={animate} style={{ left: x, bottom: g.height - y + LABEL_GAP }}>
        {tl}
      </Corner>
      <Corner animate={animate} style={{ right: g.width - x - w, bottom: g.height - y + LABEL_GAP }} align="right">
        {tr}
      </Corner>
      <Corner animate={animate} style={{ left: x, top: y + h + LABEL_GAP }}>
        {bl}
      </Corner>
      <Corner animate={animate} style={{ right: g.width - x - w, top: y + h + LABEL_GAP }} align="right">
        {br}
      </Corner>

      <motion.div
        className="absolute"
        style={{ left: x, top: y, width: w, height: h }}
        initial={init({ opacity: 0, y: T.contentShift })}
        animate={{ opacity: 1, y: 0 }}
        transition={T.content}
      >
        {children}
      </motion.div>
    </div>
  );
}

function Corner({
  children,
  style,
  align = "left",
  animate,
}: {
  children?: ReactNode;
  style: CSSProperties;
  align?: "left" | "right";
  animate: boolean;
}) {
  if (children == null) return null;
  return (
    <motion.div
      className={`label absolute whitespace-nowrap text-ink ${align === "right" ? "text-right" : ""}`}
      style={style}
      initial={animate ? { opacity: 0 } : false}
      animate={{ opacity: 1 }}
      transition={T.labels}
    >
      {children}
    </motion.div>
  );
}

/** Small uppercase mono label. `muted` for secondary metadata. */
export function MetaLabel({ children, muted = false, className = "" }: { children: ReactNode; muted?: boolean; className?: string }) {
  return <span className={`label ${muted ? "text-ink-muted" : "text-ink"} ${className}`}>{children}</span>;
}
