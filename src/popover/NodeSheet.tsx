// Quick node switcher (UI prompt §5): a paper card over the popover listing
// each node with protocol badge and last test. Select to switch; Test all;
// Manage nodes. Keyboard: ↑/↓ move, Enter selects, Esc closes.
// Motion moment 6: slides up over a fading wash, rows stagger in, the
// selection bar slides to the chosen row before the sheet closes.
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { motion } from "motion/react";
import { sheetTimeline as S, staggerDelay } from "../design/motion";
import { useMotionOk } from "../ui/motionGate";
import { commands } from "../api/client";
import type { AppError, Node } from "../api/bindings";
import { protocolBadge } from "../lib/format";
import { MetaLabel } from "../ui/Frame";

type TestState = { kind: "testing" } | { kind: "error"; error: AppError };

export function NodeSheet({
  nodes,
  selectedId,
  activeId,
  onClose,
}: {
  nodes: Node[];
  selectedId: string | null;
  activeId: string | null;
  onClose: () => void;
}) {
  const [tests, setTests] = useState<Record<string, TestState>>({});
  const [selectError, setSelectError] = useState<AppError | null>(null);
  /** Row the selection bar is moving to while select_node runs. */
  const [pending, setPending] = useState<string | null>(null);
  const animate = useMotionOk();
  const markOn = pending ?? selectedId;
  const rows = useRef<(HTMLButtonElement | null)[]>([]);

  // Focus the current node when the sheet opens.
  useEffect(() => {
    const i = Math.max(0, nodes.findIndex((n) => n.id === selectedId));
    rows.current[i]?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const test = async (n: Node) => {
    setTests((t) => ({ ...t, [n.id]: { kind: "testing" } }));
    const r = await commands.testNode(n.id);
    setTests((t) => {
      const { [n.id]: _, ...rest } = t;
      // On success the result arrives as node.lastTest via nodesChanged (SPEC §2.1).
      return r.status === "ok" ? rest : { ...rest, [n.id]: { kind: "error", error: r.error } };
    });
  };
  const testAll = () => nodes.filter((n) => !n.secretMissing && tests[n.id]?.kind !== "testing").forEach((n) => void test(n));
  const anyTesting = Object.values(tests).some((t) => t.kind === "testing");

  const select = async (n: Node) => {
    if (n.id === selectedId) return onClose();
    setSelectError(null);
    setPending(n.id); // the bar starts sliding right away
    const r = await commands.selectNode(n.id);
    if (r.status === "ok") window.setTimeout(onClose, animate ? S.closeAfterSelectMs : 0);
    else {
      setPending(null); // back to the real selection
      setSelectError(r.error);
    }
  };

  const onKey = (e: KeyboardEvent) => {
    const i = rows.current.findIndex((el) => el === document.activeElement);
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowDown" ? 1 : -1) + nodes.length) % nodes.length;
      rows.current[next]?.focus();
    }
  };

  return (
    <motion.div
      className="absolute inset-0 z-20"
      onKeyDown={onKey}
      initial={animate ? "closed" : false}
      animate="open"
      exit={animate ? "closed" : undefined}
    >
      {/* scrim: plain paper wash, no blur (§6.1) */}
      <motion.button
        type="button"
        aria-label="Close node list"
        className="absolute inset-0 bg-paper/70"
        onClick={onClose}
        tabIndex={-1}
        variants={{ closed: { opacity: 0, transition: S.scrimOut }, open: { opacity: 1, transition: S.scrimIn } }}
      />
      <motion.section
        role="dialog"
        aria-label="Choose node"
        className="absolute inset-x-3 bottom-3 flex max-h-[392px] flex-col rounded-xs bg-paper-raised shadow-sheet"
        variants={{
          closed: { y: S.cardShift, opacity: 0, transition: S.cardOut },
          open: { y: 0, opacity: 1, transition: S.cardIn },
        }}
      >
        <header className="flex items-center justify-between px-4 pt-3 pb-2">
          <MetaLabel>Nodes · {nodes.length}</MetaLabel>
          <button type="button" onClick={onClose} className="label text-ink-muted hover:text-ink focus-visible:outline-2 focus-visible:outline-cobalt">
            Close
          </button>
        </header>

        <ul className="min-h-0 flex-1 overflow-y-auto" role="listbox" aria-label="Nodes">
          {nodes.map((n, i) => {
            const selected = n.id === selectedId;
            const t = tests[n.id];
            return (
              <motion.li
                key={n.id}
                role="none"
                variants={{
                  closed: { opacity: 0, y: S.rowShift, transition: { duration: 0 } },
                  open: { opacity: 1, y: 0, transition: { ...S.cardIn, delay: S.rowsDelay + staggerDelay(i) } },
                }}
              >
                <button
                  ref={(el) => {
                    rows.current[i] = el;
                  }}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  onClick={() => void select(n)}
                  className="group relative flex h-12 w-full items-center gap-3 px-4 text-left hover:bg-ink/[0.04] focus-visible:bg-ink/[0.06] focus-visible:outline-none"
                >
                  {/* selection mark: cobalt bar on the left edge; slides between rows */}
                  {n.id === markOn && (
                    <motion.span
                      aria-hidden
                      layoutId={animate ? "node-sheet-mark" : undefined}
                      transition={S.mark}
                      className="absolute inset-y-2 left-0 w-[3px] bg-cobalt"
                    />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-body font-medium">{n.name}</span>
                    <span className="label block truncate text-ink-muted">
                      <span className="text-ink">{protocolBadge(n.protocol)}</span> · {n.id === activeId ? "carrying traffic" : `${n.server}:${n.port}`}
                    </span>
                  </span>
                  <NodeResult node={n} test={t} />
                </button>
              </motion.li>
            );
          })}
        </ul>

        {selectError && <p className="px-4 pt-2 text-secondary text-vermilion-text">{selectError.message}</p>}

        <footer className="flex items-center justify-between border-t border-line px-4 py-3">
          <button
            type="button"
            onClick={testAll}
            disabled={anyTesting || nodes.every((n) => n.secretMissing)}
            className="label border border-ink px-3 py-1.5 hover:bg-ink/5 disabled:border-ink/30 disabled:text-ink-muted focus-visible:outline-2 focus-visible:outline-cobalt"
          >
            {anyTesting ? "Testing…" : "Test all"}
          </button>
          <button
            type="button"
            onClick={() => void commands.openSettings("nodes")}
            className="label text-ink underline underline-offset-4 hover:no-underline focus-visible:outline-2 focus-visible:outline-cobalt"
          >
            Manage nodes
          </button>
        </footer>
      </motion.section>
    </motion.div>
  );
}

/** Right-hand result column: credentials missing › testing › test error › last test. */
function NodeResult({ node, test }: { node: Node; test: TestState | undefined }) {
  const cls = "label w-[68px] shrink-0 text-right tabular";
  if (node.secretMissing) return <span className={`${cls} text-vermilion-text`}>No creds</span>;
  if (test?.kind === "testing") return <span className={`${cls} text-ink-muted`}>Testing…</span>;
  if (test?.kind === "error")
    return (
      <span className={`${cls} text-vermilion-text`} title={test.error.message}>
        {test.error.code === "NOT_IMPLEMENTED" ? "Not avail." : "Error"}
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
    <span className={`${cls} text-ink`} title={lt.exitCountry ? `Exit country: ${lt.exitCountry}` : undefined}>
      {lt.latencyMs} ms
    </span>
  );
}
