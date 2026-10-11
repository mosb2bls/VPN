# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B2.2 — node store (keychain + JSON) and preferences persistence, node
  commands wired (done, awaiting owner review). Next: B2.3 QR import; then sing-box
  download/verify script, config generation, system proxy mode, harness.
- **SPEC version:** 1.1.0 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Real:** node list / import (text + clipboard) / add / update / delete / reorder /
  select, persisted (`nodes.json` + one OS keychain item); all settings getters/setters
  persisted (`settings.json`); mode setters persisted; `get_app_info`; windows + tray
  (REQ-1); link parsing/validation; restore orchestration logic (unit-tested, no OS steps).
- **Stored but not applied yet:** launch at login, auto-connect, close-to-tray (B5);
  failover/safety settings (B3/B4).
- **Placeholder data:** `get_rules_status`, `get_helper_status` (not installed),
  `get_logs` (empty). No placeholder nodes any more.
- **Stubbed (`NOT_IMPLEMENTED`):** `import_qr_image`, `test_node`, `connect`,
  `update_rules_now`, install/uninstall helper, `export_diagnostics`,
  `restore_networking`. `--restore-networking` CLI flag not yet implemented.
- **Events emitted:** `nodesChanged`, `statusChanged`, `errorRaised` (tray failures,
  startup load warnings — may fire before the UI listens; logging comes in B2).
- **Known issues:** `npm run tauri dev` fails if another Vite server holds port 1420; use
  `cargo run -p edge-app`. Real-keychain test is opt-in and not yet run.

## UI

- **Phase/step:** U2.2 — every popover state (awaiting owner review)
- **SPEC version:** 1.0.1 (mock import parser follows §2.1.1; `pinSha256` must be null)
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`, all states);
  the `settings` window still renders the U1.1 debug view until U3. Motion comes in U2.4.
- **Open requests:** none.
- **Known issues:** popover inside the real Tauri window not yet checked (`cargo run -p edge-app`).
