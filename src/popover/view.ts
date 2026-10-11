// What the popover shows, derived from backend state in one place so the
// priority rules stay consistent (UI.md D7, D8, D12):
//   1. BLOCKED       kill switch engaged — traffic is blocked
//   2. NOT RESTORED  last restore incomplete and not proxying — never looks like OFF
//   3. connection state, with HELPER MISSING replacing OFF in TUN without helper
import type { ConnectionState, HelperStatus, Node, Protocol, RestoreReport, Status } from "../api/bindings";

export type Tone = "ink" | "mustard" | "cobalt" | "vermilion" | "ring";
export type PrimaryAction = "connect" | "disconnect" | "restore" | "open_helper" | "none";
export type Banner = "blocked" | "not_restored" | "helper_missing" | "error" | null;

export interface PopoverView {
  /** Stable key for animating state changes. */
  key: string;
  word: string;
  /** One line under the state word (progress text, consequence, or reason). */
  sub: string;
  tone: Tone;
  /** Line drawn down from the tick (connecting / connected) or retracted. */
  dropped: boolean;
  /** Which labelled tick the dot hangs from. */
  tick: Protocol;
  banner: Banner;
  primary: { label: string; action: PrimaryAction; disabled: boolean };
}

const PROXYING: ConnectionState[] = ["connected", "degraded", "failing_over", "reconnecting"];

export function derivePopoverView(
  status: Status,
  reason: string | null,
  helper: HelperStatus | null,
  restore: RestoreReport | null,
  nodes: Node[],
): PopoverView {
  const shown = nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId));
  const tick: Protocol = shown?.protocol ?? "hysteria2";
  const proxying = PROXYING.includes(status.state);
  const notRestored = !!restore && !restore.fullyRestored && !proxying && status.state !== "connecting";
  const helperMissing = status.captureMode === "tun" && helper !== null && !helper.installed;
  const base = { tick, dropped: false, banner: null as Banner };

  if (status.killSwitchActive) {
    return {
      ...base,
      key: "blocked",
      word: "BLOCKED",
      sub: "Kill switch on — traffic is blocked to prevent leaks",
      tone: "vermilion",
      dropped: true,
      banner: "blocked",
      primary: { label: "DISCONNECT", action: "disconnect", disabled: false },
    };
  }
  if (notRestored) {
    return {
      ...base,
      key: "not_restored",
      word: "NOT RESTORED",
      sub: "Network settings are not back to normal",
      tone: "vermilion",
      banner: "not_restored",
      primary: { label: "RESTORE NETWORKING", action: "restore", disabled: false },
    };
  }

  const busy = (word: string, label: string, sub: string, tone: Tone = "mustard"): PopoverView => ({
    ...base,
    key: status.state,
    word,
    sub: reason ?? sub,
    tone,
    dropped: true,
    primary: { label, action: "disconnect", disabled: false },
  });

  switch (status.state) {
    case "disconnected":
      if (helperMissing)
        return {
          ...base,
          key: "helper_missing",
          word: "HELPER MISSING",
          sub: "TUN mode needs a one-time helper install",
          tone: "ink",
          banner: "helper_missing",
          primary: { label: "SET UP TUN", action: "open_helper", disabled: false },
        };
      return {
        ...base,
        key: "off",
        word: "OFF",
        sub: "Not protected · traffic goes out normally",
        tone: "ink",
        primary: { label: "CONNECT", action: "connect", disabled: nodes.length === 0 },
      };
    case "connecting":
      return busy("CONNECTING", "CANCEL", "Starting");
    case "connected":
      return busy("CONNECTED", "DISCONNECT", status.failover ? status.failover.reason : "Health checks passing", "cobalt");
    case "degraded":
      return busy("DEGRADED", "DISCONNECT", "Health checks failing");
    case "failing_over":
      return busy("FAILING OVER", "DISCONNECT", "Switching node");
    case "reconnecting":
      return busy("RECONNECTING", "DISCONNECT", "Restarting the proxy core");
    case "direct":
      return {
        ...base,
        key: "direct",
        word: "DIRECT",
        sub: "NOT PROXIED — NO PROTECTION",
        tone: "ring",
        primary: { label: "TURN OFF", action: "disconnect", disabled: false },
      };
    case "disconnecting":
      return { ...busy("DISCONNECTING", "DISCONNECTING…", "Restoring network settings", "ink"), dropped: false, primary: { label: "DISCONNECTING…", action: "none", disabled: true } };
    case "recovering":
      return { ...busy("RECOVERING", "RECOVERING…", "Checking the previous session"), primary: { label: "RECOVERING…", action: "none", disabled: true } };
    case "error":
      return {
        ...base,
        key: "error",
        word: "ERROR",
        sub: status.lastError?.message ?? "Could not connect",
        tone: "vermilion",
        banner: "error",
        primary: { label: "TRY AGAIN", action: "connect", disabled: false },
      };
  }
}
