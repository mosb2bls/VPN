# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B3.1 — macOS privileged helper + TUN mode + pf kill switch + re-attach
  (implemented and unit-tested; **awaiting owner install + real TUN tests**, TESTING §6).
  Next: Windows service / WFP (B3.2) after the macOS results; then failover (B4).
- **SPEC version:** 1.3.2 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** nodes, settings, system proxy mode (macOS), TUN mode via helper (macOS),
  helper status / install / uninstall (macOS, admin prompt), live node switch, test_node,
  logs, launch recovery (restore or TUN re-attach), `--restore-networking`, windows + tray,
  rules status.
- **Verified on real systems:** harness H-1 (HY2 + REALITY, exit GB); networksetup round
  trip. **Not yet:** M-SYSPROXY, M-HELPER, M-TUN, M-IPV6, M-DNSLEAK, M-KILL, M-TUN-FORCEKILL.
- **Stored but not applied yet:** launch at login, auto-connect, close-to-tray (B5);
  failover between nodes (B4).
- **Stubbed:** `update_rules_now`, `export_diagnostics`. Windows: system proxy, helper.
- **Known issues:** `npm run tauri dev` fails if another Vite server holds port 1420; use
  `cargo run -p edge-app`. The helper binary must be built (`cargo build -p edge-helper`)
  before Install works in dev.

## UI

- **Phase/step:** U3 — all nine settings sections built (awaiting owner review). U2 complete.
- **SPEC version:** 1.3.2
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`: all states,
  node sheet, motion 1–6); `settings` renders all nine designed sections (motion 7, 9).
- **Open requests:** none.
- **Known issues:** none known; popover show/hide behaviour in the real windows is untested on Windows.
