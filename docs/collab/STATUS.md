# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B1 — Interface contract (awaiting owner approval of SPEC v0.1.0)
- **SPEC version:** 0.1.0 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** `get_app_info`; input validation in `set_failover_settings` and `set_settings`;
  in-memory `delete_node` / `reorder_nodes` / `select_node` and mode setters (emit
  `nodesChanged` / `statusChanged`); `quit_app` (exits — no OS state exists yet).
- **Placeholder data:** `list_nodes` (two `[placeholder] …` nodes, fake values), `get_status`
  (disconnected), all settings getters (defaults), `get_rules_status`, `get_helper_status`
  (not installed), `get_logs` (empty). Nothing is persisted; restart resets it.
- **Stubbed (`NOT_IMPLEMENTED`):** import (links/clipboard/QR), add/update node, `test_node`,
  `connect`, `update_rules_now`, install/uninstall helper, `export_diagnostics`,
  `restore_networking`. `--restore-networking` CLI flag not yet implemented.
- **No events are emitted** except `nodesChanged` / `statusChanged` from the in-memory setters.
- **Known issues:** none.

## UI

- **Phase/step:** U0 — Orientation (done, awaiting owner review)
- **SPEC version:** 0.1.0
- **Real / stubbed:** no frontend code yet beyond the B0 template (`src/App.tsx`).
  Plan: `src/api/client.ts` wraps `bindings.ts`; `src/api/mock.ts` simulates the whole
  SPEC; `VITE_USE_MOCK=1` (default for `npm run dev`).
- **Open requests:** REQ-1 (popover + settings windows, tray, open-at-section),
  REQ-2 (latency in `TrafficStats`), REQ-3 (clarifications).
- **Known issues:** none.
