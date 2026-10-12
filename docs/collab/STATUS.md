# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B3.1 — macOS privileged helper + TUN mode + pf kill switch + re-attach
  (implemented and unit-tested; **awaiting owner install + real TUN tests**, TESTING §6).
  Next: Windows service / WFP (B3.2) after the macOS results; then failover (B4).
- **SPEC version:** 1.4.1 (direct fallback → UNPROTECTED state; ALL NODES DOWN; Windows: TUN disabled/explained)
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
  CI (macOS + Windows) green: tests, clippy, lint, tsc. Rust toolchain pinned to 1.99.0 (`rust-toolchain.toml`) so local
  and CI lints match. First CI run: macOS tests passed (clippy fixed since); Windows linking
  of the harness fixed (harness is macOS-only).
- **Known issues:** `npm run tauri dev` fails if another Vite server holds port 1420; use
  `cargo run -p edge-app`. The helper binary must be built (`cargo build -p edge-helper`)
  before Install works in dev.

## UI

- **Phase/step:** U4 — checking screens against the real backend (needs owner runs of the real app). U2, U3 complete.
- **SPEC version:** 1.4.1 (direct fallback → UNPROTECTED state; ALL NODES DOWN; Windows: TUN disabled/explained)
- **Real / stubbed:** `src/api/client.ts` wraps `bindings.ts` (real inside Tauri, mock in a
  browser; `VITE_USE_MOCK` overrides). Mock simulates every SPEC command, event and error
  code. The `popover` window renders the designed popover (`src/popover/`: all states,
  node sheet, motion 1–6); `settings` renders all nine designed sections (motion 7, 9).
- **Open requests:** none.
- **Known issues:** none known; popover show/hide behaviour in the real windows is untested on Windows.
