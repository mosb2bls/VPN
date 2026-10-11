// What the popover shows, derived from backend state in one place so the
// priority rules stay consistent (UI.md D7, D8, D12):
//   1. BLOCKED       kill switch engaged — traffic is blocked
//   2. NOT RESTORED  last restore incomplete and not proxying — never looks like OFF
//   3. connection state, with HELPER MISSING replacing OFF in TUN without helper
import type { ConnectionState, HelperStatus, Node, Protocol, RestoreReport, Status } from "../api/bindings";

export type Tone = "ink" | "mustard" | "cobalt" | "vermilion" | "ring";
export type PrimaryAction = "connect" | "disconnect" | "restore" | "open_helper" | "none";
export type LinkAction = "restore" | "open_logs" | "open_helper";

export interface Notice {
  /** error = vermilion bar, warn = mustard bar, info = ink bar. Text is always ink on paper (D14). */
  tone: "error" | "warn" | "info";
  title: string;
  lines: string[];
  links: { label: string; action: LinkAction }[];
}

export interface PopoverView {
  /** Stable key for animating state changes. */
  key: string;
  word: string;
  /** One line under the state word (progress text, consequence, or reason). */
  sub: string;
  /** Render `sub` as a mono label (DIRECT: "NOT PROXIED — NO PROTECTION"). */
  subMono: boolean;
  tone: Tone;
  /** Line drawn down from the tick (busy / connected) or retracted. */
  dropped: boolean;
  /** Which labelled tick the dot hangs from. */
  tick: Protocol;
  /** Replaces the coverage note + stats area when present. */
  notice: Notice | null;
  /** Running on the fallback node after automatic failover. */
  onFallback: boolean;
  primary: { label: string; action: PrimaryAction; disabled: boolean; variant: "solid" | "outline" };
}

const PROXYING: ConnectionState[] = ["connected", "degraded", "failing_over", "reconnecting"];

export function isProxying(s: Status) {
  return PROXYING.includes(s.state);
}

export function derivePopoverView(
  status: Status,
  reason: string | null,
  helper: HelperStatus | null,
  restore: RestoreReport | null,
  nodes: Node[],
): PopoverView {
  const shown = nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId));
  const tick: Protocol = shown?.protocol ?? "hysteria2";
  const proxying = isProxying(status);
  const notRestored = !!restore && !restore.fullyRestored && !proxying && status.state !== "connecting";
  const helperMissing = status.captureMode === "tun" && helper !== null && !helper.installed;
  const base = { tick, dropped: false, notice: null as Notice | null, subMono: false, onFallback: !!status.failover };
  const solid = (label: string, action: PrimaryAction, disabled = false) => ({ label, action, disabled, variant: "solid" as const });
  const outline = (label: string, action: PrimaryAction, disabled = false) => ({ label, action, disabled, variant: "outline" as const });

  const failedItems = (r: RestoreReport) => {
    const bad = r.items.filter((i) => i.result === "failed" || i.result === "skipped");
    const lines = bad.slice(0, 2).map((i) => `${i.label} — ${i.result === "failed" ? "not restored" : "not attempted"}`);
    if (bad.length > 2) lines.push(`+ ${bad.length - 2} more`);
    return { count: bad.length, lines };
  };
  const restoreIncomplete = !!restore && !restore.fullyRestored && !proxying;

  if (status.killSwitchActive) {
    // Kill switch held after an incomplete restore: the fix is restoring, not disconnecting.
    if (restoreIncomplete && restore) {
      const f = failedItems(restore);
      return {
        ...base,
        key: "blocked_restore",
        word: "BLOCKED",
        sub: "Restore incomplete — kill switch still on",
        tone: "vermilion",
        dropped: true,
        notice: { tone: "error", title: `${f.count} setting${f.count === 1 ? "" : "s"} still changed`, lines: f.lines, links: [{ label: "Details", action: "open_logs" }] },
        primary: solid("RESTORE NETWORKING", "restore"),
      };
    }
    return {
      ...base,
      key: "blocked",
      word: "BLOCKED",
      sub: reason ?? "Kill switch on — traffic is blocked",
      tone: "vermilion",
      dropped: true,
      notice: {
        tone: "error",
        title: "Traffic is blocked on purpose",
        lines: ["The kill switch stops leaks while the proxy is down. Reconnect, switch node, or disconnect."],
        links: [{ label: "Restore networking", action: "restore" }],
      },
      primary: solid("DISCONNECT", "disconnect"),
    };
  }

  if (notRestored && restore) {
    const f = failedItems(restore);
    return {
      ...base,
      key: "not_restored",
      word: "NOT RESTORED",
      sub: "Network settings are not back to normal",
      tone: "vermilion",
      notice: { tone: "error", title: `${f.count} setting${f.count === 1 ? "" : "s"} still changed`, lines: f.lines, links: [{ label: "Details", action: "open_logs" }] },
      primary: solid("RESTORE NETWORKING", "restore"),
    };
  }

  const busy = (word: string, sub: string, tone: Tone = "mustard") => ({
    ...base,
    key: status.state,
    word,
    sub: reason ?? sub,
    tone,
    dropped: true,
  });
  // System proxy has no kill switch: say so whenever the proxy is not working (SPEC §9.2).
  const proxyGap: Notice | null =
    status.captureMode === "system_proxy"
      ? {
          tone: "warn",
          title: "Apps may connect directly",
          lines: ["System proxy mode has no kill switch. Until the proxy works again, apps can bypass it."],
          links: [],
        }
      : null;

  switch (status.state) {
    case "disconnected":
      if (helperMissing)
        return {
          ...base,
          key: "helper_missing",
          word: "HELPER MISSING",
          sub: "TUN mode needs a one-time helper install",
          tone: "ink",
          notice: {
            tone: "info",
            title: "Install the helper to use TUN",
            lines: [
              "It runs the proxy for the whole device and applies the kill switch. Your system will ask for an administrator password once.",
              "Or switch Capture to System proxy.",
            ],
            links: [],
          },
          primary: solid("SET UP TUN", "open_helper"),
        };
      return {
        ...base,
        key: "off",
        word: "OFF",
        sub: "Not protected · traffic goes out normally",
        tone: "ink",
        primary: solid("CONNECT", "connect", nodes.length === 0),
      };
    case "connecting":
      return { ...busy("CONNECTING", "Starting"), primary: outline("CANCEL", "disconnect") };
    case "connected":
      return {
        ...busy("CONNECTED", status.failover ? `On fallback · ${status.failover.reason}` : "Health checks passing", "cobalt"),
        sub: status.failover ? `On fallback · ${status.failover.reason}` : (reason && reason !== "Connected" ? reason : "Health checks passing"),
        primary: outline("DISCONNECT", "disconnect"),
      };
    case "degraded":
      return { ...busy("DEGRADED", "Health checks failing"), notice: proxyGap, primary: outline("DISCONNECT", "disconnect") };
    case "failing_over":
      return { ...busy("FAILING OVER", "Switching node"), primary: outline("DISCONNECT", "disconnect") };
    case "reconnecting":
      return { ...busy("RECONNECTING", "Restarting the proxy core"), notice: proxyGap, primary: outline("DISCONNECT", "disconnect") };
    case "direct":
      return {
        ...base,
        key: "direct",
        word: "DIRECT",
        sub: "NOT PROXIED — NO PROTECTION",
        subMono: true,
        tone: "ring",
        primary: outline("TURN OFF", "disconnect"),
      };
    case "disconnecting":
      return { ...busy("DISCONNECTING", "Restoring network settings", "ink"), dropped: false, primary: outline("DISCONNECTING…", "none", true) };
    case "recovering":
      return {
        ...busy("RECOVERING", "Checking the previous session"),
        notice: {
          tone: "info",
          title: "Checking the previous session",
          lines: ["EdgeClient did not shut down cleanly. It is re-attaching or putting your network settings back."],
          links: [],
        },
        primary: outline("RECOVERING…", "none", true),
      };
    case "error": {
      const e = status.lastError;
      return {
        ...base,
        key: "error",
        word: "ERROR",
        sub: reason ?? "Could not connect",
        tone: "vermilion",
        notice: {
          tone: "error",
          title: e?.message ?? "Something went wrong",
          lines: [status.systemProxySet || status.tunActive ? "Some network changes are still in place." : "Your normal connection is in use."],
          links: [{ label: "Details", action: "open_logs" }],
        },
        primary: solid("TRY AGAIN", "connect"),
      };
    }
  }
}
