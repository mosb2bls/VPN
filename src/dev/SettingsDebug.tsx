// U1.1 debug view of the settings window: exercises every settings-side
// command against the mock/real backend. Visual design comes in U3.
import { useEffect, useState } from "react";
import { commands, on } from "../api/client";
import type {
  AppInfo,
  AppSettings,
  FailoverSettings,
  HelperStatus,
  LogEntry,
  LogLevel,
  NodeForm,
  RulesStatus,
  SafetySettings,
} from "../api/bindings";
import type { SettingsSection } from "../api/bindings";
import { protocolBadge } from "../lib/format";
import { useApp } from "../state/store";
import { useRun, type RunOutcome } from "./useRun";

const SECTIONS: SettingsSection[] = ["nodes", "connection", "failover", "safety", "rules", "helper", "startup", "logs", "about"];

function Outcome({ last, busy }: { last: RunOutcome | null; busy: string | null }) {
  if (busy) return <div className="dbg-result">… {busy}</div>;
  if (!last) return <div className="dbg-result"> </div>;
  if (last.ok) return <div className="dbg-result">✓ {last.label}{last.summary ? ` — ${last.summary}` : ""}</div>;
  const e = last.error!;
  return (
    <div className="dbg-result dbg-result-err">
      ✕ {last.label}: <b>{e.code}</b> — {e.message}
      {e.fields.length > 0 && (
        <ul>
          {e.fields.map((f) => (
            <li key={f.field}>
              <code>{f.field}</code>: {f.code} — {f.message}
            </li>
          ))}
        </ul>
      )}
      {e.detail && <div className="dbg-detail">detail: {e.detail}</div>}
    </div>
  );
}

const VALID_HY2: NodeForm = {
  protocol: "hysteria2",
  name: "Manual HY2",
  server: "manual.example.net",
  port: 24443,
  serverPorts: [],
  password: { kind: "set", value: "manual-secret-123" },
  sni: null,
  insecure: false,
  obfsPassword: { kind: "keep" },
  pinSha256: null,
  upMbps: null,
  downMbps: null,
};
const INVALID_VLESS: NodeForm = {
  protocol: "vless_reality",
  name: "",
  server: "not a host",
  port: 70000,
  uuid: { kind: "set", value: "not-a-uuid" },
  sni: "",
  fingerprint: "netscape",
  publicKey: { kind: "set", value: "short" },
  shortId: { kind: "set", value: "xyz" },
  spiderX: null,
};

export function SettingsDebug() {
  const [section, setSection] = useState<SettingsSection>("nodes");
  useEffect(() => {
    let off: (() => void) | undefined;
    void on("settingsSectionRequested", (p) => setSection(p.section)).then((u) => (off = u));
    return () => off?.();
  }, []);

  return (
    <div className="dbg dbg-set">
      <nav className="dbg-rail">
        {SECTIONS.map((s, i) => (
          <button key={s} aria-pressed={s === section} onClick={() => setSection(s)}>
            {String(i + 1).padStart(2, "0")} / {s.toUpperCase()}
          </button>
        ))}
      </nav>
      <section className="dbg-body">
        {section === "nodes" && <Nodes />}
        {section === "connection" && <Connection />}
        {section === "failover" && <JsonSettings<FailoverSettings> label="Failover" get={commands.getFailoverSettings} put={commands.setFailoverSettings} />}
        {section === "safety" && <JsonSettings<SafetySettings> label="Safety" get={commands.getSafetySettings} put={commands.setSafetySettings} />}
        {section === "rules" && <Rules />}
        {section === "helper" && <Helper />}
        {section === "startup" && <JsonSettings<AppSettings> label="App settings" get={commands.getSettings} put={commands.setSettings} />}
        {section === "logs" && <Logs />}
        {section === "about" && <About />}
      </section>
    </div>
  );
}

function Nodes() {
  const nodes = useApp((s) => s.nodes);
  const status = useApp((s) => s.status);
  const { run, last, busy } = useRun();
  const [text, setText] = useState("");
  const imported = (r: { added: unknown[]; errors: { line: number; error: { code: string } }[] }) =>
    `${r.added.length} added${r.errors.length ? `, errors: ${r.errors.map((e) => `line ${e.line} ${e.error.code}`).join("; ")}` : ""}`;

  const move = (i: number) => {
    const ids = nodes.map((n) => n.id);
    [ids[i - 1], ids[i]] = [ids[i], ids[i - 1]];
    void run("reorderNodes", commands.reorderNodes(ids), (r) => `${r.length} nodes`);
  };

  return (
    <>
      <h3>Nodes ({nodes.length})</h3>
      <table className="dbg-table">
        <tbody>
          {nodes.map((n, i) => (
            <tr key={n.id}>
              <td>{n.id === status?.activeNodeId ? "●" : n.id === status?.selectedNodeId ? "○" : ""}</td>
              <td className="dbg-badge">{protocolBadge(n.protocol)}</td>
              <td className="dbg-clip">{n.name}</td>
              <td className="dbg-mono">
                {n.lastTest ? (n.lastTest.ok ? `${n.lastTest.latencyMs} ms ${n.lastTest.exitCountry}` : n.lastTest.error?.code) : "—"}
              </td>
              <td>
                {n.details.protocol === "hysteria2" && n.details.insecure && <span className="dbg-tag">INSECURE</span>}
              </td>
              <td className="dbg-btns">
                <button disabled={i === 0} onClick={() => move(i)}>↑</button>
                <button onClick={() => run(`testNode ${n.name}`, commands.testNode(n.id), (r) => (r.ok ? `${r.latencyMs} ms` : `${r.error?.code}`))}>Test</button>
                <button onClick={() => run("deleteNode", commands.deleteNode(n.id))}>Delete</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="dbg-btns">
        <button onClick={() => nodes.forEach((n) => run(`testNode ${n.name}`, commands.testNode(n.id), (r) => (r.ok ? `${r.latencyMs} ms` : `${r.error?.code}`)))}>
          Test all
        </button>
        <button onClick={() => run("addNodeManual (valid HY2)", commands.addNodeManual(VALID_HY2), (n) => n.name)}>Add valid HY2 form</button>
        <button onClick={() => run("addNodeManual (invalid)", commands.addNodeManual(INVALID_VLESS))}>Add invalid REALITY form</button>
      </div>
      <textarea rows={3} value={text} onChange={(e) => setText(e.target.value)} placeholder="Paste hy2:// or vless:// links, one per line" />
      <div className="dbg-btns">
        <button onClick={() => run("importLinks", commands.importLinks(text), imported)}>Import links</button>
        <button onClick={() => run("importFromClipboard", commands.importFromClipboard(), imported)}>Import from clipboard</button>
        <label className="dbg-file">
          QR image…
          <input
            type="file"
            accept="image/png,image/jpeg"
            onChange={async (e) => {
              const f = e.target.files?.[0];
              if (!f) return;
              const b64 = btoa(String.fromCharCode(...new Uint8Array(await f.arrayBuffer())));
              void run("importQrImage", commands.importQrImage({ kind: "bytes", base64: b64 }), imported);
              e.target.value = "";
            }}
          />
        </label>
      </div>
      <Outcome last={last} busy={busy} />
    </>
  );
}

export function Connection() {
  const status = useApp((s) => s.status);
  const { run, last, busy } = useRun();
  if (!status) return null;
  return (
    <>
      <h3>Connection</h3>
      <p className="dbg-mono">
        capture={status.captureMode} routing={status.routingMode} dns={status.dnsProfile} systemProxySet={String(status.systemProxySet)}{" "}
        killSwitchActive={String(status.killSwitchActive)}
      </p>
      <div className="dbg-seg">
        {(["mainland", "overseas"] as const).map((p) => (
          <button key={p} aria-pressed={status.dnsProfile === p} onClick={() => run("setDnsProfile", commands.setDnsProfile(p))}>
            DNS {p.toUpperCase()}
          </button>
        ))}
      </div>
      <Outcome last={last} busy={busy} />
    </>
  );
}

export function JsonSettings<T>({ label, get, put }: { label: string; get: () => Promise<{ status: "ok"; data: T } | { status: "error"; error: import("../api/bindings").AppError }>; put: (v: T) => Promise<{ status: "ok"; data: T } | { status: "error"; error: import("../api/bindings").AppError }> }) {
  const [text, setText] = useState("");
  const { run, last, busy } = useRun();
  useEffect(() => {
    void get().then((r) => r.status === "ok" && setText(JSON.stringify(r.data, null, 2)));
  }, [get]);
  return (
    <>
      <h3>{label}</h3>
      <textarea className="dbg-json" rows={14} value={text} onChange={(e) => setText(e.target.value)} />
      <div className="dbg-btns">
        <button
          onClick={() => {
            try {
              void run(`save ${label}`, put(JSON.parse(text) as T));
            } catch {
              /* invalid JSON — ignore in debug view */
            }
          }}
        >
          Save
        </button>
      </div>
      <Outcome last={last} busy={busy} />
    </>
  );
}

export function Rules() {
  const [rules, setRules] = useState<RulesStatus | null>(null);
  const { run, last, busy } = useRun();
  useEffect(() => {
    void commands.getRulesStatus().then((r) => r.status === "ok" && setRules(r.data));
  }, [last]);
  return (
    <>
      <h3>Rules</h3>
      <pre className="dbg-pre">{JSON.stringify(rules, null, 2)}</pre>
      <button onClick={() => run("updateRulesNow", commands.updateRulesNow(), (r) => r.version)}>Update now</button>
      <Outcome last={last} busy={busy} />
    </>
  );
}

export function Helper() {
  const helper = useApp((s) => s.helper) as HelperStatus | null;
  const { run, last, busy } = useRun();
  return (
    <>
      <h3>Helper</h3>
      <pre className="dbg-pre">{JSON.stringify(helper, null, 2)}</pre>
      <div className="dbg-btns">
        <button onClick={() => run("installHelper", commands.installHelper())}>Install (admin prompt)</button>
        <button onClick={() => run("uninstallHelper", commands.uninstallHelper())}>Uninstall</button>
      </div>
      <Outcome last={last} busy={busy} />
    </>
  );
}

export function Logs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [level, setLevel] = useState<LogLevel>("info");
  const activity = useApp((s) => s.activity);
  const { run, last, busy } = useRun();
  useEffect(() => {
    const load = () => void commands.getLogs(200, level).then((r) => r.status === "ok" && setLogs(r.data));
    load();
    const t = setInterval(load, 2000); // SPEC §2.6: polling ~2 s is fine
    return () => clearInterval(t);
  }, [level]);
  return (
    <>
      <h3>Logs</h3>
      <div className="dbg-btns">
        <select value={level} onChange={(e) => setLevel(e.target.value as LogLevel)}>
          {["trace", "debug", "info", "warn", "error"].map((l) => (
            <option key={l}>{l}</option>
          ))}
        </select>
        <button onClick={() => run("exportDiagnostics", commands.exportDiagnostics(), (d) => d.path)}>Export diagnostics</button>
        <button onClick={() => run("revealDiagnostics", commands.revealDiagnostics())}>Reveal</button>
        <button onClick={() => run("restoreNetworking", commands.restoreNetworking(), (r) => (r.fullyRestored ? "fully restored" : "incomplete"))}>
          Restore networking
        </button>
      </div>
      <Outcome last={last} busy={busy} />
      <pre className="dbg-pre dbg-logs">
        {logs.map((l) => `${l.ts.slice(11, 19)} ${l.level.padEnd(5)} ${l.source.padEnd(6)} ${l.message}`).join("\n") || "(no log entries)"}
      </pre>
      <RestoreItems />
      <h4>Activity (store)</h4>
      <ul className="dbg-activity">
        {activity.map((a) => (
          <li key={a.id} data-tone={a.tone}>
            {a.at.slice(11, 19)} {a.text}
          </li>
        ))}
      </ul>
    </>
  );
}

function RestoreItems() {
  const restore = useApp((s) => s.restore);
  if (!restore) return <h4>Last restore: none</h4>;
  return (
    <>
      <h4>
        Last restore ({restore.trigger}, {restore.at.slice(11, 19)}) — {restore.fullyRestored ? "complete" : "INCOMPLETE"}
      </h4>
      <table className="dbg-table">
        <tbody>
          {restore.items.map((i) => (
            <tr key={i.kind}>
              <td>{i.label}</td>
              <td className="dbg-mono">{i.result}</td>
              <td>{i.errorMessage ?? i.action}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

export function About() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    void commands.getAppInfo().then((r) => r.status === "ok" && setInfo(r.data));
  }, []);
  return (
    <>
      <h3>About</h3>
      <pre className="dbg-pre">{JSON.stringify(info, null, 2)}</pre>
    </>
  );
}
