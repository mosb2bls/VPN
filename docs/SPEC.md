# EdgeClient — Backend ↔ UI Interface Specification

**Spec version:** 0.3.0
**Status:** approved baseline 0.1.0 + REQ-2/REQ-3/REQ-4 (Phase B1)
**Generated bindings:** `src/api/bindings.ts` (from `src-tauri/src/api/*`, via tauri-specta `=2.0.0-rc.25`)

The Rust types in `src-tauri/src/api/` are the source of truth; this document explains
their meaning. If the two disagree, the generated `bindings.ts` is what compiles, and
the disagreement is a backend bug — file a REQ.

## Changelog

| Version | Date | Change |
|---|---|---|
| 0.3.0 | 2026-10-11 | **Additive (REQ-4, owner decision):** per-item restore reporting (§2.7). New: `RestoreItem`, `RestoreItemKind`, `RestoreResult`, `RestoreTrigger`; `RestoreReport.{at, trigger, items}`; `RecoveryReport.restore`; `AppError.restoreReport`; `Status.{tunActive, lastRestore}`; event `restoreReportChanged`; codes `TUN_STOP_FAILED`, `ROUTE_RESTORE_FAILED`, `DNS_RESTORE_FAILED`, `IPV6_RESTORE_FAILED`. **Clarified:** partial restore failure is always the error `RESTORE_INCOMPLETE` (never `ok`), for `disconnect` and `restore_networking` alike. **Deprecated:** `RestoreReport.actions`, `RecoveryReport.actions`, type `RestoreAction` (mirrors; removed in the next major). |
| 0.2.0 | 2026-10-11 | **Additive (REQ-2):** `TrafficStats.latencyMs`. **Clarifications (REQ-3):** concurrent `test_node` (§2.1), `nodesChanged` after a test (§2.1), connect preconditions leave state unchanged (§2.2), progress text in `statusChanged.reason` (§2.2), `get_logs` polling (§2.6). |
| 0.1.0 | 2026-10-11 | Initial contract: 32 commands, 10 events, state machine, error codes, Notes for UI. All network/OS commands are stubs (`NOT_IMPLEMENTED`). |

Versioning: **major** = breaking (owner approval first), **minor** = additive, **patch** =
clarification/wording. Each interface change updates this file, the changelog, and
`bindings.ts` in the same commit. The constant `SPEC_VERSION` is exported in
`bindings.ts`; `get_app_info().specVersion` returns it at runtime.

---

## 1. Conventions

- **Calling:** `import { commands, events } from "./api/bindings"`. Commands are camelCase
  (`commands.listNodes()`), Rust names are snake_case (`list_nodes`).
- **Results:** every command resolves to
  `{ status: "ok", data: T } | { status: "error", error: AppError }`. Commands never throw
  for domain errors; a thrown JS `Error` means an IPC/bridge failure.
- **Field casing:** object fields camelCase; enum values snake_case (`"system_proxy"`);
  error codes SCREAMING_SNAKE_CASE.
- **Numbers:** all integers fit in JS `number` (`u16`/`u32`); byte totals are `number`
  (f64). No bigint.
- **Timestamps:** RFC 3339 UTC strings, e.g. `"2026-10-11T12:34:56Z"`.
- **Nullable:** `T | null`; absent values are `null`, never `undefined`.
- **Secrets:** never sent to the UI in full. Fields ending in `Masked` contain e.g.
  `"a1b2…••••"` (first 4 chars + `…••••`, or `"••••"` if shorter than 8). Secrets
  only travel UI → backend, inside forms (`SecretInput`).
- **Idempotency:** `connect` while connected, `disconnect` while disconnected, and
  setting a mode to its current value are no-ops that return the current status.
- **Concurrency:** if a conflicting operation is running (e.g. `connect` during
  `disconnecting`), the command fails with `BUSY` (recoverable).

## 2. Commands

"Elevation" = may trigger an OS admin/UAC prompt. "B1" column: what the current stub does
— **stub** = returns `NOT_IMPLEMENTED`; **placeholder** = returns fixed placeholder data;
**memory** = updates in-memory state only (not persisted, not applied to the OS).

### 2.1 Nodes

| Command | Params | Returns | Errors | Elevation | B1 |
|---|---|---|---|---|---|
| `list_nodes` | — | `Node[]` (user order) | — | no | placeholder (2 nodes) |
| `import_links` | `text: string` | `ImportResult` | `IMPORT_EMPTY` (+ per-line errors in result) | no | stub |
| `import_from_clipboard` | — | `ImportResult` | `CLIPBOARD_EMPTY`, `CLIPBOARD_UNAVAILABLE`, `IMPORT_EMPTY` | no | stub |
| `import_qr_image` | `source: QrImageSource` | `ImportResult` | `QR_NOT_FOUND`, `QR_IMAGE_UNREADABLE`, `QR_TOO_MANY_CODES` | no | stub |
| `add_node_manual` | `form: NodeForm` | `Node` | `VALIDATION_FAILED` (with `fields`), `NODE_DUPLICATE`, `KEYCHAIN_*` | no | stub |
| `update_node` | `id: string, form: NodeForm` | `Node` | `NODE_NOT_FOUND`, `VALIDATION_FAILED`, `KEYCHAIN_*` | no | stub |
| `delete_node` | `id: string` | `null` | `NODE_NOT_FOUND`, `BUSY` (node is active) | no | memory |
| `reorder_nodes` | `ids: string[]` (all IDs, new order) | `Node[]` | `REORDER_MISMATCH` | no | memory |
| `select_node` | `id: string` | `Status` | `NODE_NOT_FOUND` | no | memory |
| `test_node` | `id: string` | `NodeTestResult` | `NODE_NOT_FOUND`, `CORE_*`, `BUSY` | no | stub |

- **Import** accepts `hy2://`, `hysteria2://`, `vless://`, one link per line; blank lines
  and surrounding whitespace are ignored. Valid links are added even if other lines fail;
  each failing line is reported in `ImportResult.errors` with its 1-based line number and
  a code (`LINK_UNSUPPORTED_SCHEME`, `LINK_MALFORMED`, `LINK_UNSUPPORTED_PARAM`,
  `LINK_MISSING_PARAM`, `LINK_INVALID_VALUE`, `NODE_DUPLICATE`). Unsupported parameters
  are **rejected, never silently ignored**. The command itself only fails (`IMPORT_EMPTY`)
  if the input has no non-blank lines.
- **Clipboard** is read only inside `import_from_clipboard`, i.e. only when the user
  explicitly clicks an import-from-clipboard control. The backend never reads it otherwise.
- **Duplicate** = same protocol + server + port + credential (UUID or password).
- **Delete** of the node currently carrying traffic fails with `BUSY`; disconnect first.
- **select_node** while connected switches the core's selector live (no reconnect); the
  `activeNodeChanged` event follows. While disconnected it only changes the selection.
  Manual selection always overrides automatic failover (see §5).
- **test_node** is a real end-to-end test: HTTP probes through the node to the failover
  probe targets, plus an exit-country lookup. It works whether or not connected and never
  changes the system's networking. Typically 1–10 s.
- **Concurrent tests** (REQ-3): the UI may call `test_node` for several nodes at once
  ("Test all"). The backend runs at most 3 tests in parallel and queues the rest, so calls
  may take longer but never fail for that reason. A second call for a node that is
  **already being tested** fails with `BUSY` (recoverable).
- **After a test** (REQ-3): the backend stores the result as that node's `lastTest` and
  emits `nodesChanged` with the updated list. The returned `NodeTestResult` is the same
  data; the UI may patch locally or wait for the event.

### 2.2 Connection

| Command | Params | Returns | Errors | Elevation | B1 |
|---|---|---|---|---|---|
| `connect` | — | `Status` | `NO_NODES`, `NO_NODE_SELECTED`, `CORE_*`, `SYSTEM_PROXY_SET_FAILED`, `HELPER_NOT_INSTALLED`, `HELPER_*`, `TUN_START_FAILED`, `PORT_IN_USE`, `DNS_BOOTSTRAP_FAILED`, `BUSY` | TUN: no (helper already installed) | stub |
| `disconnect` | — | `Status` | `RESTORE_INCOMPLETE` (with `error.restoreReport`, §2.7) | no | returns status |
| `get_status` | — | `Status` | — | no | placeholder |
| `set_capture_mode` | `mode: CaptureMode` | `Status` | as `connect` when connected | no | memory |
| `set_routing_mode` | `mode: RoutingMode` | `Status` | as `connect` when connected | no | memory |
| `set_dns_profile` | `profile: DnsProfile` | `Status` | as `connect` when connected | no | memory |

- `connect` returns once the state is `connected` (or `direct`), or fails. Progress is
  visible through `statusChanged` events (`connecting` first).
- **Preconditions** (REQ-3): `connect` checks these **before** leaving `disconnected`; if
  one fails it returns the error and the **state stays unchanged** (no `statusChanged`):
  `NO_NODES`, `NO_NODE_SELECTED`, `BUSY`, and in TUN mode `HELPER_NOT_INSTALLED`,
  `HELPER_VERSION_MISMATCH`, `HELPER_UNREACHABLE`. Only failures *after* `connecting` has
  started lead to state `error`. So the UI can show "helper missing" from
  `getHelperStatus()` + `captureMode = tun` before any connect attempt.
- **Progress text** (REQ-3): during `connecting`, `reconnecting`, `disconnecting`,
  `failing_over` and `recovering`, each step emits `statusChanged` with a short
  display-ready `reason`, e.g. "Starting proxy core", "Setting system proxy",
  "Starting TUN", "Running health check", "Restoring network settings". The text is for
  display only — it is not a stable identifier; never parse it.
- Mode changes while disconnected just store the preference. While connected:
  - **capture mode:** fully tears down the old mode (restores OS state) before starting the
    new one; transient states `disconnecting` → `connecting` → `connected`.
  - **routing mode / DNS profile:** the core is restarted with a new config; the OS capture
    (system proxy / TUN) stays in place. In TUN mode the kill switch covers the gap.
  - switching **to `direct`**: see §4 (`direct` state).
- All three preferences persist and are reused by auto-connect.

### 2.3 Failover and safety

| Command | Params | Returns | Errors | Elevation | B1 |
|---|---|---|---|---|---|
| `get_failover_settings` | — | `FailoverSettings` | — | no | placeholder (defaults) |
| `set_failover_settings` | `settings: FailoverSettings` | `FailoverSettings` | `VALIDATION_FAILED` | no | memory, **validated** |
| `get_safety_settings` | — | `SafetySettings` | — | no | placeholder (defaults) |
| `set_safety_settings` | `settings: SafetySettings` | `SafetySettings` | `VALIDATION_FAILED` | no | memory |

`FailoverSettings` validation (field → rule): `probeTargets` → 2–5 http(s) URLs with ≥ 2
distinct hosts; `probeIntervalSecs` 5–300; `probeTimeoutSecs` 1–30 and < interval;
`failureRounds` 1–20; `minDwellSecs` 0–3600; `returnStableSecs` 60–86400;
`maxSwitchesPerHour` 1–60; `fallbackNodeId` null or an existing node
(`NODE_NOT_FOUND` as field code). Defaults: enabled, fallback auto, targets
`https://www.gstatic.com/generate_204` + `https://cp.cloudflare.com/`, interval 10 s,
timeout 5 s, 3 rounds, dwell 300 s, return-stable 600 s, 4 switches/hour.

`SafetySettings` defaults: `killSwitch: true`, `directFallback: false`, `allowLan: true`.

### 2.4 Rules

| Command | Params | Returns | Errors | Elevation | B1 |
|---|---|---|---|---|---|
| `get_rules_status` | — | `RulesStatus` | — | no | placeholder |
| `update_rules_now` | — | `RulesStatus` | `RULES_UPDATE_IN_PROGRESS`, `RULES_DOWNLOAD_FAILED`, `RULES_VALIDATION_FAILED` | no | stub |

On failure the previous rules stay active (failure is recoverable). The result of every
attempt, scheduled or manual, is also emitted as `rulesUpdated`.

### 2.5 Helper (TUN mode)

| Command | Params | Returns | Errors | Elevation | B1 |
|---|---|---|---|---|---|
| `get_helper_status` | — | `HelperStatus` | — | no | placeholder (not installed) |
| `install_helper` | — | `HelperStatus` | `HELPER_INSTALL_CANCELLED`, `HELPER_INSTALL_FAILED` | **yes** — macOS admin password prompt / Windows UAC | stub |
| `uninstall_helper` | — | `HelperStatus` | `BUSY` (TUN connected), `HELPER_UNINSTALL_FAILED` | **yes** | stub |

### 2.6 App

| Command | Params | Returns | Errors | Elevation | B1 |
|---|---|---|---|---|---|
| `get_settings` | — | `AppSettings` | — | no | placeholder (defaults) |
| `set_settings` | `settings: AppSettings` | `AppSettings` | `VALIDATION_FAILED` (`rulesUpdateIntervalHours` 1–168), `LAUNCH_AT_LOGIN_FAILED`, `SETTINGS_SAVE_FAILED` | no | memory, **validated** |
| `get_logs` | `limit: number, minLevel: LogLevel \| null` | `LogEntry[]` (oldest first) | — | no | returns `[]` |
| `export_diagnostics` | — | `DiagnosticsExport` | `DIAGNOSTICS_EXPORT_FAILED` | no | stub |
| `restore_networking` | — | `RestoreReport` (only when fully restored) | `RESTORE_INCOMPLETE` (with `error.restoreReport`, §2.7) | only if helper is not installed but a TUN/firewall journal exists | stub |
| `get_app_info` | — | `AppInfo` | — | no | **real** |
| `quit_app` | — | `null` (process exits) | — (an incomplete restore is persisted, §2.7; the app still quits) | no | exits immediately |

- `get_logs` (REQ-3): polling every ~2 s while a logs view is visible is fine (the
  backend keeps a 2000-entry in-memory ring buffer; `limit` is capped at 1000). There is
  no log-stream event in this version.
- `AppSettings` defaults: launch at login off, auto-connect off, close-to-tray on, rule
  auto-update on every 24 h, log level `info`.
- `export_diagnostics` writes a zip to the user's Downloads folder (logs, redacted config,
  status, versions, OS info — all redacted) and returns its path. It is never uploaded.
- `restore_networking` is safe at any time: it disconnects, restores system proxy,
  routes, DNS, IPv6 and firewall from the journal (or to known-safe defaults for items we
  own, such as removing our pf anchor), and stops any core. The same is available without
  the UI as `EdgeClient --restore-networking`.
- `quit_app`: disconnect → restore → exit. Exits even if restore is incomplete; the
  journal is kept so the next launch (or helper) finishes the restore.

### 2.7 Restore reporting (REQ-4)

Every operation that puts the OS back — `disconnect`, `restore_networking`, `quit_app`,
`set_routing_mode(direct)`, `set_capture_mode` while connected (teardown of the old
mode), launch-time recovery and `--restore-networking` — runs the **same** restore and
produces the **same** `RestoreReport`:

- `items` has **exactly six entries, always, in this order**: `tun_session`, `routes`,
  `dns`, `ipv6`, `system_proxy`, `firewall` (the firewall/kill switch goes last so it
  keeps blocking until everything else is back). Each has a display `label`, a
  display-ready `action` (what was done or found), a `result` and, if not clean,
  `errorCode` + `errorMessage`.
- `result`: `restored` (we had changed it; it is back) · `not_needed` (we had not changed
  it) · `failed` (attempted, still changed) · `skipped` (could not be attempted, e.g.
  helper unreachable — may still be changed).
- `fullyRestored` is true **only** if every item is `restored` or `not_needed`.
  `failed` and `skipped` both make it incomplete. All items are always attempted; one
  failure never stops the rest.

**Partial failure is always an error, never `ok`.** `disconnect` and
`restore_networking` (and the mode setters when they tear down) return
`error.code = "RESTORE_INCOMPLETE"`, `recoverable: false`, with the full report in
`error.restoreReport`. On full success, `restore_networking` returns the report and
`disconnect` returns `Status`; in both cases the report is also in `Status.lastRestore`.

**Latest report:** `Status.lastRestore` always holds the most recent report, and the
event `restoreReportChanged` fires whenever it changes. An **incomplete** report is
persisted on disk and reloaded at launch (state `error`, see below) until a later
restore completes fully; a complete report is not persisted (after a restart
`lastRestore` is null unless something is still incomplete).

**True state after a partial failure:**
- `state` is `error` with `lastError.code = "RESTORE_INCOMPLETE"` — never
  `disconnected` (or `direct`) while any item is not clean.
- `killSwitchActive`, `systemProxySet` and `tunActive` describe what is **actually** in
  effect: an item that was held stays `true` unless its own restore result is clean.
- The state leaves `error` only when a later restore completes fully (`disconnect` or
  `restore_networking` again → `disconnected`), or when the user connects (see below).
- `connect` from this state is allowed. The backend keeps the **original** journal (the
  values captured before the first connect) and never records the half-restored state as
  "previous", so a later full restore still returns to the true original settings.
- `quit_app` with an incomplete restore still exits; the journal and report are kept and
  the next launch (or the helper, for its items) retries in `recovering`.

## 3. Events (backend → UI)

Listen with `events.<name>.listen(cb)`; payload in `event.payload`. Wire names are
kebab-case (`status-changed`).

| Event | Payload | When |
|---|---|---|
| `statusChanged` | `{ status: Status, reason: string \| null }` | Any state or mode change. Always carries the full `Status`. |
| `activeNodeChanged` | `{ previousNodeId, nodeId, cause: "manual" \| "failover" \| "failback" \| "restore" }` | The node carrying traffic changed. |
| `failoverEvent` | `{ kind: "switched_to_fallback" \| "returned_to_primary" \| "suppressed", fromNodeId, toNodeId, reason, at }` | Every automatic failover decision. `reason` is display-ready. |
| `trafficStatsUpdated` | `{ stats: TrafficStats }` | ~1 Hz while connected. Includes `latencyMs` (REQ-2). |
| `healthCheck` | `{ nodeId, roundOk, probes: ProbeResult[], consecutiveFailedRounds, at }` | Each probe round on the active node (default every 10 s). |
| `rulesUpdated` | `{ ok, changed, status: RulesStatus }` | After every rule update attempt. |
| `helperStatusChanged` | `{ status: HelperStatus }` | Helper installed/uninstalled/started/stopped/unreachable. |
| `nodesChanged` | `{ nodes: Node[], selectedNodeId }` | Node list or selection changed by any source (UI, tray, import). |
| `recoveryCompleted` | `{ report: RecoveryReport }` | Launch-time or helper-start recovery finished. Also stored in `Status.lastRecovery`. |
| `restoreReportChanged` | `{ report: RestoreReport }` | `Status.lastRestore` changed: any restore finished, or an incomplete report was reloaded at launch (REQ-4, §2.7). |
| `errorRaised` | `{ error: AppError }` | Background error not tied to a command. `recoverable: false` → user action needed. |

The UI should call `get_status` / `list_nodes` once at startup (events may have fired
before the listener was attached) and then rely on events.

## 4. Connection state machine

`ConnectionState`: `disconnected`, `connecting`, `connected`, `degraded`, `failing_over`,
`reconnecting`, `direct`, `disconnecting`, `recovering`, `error`.

| State | Meaning | OS state held | Kill switch (TUN) |
|---|---|---|---|
| `disconnected` | Idle. | none | off |
| `connecting` | Starting core, setting system proxy or TUN. | partial | armed once TUN is up |
| `connected` | Proxying; health probes passing. | yes | armed (engages if core dies) |
| `degraded` | Proxying, but probes failing below the failover threshold, or failover disabled/suppressed. Traffic may be failing. | yes | armed |
| `failing_over` | Switching the selector to the fallback node or back. Brief. | yes | armed |
| `reconnecting` | Core crashed, network changed, or woke from sleep; restarting with backoff. In TUN mode, **proxied traffic is blocked** by the kill switch meanwhile. | yes | **engaged** (TUN) |
| `direct` | Routing mode `direct`: no proxy, normal connectivity, no core. | none | off |
| `disconnecting` | Stopping core and restoring OS state from the journal. | partial | being removed |
| `recovering` | At launch/helper start, a journal was found; re-attaching or restoring. | partial | unchanged until decided |
| `error` | Could not connect, or a restore was incomplete (`lastError.code = RESTORE_INCOMPLETE`, details in `lastRestore`, §2.7). If `killSwitchActive` is true, traffic is **blocked** until a restore completes. | maybe (see `killSwitchActive` / `systemProxySet` / `tunActive`) | maybe engaged |

Transitions (trigger → next state):

| From | Trigger | To |
|---|---|---|
| `disconnected` | `connect`, routing ≠ direct | `connecting` |
| `disconnected` | `connect`, routing = direct | `direct` |
| `disconnected` | app launch, auto-connect on | `connecting` |
| `disconnected` / any | app launch / helper start finds a journal | `recovering` |
| `connecting` | core up, first probe round passes | `connected` |
| `connecting` | core up, first probe round fails | `degraded` |
| `connecting` | start failure (core, proxy, TUN, helper) | `error` (OS state rolled back) |
| `connecting` | `disconnect` | `disconnecting` |
| `connected` | probe round fails (below threshold) | `degraded` |
| `degraded` | probe round passes | `connected` |
| `connected` / `degraded` | failover threshold reached and switch allowed | `failing_over` |
| `degraded` | threshold reached but switch suppressed (dwell, rate limit, no fallback) | `degraded` (+ `failoverEvent` kind `suppressed`) |
| `connected` (on fallback) | primary passed continuously for `returnStableSecs` and dwell elapsed | `failing_over` |
| `failing_over` | selector switched | `connected` or `degraded` (by next probe) |
| `connected` / `degraded` / `failing_over` | core crash, network change, wake from sleep | `reconnecting` |
| `reconnecting` | core back up | `connected` / `degraded` |
| `reconnecting` | retries exhausted (backoff 1, 2, 4, 8, 16, 30 s…; give up after 5 min) | `error` (kill switch stays engaged in TUN; with `directFallback` on, OS state is restored instead) |
| `connected` / `degraded` / `failing_over` / `reconnecting` / `error` | `disconnect`, `quit_app`, `restore_networking` | `disconnecting` |
| proxying states | `set_routing_mode(direct)` | `disconnecting` → `direct` |
| `direct` | `set_routing_mode(rule \| global)` | `connecting` |
| `direct` | `disconnect` | `disconnected` |
| any proxying state | `set_capture_mode(other)` | `disconnecting` → `connecting` |
| any proxying state | `set_routing_mode(rule ↔ global)` / `set_dns_profile` | stays (core restarted; brief `reconnecting` possible) |
| `disconnecting` | restore complete | `disconnected` (or `direct`) |
| `disconnecting` | restore incomplete | `error` (`RESTORE_INCOMPLETE`, report in `lastRestore` and `error.restoreReport`) |
| `error` (restore incomplete) | `disconnect` / `restore_networking` and the restore completes fully | `disconnecting` → `disconnected` |
| `error` (restore incomplete) | `disconnect` / `restore_networking` and still incomplete | `disconnecting` → `error` (new report) |
| `error` (restore incomplete) | `connect` | `connecting` (original journal kept, §2.7) |
| `recovering` | healthy session found (helper kept TUN up) | `connected` (report `reattached`) |
| `recovering` | journal restored | `disconnected` (report `restored`) |
| `recovering` | restore incomplete | `error` (report `restore_incomplete`) |

## 5. Failover behaviour (summary; implemented in Phase B4)

- Applies only while connected with failover enabled. **Primary** = the selected node;
  **fallback** = `fallbackNodeId`, or (if null) the first node of the other protocol in
  list order. Node switching uses a sing-box `selector` via the local controller API,
  never `urltest`.
- Every `probeIntervalSecs`, the backend probes **through the active node** to each probe
  target. A round fails only if **all** targets fail. User traffic errors and individual
  websites are never counted.
- After `failureRounds` consecutive failed rounds (default 3 × 10 s ≈ 30 s) → switch to the
  fallback, unless within `minDwellSecs` of the last switch or over `maxSwitchesPerHour`
  (then `suppressed`).
- While on the fallback, the primary is probed in the background; it must pass
  continuously for `returnStableSecs` (default 10 min) before switching back.
- Manual `select_node` always wins and resets failover state.

## 6. Routing, DNS and IPv6 (planned behaviour — verified in B2/B3; **untested**)

### 6.1 Routing modes (default `rule`)

- **rule:** `geoip-cn`, `geosite-cn` and private/LAN ranges → direct; everything else →
  proxy.
- **global:** everything → proxy, except the always-direct set below.
- **direct:** no core, no proxy, normal connectivity; kill switch off (state `direct`).

Always direct in `rule` and `global` (never through the tunnel):

- Loopback: `127.0.0.0/8`, `::1/128`
- Private / LAN: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `100.64.0.0/10` (CGNAT,
  e.g. Tailscale), `fc00::/7` — only when `allowLan` is on
- Link-local: `169.254.0.0/16`, `fe80::/10`
- Multicast / broadcast: `224.0.0.0/4`, `ff00::/8`, `255.255.255.255/32` (mDNS, SSDP)
- DHCP: UDP 67/68
- The proxy server's own address(es), resolved via the bootstrap resolver
- `.local` names (mDNS)

Public IPv6 destinations: in TUN mode **rejected** (fast fail so apps fall back to IPv4);
in system proxy mode **not controllable** (see Notes for UI).

### 6.2 DNS profiles (default `mainland`)

| Query for | `mainland` | `overseas` |
|---|---|---|
| Direct (China / geosite-cn) domains | China resolver (AliDNS 223.5.5.5 / DNSPod 119.29.29.29; DoH/DoT if reliable, else UDP), sent **direct** | Non-China public resolver (e.g. Cloudflare 1.1.1.1 / Quad9), sent **direct** |
| Proxied domains | DoH (Cloudflare / Google) **through the proxy** | DoH **through the proxy** |
| Proxy server hostname | Bootstrap resolver, direct, never through the tunnel (skipped for IP literals) | same |

Exact resolvers and transports are fixed in the B2 DNS ADR after testing.

### 6.3 Fake-IP (TUN only)

TUN mode may answer A queries with fake IPs from `198.18.0.0/15`. Excluded (real
answers): the proxy server hostname, OS connectivity checks (`captive.apple.com`,
`*.msftconnecttest.com`, `*.msftncsi.com`), `*.local`, `*.lan`, `*.home.arpa`, single-label
LAN names, and any direct (China) domain in `rule` mode. AAAA queries are answered empty
(IPv4-only service).

## 7. Data types

All types are defined in `bindings.ts`; key semantics:

- **`Node`** — `{ id, name, protocol, server, port, details, lastTest }`. `id` is a UUID
  assigned by the backend. `details` is a union tagged by `protocol`:
  - `hysteria2`: `passwordMasked`, `serverPorts` (port-hopping ranges like `"20000-30000"`,
    empty = single port), `sni`, `insecure`, `obfsPasswordMasked` (non-null = salamander
    obfs on), `pinSha256`, `upMbps`, `downMbps`.
  - `vless_reality`: `uuidMasked`, `flow` (always `xtls-rprx-vision`), `sni`,
    `fingerprint`, `publicKeyMasked`, `shortIdMasked`, `spiderX`. Encryption `none`,
    security `reality`, transport `tcp` are fixed and not exposed.
- **`NodeForm`** — union tagged by `protocol` (`Hy2Form` / `VlessForm`). Secret fields are
  `SecretInput`: `{kind:"set", value}` | `{kind:"keep"}` | `{kind:"clear"}`.
  `add_node_manual` requires `set` for required secrets (HY2 `password`; VLESS `uuid`,
  `publicKey`, `shortId` — an empty short ID is `set` with `""`). `keep` is for
  `update_node`. `clear` is valid only for HY2 `obfsPassword` (turns obfs off).
- **Field-level errors** — `VALIDATION_FAILED` carries `fields: FieldError[]` where
  `field` is the camelCase form field name (`server`, `port`, `serverPorts`, `password`,
  `sni`, `obfsPassword`, `pinSha256`, `upMbps`, `downMbps`, `uuid`, `fingerprint`,
  `publicKey`, `shortId`, `spiderX`, `name`) and `code` is a `FIELD_*` code. Supported
  fingerprints: `chrome`, `firefox`, `safari`, `ios`, `android`, `edge`, `360`, `qq`,
  `random`, `randomized` (sing-box uTLS list; confirmed in B2).
- **`Status`** — see §4. `selectedNodeId` = user choice; `activeNodeId` = node carrying
  traffic (null when not proxying); `failover` non-null while on the fallback;
  `killSwitchActive`, `systemProxySet`, `tunActive` report the OS state **actually** in
  effect (not intended); `lastRestore` = most recent `RestoreReport` (§2.7).
- **`TrafficStats`** — bytes/s and totals since connect (numbers), active connection
  count, and `latencyMs`: median probe latency of the most recent **passing** health round
  on the active node, or `null` if there has been no round yet or the last round failed
  (REQ-2). It is updated at the probe interval (default 10 s), not every second.
- **`NodeTestResult`** — `ok`, `latencyMs`, `exitCountry` (ISO 3166-1 alpha-2), `error`.
- **`RestoreReport`** — `{ at, trigger, fullyRestored, items: RestoreItem[6], actions }`
  (§2.7; `actions` deprecated). **`RecoveryReport`** — `{ outcome: reattached | restored |
  restore_incomplete, at, restore: RestoreReport | null, actions }` (`restore` is null
  when re-attached; `actions` deprecated).
- **`RulesStatus`**, **`HelperStatus`**, **`AppSettings`**, **`LogEntry`**, **`AppInfo`**.
- **`AppError`** — `{ code, message, recoverable, fields, detail, restoreReport }`. Show
  `message` directly. `detail` is redacted technical text for a "details" disclosure /
  logs view. `restoreReport` is set for `RESTORE_INCOMPLETE` (always) and null otherwise.

## 8. Error codes

Stable IDs; never renamed or reused. Messages are display-ready (may be reworded in a
patch). Full list, with default messages, in `src-tauri/src/api/error.rs`
(`ErrorCode::message`).

| Group | Codes |
|---|---|
| General | `NOT_IMPLEMENTED`, `INTERNAL`, `INVALID_ARGUMENT`, `BUSY`, `UNSUPPORTED_PLATFORM` |
| Nodes & import | `NODE_NOT_FOUND`, `NO_NODE_SELECTED`, `NO_NODES`, `NODE_DUPLICATE`, `REORDER_MISMATCH`, `VALIDATION_FAILED`, `LINK_UNSUPPORTED_SCHEME`, `LINK_MALFORMED`, `LINK_UNSUPPORTED_PARAM`, `LINK_MISSING_PARAM`, `LINK_INVALID_VALUE`, `IMPORT_EMPTY`, `CLIPBOARD_EMPTY`, `CLIPBOARD_UNAVAILABLE`, `QR_NOT_FOUND`, `QR_IMAGE_UNREADABLE`, `QR_TOO_MANY_CODES` |
| Fields | `FIELD_REQUIRED`, `FIELD_INVALID`, `FIELD_OUT_OF_RANGE`, `FIELD_INVALID_HOST`, `FIELD_INVALID_PORT`, `FIELD_INVALID_PORT_RANGE`, `FIELD_INVALID_UUID`, `FIELD_INVALID_PUBLIC_KEY`, `FIELD_INVALID_SHORT_ID`, `FIELD_INVALID_FINGERPRINT`, `FIELD_INVALID_PIN`, `FIELD_INVALID_SNI`, `FIELD_INVALID_BANDWIDTH`, `FIELD_TOO_LONG`, `FIELD_CANNOT_CLEAR` |
| Keychain | `KEYCHAIN_UNAVAILABLE`, `KEYCHAIN_ACCESS_DENIED`, `SECRET_MISSING` |
| Core / connection | `CORE_BINARY_MISSING`, `CORE_CHECKSUM_MISMATCH`, `CORE_CONFIG_INVALID`, `CORE_START_FAILED`, `CORE_CRASHED`, `CORE_API_UNREACHABLE`, `PORT_IN_USE`, `DNS_BOOTSTRAP_FAILED`, `NODE_UNREACHABLE`, `NODE_HANDSHAKE_FAILED`, `NODE_AUTH_FAILED`, `PROBE_TIMEOUT`, `PROBE_FAILED`, `ALL_NODES_FAILED`, `SYSTEM_PROXY_SET_FAILED`, `SYSTEM_PROXY_RESTORE_FAILED`, `NETWORK_CHANGED` |
| Helper / TUN | `HELPER_NOT_INSTALLED`, `HELPER_INSTALL_CANCELLED`, `HELPER_INSTALL_FAILED`, `HELPER_UNINSTALL_FAILED`, `HELPER_UNREACHABLE`, `HELPER_VERSION_MISMATCH`, `HELPER_AUTH_FAILED`, `TUN_START_FAILED`, `KILL_SWITCH_FAILED`, `KILL_SWITCH_REMOVE_FAILED` |
| Recovery | `TUN_STOP_FAILED`, `ROUTE_RESTORE_FAILED`, `DNS_RESTORE_FAILED`, `IPV6_RESTORE_FAILED`, `JOURNAL_CORRUPT`, `RESTORE_INCOMPLETE` (item-level codes appear in `RestoreItem.errorCode`; `SYSTEM_PROXY_RESTORE_FAILED`, `KILL_SWITCH_REMOVE_FAILED`, `HELPER_UNREACHABLE` are also used there) |
| Rules | `RULES_DOWNLOAD_FAILED`, `RULES_VALIDATION_FAILED`, `RULES_UPDATE_IN_PROGRESS` |
| App | `SETTINGS_INVALID`, `SETTINGS_SAVE_FAILED`, `LAUNCH_AT_LOGIN_FAILED`, `DIAGNOSTICS_EXPORT_FAILED` |

`ErrorCode` is a closed string union in TS; the UI should still render unknown codes
gracefully using `message`.

## 9. Notes for UI

Things the UI must explain to the user (wording is the UI's choice; substance is not):

1. **System proxy mode does not protect the whole device.** It only covers apps that
   honour the OS proxy setting (most browsers do; many CLI tools, games, some
   updaters and system services do not). Never present it as a VPN or as "all traffic
   protected".
2. **System proxy mode cannot stop IPv6 bypass and has no kill switch.** If the core
   stops, apps fall back to direct connections. Recommend TUN mode for full-device
   protection.
3. **TUN mode needs a one-time privileged helper install.** Explain before calling
   `install_helper` that macOS will ask for an administrator password (Windows: UAC),
   what the helper does (runs the bundled proxy core with a TUN interface, applies the
   kill switch, restores networking), and that it can be uninstalled from the app.
4. **Kill switch (TUN):** while the core is restarting or failed, proxied traffic is
   **blocked** on purpose; China/LAN direct traffic may also be briefly blocked. When
   `status.killSwitchActive` is true, show it prominently and offer Disconnect and
   Restore networking.
5. **Direct fallback** (`safety.directFallback`, off by default): when on, if all nodes
   fail, traffic goes out **unprotected, directly**. Warn before enabling.
6. **Insecure TLS** (`hy2 insecure: true`): disables certificate verification; show a
   warning on the node.
7. **Failover reasons** in `failoverEvent.reason` are display-ready; show them (toast /
   activity log). Explain that HY2 uses UDP and some networks block it.
8. **Restore networking** is always safe to offer; also mention the
   `--restore-networking` command-line flag for when the UI can't start.
9. **`direct` routing mode** means no proxy at all; make that state visually distinct
   from `connected`.
10. **Recovery and incomplete restores:** when `lastRecovery` is present after launch,
    tell the user whether the previous session was re-attached or networking was
    restored. Whenever `lastRestore.fullyRestored` is false (or a command returns
    `RESTORE_INCOMPLETE`), show **which items** failed or were skipped (from
    `items`), make clear networking is **not** back to normal, and offer Restore
    networking again. A partial failure must never look like success.
11. **Clipboard:** import from clipboard only on explicit user action; the backend never
    reads the clipboard on its own.
12. **Secrets** are shown masked only; there is no "reveal" command. To change a secret,
    the user re-enters it.
13. **Placeholder data (B1):** two nodes named `[placeholder] …` and fixed statuses are
    returned until Phase B2. Most actions return `NOT_IMPLEMENTED`; render that error
    gracefully.
