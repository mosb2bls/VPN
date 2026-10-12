// Settings window (SPEC §2.8: 760 × 560, resizable, min 640 × 480). Left rail
// with the nine sections; content column on the right. Same paper and ink as
// the popover, no grid. Motion moment 7: the rail's ink block slides to the
// chosen section and the content cross-fades with a small upward shift.
import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { on } from "../api/client";
import type { SettingsSection } from "../api/bindings";
import { navTimeline as N } from "../design/motion";
import { protocolBadge } from "../lib/format";
import { derivePopoverView, TONE_CLASS } from "../popover/view";
import { useApp } from "../state/store";
import { useMotionOk } from "../ui/motionGate";
import { SettingsNav } from "./nav";
import { AboutSection } from "./sections/About";
import { ConnectionSection } from "./sections/Connection";
import { FailoverSection } from "./sections/Failover";
import { HelperSection } from "./sections/Helper";
import { LogsSection } from "./sections/Logs";
import { NodesSection } from "./sections/Nodes";
import { RulesSection } from "./sections/Rules";
import { SafetySection } from "./sections/Safety";
import { StartupSection } from "./sections/Startup";

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
            <SettingsNav.Provider value={setSection}>
              <Section id={section} />
            </SettingsNav.Provider>
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

function Section({ id }: { id: SettingsSection }) {
  switch (id) {
    case "nodes":
      return <NodesSection />;
    case "connection":
      return <ConnectionSection />;
    case "failover":
      return <FailoverSection />;
    case "safety":
      return <SafetySection />;
    case "rules":
      return <RulesSection />;
    case "helper":
      return <HelperSection />;
    case "startup":
      return <StartupSection />;
    case "logs":
      return <LogsSection />;
    case "about":
      return <AboutSection />;
  }
}
