// What the popover shows, derived from backend state in one place so the
// priority rules stay consistent (UI.md D7, D8, D20, D21):
//   1. BLOCKED       kill switch engaged — traffic is blocked
//   2. NOT RESTORED  last restore incomplete and not proxying — never looks like OFF
//   3. connection state; when idle, "no nodes", "credentials missing" and
//      "helper missing" take precedence over plain OFF
// The state is carried by the colour of the state word (owner, D21).
import type { ConnectionState, HelperStatus, Node, RestoreReport, Status } from "../api/bindings";
import { APP_NAME } from "../lib/brand";

/** Colour of the state word. `hollow` = outlined letters (DIRECT). */
export type Tone = "ink" | "amber" | "cobalt" | "vermilion" | "hollow";
export type Action =
  | "connect"
  | "disconnect"
  | "restore"
  | "open_helper"
  | "open_nodes"
  | "open_logs"
  | "open_safety"
  | "import_clipboard"
  | "none";

export interface Notice {
  /** error = vermilion bar, warn = mustard bar, info = ink bar. Text is always ink on paper (D14). */
  tone: "error" | "warn" | "info";
  /** Small mono label above the title (e.g. "Experimental — untested on Windows"). */
  badge?: string | null;
  title: string;
  lines: string[];
  links: { label: string; action: Action }[];
}

export interface PopoverView {
  /** Stable key for animating state changes. */
  key: string;
  word: string;
  tone: Tone;
  /** One line under the state word (progress text, consequence, or reason). */
  sub: string;
  /** Render `sub` as a mono label (DIRECT: "NOT PROXIED — NO PROTECTION"). */
  subMono: boolean;
  /** Replaces the coverage note + stats area when present. */
  notice: Notice | null;
  /** Running on the fallback node after automatic failover. */
  onFallback: boolean;
  primary: { label: string; action: Action; disabled: boolean; variant: "solid" | "outline" };
}

const PROXYING: ConnectionState[] = ["connected", "degraded", "failing_over", "reconnecting"];

export function isProxying(s: Status) {
  return PROXYING.includes(s.state);
}

export const TONE_CLASS: Record<Tone, string> = {
  ink: "text-ink",
  amber: "text-mustard-text",
  cobalt: "text-cobalt",
  vermilion: "text-vermilion",
  hollow: "text-transparent [-webkit-text-stroke:1.25px_var(--color-ink)]",
};

export function derivePopoverView(
  status: Status,
  reason: string | null,
  helper: HelperStatus | null,
  restore: RestoreReport | null,
  nodes: Node[],
  /** Experimental label for TUN / helper on this platform (Windows), or null. */
  opts: { tunLabel?: string | null } = {},
): PopoverView {
  const shown = nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId));
  const proxying = isProxying(status);
  const restoreIncomplete = !!restore && !restore.fullyRestored && !proxying && status.state !== "connecting";
  const helperMissing = status.captureMode === "tun" && helper !== null && !helper.installed;
  const base = { notice: null as Notice | null, subMono: false, onFallback: !!status.failover && !status.failover.direct };
  const solid = (label: string, action: Action, disabled = false) => ({ label, action, disabled, variant: "solid" as const });
  const outline = (label: string, action: Action, disabled = false) => ({ label, action, disabled, variant: "outline" as const });

  const failed = (r: RestoreReport): Notice => {
    const bad = r.items.filter((i) => i.result === "failed" || i.result === "skipped");
    const lines = bad.slice(0, 2).map((i) => `${i.label} — ${i.result === "failed" ? "not restored" : "not attempted"}`);
    if (bad.length > 2) lines.push(`+ ${bad.length - 2} more`);
    return {
      tone: "error",
      title: `${bad.length} setting${bad.length === 1 ? "" : "s"} still changed`,
      lines,
      links: [{ label: "Details", action: "open_logs" }],
    };
  };

  if (status.killSwitchActive) {
    if (restoreIncomplete && restore)
      return {
        ...base,
        key: "blocked_restore",
        word: "BLOCKED",
        tone: "vermilion",
        sub: "Restore incomplete — kill switch still on",
        notice: failed(restore),
        primary: solid("RESTORE NETWORKING", "restore"),
      };
    return {
      ...base,
      key: "blocked",
      word: "BLOCKED",
      tone: "vermilion",
      sub: reason ?? "Kill switch on — traffic is blocked",
      notice: {
        tone: "error",
        title: "Traffic is blocked on purpose",
        lines: ["The kill switch stops leaks while the proxy is down. Reconnect, switch node, or disconnect."],
        links: [{ label: "Restore networking", action: "restore" }],
      },
      primary: solid("DISCONNECT", "disconnect"),
    };
  }

  if (restoreIncomplete && restore)
    return {
      ...base,
      key: "not_restored",
      word: "NOT RESTORED",
      tone: "vermilion",
      sub: "Network settings are not back to normal",
      notice: failed(restore),
      primary: solid("RESTORE NETWORKING", "restore"),
    };

  const busy = (word: string, sub: string, tone: Tone = "amber") => ({ ...base, key: status.state, word, tone, sub: reason ?? sub });
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

  // All nodes failing (SPEC 1.4.0, §5). Direct fallback on → traffic is going out
  // unprotected; off → traffic stays blocked. Both rank right after BLOCKED / NOT RESTORED.
  if (proxying && (status.failover?.direct || status.lastError?.code === "DIRECT_FALLBACK_ACTIVE"))
    return {
      ...base,
      key: "unprotected",
      word: "UNPROTECTED",
      tone: "vermilion",
      sub: "All nodes failing — traffic is going out directly",
      notice: {
        tone: "error",
        title: "Traffic is not protected",
        lines: [
          `Every node is failing and direct fallback is on, so traffic goes out directly — visible to your network. ${APP_NAME} switches back to the proxy as soon as a node recovers.`,
        ],
        links: [{ label: "Direct fallback settings", action: "open_safety" }],
      },
      primary: solid("DISCONNECT", "disconnect"),
    };
  if (proxying && status.lastError?.code === "ALL_NODES_FAILED")
    return {
      ...base,
      key: "all_down",
      word: "ALL NODES DOWN",
      tone: "vermilion",
      sub: "Traffic is blocked to prevent leaks",
      notice: {
        tone: "error",
        title: "No node is reachable",
        lines: [
          `${APP_NAME} keeps checking and reconnects as soon as a node answers.`,
          ...(status.captureMode === "system_proxy" ? ["In system proxy mode, apps that ignore the proxy still connect directly."] : []),
        ],
        links: [{ label: "Test nodes", action: "open_nodes" }],
      },
      primary: outline("DISCONNECT", "disconnect"),
    };

  switch (status.state) {
    case "disconnected":
      if (nodes.length === 0)
        return {
          ...base,
          key: "empty",
          word: "OFF",
          tone: "ink",
          sub: "No nodes yet",
          notice: {
            tone: "info",
            title: "Add your first node",
            lines: ["Paste a hy2:// or vless:// link, import a QR image, or enter the details by hand in Settings."],
            links: [
              { label: "Import from clipboard", action: "import_clipboard" },
              { label: "Open settings", action: "open_nodes" },
            ],
          },
          primary: solid("ADD NODE", "open_nodes"),
        };
      if (shown?.secretMissing)
        return {
          ...base,
          key: "secret_missing",
          word: "OFF",
          tone: "ink",
          sub: "Credentials missing for this node",
          notice: {
            tone: "warn",
            title: "Credentials missing — edit to re-enter",
            lines: [`The saved password or key for “${shown.name}” is no longer in the system keychain.`],
            links: [{ label: "Edit node", action: "open_nodes" }],
          },
          primary: solid("CONNECT", "connect", true),
        };
      if (helperMissing)
        return {
          ...base,
          key: "helper_missing",
          word: "HELPER MISSING",
          tone: "ink",
          sub: "TUN mode needs a one-time helper install",
          notice: {
            tone: "info",
            badge: opts.tunLabel,
            title: "Install the helper to use TUN",
            lines: [
              "It runs the proxy for the whole device and applies the kill switch. Your system will ask for administrator approval once.",
              "Or switch Capture to System proxy.",
            ],
            links: [],
          },
          primary: solid("SET UP TUN", "open_helper"),
        };
      return { ...base, key: "off", word: "OFF", tone: "ink", sub: "Not protected · traffic goes out normally", primary: solid("CONNECT", "connect") };
    case "connecting":
      return { ...busy("CONNECTING", "Starting"), primary: outline("CANCEL", "disconnect") };
    case "connected":
      return {
        ...busy("CONNECTED", "Health checks passing", "cobalt"),
        sub: status.failover ? `On fallback · ${status.failover.reason}` : reason && reason !== "Connected" ? reason : "Health checks passing",
        primary: outline("DISCONNECT", "disconnect"),
      };
    case "degraded":
      return { ...busy("DEGRADED", "Health checks failing"), notice: proxyGap, primary: outline("DISCONNECT", "disconnect") };
    case "failing_over":
      return { ...busy("FAILING OVER", "Switching node"), primary: outline("DISCONNECT", "disconnect") };
    case "reconnecting":
      return { ...busy("RECONNECTING", "Restarting the proxy core"), notice: proxyGap, primary: outline("DISCONNECT", "disconnect") };
    case "direct":
      return { ...base, key: "direct", word: "DIRECT", tone: "hollow", sub: "NOT PROXIED — NO PROTECTION", subMono: true, primary: outline("TURN OFF", "disconnect") };
    case "disconnecting":
      return { ...busy("DISCONNECTING", "Restoring network settings", "ink"), primary: outline("DISCONNECTING…", "none", true) };
    case "recovering":
      return {
        ...busy("RECOVERING", "Checking the previous session"),
        notice: {
          tone: "info",
          title: "Checking the previous session",
          lines: [`${APP_NAME} did not shut down cleanly. It is re-attaching or putting your network settings back.`],
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
        tone: "vermilion",
        sub: reason ?? "Could not connect",
        notice: {
          tone: "error",
          title: e?.message ?? "Something went wrong",
          lines: [status.systemProxySet || status.tunActive ? "Some network changes are still in place." : "Your normal connection is in use."],
          links: [{ label: "Details", action: "open_logs" }],
        },
        primary: solid("TRY AGAIN", "connect", !!shown?.secretMissing),
      };
    }
  }
}
