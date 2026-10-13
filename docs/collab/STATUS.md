# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** Windows test loop (B3.2 done, untested parts being tested by the Windows
  tester). Round 1 (`0e336ca`): 9 pass, F-1 (pipe access denied, blocked all TUN) fixed in
  SPEC 1.5.0 together with M-1, M-2, E-1 (Windows title bar) and checklist issues 1–9.
- **SPEC version:** 1.5.0
- **Real:** nodes, settings, system proxy mode (macOS), TUN mode via helper (macOS),
  helper status / install / uninstall (macOS, admin prompt), live node switch, test_node,
  logs, launch recovery (restore or TUN re-attach), `--restore-networking`, windows + tray,
  rules status, automatic failover.
- **Verified on real systems (macOS arm64, 2026-10-12):** harness H-1 (HY2 + REALITY, exit
  GB); networksetup round trip; owner-reported PASS for M-HELPER, M-TUN, M-IPV6, M-DNSLEAK,
  M-KILL, M-TUN-FORCEKILL, M-SYSPROXY, M-FAILOVER, M-LIFECYCLE.
- **Failover (B4):** implemented and M-FAILOVER passed. **Direct fallback when all nodes
  fail: implemented in SPEC 1.4.0** (unit-tested; real test pending — see TESTING
  M-DIRECTFALLBACK).
- **Lifecycle (B5):** launch at login, auto-connect, close-to-tray implemented (M-LIFECYCLE
  pending).
- **Rules:** bundled + automatic / manual updates (validated, atomic). TUN uses the
  helper's installed copy.
- **Diagnostics:** `export_diagnostics` real (redacted zip in Downloads).
- **Packaging (B6):** `npm run bundle:mac` / `bundle:win` (sing-box + helper as sidecars,
  rule-sets as resources) — macOS bundle verified locally. CI: tests + lint on every push to
  main; installers on `v*` tags or manual run (owner decision). README with Gatekeeper /
  SmartScreen steps.
- **Windows (all untested on a real PC):** system proxy (W-SYSPROXY); helper service +
  named pipe + TUN + WFP kill switch (B3.2, SPEC 1.4.2, ADR-0007; W-HELPER, W-TUN, W-KILL).
  CI (macOS + Windows) green: tests, clippy, lint, tsc. Windows checklist (TESTING §3,
  W-*) run by the Windows tester session; manual CI build can be Windows-only. SPEC 1.4.3:
  single instance; Windows uninstaller restores networking and removes the helper.
  SPEC 1.4.4: renamed to Lynnk (OS-visible names; identifiers unchanged).
  macOS re-run after the helper refactor: M-HELPER (reinstall), M-TUN, M-KILL PASS
  2026-10-13. Fixed: macOS helper install failed with "Operation not permitted" when the
  app/checkout lives in a privacy-protected folder (Desktop) — install files are now staged
  in a private temp dir first. Rust toolchain pinned to 1.99.0 (`rust-toolchain.toml`) so local
  and CI lints match. First CI run: macOS tests passed (clippy fixed since); Windows linking
  of the harness fixed (harness is macOS-only).
- **Known issues:** `npm run tauri dev` fails if another Vite server holds port 1420; use
  `cargo run -p edge-app`. The helper binary must be built (`cargo build -p edge-helper`)
  before Install works in dev.

## UI

- **Phase/step:** U4 — checking screens against the real backend (needs owner runs of the real app). U2, U3 complete.
- **SPEC version:** 1.5.0 (Windows Settings title bar; F-2 popover scrollbars fixed; app-styled scrollbars; dev routes `#popover` / `#settings`)
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`: all states,
  node sheet, motion 1–6); `settings` renders all nine designed sections (motion 7, 9).
- **Open requests:** none.
- **Known issues:** none known; popover show/hide behaviour in the real windows is untested on Windows.
