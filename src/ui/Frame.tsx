// The instrument frame (reference 02/05): paper with grain, a hairline module
// grid, a 1.5 px ink frame, tick marks on the top edge at each grid column,
// registration dashes outside the corners, and four corner metadata labels.
import type { CSSProperties, ReactNode } from "react";

export interface FrameGeometry {
  /** Outer size (the window or card). */
  width: number;
  height: number;
  /** Grid module in px. */
  module: number;
  cols: number;
  rows: number;
}

/** Frame rectangle centred horizontally, vertically centred in the outer box. */
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
  /** Content laid out inside the frame rectangle. */
  children?: ReactNode;
  className?: string;
}

const TICK = 5; // tick length above the top edge
const DASH = 12; // registration dash length
const DASH_GAP = 6; // gap between frame and dash

export function Frame({ tl, tr, bl, br, grid = true, grain = true, children, className = "", ...g }: FrameProps) {
  const { x, y, w, h } = frameRect(g);
  const cols = Array.from({ length: g.cols - 1 }, (_, i) => x + (i + 1) * g.module);
  const rows = Array.from({ length: g.rows - 1 }, (_, i) => y + (i + 1) * g.module);
  const labelGap = 12; // clear of the 5 px ticks, like the reference

  return (
    <div
      className={`relative overflow-hidden text-ink ${grain ? "paper-grain" : "bg-paper"} ${className}`}
      style={{ width: g.width, height: g.height }}
    >
      <svg className="pointer-events-none absolute inset-0" width={g.width} height={g.height} aria-hidden>
        {grid && (
          <g stroke="var(--color-grid)" strokeWidth={1} shapeRendering="crispEdges">
            {cols.map((cx) => (
              <line key={`c${cx}`} x1={cx + 0.5} x2={cx + 0.5} y1={y} y2={y + h} />
            ))}
            {rows.map((ry) => (
              <line key={`r${ry}`} x1={x} x2={x + w} y1={ry + 0.5} y2={ry + 0.5} />
            ))}
          </g>
        )}
        <g stroke="var(--color-ink)" strokeWidth={1}>
          {cols.map((cx) => (
            <line key={`t${cx}`} x1={cx + 0.5} x2={cx + 0.5} y1={y - TICK} y2={y} />
          ))}
          {[y, y + h].map((ry) => (
            <g key={`d${ry}`}>
              <line x1={x - DASH_GAP - DASH} x2={x - DASH_GAP} y1={ry} y2={ry} />
              <line x1={x + w + DASH_GAP} x2={x + w + DASH_GAP + DASH} y1={ry} y2={ry} />
            </g>
          ))}
        </g>
        <rect x={x} y={y} width={w} height={h} fill="none" stroke="var(--color-ink)" strokeWidth={1.5} />
      </svg>

      <Corner style={{ left: x, bottom: g.height - y + labelGap }}>{tl}</Corner>
      <Corner style={{ right: g.width - x - w, bottom: g.height - y + labelGap }} align="right">
        {tr}
      </Corner>
      <Corner style={{ left: x, top: y + h + labelGap }}>{bl}</Corner>
      <Corner style={{ right: g.width - x - w, top: y + h + labelGap }} align="right">
        {br}
      </Corner>

      <div className="absolute" style={{ left: x, top: y, width: w, height: h }}>
        {children}
      </div>
    </div>
  );
}

function Corner({ children, style, align = "left" }: { children?: ReactNode; style: CSSProperties; align?: "left" | "right" }) {
  if (children == null) return null;
  return (
    <div className={`label absolute whitespace-nowrap text-ink ${align === "right" ? "text-right" : ""}`} style={style}>
      {children}
    </div>
  );
}

/** Small uppercase mono label. `muted` for secondary metadata. */
export function MetaLabel({ children, muted = false, className = "" }: { children: ReactNode; muted?: boolean; className?: string }) {
  return <span className={`label ${muted ? "text-ink-muted" : "text-ink"} ${className}`}>{children}</span>;
}
