// Settings window (SPEC §2.8: 760 × 560, resizable, min 640 × 480). Left rail
// with the nine sections; content column on the right. Same paper and ink as
// the popover, no grid. Motion moment 7: the rail's ink block slides to the
// chosen section and the content cross-fades with a small upward shift.
import { useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { commands, on } from "../api/client";
import type { AppSettings, FailoverSettings, SafetySettings, SettingsSection } from "../api/bindings";
import { navTimeline as N } from "../design/motion";
import { About, Connection, Helper, JsonSettings, Logs, Rules } from "../dev/SettingsDebug";
import { protocolBadge } from "../lib/format";
import { derivePopoverView, TONE_CLASS } from "../popover/view";
import { useApp } from "../state/store";
import { useMotionOk } from "../ui/motionGate";
import { Note, SectionHeader } from "./kit";
import { NodesSection } from "./sections/Nodes";
import "../dev/dev.css"; // debug panels for sections not designed yet

const SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: "nodes", label: "Nodes" },
  { id: "connection", label: "Connection" },
  { id: "failover", label: "Failover" },
  { id: "safety", label: "Safety" },
  { id: "rules", label: "Rules" },
  { id: "helper", label: "Helper" },
  { id: "startup", label: "Startup" },
  { id: "logs", label: "Logs" },
  { id: "about", label: "About" },
];
const num = (i: number) => String(i + 1).padStart(2, "0");

export function Settings() {
  const [section, setSection] = useState<SettingsSection>("nodes");
  const animate = useMotionOk();

  // openSettings(section) from the popover or tray (SPEC §2.8).
  useEffect(() => {
    let off: (() => void) | undefined;
    void on("settingsSectionRequested", (p) => setSection(p.section)).then((u) => (off = u));
    return () => off?.();
  }, []);

  const idx = SECTIONS.findIndex((s) => s.id === section);

  return (
    <div className="paper-grain grid h-full min-h-0 grid-cols-[184px_1fr] text-ink">
      <nav aria-label="Settings sections" className="flex min-h-0 flex-col border-r border-line px-3 pt-5 pb-4">
        <span className="label px-2 pb-4">EdgeClient</span>
        <ul className="flex flex-col">
          {SECTIONS.map((s, i) => {
            const on = s.id === section;
            return (
              <li key={s.id}>
                <button
                  type="button"
                  aria-current={on ? "page" : undefined}
                  onClick={() => setSection(s.id)}
                  className={`label relative flex h-8 w-full items-center gap-3 px-2 text-left transition-colors duration-base ${
                    on ? "text-paper" : "text-ink hover:bg-ink/5"
                  } focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-cobalt`}
                >
                  {on && (
                    <motion.span
                      aria-hidden
                      layoutId={animate ? "settings-rail" : undefined}
                      transition={N.rail}
                      className="absolute inset-0 bg-ink"
                    />
                  )}
                  <span className="relative tabular">{num(i)}</span>
                  <span className="relative">{s.label}</span>
                </button>
              </li>
            );
          })}
        </ul>
        <div className="mt-auto px-2">
          <RailStatus />
        </div>
      </nav>

      <main className="min-h-0 overflow-y-auto" id="settings-content">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={section}
            className="mx-auto flex max-w-[620px] flex-col px-10 pt-8 pb-12"
            initial={animate ? { opacity: 0, y: N.shift } : false}
            animate={{ opacity: 1, y: 0, transition: N.contentIn }}
            exit={animate ? { opacity: 0, transition: N.contentOut } : undefined}
          >
            <Section id={section} index={`${num(idx)} / ${SECTIONS[idx].label}`} />
          </motion.div>
        </AnimatePresence>
      </main>
    </div>
  );
}

/** Small live status at the bottom of the rail: coloured word + node. */
function RailStatus() {
  const status = useApp((s) => s.status);
  const nodes = useApp((s) => s.nodes);
  const helper = useApp((s) => s.helper);
  const restore = useApp((s) => s.restore);
  const reason = useApp((s) => s.statusReason);
  if (!status) return null;
  const v = derivePopoverView(status, reason, helper, restore, nodes);
  const node = nodes.find((n) => n.id === (status.activeNodeId ?? status.selectedNodeId));
  return (
    <div className="flex flex-col gap-0.5">
      <span className="label text-ink-muted">Status</span>
      <span className={`font-display text-[15px] leading-5 font-[850] [font-stretch:82%] uppercase ${TONE_CLASS[v.tone]}`}>{v.word}</span>
      {node && (
        <span className="label truncate text-ink-muted">
          {protocolBadge(node.protocol)} · {node.name}
        </span>
      )}
    </div>
  );
}

function Section({ id, index }: { id: SettingsSection; index: string }) {
  if (id === "nodes") return <NodesSection />;
  const title = index.split(" / ")[1];
  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index={index} title={title} />
      <Note tone="info" title="Design pending">
        This section is designed in a later U3 step. The controls below are the working debug view.
      </Note>
      <DebugBody id={id} />
    </div>
  );
}

function DebugBody({ id }: { id: SettingsSection }): ReactNode {
  const wrap = (c: ReactNode) => <div className="dbg flex flex-col gap-2 border border-line bg-paper-raised p-4">{c}</div>;
  switch (id) {
    case "connection":
      return wrap(<Connection />);
    case "failover":
      return wrap(<JsonSettings<FailoverSettings> label="Failover" get={commands.getFailoverSettings} put={commands.setFailoverSettings} />);
    case "safety":
      return wrap(<JsonSettings<SafetySettings> label="Safety" get={commands.getSafetySettings} put={commands.setSafetySettings} />);
    case "rules":
      return wrap(<Rules />);
    case "helper":
      return wrap(<Helper />);
    case "startup":
      return wrap(<JsonSettings<AppSettings> label="App settings" get={commands.getSettings} put={commands.setSettings} />);
    case "logs":
      return wrap(<Logs />);
    case "about":
      return wrap(<About />);
    default:
      return null;
  }
}
