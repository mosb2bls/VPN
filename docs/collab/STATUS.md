# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B2.1 — share-link parsing + validation in `edge-core` (done, awaiting
  owner review). Next: B2.2 node store (keychain + settings JSON) and wiring
  `import_links` / `add_node_manual` / `update_node` / `list_nodes` to real data.
- **SPEC version:** 1.0.1 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** `get_app_info`; input validation in `set_failover_settings` and `set_settings`;
  in-memory `delete_node` / `reorder_nodes` / `select_node` and mode setters (emit
  `nodesChanged` / `statusChanged`); `quit_app` (exits — no OS state exists yet);
  windows `popover` / `settings` + tray icon and menu (REQ-1; `hide_popover`,
  `open_settings`, `reveal_diagnostics`, `settingsSectionRequested`, `popoverShown`) —
  launches cleanly; hand-testing pending (TESTING.md M-SHELL);
  link parsing / validation / masking in `edge-core` (not yet wired to commands);
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

- **Phase/step:** U2.1 — popover static layout, OFF state (awaiting owner review)
- **SPEC version:** 1.0.0 (window commands/events real; no PENDING flags)
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`); the `settings`
  window still renders the U1.1 debug view until U3.
- **Open requests:** none.
- **Known issues:** popover states other than OFF use provisional styling until U2.2.
