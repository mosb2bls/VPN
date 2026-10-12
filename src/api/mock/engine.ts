// Mock backend: implements every SPEC command and event in memory, follows the
// state machine in SPEC §4 and failover rules in §5, and exposes dev controls
// to force any state, event or error code. Timings are shortened for design
// work; `dev.timeScale` speeds them up further.
import { SPEC_VERSION } from "../bindings";
import type {
  ActiveNodeChanged,
  AppError,
  AppInfo,
  AppSettings,
  ConnectionState,
  ErrorCode,
  FailoverSettings,
  HealthCheck,
  HelperStatus,
  ImportResult,
  LogEntry,
  LogLevel,
  LogSource,
  Node,
  NodeForm,
  NodeTestResult,
  RecoveryOutcome,
  RecoveryReport,
  RestoreItem,
  RestoreItemKind,
  RestoreReport,
  RestoreTrigger,
  RulesStatus,
  SafetySettings,
  Status,
  TrafficStats,
} from "../bindings";
import type {
  Backend,
  CommandName,
  Commands,
  EventHandler,
  EventMap,
  EventName,
  Result,
  Unlisten,
} from "../types";
import { ERROR_MESSAGES, appError } from "./errors";
import {
  DEFAULT_FAILOVER,
  DEFAULT_SAFETY,
  DEFAULT_SETTINGS,
  ID_HY2,
  defaultHelper,
  defaultNodes,
  defaultRules,
  manyNodes,
  mask,
  newId,
  oneNode,
  type NodeFixture,
} from "./fixtures";
import { parseLink } from "./links";
import { validateFailover, validateNodeForm, validateSettings } from "./validate";
import { APP_NAME } from "../../lib/brand";

// ───────────────────────────── dev configuration ─────────────────────────────

export type Outcome<E extends ErrorCode = ErrorCode> = "ok" | E;
export type NodeHealth = "ok" | "fail" | "flaky";

export interface DevConfig {
  /** Mimic the current backend (B1): stubbed commands return NOT_IMPLEMENTED. */
  stubMode: boolean;
  /** Divides every mock delay (1 = design speed, 4 = fast). */
  timeScale: number;
  /** Simulated IPC round-trip per command, ms. */
  ipcDelayMs: number;
  /** Run periodic health probes while connected (off = states stay frozen). */
  autoProbes: boolean;
  /** Per-node probe health used by connect, health rounds and tests. */
  nodeHealth: Record<string, NodeHealth>;
  /** Failure after `connecting` started (→ `error`, OS state rolled back). */
  connectOutcome: Outcome;
  reconnectOutcome: "recover" | "give_up";
  /** Items that fail (or are skipped) on the next restore; others restore cleanly. */
  restoreFailures: Partial<Record<RestoreItemKind, "failed" | "skipped">>;
  helperInstallOutcome: Outcome<"HELPER_INSTALL_CANCELLED" | "HELPER_INSTALL_FAILED">;
  rulesOutcome: "changed" | "unchanged" | "RULES_DOWNLOAD_FAILED" | "RULES_VALIDATION_FAILED";
  qrOutcome: Outcome<"QR_NOT_FOUND" | "QR_IMAGE_UNREADABLE" | "QR_TOO_MANY_CODES">;
  clipboard: { kind: "text"; text: string } | { kind: "empty" } | { kind: "unavailable" };
  /** One-shot forced errors per command, consumed on next call. */
  forced: Partial<Record<CommandName, ErrorCode>>;
}

const STUBBED: CommandName[] = [
  "importLinks",
  "importFromClipboard",
  "importQrImage",
  "addNodeManual",
  "updateNode",
  "testNode",
  "connect",
  "updateRulesNow",
  "installHelper",
  "uninstallHelper",
  "exportDiagnostics",
  "restoreNetworking",
];

export type ScenarioId =
  | "off"
  | "no_nodes"
  | "helper_missing"
  | "connecting"
  | "connected"
  | "connected_tun"
  | "degraded"
  | "failing_over"
  | "on_fallback"
  | "reconnecting"
  | "blocked"
  | "direct"
  | "disconnecting"
  | "recovering"
  | "error"
  | "restore_incomplete"
  | "restore_incomplete_proxy"
  | "unprotected"
  | "all_down";

export const SCENARIOS: { id: ScenarioId; label: string }[] = [
  { id: "off", label: "OFF" },
  { id: "no_nodes", label: "OFF · no nodes" },
  { id: "helper_missing", label: "HELPER MISSING" },
  { id: "connecting", label: "CONNECTING" },
  { id: "connected", label: "CONNECTED · proxy" },
  { id: "connected_tun", label: "CONNECTED · TUN" },
  { id: "degraded", label: "DEGRADED" },
  { id: "failing_over", label: "FAILING OVER" },
  { id: "on_fallback", label: "CONNECTED · on fallback" },
  { id: "reconnecting", label: "RECONNECTING · proxy" },
  { id: "blocked", label: "BLOCKED · kill switch" },
  { id: "direct", label: "DIRECT" },
  { id: "disconnecting", label: "DISCONNECTING" },
  { id: "recovering", label: "RECOVERING" },
  { id: "error", label: "ERROR" },
  { id: "restore_incomplete", label: "NOT RESTORED · TUN" },
  { id: "restore_incomplete_proxy", label: "NOT RESTORED · proxy" },
  { id: "unprotected", label: "UNPROTECTED · direct fallback" },
  { id: "all_down", label: "ALL NODES DOWN" },
];

// ─────────────────────────────── helpers ────────────────────────────────

const ok = <T>(data: T): Result<T> => ({ status: "ok", data });
const fail = <T>(error: AppError): Result<T> => ({ status: "error", error });
const nowIso = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");
const clone = <T>(v: T): T => structuredClone(v);
const rand = (lo: number, hi: number) => lo + Math.random() * (hi - lo);
const PROXYING: ConnectionState[] = ["connected", "degraded", "failing_over", "reconnecting"];
const LEVELS: LogLevel[] = ["trace", "debug", "info", "warn", "error"];
const RESTORE_ORDER: RestoreItemKind[] = ["tun_session", "routes", "dns", "ipv6", "system_proxy", "firewall"];
// Labels as the backend uses them (crates/core/src/restore.rs).
const RESTORE_LABELS: Record<RestoreItemKind, string> = {
  tun_session: "TUN session (helper)",
  routes: "Routes",
  dns: "DNS servers",
  ipv6: "IPv6 settings",
  system_proxy: "System proxy",
  firewall: "Firewall / kill switch",
};
const FAIL_CODE: Record<RestoreItemKind, ErrorCode> = {
  tun_session: "TUN_STOP_FAILED",
  routes: "ROUTE_RESTORE_FAILED",
  dns: "DNS_RESTORE_FAILED",
  ipv6: "IPV6_RESTORE_FAILED",
  system_proxy: "SYSTEM_PROXY_RESTORE_FAILED",
  firewall: "KILL_SWITCH_REMOVE_FAILED",
};
const RESTORED_TEXT: Record<RestoreItemKind, (win: boolean) => string> = {
  tun_session: () => "Stopped the TUN session and removed the interface",
  routes: () => "Removed TUN and server-bypass routes",
  dns: (w) => (w ? "Restored previous DNS servers on Wi-Fi" : "Restored previous DNS servers on Wi-Fi and Ethernet"),
  ipv6: () => "Restored previous IPv6 settings on Wi-Fi",
  system_proxy: (w) => (w ? "Restored previous WinINet proxy settings" : "Restored previous proxy settings on Wi-Fi and Ethernet"),
  firewall: (w) => (w ? "Removed WFP kill-switch filters" : "Removed the pf kill-switch anchor"),
};
const FAILED_TEXT: Record<RestoreItemKind, (win: boolean) => string> = {
  tun_session: () => "Tried to stop the TUN session; the interface is still up",
  routes: () => "Tried to remove TUN routes; 2 routes remain",
  dns: () => "Tried to restore DNS servers on Wi-Fi; write was refused",
  ipv6: () => "Tried to restore IPv6 on Wi-Fi; setting unchanged",
  system_proxy: () => `Tried to restore the system proxy; it still points to ${APP_NAME}`,
  firewall: (w) => (w ? "Tried to remove WFP filters; they are still active" : "Tried to remove the pf anchor; it is still loaded"),
};
const protoLabel = (n: Node | undefined) => (n?.protocol === "vless_reality" ? "REALITY" : "HY2");

function freshStatus(): Status {
  return {
    state: "disconnected",
    captureMode: "system_proxy",
    routingMode: "rule",
    dnsProfile: "mainland",
    selectedNodeId: ID_HY2,
    activeNodeId: null,
    connectedSince: null,
    killSwitchActive: false,
    systemProxySet: false,
    tunActive: false,
    failover: null,
    lastError: null,
    lastRecovery: null,
    lastRestore: null,
  };
}

// ─────────────────────────────── engine ────────────────────────────────

export class MockEngine {
  dev: DevConfig = {
    stubMode: false,
    timeScale: 1,
    ipcDelayMs: 120,
    autoProbes: true,
    nodeHealth: {},
    connectOutcome: "ok",
    reconnectOutcome: "recover",
    restoreFailures: {},
    helperInstallOutcome: "ok",
    rulesOutcome: "changed",
    qrOutcome: "ok",
    clipboard: { kind: "text", text: "" },
    forced: {},
  };

  private fixtures: NodeFixture[] = defaultNodes();
  private status: Status = freshStatus();
  private failoverSettings: FailoverSettings = clone(DEFAULT_FAILOVER);
  private safety: SafetySettings = clone(DEFAULT_SAFETY);
  private settings: AppSettings = clone(DEFAULT_SETTINGS);
  private rules: RulesStatus = defaultRules();
  private helper: HelperStatus = defaultHelper();
  private logs: LogEntry[] = [];
  private testing = new Set<string>();
  private testSlots = 0;
  private testQueue: (() => void)[] = [];

  /** Dev: `?platform=windows` on the dev page simulates Windows (no TUN / helper). */
  private platformOverride: AppInfo["platform"] | null = (() => {
    const p = typeof location !== "undefined" ? new URLSearchParams(location.search).get("platform") : null;
    return p === "windows" || p === "macos" ? p : null;
  })();

  /** Bumped to cancel any running state sequence. */
  private gen = 0;
  private statsTimer: number | null = null;
  private healthTimer: number | null = null;
  private stats: TrafficStats = this.zeroStats();
  private failedRounds = 0;
  private lastSwitchAt = 0;
  private switchTimes: number[] = [];
  private primaryOkSince: number | null = null;

  private listeners = new Map<EventName, Set<(p: unknown) => void>>();
  private anyListeners = new Set<(name: string, payload: unknown) => void>();
  private devListeners = new Set<() => void>();
  /** Path of the latest diagnostics export (reveal_diagnostics reveals only this). */
  private lastExport: string | null = null;
  onQuit: (() => void) | null = null;

  constructor() {
    this.dev.clipboard = { kind: "text", text: "" };
    this.log("info", "app", `${APP_NAME} started (mock backend)`);
  }

  // ── events ──

  on<K extends EventName>(name: K, cb: EventHandler<K>): Unlisten {
    let set = this.listeners.get(name);
    if (!set) this.listeners.set(name, (set = new Set()));
    const h = cb as (p: unknown) => void;
    set.add(h);
    return () => set!.delete(h);
  }

  /** Dev page event monitor (also shows window commands as pseudo-events). */
  onAny(cb: (name: string, payload: unknown) => void): Unlisten {
    this.anyListeners.add(cb);
    return () => this.anyListeners.delete(cb);
  }

  private emit<K extends EventName>(name: K, payload: EventMap[K]) {
    const p = clone(payload);
    // Async like real IPC events.
    queueMicrotask(() => {
      this.listeners.get(name)?.forEach((h) => h(p));
      this.anyListeners.forEach((h) => h(name, p));
    });
  }

  subscribeDev(cb: () => void): Unlisten {
    this.devListeners.add(cb);
    return () => this.devListeners.delete(cb);
  }

  setDev(patch: Partial<DevConfig>) {
    this.dev = { ...this.dev, ...patch };
    this.devListeners.forEach((h) => h());
  }

  // ── internals ──

  private get nodes(): Node[] {
    return this.fixtures.map((f) => f.node);
  }
  private node(id: string | null | undefined): Node | undefined {
    return this.fixtures.find((f) => f.node.id === id)?.node;
  }
  private health(id: string | null | undefined): NodeHealth {
    return (id && this.dev.nodeHealth[id]) || "ok";
  }
  private sleep(ms: number) {
    return new Promise<void>((r) => setTimeout(r, ms / Math.max(this.dev.timeScale, 0.1)));
  }

  private log(level: LogLevel, source: LogSource, message: string) {
    this.logs.push({ ts: nowIso(), level, source, message });
    if (this.logs.length > 2000) this.logs.splice(0, this.logs.length - 2000);
  }

  private setStatus(patch: Partial<Status>, reason: string | null = null) {
    const prev = this.status.state;
    this.status = { ...this.status, ...patch };
    if (patch.state && patch.state !== prev) this.log("info", "app", `State ${prev} → ${patch.state}${reason ? ` (${reason})` : ""}`);
    this.emit("statusChanged", { status: this.status, reason });
  }

  private emitNodes() {
    this.emit("nodesChanged", { nodes: this.nodes, selectedNodeId: this.status.selectedNodeId });
  }

  private setActive(nodeId: string | null, cause: ActiveNodeChanged["cause"]) {
    const previousNodeId = this.status.activeNodeId;
    if (previousNodeId === nodeId) return;
    this.status = { ...this.status, activeNodeId: nodeId };
    this.emit("activeNodeChanged", { previousNodeId, nodeId, cause });
  }

  private zeroStats(): TrafficStats {
    return { upBytesPerSec: 0, downBytesPerSec: 0, upBytesTotal: 0, downBytesTotal: 0, activeConnections: 0, latencyMs: null };
  }

  private startTimers() {
    this.stopTimers();
    this.stats = this.zeroStats();
    this.failedRounds = 0;
    const scale = () => Math.max(this.dev.timeScale, 0.1);
    this.statsTimer = window.setInterval(() => this.tickStats(), 1000 / scale());
    // Mock probes run every 3 s (scaled) rather than probeIntervalSecs so
    // failover can be watched without waiting.
    this.healthTimer = window.setInterval(() => this.dev.autoProbes && this.healthRound(), 3000 / scale());
  }

  private stopTimers() {
    if (this.statsTimer !== null) clearInterval(this.statsTimer);
    if (this.healthTimer !== null) clearInterval(this.healthTimer);
    this.statsTimer = this.healthTimer = null;
  }

  private tickStats() {
    const st = this.status.state;
    if (!PROXYING.includes(st)) return;
    const blocked = st === "reconnecting" || st === "failing_over";
    const walk = (v: number | null, lo: number, hi: number) =>
      blocked ? 0 : Math.round(Math.min(hi, Math.max(lo, (v || (lo + hi) / 2) * rand(0.75, 1.3))));
    const up = walk(this.stats.upBytesPerSec, 4_000, 600_000);
    const down = walk(this.stats.downBytesPerSec, 30_000, 9_000_000);
    this.stats = {
      upBytesPerSec: up,
      downBytesPerSec: down,
      upBytesTotal: (this.stats.upBytesTotal ?? 0) + up,
      downBytesTotal: (this.stats.downBytesTotal ?? 0) + down,
      activeConnections: blocked ? 0 : Math.round(rand(6, 42)),
      latencyMs: this.stats.latencyMs,
    };
    this.emit("trafficStatsUpdated", { stats: this.stats });
  }

  private probeRound(nodeId: string): HealthCheck {
    const h = this.health(nodeId);
    const base = this.node(nodeId)?.protocol === "vless_reality" ? 110 : 45;
    const probes = this.failoverSettings.probeTargets.map((url) => {
      const pass = h === "ok" || (h === "flaky" && Math.random() < 0.5);
      let target = url;
      try {
        target = new URL(url).hostname.split(".").slice(-2, -1)[0] ?? url;
      } catch {
        /* keep url */
      }
      return { target, ok: pass, latencyMs: pass ? Math.round(base * rand(0.8, 1.4)) : null };
    });
    const roundOk = probes.some((p) => p.ok);
    return { nodeId, roundOk, probes, consecutiveFailedRounds: 0, at: nowIso() };
  }

  private healthRound() {
    if (this.status.failover?.direct && this.status.state === "degraded") return this.directRound();
    const st = this.status.state;
    const active = this.status.activeNodeId;
    if (!active || (st !== "connected" && st !== "degraded")) return;
    const round = this.probeRound(active);
    this.failedRounds = round.roundOk ? 0 : this.failedRounds + 1;
    if (round.roundOk) this.suppressedThisStreak = false; // a new failure streak may report again
    round.consecutiveFailedRounds = this.failedRounds;
    this.emit("healthCheck", round);

    if (round.roundOk) {
      const lat = round.probes.filter((p) => p.ok).map((p) => p.latencyMs!).sort((a, b) => a - b);
      this.stats = { ...this.stats, latencyMs: lat[Math.floor(lat.length / 2)] };
      if (st === "degraded") this.setStatus({ state: "connected", lastError: null }, "Health check passed");
    } else {
      this.stats = { ...this.stats, latencyMs: null };
      if (st === "connected")
        this.setStatus(
          { state: "degraded", lastError: appError("PROBE_FAILED", { detail: "all probe targets failed" }) },
          `Health check failed (${this.failedRounds}/${this.failoverSettings.failureRounds})`,
        );
      else this.setStatus({}, `Health check failed (${this.failedRounds}/${this.failoverSettings.failureRounds})`);
      const n = this.failoverSettings.failureRounds;
      // SPEC 1.3.3: switching is retried every failed round past the threshold (e.g. once the
      // dwell time has passed), but "suppressed" is reported only once per failure streak.
      if (this.failoverSettings.enabled && this.failedRounds >= n) void this.tryFailover();
    }

    // Background check of the primary while on the fallback.
    const fo = this.status.failover;
    if (fo && this.status.state === "connected") {
      if (this.health(fo.primaryNodeId) === "ok") {
        this.primaryOkSince ??= Date.now();
        const stableMs = (this.failoverSettings.returnStableSecs * 1000) / Math.max(this.dev.timeScale, 0.1);
        if (Date.now() - this.primaryOkSince >= stableMs && this.dwellElapsed()) void this.failback();
      } else this.primaryOkSince = null;
    }
  }

  private dwellElapsed() {
    return Date.now() - this.lastSwitchAt >= this.failoverSettings.minDwellSecs * 1000 / Math.max(this.dev.timeScale, 0.1);
  }

  private resolveFallback(fromId: string): Node | undefined {
    if (this.status.failover) return undefined; // already on the fallback
    const from = this.node(fromId);
    const fixed = this.node(this.failoverSettings.fallbackNodeId);
    if (fixed && fixed.id !== fromId) return fixed;
    return this.nodes.find((n) => n.protocol !== from?.protocol);
  }

  private failoverReason(from: Node | undefined) {
    const n = this.failedRounds;
    return from?.protocol === "vless_reality"
      ? `REALITY: ${n} consecutive probe failures — server unreachable over TCP 443`
      : `HY2: ${n} consecutive probe failures — UDP may be blocked on this network`;
  }

  /** "suppressed" already reported in the current failure streak (SPEC 1.3.3). */
  private suppressedThisStreak = false;

  private suppress(fromId: string, reason: string) {
    if (this.suppressedThisStreak) return;
    this.suppressedThisStreak = true;
    this.log("warn", "app", `Failover suppressed: ${reason}`);
    this.emit("failoverEvent", { kind: "suppressed", fromNodeId: fromId, toNodeId: null, reason, at: nowIso() });
  }

  /** Every node is failing: go direct (direct fallback on) or stay blocked and report it. */
  private allNodesFailing(fromId: string) {
    const reasonBlocked = "All nodes failing — traffic stays blocked until a node recovers";
    if (!this.safety.directFallback) {
      this.suppress(fromId, reasonBlocked);
      if (this.status.lastError?.code !== "ALL_NODES_FAILED")
        this.setStatus({ state: "degraded", lastError: appError("ALL_NODES_FAILED", { recoverable: false }) }, reasonBlocked);
      return;
    }
    if (this.status.failover?.direct) return;
    ++this.gen;
    const reason = "All nodes failing — direct fallback: traffic is going out directly, unprotected";
    const primary = this.status.failover?.primaryNodeId ?? fromId;
    this.failedRounds = 0;
    this.suppressedThisStreak = false;
    this.setActive(null, "failover");
    this.emit("failoverEvent", { kind: "switched_to_direct", fromNodeId: fromId, toNodeId: null, reason, at: nowIso() });
    this.log("warn", "app", reason);
    this.stats = { ...this.stats, latencyMs: null };
    this.setStatus(
      {
        state: "degraded",
        lastError: appError("DIRECT_FALLBACK_ACTIVE"),
        failover: { primaryNodeId: primary, fallbackNodeId: "", since: nowIso(), reason, earliestReturnAt: null, direct: true },
      },
      reason,
    );
  }

  /** While direct: test the primary, then the fallback; leave direct as soon as one passes. */
  private directRound() {
    const fo = this.status.failover!;
    const primary = this.node(fo.primaryNodeId);
    const other = this.nodes.find((n) => n.protocol !== primary?.protocol);
    const pick = primary && this.health(primary.id) !== "fail" ? primary : other && this.health(other.id) !== "fail" ? other : null;
    this.emit("healthCheck", { ...this.probeRound(primary?.id ?? ""), consecutiveFailedRounds: 0 });
    if (!pick) return;
    const back = pick.id === fo.primaryNodeId;
    const reason = `${protoLabel(pick)} is reachable again — leaving direct fallback`;
    this.setActive(pick.id, back ? "failback" : "failover");
    this.emit("failoverEvent", {
      kind: back ? "returned_to_primary" : "switched_to_fallback",
      fromNodeId: fo.primaryNodeId,
      toNodeId: pick.id,
      reason,
      at: nowIso(),
    });
    this.setStatus(
      {
        state: "connected",
        lastError: null,
        failover: back ? null : { primaryNodeId: fo.primaryNodeId, fallbackNodeId: pick.id, since: nowIso(), reason, earliestReturnAt: null, direct: false },
      },
      reason,
    );
  }

  async tryFailover(force = false) {
    const fromId = this.status.activeNodeId;
    if (!fromId) return;
    const from = this.node(fromId);
    const to = this.resolveFallback(fromId);
    const hourAgo = Date.now() - 3_600_000;
    this.switchTimes = this.switchTimes.filter((t) => t > hourAgo);
    // SPEC 1.4.0 §5: all nodes failing = no other node, or its background probe fails too.
    if (!force && (!to || this.health(to.id) === "fail")) return this.allNodesFailing(fromId);
    if (!to) return this.suppress(fromId, `${protoLabel(from)} failing and no fallback node is available`);
    if (!force && !this.dwellElapsed())
      return this.suppress(fromId, `Minimum time on node (${Math.round(this.failoverSettings.minDwellSecs / 60)} min) not elapsed`);
    if (!force && this.switchTimes.length >= this.failoverSettings.maxSwitchesPerHour)
      return this.suppress(fromId, `Switch limit reached (${this.failoverSettings.maxSwitchesPerHour} per hour)`);

    const reason = force ? `${protoLabel(from)}: switched by dev panel` : this.failoverReason(from);
    const gen = ++this.gen;
    this.setStatus({ state: "failing_over" }, `Switching to ${to.name}`);
    await this.sleep(1100);
    if (gen !== this.gen) return;
    this.lastSwitchAt = Date.now();
    this.switchTimes.push(this.lastSwitchAt);
    this.failedRounds = 0;
    this.suppressedThisStreak = false;
    this.primaryOkSince = null;
    const earliest = new Date(
      Date.now() + Math.max(this.failoverSettings.minDwellSecs, this.failoverSettings.returnStableSecs) * 1000,
    );
    this.setActive(to.id, "failover");
    this.emit("failoverEvent", { kind: "switched_to_fallback", fromNodeId: fromId, toNodeId: to.id, reason, at: nowIso() });
    this.log("warn", "app", `Failover: ${from?.name} → ${to.name}: ${reason}`);
    const healthy = this.health(to.id) !== "fail";
    this.setStatus(
      {
        state: healthy ? "connected" : "degraded",
        lastError: healthy ? null : appError("PROBE_FAILED"),
        failover: {
          primaryNodeId: fromId,
          fallbackNodeId: to.id,
          since: nowIso(),
          reason,
          earliestReturnAt: earliest.toISOString().replace(/\.\d+Z$/, "Z"),
          direct: false,
        },
      },
      reason,
    );
  }

  async failback() {
    const fo = this.status.failover;
    if (!fo) return;
    const primary = this.node(fo.primaryNodeId);
    const gen = ++this.gen;
    this.setStatus({ state: "failing_over" }, `Returning to ${primary?.name ?? "primary"}`);
    await this.sleep(1100);
    if (gen !== this.gen) return;
    const reason = `${protoLabel(primary)} passed health checks for ${Math.round(this.failoverSettings.returnStableSecs / 60)} min — returning to primary`;
    this.lastSwitchAt = Date.now();
    this.switchTimes.push(this.lastSwitchAt);
    this.primaryOkSince = null;
    this.failedRounds = 0;
    this.suppressedThisStreak = false;
    this.setActive(fo.primaryNodeId, "failback");
    this.emit("failoverEvent", {
      kind: "returned_to_primary",
      fromNodeId: fo.fallbackNodeId,
      toNodeId: fo.primaryNodeId,
      reason,
      at: nowIso(),
    });
    this.setStatus({ state: "connected", failover: null, lastError: null }, reason);
  }

  /** OS items this app has changed and not yet restored (the mock's journal). */
  private held = new Set<RestoreItemKind>();

  /** Status flags from what is actually held. `engaged`: core is down, so a held firewall blocks. */
  private flagsFromHeld(engaged: boolean): Partial<Status> {
    return {
      systemProxySet: this.held.has("system_proxy"),
      tunActive: this.held.has("tun_session"),
      killSwitchActive: engaged && this.held.has("firewall"),
    };
  }

  private holdFor(capture: Status["captureMode"]) {
    if (capture === "tun") {
      for (const k of ["tun_session", "routes", "dns", "ipv6"] as const) this.held.add(k);
      if (this.safety.killSwitch) this.held.add("firewall");
    } else this.held.add("system_proxy");
  }

  private osHeld(): Partial<Status> {
    this.holdFor(this.status.captureMode);
    return this.flagsFromHeld(false);
  }

  /** Clean release (re-attach preamble, direct-fallback give-up): nothing left changed. */
  private osReleased(): Partial<Status> {
    this.held.clear();
    return { ...this.flagsFromHeld(false), activeNodeId: null, connectedSince: null, failover: null };
  }

  /** connecting → connected/degraded/error. Returns final result. */
  private async runConnect(cause: ActiveNodeChanged["cause"] = "manual"): Promise<Result<Status>> {
    const gen = ++this.gen;
    const tun = this.status.captureMode === "tun";
    const node = this.node(this.status.selectedNodeId)!;
    const steps = [
      "Resolving server address",
      "Starting proxy core",
      tun ? "Starting TUN" : "Setting system proxy",
      ...(tun && this.safety.killSwitch ? ["Arming kill switch"] : []),
      "Running health check",
    ];
    const before = new Set(this.held);
    const failAt = this.dev.connectOutcome === "ok" ? -1 : Math.min(steps.length - 1, 1 + Math.floor(Math.random() * 2));
    this.setStatus({ state: "connecting", lastError: null, failover: null }, steps[0]);
    for (let i = 0; i < steps.length; i++) {
      if (i > 0) this.setStatus({}, steps[i]);
      await this.sleep(i === steps.length - 1 ? 900 : 550);
      if (gen !== this.gen) return ok(clone(this.status));
      if (i === failAt) {
        const e = appError(this.dev.connectOutcome as ErrorCode, { detail: `mock: failed at "${steps[i]}"` });
        this.stopTimers();
        this.held = before; // the start attempt is rolled back; earlier leftovers stay
        this.setStatus(
          { state: "error", lastError: e, ...this.flagsFromHeld(true), activeNodeId: null, connectedSince: null, failover: null },
          "Connection failed — network settings rolled back",
        );
        this.log("error", "app", `Connect failed: ${e.code}`);
        return fail(e);
      }
    }
    this.failedRounds = 0;
    this.setActive(node.id, cause);
    const healthy = this.health(node.id) !== "fail";
    this.startTimers();
    this.setStatus(
      {
        state: healthy ? "connected" : "degraded",
        connectedSince: nowIso(),
        lastError: healthy ? null : appError("PROBE_FAILED"),
        ...this.osHeld(),
      },
      healthy ? "Connected" : "Connected, but the first health check failed",
    );
    this.log("info", "core", `outbound/${node.protocol === "hysteria2" ? "hysteria2" : "vless"}[${node.name}]: ready`);
    if (healthy) {
      const r = this.probeRound(node.id);
      const lat = r.probes.filter((p) => p.ok).map((p) => p.latencyMs!);
      this.stats = { ...this.stats, latencyMs: lat.length ? Math.min(...lat) : null };
    }
    return ok(clone(this.status));
  }

  /** proxying → disconnecting → disconnected / direct, or `error` if the restore is incomplete. */
  private async runDisconnect(
    trigger: RestoreTrigger = "disconnect",
    target: "disconnected" | "direct" = "disconnected",
  ): Promise<Result<Status>> {
    const gen = ++this.gen;
    const tun = this.held.has("tun_session");
    const steps = ["Stopping proxy core", tun ? "Removing TUN and kill switch" : "Restoring system proxy", "Restoring network settings"];
    this.stopTimers();
    for (let i = 0; i < steps.length; i++) {
      this.setStatus(i === 0 ? { state: "disconnecting" } : {}, steps[i]);
      await this.sleep(450);
      if (gen !== this.gen) return ok(clone(this.status));
    }
    const report = this.buildRestoreReport(trigger);
    this.setActive(null, "manual");
    const e = this.applyRestore(report);
    if (e) return fail(e);
    this.setStatus(
      {
        state: target,
        lastError: null,
        ...this.flagsFromHeld(false),
        activeNodeId: null,
        failover: null,
        connectedSince: target === "direct" ? nowIso() : null,
      },
      null,
    );
    return ok(clone(this.status));
  }

  /** Run the six-item restore (SPEC §2.7) against what is held. */
  private buildRestoreReport(trigger: RestoreTrigger, failures = this.dev.restoreFailures): RestoreReport {
    const win = this.appInfo().platform === "windows";
    const items: RestoreItem[] = RESTORE_ORDER.map((kind) => {
      const label = RESTORE_LABELS[kind];
      if (!this.held.has(kind))
        return { kind, label, action: `Not changed by ${APP_NAME}`, result: "not_needed", errorCode: null, errorMessage: null };
      const f = failures[kind];
      if (!f) {
        this.held.delete(kind);
        return { kind, label, action: RESTORED_TEXT[kind](win), result: "restored", errorCode: null, errorMessage: null };
      }
      const code: ErrorCode = f === "skipped" ? "HELPER_UNREACHABLE" : FAIL_CODE[kind];
      return {
        kind,
        label,
        action: f === "skipped" ? "Not attempted — the privileged helper is not responding" : FAILED_TEXT[kind](win),
        result: f,
        errorCode: code,
        errorMessage: ERROR_MESSAGES[code],
      };
    });
    const clean = (r: RestoreItem["result"]) => r === "restored" || r === "not_needed";
    return {
      at: nowIso(),
      trigger,
      fullyRestored: items.every((i) => clean(i.result)),
      items,
    };
  }

  /** Store + announce a report. Incomplete → state `error` with true flags; returns that error. */
  private applyRestore(report: RestoreReport): AppError | null {
    this.status = { ...this.status, lastRestore: report };
    this.emit("restoreReportChanged", { report });
    const bad = report.items.filter((i) => i.result === "failed" || i.result === "skipped");
    this.log(bad.length ? "error" : "info", "app", bad.length ? `Restore incomplete: ${bad.map((i) => i.label).join(", ")}` : "Restore complete");
    if (report.fullyRestored) return null;
    const e = appError("RESTORE_INCOMPLETE", { recoverable: false, restoreReport: report });
    this.setStatus(
      { state: "error", lastError: e, ...this.flagsFromHeld(true), activeNodeId: null, connectedSince: null, failover: null },
      "Networking is not fully restored",
    );
    return e;
  }

  // ── command plumbing ──

  private async call<T>(name: CommandName, fn: () => Promise<Result<T>> | Result<T>): Promise<Result<T>> {
    await new Promise((r) => setTimeout(r, this.dev.ipcDelayMs));
    const forced = this.dev.forced[name];
    if (forced) {
      const { [name]: _, ...rest } = this.dev.forced;
      this.setDev({ forced: rest });
      this.log("warn", "app", `${name}: forced ${forced} (dev panel)`);
      return fail(appError(forced, { detail: `mock: forced from dev panel` }));
    }
    if (this.dev.stubMode && STUBBED.includes(name)) {
      return fail(appError("NOT_IMPLEMENTED", { detail: `${name}: not implemented (stub)` }));
    }
    return fn();
  }

  private importParsed(lines: string[]): ImportResult {
    const added: Node[] = [];
    const errors: ImportResult["errors"] = [];
    lines.forEach((raw, i) => {
      const line = raw.trim();
      if (!line) return;
      const p = parseLink(line);
      if (!p.ok) {
        errors.push({ line: i + 1, scheme: p.scheme, error: p.error });
        return;
      }
      const dup = this.fixtures.some(
        (f) =>
          f.node.protocol === p.node.protocol &&
          f.node.server === p.node.server &&
          f.node.port === p.node.port &&
          f.credential === p.credential,
      );
      if (dup) {
        errors.push({ line: i + 1, scheme: p.node.protocol === "hysteria2" ? "hy2" : "vless", error: appError("NODE_DUPLICATE") });
        return;
      }
      this.fixtures.push({ node: p.node, credential: p.credential });
      added.push(p.node);
    });
    if (added.length) {
      // SPEC 1.1.1: a selection change from import also emits statusChanged.
      if (!this.status.selectedNodeId) this.setStatus({ selectedNodeId: added[0].id });
      this.emitNodes();
      this.log("info", "app", `Imported ${added.length} node(s)`);
    }
    return { added, errors };
  }

  private nodeFromForm(id: string, form: NodeForm, prev?: NodeFixture): NodeFixture {
    const keepOr = (s: { kind: string; value?: string }, prevMasked: string | null | undefined) =>
      s.kind === "set" ? mask(s.value ?? "") : s.kind === "clear" ? null : (prevMasked ?? null);
    if (form.protocol === "hysteria2") {
      const pd = prev?.node.details.protocol === "hysteria2" ? prev.node.details : undefined;
      return {
        credential: form.password.kind === "set" ? form.password.value : (prev?.credential ?? ""),
        node: {
          id,
          name: form.name.trim(),
          protocol: "hysteria2",
          server: form.server.trim(),
          port: form.port,
          lastTest: prev?.node.lastTest ?? null,
          // Re-entering the credential fixes a missing keychain entry.
          secretMissing: form.password.kind === "set" ? false : (prev?.node.secretMissing ?? false),
          details: {
            protocol: "hysteria2",
            passwordMasked: keepOr(form.password, pd?.passwordMasked) ?? "••••",
            serverPorts: form.serverPorts,
            sni: form.sni,
            insecure: form.insecure,
            obfsPasswordMasked: keepOr(form.obfsPassword, pd?.obfsPasswordMasked),
            pinSha256: form.pinSha256,
            upMbps: form.upMbps,
            downMbps: form.downMbps,
          },
        },
      };
    }
    const pd = prev?.node.details.protocol === "vless_reality" ? prev.node.details : undefined;
    return {
      credential: form.uuid.kind === "set" ? form.uuid.value : (prev?.credential ?? ""),
      node: {
        id,
        name: form.name.trim(),
        protocol: "vless_reality",
        server: form.server.trim(),
        port: form.port,
        lastTest: prev?.node.lastTest ?? null,
        secretMissing:
          form.uuid.kind === "set" && form.publicKey.kind === "set" && form.shortId.kind === "set"
            ? false
            : (prev?.node.secretMissing ?? false),
        details: {
          protocol: "vless_reality",
          uuidMasked: keepOr(form.uuid, pd?.uuidMasked) ?? "••••",
          flow: "xtls-rprx-vision",
          sni: form.sni.trim(),
          fingerprint: form.fingerprint,
          publicKeyMasked: keepOr(form.publicKey, pd?.publicKeyMasked) ?? "••••",
          shortIdMasked: form.shortId.kind === "set" ? (form.shortId.value ? "••••" : "") : (pd?.shortIdMasked ?? "••••"),
          spiderX: null,
        },
      },
    };
  }

  private async acquireTestSlot() {
    if (this.testSlots < 3) {
      this.testSlots++;
      return;
    }
    await new Promise<void>((r) => this.testQueue.push(r));
    this.testSlots++;
  }
  private releaseTestSlot() {
    this.testSlots--;
    this.testQueue.shift()?.();
  }

  private isBusyState() {
    return ["connecting", "disconnecting", "recovering"].includes(this.status.state);
  }

  // ── commands (SPEC §2) ──

  commands: Commands = {
    listNodes: () => this.call("listNodes", () => ok(clone(this.nodes))),

    importLinks: (text: string) =>
      this.call("importLinks", async () => {
        const lines = text.split(/\r?\n/);
        if (!lines.some((l) => l.trim())) return fail(appError("IMPORT_EMPTY"));
        await this.sleep(250);
        return ok(this.importParsed(lines));
      }),

    importFromClipboard: () =>
      this.call("importFromClipboard", async () => {
        const c = this.dev.clipboard;
        if (c.kind === "unavailable") return fail(appError("CLIPBOARD_UNAVAILABLE"));
        if (c.kind === "empty") return fail(appError("CLIPBOARD_EMPTY"));
        const lines = c.text.split(/\r?\n/);
        if (!lines.some((l) => l.trim())) return fail(appError("IMPORT_EMPTY"));
        await this.sleep(200);
        return ok(this.importParsed(lines));
      }),

    importQrImage: (source) =>
      this.call("importQrImage", async () => {
        // SPEC 1.1.1: path must be an absolute regular file; bytes may be base64 or a data: URL.
        if (source.kind === "path" && !/^(\/|[A-Za-z]:\\)/.test(source.path)) return fail(appError("QR_IMAGE_UNREADABLE"));
        await this.sleep(1400); // decoding can take a few seconds — the UI shows a busy state
        if (this.dev.qrOutcome !== "ok") return fail(appError(this.dev.qrOutcome, { detail: `source: ${source.kind}` }));
        const n = this.fixtures.length + 1;
        return ok(
          this.importParsed([`hy2://mock-qr-password-${n}@qr.example.net:24443?sni=qr.example.net#QR import ${n}`]),
        );
      }),

    addNodeManual: (form) =>
      this.call("addNodeManual", async () => {
        await this.sleep(200);
        const fields = validateNodeForm(form, "add");
        if (fields.length) return fail(appError("VALIDATION_FAILED", { fields }));
        const fx = this.nodeFromForm(newId(), form);
        if (
          this.fixtures.some(
            (f) =>
              f.node.protocol === fx.node.protocol &&
              f.node.server === fx.node.server &&
              f.node.port === fx.node.port &&
              f.credential === fx.credential,
          )
        )
          return fail(appError("NODE_DUPLICATE"));
        this.fixtures.push(fx);
        if (!this.status.selectedNodeId) this.setStatus({ selectedNodeId: fx.node.id });
        this.emitNodes();
        return ok(clone(fx.node));
      }),

    updateNode: (id, form) =>
      this.call("updateNode", async () => {
        await this.sleep(200);
        const idx = this.fixtures.findIndex((f) => f.node.id === id);
        if (idx < 0) return fail(appError("NODE_NOT_FOUND"));
        const fields = validateNodeForm(form, "update");
        if (fields.length) return fail(appError("VALIDATION_FAILED", { fields }));
        this.fixtures[idx] = this.nodeFromForm(id, form, this.fixtures[idx]);
        this.emitNodes();
        return ok(clone(this.fixtures[idx].node));
      }),

    deleteNode: (id) =>
      this.call("deleteNode", () => {
        const idx = this.fixtures.findIndex((f) => f.node.id === id);
        if (idx < 0) return fail(appError("NODE_NOT_FOUND"));
        if (id === this.status.activeNodeId) return fail(appError("BUSY", { detail: "node is carrying traffic; disconnect first" }));
        this.fixtures.splice(idx, 1);
        if (this.failoverSettings.fallbackNodeId === id) this.failoverSettings.fallbackNodeId = null;
        if (this.status.selectedNodeId === id) {
          this.setStatus({ selectedNodeId: this.fixtures[0]?.node.id ?? null });
        }
        this.emitNodes();
        return ok(null);
      }),

    reorderNodes: (ids) =>
      this.call("reorderNodes", () => {
        const cur = new Set(this.fixtures.map((f) => f.node.id));
        if (ids.length !== cur.size || new Set(ids).size !== ids.length || !ids.every((i) => cur.has(i)))
          return fail(appError("REORDER_MISMATCH"));
        this.fixtures = ids.map((i) => this.fixtures.find((f) => f.node.id === i)!);
        this.emitNodes();
        return ok(clone(this.nodes));
      }),

    selectNode: (id) =>
      this.call("selectNode", () => {
        if (!this.node(id)) return fail(appError("NODE_NOT_FOUND"));
        const proxying = PROXYING.includes(this.status.state);
        this.status = { ...this.status, selectedNodeId: id };
        if (proxying) {
          ++this.gen; // manual selection wins over a running failover
          this.failedRounds = 0;
          this.primaryOkSince = null;
          this.setActive(id, "manual");
          const healthy = this.health(id) !== "fail";
          this.setStatus(
            { failover: null, state: healthy ? "connected" : "degraded", lastError: healthy ? null : appError("PROBE_FAILED") },
            `Switched to ${this.node(id)!.name}`,
          );
        } else this.setStatus({});
        this.emitNodes();
        return ok(clone(this.status));
      }),

    testNode: (id) =>
      this.call("testNode", async () => {
        const fx = this.fixtures.find((f) => f.node.id === id);
        if (!fx) return fail(appError("NODE_NOT_FOUND"));
        if (this.testing.has(id)) return fail(appError("BUSY", { detail: "node is already being tested" }));
        if (fx.node.secretMissing) return fail(appError("SECRET_MISSING"));
        this.testing.add(id);
        await this.acquireTestSlot();
        try {
          await this.sleep(rand(900, 2600));
          const h = this.health(id);
          const pass = h === "ok" || (h === "flaky" && Math.random() < 0.5);
          const base = fx.node.protocol === "vless_reality" ? 110 : 45;
          const result: NodeTestResult = {
            nodeId: id,
            ok: pass,
            latencyMs: pass ? Math.round(base * rand(0.85, 1.5)) : null,
            exitCountry: pass ? "GB" : null,
            error: pass
              ? null
              : appError(fx.node.protocol === "hysteria2" ? "PROBE_TIMEOUT" : "NODE_HANDSHAKE_FAILED", {
                  detail: "mock: node health set to fail",
                }),
            testedAt: nowIso(),
          };
          const live = this.fixtures.find((f) => f.node.id === id);
          if (live) {
            live.node = { ...live.node, lastTest: result };
            this.emitNodes();
          }
          return ok(result);
        } finally {
          this.testing.delete(id);
          this.releaseTestSlot();
        }
      }),

    connect: () =>
      this.call("connect", async () => {
        const st = this.status.state;
        if (PROXYING.includes(st) || st === "direct") return ok(clone(this.status));
        if (this.isBusyState()) return fail(appError("BUSY"));
        if (!this.fixtures.length) return fail(appError("NO_NODES"));
        if (!this.node(this.status.selectedNodeId)) return fail(appError("NO_NODE_SELECTED"));
        if (this.node(this.status.selectedNodeId)!.secretMissing) return fail(appError("SECRET_MISSING"));
        if (this.status.routingMode === "direct") {
          this.setStatus({ state: "direct", lastError: null, connectedSince: nowIso() }, "Direct mode — no proxy");
          return ok(clone(this.status));
        }
        if (this.status.captureMode === "tun") {
          if (!this.helper.installed) return fail(appError("HELPER_NOT_INSTALLED"));
          if (this.helper.needsUpgrade) return fail(appError("HELPER_VERSION_MISMATCH"));
          if (!this.helper.running) return fail(appError("HELPER_UNREACHABLE"));
        }
        return this.runConnect();
      }),

    disconnect: () =>
      this.call("disconnect", async () => {
        const st = this.status.state;
        if (st === "disconnected") return ok(clone(this.status));
        if (st === "direct") {
          this.setStatus({ state: "disconnected", connectedSince: null });
          return ok(clone(this.status));
        }
        if (st === "disconnecting" || st === "recovering") return fail(appError("BUSY"));
        return this.runDisconnect();
      }),

    getStatus: () => this.call("getStatus", () => ok(clone(this.status))),

    setCaptureMode: (mode) =>
      this.call("setCaptureMode", async () => {
        if (mode === this.status.captureMode) return ok(clone(this.status));
        if (this.isBusyState()) return fail(appError("BUSY"));
        if (!PROXYING.includes(this.status.state)) {
          this.setStatus({ captureMode: mode });
          return ok(clone(this.status));
        }
        if (mode === "tun" && !this.helper.installed) return fail(appError("HELPER_NOT_INSTALLED"));
        const down = await this.runDisconnect("capture_mode_switch");
        if (down.status === "error") return down;
        this.status = { ...this.status, captureMode: mode };
        return this.runConnect();
      }),

    setRoutingMode: (mode) =>
      this.call("setRoutingMode", async () => {
        const st = this.status.state;
        if (mode === this.status.routingMode) return ok(clone(this.status));
        if (this.isBusyState()) return fail(appError("BUSY"));
        if (st === "direct") {
          this.status = { ...this.status, routingMode: mode };
          return this.runConnect();
        }
        if (!PROXYING.includes(st)) {
          this.setStatus({ routingMode: mode });
          return ok(clone(this.status));
        }
        if (mode === "direct") {
          this.status = { ...this.status, routingMode: mode };
          return this.runDisconnect("direct_mode", "direct");
        }
        return this.restartCore({ routingMode: mode }, `Restarting proxy core for ${mode.toUpperCase()} routing`);
      }),

    setDnsProfile: (profile) =>
      this.call("setDnsProfile", async () => {
        if (profile === this.status.dnsProfile) return ok(clone(this.status));
        if (this.isBusyState()) return fail(appError("BUSY"));
        if (!PROXYING.includes(this.status.state)) {
          this.setStatus({ dnsProfile: profile });
          return ok(clone(this.status));
        }
        return this.restartCore({ dnsProfile: profile }, `Restarting proxy core for ${profile} DNS`);
      }),

    getFailoverSettings: () => this.call("getFailoverSettings", () => ok(clone(this.failoverSettings))),
    setFailoverSettings: (s) =>
      this.call("setFailoverSettings", () => {
        const fields = validateFailover(s, this.nodes);
        if (fields.length) return fail(appError("VALIDATION_FAILED", { fields }));
        this.failoverSettings = clone(s);
        return ok(clone(s));
      }),

    getSafetySettings: () => this.call("getSafetySettings", () => ok(clone(this.safety))),
    setSafetySettings: (s) =>
      this.call("setSafetySettings", () => {
        this.safety = clone(s);
        return ok(clone(s));
      }),

    getRulesStatus: () => this.call("getRulesStatus", () => ok(clone(this.rules))),
    updateRulesNow: () =>
      this.call("updateRulesNow", async () => {
        if (this.rules.updating) return fail(appError("RULES_UPDATE_IN_PROGRESS"));
        this.rules = { ...this.rules, updating: true };
        await this.sleep(2200);
        return this.finishRulesUpdate();
      }),

    getHelperStatus: () => this.call("getHelperStatus", () => ok(clone(this.helper))),
    installHelper: () =>
      this.call("installHelper", async () => {
        // SPEC 1.4.2: Windows helper = service via UAC; install failures carry the helper's message in `detail`.
        const win = this.appInfo().platform === "windows";
        this.log("info", "app", "Requesting administrator approval to install helper");
        await this.sleep(2500);
        const o = this.dev.helperInstallOutcome;
        if (o === "HELPER_INSTALL_FAILED")
          return fail(
            appError(o, {
              detail: win
                ? "helper: CreateServiceW failed: Access is denied. (os error 5)\nservice: LynnkHelper · step 2/4 (register service)"
                : "helper: launchctl bootstrap system failed: 5: Input/output error\nstep 3/4 (load daemon)",
            }),
          );
        if (o !== "ok") return fail(appError(o));
        this.helper = { ...this.helper, installed: true, running: true, version: this.helper.expectedVersion, needsUpgrade: false };
        this.emit("helperStatusChanged", { status: this.helper });
        this.log("info", "helper", `Helper ${this.helper.version} installed and running`);
        return ok(clone(this.helper));
      }),
    uninstallHelper: () =>
      this.call("uninstallHelper", async () => {
        if (this.status.captureMode === "tun" && PROXYING.includes(this.status.state))
          return fail(appError("BUSY", { detail: "TUN is connected; disconnect first" }));
        await this.sleep(2000);
        this.helper = defaultHelper();
        this.emit("helperStatusChanged", { status: this.helper });
        return ok(clone(this.helper));
      }),

    getSettings: () => this.call("getSettings", () => ok(clone(this.settings))),
    setSettings: (s) =>
      this.call("setSettings", () => {
        const fields = validateSettings(s);
        if (fields.length) return fail(appError("VALIDATION_FAILED", { fields }));
        this.settings = clone(s);
        return ok(clone(s));
      }),

    getLogs: (limit, minLevel) =>
      this.call("getLogs", () => {
        const min = LEVELS.indexOf(minLevel ?? "trace");
        const filtered = this.logs.filter((l) => LEVELS.indexOf(l.level) >= min);
        return ok(clone(filtered.slice(-Math.min(limit, 1000))));
      }),

    exportDiagnostics: () =>
      this.call("exportDiagnostics", async () => {
        await this.sleep(1200);
        const stamp = nowIso().replace(/:/g, "-");
        const win = this.appInfo().platform === "windows";
        const path = win
          ? `C:\\Users\\you\\Downloads\\${APP_NAME}-diagnostics-${stamp}.zip`
          : `/Users/you/Downloads/${APP_NAME}-diagnostics-${stamp}.zip`;
        this.log("info", "app", "Diagnostics exported (redacted)");
        this.lastExport = path;
        return ok({ path });
      }),

    restoreNetworking: () =>
      this.call("restoreNetworking", async () => {
        if (this.status.state === "disconnecting" || this.status.state === "recovering") return fail(appError("BUSY"));
        const gen = ++this.gen;
        this.stopTimers();
        this.setStatus({ state: "disconnecting" }, "Restoring network settings");
        await this.sleep(1200);
        if (gen !== this.gen) return fail(appError("BUSY"));
        const report = this.buildRestoreReport("restore_networking");
        this.setActive(null, "manual");
        const e = this.applyRestore(report);
        if (e) return fail(e);
        this.setStatus(
          { state: "disconnected", lastError: null, ...this.flagsFromHeld(false), activeNodeId: null, connectedSince: null, failover: null },
          "Network settings restored",
        );
        return ok(clone(report));
      }),

    getAppInfo: () => this.call("getAppInfo", () => ok(this.appInfo())),

    quitApp: () =>
      this.call("quitApp", async () => {
        // Quits even if the restore is incomplete; the report persists (SPEC §2.7).
        if (this.status.state !== "disconnected") await this.runDisconnect("quit");
        this.log("info", "app", "Quit");
        setTimeout(() => this.onQuit?.(), 0);
        return ok(null);
      }),

    // SPEC §2.8 windows. In the browser both "windows" are on one dev page.
    hidePopover: () =>
      this.call("hidePopover", () => {
        this.anyListeners.forEach((h) => h("· hidePopover (command)", null));
        this.hidePopoverWindow();
        return ok(null);
      }),
    openSettings: (section) =>
      this.call("openSettings", () => {
        this.anyListeners.forEach((h) => h("· openSettings (command)", { section }));
        this.hidePopoverWindow(); // SPEC §2.8: open_settings hides the popover
        if (section) this.emit("settingsSectionRequested", { section });
        return ok(null);
      }),
    revealDiagnostics: () =>
      this.call("revealDiagnostics", () => {
        if (!this.lastExport) return fail(appError("DIAGNOSTICS_NOT_FOUND"));
        this.anyListeners.forEach((h) => h("· revealDiagnostics (command)", { path: this.lastExport }));
        return ok(null);
      }),
  };

  private appInfo(): AppInfo {
    const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
    return {
      appName: APP_NAME,
      appVersion: "0.1.0",
      specVersion: SPEC_VERSION,
      coreVersion: this.dev.stubMode ? null : "0.0.0-mock",
      platform: this.platformOverride ?? (/Windows/.test(ua) ? "windows" : /Mac/.test(ua) ? "macos" : "other"),
    };
  }

  private async restartCore(patch: Partial<Status>, reason: string): Promise<Result<Status>> {
    const gen = ++this.gen;
    const tun = this.status.captureMode === "tun";
    this.setStatus({ ...patch, state: "reconnecting", killSwitchActive: tun && this.safety.killSwitch }, reason);
    await this.sleep(900);
    if (gen !== this.gen) return ok(clone(this.status));
    this.setStatus({ state: "connected", killSwitchActive: false, lastError: null }, "Proxy core restarted");
    return ok(clone(this.status));
  }

  private finishRulesUpdate(): Result<RulesStatus> {
    const o = this.dev.rulesOutcome;
    const at = nowIso();
    const next = new Date(Date.now() + this.settings.rulesUpdateIntervalHours * 3_600_000).toISOString().replace(/\.\d+Z$/, "Z");
    if (o === "changed" || o === "unchanged") {
      this.rules = {
        ...this.rules,
        updating: false,
        lastCheckAt: at,
        nextCheckAt: next,
        lastError: null,
        ...(o === "changed"
          ? { version: at.slice(0, 10).replace(/-/g, "."), source: "downloaded" as const, updatedAt: at }
          : {}),
      };
      this.emit("rulesUpdated", { ok: true, changed: o === "changed", status: this.rules });
      this.log("info", "app", o === "changed" ? `Rules updated to ${this.rules.version}` : "Rules already current");
      return ok(clone(this.rules));
    }
    const e = appError(o, { detail: "mock: rule source returned HTTP 503" });
    this.rules = { ...this.rules, updating: false, lastCheckAt: at, nextCheckAt: next, lastError: e };
    this.emit("rulesUpdated", { ok: false, changed: false, status: this.rules });
    this.log("warn", "app", `Rule update failed: ${o}`);
    return fail(e);
  }

  // ───────────────────────────── dev controls ─────────────────────────────

  /** Snap straight into a state for design work (no transition sequence). */
  applyScenario(id: ScenarioId) {
    ++this.gen;
    this.stopTimers();
    this.setDev({ autoProbes: false });
    if (id === "no_nodes") this.fixtures = [];
    else if (!this.fixtures.length) this.fixtures = defaultNodes();
    const hy2 = this.nodes.find((n) => n.protocol === "hysteria2");
    const reality = this.nodes.find((n) => n.protocol === "vless_reality");
    const sel = hy2?.id ?? this.nodes[0]?.id ?? null;
    const since = new Date(Date.now() - (2 * 3600 + 34 * 60 + 10) * 1000).toISOString().replace(/\.\d+Z$/, "Z");
    const base: Status = {
      ...freshStatus(),
      selectedNodeId: id === "no_nodes" ? null : sel,
      routingMode: this.status.routingMode === "direct" ? "rule" : this.status.routingMode,
      dnsProfile: this.status.dnsProfile,
    };
    const live = (p: Partial<Status>): Status => ({
      ...base,
      activeNodeId: sel,
      connectedSince: since,
      systemProxySet: (p.captureMode ?? base.captureMode) === "system_proxy",
      ...p,
    });
    const reason = "HY2: 3 consecutive probe failures — UDP may be blocked on this network";
    let s: Status = base;
    let why: string | null = null;
    switch (id) {
      case "off":
      case "no_nodes":
        break;
      case "helper_missing":
        this.helper = defaultHelper();
        this.emit("helperStatusChanged", { status: this.helper });
        s = { ...base, captureMode: "tun" };
        break;
      case "connecting":
        s = { ...base, state: "connecting" };
        why = "Starting proxy core";
        break;
      case "connected":
        s = live({ state: "connected" });
        break;
      case "connected_tun":
        this.installedHelper();
        s = live({ state: "connected", captureMode: "tun" });
        break;
      case "degraded":
        s = live({ state: "degraded", lastError: appError("PROBE_FAILED", { detail: "all probe targets failed" }) });
        why = "Health check failed (2/3)";
        break;
      case "failing_over":
        s = live({ state: "failing_over" });
        why = `Switching to ${reality?.name ?? "fallback"}`;
        break;
      case "on_fallback":
        s = live({
          state: "connected",
          activeNodeId: reality?.id ?? sel,
          failover: reality && sel
            ? { primaryNodeId: sel, fallbackNodeId: reality.id, since: nowIso(), reason, earliestReturnAt: new Date(Date.now() + 600_000).toISOString().replace(/\.\d+Z$/, "Z"), direct: false }
            : null,
        });
        why = reason;
        break;
      case "reconnecting":
        s = live({ state: "reconnecting", lastError: appError("CORE_CRASHED") });
        why = "Restarting proxy core (attempt 2)";
        break;
      case "blocked":
        this.installedHelper();
        s = live({
          state: "error",
          captureMode: "tun",
          systemProxySet: false,
          killSwitchActive: true,
          lastError: appError("ALL_NODES_FAILED", { recoverable: false }),
        });
        break;
      case "direct":
        s = { ...base, state: "direct", routingMode: "direct", connectedSince: since };
        break;
      case "disconnecting":
        s = live({ state: "disconnecting" });
        why = "Restoring network settings";
        break;
      case "recovering":
        s = { ...base, state: "recovering" };
        why = "Found saved network state from the previous session";
        break;
      case "error":
        s = { ...base, state: "error", lastError: appError("CORE_START_FAILED", { detail: "mock: sing-box exited with code 1" }) };
        why = "Connection failed — network settings rolled back";
        break;
      case "unprotected": {
        const reason = "All nodes failing — direct fallback: traffic is going out directly, unprotected";
        s = live({
          state: "degraded",
          activeNodeId: null,
          lastError: appError("DIRECT_FALLBACK_ACTIVE"),
          failover: sel ? { primaryNodeId: sel, fallbackNodeId: "", since: nowIso(), reason, earliestReturnAt: null, direct: true } : null,
        });
        why = reason;
        break;
      }
      case "all_down":
        s = live({ state: "degraded", lastError: appError("ALL_NODES_FAILED", { recoverable: false }) });
        why = "All nodes failing — traffic stays blocked until a node recovers";
        break;
      case "restore_incomplete":
      case "restore_incomplete_proxy": {
        const tun = id === "restore_incomplete";
        if (tun) this.installedHelper();
        this.held.clear();
        this.holdFor(tun ? "tun" : "system_proxy");
        const report = this.buildRestoreReport(
          "disconnect",
          tun ? { dns: "failed", firewall: "failed" } : { system_proxy: "failed" },
        );
        s = {
          ...base,
          captureMode: tun ? "tun" : "system_proxy",
          state: "error",
          lastError: appError("RESTORE_INCOMPLETE", { recoverable: false, restoreReport: report }),
          lastRestore: report,
          ...this.flagsFromHeld(true),
        };
        why = "Networking is not fully restored";
        break;
      }
    }
    if (id !== "restore_incomplete" && id !== "restore_incomplete_proxy") {
      // Keep the mock journal consistent with the snapped state.
      const holding = ["connected", "degraded", "failing_over", "reconnecting", "disconnecting"].includes(s.state) || id === "blocked";
      this.held.clear();
      if (holding) this.holdFor(s.captureMode);
      s = { ...s, ...this.flagsFromHeld(s.killSwitchActive) };
    } else if (s.lastRestore) this.emit("restoreReportChanged", { report: s.lastRestore });
    this.status = s;
    this.emitNodes();
    this.emit("statusChanged", { status: this.status, reason: why });
    if (PROXYING.includes(s.state)) {
      this.startTimers();
      this.stats = { ...this.stats, latencyMs: s.state === "degraded" ? null : 48 };
    }
    if (id === "on_fallback" && reality && sel) {
      this.emit("activeNodeChanged", { previousNodeId: sel, nodeId: reality.id, cause: "failover" });
      this.emit("failoverEvent", { kind: "switched_to_fallback", fromNodeId: sel, toNodeId: reality.id, reason, at: nowIso() });
    }
  }

  private installedHelper() {
    if (this.helper.installed && this.helper.running) return;
    this.helper = { ...this.helper, installed: true, running: true, version: this.helper.expectedVersion, needsUpgrade: false };
    this.emit("helperStatusChanged", { status: this.helper });
  }

  setNodeSet(kind: "default" | "one" | "many" | "none") {
    if (PROXYING.includes(this.status.state)) return;
    this.fixtures = kind === "default" ? defaultNodes() : kind === "one" ? oneNode() : kind === "many" ? manyNodes() : [];
    this.setStatus({ selectedNodeId: this.fixtures[0]?.node.id ?? null });
    this.emitNodes();
  }

  setSecretMissing(id: string, missing: boolean) {
    const f = this.fixtures.find((x) => x.node.id === id);
    if (!f) return;
    f.node = { ...f.node, secretMissing: missing };
    this.emitNodes();
  }

  setNodeHealth(id: string, h: NodeHealth) {
    this.setDev({ nodeHealth: { ...this.dev.nodeHealth, [id]: h }, autoProbes: true });
  }

  /** Every node fails its probes: all-nodes-failing path (direct fallback or ALL_NODES_FAILED). */
  simulateAllDown() {
    const all = Object.fromEntries(this.nodes.map((n) => [n.id, "fail" as const]));
    this.lastSwitchAt = 0;
    this.setDev({ nodeHealth: all, autoProbes: true });
  }

  /** Make HY2 fail its probes so the natural failover path runs. */
  simulateUdpBlocked() {
    const hy2s = Object.fromEntries(this.nodes.filter((n) => n.protocol === "hysteria2").map((n) => [n.id, "fail" as const]));
    this.lastSwitchAt = 0;
    this.setDev({ nodeHealth: { ...this.dev.nodeHealth, ...hy2s }, autoProbes: true });
  }

  restoreAllHealth() {
    this.setDev({ nodeHealth: {}, autoProbes: true });
  }

  async simulateCoreCrash(kind: "crash" | "network" | "wake") {
    if (!PROXYING.includes(this.status.state)) return;
    const gen = ++this.gen;
    const tun = this.status.captureMode === "tun";
    const code: ErrorCode = kind === "network" ? "NETWORK_CHANGED" : "CORE_CRASHED";
    const first = kind === "crash" ? "Proxy core stopped — restarting" : kind === "network" ? "Network changed — reconnecting" : "Woke from sleep — reconnecting";
    this.log(kind === "crash" ? "error" : "info", "core", kind === "crash" ? "sing-box exited unexpectedly (signal 9)" : first);
    this.setStatus({ state: "reconnecting", lastError: appError(code), killSwitchActive: tun && this.safety.killSwitch }, first);
    const attempts = this.dev.reconnectOutcome === "recover" ? 2 : 4;
    for (let i = 1; i <= attempts; i++) {
      await this.sleep(1300);
      if (gen !== this.gen) return;
      this.setStatus({}, `Restarting proxy core (attempt ${i})`);
    }
    await this.sleep(1000);
    if (gen !== this.gen) return;
    if (this.dev.reconnectOutcome === "recover") {
      this.failedRounds = 0;
      this.setStatus({ state: "connected", lastError: null, killSwitchActive: false }, "Proxy core restarted");
      return;
    }
    const e = appError("CORE_START_FAILED", { recoverable: false, detail: "mock: gave up after 5 min of retries" });
    const keepBlocked = tun && this.safety.killSwitch && !this.safety.directFallback;
    this.stopTimers();
    if (keepBlocked) this.setStatus({ state: "error", lastError: e, killSwitchActive: true }, "Could not restart — traffic stays blocked");
    else {
      this.setActive(null, "manual");
      this.setStatus({ state: "error", lastError: e, ...this.osReleased() }, "Could not restart — network settings restored");
    }
    this.emit("errorRaised", { error: e });
  }

  /** Launch finds a journal from a TUN session that did not shut down cleanly. */
  async simulateLaunchRecovery(outcome: RecoveryOutcome) {
    const gen = ++this.gen;
    this.stopTimers();
    this.held = new Set(["tun_session", "routes", "dns", "ipv6", "firewall"]);
    this.status = { ...this.status, captureMode: "tun", activeNodeId: null, connectedSince: null, failover: null };
    this.setStatus({ state: "recovering", lastError: null, ...this.flagsFromHeld(false) }, "Found saved network state from the previous session");
    await this.sleep(1200);
    if (gen !== this.gen) return;
    this.setStatus({}, outcome === "reattached" ? "Re-attaching to running session" : "Restoring network settings");
    await this.sleep(1000);
    if (gen !== this.gen) return;

    if (outcome === "reattached") {
      this.installedHelper();
      const report: RecoveryReport = { outcome, at: nowIso(), restore: null };
      this.setActive(this.status.selectedNodeId, "restore");
      this.startTimers();
      this.setStatus({ state: "connected", connectedSince: nowIso(), lastRecovery: report, ...this.osHeld() }, "Re-attached to previous session");
      this.emit("recoveryCompleted", { report });
      return;
    }
    const failures =
      outcome === "restored"
        ? {}
        : Object.keys(this.dev.restoreFailures).length
          ? this.dev.restoreFailures
          : ({ dns: "failed", firewall: "failed" } as const);
    const restore = this.buildRestoreReport("recovery", failures);
    const report: RecoveryReport = { outcome, at: nowIso(), restore };
    this.status = { ...this.status, lastRecovery: report };
    if (!this.applyRestore(restore)) {
      this.setStatus({ state: "disconnected", lastError: null, ...this.flagsFromHeld(false) }, "Previous network settings restored");
    }
    this.emit("recoveryCompleted", { report });
  }

  raiseError(code: ErrorCode, recoverable: boolean) {
    const e = appError(code, { recoverable, detail: "mock: raised from dev panel" });
    this.log(recoverable ? "warn" : "error", "app", `${code}: ${e.message}`);
    this.emit("errorRaised", { error: e });
  }

  scheduledRulesUpdate() {
    this.rules = { ...this.rules, updating: true };
    void this.sleep(800).then(() => this.finishRulesUpdate());
  }

  setHelperState(kind: "missing" | "running" | "stopped" | "outdated") {
    const exp = this.helper.expectedVersion;
    this.helper =
      kind === "missing"
        ? defaultHelper()
        : kind === "running"
          ? { installed: true, running: true, version: exp, expectedVersion: exp, needsUpgrade: false }
          : kind === "stopped"
            ? { installed: true, running: false, version: exp, expectedVersion: exp, needsUpgrade: false }
            : { installed: true, running: true, version: "0.0.9", expectedVersion: exp, needsUpgrade: true };
    this.emit("helperStatusChanged", { status: this.helper });
  }

  resetAll() {
    ++this.gen;
    this.stopTimers();
    this.fixtures = defaultNodes();
    this.status = freshStatus();
    this.failoverSettings = clone(DEFAULT_FAILOVER);
    this.safety = clone(DEFAULT_SAFETY);
    this.settings = clone(DEFAULT_SETTINGS);
    this.rules = defaultRules();
    this.helper = defaultHelper();
    this.lastSwitchAt = 0;
    this.switchTimes = [];
    this.held.clear();
    this.setDev({ nodeHealth: {}, forced: {}, autoProbes: true });
    this.emitNodes();
    this.emit("statusChanged", { status: this.status, reason: null });
    this.emit("helperStatusChanged", { status: this.helper });
  }

  /** Simulated popover window visibility (SPEC §2.8). */
  popoverVisible = true;

  /** The tray click that shows the popover (SPEC §2.8). */
  emitPopoverShown() {
    this.popoverVisible = true;
    this.emit("popoverShown", null);
  }

  /** Every backend hide emits popoverHidden — but not if already hidden (SPEC 1.2.0). */
  hidePopoverWindow() {
    if (!this.popoverVisible) return;
    this.popoverVisible = false;
    this.emit("popoverHidden", null);
  }

  asBackend(): Backend {
    return {
      kind: "mock",
      commands: this.commands,
      on: async (name, cb) => this.on(name, cb),
    };
  }
}
