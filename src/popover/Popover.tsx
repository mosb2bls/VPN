// Tray popover, 340 × 480 (SPEC §2.8). Laid out on the 24 px grid: the frame
// is 12 × 17 modules and every block below spans whole rows (UI.md D10).
//
//   rows 1–2   status instrument (dot on the HY2 / REALITY tick)
//   rows 3–5   state word + one-line consequence / progress
//   rows 6–7   node (opens the switcher sheet — U2.3)
//   rows 8–9   primary action
//   row  10    routing mode
//   row  11    capture mode
//   rows 12–13 coverage note
//   rows 14–15 live speeds + latency
//   row  16    last event
//   row  17    footer: settings · restore networking · quit
import { useEffect, useState, type ReactNode } from "react";
import { commands } from "../api/client";
import type { CaptureMode, RoutingMode } from "../api/bindings";
import { protocolBadge, rate, since, transport } from "../lib/format";
import { selectLastEvent, useApp } from "../state/store";
import { Frame, MetaLabel } from "../ui/Frame";
import { Segmented } from "../ui/Segmented";
import { StatusInstrument } from "./StatusInstrument";
import { derivePopoverView, type PopoverView } from "./view";

const ROUTING = [
  { value: "rule", label: "Rule" },
  { value: "global", label: "Global" },
  { value: "direct", label: "Direct" },
] satisfies { value: RoutingMode; label: string }[];
const CAPTURE = [
  { value: "system_proxy", label: "System proxy" },
  { value: "tun", label: "TUN" },
] satisfies { value: CaptureMode; label: string }[];

const WORD_COLOR: Record<PopoverView["tone"], string> = {
  ink: "text-ink",
  ring: "text-ink",
  mustard: "text-ink",
  cobalt: "text-cobalt",
  vermilion: "text-ink",
};

function useNow(ms: number) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

export function Popover() {
  const status = useApp((s) => s.status);
  const reason = useApp((s) => s.statusReason);
  const nodes = useApp((s) => s.nodes);
  const helper = useApp((s) => s.helper);
  const restore = useApp((s) => s.restore);
  const stats = useApp((s) => s.stats);
  const lastEvent = useApp(selectLastEvent);
  const now = useNow(1000);

  // Esc hides the popover (SPEC §2.8).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && void commands.hidePopover();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const shown = status ? nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId)) : undefined;
  const view = status ? derivePopoverView(status, reason, helper, restore, nodes) : null;
  const live = !!status?.connectedSince && (status.state === "direct" || ["connected", "degraded", "failing_over", "reconnecting"].includes(status.state));

  const primary = () => {
    if (!view) return;
    const a = view.primary.action;
    if (a === "connect") void commands.connect();
    else if (a === "disconnect") void commands.disconnect();
    else if (a === "restore") void commands.restoreNetworking();
    else if (a === "open_helper") void commands.openSettings("helper");
  };

  return (
    <Frame
      width={340}
      height={480}
      module={24}
      cols={12}
      rows={17}
      tl="EdgeClient"
      tr="01 / Status"
      bl={shown ? `${protocolBadge(shown.protocol)} · ${transport(shown.protocol)} ${shown.port}` : "No node"}
      br={<span className={`tabular ${live ? "text-ink" : "text-ink-muted"}`}>{since(live ? status?.connectedSince : null, now)}</span>}
    >
      {!status || !view ? (
        <div className="grid h-full place-items-center">
          <MetaLabel muted>Loading…</MetaLabel>
        </div>
      ) : (
        <div className="grid h-full grid-cols-1 px-3" style={{ gridTemplateRows: "repeat(17, 24px)" }}>
          {/* rows 1–2 · instrument */}
          <div className="relative -mx-3 row-span-2">
            <StatusInstrument tick={view.tick} tone={view.tone} dropped={view.dropped} />
          </div>

          {/* rows 3–5 · state */}
          <div className="row-span-3 flex flex-col justify-center gap-1.5">
            <h1 className={`state-word ${WORD_COLOR[view.tone]}`}>{view.word}</h1>
            <p className={`truncate ${view.tone === "ring" ? "label text-ink" : "text-secondary text-ink-muted"}`}>{view.sub}</p>
          </div>

          {/* rows 6–7 · node */}
          <button
            type="button"
            className="group row-span-2 flex flex-col justify-center border-t border-line text-left focus-visible:outline-2 focus-visible:outline-cobalt"
            aria-label="Choose node"
            disabled={nodes.length === 0}
          >
            <MetaLabel muted>Node</MetaLabel>
            <span className="mt-1 flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate text-body font-medium">{shown ? shown.name : "No nodes — add one in Settings"}</span>
              {shown?.lastTest?.ok && <span className="label tabular text-ink-muted">{shown.lastTest.latencyMs} ms</span>}
              {shown && <span className="label border border-ink px-1 leading-[14px]">{protocolBadge(shown.protocol)}</span>}
              <Chevron />
            </span>
          </button>

          {/* rows 8–9 · primary action */}
          <div className="row-span-2 flex items-center">
            <button
              type="button"
              onClick={primary}
              disabled={view.primary.disabled}
              className="h-10 w-full bg-ink font-display text-[13px] font-[750] tracking-[0.12em] text-paper uppercase transition-transform duration-fast active:translate-y-px disabled:bg-ink/30 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt"
            >
              {view.primary.label}
            </button>
          </div>

          {/* row 10 · routing */}
          <Row label="Routing">
            <Segmented label="Routing mode" options={ROUTING} value={status.routingMode} onChange={(m) => void commands.setRoutingMode(m)} />
          </Row>

          {/* row 11 · capture */}
          <Row label="Capture">
            <Segmented label="Capture mode" options={CAPTURE} value={status.captureMode} onChange={(m) => void commands.setCaptureMode(m)} />
          </Row>

          {/* rows 12–13 · coverage note */}
          <p className="row-span-2 flex items-center text-[11px] leading-[15px] text-ink-muted">
            {status.captureMode === "system_proxy"
              ? "Only apps that use the system proxy are covered. Use TUN for whole-device protection."
              : "TUN covers the whole device. The kill switch blocks traffic if the proxy stops."}
          </p>

          {/* rows 14–15 · speeds */}
          <div className="row-span-2 grid grid-cols-3 border-t border-line pt-1.5">
            <Stat label="Up" value={stats ? rate(stats.upBytesPerSec) : "—"} />
            <Stat label="Down" value={stats ? rate(stats.downBytesPerSec) : "—"} />
            <Stat label="Latency" value={stats?.latencyMs != null ? `${stats.latencyMs} ms` : "—"} />
          </div>

          {/* row 16 · last event */}
          <button
            type="button"
            className="flex items-center gap-2 border-t border-line text-left"
            title={lastEvent?.text}
            aria-label="Last event"
          >
            <MetaLabel muted>Last</MetaLabel>
            <span className={`min-w-0 flex-1 truncate text-secondary ${lastEvent?.tone === "error" ? "text-vermilion-text" : lastEvent ? "text-ink" : "text-ink-muted"}`}>
              {lastEvent ? lastEvent.text : "No events yet"}
            </span>
          </button>

          {/* row 17 · footer */}
          <nav className="flex items-center justify-between border-t border-line">
            <FooterLink onClick={() => void commands.openSettings(null)}>Settings</FooterLink>
            <FooterLink onClick={() => void commands.restoreNetworking()}>Restore networking</FooterLink>
            <FooterLink onClick={() => void commands.quitApp()}>Quit</FooterLink>
          </nav>
        </div>
      )}
    </Frame>
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

function Stat({ label, value }: { label: string; value: string }) {
  const empty = value === "—";
  return (
    <div className="flex flex-col">
      <MetaLabel muted>{label}</MetaLabel>
      <span className={`font-display text-[18px] leading-6 font-[650] tabular ${empty ? "text-ink-muted" : "text-ink"}`}>{value}</span>
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
