// Dev state switcher: drives the mock backend into any state, event or error.
// Only rendered when the mock is active.
import { useEffect, useState, useSyncExternalStore } from "react";
import type { ErrorCode, RestoreItemKind } from "../api/bindings";
import type { CommandName } from "../api/types";
import { ALL_ERROR_CODES, SAMPLE_LINKS, SCENARIOS, mockEngine, type DevConfig, type NodeHealth } from "../api/mock";
import { commands } from "../api/client";
import { useApp } from "../state/store";

function useDev(): DevConfig {
  return useSyncExternalStore(
    (cb) => mockEngine.subscribeDev(cb),
    () => mockEngine.dev,
  );
}

const RESTORE_KINDS: RestoreItemKind[] = ["tun_session", "routes", "dns", "ipv6", "system_proxy", "firewall"];
const COMMAND_NAMES = Object.keys(commands) as CommandName[];

function Select<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: readonly T[]; onChange: (v: T) => void }) {
  return (
    <label className="dev-field">
      <span>{label}</span>
      <select value={value} onChange={(e) => onChange(e.target.value as T)}>
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    </label>
  );
}

export function DevPanel() {
  const dev = useDev();
  const nodes = useApp((s) => s.nodes);
  const [errCode, setErrCode] = useState<ErrorCode>("CORE_CRASHED");
  const [errRecoverable, setErrRecoverable] = useState(false);
  const [forceCmd, setForceCmd] = useState<CommandName>("connect");
  const [forceCode, setForceCode] = useState<ErrorCode>("NOT_IMPLEMENTED");

  return (
    <div className="dev-panel">
      <section>
        <h4>Scenario (snap to state)</h4>
        <div className="dev-grid">
          {SCENARIOS.map((s) => (
            <button key={s.id} onClick={() => mockEngine.applyScenario(s.id)}>
              {s.label}
            </button>
          ))}
        </div>
        <p className="dev-hint">Snapping freezes health probes. Flows below re-enable them.</p>
        <h4>Node set</h4>
        <div className="dev-grid">
          {(["default", "one", "many", "none"] as const).map((k) => (
            <button key={k} onClick={() => mockEngine.setNodeSet(k)}>
              {k}
            </button>
          ))}
        </div>
      </section>

      <section>
        <h4>Flows (real transitions)</h4>
        <div className="dev-grid">
          <button onClick={() => mockEngine.simulateUdpBlocked()}>HY2 UDP blocked → failover</button>
          <button onClick={() => mockEngine.restoreAllHealth()}>All nodes healthy</button>
          <button onClick={() => void mockEngine.tryFailover(true)}>Force failover now</button>
          <button onClick={() => void mockEngine.failback()}>Force failback now</button>
          <button onClick={() => void mockEngine.simulateCoreCrash("crash")}>Core crash</button>
          <button onClick={() => void mockEngine.simulateCoreCrash("network")}>Network change</button>
          <button onClick={() => void mockEngine.simulateCoreCrash("wake")}>Wake from sleep</button>
          <button onClick={() => void mockEngine.simulateLaunchRecovery("reattached")}>Launch: re-attached</button>
          <button onClick={() => void mockEngine.simulateLaunchRecovery("restored")}>Launch: restored</button>
          <button onClick={() => void mockEngine.simulateLaunchRecovery("restore_incomplete")}>Launch: incomplete</button>
          <button onClick={() => mockEngine.scheduledRulesUpdate()}>Scheduled rules update</button>
          <button onClick={() => mockEngine.emitPopoverShown()}>popoverShown (REQ-1)</button>
        </div>
        <h4>Helper</h4>
        <div className="dev-grid">
          {(["missing", "running", "stopped", "outdated"] as const).map((k) => (
            <button key={k} onClick={() => mockEngine.setHelperState(k)}>
              {k}
            </button>
          ))}
        </div>
        <h4>Node health</h4>
        {nodes.map((n) => (
          <Select<NodeHealth>
            key={n.id}
            label={n.name}
            value={dev.nodeHealth[n.id] ?? "ok"}
            options={["ok", "flaky", "fail"]}
            onChange={(v) => mockEngine.setNodeHealth(n.id, v)}
          />
        ))}
      </section>

      <section>
        <h4>Outcomes</h4>
        <Select label="connect" value={dev.connectOutcome} options={["ok", "CORE_START_FAILED", "PORT_IN_USE", "DNS_BOOTSTRAP_FAILED", "SYSTEM_PROXY_SET_FAILED", "TUN_START_FAILED", "KILL_SWITCH_FAILED", "NODE_AUTH_FAILED", "CORE_BINARY_MISSING"] as const} onChange={(v) => mockEngine.setDev({ connectOutcome: v })} />
        <Select label="reconnect" value={dev.reconnectOutcome} options={["recover", "give_up"] as const} onChange={(v) => mockEngine.setDev({ reconnectOutcome: v })} />
        <h4>Next restore (per item)</h4>
        {RESTORE_KINDS.map((k) => (
          <Select<"ok" | "failed" | "skipped">
            key={k}
            label={k}
            value={dev.restoreFailures[k] ?? "ok"}
            options={["ok", "failed", "skipped"]}
            onChange={(v) => {
              const { [k]: _, ...rest } = dev.restoreFailures;
              mockEngine.setDev({ restoreFailures: v === "ok" ? rest : { ...rest, [k]: v } });
            }}
          />
        ))}
        <div className="dev-grid">
          <button onClick={() => mockEngine.setDev({ restoreFailures: {} })}>all clean</button>
          <button onClick={() => mockEngine.setDev({ restoreFailures: { dns: "failed" } })}>DNS fails</button>
          <button onClick={() => mockEngine.setDev({ restoreFailures: { tun_session: "skipped", routes: "skipped", firewall: "skipped" } })}>helper unreachable</button>
        </div>
        <p className="dev-hint">Items the app never changed report “not needed” regardless.</p>
        <Select label="helper install" value={dev.helperInstallOutcome} options={["ok", "HELPER_INSTALL_CANCELLED", "HELPER_INSTALL_FAILED"] as const} onChange={(v) => mockEngine.setDev({ helperInstallOutcome: v })} />
        <Select label="rules update" value={dev.rulesOutcome} options={["changed", "unchanged", "RULES_DOWNLOAD_FAILED", "RULES_VALIDATION_FAILED"] as const} onChange={(v) => mockEngine.setDev({ rulesOutcome: v })} />
        <Select label="QR import" value={dev.qrOutcome} options={["ok", "QR_NOT_FOUND", "QR_IMAGE_UNREADABLE", "QR_TOO_MANY_CODES"] as const} onChange={(v) => mockEngine.setDev({ qrOutcome: v })} />
        <Select
          label="clipboard"
          value={dev.clipboard.kind === "text" ? (dev.clipboard.text ? "sample links" : "blank text") : dev.clipboard.kind}
          options={["sample links", "blank text", "empty", "unavailable"] as const}
          onChange={(v) =>
            mockEngine.setDev({
              clipboard:
                v === "sample links" ? { kind: "text", text: SAMPLE_LINKS } : v === "blank text" ? { kind: "text", text: "  \n" } : { kind: v },
            })
          }
        />
      </section>

      <section>
        <h4>Errors</h4>
        <div className="dev-row">
          <select value={forceCmd} onChange={(e) => setForceCmd(e.target.value as CommandName)}>
            {COMMAND_NAMES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
          <select value={forceCode} onChange={(e) => setForceCode(e.target.value as ErrorCode)}>
            {ALL_ERROR_CODES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
          <button onClick={() => mockEngine.setDev({ forced: { ...dev.forced, [forceCmd]: forceCode } })}>Arm</button>
        </div>
        <p className="dev-hint">
          One-shot. Armed:{" "}
          {Object.entries(dev.forced).map(([k, v]) => `${k}→${v}`).join(", ") || "none"}
        </p>
        <div className="dev-row">
          <select value={errCode} onChange={(e) => setErrCode(e.target.value as ErrorCode)}>
            {ALL_ERROR_CODES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
          <label className="dev-check">
            <input type="checkbox" checked={errRecoverable} onChange={(e) => setErrRecoverable(e.target.checked)} />
            recoverable
          </label>
          <button onClick={() => mockEngine.raiseError(errCode, errRecoverable)}>Raise errorRaised</button>
        </div>

        <h4>Mock</h4>
        <label className="dev-check">
          <input type="checkbox" checked={dev.stubMode} onChange={(e) => mockEngine.setDev({ stubMode: e.target.checked })} />
          Stub mode — behave like backend B1 (NOT_IMPLEMENTED)
        </label>
        <label className="dev-check">
          <input type="checkbox" checked={dev.autoProbes} onChange={(e) => mockEngine.setDev({ autoProbes: e.target.checked })} />
          Health probes running
        </label>
        <Select<string> label="speed ×" value={String(dev.timeScale)} options={["0.5", "1", "2", "4"]} onChange={(v) => mockEngine.setDev({ timeScale: Number(v) })} />
        <Select<string> label="IPC delay ms" value={String(dev.ipcDelayMs)} options={["0", "120", "600", "2000"]} onChange={(v) => mockEngine.setDev({ ipcDelayMs: Number(v) })} />
        <button onClick={() => mockEngine.resetAll()}>Reset everything</button>
      </section>

      <EventMonitor />
    </div>
  );
}

function EventMonitor() {
  const [items, setItems] = useState<{ id: number; t: string; name: string; payload: string }[]>([]);
  useEffect(() => {
    let id = 0;
    return mockEngine.onAny((name, payload) => {
      if (name === "trafficStatsUpdated") return; // 1 Hz noise
      const text = JSON.stringify(payload);
      setItems((xs) =>
        [{ id: ++id, t: new Date().toTimeString().slice(0, 8), name, payload: text.length > 220 ? `${text.slice(0, 220)}…` : text }, ...xs].slice(0, 80),
      );
    });
  }, []);
  return (
    <section className="dev-events">
      <h4>
        Events <button onClick={() => setItems([])}>clear</button>
      </h4>
      <ol>
        {items.map((e) => (
          <li key={e.id}>
            <span>{e.t}</span> <b>{e.name}</b> <code>{e.payload}</code>
          </li>
        ))}
      </ol>
    </section>
  );
}
