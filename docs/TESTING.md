# EdgeClient — Testing

Results are recorded **redacted** (no hostnames, IPs, passwords, UUIDs, keys, short IDs or
links). Nothing here is "working" until a dated result row says so.

## 1. Automated tests (run on every change; CI from Phase B6)

```
cargo test --workspace     # Rust unit tests, incl. bindings-are-current and SPEC-version checks
npx tsc --noEmit -p .      # TypeScript check of the generated bindings (+ UI code)
```

| Area | Tests | Status |
|---|---|---|
| Product identity | `edge-core` `tests::*` | passing |
| Error-code catalogue | `edge-core` `error_code::tests::*` | passing |
| Share-link parsing (B2.1) | `edge-core` `link::tests::*` — owner example link shapes (HY2, REALITY), scheme aliases/case, default port, URL-encoded passwords and names (incl. UTF-8), `user:pass` auth, port hopping + `mport`, salamander obfs, bandwidth hints, every rejection rule (unknown/duplicate params, `pinSHA256`, `ech`, `pqv`, non-raw transport, non-REALITY security, wrong flow, bad UUID/pbk/sid/fp/port/host, bad `%` escapes), IPv6 literals parsed then rejected, multi-line input with line numbers / blanks / CRLF, **errors and `Debug` never contain secret values** | passing (fake values only) |
| Node store + preferences (B2.2) | `edge-app` `store::tests::*`, `store::prefs::tests::*` — import → persist → reload, configs round-trip to the parser's output, **`nodes.json` contains no secret values** and is mode 0600, duplicates, manual add/update with keep/set/clear secrets, field errors, delete/reorder/select, keychain save failure rolls back, **unreadable keychain is never overwritten**, corrupt files quarantined, partial settings files | passing (in-memory keychain fake) |
| Real OS keychain (B2.2) | `edge-app` `store::secrets::tests::real_keychain_roundtrip` — **opt-in** (`-- --ignored`), separate test service name, deletes its item | **passed** 2026-10-12, macOS arm64 (run by owner) |
| QR import (B2.3) | `edge-core` `qr::tests::*` — PNG + JPEG, small code inside a 3000×2000 screenshot, two different codes → `QR_TOO_MANY_CODES`, same code twice → OK, blank → `QR_NOT_FOUND`, garbage / empty / oversized → `QR_IMAGE_UNREADABLE`; `edge-app` `api::commands::tests::qr_sources` (data URL, base64, relative / missing / directory paths) | passing (generated images, fake links) |
| sing-box pin + verification (B2.4) | `edge-app` `core_bin::tests::*` — tamper detection, lock-file consistency, fetched binary verifies and reports the pinned version (skipped if not fetched); manual script checks recorded in ADR-0002 | passing (2026-10-12, macOS arm64, sing-box 1.14.3) |
| sing-box config generation (B2.5) | `edge-core` `singbox::tests::*` — selector/outbounds (HY2 domain+bootstrap, IP+hopping+obfs+bandwidth, VLESS REALITY), Rule vs Global, LAN allow/reject, DNS profiles (direct/bootstrap never via tunnel), snapshots, **`sing-box check` (1.14.3) on all 8 routing×DNS×LAN combinations**, negative controls (unknown type, `reality.spider_x` rejected) | passing (2026-10-12; `sing-box check` runs when the core is fetched) |
| Rule-sets (B2.6) | `edge-app` `rules::tests::*` — embedded manifest valid; copy + verify + repair tampered active file; refuse missing / tampered bundle; real fetched bundle matches manifest. Manual (2026-10-12): git blob hashes match GitHub API; `sing-box rule-set match`: baidu.com / qq.com ∈ geosite-cn, google.com ∉; 114.114.114.114 / 223.5.5.5 ∈ geoip-cn, 8.8.8.8 ∉ | passing |
| System proxy plans (B2.7) | `edge-core` `sysproxy::tests::*` — parsing real `networksetup` output, set/restore plans incl. PAC + auto-discovery, exact empty restore, user changes left alone, auth proxies refused | passing |
| System proxy executor (B2.7) | `edge-app` `net::osproxy::tests::*` — simulated `networksetup` (`FakeMac`): plan → apply → restore is exact, disabled services skipped, failure reported then retry succeeds, settings that do not stick detected | passing |
| Journal / restore / reports (B2.7) | `edge-app` `net::journal::tests::*`, `net::session::tests::*` — journal round trip, corrupt journal → `JOURNAL_CORRUPT` items, **partial failure keeps journal + persists `last_restore.json`, retry completes and cleans up** | passing |
| Redaction / logs (B2.7) | `edge-core` `redact::tests::*`, `edge-app` `net::logbuf::tests::*` — secrets, UUIDs, share links; harness mode hides hosts/IPs; log file contains no secrets | passing |
| Probe pipeline (B2.7) | `edge-app` `net::probe::tests::dead_node_fails_cleanly` — real sing-box from stdin config, Clash API, probes to an unroutable TEST-NET node fail with `PROBE_TIMEOUT` | passing (core fetched) |
| Real `networksetup` round trip (B2.7) | `edge-app` `net::osproxy::tests::real_networksetup_roundtrip` — **opt-in, changes real settings** on one service (default "Thunderbolt Bridge"), restores and compares | not run (owner) |
| Field validators / masking | `edge-core` `validate::tests::*`, `secret::tests::*` | passing |
| Restore orchestration (REQ-4) | `edge-core` `restore::tests::*` — all restored; not-needed is clean; **failing DNS restore → incomplete, reported per item**; a failure never stops later items; skipped = incomplete; firewall restored last; held state stays true for items that were not restored | passing (logic only, fake restorer) |
| Restore wire format (REQ-4) | `edge-app` `api::types::tests::restore_incomplete_error_carries_per_item_report` | passing |
| UI contract | `edge-app` `api::tests::bindings_are_current`, `spec_md_version_matches`, `api::types::tests::tagged_union_wire_shapes` | passing |
| Settings validation | `edge-app` `api::commands::tests::*` | passing |
| Popover placement (REQ-1) | `edge-app` `shell::position::tests::*` — macOS menu bar, right-edge clamp, Windows taskbar bottom/top/left/right, negative-origin second monitor, auto-hide fallback | passing (maths only) |

## 2. Planned macOS system tests (local only; owner runs root steps)

Each records before/after snapshots of: system proxy (all network services), routes
(`netstat -rn`), DNS (`scutil --dns`), IPv6 per service, pf anchors (`pfctl -sA`, our
anchor rules), helper/TUN state. Scheduled phase in brackets.

| ID | Test | Phase | Status |
|---|---|---|---|
| M-RESTORE-PARTIAL | **Partial restore failure (REQ-4).** With a debug-only fault-injection switch, make the DNS restore step fail during `disconnect`, then during `restore_networking`. Pass: both return `RESTORE_INCOMPLETE` with a 6-item report where `dns` is `failed` (`DNS_RESTORE_FAILED`) and the others are `restored`/`not_needed`; `Status.state = error`; `killSwitchActive`/`systemProxySet`/`tunActive` match the snapshot; `restoreReportChanged` fired; after app restart `lastRestore` is the same incomplete report; removing the fault and running Restore networking returns a full report, state `disconnected`, snapshot equals the pre-connect snapshot. Repeat with the firewall step failing (kill switch must still be reported active and actually block). | B2 (system proxy items), B3 (TUN/DNS/IPv6/firewall items) | not run |
| M-SHELL | **Windows and tray (REQ-1).** Launch: no Dock icon, tray icon present, no window visible. Left-click tray: popover appears directly below the icon, inside the screen, with a shadow; click elsewhere → hides; click tray while open → closes (does not flicker back). Esc in popover → hides (once the UI wires it). Right-click / Ctrl-click tray → menu with header, Connect, Node ▸, Routing ▸, Capture ▸, Restore networking, Settings…, Quit; choosing a node / mode updates the check marks; Connect shows an error (stub). Settings… → settings window focused; close button hides it; Quit exits. `popoverHidden` reaches the popover on every hide (blur, tray toggle, Esc, Settings…) — check in the UI dev overlay. Repeat on an external monitor. | B1.3 | not run |
| H-1 | **Real-server harness, system-proxy shape** (owner runs; see §4). Pass: every HY2 and REALITY link PASS (both probe targets, HTTP proxy via the local inbound), exit country = GB. | B2.7 | not run |
| M-SYSPROXY | **System proxy end to end** (see §5). Pass: proxies point to 127.0.0.1 while connected; a non-CN site loads via the UK exit and a CN site works; live node switch and Rule ↔ Global work; after Disconnect, after Quit, and after force-quit + relaunch (and after `--restore-networking`), the proxy snapshot equals the one taken before connecting. | B2.7 | not run |
| (others) | Connectivity, DNS leak, IPv6 bypass, kill switch, force-kill, quit/disconnect/direct snapshots, failover, network change — per brief §8 | B2–B4 | not run |

Fault injection is compiled only into debug builds (`cfg(debug_assertions)`) and is never
present in release builds; mechanism defined in the B2/B3 recovery ADR.

## 3. Windows manual checklist (owner runs on own PC; Windows version: TBD)

Fill in date, app version, SPEC version, Windows build, result.

| ID | Step | Expected | Result |
|---|---|---|---|
| W-SHELL | Launch the app. Left-click the tray icon (also from the overflow "^" area) with the taskbar at the bottom, then move the taskbar to the top / left / right if possible, and on a second monitor. | Popover appears next to the icon, fully on screen, never over the taskbar; clicking elsewhere hides it; clicking the icon while open closes it. Right-click shows the menu; Settings… opens the settings window; closing it hides it; Quit exits. | not run |
| W-RESTORE-PARTIAL | Using a **debug build**, enable the fault-injection switch for the DNS restore step (instructions added in B3). Connect in TUN mode, then Disconnect. Repeat with Settings → Restore networking. | Both show "Restore incomplete" with the per-item list: DNS failed, other items restored/not needed. App state shows Error, not Disconnected. Kill-switch / proxy / TUN indicators match reality (check `netsh interface ipv4 show dnsservers`, `route print`, WFP/firewall rules, Internet Options proxy). Restart the app: the same incomplete report is shown. Disable the fault, Restore networking: full report, state Disconnected, all settings equal the pre-connect values. | not run |
| W-RESTORE-PARTIAL-FW | Same, with the firewall/kill-switch removal step failing. | Kill switch reported active **and** still blocking proxied destinations; Restore networking after removing the fault clears it. | not run |

## 4. Real-server harness (H-1; owner runs, local only)

The harness reads the link files itself and prints only a redacted summary. Agents never
run it against the real link directory. In a terminal on the Mac:

```
cd /Users/rw/Desktop/VPN
npm run fetch-core
EDGE_TEST_LINKS=/Users/rw/.config/personal-edge-proxy npm run harness
```

Defaults: link files `iphone-hy2.txt` and `iphone-reality.txt` in that directory (override
with `EDGE_TEST_LINK_FILES=a.txt,b.txt`); DNS profile Mainland (`EDGE_TEST_DNS=overseas`
for the other); `EDGE_TEST_DETAILS=1` adds a redacted technical detail line per failure.
Exit code 0 = all PASS, 2 = some FAIL / SKIP, 1 = setup error. The table contains no
hostnames, IPs or credentials and can be pasted into the conversation.

Self-test on fake links (agent, 2026-10-12, `--links-dir <scratch> --files …`): two fake
nodes at unroutable TEST-NET addresses → both FAIL `PROBE_FAILED,HTTP_PROXY_FAILED`; an
unsupported `vmess://` line → SKIP `LINK_UNSUPPORTED_SCHEME`; output free of secrets, hosts
and IPs. This found and fixed one bug: the HTTP-proxy check first used `www.gstatic.com`,
which is in geosite-cn, so Rule mode sent it direct and it "passed" through a dead node.

## 5. System proxy end-to-end steps (M-SYSPROXY; owner runs)

1. Snapshot (read-only):
   `for s in Wi-Fi "Thunderbolt Bridge"; do for c in -getwebproxy -getsecurewebproxy -getsocksfirewallproxy -getproxybypassdomains -getautoproxyurl -getproxyautodiscovery; do networksetup $c "$s"; done; done > /tmp/before.txt`
2. Start the app (`cargo run -p edge-app`), import your nodes (popover → Import from
   clipboard), click Connect. Check `networksetup -getwebproxy Wi-Fi` shows
   `127.0.0.1:<port>`; browse a non-CN site (exit should be UK) and a CN site.
3. Switch node in the popover / tray (live); switch Rule ↔ Global (core restart).
4. Disconnect. Snapshot again to `/tmp/after.txt`; `diff /tmp/before.txt /tmp/after.txt`
   must print nothing.
5. Connect, then Quit from the tray → snapshot/diff must be empty.
6. Connect, then force-quit (`pkill -9 -f edge-app`). Proxies now point at a dead port
   (expected: fails closed). Relaunch → popover/status shows "Previous network settings
   restored" → diff empty.
7. Connect, force-quit, then run `./target/debug/edge-app --restore-networking` instead of
   relaunching → prints the per-item report, exit 0 → diff empty.
