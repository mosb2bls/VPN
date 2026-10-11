# ADR-0003: sing-box routing and DNS (system proxy mode)

- Status: accepted for implementation (Phase B2.5, 2026-10-12). **Untested against the
  real server and real networks** — verified only by unit/snapshot tests and
  `sing-box check` (1.14.3). DNS-leak tests (brief §8) are scheduled for B2/B3.
- Generator: `crates/core/src/singbox.rs`; snapshots in `crates/core/tests/snapshots/`.

## Sources (sing-box 1.14 documentation, read 2026-10-12)

- Migration guide (1.12 DNS server format, 1.11 rule actions, 1.14 changes):
  https://sing-box.sagernet.org/migration/
- DNS (`dns`, servers, HTTPS server, rules, rule actions):
  https://sing-box.sagernet.org/configuration/dns/ ,
  …/dns/server/https/ , …/dns/rule/ , …/dns/rule_action/
- Route (rules, rule actions, `default_domain_resolver`):
  https://sing-box.sagernet.org/configuration/route/ , …/route/rule/ , …/route/rule_action/
- Dial fields (`domain_resolver` — required for outbounds with a domain server since
  1.14): https://sing-box.sagernet.org/configuration/shared/dial/
- Outbounds: …/outbound/hysteria2/ , …/outbound/vless/ ; TLS / uTLS / REALITY:
  …/shared/tls/

## Decisions

**Outbounds.** One `selector` (`proxy`) over all node outbounds (`node-<id>`), default =
selected node, `interrupt_exist_connections: true`. Switching via the Clash API on
`127.0.0.1:<random port>` with a random per-session secret. No `urltest`. A `direct`
outbound resolves via `dns-direct`.

- HY2: `tls.alpn = ["h3"]` (the server is an Xray Hysteria inbound with ALPN h3);
  `server_name` = link SNI, else the server domain; port hopping → `server_ports`
  (`"start:end"`); no `up/down_mbps` unless given (→ BBR); salamander obfs if set.
- VLESS: `flow: xtls-rprx-vision`, uTLS fingerprint, REALITY public key + short ID,
  `packet_encoding` left at sing-box's default (`xudp`, matches Xray).
- **Proxy server hostname** (any node whose server is a domain) resolves through
  `domain_resolver: dns-bootstrap` — a dedicated direct DoH server, never the tunnel. IP
  literals skip it. `route.default_domain_resolver` is also `dns-bootstrap`.

**DNS servers** (all IP literals, so no resolution is needed to reach them):

| Tag | Mainland profile | Overseas profile | Path |
|---|---|---|---|
| `dns-proxy` | Cloudflare DoH `1.1.1.1` (SNI cloudflare-dns.com) | same | **through `proxy`** (`detour`) |
| `dns-direct` | AliDNS DoH `223.5.5.5` (SNI dns.alidns.com) | Quad9 DoH `9.9.9.9` (SNI dns.quad9.net) | direct |
| `dns-bootstrap` | same as `dns-direct` (separate entry) | same | direct |
| `dns-local` | system resolver | same | direct |

**DNS rules:** AAAA → empty NOERROR (`predefined`; IPv4-only service) · LAN suffixes
(`local`, `lan`, `home.arpa`, `localdomain`, `internal`) → `dns-local` · Rule mode only:
`geosite-cn` → `dns-direct` · `final` → `dns-proxy`. `dns.strategy = ipv4_only`.

**Route rules (in order):** `sniff` · `hijack-dns` for DNS protocol · loopback
(`127.0.0.0/8`, `::1`) → direct · `ip_is_private` and LAN suffixes → direct if `allowLan`,
else `reject` · `ip_version: 6` → `reject` (TCP RST / ICMP unreachable, so apps fall back
to IPv4 fast) · Rule mode only: `rule_set: [geosite-cn, geoip-cn]` → direct · `final:
proxy` (fail closed: anything not explicitly direct goes to the proxy or fails).

**Rule-sets:** local binary `.srs` files (`geosite-cn`, `geoip-cn`), bundled and updated
in a later step.

**spiderX:** sing-box 1.14 REALITY has no spiderX option (verified: `sing-box check`
rejects `tls.reality.spider_x`). Links with a non-empty `spx` are now rejected
(`LINK_UNSUPPORTED_PARAM`) instead of being silently ignored.

## Known limitations / open questions

1. **`geoip-cn` only applies to IP destinations.** In system-proxy mode apps hand the proxy
   a domain, and sing-box IP rules do not match a domain destination unless it is resolved
   first (`resolve` action). We do **not** add a `resolve` step (it would add a DNS round
   trip — via the proxied DoH — to every proxied connection). Effect: China-hosted sites
   whose domains are not in `geosite-cn` go through the proxy. Revisit if the harness shows
   problems. (In TUN mode apps connect to IPs, so `geoip-cn` applies there.)
2. **AliDNS over DoH reliability** from mainland networks is untested; the brief allows
   UDP fallback if DoH is unreliable. To be decided from DNS-leak/harness results.
3. **Overseas direct resolver** = Quad9 (not Cloudflare, so direct and proxied lookups
   use different providers). Arbitrary choice; any reputable non-China resolver works.
4. **Bootstrap lookups are visible to the local network** (DoH to AliDNS/Quad9 hides the
   name from on-path observers but not from that resolver). Unavoidable for a domain
   server address; use an IP literal server to avoid it.
5. Fake-IP, TUN inbound, `auto_detect_interface`/`strict_route`, server-IP route
   exclusions: Phase B3.
