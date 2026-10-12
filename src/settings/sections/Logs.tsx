// 08 / LOGS — recovery / restore reports, Restore networking, diagnostics, log
// viewer (SPEC §2.6, §2.7, §9.8, §9.10). Logs arrive already redacted.
import { useEffect, useRef, useState } from "react";
import { commands } from "../../api/client";
import type { AppError, AppSettings, LogEntry, LogLevel, LogSource, RestoreItem, RestoreReport } from "../../api/bindings";
import { useApp } from "../../state/store";
import { usePageVisible } from "../../ui/motionGate";
import { Button, ErrorNote, Note, SectionHeader } from "../kit";
import { useResource } from "../useResource";

const RESULT: Record<RestoreItem["result"], { text: string; cls: string }> = {
  restored: { text: "Restored", cls: "text-cobalt" },
  not_needed: { text: "Not changed", cls: "text-ink-muted" },
  failed: { text: "Not restored", cls: "text-vermilion-text" },
  skipped: { text: "Not attempted", cls: "text-vermilion-text" },
};
const TRIGGER: Record<RestoreReport["trigger"], string> = {
  disconnect: "Disconnect",
  restore_networking: "Restore networking",
  quit: "Quit",
  direct_mode: "Switch to Direct",
  capture_mode_switch: "Capture mode change",
  recovery: "Launch recovery",
  command_line: "--restore-networking",
};
const LEVELS: LogLevel[] = ["error", "warn", "info", "debug", "trace"];
const SOURCES: LogSource[] = ["app", "core", "helper"];

export function LogsSection() {
  const restore = useApp((s) => s.restore);
  const recovery = useApp((s) => s.recovery);
  const [restoring, setRestoring] = useState(false);
  const [restoreError, setRestoreError] = useState<AppError | null>(null);
  const incomplete = !!restore && !restore.fullyRestored;

  const restoreNow = async () => {
    setRestoring(true);
    setRestoreError(null);
    const r = await commands.restoreNetworking();
    setRestoring(false);
    // An incomplete result also updates `restore` (restoreReportChanged), shown below.
    if (r.status === "error" && r.error.code !== "RESTORE_INCOMPLETE") setRestoreError(r.error);
  };

  return (
    <div className="flex flex-col gap-6">
      <SectionHeader index="08 / Logs" title="Logs" />

      {incomplete && restore ? (
        <section className="flex flex-col gap-3" aria-labelledby="restore-h">
          <Note tone="error" title="Networking is not back to normal">
            Some settings EdgeClient changed could not be put back. Until they are, parts of your connection may not work or may bypass
            your usual settings.
          </Note>
          <RestoreTable report={restore} />
          <div className="flex items-center gap-3">
            <Button variant="solid" busy={restoring} onClick={() => void restoreNow()}>
              {restoring ? "Restoring…" : "Retry restore"}
            </Button>
            <span className="text-secondary text-ink-muted">Safe to retry as often as needed.</span>
          </div>
          <p className="text-secondary text-ink-muted">
            If EdgeClient cannot open, run it from a terminal with <code className="font-mono text-[11px] text-ink">--restore-networking</code> —
            it does the same restore without the window.
          </p>
        </section>
      ) : (
        <section className="flex flex-col gap-3">
          <h3 className="label">Network state</h3>
          <p className="text-secondary">
            {restore ? `Last restore (${TRIGGER[restore.trigger]}, ${new Date(restore.at).toLocaleString()}) completed — everything EdgeClient changed is back.` : "No restore has run since EdgeClient started."}
          </p>
          {restore && (
            <details>
              <summary className="label cursor-pointer text-ink-muted">Show items</summary>
              <div className="mt-2">
                <RestoreTable report={restore} />
              </div>
            </details>
          )}
          <div>
            <Button busy={restoring} onClick={() => void restoreNow()}>
              {restoring ? "Restoring…" : "Restore networking"}
            </Button>
          </div>
        </section>
      )}
      {restoreError && <ErrorNote error={restoreError} />}

      {recovery && (
        <Note tone={recovery.outcome === "restore_incomplete" ? "error" : "info"} title="After the last unclean shutdown">
          {recovery.outcome === "reattached"
            ? "The previous TUN session was still healthy, so EdgeClient re-attached to it."
            : recovery.outcome === "restored"
              ? "EdgeClient put your network settings back."
              : "Some network settings could not be put back — see above."}{" "}
          <span className="text-ink-muted">{new Date(recovery.at).toLocaleString()}</span>
        </Note>
      )}

      <Diagnostics />
      <LogViewer />
    </div>
  );
}

function RestoreTable({ report }: { report: RestoreReport }) {
  return (
    <table className="w-full text-secondary">
      <tbody>
        {report.items.map((i) => (
          <tr key={i.kind} className="border-t border-line align-top">
            <td className="w-[150px] py-1.5 pr-3 font-medium">{i.label}</td>
            <td className={`label w-[110px] py-1.5 pt-2 ${RESULT[i.result].cls}`}>{RESULT[i.result].text}</td>
            <td className="py-1.5">
              {i.errorMessage ?? i.action}
              {i.errorMessage && <span className="block text-ink-muted">{i.action}</span>}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Diagnostics() {
  const [path, setPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const app = useResource<AppSettings>(commands.getSettings, commands.setSettings);

  const exportNow = async () => {
    setBusy(true);
    setError(null);
    const r = await commands.exportDiagnostics();
    setBusy(false);
    if (r.status === "ok") setPath(r.data.path);
    else setError(r.error);
  };
  const reveal = async () => {
    const r = await commands.revealDiagnostics();
    if (r.status === "error") setError(r.error);
  };

  return (
    <section className="flex flex-col gap-3 border-t border-line pt-5">
      <h3 className="label">Diagnostics</h3>
      <p className="text-secondary">
        Exports a zip to your Downloads folder with logs, a redacted configuration, status and versions — no passwords, keys or full
        server details. It is never uploaded; share it only if you choose to.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => void exportNow()} busy={busy}>
          {busy ? "Exporting…" : "Export diagnostics"}
        </Button>
        {path && <Button variant="ghost" onClick={() => void reveal()}>Show in folder</Button>}
        {app.saved && (
          <label className="ml-auto flex items-center gap-2">
            <span className="label text-ink-muted">Log detail</span>
            <select
              value={app.saved.logLevel}
              onChange={(e) => void app.save({ ...app.saved!, logLevel: e.target.value as LogLevel })}
              className="h-8 border border-line bg-paper-raised px-2 font-mono text-[12px] focus-visible:outline-2 focus-visible:outline-cobalt"
            >
              {(["info", "debug", "trace", "warn", "error"] as LogLevel[]).map((l) => (
                <option key={l}>{l}</option>
              ))}
            </select>
          </label>
        )}
      </div>
      {path && <p className="font-mono text-[11px] break-all text-ink-muted">{path}</p>}
      {error && <ErrorNote error={error} />}
      {app.error && <ErrorNote error={app.error} />}
    </section>
  );
}

function LogViewer() {
  const [min, setMin] = useState<LogLevel>("info");
  const [sources, setSources] = useState<Set<LogSource>>(new Set(SOURCES));
  const [paused, setPaused] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [error, setError] = useState<AppError | null>(null);
  const visible = usePageVisible();
  const box = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  // SPEC §2.6: polling ~2 s while the view is visible is fine.
  useEffect(() => {
    if (paused || !visible) return;
    const load = () =>
      void commands.getLogs(500, min).then((r) => {
        if (r.status === "ok") {
          setLogs(r.data);
          setError(null);
        } else setError(r.error);
      });
    load();
    const t = window.setInterval(load, 2000);
    return () => window.clearInterval(t);
  }, [min, paused, visible]);

  useEffect(() => {
    if (stick.current && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [logs]);

  const shown = logs.filter((l) => sources.has(l.source));
  const toggle = (s: LogSource) =>
    setSources((cur) => {
      const n = new Set(cur);
      if (n.has(s)) n.delete(s);
      else n.add(s);
      return n;
    });

  return (
    <section className="flex flex-col gap-3 border-t border-line pt-5">
      <div className="flex flex-wrap items-center gap-3">
        <h3 className="label">Log</h3>
        <select
          aria-label="Minimum level"
          value={min}
          onChange={(e) => setMin(e.target.value as LogLevel)}
          className="h-7 border border-line bg-paper-raised px-1 font-mono text-[11px] focus-visible:outline-2 focus-visible:outline-cobalt"
        >
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l} and above
            </option>
          ))}
        </select>
        <div className="flex gap-1" role="group" aria-label="Sources">
          {SOURCES.map((s) => (
            <button
              key={s}
              type="button"
              aria-pressed={sources.has(s)}
              onClick={() => toggle(s)}
              className={`label h-7 border px-2 ${sources.has(s) ? "border-ink bg-ink text-paper" : "border-line text-ink-muted"} focus-visible:outline-2 focus-visible:outline-cobalt`}
            >
              {s}
            </button>
          ))}
        </div>
        <Button variant="ghost" className="ml-auto h-7" onClick={() => setPaused((p) => !p)}>
          {paused ? "Resume" : "Pause"}
        </Button>
      </div>
      <div
        ref={box}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className="h-[260px] overflow-y-auto border border-line bg-paper-raised px-3 py-2 font-mono text-[11px] leading-[17px]"
        role="log"
        aria-live="off"
      >
        {shown.length === 0 ? (
          <p className="text-ink-muted">No log entries at this level.</p>
        ) : (
          shown.map((l, i) => (
            <div key={`${l.ts}-${i}`} className="grid grid-cols-[64px_44px_48px_1fr] gap-2">
              <span className="text-ink-muted">{l.ts.slice(11, 19)}</span>
              <span className={l.level === "error" ? "text-vermilion-text" : l.level === "warn" ? "text-mustard-text" : "text-ink-muted"}>{l.level}</span>
              <span className="text-ink-muted">{l.source}</span>
              <span className="break-all whitespace-pre-wrap">{l.message}</span>
            </div>
          ))
        )}
      </div>
      <p className="text-secondary text-ink-muted">Entries are redacted before they reach this window. Times are UTC.</p>
      {error && <ErrorNote error={error} />}
    </section>
  );
}
