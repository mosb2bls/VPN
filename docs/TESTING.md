# Lynnk — Testing

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
| Real `networksetup` round trip (B2.7) | `edge-app` `net::osproxy::tests::real_networksetup_roundtrip` — **opt-in, changes real settings** on one service (default "Thunderbolt Bridge"), restores and compares | **passed** 2026-10-12 (owner): apply ok, restored exactly |
| Failover engine (B4) | `edge-core` `failover::tests::*` — 3 failed rounds → switch with the exact reason text; **one website failing never switches**; intermittent failures reset the streak; return needs dwell + 10 min continuous primary health (a single failure restarts the timer); all-failing reported once and blocks; **direct fallback engages and leaves when a node recovers**; rescue switch inside dwell; rate limit; disabled; no fallback | passing |
| Rule updates (B2) | `edge-app` `rules_update::tests::*` — real sing-box: bundled rules validate, swapped / corrupt files rejected, failed swap restores old files; due-time logic; `rules::tests::*` active manifest. Opt-in real download `real_rules_update` (GitHub): **passed** 2026-10-12 (agent) — newest commits equal the bundle → "already current" | passing |
| Diagnostics (B2) | `edge-app` `diagnostics::tests::zip_contains_no_secrets_or_servers` — fake secrets, UUID, short ID, REALITY key, server host / IP and SNIs all absent from the zip; never overwrites | passing |
| Packaging (B6) | Local `tauri build --config src-tauri/tauri.bundle.macos.json --bundles app` (2026-10-12, macOS arm64): `Lynnk.app/Contents/MacOS/{edge-app, edge-helper, sing-box}`, rules in `Contents/Resources/resources/rules`; bundled sing-box hash = pinned hash; release app starts with no startup warnings (rules found); `--restore-networking` works from the bundle. CI workflow `.github/workflows/ci.yml` written, **not yet run** | partial |
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
| H-1 | **Real-server harness, system-proxy shape** (owner runs; see §4). Pass: every HY2 and REALITY link PASS (both probe targets, HTTP proxy via the local inbound), exit country = GB. | B2.7 | **PASS** 2026-10-12 (owner, macOS arm64, sing-box 1.14.3, DNS Mainland, Rule): HY2 — gstatic 58 ms, cloudflare 152 ms, HTTP proxy ok 40 ms, exit GB · REALITY Vision — gstatic 58 ms, cloudflare 61 ms, HTTP proxy ok 82 ms, exit GB. First confirmation that the sing-box HY2 client works with the Xray Hysteria inbound. |
| M-SYSPROXY | **System proxy end to end** (see §5). Pass: proxies point to 127.0.0.1 while connected; a non-CN site loads via the UK exit and a CN site works; live node switch and Rule ↔ Global work; after Disconnect, after Quit, and after force-quit + relaunch (and after `--restore-networking`), the proxy snapshot equals the one taken before connecting. | B2.7 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-HELPER | **Helper install / status / uninstall** (TESTING §6). Pass: admin prompt appears once; `getHelperStatus` → installed + running; files root-owned (`ls -l /Library/PrivilegedHelperTools/io.github.mosb2bls.edgeclient.*`, socket `/var/run/io.github.mosb2bls.edgeclient.helper.sock` owned by you, mode `srw-------`); uninstall removes everything. Cancel at the prompt → "installation was cancelled". | B3.1 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-TUN | **TUN connectivity**: helper installed, capture mode TUN, connect with HY2, then REALITY. Pass: `ifconfig` shows a utun with 172.19.0.1; `curl -s https://1.1.1.1/cdn-cgi/trace` (no proxy settings) shows `loc=GB`; a CN site works; a terminal tool that ignores system proxy (e.g. `curl` with no proxy env) also goes via UK; Disconnect removes the utun and routes: `netstat -rn \| grep -v UHLW \| diff <(grep -v UHLW /tmp/routes-before.txt) -` prints nothing (`UHLW` lines are LAN neighbour/ARP cache entries whose timers change by themselves). | B3.1 | **PASS** 2026-10-12; **re-run PASS 2026-10-13** after the B3.2 helper refactor (helper reinstalled; HY2 + REALITY: utun 172.19.0.1, `loc=GB`, CN 200, routes same; owner-run, output seen) |
| M-IPV6 | **IPv6 bypass (TUN)**: while connected, `curl -6 -m 5 https://ipv6.google.com` fails immediately (rejected), and `sudo tcpdump -ni en0 ip6 and not icmp6` shows no public IPv6 traffic leaving the physical interface during the attempt. | B3.1 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-DNSLEAK | **DNS leak (TUN, Mainland and Overseas)**: `sudo tcpdump -ni en0 'udp port 53 or tcp port 53 or port 853 or host 1.1.1.1 or host 8.8.8.8'` while `dscacheutil -flushcache; curl -s -o /dev/null https://www.google.com; curl -s -o /dev/null https://www.baidu.com`. Pass: **no** plaintext DNS and **no** traffic to 1.1.1.1 / 8.8.8.8 on en0 (proxied DoH goes through the tunnel); only TLS to 223.5.5.5 (Mainland) or 9.9.9.9 (Overseas) for direct / bootstrap lookups. Repeat in system proxy mode for reference (system DNS is visible there — expected, documented). | B3.1 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-KILL | **Kill switch**: while connected in TUN mode and a download/stream is running, `sudo pkill -9 -f io.github.mosb2bls.edgeclient.sing-box`. Pass: within ~1 s `sudo pfctl -a com.apple/250.EdgeClient -s rules` shows the rules; `curl -m 5 https://www.google.com` fails fast (no direct leak); the app shows Reconnecting + kill switch; within a few seconds the core is back, the rules are gone, traffic resumes. Disconnect → no rules, `pfctl -s References` no longer lists our token. **Timing:** the core is back within ~1–2 s, so a single `pkill` then typing `pfctl` usually misses the window — keep it down instead: `sudo -v && (for i in $(seq 1 16); do sudo pkill -9 -f io.github.mosb2bls.edgeclient.sing-box; sleep 0.5; done) & sleep 3; sudo pfctl -a com.apple/250.EdgeClient -s rules 2>/dev/null; curl -m 3 -s -o /dev/null -w "%{http_code}\n" https://www.google.com; wait` (rules listed, `000`). Helper log evidence: `sudo grep -E "exited unexpectedly\|kill switch\|restarted" "/Library/Application Support/io.github.mosb2bls.edgeclient.helper/helper.log" \| tail -6`. | B3.1 | **PASS** 2026-10-12; **re-run PASS 2026-10-13** after the B3.2 helper refactor (owner-reported) |
| M-TUN-FORCEKILL | **App force-quit in TUN**: connected in TUN, `pkill -9 -f target/debug/edge-app`. Pass: traffic keeps flowing (helper keeps the session); relaunch → "Re-attached to the running TUN session", Disconnect works. Also: force-quit, then `launchctl kickstart -k system/io.github.mosb2bls.edgeclient.helper` (helper restart) → helper cleans up (no utun, no pf rules); relaunch app → restore report, networking normal. | B3.1 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-FAILOVER | **Failover with UDP blocked** (system proxy or TUN, HY2 selected, REALITY present). Block UDP to the server port: `echo "block drop out quick proto udp to any port 24443" \| sudo pfctl -a com.apple/251.EdgeTest -f - && sudo pfctl -E` (note the printed token). Pass: within ~30 s a failover event "HY2: 3 consecutive probe failures — UDP may be blocked on this network", active node REALITY, browsing keeps working. Unblock: `sudo pfctl -a com.apple/251.EdgeTest -F all && sudo pfctl -X <token>`. Pass: after ≥ 10 min of HY2 health (and ≥ 5 min since the switch) "HY2: healthy for 10 min — switching back". | B4 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-LIFECYCLE | Settings → Startup: enable launch at login → `ls ~/Library/LaunchAgents` shows the app's plist; log out/in → app starts in the tray, no window. Enable auto-connect, quit, relaunch → connects with the last node / routing / capture mode. Turn close-to-tray off → closing the settings window quits and networking is restored (proxy snapshot diff empty). Disable launch at login → plist removed. | B5 | **PASS** 2026-10-12 (owner-reported, macOS arm64; steps as written in this table / §5–6, no output pasted) |
| M-DIRECTFALLBACK | Settings → Safety: direct fallback ON, reconnect. Block both nodes: `printf "block drop out quick proto udp to any port 24443\nblock drop out quick proto tcp to <server> port 443\n"` … (or block the server IP entirely) via a test anchor as in M-FAILOVER. Pass: after ~30 s "all nodes are failing — direct fallback is on…", banner "unprotected", browsing works directly (exit country = your local one). Unblock → back through a node within ~10 s, exit GB. With the setting OFF the same block → "traffic is blocked", browsing fails. | B4 | not run |
| (others) | Connectivity, DNS leak, IPv6 bypass, kill switch, force-kill, quit/disconnect/direct snapshots, failover, network change — per brief §8 | B2–B4 | not run |

Fault injection is compiled only into debug builds (`cfg(debug_assertions)`) and is never
present in release builds; mechanism defined in the B2/B3 recovery ADR.

## 3. Windows manual checklist (Windows tester session on the owner's Windows 11 PC)

Run by the **Windows tester** session (CLAUDE.md), which only tests and diagnoses and
writes redacted reports to `docs/windows-test/`. The backend agent updates the Status
column from those reports. Each report names the commit / installer artifact tested.

**Report rules (mandatory).** Never put server hostnames or IPs, share links, passwords,
UUIDs, keys or short IDs into a report, log excerpt or screenshot. That includes the node
list in Settings, which shows the server: don't screenshot it. Exit location is reported
as the `loc=` country only. Network checks are reported as the printed
`same` / `DIFFERENT` lines, counts and pass/fail, never raw routes or DNS addresses.
`C:\edge-test\` holds local snapshots (they contain local addresses): never commit or
paste them.

**Elevated** = run in an *Administrator* PowerShell window (Windows PowerShell 5.1 or 7).
Everything else runs in a normal PowerShell window as the signed-in user.

### 3.0 Setup (once per PowerShell window)

```powershell
$Inst = "$env:LOCALAPPDATA\Lynnk"     # per-user install dir (NSIS default)
$App  = Get-ChildItem $Inst -Filter *.exe | Where-Object Name -notin 'sing-box.exe','edge-helper.exe','uninstall.exe' | Select-Object -First 1
$Svc  = 'io.github.mosb2bls.edgeclient.helper'
$Snap = 'C:\edge-test'                      # local only
$Repo = '<path to your clone>'               # e.g. "$HOME\Desktop\VPN" (scripts\windows\*.ps1)
function Snapshot($name, $compare) {          # W-BASELINE helper
  $a = @('-NoProfile','-ExecutionPolicy','Bypass','-File',"$Repo\scripts\windows\net-snapshot.ps1",'-Out',"$Snap\$name.json")
  if ($compare) { $a += @('-Compare', "$Snap\$compare.json") }
  powershell @a
}
function Check($via, [switch]$cn, [switch]$save) {   # each call = a fresh process (no proxy caching)
  $a = @('-NoProfile','-ExecutionPolicy','Bypass','-File',"$Repo\scripts\windows\exit-check.ps1",'-Via',$via,'-Dir',$Snap)
  if ($cn) { $a += '-Cn' }; if ($save) { $a += '-SaveBaseline' }
  powershell @a
}
function ExitBaseline { Check direct -save }  # once, while DISCONNECTED
function Loc { Check system }                 # through the Windows proxy, like browsers
function LocDirect { Check direct }           # curl, ignores the Windows proxy (TUN tests)
function Cn { Check system -cn }              # CN site through the Windows proxy
function CnDirect { Check direct -cn }        # CN site, no proxy
```

`$App` is the app executable inside `$Inst`. Run **`ExitBaseline` once while disconnected**
(again after long breaks / reboots: the ISP may change your address, round 3),
before testing. It stores a hash of your direct exit address in `$Snap` (local only).

Afterwards `Loc` / `LocDirect` print one line such as `loc=GB exit=DIFFERENT via=system`:
- `exit=DIFFERENT` = the traffic went through the server;
- `exit=same` = it went direct.

This matters because `loc=` alone can't tell the two apart when your own exit country is also
GB (round 1). No address is ever printed. Each helper starts a new PowerShell process, because
one long-lived window caches the proxy setting and would silently go direct after connecting.
`Cn` / `CnDirect` print `cn=<HTTP status>`. `curl` ignores the Windows proxy, so use `Loc` /
`Cn` for system-proxy checks and `LocDirect` / `CnDirect` for TUN checks.

### 3.R Recovery — if the network breaks during a test (elevated)

Gentle to heavy; stop once websites load again. Note in the report which step fixed it.

0. Optional, for diagnosis: `Snapshot broken before`. The `DIFFERENT` sections show what
   was left changed.
1. Quit Lynnk (tray), then let it undo its own changes:
   `(Start-Process $App.FullName -ArgumentList '--restore-networking' -Wait -PassThru).ExitCode`.
   0 = restored, 2 = incomplete; continue either way.
2. End the TUN session and the kill switch:
   `Stop-Service $Svc -ErrorAction SilentlyContinue; Get-Process sing-box, edge-helper -ErrorAction SilentlyContinue | Stop-Process -Force`.
   The WFP filters are in a dynamic session and vanish with the helper process.
3. Undo test commands:
   `Get-NetFirewallRule -DisplayName 'EdgeTest*' -ErrorAction SilentlyContinue | Remove-NetFirewallRule`;
   `pktmon stop; pktmon filter remove`; re-enable a disabled adapter
   (`Get-NetAdapter`, then `Enable-NetAdapter -Name '<name>' -Confirm:$false`);
   `ipconfig /flushdns`.
4. Settings → Network & internet → Proxy: turn off "Use a proxy server" if it shows
   `127.0.0.1`, and turn off a setup script you did not set yourself. Restart the browser.
5. Check: `Test-NetConnection 1.1.1.1 -Port 443` and `Resolve-DnsName example.com -DnsOnly`.
6. Reboot. This clears the WFP filters and the TUN adapter (`tun0`); the helper cleans up its
   journal at start. To keep the helper from starting at boot until fixed:
   `Set-Service $Svc -StartupType Disabled` (undo with `Automatic`, or uninstall the helper).
7. Last resort, after a reboot didn't help: `netsh winsock reset; netsh int ip reset`, then
   reboot. This also clears manually set IP / DNS settings.

### 3.1 Status

| ID | What | Elevated | Status |
|---|---|---|---|
| W-BASELINE | Snapshot + compare tool works | yes (for the WFP count) | **PASS** round 1 (`0e336ca`, `docs/windows-test/2026-10-12-0e336ca.md`) |
| W-INSTALL | Installer install / uninstall leaves nothing behind | partly | 1–3 **PASS** round 1; 3b **PASS** round 3 (`f5d97fa`); 4–6 deferred |
| W-SYSPROXY | System proxy mode | no | **PASS** round 1 |
| W-HELPER | Helper service install / uninstall | UAC prompt | **PASS** round 3 (`f5d97fa`) (F-1, F-3 fixed) |
| W-TUN | TUN connectivity | no | 1–2 **PASS** round 3 (`f5d97fa`); 3 FAIL (F-4 firewall rule left — fixed in 1.5.2, retest) |
| W-DNSLEAK | DNS leak, both DNS profiles (pktmon) | **yes** | **PASS** round 3 (`f5d97fa`) (O-6: 2–3 packets to the other profile's DoH resolver after a profile switch, encrypted) |
| W-IPV6 | IPv6 bypass | no | **PASS** round 3 (`f5d97fa`) |
| W-KILL | Kill switch (core crash, helper crash) | **yes** | 1, 2, 4 **PASS** round 3 (`f5d97fa`); 3 FAIL (F-5 app status after helper crash — fixed in 1.5.2, retest) |
| W-FORCEKILL | App force-kill + relaunch, `--restore-networking` | no | **PASS** (1–2 round 1, 3 round 3 (`f5d97fa`)) |
| W-RESTORE | Disconnect / Direct / Quit restore exactly | no | **PASS** (system proxy round 1, TUN round 3 (`f5d97fa`) apart from F-4) |
| W-RESTORE-PARTIAL | Partial restore reporting (REQ-4) | **yes** | **PASS** round 3 (`f5d97fa`) |
| W-RESTORE-PARTIAL-FW | Kill-switch removal failure | — | N/A on Windows (see item) |
| W-FAILOVER | Failover with UDP 24443 blocked | **yes** | switch **PASS** (system proxy) round 3 (`f5d97fa`); return FAIL (F-7 — fixed in 1.5.2, retest); TUN not testable with a firewall rule |
| W-NETCHANGE | Network change, sleep / wake | partly | **PASS** both modes round 3 (`f5d97fa`) |
| W-REBOOT | Forced reboot with a leftover journal | no | TUN **PASS** round 3 (`f5d97fa`); system proxy (login OFF) PASS owner-reported; login ON deferred |
| W-LOGIN | Launch at login, single instance | no | 1, 3, 4 **PASS**; 5 FAIL round 3 (`f5d97fa`) (F-8 not listed in Task Manager — fixed in 1.5.2, retest); 2 deferred |
| W-SHELL | Tray popover, scaling, Settings title bar | no | **PASS** round 3 (`f5d97fa`) (owner-reported) |

### W-BASELINE — snapshot + compare (elevated)

`scripts\windows\net-snapshot.ps1` records:
- adapters, routes, DNS servers, NRPT rules and IPv6 bindings;
- the WinINet proxy (registry values and `DefaultConnectionSettings`) and the WinHTTP proxy;
- firewall rules with our names, the helper service and its folders;
- when run elevated, the number of our WFP kill-switch filters.

`-Compare` prints one `same` / `DIFFERENT` line per section and `RESULT: identical` or
`RESULT: n section(s) differ`. Exit codes: 0 identical, 3 different, 1 script error.

```powershell
Snapshot before; Snapshot again before      # two runs, nothing changed in between
```

**Pass:** `RESULT: identical`. Other items use this pattern: `Snapshot before` while
disconnected right before the test, `Snapshot after before` after it. Don't change networks
(Wi-Fi ↔ Ethernet) between the two snapshots, because DHCP routes and DNS servers would
legitimately differ.

### W-INSTALL — installer install / uninstall (partly elevated)

1. Elevated: `Snapshot pre-install`.
2. Download the artifact **in the browser** (a `gh run download` copy has no
   Mark-of-the-Web, so SmartScreen is never consulted), extract it in Explorer, and run
   `Lynnk_<version>_x64-setup.exe`. SmartScreen *may* warn because the installer is unsigned
   (More info → Run anyway). Round 1 saw no warning, so record whether one appeared.
   **Pass:**
   - no UAC prompt (per-user install);
   - `Test-Path $Inst\sing-box.exe, $Inst\edge-helper.exe, $Inst\resources\rules\geosite-cn.srs` → all `True`;
   - a Start-menu entry exists, and the app starts in the tray;
   - Settings → Apps → Installed apps shows the publisher **Lynnk**.
3. Uninstall without the helper: Settings → Apps → Installed apps → Lynnk → Uninstall
   (or `& "$Inst\uninstall.exe"`).
   **Pass:**
   - no UAC prompt;
   - `Test-Path $Inst\*.exe` → `False`;
   - elevated: `Snapshot post-uninstall pre-install` → identical.

   Expected along the way:
   - the uninstaller asks to close the running app. Its pre-uninstall hook has already run
     `--restore-networking` at that point.
   - the "delete app data" box decides whether `%APPDATA%\io.github.mosb2bls.edgeclient` and
     `%LOCALAPPDATA%\io.github.mosb2bls.edgeclient` (node list, settings, logs) survive.
     Note which. The keychain entry (Windows Credential Manager) is separate.
3b. **Upgrade over an existing install** (with the helper installed): run the newer
   installer without uninstalling first. **Pass:**
   - no UAC prompt;
   - the app starts;
   - the Helper page shows **Reinstall helper** (the installed helper is the older build);
   - `& $App.FullName --helper-status` → `needs_upgrade=true`.

   Reinstall. **Pass:** `needs_upgrade=false`, `running=true`.
4. Reinstall, then install the helper (W-HELPER), connect in **TUN** and uninstall the app
   while connected. **Pass:**
   - exactly **one** UAC prompt (helper removal), which you accept;
   - afterwards `Get-Service $Svc -ErrorAction SilentlyContinue` → nothing;
   - `Test-Path "$env:ProgramFiles\Lynnk\Helper", "$env:ProgramData\Lynnk\Helper"` → `False, False`;
   - elevated: `Snapshot post-uninstall2 pre-install` → identical (no TUN adapter (`tun0`), `wfpKillSwitchFilters` 0).
5. Repeat step 4 connected in **System proxy** mode. **Pass:** `internetSettings` and
   `defaultConnectionSettings` are `same`.
6. Repeat step 4 and **decline** the UAC prompt. **Expected:** the app is removed, but the
   helper service remains. `Get-Service $Svc` → Running. This is a known limitation:
   decline means "keep the helper". Clean up by reinstalling the app, then Settings →
   Uninstall helper.

### W-SYSPROXY — system proxy mode

1. `Snapshot before`. `Loc` → `exit=same`.
2. Import your nodes. Capture mode System proxy, Rule, connect (HY2). **Pass:**
   - Windows Settings → Network & internet → Proxy shows a manual proxy `127.0.0.1:<port>`, setup script off;
   - `Loc` → `exit=DIFFERENT`, and `Cn` → `cn=200`;
   - Edge / Chrome open a non-CN site and a CN site.
3. Switch node to REALITY, then Rule ↔ Global. **Pass:** `Loc` → `exit=DIFFERENT` after each switch.
4. Disconnect. **Pass:** `Snapshot after before` → identical. The Proxy settings page
   looks as before.

Not a failure: apps that ignore the system proxy, DNS lookups and IPv6 bypass the proxy in
this mode. This is documented; use TUN for whole-device protection.

### W-HELPER — helper service (UAC)

Before step 1: if an **older** helper is installed (e.g. after upgrading the app), the Helper
page shows **Reinstall helper** (`needsUpgrade`, SPEC 1.5.1). Use it, or Uninstall then
Install, so the tests run on this build's helper. At any point,
`& $App.FullName --helper-status` prints one line
(`installed=… running=… version=… needs_upgrade=…`, plus `error=… detail=…` if unusable).
1. Settings → capture mode TUN → Install helper. Cancel the UAC prompt first.
   **Pass:** "cancelled" is shown, and `Get-Service $Svc -ErrorAction SilentlyContinue` → nothing.
2. Install again and accept. **Pass:**
   - `Get-Service $Svc | Format-List Status,StartType` → Running / Automatic;
   - `(Get-CimInstance Win32_Service -Filter "Name='$Svc'").StartName` → `LocalSystem`;
   - `Get-ChildItem "$env:ProgramFiles\Lynnk\Helper" | Select-Object Name` → `edge-helper.exe`, `sing-box.exe`;
   - as your normal user, `Get-ChildItem "$env:ProgramData\Lynnk\Helper"` → access denied;
   - `Test-Path '\\.\pipe\io.github.mosb2bls.edgeclient.helper'` → `True`;
   - the app shows the helper as installed **and running** (`get_helper_status` →
     `running: true` with a version). Round 1 failed here with "Access is denied" on the pipe
     (F-1, fixed by opening it with only the rights its access list grants). If it fails,
     report the `detail` text.
3. Settings → Uninstall helper (one UAC prompt). **Pass:** the service is gone and both
   folders are gone.

### W-TUN — TUN connectivity

1. `Snapshot before`. Helper installed, capture mode TUN, Rule, connect HY2.
   **Pass:**
   - `Get-NetAdapter | Where-Object { $_.Name -eq 'tun0' -or $_.InterfaceDescription -like '*sing-tun*' }` → one adapter, Up (sing-box names it `tun0`, "sing-tun Tunnel");
   - `LocDirect` → `exit=DIFFERENT` (no proxy configured anywhere), and `CnDirect` → `cn=200`;
   - `Resolve-DnsName example.com` answers;
   - Windows proxy settings untouched (`Snapshot during before` shows `internetSettings same`).
2. Switch to REALITY, then Global. **Pass:** `LocDirect` → `exit=DIFFERENT` each time. In Global,
   `CnDirect` still → `cn=200` (via the UK).
3. Disconnect. **Pass:** `Snapshot after before` → identical (adapter gone, routes as
   before).

### W-DNSLEAK — DNS leak, both DNS profiles (elevated, pktmon)

Run for **Mainland** and **Overseas** (Settings → DNS profile), each in TUN mode, Rule.
System-proxy mode cannot prevent DNS leaks, so it isn't tested here.

```powershell
pktmon filter remove
pktmon filter add DNS53u -t UDP -p 53
pktmon filter add DNS53t -t TCP -p 53
pktmon filter add DoT853 -t TCP -p 853
pktmon filter add AliDNS -i 223.5.5.5
pktmon filter add Quad9 -i 9.9.9.9
pktmon filter add CfDNS -i 1.1.1.1
pktmon start --capture --comp nics --pkt-size 64 --file-name "$Snap\dns.etl"
'example.com','www.bbc.co.uk','www.wikipedia.org','www.baidu.com','www.qq.com','github.com' |
  ForEach-Object { Resolve-DnsName $_ -DnsOnly -ErrorAction SilentlyContinue | Out-Null; curl.exe -s -o NUL -m 10 "https://$_" }
pktmon counters        # per adapter and filter: packet counts only
pktmon stop
pktmon filter remove
```

**Pass**, on the **physical** adapter (Wi-Fi / Ethernet; the TUN adapter (`tun0`) is expected to
see DNS):
- DNS53u, DNS53t and DoT853: 0;
- CfDNS: 0 (Cloudflare DNS goes only through the tunnel);
- Mainland: AliDNS > 0 and Quad9 0. Overseas: Quad9 > 0 and AliDNS 0.

Report the counters table as printed: it shows adapter names and counts only. Optional
cross-check: ipleak.net in Edge shows no resolver from your ISP.

### W-IPV6 — IPv6 bypass

1. Before connecting: `curl.exe -6 -s -o NUL -m 10 -w "%{http_code}" https://ipv6.google.com`.
   If this is not `200`, the network has no IPv6; record **N/A** and skip.
2. TUN connected: the same command → fails (`000`) **quickly**. Measure with
   `Measure-Command { curl.exe -6 -s -o NUL -m 10 https://ipv6.google.com }`: under
   ~2 s means rejected rather than timed out.
3. Browse a few dual-stack sites (google.com, wikipedia.org, facebook.com), then (set `$connectedAt = Get-Date` right after connecting):
   ```powershell
   (Get-NetTCPConnection -State Established | Where-Object { $_.CreationTime -gt $connectedAt -and $_.RemoteAddress -match ':' -and $_.RemoteAddress -notmatch '^(::1|fe80|fdfe:dcba:9876)' }).Count
   ```
   **Pass:** `0`. Report the count only. In Edge, test-ipv6.com shows no IPv6.
4. System-proxy mode: record what step 3 shows, as an observation only. Expected and
   documented:
   - IPv6 bypasses the proxy for apps that ignore it (`curl.exe -6` → 200);
   - IPv6-only sites **fail** for apps that use the proxy.

### W-KILL — kill switch (elevated)

TUN connected, kill switch enabled in Settings, a large download (e.g. Edge →
`https://speed.cloudflare.com/__down?bytes=1000000000`) or video running in the
browser.

1. Keep the core down for ~20 s (the helper restarts it with backoff 1, 2, 4, 8 s…):
   ```powershell
   $end = (Get-Date).AddSeconds(20); while ((Get-Date) -lt $end) { Stop-Process -Name sing-box -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 300 }
   ```
   During the loop, in a second elevated window (run the §3.0 block there first):
   `LocDirect` → `loc=FAILED` (no direct traffic). Then:
   ```powershell
   netsh wfp show filters file="$Snap\wfp.xml" | Out-Null; (Select-String -Path "$Snap\wfp.xml" -Pattern 'Lynnk kill switch' -SimpleMatch).Count
   ```
   **Pass:**
   - the count is > 0 and the download stalls;
   - the app shows Reconnecting with the kill switch active.
2. After the loop ends. **Pass:**
   - within ~30 s the app is Connected again, and `LocDirect` → `exit=DIFFERENT`;
   - the same WFP count → 0.
3. Helper crash while the kill switch is engaged: start the loop again and, during it, run
   `Stop-Process -Name edge-helper -Force`. **Pass:**
   - immediately the WFP count → 0 (dynamic session: Windows removes the filters with the process);
   - `Get-Service $Svc` → Running again within ~10 s (automatic restart);
   - the PC is never left offline;
   - the app ends Connected (it reconnected), or in Error with a restore report. Record which.
4. Disconnect. **Pass:** `Snapshot after before` → identical. Delete `$Snap\wfp.xml`.

### W-FORCEKILL — app force-kill + relaunch

1. **System proxy:**
   1. `Snapshot before`, then connect.
   2. `Stop-Process -Name $App.BaseName -Force`. **Expected:** the app's `sing-box.exe`
      child survives on Windows, so `Loc` probably still → `exit=DIFFERENT`. Record what you see.
      Either result is acceptable, but no traffic may go direct.
   3. Relaunch from the Start menu. **Pass:** a "previous network settings restored"
      notice, and `Snapshot after before` → identical.
2. **CLI restore:** connect, then force-kill as above, then:
   ```powershell
   (Start-Process $App.FullName -ArgumentList '--restore-networking' -Wait -PassThru).ExitCode
   ```
   **Pass:** `0`, and `Snapshot after before` → identical.
3. **TUN:** connect, then force-kill the app. **Pass:**
   - `LocDirect` → `exit=DIFFERENT` keeps working (the helper owns the session);
   - relaunch → "re-attached to the running session";
   - Disconnect → `Snapshot after before` identical.

### W-RESTORE — Disconnect / Direct / Quit

For **System proxy** and for **TUN**, starting each time from `Snapshot before` and a
connected state:
- (a) Disconnect;
- (b) switch to Direct;
- (c) tray → Quit.

**Pass:**
- after each one, `Snapshot after before` → identical;
- after (b), `Loc` / `LocDirect` → `exit=same`;
- after (c), relaunching shows no recovery notice.

### W-RESTORE-PARTIAL — partial restore reporting, REQ-4 (elevated)

1. TUN connected. Elevated: `Stop-Service $Svc`. Then, within ~5 s, press **Disconnect**
   in the app. If the app already reacted on its own, record what it showed.
   **Pass:**
   - "Restore incomplete" with the per-item list: TUN session / routes / firewall
     **skipped (HELPER_UNREACHABLE)**, system proxy not needed;
   - the state is **Error**, not Disconnected.
2. Quit and relaunch the app. **Pass:** the same incomplete report is shown again.
3. `Start-Service $Svc`, then Settings → Restore networking. **Pass:** a complete report,
   state Disconnected, and `Snapshot after before` → identical.

(Stopping the service ends its session cleanly. The app reports "incomplete" because it
could not *confirm* that, which is the intended honest behaviour.)

### W-RESTORE-PARTIAL-FW — N/A on Windows

The WFP kill switch lives in a dynamic WFP session (ADR-0007). It cannot "fail to be
removed" while the helper runs, and Windows removes it if the helper dies. There's no
release-build way to force the macOS-style failure, so W-KILL step 3 covers the helper-death
case instead.

### W-FAILOVER — failover with UDP 24443 blocked (elevated)

For System proxy and TUN, Rule mode, HY2 selected, REALITY present, failover on (default
settings). **Add the block rule before connecting**: Windows Firewall does not re-check an
already open UDP flow. **TUN:** the rule does not affect the helper's sing-box, so block UDP
24443 elsewhere (router, or a phone hotspot that blocks UDP), or record TUN as not testable.
The return to HY2 takes ≥ 10 min after unblocking; a script that waits should say so while it
waits. If HY2 stays failing in the running core while the server is fine, the app checks it
with a fresh core every ~30 s and returns by restarting the core (SPEC 1.5.2, F-7) — the log
shows "fresh check passes".

```powershell
New-NetFirewallRule -DisplayName 'EdgeTest-BlockHY2' -Direction Outbound -Protocol UDP -RemotePort 24443 -Action Block | Out-Null
```

**Pass:**
- within ~30–60 s the popover shows the switch to REALITY with the reason (the app has no OS notifications — round 3);
- `Loc` / `LocDirect` → `exit=DIFFERENT`, and no "all nodes failing".

Then:

```powershell
Remove-NetFirewallRule -DisplayName 'EdgeTest-BlockHY2'; Get-NetFirewallRule -DisplayName 'EdgeTest*' -ErrorAction SilentlyContinue
```

**Pass:**
- the second command prints nothing;
- the app returns to HY2 only after ~10 min of continuous success, and not before 5 min;
- no back-and-forth switching.

### W-NETCHANGE — network change, sleep / wake (partly elevated)

For System proxy and TUN, connected:
- (a) Elevated: `Disable-NetAdapter -Name 'Wi-Fi' -Confirm:$false; Start-Sleep 15; Enable-NetAdapter -Name 'Wi-Fi'`.
  Use your adapter name from `Get-NetAdapter`. Alternatively switch Wi-Fi ↔ Ethernet or
  to a phone hotspot.
- (b) Start → Power → Sleep; keep it asleep ≥ 2 min **by the clock** (Modern Standby PCs may wake early and keep the network up — check the real duration with `Get-WinEvent -FilterHashtable @{LogName='System'; Id=506,507} -MaxEvents 4`), wake.

**Pass:**
- within ~30 s of the network returning, `Loc` / `LocDirect` → `exit=DIFFERENT`, and the state is
  Connected (Degraded or Reconnecting briefly is fine);
- at most one failover switch, and no "restore incomplete";
- TUN only: the W-DNSLEAK counters stay 0 on the physical adapter for DNS53u, DNS53t and
  DoT853 across the change. This check is optional.

### W-REBOOT — forced reboot with a leftover journal

1. **TUN:** `Snapshot before`, connect, then `shutdown /r /f /t 0`. The app gets no clean
   quit. After signing in again, *before* starting the app:
   - **Pass:** `Get-Service $Svc` → Running;
   - **Pass:** no TUN adapter (`tun0`);
   - elevated: `Snapshot boot before`. **Pass:** `wfpKillSwitchFilters` 0, and normal
     browsing works (the helper cleaned up at boot).

   Start the app. **Pass:** `Snapshot after before` → identical. A recovery notice appears only if the app itself had to restore something; when the helper already cleaned up at boot, `lastRecovery` is `restored` with all items `not_needed` and no notice is shown (round 3, O-7 — intended).
2. **System proxy:** `Snapshot before`, connect, `shutdown /r /f /t 0`. After signing in,
   with launch at login off:
   - **Expected:** `Loc` → `loc=FAILED` (the proxy still points at the dead port; fail closed).

   Start the app. **Pass:** "previous network settings restored", `Loc` → `exit=same`,
   and `Snapshot after before` → identical. Repeat with launch at login **on**. **Pass:**
   restored at sign-in without opening anything.

### W-LOGIN — launch at login, single instance

1. Settings → Launch at login ON.
   **Pass:** `Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run` shows
   a value pointing to `$App` with `--autostart`, and Task Manager → Startup apps lists it
   as Enabled.
2. Sign out and in. **Pass:** the tray icon appears with no window. With auto-connect ON,
   it connects with the last node and mode.
3. While it runs, start the app again from the Start menu. **Pass:** still **one** tray
   icon (`(Get-Process -Name $App.BaseName).Count` → 1), and the Settings window opens.
4. Launch at login OFF. **Pass:** the Run value is gone, and so is Task Manager's record:
   `Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -Name Lynnk -ErrorAction SilentlyContinue`
   → nothing (round 1, M-2).
5. Launch at login ON, then disable Lynnk in Task Manager → Startup apps.
   **Pass:** Lynnk Settings now shows launch at login **off**. Turning it on in Lynnk again
   → Task Manager shows **Enabled**.

The single-instance plugin's message window (`…edgeclient-siw`) reports as "visible",
15×15 at 0,0. That is by design: it's layered, transparent and a tool window, so nothing is
drawn (round 1, M-3). Only report it if something actually shows in the top-left corner.

### Tray icons (macOS and Windows, SPEC 1.5.3)

**Pass:**
- The app icon is the cat on a white tile in Finder / Dock (macOS) and in Explorer, Start
  and the taskbar (Windows).
- The tray / menu bar never shows the white tile; it shows the state icons: hollow circle =
  Off, two dots = Connecting (pulsing; static if reduced motion / "Animation effects" is
  off), solid circle = Connected, short bar = Connection unavailable.
- Tooltips: `Lynnk — Off`, `Lynnk — Connecting`, `Lynnk — Connected`,
  `Lynnk — Connection unavailable`.
- Switching HY2 ↔ REALITY or Rule ↔ Global doesn't change the icon.
- The animation stops as soon as the state changes.
- Eye openings and background stay transparent:
  - macOS: light and dark menu bar, the icon follows the menu-bar colour;
  - Windows: light and dark taskbar (Settings → Personalization → Colors), the icon switches
    black ↔ white within ~2 s, and is sharp at 100 / 125 / 150 / 200 %.

### W-SHELL — tray popover position, display scaling, Settings title bar

Windows 11 keeps the taskbar at the bottom.

**Settings window title bar** (from SPEC 1.5.0, after the UI's custom title bar ships): there
is no native Windows title bar.

**Pass:**
- the app's own bar drags the window, and double-click maximizes / restores;
- minimize, maximize and close work, and close hides the window (the app keeps running in the tray);
- the window can still be resized from its edges.

**Steps:**
- left-click the tray icon, both when it is pinned and when it is in the overflow (^) menu;
- with taskbar auto-hide on;
- at display scale 100 %, 125 %, 150 % and 175 % (Settings → System → Display; change it
  while the app runs);
- on a second monitor with a different scale, if available.

**Pass:**
- the popover sits directly above the taskbar next to the icon, is fully on screen and never
  overlaps the taskbar;
- text is sharp at every scale;
- clicking elsewhere or pressing Esc hides it, and clicking the icon while it is open closes it;
- right-click shows the menu: Settings… opens the settings window, closing it hides it, and
  Quit exits (with restore).

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

## 6. Helper install / TUN test steps (owner; needs your admin password)

1. Build what the installer needs: `cargo build -p edge-helper && npm run fetch-core`.
2. Start the app: `cargo run -p edge-app`. Settings → Helper → Install (or, until the UI
   screen exists, Settings → Connection → TUN, Connect → "helper not installed" → Install).
   macOS asks for your administrator password once.
3. Check (read-only): `ls -l /Library/PrivilegedHelperTools/ | grep edgeclient`,
   `ls -l /var/run/io.github.mosb2bls.edgeclient.helper.sock`,
   `sudo launchctl print system/io.github.mosb2bls.edgeclient.helper | head -20`.
4. Run M-TUN, M-IPV6, M-DNSLEAK, M-KILL, M-TUN-FORCEKILL from the table above.
5. Uninstall from the app (admin prompt again) and confirm the files are gone.
