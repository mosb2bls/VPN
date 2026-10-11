# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B1.2 — REQ-4 restore reporting (done, awaiting owner review).
  Next: **B1.3 = REQ-1** (popover/settings windows, tray; SPEC v0.4.0), then Phase B2.
- **SPEC version:** 0.3.0 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** `get_app_info`; input validation in `set_failover_settings` and `set_settings`;
  in-memory `delete_node` / `reorder_nodes` / `select_node` and mode setters (emit
  `nodesChanged` / `statusChanged`); `quit_app` (exits — no OS state exists yet);
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
- **Known issues:** UI files fail `tsc` against SPEC 0.3.0 until the mock/client add the new
  fields (7 errors, UI-owned). Runtime/dev unaffected.

## UI

- **Phase/step:** U1.1 — API client + full mock + dev state switcher (awaiting owner review)
- **SPEC version:** 0.2.0
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). `src/api/mock.ts` simulates every SPEC command,
  event and error code. Screens are temporary debug views (`src/dev/`); the designed
  popover/settings come in U2/U3. Inside Tauri the current `main` window shows the dev
  page without the mock panel until REQ-1 adds `popover` / `settings`.
- **Pending flags:** `// PENDING REQ-1` (window commands/events) — mock-only; real build
  returns `NOT_IMPLEMENTED`.
- **Open requests:** REQ-1 (accepted, backend B1.2), REQ-4 (restore-report clarification).
- **Known issues:** none.
