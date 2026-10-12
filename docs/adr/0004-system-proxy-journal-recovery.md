# ADR-0004: System proxy mode — core process, journal, restore, recovery (macOS)

- Status: implemented (Phase B2.7, 2026-10-12). Unit-tested with a simulated
  `networksetup`; real `networksetup` behaviour verified by hand on an unused service;
  **end-to-end connect/disconnect on the owner's Mac and the real-server harness are
  pending owner runs** (TESTING.md M-SYSPROXY, H-1).

## Context

Brief §7.2 / §7.7: unprivileged sing-box with a local mixed inbound; set the OS proxy on
connect and restore the exact previous settings on disconnect (macOS per network service);
journal before any OS change; restore on normal disconnect/quit; on launch re-attach or
restore; `restore_networking` + a CLI flag that works without the UI. REQ-4: one per-item
restore report everywhere; partial failure is `RESTORE_INCOMPLETE`; true state reported.

## Decisions

**Core process.** The bundled, hash-verified sing-box (ADR-0002) runs as a child process
with `run -c stdin`: the generated config — which contains credentials — is written to its
stdin and **never touches disk** (verified: sing-box reports "decode config at stdin").
Working dir `<data>/run` (0700). Output lines are redacted (`edge_core::redact`: all node
secrets, the Clash secret, UUID-shaped tokens, share links) before entering the log ring
buffer / rotating file `<data>/logs/edgeclient.log` (0600, 1 MiB × 4).

**Control.** Clash API on `127.0.0.1:<random port>` with a random 128-bit secret per
start; the API client never uses a proxy. Node switching = `PUT /proxies/proxy`. Health =
`GET /proxies/<node>/delay` to both probe targets (a round passes if one passes).

**System proxy (macOS).** `/usr/sbin/networksetup`, argv only, no shell. For every
enabled network service: snapshot web / secure-web / SOCKS (enabled, server, port,
authenticated), bypass list, PAC (url, enabled), auto-discovery; then set all three proxies
to `127.0.0.1:<mixed port>`, append our bypass entries (loopback, `*.local`, and LAN ranges
if allowed) to the user's list, and turn off PAC / auto-discovery if they were on (they
would override manual proxies). Read back and verify. Services with an *authenticated*
proxy are refused (we cannot read its password, so we could not restore it).

Verified on the owner's Mac (macOS 27, admin user, no sudo), on the unused "Thunderbolt
Bridge" service: `-setwebproxy <svc> "" 0 off` + `-setwebproxystate <svc> off` restores an
empty, disabled proxy exactly; `-setproxybypassdomains <svc> Empty` clears the list. The
`networksetup(8)` man page: admin privileges are enough unless "require an administrator
password to access system-wide preferences" is enabled (then root is needed and setting
the proxy fails with `SYSTEM_PROXY_SET_FAILED`).

**Restore only what is still ours.** Each item is put back only if it still holds the value
we set; items the user changed while connected are left alone and named in the report's
`action` text. Restore is idempotent.

**Journal** `<data>/journal.json` (0600, atomic): written empty before anything, then
updated with the core PID + executable path after start, then with every service's
previous settings **before** the proxy is changed. Deleted only after a complete restore.
On partial failure it is kept, the per-item report is persisted to
`<data>/last_restore.json`, the state is `error` (`RESTORE_INCOMPLETE`) and
`systemProxySet` stays true.

**Order.** Connect: journal → core → journal(pid) → API ready → snapshot → journal(prev) →
set proxy → read back → health check → `connected` / `degraded`. Any failure after the
journal exists rolls back through the normal restore. Disconnect / restore / quit /
direct-mode / capture-mode switch: restore the proxy first (apps stop using the core),
then stop the core.

**Recovery.** System-proxy sessions are **not re-attached**: the app owns the core, so after
an app crash the proxy points at a dead port (traffic fails closed). On launch, if a journal
exists: `recovering` → kill the stale core (only if that PID still runs the recorded
executable) → restore → `disconnected` or `error`, reported via `lastRecovery`,
`recoveryCompleted`, `restoreReportChanged`. `EdgeClient --restore-networking` runs the same
restore without the UI (exit 0 = nothing to do / fully restored, 2 = incomplete).

**Core crash while connected.** Restart with backoff 1, 2, 4, 8, 16, 30 s… for up to 5
minutes on the same local port (the system proxy keeps pointing at it, so proxied traffic
fails closed meanwhile; state `reconnecting`). Gives up → `error` (`CORE_CRASHED`), or — only
if the user enabled direct fallback — restores networking.

**Mode changes while connected.** Routing / DNS profile: restart the core with a new config
on the same port (system proxy untouched). Direct: full restore → state `direct`. Capture
mode: full teardown, then connect in the new mode (TUN → `HELPER_NOT_INSTALLED` until B3).

**Node tests** (`test_node`, harness): a temporary core for one node on random ports, no OS
changes; ≤ 3 in parallel; exit country from `https://1.1.1.1/cdn-cgi/trace` (`loc=` only;
the IP is never stored).

## Known limitations

- Windows system proxy (WinINet + notify) and Windows stale-process cleanup: Windows phase.
- Node edits while connected take effect on the next connect / mode change (the running
  core keeps its config); deleting the active node is refused (`BUSY`).
- `www.gstatic.com` is in geosite-cn, so in Rule mode browsers reach it directly. Node
  probes are unaffected (they target the node explicitly), but end-to-end checks must use a
  non-CN target (the harness uses `http://cp.cloudflare.com/`).
- Automatic failover between nodes: B4 (here only `connected` ↔ `degraded`).

## Addendum (2026-10-12): Windows system proxy

- WinINet per-user settings of the default (LAN) connection via
  `InternetQueryOptionW` / `InternetSetOptionW(INTERNET_OPTION_PER_CONNECTION_OPTION)`
  (flags, proxy server, bypass, PAC URL), then `INTERNET_OPTION_SETTINGS_CHANGED` +
  `INTERNET_OPTION_REFRESH` — the documented API behind "Proxy settings" in Windows; it
  updates the per-connection settings (including the binary DefaultConnectionSettings) rather
  than writing registry values directly. Reference: Microsoft Learn, WinINet
  `INTERNET_PER_CONN_OPTION_LISTW`, "Option Flags".
- We set: flags DIRECT|PROXY (PAC and auto-detect off while connected), server
  `127.0.0.1:<port>`, bypass `localhost;127.*;<local>` + LAN ranges if allowed. Previous
  values are journaled first and restored exactly; if the current settings are no longer
  ours (user changed them), they are left alone.
- Code: pure decisions `edge_core::winproxy`; OS calls `crates/winsys` (no C toolchain
  needed, compile-checked from macOS). **Untested on Windows** — checklist W-SYSPROXY.
- Not covered: per-dial-up / VPN-connection proxy settings (only the default LAN
  connection), WinHTTP (`netsh winhttp`) used by some services — documented limitation
  of system-proxy mode.
