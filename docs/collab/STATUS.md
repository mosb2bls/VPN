# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B2.7 — system proxy mode on macOS (core runner, Clash API, journal,
  exact restore, recovery, `--restore-networking`) + real-server harness (done; awaiting
  owner runs of the harness and the end-to-end test). Next: B3 (helper / TUN) after the
  owner's results; rule auto-update and Windows system proxy later in plan.
- **SPEC version:** 1.3.1 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** nodes (import text / clipboard / QR, add, update, delete, reorder, select —
  persisted, keychain); all settings; **connect / disconnect / restore_networking / quit /
  mode switches in system proxy mode on macOS**; live node switching; `test_node`
  (temporary core); `get_logs` (redacted); launch recovery from the journal; windows + tray;
  rules status.
- **Untested against the real server / real Mac settings:** everything in the line above
  that touches the network or OS (owner runs H-1 and M-SYSPROXY in TESTING.md).
- **Stored but not applied yet:** launch at login, auto-connect, close-to-tray (B5); failover
  between nodes (B4; health checks run and drive connected ↔ degraded only).
- **Stubbed (`NOT_IMPLEMENTED`):** `update_rules_now`, install/uninstall helper,
  `export_diagnostics`. TUN → `HELPER_NOT_INSTALLED`. Windows system proxy → error.
- **Known issues:** `npm run tauri dev` fails if another Vite server holds port 1420; use
  `cargo run -p edge-app`.

## UI

- **Phase/step:** U2 complete (popover: all states, node sheet, motion 1–6); starting U3
- **SPEC version:** 1.3.1
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`: all states,
  node sheet, open/hide motion); `settings` still renders the U1.1 debug view until U3.
- **Open requests:** none.
- **Known issues:** none known; popover show/hide behaviour in the real windows is untested on Windows.
