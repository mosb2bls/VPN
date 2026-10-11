// App state fed by the backend: startup snapshot (SPEC §3: getStatus/listNodes
// first, then events) plus a short activity history for the "last event" line.
import { useSyncExternalStore } from "react";
import { commands, on } from "../api/client";
import type {
  AppError,
  HealthCheck,
  HelperStatus,
  Node,
  RecoveryReport,
  RestoreReport,
  Status,
  TrafficStats,
} from "../api/bindings";

export type ActivityTone = "info" | "warn" | "error";
export interface Activity {
  id: number;
  at: string;
  tone: ActivityTone;
  text: string;
  error?: AppError;
}

export interface AppState {
  ready: boolean;
  bootError: AppError | null;
  status: Status | null;
  /** `reason` of the latest statusChanged (progress text; display only). */
  statusReason: string | null;
  nodes: Node[];
  stats: TrafficStats | null;
  health: HealthCheck | null;
  helper: HelperStatus | null;
  recovery: RecoveryReport | null;
  /** Latest restore (SPEC §2.7). Incomplete ⇒ networking is NOT back to normal. */
  restore: RestoreReport | null;
  /** Newest first, max 50. Failover decisions, errors, recovery. */
  activity: Activity[];
}

let state: AppState = {
  ready: false,
  bootError: null,
  status: null,
  statusReason: null,
  nodes: [],
  stats: null,
  health: null,
  helper: null,
  recovery: null,
  restore: null,
  activity: [],
};

const subs = new Set<() => void>();
function set(patch: Partial<AppState>) {
  state = { ...state, ...patch };
  subs.forEach((s) => s());
}

let seq = 0;
function pushActivity(tone: ActivityTone, text: string, error?: AppError, at = new Date().toISOString()) {
  set({ activity: [{ id: ++seq, at, tone, text, error }, ...state.activity].slice(0, 50) });
}

const PROXYING = new Set(["connected", "degraded", "failing_over", "reconnecting"]);
const nodeName = (id: string | null | undefined) => state.nodes.find((n) => n.id === id)?.name ?? "unknown node";

/** Apply a Status returned by a command (events usually follow, harmless twice). */
export function applyStatus(status: Status, reason?: string | null) {
  const prev = state.status;
  set({
    status,
    restore: status.lastRestore,
    statusReason: reason === undefined ? state.statusReason : reason,
    stats: PROXYING.has(status.state) ? state.stats : null,
    health: PROXYING.has(status.state) ? state.health : null,
  });
  // Record entering `error` once per distinct error.
  if (status.state === "error" && status.lastError && (prev?.state !== "error" || prev.lastError?.code !== status.lastError.code)) {
    pushActivity("error", status.lastError.message, status.lastError);
  }
}

export function applyNodes(nodes: Node[]) {
  set({ nodes });
}

let started = false;
export async function startStore() {
  if (started) return;
  started = true;
  // Subscribe first so nothing is lost between the snapshot and the listeners.
  await Promise.all([
    on("statusChanged", (p) => applyStatus(p.status, p.reason)),
    // nodesChanged also carries the selection (import, delete, tray) — keep Status in step.
    on("nodesChanged", (p) =>
      set({ nodes: p.nodes, status: state.status ? { ...state.status, selectedNodeId: p.selectedNodeId } : state.status }),
    ),
    on("trafficStatsUpdated", (p) => set({ stats: p.stats })),
    on("healthCheck", (p) => set({ health: p })),
    on("helperStatusChanged", (p) => set({ helper: p.status })),
    on("failoverEvent", (p) => pushActivity(p.kind === "suppressed" ? "warn" : "info", p.reason, undefined, p.at)),
    on("activeNodeChanged", (p) => {
      // Only a live switch between nodes; connect/disconnect also emit this (null ↔ id).
      if (p.cause === "manual" && p.nodeId && p.previousNodeId) pushActivity("info", `Switched to ${nodeName(p.nodeId)}`);
    }),
    on("errorRaised", (p) => pushActivity(p.error.recoverable ? "warn" : "error", p.error.message, p.error)),
    on("recoveryCompleted", (p) => {
      set({ recovery: p.report });
      const text =
        p.report.outcome === "reattached"
          ? "Previous session re-attached"
          : p.report.outcome === "restored"
            ? "Network settings from the previous session were restored"
            : "Some network settings from the previous session could not be restored";
      pushActivity(p.report.outcome === "restore_incomplete" ? "error" : "info", text, undefined, p.report.at);
    }),
    on("restoreReportChanged", (p) => {
      set({ restore: p.report });
      if (!p.report.fullyRestored) {
        const bad = p.report.items.filter((i) => i.result === "failed" || i.result === "skipped");
        pushActivity("error", `Networking not fully restored: ${bad.map((i) => i.label).join(", ")}`, undefined, p.report.at);
      }
    }),
    // Popover shown from the tray (popover window only): refresh in case anything was missed.
    on("popoverShown", () => {
      void commands.getStatus().then((r) => r.status === "ok" && applyStatus(r.data));
    }),
    on("rulesUpdated", (p) => {
      if (!p.ok && p.status.lastError) pushActivity("warn", p.status.lastError.message, p.status.lastError);
    }),
  ]);

  const [status, nodes, helper] = await Promise.all([commands.getStatus(), commands.listNodes(), commands.getHelperStatus()]);
  const bootError =
    status.status === "error" ? status.error : nodes.status === "error" ? nodes.error : null;
  set({
    ready: true,
    bootError,
    status: status.status === "ok" ? status.data : state.status,
    nodes: nodes.status === "ok" ? nodes.data : state.nodes,
    helper: helper.status === "ok" ? helper.data : state.helper,
    recovery: status.status === "ok" ? status.data.lastRecovery : null,
    restore: status.status === "ok" ? status.data.lastRestore : null,
  });
}

export function useApp<T>(select: (s: AppState) => T): T {
  return useSyncExternalStore(
    (cb) => {
      subs.add(cb);
      return () => subs.delete(cb);
    },
    () => select(state),
  );
}

/**
 * True while networking is not back to normal after a restore (SPEC §2.7, §9.10).
 * Takes precedence over every other popover state except an engaged kill switch.
 */
export const selectNotRestored = (s: AppState): boolean => s.restore !== null && !s.restore.fullyRestored;

/** The most recent failover reason or error, for the popover's one-line slot. */
export const selectLastEvent = (s: AppState): Activity | null => s.activity[0] ?? null;
