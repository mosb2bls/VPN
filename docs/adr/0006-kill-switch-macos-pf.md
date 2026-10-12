# ADR-0006: TUN kill switch with a pf anchor (macOS)

- Status: implemented (Phase B3.1, 2026-10-12). Rule generation unit-tested; **not yet run
  on a real Mac** (needs the helper installed; test M-KILL).

## Context

Brief §7.6: in TUN mode, if sing-box crashes its routes vanish and traffic would leak
direct. The helper must apply a firewall kill switch (macOS pf anchor) allowing only
loopback, LAN, DHCP / necessary local services, and the proxy server IP on its ports, until
the core restarts or the user disconnects. Brief blocking of China direct traffic during a
crash is acceptable. Disconnect / Direct / Quit remove it.

## Decisions

- **Anchor** `com.apple/250.EdgeClient`. The stock `/etc/pf.conf` contains
  `anchor "com.apple/*"` (checked on the owner's Mac, 2026-10-12), so rules loaded with
  `pfctl -a com.apple/250.EdgeClient -f -` are evaluated without editing pf.conf.
- **Enable by reference**: `pfctl -E` returns a token; we release it with `pfctl -X <token>`
  so pf stays enabled if something else (e.g. another VPN client) also uses it. The token is
  journaled so a crashed helper's reference is released at its next start.
- **When**: engaged only after an *unexpected* core exit during a TUN session with the
  kill switch enabled (default on); released when the restarted core is up, on Stop /
  Restore, and at helper start if left over. While the core runs normally no pf rules are
  loaded (the TUN with `strict_route` carries all traffic).
- **Rules** (`edge_core::pf::rules`, outbound only, `quick`):
  `pass` lo0 · DHCP (udp 68→67) · link-local / multicast / broadcast (v4 + v6) · LAN ranges
  `10/8 172.16/12 192.168/16 100.64/10` if allowed · each proxy server IP on its exact
  protocol/ports (HY2 UDP port or hopping ranges, VLESS TCP port) · bootstrap DoH resolvers
  (223.5.5.5, 9.9.9.9) on TCP 443 so a domain server address can be re-resolved on restart ·
  then `block return out quick all` (**reject**, not silent drop: TCP RST / ICMP
  unreachable so apps fail fast).
- Server IPs are resolved at session start (IPv4 only; the service is IPv4-only).

## Consequences / open points

- China direct traffic is blocked while the kill switch is engaged (accepted by the brief).
- If restart fails for 5 minutes the kill switch **stays** until the user disconnects or
  uses Restore networking (fail closed); the UI shows `killSwitchActive`.
- Another app that flushes pf entirely (`pfctl -F all`) would remove our anchor rules;
  detected only indirectly. The owner's Mac has an (empty) Sangfor aTrust anchor set —
  check for interference during M-KILL.
