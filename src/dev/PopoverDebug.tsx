// U1.1 debug view of the popover: every value the real popover will show, with
// working controls, but no visual design yet (that is U2).
import { useEffect, useState } from "react";
import { commands } from "../api/client";
import type { CaptureMode, RoutingMode } from "../api/bindings";
import { protocolBadge, rate, since, transport } from "../lib/format";
import { selectLastEvent, selectNotRestored, useApp } from "../state/store";
import { useRun } from "./useRun";

function useNow(ms: number) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

const LABEL: Record<string, string> = {
  disconnected: "OFF",
  connecting: "CONNECTING",
  connected: "CONNECTED",
  degraded: "DEGRADED",
  failing_over: "FAILING OVER",
  reconnecting: "RECONNECTING",
  direct: "DIRECT — NOT PROXIED",
  disconnecting: "DISCONNECTING",
  recovering: "RECOVERING",
  error: "ERROR",
};

export function PopoverDebug() {
  const status = useApp((s) => s.status);
  const reason = useApp((s) => s.statusReason);
  const nodes = useApp((s) => s.nodes);
  const stats = useApp((s) => s.stats);
  const helper = useApp((s) => s.helper);
  const lastEvent = useApp(selectLastEvent);
  const notRestored = useApp(selectNotRestored);
  const restore = useApp((s) => s.restore);
  const now = useNow(1000);
  const { run, last, busy } = useRun();

  if (!status) return <div className="dbg-pad">Loading…</div>;

  const shown = nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId));
  const helperMissing = status.captureMode === "tun" && helper !== null && !helper.installed;
  const off = status.state === "disconnected" || status.state === "error";
  const stateWord = status.killSwitchActive
    ? "BLOCKED"
    : notRestored && off
      ? "NOT RESTORED"
      : helperMissing && off
        ? "HELPER MISSING"
        : LABEL[status.state];

  return (
    <div className="dbg dbg-pop">
      <div className="dbg-meta">
        <span>EDGECLIENT</span>
        <span>U1.1 DEBUG</span>
      </div>

      <div className="dbg-state" data-state={status.state} data-blocked={status.killSwitchActive || undefined}>
        {stateWord}
      </div>
      <div className="dbg-sub">{reason ?? " "}</div>
      <div className="dbg-row">
        <span>{shown ? shown.name : "No node"}</span>
        {shown && <span className="dbg-badge">{protocolBadge(shown.protocol)}</span>}
        <span className="dbg-spacer" />
        <span className="dbg-mono">{since(status.connectedSince, now)}</span>
      </div>

      {status.killSwitchActive && (
        <div className="dbg-alert">
          Kill switch active: traffic is blocked. {status.lastError?.message}
          <div className="dbg-btns">
            <button onClick={() => run("disconnect", commands.disconnect())}>Disconnect</button>
            <button onClick={() => run("restoreNetworking", commands.restoreNetworking())}>Restore networking</button>
          </div>
        </div>
      )}
      {notRestored && restore && (
        <div className="dbg-alert">
          Networking is not back to normal:
          {restore.items
            .filter((i) => i.result === "failed" || i.result === "skipped")
            .map((i) => (
              <div key={i.kind}>
                {i.label} — {i.result.toUpperCase()}: {i.errorMessage}
              </div>
            ))}
          <div className="dbg-btns">
            <button onClick={() => run("restoreNetworking", commands.restoreNetworking(), (r) => (r.fullyRestored ? "fully restored" : "incomplete"))}>
              Retry restore
            </button>
          </div>
        </div>
      )}
      {!status.killSwitchActive && !notRestored && status.lastError && (
        <div className="dbg-warn">
          {status.lastError.code}: {status.lastError.message}
        </div>
      )}
      {helperMissing && <div className="dbg-warn">TUN needs the helper — install it in Settings → Helper.</div>}

      <div className="dbg-btns">
        <button disabled={!!busy} onClick={() => run("connect", commands.connect())}>
          Connect
        </button>
        <button disabled={!!busy} onClick={() => run("disconnect", commands.disconnect())}>
          Disconnect
        </button>
      </div>

      <label className="dbg-field">
        Node
        <select
          value={status.selectedNodeId ?? ""}
          onChange={(e) => run("selectNode", commands.selectNode(e.target.value))}
        >
          {nodes.map((n) => (
            <option key={n.id} value={n.id}>
              {protocolBadge(n.protocol)} · {n.name} {n.lastTest?.latencyMs ? `· ${n.lastTest.latencyMs} ms` : ""}
            </option>
          ))}
        </select>
      </label>

      <div className="dbg-seg">
        {(["rule", "global", "direct"] as RoutingMode[]).map((m) => (
          <button key={m} aria-pressed={status.routingMode === m} onClick={() => run("setRoutingMode", commands.setRoutingMode(m))}>
            {m.toUpperCase()}
          </button>
        ))}
      </div>
      <div className="dbg-seg">
        {(["system_proxy", "tun"] as CaptureMode[]).map((m) => (
          <button key={m} aria-pressed={status.captureMode === m} onClick={() => run("setCaptureMode", commands.setCaptureMode(m))}>
            {m === "tun" ? "TUN" : "SYSTEM PROXY"}
          </button>
        ))}
      </div>
      {status.captureMode === "system_proxy" && (
        <div className="dbg-note">Only apps that use the system proxy are covered. Use TUN for whole-device protection.</div>
      )}

      <div className="dbg-row dbg-mono">
        <span>↑ {rate(stats?.upBytesPerSec)}</span>
        <span>↓ {rate(stats?.downBytesPerSec)}</span>
        <span>{stats?.latencyMs != null ? `${stats.latencyMs} ms` : "— ms"}</span>
      </div>

      <div className="dbg-event" data-tone={lastEvent?.tone}>
        {lastEvent ? lastEvent.text : "No events yet"}
      </div>

      <div className="dbg-result">
        {busy
          ? `… ${busy}`
          : last
            ? last.ok
              ? `✓ ${last.label}${last.summary ? ` — ${last.summary}` : ""}`
              : `✕ ${last.label}: ${last.error?.code} — ${last.error?.message}`
            : " "}
      </div>

      <div className="dbg-meta dbg-foot">
        <span>{shown ? `${protocolBadge(shown.protocol)} · ${transport(shown.protocol)} ${shown.port}` : "—"}</span>
        <span className="dbg-links">
          <button onClick={() => run("openSettings", commands.openSettings("nodes"))}>Settings</button>
          <button onClick={() => run("restoreNetworking", commands.restoreNetworking(), (r) => (r.fullyRestored ? "fully restored" : "incomplete"))}>
            Restore
          </button>
          <button onClick={() => run("quitApp", commands.quitApp())}>Quit</button>
        </span>
      </div>
    </div>
  );
}
