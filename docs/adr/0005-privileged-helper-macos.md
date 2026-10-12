# ADR-0005: Privileged helper for TUN mode (macOS)

- Status: implemented (Phase B3.1, 2026-10-12). Unit-tested; **not yet installed or run on
  a real Mac** — install and TUN tests need the owner (admin password). Windows service:
  Windows phase.

## Context

Brief §7.2: the UI process stays unprivileged; a privileged helper runs sing-box with TUN.
macOS: root launchd daemon installed once via an admin prompt. Minimal, authenticated local
IPC (Unix socket with restricted permissions). Only structured requests (start, stop,
status, restore, kill switch). Only runs the bundled, checksum-verified sing-box with
validated configs. Never executes arbitrary commands or paths. ADR required.

## Decisions

**Install mechanism.** `install_helper` runs the helper binary's own `install` subcommand as
root through the standard macOS administrator prompt
(`osascript -e 'do shell script "…" with administrator privileges'`; every argument
single-quoted, then AppleScript-escaped — unit-tested against injection). Chosen over
`SMAppService` / `SMJobBless` because those require a code-signed app with a provisioning
profile / signing requirements, and builds are unsigned (brief §10 B6). Cancel →
`HELPER_INSTALL_CANCELLED`.

`install` (as root): refuses unless the given sing-box matches the SHA-256 **embedded in the
helper at build time** (same pin as the app, ADR-0002) and the rule-sets match the embedded
manifest; copies itself to `/Library/PrivilegedHelperTools/io.github.mosb2bls.edgeclient.helper`
and sing-box to `…/io.github.mosb2bls.edgeclient.sing-box` (root:wheel 0755); rules to
`/Library/Application Support/io.github.mosb2bls.edgeclient.helper/rules` (dir root 0700);
records the installing user's uid in `helper.json` (root 0600); writes
`/Library/LaunchDaemons/io.github.mosb2bls.edgeclient.helper.plist` (RunAtLoad, KeepAlive)
and `launchctl bootstrap system`. After install, nothing the helper runs or reads is
user-writable. `uninstall` (as root, the installed binary): ends any session, removes the
kill switch, `launchctl bootout`, deletes all files.

**IPC.** Unix socket `/var/run/io.github.mosb2bls.edgeclient.helper.sock`, chowned to the
allowed uid, mode 0600, **and** every connection's peer uid checked with `getpeereid` (must
be the allowed uid or root). One JSON request per connection (max 1 MiB, 10 s read
timeout), a closed `serde` enum: `ping`, `status`, `start`, `stop`, `restore`. Anything
else → `INVALID_ARGUMENT`. There is no request that takes a path, a command, or a
sing-box config.

**Validated configs.** `start` carries a `TunProfile` (nodes, selected, routing, DNS
profile, allow-LAN, kill-switch flag). The helper re-validates **every field** with the same
validators as link import (`WireNode::to_config`), checks node ids (`[A-Za-z0-9-]`, ≤ 64),
and **generates the sing-box config itself** with `edge_core::singbox::generate`
(`Capture::Tun`) — the same generator that is checked with `sing-box check` in tests. The
config reaches sing-box on stdin; secrets are never logged (helper log is redacted) or
persisted.

**Core.** Only `…/io.github.mosb2bls.edgeclient.sing-box`, re-hashed before every start.
Clash API on 127.0.0.1 with a fresh random port + secret per start, returned to the app
(the app uses it for node switching, health checks and stats exactly as in system-proxy
mode).

**TUN config** (sing-box 1.14 docs): `tun` inbound, address `172.19.0.1/30` +
`fdfe:dcba:9876::1/126`, `auto_route`, `strict_route`; DNS is hijacked by default when
`auto_route` is on (1.14 `dns_mode`); `route.auto_detect_interface` binds outbound sockets
to the physical interface (no loops). IPv6 is captured by the TUN and then **rejected** by
the route rules (fast fail) — no OS IPv6 setting is changed, so `ipv6` restore items are
`not_needed`. No fake-IP (real IPs keep `geoip-cn` effective; brief allows either).

**Supervision and recovery.** The helper waits on its sing-box child; unexpected exit →
kill switch (ADR-0006) → restart with backoff for up to 5 min → release the kill switch when
the core is back. A root-owned journal (`…helper/journal.json`) records the core pid and pf
token. On helper start, a journal means the previous helper died mid-session (its core died
with it): clean up (kill stale core only if its path is ours, remove the pf anchor) and
report it in `status.startupRecovery`. On SIGTERM (launchd stop / uninstall) the helper ends
the session cleanly.

**App side.** The app journal records `tun: true` before asking the helper to start. On
app crash / force-quit the TUN session keeps running in the helper; on next launch the app
**re-attaches** (`RecoveryOutcome::reattached`) if the helper still reports it running,
otherwise it restores through the helper. Disconnect / quit / restore call the helper's
`restore` and map its per-item results into the REQ-4 report (`tun_session`, `routes`,
`dns`, `ipv6`, `firewall`); if the helper is unreachable those items are `skipped`
(`HELPER_UNREACHABLE`) and the restore is incomplete — never reported as success.

**Version check.** `get_helper_status.needsUpgrade` and `connect` (TUN) compare the
helper's version, protocol and core hash with the app's; mismatch →
`HELPER_VERSION_MISMATCH` (reinstall).

## Known limitations / risks

- The install step runs the not-yet-installed helper binary (from the app bundle / build
  dir) as root once; that is inherent to an admin-prompt install of unsigned software.
- macOS may show "background item added" notifications for the daemon.
- `strict_route` behaviour on macOS is not documented by sing-box; DNS / IPv6 leak tests
  (TESTING.md M-DNSLEAK, M-IPV6) decide whether more is needed.
- Server IPs for the kill switch are resolved once at session start (system resolver); if
  the server's IP changes mid-session the kill switch would block it until reconnect.
