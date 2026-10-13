// Tray popover, 340 × 480 (SPEC §2.8). Laid out on a 24 px rhythm: the frame
// is 12 × 17 modules and every block spans whole rows (UI.md D10). The grid is
// not drawn (D15); the state is carried by the colour of the word (D21).
//
//   rows 1–4   state word + one-line consequence / progress
//   rows 5–6   node (opens the switcher sheet)
//   rows 7–8   primary action
//   row  9     routing mode
//   row  10    capture mode
//   rows 11–12 coverage note         ┐ replaced by the state's notice
//   rows 13–15 live speeds + latency ┘ (rows 11–15) when there is one
//   row  16    last event
//   row  17    footer: settings · restore networking · quit
import { useEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { commands, on, windowLabel } from "../api/client";
import { openTimeline, stateTimeline as ST } from "../design/motion";
import type { CaptureMode, ImportResult, RoutingMode, Status } from "../api/bindings";
import type { Result } from "../api/types";
import { protocolBadge, rate, since, transport } from "../lib/format";
import { applyStatus, selectLastEvent, selectTunLabel, useApp } from "../state/store";
import { Frame, MetaLabel } from "../ui/Frame";
import { Segmented } from "../ui/Segmented";
import { MotionGate, useMotionOk } from "../ui/motionGate";
import { Rolling } from "../ui/Rolling";
import { Swap } from "../ui/Swap";
import { NodeSheet } from "./NodeSheet";
import { TONE_CLASS, derivePopoverView, isProxying, type Action, type Notice, type Tone } from "./view";
import { APP_NAME } from "../lib/brand";

const ROUTING = [
  { value: "rule", label: "Rule" },
  { value: "global", label: "Global" },
  { value: "direct", label: "Direct" },
] satisfies { value: RoutingMode; label: string }[];
const CAPTURE = [
  { value: "system_proxy", label: "System proxy" },
  { value: "tun", label: "TUN" },
] satisfies { value: CaptureMode; label: string }[];

/** Events older than this, or from before the current connection, are shown muted. */
const STALE_MS = 10 * 60_000;

function useNow(ms: number) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

const isStatus = (d: unknown): d is Status => !!d && typeof d === "object" && "state" in d && "captureMode" in d;

export function Popover() {
  const status = useApp((s) => s.status);
  const reason = useApp((s) => s.statusReason);
  const nodes = useApp((s) => s.nodes);
  const helper = useApp((s) => s.helper);
  const restore = useApp((s) => s.restore);
  const stats = useApp((s) => s.stats);
  const lastEvent = useApp(selectLastEvent);
  const now = useNow(1000);
  const [sheet, setSheet] = useState(false);
  /** Outcome of the last action taken in the popover (shown in the "Last" row). */
  const [actionNote, setActionNote] = useState<{ text: string; tone: "error" | "warn" | "info" } | null>(null);

  // Motion moment 1: every show from the tray replays the open sequence.
  const [openSeq, setOpenSeq] = useState(0);
  /** "esc": fade out, then ask the backend to hide. "parked": hidden by the backend — invisible until shown. */
  const [hiding, setHiding] = useState<null | "esc" | "parked">(null);
  const hidingRef = useRef(hiding);
  hidingRef.current = hiding;
  /** Show without the open animation (page not visible, or recovering from a stuck state). */
  const [instant, setInstant] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  const reveal = (animated: boolean) => {
    setSheet(false);
    setHiding(null);
    setInstant(!animated);
    setOpenSeq((n) => n + 1);
  };

  useEffect(() => {
    const offs: (() => void)[] = [];
    void on("popoverShown", () => reveal(document.visibilityState !== "hidden")).then((u) => offs.push(u));
    // The only park signal (SPEC 1.2.0): the backend sends it after every hide, whatever the
    // cause. Window blur / visibility are NOT used to park: on macOS a stray blur can arrive
    // just after the popover is shown, which left it visible but blank (owner bug report).
    void on("popoverHidden", () => setHiding((h) => h ?? "parked")).then((u) => offs.push(u));
    return () => offs.forEach((u) => u());
  }, []);

  // Self-heal: if the real popover window is focused, becomes visible or is clicked while its
  // content is parked, show it at once — the popover must never stay blank.
  useEffect(() => {
    if (windowLabel() !== "popover") return;
    const heal = () => {
      if (hidingRef.current === "parked" && document.visibilityState === "visible") reveal(false);
    };
    window.addEventListener("focus", heal);
    window.addEventListener("pointerdown", heal);
    document.addEventListener("visibilitychange", heal);
    return () => {
      window.removeEventListener("focus", heal);
      window.removeEventListener("pointerdown", heal);
      document.removeEventListener("visibilitychange", heal);
    };
  }, []);

  // Watchdog: if the open animation has not brought the content in shortly after a show
  // (e.g. the webview had not resumed animating yet), snap it to visible.
  useEffect(() => {
    if (openSeq === 0 || instant) return;
    const t = window.setTimeout(() => {
      const el = wrapRef.current;
      if (hidingRef.current === null && el && Number(getComputedStyle(el).opacity) < 0.99) reveal(false);
    }, 900);
    return () => window.clearTimeout(t);
  }, [openSeq, instant]);

  // Esc hides the popover (SPEC §2.8). The sheet stops its own Esc first.
  useEffect(() => {
    // Hiding must never depend on an animation callback (those pause in hidden/throttled
    // webviews): fade, and hide on a plain timer of the same length.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      setHiding("esc");
      window.setTimeout(async () => {
        const r = await commands.hidePopover();
        // If the window could not be hidden, never leave it faded out.
        if (r.status === "error") reveal(false);
        else setHiding("parked");
      }, openTimeline.hide.duration * 1000);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const run = async (p: Promise<Result<unknown>>) => {
    setActionNote(null);
    const r = await p;
    if (r.status === "error") setActionNote({ text: r.error.message, tone: "error" });
    else if (isStatus(r.data)) applyStatus(r.data);
    return r;
  };
  const importClipboard = async () => {
    const r = await run(commands.importFromClipboard());
    if (r.status !== "ok") return;
    const { added, errors } = r.data as ImportResult;
    const text = `${added.length} node${added.length === 1 ? "" : "s"} added${errors.length ? `, ${errors.length} line${errors.length === 1 ? "" : "s"} rejected — see Settings › Nodes` : ""}`;
    setActionNote({ text, tone: errors.length ? "warn" : "info" });
  };
  const act = (a: Action) => {
    if (a === "connect") void run(commands.connect());
    else if (a === "disconnect") void run(commands.disconnect());
    else if (a === "restore") void run(commands.restoreNetworking());
    else if (a === "open_helper") void run(commands.openSettings("helper"));
    else if (a === "open_nodes") void run(commands.openSettings("nodes"));
    else if (a === "open_logs") void run(commands.openSettings("logs"));
    else if (a === "import_clipboard") void importClipboard();
    else if (a === "open_safety") void run(commands.openSettings("safety"));
  };

  const shown = status ? nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId)) : undefined;
  const tunLabel = useApp(selectTunLabel);
  const view = status ? derivePopoverView(status, reason, helper, restore, nodes, { tunLabel }) : null;
  /** Direct fallback active: no node carries traffic (SPEC 1.4.0). */
  const directFallback = !!status?.failover?.direct;
  const live = !!status?.connectedSince && (status.state === "direct" || isProxying(status));

  // Last row: an action error wins; otherwise the latest event, muted when stale.
  const eventAt = lastEvent ? Date.parse(lastEvent.at) : 0;
  const stale = !!lastEvent && (now - eventAt > STALE_MS || (!!status?.connectedSince && eventAt < Date.parse(status.connectedSince)));
  const lastText = actionNote ? actionNote.text : (lastEvent?.text ?? "No events yet");
  const lastClass = actionNote
    ? actionNote.tone === "error"
      ? "text-vermilion-text"
      : "text-ink"
    : !lastEvent || stale
      ? "text-ink-muted"
      : lastEvent.tone === "error"
        ? "text-vermilion-text"
        : "text-ink";

  return (
    <motion.div
      key={openSeq}
      ref={wrapRef}
      className="relative overflow-hidden"
      style={{ width: 340, height: 480 }}
      initial={instant ? false : { opacity: 0 }}
      animate={{ opacity: hiding ? 0 : 1 }}
      transition={hiding === "parked" ? { duration: 0 } : hiding ? openTimeline.hide : openTimeline.paper}
    >
      <MotionGate.Provider value={hiding === null}>
      <Frame
        animate={!instant}
        width={340}
        height={480}
        module={24}
        cols={12}
        rows={17}
        grid={false}
        tl={APP_NAME}
        tr="01 / Status"
        bl={(() => {
          const t = directFallback
            ? "Direct · no proxy"
            : shown
              ? `${protocolBadge(shown.protocol)} · ${transport(shown.protocol)} ${shown.port}`
              : "No node";
          return <Swap k={t}>{t}</Swap>;
        })()}
        br={<Rolling value={since(live ? status?.connectedSince : null, now)} className={`tabular ${live ? "text-ink" : "text-ink-muted"}`} />}
      >
        {!status || !view ? (
          <div className="grid h-full place-items-center">
            <MetaLabel muted>Loading…</MetaLabel>
          </div>
        ) : (
          <div className="grid h-full grid-cols-1 px-3" style={{ gridTemplateRows: "repeat(17, 24px)" }}>
            {/* rows 1–4 · state */}
            <div className="row-span-4 flex flex-col justify-end gap-1.5 pb-3">
              <StateWord word={view.word} tone={view.tone} />
              <SubLine text={view.sub} mono={view.subMono} />
            </div>

            {/* rows 5–6 · node */}
            <button
              type="button"
              className="group row-span-2 flex flex-col justify-center text-left focus-visible:outline-2 focus-visible:outline-cobalt"
              aria-label={nodes.length ? "Choose node" : "Add a node in Settings"}
              aria-haspopup="dialog"
              onClick={() => (nodes.length ? setSheet(true) : act("open_nodes"))}
            >
              <MetaLabel muted>
                <Swap k={directFallback ? "direct" : view.onFallback ? "fallback" : "node"}>{directFallback ? "Node · not in use" : view.onFallback ? "Node · on fallback" : "Node"}</Swap>
              </MetaLabel>
              <span className="mt-1 flex items-center gap-2">
                {/* Motion moment 3: on a node change the row slides to the new node. */}
                <Swap k={shown?.id ?? "none"} variant="slideX" className="min-w-0 flex-1">
                  <span className="flex h-5 items-center gap-2">
                    <span className="min-w-0 flex-1 truncate text-body font-medium">
                      {shown ? shown.name : nodes.length ? "Choose a node" : "No nodes — add one in Settings"}
                    </span>
                    {shown?.secretMissing ? (
                      <span className="label text-vermilion-text">No creds</span>
                    ) : (
                      shown?.lastTest?.ok && <span className="label tabular text-ink-muted">{shown.lastTest.latencyMs} ms</span>
                    )}
                    {shown && <span className="label border border-ink px-1 leading-[14px]">{protocolBadge(shown.protocol)}</span>}
                  </span>
                </Swap>
                <Chevron />
              </span>
            </button>

            {/* rows 7–8 · primary action */}
            <div className="row-span-2 flex items-center">
              <button
                type="button"
                onClick={() => act(view.primary.action)}
                disabled={view.primary.disabled}
                className={`h-10 w-full border border-ink font-display text-[13px] font-[750] tracking-[0.12em] uppercase transition-[transform,background-color,color,border-color] duration-base ease-enter active:translate-y-px active:duration-fast focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt ${
                  view.primary.variant === "solid"
                    ? "bg-ink text-paper disabled:border-ink/30 disabled:bg-ink/30"
                    : "bg-transparent text-ink hover:bg-ink/5 disabled:border-ink/30 disabled:text-ink-muted"
                }`}
              >
                <motion.span key={view.primary.label} className="inline-block" initial={{ opacity: 0, y: 3 }} animate={{ opacity: 1, y: 0 }} transition={ST.label}>
                  {view.primary.label}
                </motion.span>
              </button>
            </div>

            {/* rows 9–10 · modes */}
            <Row label="Routing">
              <Segmented label="Routing mode" options={ROUTING} value={status.routingMode} onChange={(m) => void run(commands.setRoutingMode(m))} />
            </Row>
            <Row label="Capture">
              <Segmented
                label="Capture mode"
                options={CAPTURE.map((o) => (o.value === "tun" && tunLabel ? { ...o, title: `TUN (${tunLabel.toLowerCase()})` } : o))}
                value={status.captureMode}
                onChange={(m) => void run(commands.setCaptureMode(m))}
              />
            </Row>

            {view.notice ? (
              /* rows 11–15 · notice */
              <NoticeBlock notice={view.notice} onAction={act} />
            ) : (
              <>
                {/* rows 11–12 · coverage note */}
                <div className="row-span-2 flex flex-col justify-center gap-0.5">
                  {/* Windows: TUN is experimental (owner decision; one flag in lib/experimental.ts). */}
                  {tunLabel && <span className="label truncate text-ink">{tunLabel}</span>}
                  <p className="text-[11px] leading-[15px] text-ink-muted">
                    {status.captureMode === "system_proxy"
                      ? "Only apps that use the system proxy are covered. Use TUN for whole-device protection."
                      : "TUN covers the whole device. The kill switch blocks traffic if the proxy stops."}
                  </p>
                </div>
                {/* rows 13–15 · speeds (row 15 left open) */}
                <div className="row-span-3 grid grid-cols-3 content-start pt-1">
                  <Stat label="Up" mode="fade" value={stats ? rate(stats.upBytesPerSec) : "—"} />
                  <Stat label="Down" mode="fade" value={stats ? rate(stats.downBytesPerSec) : "—"} />
                  <Stat label="Latency" value={stats?.latencyMs != null ? `${stats.latencyMs} ms` : "—"} />
                </div>
              </>
            )}

            {/* row 16 · last event */}
            <button
              type="button"
              className="flex items-center gap-2 text-left hover:[&_span]:underline focus-visible:outline-2 focus-visible:outline-cobalt"
              title={`${lastText} — open details`}
              aria-label="Last event — open details"
              // Click for details (prompt §5): failover reasons → Failover, everything else → Logs.
              onClick={() =>
                void run(commands.openSettings(!actionNote && lastEvent && /fallback|primary|probe|failover|switch|direct|all nodes/i.test(lastEvent.text) ? "failover" : "logs"))
              }
            >
              <MetaLabel muted>Last</MetaLabel>
              <Swap k={lastText} className="min-w-0 flex-1">
                <span className={`block truncate text-secondary ${lastClass}`}>{lastText}</span>
              </Swap>
            </button>

            {/* row 17 · footer */}
            <nav className="flex items-center justify-between border-t border-line">
              <FooterLink onClick={() => void run(commands.openSettings(null))}>Settings</FooterLink>
              <FooterLink onClick={() => act("restore")}>Restore networking</FooterLink>
              <FooterLink onClick={() => void run(commands.quitApp())}>Quit</FooterLink>
            </nav>
          </div>
        )}
      </Frame>

      <AnimatePresence>
        {sheet && status && (
          <NodeSheet key="sheet" nodes={nodes} selectedId={status.selectedNodeId} activeId={status.activeNodeId} onClose={() => setSheet(false)} />
        )}
      </AnimatePresence>
      </MotionGate.Provider>
    </motion.div>
  );
}

/** Motion moment 2: the state word behaves like a one-line counter (clipped). */
function StateWord({ word, tone }: { word: string; tone: Tone }) {
  const animate = useMotionOk();
  return (
    <h1 className="relative h-8 overflow-hidden" aria-live="polite">
      {animate ? (
        <AnimatePresence initial={false}>
          <motion.span
            key={word + tone}
            className={`state-word absolute inset-x-0 top-0 ${TONE_CLASS[tone]}`}
            initial={{ y: ST.wordShift, opacity: 0 }}
            animate={{ y: 0, opacity: 1, transition: ST.wordIn }}
            exit={{ y: -ST.wordShift, opacity: 0, transition: ST.wordOut }}
          >
            {word}
          </motion.span>
        </AnimatePresence>
      ) : (
        <span className={`state-word absolute inset-x-0 top-0 ${TONE_CLASS[tone]}`}>{word}</span>
      )}
    </h1>
  );
}

function SubLine({ text, mono }: { text: string; mono: boolean }) {
  const animate = useMotionOk();
  const cls = `absolute inset-x-0 top-0 truncate ${mono ? "label text-ink" : "text-secondary text-ink-muted"}`;
  return (
    <div className="relative h-4">
      {animate ? (
        <AnimatePresence initial={false}>
          <motion.p
            key={text}
            className={cls}
            title={text}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: ST.sub }}
            exit={{ opacity: 0, transition: ST.sub }}
          >
            {text}
          </motion.p>
        </AnimatePresence>
      ) : (
        <p className={cls} title={text}>
          {text}
        </p>
      )}
    </div>
  );
}

function NoticeBlock({ notice, onAction }: { notice: Notice; onAction: (a: Action) => void }) {
  const bar = notice.tone === "error" ? "bg-vermilion" : notice.tone === "warn" ? "bg-mustard" : "bg-ink";
  return (
    <div className="row-span-5 flex gap-2.5 overflow-hidden py-2" role={notice.tone === "error" ? "alert" : "status"}>
      <span aria-hidden className={`w-[3px] shrink-0 ${bar}`} />
      <div className="flex min-w-0 flex-col gap-1">
        {notice.badge && <span className="label text-ink">{notice.badge}</span>}
        <p className="text-body font-medium">{notice.title}</p>
        {notice.lines.map((l) => (
          <p key={l} className="text-[11px] leading-[15px] text-ink">
            {l}
          </p>
        ))}
        {notice.links.length > 0 && (
          <div className="mt-auto flex flex-wrap gap-x-4 gap-y-1">
            {notice.links.map((l) => (
              <button
                key={l.label}
                type="button"
                onClick={() => onAction(l.action)}
                className="label text-ink underline underline-offset-4 hover:no-underline focus-visible:outline-2 focus-visible:outline-cobalt"
              >
                {l.label}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[72px_1fr] items-center">
      <MetaLabel muted>{label}</MetaLabel>
      {children}
    </div>
  );
}

function Stat({ label, value, mode = "roll" }: { label: string; value: string; mode?: "roll" | "fade" }) {
  const empty = value === "—";
  return (
    <div className="flex flex-col">
      <MetaLabel muted>{label}</MetaLabel>
      <Rolling value={value} mode={mode} className={`font-display text-[18px] leading-6 font-[650] tabular ${empty ? "text-ink-muted" : "text-ink"}`} />
    </div>
  );
}

function FooterLink({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="label text-ink-muted underline-offset-4 transition-colors duration-fast hover:text-ink hover:underline focus-visible:outline-2 focus-visible:outline-cobalt"
    >
      {children}
    </button>
  );
}

function Chevron() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden className="text-ink-muted group-hover:text-ink">
      <path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}
