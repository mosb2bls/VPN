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
- Status: open
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
- Response:

## REQ-2: Latency in live stats
- From: ui
- Date: 2026-10-11
- Status: open
- Need: The popover shows live up / down / **latency**. `TrafficStats` has no latency;
  `healthCheck` has per-probe latency but only every ~10 s and isn't available on popover
  open (it's an event, there is no getter).
- Proposed shape: add `latencyMs: number | null` to `TrafficStats` = median latency of the
  most recent passing health round on the active node (null if none yet / last round
  failed). Additive → minor bump.
- Response:

## REQ-3: Clarifications for node testing, progress and logs
- From: ui
- Date: 2026-10-11
- Status: open
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
- Response:
