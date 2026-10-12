# Windows baseline — 2026-10-12 (before any Lynnk install)

Redacted summary of the clean system. The full snapshots (`C:\edge-test\before.json`,
`again.json`, `pre-install.json`) contain local addresses and stay on the PC.

- Windows: Windows 11 Pro (zh-CN UI), 26H2, build 26300.9457 (`[Environment]::OSVersion.Version` 10.0.26300.0), x64
- Repo at `0e336ca` (SPEC 1.4.4); snapshot tool `scripts\windows\net-snapshot.ps1`, run elevated
- No previous Lynnk / EdgeClient install: `%LOCALAPPDATA%\Lynnk`, `%ProgramFiles%\Lynnk`,
  `%ProgramData%\Lynnk`, `%APPDATA%\io.github.mosb2bls.edgeclient` all absent

| Section | Baseline (counts / flags only) |
|---|---|
| Adapters | `WLAN` Up (active uplink); `以太网` (Ethernet) Disconnected; `蓝牙网络连接` (Bluetooth PAN) Disconnected; `WLAN 2`, `WLAN 5` Not Present. No Wintun adapter |
| Routes | 46 total (19 IPv4, 27 IPv6); one IPv4 default route, one IPv6 default route |
| DNS servers | 7 interface/family entries with servers configured |
| NRPT rules | 0 |
| IPv6 binding (`ms_tcpip6`) | enabled on all 5 adapters |
| IPv6 internet | yes: `curl -6 https://ipv6.google.com` → 200 |
| WinINet proxy | `ProxyEnable=0`, no `ProxyServer`, no `AutoConfigURL`, `AutoDetect` unset |
| WinHTTP proxy | Direct access (no proxy server) |
| Firewall rules matching `*Lynnk*` / `*sing-box*` / `EdgeTest*` | 0 |
| Helper service `io.github.mosb2bls.edgeclient.helper` | not installed |
| Helper folders (Program Files / ProgramData) | absent / absent |
| WFP filters named "Lynnk kill switch" (elevated) | 0 |

W-BASELINE (elevated): `Snapshot before; Snapshot again before` → 12 sections `same`,
`RESULT: identical`, exit 0. `Snapshot pre-install` taken for W-INSTALL step 1.

Note for readers of the JSON: an empty section is serialised as `null`, so `@(...).Count`
on it gives 1 — use the live cmdlets to count (both NRPT and firewall are really 0).
