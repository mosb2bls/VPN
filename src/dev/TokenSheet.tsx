// U1.2 token sheet: every design token, the frame/grid at real popover size,
// and mono / width / grid comparisons for the owner to decide on.
import { useState, type ReactNode } from "react";
import { motion } from "motion/react";
import { duration, ease } from "../design/motion";
import { Frame, MetaLabel } from "../ui/Frame";

// ── contrast (WCAG 2.x) ──
const hex = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const lum = (rgb: number[]) => {
  const c = rgb.map((v) => {
    const x = v / 255;
    return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const ratio = (a: number[], b: number[]) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};
const over = (rgb: number[], alpha: number, bg: number[]) => rgb.map((v, i) => Math.round(v * alpha + bg[i] * (1 - alpha)));
const PAPER = hex("#F2EEE5");
const INK = hex("#161512");

interface Swatch {
  name: string;
  value: string;
  rgb: number[];
  role: string;
  /** How the token is allowed to be used, which decides the contrast that matters. */
  use: "text" | "fill" | "line";
}
const sw = (name: string, value: string, role: string, use: Swatch["use"], alpha?: number): Swatch => ({
  name,
  value,
  role,
  use,
  rgb: alpha === undefined ? hex(value) : over(INK, alpha, PAPER),
});
const SWATCHES: Swatch[] = [
  sw("paper", "#F2EEE5", "Window background", "fill"),
  sw("paper-raised", "#F7F4EE", "Sheets, cards, inputs", "fill"),
  sw("ink", "#161512", "Text, frame, primary controls", "text"),
  sw("ink-muted", "#625F58", "Secondary text, metadata", "text"),
  sw("grid", "ink 9 %", "Grid hairlines", "line", 0.09),
  sw("line", "ink 20 %", "Dividers, input borders", "line", 0.2),
  sw("cobalt", "#2439BC", "Connected · selection · focus", "text"),
  sw("vermilion", "#D34F2E", "Error / blocked — dot, fill, border", "fill"),
  sw("vermilion-text", "#B8401F", "Red text when unavoidable", "text"),
  sw("mustard", "#E7C24E", "Connecting / degraded — fill + ink text", "fill"),
];

function Section({ n, title, children }: { n: string; title: string; children: ReactNode }) {
  return (
    <section className="border-t border-line pt-4">
      <div className="mb-4 flex justify-between">
        <MetaLabel>{title}</MetaLabel>
        <MetaLabel muted>{n}</MetaLabel>
      </div>
      {children}
    </section>
  );
}

function Pass({ r, min }: { r: number; min: number }) {
  return (
    <span className={r >= min ? "text-ink" : "text-vermilion-text"}>
      {r.toFixed(2)}:1 {r >= min ? "✓" : "✕"}
    </span>
  );
}

export function TokenSheet() {
  const [grain, setGrain] = useState(true);
  return (
    <div className="paper-grain min-h-screen px-10 py-8 text-body">
      <div className="mx-auto flex max-w-[1180px] flex-col gap-10">
        <header className="flex items-end justify-between">
          <div>
            <MetaLabel muted>EdgeClient · U1.2</MetaLabel>
            <h1 className="mt-1 font-display text-[40px] leading-none font-[850] [font-stretch:82%]">TOKENS</h1>
          </div>
          <MetaLabel muted>docs/UI.md §3 · src/design/</MetaLabel>
        </header>

        <Section n="01" title="Colour">
          <div className="grid grid-cols-5 gap-4">
            {SWATCHES.map((s) => {
              const onPaper = ratio(s.rgb, PAPER);
              const inkOn = ratio(INK, s.rgb);
              return (
                <div key={s.name} className="flex flex-col gap-2">
                  <div className="h-16 border border-line" style={{ background: `rgb(${s.rgb.join(" ")})` }} />
                  <div className="flex justify-between">
                    <MetaLabel>{s.name}</MetaLabel>
                    <MetaLabel muted>{s.value}</MetaLabel>
                  </div>
                  <div className="text-secondary text-ink-muted">{s.role}</div>
                  <div className="label text-ink-muted">
                    {s.use === "text" && (
                      <>
                        TEXT ON PAPER <Pass r={onPaper} min={4.5} />
                      </>
                    )}
                    {s.use === "fill" && s.name !== "paper" && s.name !== "paper-raised" && (
                      <>
                        INK TEXT ON IT <Pass r={inkOn} min={4.5} />
                        <br />
                        VS PAPER <Pass r={onPaper} min={3} />
                      </>
                    )}
                    {s.use === "line" && <>DECORATIVE · {onPaper.toFixed(2)}:1</>}
                  </div>
                </div>
              );
            })}
          </div>
          <p className="mt-4 text-secondary text-ink-muted">
            Text needs 4.5:1. Fills and borders that carry state need 3:1 against paper and always come with a text label.
            Vermilion vs paper is 3.6:1 — fine for the dot and borders, never for small red text (use vermilion-text), and neither ink nor paper text reaches 4.5:1 on a vermilion fill. Mustard vs paper is only 1.5:1, so a mustard dot needs an ink outline.
          </p>
        </Section>

        <Section n="02" title="State colour in context (static)">
          <div className="flex gap-10">
            {[
              { l: "OFF", c: "var(--color-ink)" },
              { l: "CONNECTING", c: "var(--color-mustard)" },
              { l: "CONNECTED", c: "var(--color-cobalt)" },
              { l: "ERROR · BLOCKED", c: "var(--color-vermilion)" },
              { l: "DIRECT — NOT PROXIED", c: "ring" },
            ].map((d) => (
              <div key={d.l} className="flex items-center gap-2">
                <span
                  className="inline-block size-[14px] rounded-full"
                  style={d.c === "ring" ? { border: "1.5px solid var(--color-ink)" } : { background: d.c }}
                />
                <MetaLabel>{d.l}</MetaLabel>
              </div>
            ))}
          </div>
        </Section>

        <Section n="03" title="Type">
          <div className="grid grid-cols-2 gap-10">
            <div className="flex flex-col gap-5">
              <MetaLabel muted>State word · Archivo 850 · 30 px · width 82 % · box = popover content width 264 px</MetaLabel>
              {["CONNECTED", "FAILING OVER", "NOT RESTORED", "DIRECT — NOT PROXIED"].map((w) => (
                <div key={w} className="w-[264px] outline outline-1 outline-grid">
                  <div className="state-word">{w}</div>
                </div>
              ))}
              <MetaLabel muted>Width axis for “FAILING OVER” (decision)</MetaLabel>
              {[70, 82, 92, 100].map((wd) => (
                <div key={wd} className="flex w-[340px] items-baseline gap-3">
                  <span className="w-[36px] label text-ink-muted">{wd}%</span>
                  <span className="state-word" style={{ fontStretch: `${wd}%` }}>
                    FAILING OVER
                  </span>
                </div>
              ))}
            </div>
            <div className="flex flex-col gap-5">
              <MetaLabel muted>Numbers · Archivo 20 px · tabular figures</MetaLabel>
              <div className="flex gap-6 font-display text-number font-[650] tabular">
                <span>↑ 12.4 MB/s</span>
                <span>↓ 118 KB/s</span>
                <span>48 ms</span>
              </div>
              <div className="font-display text-number font-[650] tabular">02:34:10 · 11:11:11 · 00:00:00</div>
              <MetaLabel muted>Body 13 px · secondary 12 px muted</MetaLabel>
              <p className="max-w-[300px] text-body">Only apps that use the system proxy are covered. Use TUN for whole-device protection.</p>
              <p className="max-w-[300px] text-secondary text-ink-muted">
                HY2: 3 consecutive probe failures — UDP may be blocked on this network.
              </p>
            </div>
          </div>

          <div className="mt-8 grid grid-cols-2 gap-10">
            {[
              { name: "IBM Plex Mono", cls: "font-mono" },
              { name: "JetBrains Mono", cls: "font-mono-alt" },
            ].map((m) => (
              <div key={m.name} className={`${m.cls} flex flex-col gap-3 border border-line p-4`}>
                <div className="text-[10px] tracking-[0.14em] text-ink-muted uppercase">Mono candidate · {m.name}</div>
                <div className="flex justify-between text-[10px] tracking-[0.14em] uppercase">
                  <span>EdgeClient</span>
                  <span>01 / Status</span>
                </div>
                <div className="flex justify-between text-[10px] tracking-[0.14em] uppercase">
                  <span>HY2 · UDP 24443</span>
                  <span>02:34:10</span>
                </div>
                <div className="flex gap-2 text-[10px] tracking-[0.1em]">
                  <span className="border border-ink px-1 py-px">HY2</span>
                  <span className="border border-ink px-1 py-px">REALITY</span>
                  <span className="bg-ink px-1 py-px text-paper">RULE</span>
                </div>
                <div className="text-[11px] leading-4 whitespace-pre">
                  19:42:10 info  core   outbound/hysteria2[London HY2]: ready
                  <br />
                  19:42:13 warn  app    Failover suppressed: dwell 5 min
                  <br />
                  0123456789 Il1| O0 ↑↓ —
                </div>
              </div>
            ))}
          </div>
        </Section>

        <Section n="04" title="Space · 4 px base">
          <div className="flex items-end gap-5">
            {[4, 8, 12, 16, 24, 32, 48].map((s) => (
              <div key={s} className="flex flex-col items-start gap-1">
                <div className="bg-ink" style={{ width: s, height: s }} />
                <MetaLabel muted>{s}</MetaLabel>
              </div>
            ))}
            <div className="ml-10 flex items-end gap-6">
              <div className="flex flex-col gap-1">
                <div className="h-8 w-16 border border-ink" />
                <MetaLabel muted>radius 0</MetaLabel>
              </div>
              <div className="flex flex-col gap-1">
                <div className="h-8 w-16 rounded-xs border border-ink" />
                <MetaLabel muted>radius 2</MetaLabel>
              </div>
              <div className="flex flex-col gap-1">
                <div className="h-8 w-16 border border-line" />
                <MetaLabel muted>border 1 · line</MetaLabel>
              </div>
            </div>
          </div>
        </Section>

        <Section n="05" title="Frame + grid at real popover size (decision: module)">
          <div className="mb-3 flex gap-3">
            <button className="label border border-ink px-2 py-1" onClick={() => setGrain((g) => !g)}>
              Grain: {grain ? "on" : "off"}
            </button>
          </div>
          <div className="flex gap-10">
            {[
              { module: 24, cols: 12, rows: 17 },
              { module: 32, cols: 9, rows: 12 },
            ].map((g) => (
              <figure key={g.module} className="flex flex-col gap-2">
                <MetaLabel muted>
                  Module {g.module} · {g.cols} × {g.rows} · frame {g.cols * g.module} × {g.rows * g.module}
                </MetaLabel>
                <div className="shadow-sheet">
                  <Frame
                    width={340}
                    height={480}
                    {...g}
                    grain={grain}
                    tl="EdgeClient"
                    tr="01 / Status"
                    bl="HY2 · UDP 24443"
                    br={<span className="tabular">02:34:10</span>}
                  >
                    <div className="p-4">
                      <MetaLabel muted>State</MetaLabel>
                      <div className="state-word mt-1">CONNECTED</div>
                    </div>
                  </Frame>
                </div>
              </figure>
            ))}
            <div className="flex max-w-[300px] flex-col gap-3 text-secondary text-ink-muted">
              <p>Both frames are 288 px wide (26 px side margins leave room for the registration dashes).</p>
              <p>
                <b className="text-ink">24 px</b> gives 12 columns: finer alignment for a dense popover (rows of controls are 24 or 48 px).
              </p>
              <p>
                <b className="text-ink">32 px</b> is closer to the reference’s airy 14 × 7 look but leaves only 9 columns for
                segmented controls and stats.
              </p>
            </div>
          </div>
        </Section>

        <Section n="06" title="Elevation · paper card">
          <div className="flex gap-10">
            <div className="h-28 w-64 rounded-xs bg-paper-raised p-4 shadow-sheet">
              <MetaLabel muted>Sheet / dropdown</MetaLabel>
              <p className="mt-2 text-secondary">Paper-raised with a soft, low-contrast drop shadow. No blur, no glass.</p>
            </div>
            <div className="h-28 w-64 border border-line bg-paper-raised p-4">
              <MetaLabel muted>Input / inline card</MetaLabel>
              <p className="mt-2 text-secondary">Flat: 1 px line border, no shadow.</p>
            </div>
          </div>
        </Section>

        <Section n="07" title="Motion tokens">
          <MotionTable />
        </Section>
      </div>
    </div>
  );
}

function MotionTable() {
  const [run, setRun] = useState(0);
  const rows: { name: string; d: number; e: readonly number[] }[] = [
    { name: "fast · enter", d: duration.fast, e: ease.enter },
    { name: "base · enter", d: duration.base, e: ease.enter },
    { name: "slow · enter", d: duration.slow, e: ease.enter },
    { name: "draw · enter", d: duration.draw, e: ease.enter },
    { name: "fast · exit", d: duration.fast, e: ease.exit },
    { name: "draw · loop", d: duration.draw, e: ease.loop },
  ];
  return (
    <div className="flex flex-col gap-3">
      <button className="label w-fit border border-ink px-2 py-1" onClick={() => setRun((r) => r + 1)}>
        Replay
      </button>
      {rows.map((r) => (
        <div key={r.name} className="flex items-center gap-4">
          <span className="label w-[120px] text-ink-muted">{r.name}</span>
          <span className="label w-[180px] text-ink-muted tabular">
            {Math.round(r.d * 1000)} ms · [{r.e.join(", ")}]
          </span>
          <div className="relative h-px w-[320px] bg-line">
            <motion.span
              key={run}
              className="absolute -top-[5px] size-[11px] rounded-full bg-ink"
              initial={{ x: 0 }}
              animate={{ x: 309 }}
              transition={{ duration: r.d, ease: r.e as [number, number, number, number] }}
            />
          </div>
        </div>
      ))}
      <p className="text-secondary text-ink-muted">Stagger 35 ms per item, max 6. Springs only with bounce 0.</p>
    </div>
  );
}
