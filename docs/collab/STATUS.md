# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B2.6 — bundled, pinned rule-sets (done, awaiting owner review). Next:
  B2.7 system proxy mode (core process, Clash API, OS proxy + journal/restore); B2.8
  real-server harness; rule auto-update after that.
- **SPEC version:** 1.3.0 (`docs/SPEC.md`, `src/api/bindings.ts`)
- **Core + rules:** `npm run fetch-core` installs the verified sing-box binary into
  `src-tauri/binaries/` and the rule-sets into `resources/rules/` (both gitignored). Not
  required to build; required to connect (B2.7+). Without the rules, startup emits
  `errorRaised(RULES_MISSING)`.
- **Real:** node list / import (text + clipboard + QR image) / add / update / delete / reorder /
  select, persisted (`nodes.json` + one OS keychain item); all settings getters/setters
  persisted (`settings.json`); mode setters persisted; `get_app_info`; windows + tray
  (REQ-1); link parsing/validation; restore orchestration logic (unit-tested, no OS steps).
- **Stored but not applied yet:** launch at login, auto-connect, close-to-tray (B5);
  failover/safety settings (B3/B4).
- **Rules:** `get_rules_status` real (bundled set). `update_rules_now` stubbed.
- **Placeholder data:** `get_helper_status` (not installed),
  `get_logs` (empty). No placeholder nodes any more.
- **Stubbed (`NOT_IMPLEMENTED`):** `test_node`, `connect`,
  `update_rules_now`, install/uninstall helper, `export_diagnostics`,
  `restore_networking`. `--restore-networking` CLI flag not yet implemented.
- **Events emitted:** `nodesChanged`, `statusChanged`, `errorRaised` (tray failures,
  startup load warnings — may fire before the UI listens; logging comes in B2).
- **Known issues:** `npm run tauri dev` fails if another Vite server holds port 1420; use
  `cargo run -p edge-app`. Real-keychain test passed (owner, 2026-10-12).

## UI

- **Phase/step:** U2.4.3 — motion moment 3: failover (awaiting owner review)
- **SPEC version:** 1.3.0 (`RULES_MISSING` in mock; rules status shaped like the real bundle)
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`: all states,
  node sheet, open/hide motion); `settings` still renders the U1.1 debug view until U3.
- **Open requests:** none.
- **Known issues:** none known; popover show/hide behaviour in the real windows is untested on Windows.
