// 01 / NODES — list, reorder, test, edit, delete; import via paste, clipboard,
// QR image or the manual form (UI prompt §5, SPEC §2.1, §9 items 6, 11, 14–17).
import { useState } from "react";
import { commands } from "../../api/client";
import type { AppError, ImportResult, Node } from "../../api/bindings";
import { protocolBadge } from "../../lib/format";
import { useApp } from "../../state/store";
import { HoldButton } from "../../ui/HoldButton";
import { Button, ErrorNote, Note, SectionHeader } from "../kit";
import { NodeEditor } from "../nodes/NodeEditor";
import { ImportResultNote, LINK_RULES, PasteLinks, QrImport } from "../nodes/ImportPanels";

type Panel = { kind: "none" } | { kind: "paste" } | { kind: "qr" } | { kind: "add" } | { kind: "edit"; id: string };
type Test = { kind: "testing" } | { kind: "error"; error: AppError };

export function NodesSection() {
  const nodes = useApp((s) => s.nodes);
  const status = useApp((s) => s.status);
  const [panel, setPanel] = useState<Panel>({ kind: "none" });
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [saved, setSaved] = useState<{ name: string; reconnect: boolean } | null>(null);
  const [clipBusy, setClipBusy] = useState(false);
  const [tests, setTests] = useState<Record<string, Test>>({});

  const activeId = status?.activeNodeId ?? null;
  const connected = !!activeId;

  const clipboard = async () => {
    setClipBusy(true);
    setError(null);
    setResult(null);
    const r = await commands.importFromClipboard();
    setClipBusy(false);
    if (r.status === "error") setError(r.error);
    else setResult(r.data);
  };

  const test = async (n: Node) => {
    setTests((t) => ({ ...t, [n.id]: { kind: "testing" } }));
    const r = await commands.testNode(n.id);
    setTests((t) => {
      const { [n.id]: _, ...rest } = t;
      return r.status === "ok" ? rest : { ...rest, [n.id]: { kind: "error", error: r.error } };
    });
  };

  const move = async (i: number, d: -1 | 1) => {
    const ids = nodes.map((n) => n.id);
    [ids[i], ids[i + d]] = [ids[i + d], ids[i]];
    const r = await commands.reorderNodes(ids);
    if (r.status === "error") setError(r.error);
  };

  const remove = async (n: Node) => {
    setError(null);
    const r = await commands.deleteNode(n.id);
    if (r.status === "error") setError(r.error);
  };

  const reconnect = async () => {
    setSaved(null);
    const d = await commands.disconnect();
    if (d.status === "error") return setError(d.error);
    const c = await commands.connect();
    if (c.status === "error") setError(c.error);
  };

  const openPanel = (p: Panel) => {
    setPanel(p);
    setResult(null);
    setError(null);
    setSaved(null);
  };

  const toolbar = (
    <>
      <Button onClick={() => openPanel({ kind: "paste" })} aria-pressed={panel.kind === "paste"}>
        Paste links
      </Button>
      <Button onClick={() => void clipboard()} busy={clipBusy}>
        {clipBusy ? "Reading…" : "From clipboard"}
      </Button>
      <Button onClick={() => openPanel({ kind: "qr" })} aria-pressed={panel.kind === "qr"}>
        QR image
      </Button>
      <Button onClick={() => openPanel({ kind: "add" })} aria-pressed={panel.kind === "add"}>
        Add manually
      </Button>
    </>
  );

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="01 / Nodes" title={`Nodes${nodes.length ? ` · ${nodes.length}` : ""}`} />
      <div className="flex flex-wrap gap-2">{toolbar}</div>

      {panel.kind === "paste" && (
        <PasteLinks
          onResult={(r) => {
            setResult(r);
            if (!r.errors.length) setPanel({ kind: "none" });
          }}
          onCancel={() => setPanel({ kind: "none" })}
        />
      )}
      {panel.kind === "qr" && (
        <QrImport
          onResult={(r) => {
            setResult(r);
            setPanel({ kind: "none" });
          }}
          onCancel={() => setPanel({ kind: "none" })}
        />
      )}
      {panel.kind === "add" && (
        <NodeEditor
          node={null}
          isActive={false}
          onDone={(n) => {
            setPanel({ kind: "none" });
            setSaved({ name: n.name, reconnect: false });
          }}
          onCancel={() => setPanel({ kind: "none" })}
        />
      )}

      {result && <ImportResultNote result={result} onClose={() => setResult(null)} />}
      {error && <ErrorNote error={error} title={error.code === "BUSY" ? "This node is carrying traffic" : undefined} />}
      {error?.code === "BUSY" && <p className="text-secondary text-ink-muted">Disconnect or switch to another node first, then delete it.</p>}
      {saved && (
        <Note tone={saved.reconnect ? "warn" : "ok"} title={`Saved “${saved.name}”`}>
          {saved.reconnect && (
            <div className="flex items-center gap-3">
              <span>Changes to the active node apply after reconnecting.</span>
              <Button onClick={() => void reconnect()}>Reconnect now</Button>
            </div>
          )}
        </Note>
      )}

      {nodes.length === 0 ? (
        <div className="flex flex-col items-start gap-2 border-t border-line pt-6">
          <p className="font-display text-[20px] font-[750]">No nodes yet</p>
          <p className="max-w-[460px] text-secondary text-ink-muted">
            Paste share links, read them from the clipboard, drop a QR image, or enter a node by hand. {LINK_RULES}
          </p>
        </div>
      ) : (
        <ol className="flex flex-col border-t border-line" aria-label="Nodes (connection order)">
          {nodes.map((n, i) => {
            if (panel.kind === "edit" && panel.id === n.id)
              return (
                <li key={n.id} className="py-3">
                  <NodeEditor
                    node={n}
                    isActive={connected && n.id === activeId}
                    onDone={(saved, note) => {
                      setPanel({ kind: "none" });
                      setSaved({ name: saved.name, reconnect: note === "reconnect" });
                    }}
                    onCancel={() => setPanel({ kind: "none" })}
                  />
                </li>
              );
            const t = tests[n.id];
            const hy2 = n.details.protocol === "hysteria2" ? n.details : null;
            return (
              <li key={n.id} className="grid grid-cols-[20px_3px_1fr_auto] items-start gap-x-3 border-b border-line py-3">
                <span className="label pt-0.5 text-ink-muted tabular">{String(i + 1).padStart(2, "0")}</span>
                <span className={`h-full min-h-10 w-[3px] ${n.id === status?.selectedNodeId ? "bg-cobalt" : "bg-transparent"}`} aria-hidden />
                <div className="flex min-w-0 flex-col gap-0.5">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-body font-medium">{n.name}</span>
                    {n.id === activeId && <span className="label shrink-0 text-cobalt">Active</span>}
                    {n.id !== activeId && n.id === status?.selectedNodeId && <span className="label shrink-0 text-ink-muted">Selected</span>}
                  </span>
                  <span className="label truncate text-ink-muted">
                    <span className="text-ink">{protocolBadge(n.protocol)}</span> · {n.server}:{n.port}
                    {hy2 && hy2.serverPorts.length > 0 && ` · hop ${hy2.serverPorts.join(",")}`}
                  </span>
                  {n.secretMissing && <span className="text-secondary text-vermilion-text">Credentials missing — edit to re-enter</span>}
                  {hy2?.insecure && <span className="text-secondary text-mustard-text">Certificate verification is off (insecure)</span>}
                </div>
                <div className="flex flex-col items-end gap-1.5">
                  <TestResult node={n} test={t} />
                  <div className="flex items-center gap-1">
                    <Button variant="ghost" className="px-1.5" disabled={i === 0} onClick={() => void move(i, -1)} aria-label={`Move ${n.name} up`} title="Move up">
                      ↑
                    </Button>
                    <Button variant="ghost" className="px-1.5" disabled={i === nodes.length - 1} onClick={() => void move(i, 1)} aria-label={`Move ${n.name} down`} title="Move down">
                      ↓
                    </Button>
                    <Button disabled={n.secretMissing || t?.kind === "testing"} onClick={() => void test(n)}>
                      {t?.kind === "testing" ? "Testing…" : "Test"}
                    </Button>
                    <Button onClick={() => openPanel({ kind: "edit", id: n.id })}>Edit</Button>
                    <HoldButton onConfirm={() => void remove(n)} title="Press and hold to delete">
                      Delete
                    </HoldButton>
                  </div>
                </div>
              </li>
            );
          })}
        </ol>
      )}
      {nodes.length > 1 && <p className="text-secondary text-ink-muted">Order matters: automatic failover picks the first node of the other protocol.</p>}
    </div>
  );
}

function TestResult({ node, test }: { node: Node; test: Test | undefined }) {
  const cls = "label h-4 text-right tabular";
  if (test?.kind === "testing") return <span className={`${cls} text-ink-muted`}>Testing…</span>;
  if (test?.kind === "error")
    return (
      <span className={`${cls} ${test.error.code === "NOT_IMPLEMENTED" ? "text-ink-muted" : "text-vermilion-text"}`} title={test.error.message}>
        {test.error.code === "NOT_IMPLEMENTED" ? "Not available" : "Test error"}
      </span>
    );
  const lt = node.lastTest;
  if (!lt) return <span className={`${cls} text-ink-muted`}>Untested</span>;
  if (!lt.ok)
    return (
      <span className={`${cls} text-vermilion-text`} title={lt.error?.message}>
        Failed
      </span>
    );
  return (
    <span className={`${cls} text-ink`} title={`Tested ${new Date(lt.testedAt).toLocaleString()}`}>
      {lt.latencyMs} ms{lt.exitCountry ? ` · ${lt.exitCountry}` : ""}
    </span>
  );
}
