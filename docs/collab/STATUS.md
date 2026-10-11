# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B1.3 — REQ-1 windows + tray, and removal of deprecated restore
  `actions` (done, awaiting owner review). Next: **Phase B2** (core, system proxy mode).
- **SPEC version:** 1.0.0 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** `get_app_info`; input validation in `set_failover_settings` and `set_settings`;
  in-memory `delete_node` / `reorder_nodes` / `select_node` and mode setters (emit
  `nodesChanged` / `statusChanged`); `quit_app` (exits — no OS state exists yet);
  windows `popover` / `settings` + tray icon and menu (REQ-1; `hide_popover`,
  `open_settings`, `reveal_diagnostics`, `settingsSectionRequested`, `popoverShown`) —
  launches cleanly; hand-testing pending (TESTING.md M-SHELL);
  restore orchestration logic in `edge-core` (`restore.rs`, unit-tested with a fake
  restorer — no real OS restore steps yet).
- **Placeholder data:** `list_nodes` (two `[placeholder] …` nodes, fake values), `get_status`
  (disconnected), all settings getters (defaults), `get_rules_status`, `get_helper_status`
  (not installed), `get_logs` (empty). Nothing is persisted; restart resets it.
- **Stubbed (`NOT_IMPLEMENTED`):** import (links/clipboard/QR), add/update node, `test_node`,
  `connect`, `update_rules_now`, install/uninstall helper, `export_diagnostics`,
  `restore_networking`. `--restore-networking` CLI flag not yet implemented.
- `TrafficStats.latencyMs`, `Status.lastRestore`, `restoreReportChanged` exist in the
  contract; nothing emits them yet.
- **No events are emitted** except `nodesChanged` / `statusChanged` from the in-memory setters.
- **Known issues:** `npm run tauri dev` fails if another Vite server already holds port
  1420 (e.g. the UI preview); use `cargo run -p edge-app` against the running server.

## UI

- **Phase/step:** U1.2 — design tokens, fonts, frame/grid, token sheet (awaiting owner review)
- **SPEC version:** 0.3.0 (REQ-4 restore reports implemented in mock, store and debug views)
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). `src/api/mock.ts` simulates every SPEC command,
  event and error code, including per-item restore failures. Screens are temporary debug
  views (`src/dev/`); designed popover/settings come in U2/U3. Inside Tauri the `main`
  window shows the dev page until REQ-1 adds `popover` / `settings`.
- **Pending flags:** `// PENDING REQ-1` (window commands/events) — mock-only; real build
  returns `NOT_IMPLEMENTED`.
- **Open requests:** REQ-1 (accepted, backend B1.3 → SPEC v0.4.0).
- **Known issues:** none.
