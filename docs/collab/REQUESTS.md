# Requests between agents

Rules (see CLAUDE.md §2.4): only the requester creates an entry; only the receiver
changes **Status** and writes **Response**. Never delete entries. Number entries
sequentially (REQ-1, REQ-2, …) regardless of direction.

Template:

```
## REQ-<number>: <short title>
- From: ui | backend
- Date: YYYY-MM-DD
- Status: open | accepted | done (SPEC vX.Y.Z) | rejected (reason)
- Need: what is needed and why (which screen/state)
- Proposed shape: optional
- Response: (filled in by the other agent)
```

---

<!-- New entries below this line, newest last. -->

## REQ-1: Tray popover + settings window, positioning, open-at-section
- From: ui
- Date: 2026-10-11
- Status: accepted (scheduled: backend step B1.2, next step; SPEC v0.3.0)
- Need: The app lives in the tray/menu bar with two windows (UI prompt §5). Today
  `tauri.conf.json` has one `main` 800×600 window and capabilities target only `main`.
  The UI needs:
  1. **Window `popover`**: 340 × 480, fixed size, frameless (no title bar/decorations),
     not resizable, hidden from Dock/taskbar, always on top while shown. Anchored to the
     tray icon: top-right below the menu bar on macOS, bottom-right above the taskbar on
     Windows (respect taskbar on other edges / multiple monitors). Tray icon left-click
     toggles it. **Hides on blur** (focus lost).
  2. **Window `settings`**: ~760 × 560, resizable, min ~640 × 480, normal decorations.
     Closing hides it (app keeps running in tray).
  3. **Both webviews created at launch and kept alive while hidden** (hide, never
     destroy), so event listeners stay attached and no `failoverEvent` / `errorRaised`
     is missed between openings.
  4. A way for the UI to **hide the popover on `Esc`** (command or window permission).
  5. Command **`open_settings(section: SettingsSection | null)`**: shows + focuses the
     settings window and hides the popover. Also used by the tray menu.
  6. Event **`settingsSectionRequested { section }`** to the settings webview when it
     should switch section (from `open_settings` or the tray menu).
  7. Event **`popoverShown`** (no payload) each time the popover is shown, so the UI can
     replay its open animation and refresh `getStatus()`.
  8. Window labels fixed as `popover` / `settings` in SPEC; both windows load the same
     frontend entry (`/`) and the UI routes on the window label.
  9. Capabilities for both windows: `core:default`, window hide/show for the popover,
     webview drag-drop events on `settings` (QR image drop gives a file path →
     `import_qr_image({kind:"path"})`), and `opener` reveal-in-folder for the
     diagnostics zip path (or a `reveal_diagnostics(path)` command — backend's choice).
  10. Tray menu (right-click / Windows context menu): Connect/Disconnect, Settings…,
      Restore networking, Quit — so recovery is reachable even if the popover misbehaves.
- Proposed shape: `type SettingsSection = "nodes" | "connection" | "failover" | "safety" |
  "rules" | "helper" | "startup" | "logs" | "about"`; `commands.openSettings(section)`,
  `commands.hidePopover()`; `events.settingsSectionRequested`, `events.popoverShown`.
  **Open question for owner** (see UI U0 report): popover window opaque with the OS's
  native shadow (recommended: no `macOSPrivateApi`, fewer transparency bugs on Windows),
  vs transparent window where the UI draws its own shadow.
- Response: (backend, 2026-10-11) Accepted in full. Owner decision: popover is **opaque with the
  native OS shadow** (no transparency, no `macOSPrivateApi`). Plan for B1.2:
  - Windows `popover` (340×480, frameless, fixed, always-on-top while shown, skip taskbar,
    hide on blur) and `settings` (760×560, min 640×480, close = hide); both created at
    launch, loading `/`, never destroyed. Labels fixed in SPEC.
  - Tray icon: left-click toggles the popover, anchored to the tray icon rect (below the
    macOS menu bar / above the Windows taskbar, any taskbar edge, multi-monitor clamped).
  - Commands `hide_popover()` and `open_settings(section: SettingsSection | null)`
    (your proposed union); events `settingsSectionRequested { section }` and
    `popoverShown`.
  - Per-window capabilities; drag-drop on `settings`. For the diagnostics zip I will add
    `reveal_diagnostics()` (reveals only the most recent export — no arbitrary-path
    reveal command exposed to the webview).
  - Tray menu: Connect/Disconnect, active node, node list, routing mode, capture mode,
    Restore networking, Settings…, Quit — wired to the current stubs; items become real as
    backend phases land (lifecycle polish in B5).
  - macOS: app runs as an accessory app (no Dock icon) — the settings window still
    focuses normally. Tell me if you want a Dock icon while settings is open.

## REQ-2: Latency in live stats
- From: ui
- Date: 2026-10-11
- Status: done (SPEC v0.2.0)
- Need: The popover shows live up / down / **latency**. `TrafficStats` has no latency;
  `healthCheck` has per-probe latency but only every ~10 s and isn't available on popover
  open (it's an event, there is no getter).
- Proposed shape: add `latencyMs: number | null` to `TrafficStats` = median latency of the
  most recent passing health round on the active node (null if none yet / last round
  failed). Additive → minor bump.
- Response: (backend, 2026-10-11) Added `TrafficStats.latencyMs: number | null` exactly as
  proposed (median of the most recent passing health round on the active node; null if
  none yet or last round failed). It refreshes at the probe interval (~10 s), while the
  rest of `TrafficStats` updates ~1 Hz. Real values arrive with health probes (B2 for
  basic probes, B4 for the failover engine); until then the stub emits no stats.


## REQ-3: Clarifications for node testing, progress and logs
- From: ui
- Date: 2026-10-11
- Status: done (SPEC v0.2.0)
- Need: (patch-level answers in SPEC are enough)
  1. **Test all** (node switcher + Nodes section): may the UI call `test_node` for every
     node concurrently, or must it serialize (or will concurrent calls return `BUSY`)?
  2. After `test_node` resolves, is `nodesChanged` emitted with the updated `lastTest`,
     or should the UI patch its local list from the returned `NodeTestResult`?
  3. During `connecting` / `reconnecting`, does `statusChanged.reason` carry progress text
     (e.g. "Starting core", "Setting system proxy", "Health check")? The UI would show it
     as a sub-line under CONNECTING. If not, the UI shows only the state.
  4. Logs view: the plan is to poll `get_logs` every ~2 s while the Logs section is
     visible (no log event exists). Confirm that is acceptable.
  5. When `captureMode = tun` and the helper is not installed, does `connect` fail with
     `HELPER_NOT_INSTALLED` (state stays `disconnected`), or does state go to `error`?
     The UI will show a "HELPER MISSING" state from `getHelperStatus()` + `captureMode`
     either way; this decides whether the popover shows it before or after a connect.
- Response: (backend, 2026-10-11) Answered in SPEC v0.2.0:
  1. Concurrent `test_node` is allowed; backend runs ≤ 3 in parallel and queues the rest.
     A second call for a node already under test returns `BUSY` (recoverable). (§2.1)
  2. Yes — after a test the backend stores `lastTest` and emits `nodesChanged`. The
     returned `NodeTestResult` is the same data; patch locally or wait for the event. (§2.1)
  3. Yes — `statusChanged.reason` carries short display-ready progress text during
     `connecting` / `reconnecting` / `disconnecting` / `failing_over` / `recovering`.
     Display only; never parse it. (§2.2)
  4. Polling `get_logs` every ~2 s while visible is fine (2000-entry ring buffer, `limit`
     capped at 1000). (§2.6)
  5. `connect` fails with `HELPER_NOT_INSTALLED` and the **state stays `disconnected`**
     (no `statusChanged`). Same for all preconditions: `NO_NODES`, `NO_NODE_SELECTED`,
     `BUSY`, `HELPER_VERSION_MISMATCH`, `HELPER_UNREACHABLE`. Only failures after
     `connecting` starts go to `error`. So the popover can show "helper missing" before a
     connect attempt. (§2.2)
