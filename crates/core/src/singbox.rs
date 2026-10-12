//! sing-box 1.14 configuration generation (system proxy mode; TUN in B3).
//!
//! Design (see docs/adr/0003-routing-and-dns.md):
//! - One `selector` outbound `proxy` over all node outbounds; node switching
//!   via the local Clash API (127.0.0.1, random secret). No `urltest`.
//! - Node outbounds whose server is a domain resolve it through a dedicated
//!   direct **bootstrap** DNS server (`domain_resolver`), never the tunnel.
//! - DNS: China domains (geosite-cn) → profile's direct resolver; everything
//!   else → DoH **through the proxy**; LAN names → system resolver; AAAA → empty
//!   (IPv4-only service).
//! - Routing: loopback always direct; private/LAN direct if allowed (else
//!   reject); IPv6 destinations rejected (fast fail); Rule mode sends
//!   geosite-cn / geoip-cn direct; everything else → `proxy` (fail closed).
//!
//! References (sing-box 1.14 docs, checked 2026-10-12):
//! - DNS servers / rules / rule actions: https://sing-box.sagernet.org/configuration/dns/
//! - Route rules / actions: https://sing-box.sagernet.org/configuration/route/
//! - Dial fields (`domain_resolver`): https://sing-box.sagernet.org/configuration/shared/dial/
//! - Hysteria2 / VLESS outbounds, TLS / REALITY: https://sing-box.sagernet.org/configuration/outbound/

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::link::{Hy2Config, NodeConfig, VlessRealityConfig};
use crate::validate::{self, Host};

pub const TAG_PROXY: &str = "proxy";
pub const TAG_DIRECT: &str = "direct";
pub const TAG_MIXED_IN: &str = "mixed-in";
pub const TAG_TUN_IN: &str = "tun-in";
/// TUN interface addresses (private ranges; IPv6 included so public IPv6 is
/// captured and then rejected instead of bypassing the tunnel).
pub const TUN_ADDRESSES: [&str; 2] = ["172.19.0.1/30", "fdfe:dcba:9876::1/126"];
pub const DNS_PROXY: &str = "dns-proxy";
pub const DNS_DIRECT: &str = "dns-direct";
pub const DNS_BOOTSTRAP: &str = "dns-bootstrap";
pub const DNS_LOCAL: &str = "dns-local";

/// Non-routable / LAN name suffixes resolved by the system resolver and sent direct.
pub const LAN_SUFFIXES: &[&str] = &["local", "lan", "home.arpa", "localdomain", "internal"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routing {
    Rule,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsProfile {
    Mainland,
    Overseas,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    /// Local mixed (HTTP + SOCKS5) inbound on 127.0.0.1:port.
    SystemProxy { port: u16 },
    /// TUN inbound with auto_route + strict_route (run by the privileged helper).
    Tun,
}

#[derive(Debug, Clone)]
pub struct RuleSets {
    pub geosite_cn: PathBuf,
    pub geoip_cn: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GenInput<'a> {
    /// (node id, config) in user order. Must be non-empty.
    pub nodes: Vec<(&'a str, &'a NodeConfig)>,
    pub selected: &'a str,
    pub routing: Routing,
    pub dns: DnsProfile,
    pub capture: Capture,
    pub allow_lan: bool,
    /// Clash API on 127.0.0.1:port with this secret.
    pub controller_port: u16,
    pub controller_secret: &'a str,
    pub rule_sets: RuleSets,
    /// Direct fallback enabled: `direct` is a selectable member of the selector
    /// (used only when every node fails).
    pub direct_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenError {
    NoNodes,
    SelectedNotFound,
}

pub fn node_tag(id: &str) -> String {
    format!("node-{id}")
}

/// A DoH server definition (IP literal, so no bootstrap needed).
struct Doh {
    ip: &'static str,
    sni: &'static str,
}

/// Proxied DoH (through `proxy`).
const PROXIED_DOH: Doh = Doh { ip: "1.1.1.1", sni: "cloudflare-dns.com" };
/// Direct resolvers per profile.
const MAINLAND_DIRECT: Doh = Doh { ip: "223.5.5.5", sni: "dns.alidns.com" };
const OVERSEAS_DIRECT: Doh = Doh { ip: "9.9.9.9", sni: "dns.quad9.net" };

fn doh_server(tag: &str, d: &Doh, detour: Option<&str>) -> Value {
    let mut v = json!({
        "type": "https",
        "tag": tag,
        "server": d.ip,
        "tls": { "enabled": true, "server_name": d.sni }
    });
    if let Some(det) = detour {
        v["detour"] = json!(det);
    }
    v
}

pub fn generate(input: &GenInput) -> Result<Value, GenError> {
    if input.nodes.is_empty() {
        return Err(GenError::NoNodes);
    }
    if !input.nodes.iter().any(|(id, _)| *id == input.selected) {
        return Err(GenError::SelectedNotFound);
    }
    let direct_doh = match input.dns {
        DnsProfile::Mainland => &MAINLAND_DIRECT,
        DnsProfile::Overseas => &OVERSEAS_DIRECT,
    };

    // --- outbounds ---------------------------------------------------------
    let mut members: Vec<String> = input.nodes.iter().map(|(id, _)| node_tag(id)).collect();
    if input.direct_fallback {
        members.push(TAG_DIRECT.into());
    }
    let mut outbounds: Vec<Value> = vec![json!({
        "type": "selector",
        "tag": TAG_PROXY,
        "outbounds": members,
        "default": node_tag(input.selected),
        "interrupt_exist_connections": true
    })];
    for (id, cfg) in &input.nodes {
        outbounds.push(match cfg {
            NodeConfig::Hysteria2(h) => hy2_outbound(&node_tag(id), h),
            NodeConfig::VlessReality(v) => vless_outbound(&node_tag(id), v),
        });
    }
    outbounds.push(json!({ "type": "direct", "tag": TAG_DIRECT, "domain_resolver": DNS_DIRECT }));

    // --- DNS ---------------------------------------------------------------
    let servers = vec![
        doh_server(DNS_PROXY, &PROXIED_DOH, Some(TAG_PROXY)),
        doh_server(DNS_DIRECT, direct_doh, None),
        doh_server(DNS_BOOTSTRAP, direct_doh, None),
        json!({ "type": "local", "tag": DNS_LOCAL }),
    ];
    let mut dns_rules = vec![
        // IPv4-only service: answer AAAA with an empty NOERROR.
        json!({ "query_type": ["AAAA"], "action": "predefined", "rcode": "NOERROR" }),
        json!({ "domain_suffix": LAN_SUFFIXES, "action": "route", "server": DNS_LOCAL }),
    ];
    if input.routing == Routing::Rule {
        dns_rules.push(json!({ "rule_set": ["geosite-cn"], "action": "route", "server": DNS_DIRECT }));
    }

    // --- route -------------------------------------------------------------
    let mut rules = vec![
        json!({ "action": "sniff" }),
        json!({ "protocol": "dns", "action": "hijack-dns" }),
        json!({ "ip_cidr": ["127.0.0.0/8", "::1/128"], "action": "route", "outbound": TAG_DIRECT }),
    ];
    let lan = if input.allow_lan {
        json!({ "action": "route", "outbound": TAG_DIRECT })
    } else {
        json!({ "action": "reject" })
    };
    let mut private = json!({ "ip_is_private": true });
    let mut lan_names = json!({ "domain_suffix": LAN_SUFFIXES });
    merge(&mut private, &lan);
    merge(&mut lan_names, &lan);
    rules.push(private);
    rules.push(lan_names);
    // IPv4-only: fail IPv6 destinations fast so apps fall back to IPv4.
    rules.push(json!({ "ip_version": 6, "action": "reject" }));
    if input.routing == Routing::Rule {
        rules.push(json!({ "rule_set": ["geosite-cn", "geoip-cn"], "action": "route", "outbound": TAG_DIRECT }));
    }

    let rule_set = vec![
        json!({ "type": "local", "tag": "geosite-cn", "format": "binary", "path": input.rule_sets.geosite_cn }),
        json!({ "type": "local", "tag": "geoip-cn", "format": "binary", "path": input.rule_sets.geoip_cn }),
    ];

    let tun = matches!(input.capture, Capture::Tun);
    let inbounds = match &input.capture {
        Capture::SystemProxy { port } => vec![json!({
            "type": "mixed",
            "tag": TAG_MIXED_IN,
            "listen": "127.0.0.1",
            "listen_port": port
        })],
        // DNS: with auto_route, sing-box 1.14 hijacks DNS by default
        // (dns_mode = hijack); the hijack-dns route rule handles it.
        Capture::Tun => vec![json!({
            "type": "tun",
            "tag": TAG_TUN_IN,
            "address": TUN_ADDRESSES,
            "auto_route": true,
            "strict_route": true
        })],
    };

    Ok(json!({
        "log": { "level": "warn", "timestamp": true },
        "dns": {
            "servers": servers,
            "rules": dns_rules,
            "final": DNS_PROXY,
            "strategy": "ipv4_only"
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": {
            "rules": rules,
            "rule_set": rule_set,
            "final": TAG_PROXY,
            "default_domain_resolver": DNS_BOOTSTRAP,
            // TUN: bind outbound sockets to the physical interface (no loops).
            "auto_detect_interface": tun
        },
        "experimental": {
            "clash_api": {
                "external_controller": format!("127.0.0.1:{}", input.controller_port),
                "secret": input.controller_secret
            }
        }
    }))
}

fn merge(target: &mut Value, extra: &Value) {
    if let (Some(t), Some(e)) = (target.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            t.insert(k.clone(), v.clone());
        }
    }
}

fn is_domain(server: &str) -> bool {
    matches!(validate::parse_host(server), Ok(Host::Domain(_)))
}

fn with_bootstrap(mut v: Value, server: &str) -> Value {
    if is_domain(server) {
        v["domain_resolver"] = json!(DNS_BOOTSTRAP);
    }
    v
}

fn hy2_outbound(tag: &str, h: &Hy2Config) -> Value {
    let mut tls = json!({ "enabled": true, "alpn": ["h3"], "insecure": h.insecure });
    match (&h.sni, is_domain(&h.server)) {
        (Some(sni), _) => tls["server_name"] = json!(sni),
        (None, true) => tls["server_name"] = json!(h.server),
        (None, false) => {}
    }
    let mut v = json!({
        "type": "hysteria2",
        "tag": tag,
        "server": h.server,
        "password": h.password.expose(),
        "tls": tls
    });
    if h.server_ports.is_empty() {
        v["server_port"] = json!(h.port);
    } else {
        // sing-box range syntax is "start:end".
        v["server_ports"] =
            json!(h.server_ports.iter().map(|r| format!("{}:{}", r.start, r.end)).collect::<Vec<_>>());
    }
    if let Some(up) = h.up_mbps {
        v["up_mbps"] = json!(up);
    }
    if let Some(down) = h.down_mbps {
        v["down_mbps"] = json!(down);
    }
    if let Some(pw) = &h.obfs_password {
        v["obfs"] = json!({ "type": "salamander", "password": pw.expose() });
    }
    with_bootstrap(v, &h.server)
}

fn vless_outbound(tag: &str, c: &VlessRealityConfig) -> Value {
    let mut tls = json!({
        "enabled": true,
        "server_name": c.sni,
        "utls": { "enabled": true, "fingerprint": c.fingerprint },
        "reality": { "enabled": true, "public_key": c.public_key.expose(), "short_id": c.short_id.expose() }
    });
    if !c.alpn.is_empty() {
        tls["alpn"] = json!(c.alpn);
    }
    let v = json!({
        "type": "vless",
        "tag": tag,
        "server": c.server,
        "server_port": c.port,
        "uuid": c.uuid.expose(),
        "flow": validate::VISION_FLOW,
        "tls": tls
    });
    with_bootstrap(v, &c.server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::parse_link;
    use std::path::Path;

    // Fake values only.
    fn hy2() -> NodeConfig {
        parse_link("hysteria2://FAKE-pass@proxy.example.com:24443/?sni=proxy.example.com&insecure=0#HY2")
            .unwrap()
            .config
    }
    fn hy2_hopping_obfs_ip() -> NodeConfig {
        parse_link("hy2://FAKE-pass@203.0.113.7:443,20000-30000/?obfs=salamander&obfs-password=FAKE-obfs&upmbps=50&downmbps=100")
            .unwrap()
            .config
    }
    fn vless() -> NodeConfig {
        parse_link(&format!(
            "vless://00000000-0000-4000-8000-0000000000aa@203.0.113.7:443?type=raw&security=reality&fp=chrome&sni=www.example.org&pbk={}E&sid=0123abcd&flow=xtls-rprx-vision#R",
            "B".repeat(42)
        ))
        .unwrap()
        .config
    }

    fn input<'a>(nodes: Vec<(&'a str, &'a NodeConfig)>, routing: Routing, dns: DnsProfile, allow_lan: bool, rs: &Path) -> GenInput<'a> {
        GenInput {
            selected: nodes[0].0,
            nodes,
            routing,
            dns,
            capture: Capture::SystemProxy { port: 17890 },
            allow_lan,
            controller_port: 17891,
            controller_secret: "FAKE-controller-secret",
            rule_sets: RuleSets { geosite_cn: rs.join("geosite-cn.srs"), geoip_cn: rs.join("geoip-cn.srs") },
            direct_fallback: false,
        }
    }

    #[test]
    fn errors() {
        let rs = Path::new("/rules");
        let a = hy2();
        let mut i = input(vec![("a", &a)], Routing::Rule, DnsProfile::Mainland, true, rs);
        i.selected = "missing";
        assert_eq!(generate(&i).unwrap_err(), GenError::SelectedNotFound);
        i.nodes.clear();
        assert_eq!(generate(&i).unwrap_err(), GenError::NoNodes);
    }

    #[test]
    fn outbounds_and_selector() {
        let (a, b, c) = (hy2(), hy2_hopping_obfs_ip(), vless());
        let cfg = generate(&input(vec![("a", &a), ("b", &b), ("c", &c)], Routing::Rule, DnsProfile::Mainland, true, Path::new("/r"))).unwrap();
        let ob = cfg["outbounds"].as_array().unwrap();
        assert_eq!(ob[0]["type"], "selector");
        assert_eq!(ob[0]["outbounds"], json!(["node-a", "node-b", "node-c"]));
        assert_eq!(ob[0]["default"], "node-a");
        // HY2 with domain server: bootstrap resolver, SNI, single port.
        assert_eq!(ob[1]["domain_resolver"], DNS_BOOTSTRAP);
        assert_eq!(ob[1]["server_port"], 24443);
        assert_eq!(ob[1]["tls"]["server_name"], "proxy.example.com");
        assert_eq!(ob[1]["tls"]["alpn"], json!(["h3"]));
        assert!(ob[1].get("up_mbps").is_none(), "no bandwidth hint → BBR");
        // HY2 with IP, hopping, obfs, bandwidth.
        assert!(ob[2].get("domain_resolver").is_none());
        assert!(ob[2].get("server_port").is_none());
        assert_eq!(ob[2]["server_ports"], json!(["443:443", "20000:30000"]));
        assert_eq!(ob[2]["obfs"], json!({"type": "salamander", "password": "FAKE-obfs"}));
        assert_eq!((ob[2]["up_mbps"].as_u64(), ob[2]["down_mbps"].as_u64()), (Some(50), Some(100)));
        assert!(ob[2]["tls"].get("server_name").is_none());
        // VLESS REALITY Vision.
        assert_eq!(ob[3]["flow"], "xtls-rprx-vision");
        assert_eq!(ob[3]["tls"]["reality"]["short_id"], "0123abcd");
        assert_eq!(ob[3]["tls"]["utls"]["fingerprint"], "chrome");
        assert_eq!(ob[3]["tls"]["server_name"], "www.example.org");
        assert_eq!(ob[4]["type"], "direct");
        let mut i = input(vec![("a", &a)], Routing::Rule, DnsProfile::Mainland, true, Path::new("/r"));
        i.direct_fallback = true;
        let cfg = generate(&i).unwrap();
        assert_eq!(cfg["outbounds"][0]["outbounds"], json!(["node-a", "direct"]), "direct last, never default");
        assert_eq!(cfg["outbounds"][0]["default"], "node-a");
    }

    #[test]
    fn rule_vs_global_and_lan() {
        let a = hy2();
        let rule = generate(&input(vec![("a", &a)], Routing::Rule, DnsProfile::Mainland, true, Path::new("/r"))).unwrap();
        let global = generate(&input(vec![("a", &a)], Routing::Global, DnsProfile::Mainland, false, Path::new("/r"))).unwrap();
        let has_cn = |c: &Value| c["route"]["rules"].as_array().unwrap().iter().any(|r| r.get("rule_set").is_some());
        assert!(has_cn(&rule));
        assert!(!has_cn(&global));
        assert_eq!(rule["route"]["final"], TAG_PROXY);
        assert_eq!(global["route"]["final"], TAG_PROXY);
        let private = |c: &Value| c["route"]["rules"].as_array().unwrap().iter().find(|r| r.get("ip_is_private").is_some()).cloned().unwrap();
        assert_eq!(private(&rule)["outbound"], TAG_DIRECT);
        assert_eq!(private(&global)["action"], "reject");
        // Loopback is always direct, and comes before the LAN rule.
        assert_eq!(global["route"]["rules"][2]["outbound"], TAG_DIRECT);
    }

    #[test]
    fn tun_capture() {
        let a = hy2();
        let mut i = input(vec![("a", &a)], Routing::Rule, DnsProfile::Mainland, true, Path::new("/r"));
        i.capture = Capture::Tun;
        let cfg = generate(&i).unwrap();
        let tun = &cfg["inbounds"][0];
        assert_eq!(tun["type"], "tun");
        assert_eq!(tun["auto_route"], true);
        assert_eq!(tun["strict_route"], true);
        assert!(tun["address"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains(':')), "IPv6 captured");
        assert_eq!(cfg["route"]["auto_detect_interface"], true);
        // IPv6 destinations are rejected (fast fail) — captured by the TUN above.
        assert!(cfg["route"]["rules"].as_array().unwrap().iter().any(|r| r["ip_version"] == 6 && r["action"] == "reject"));
        let sp = generate(&input(vec![("a", &a)], Routing::Rule, DnsProfile::Mainland, true, Path::new("/r"))).unwrap();
        assert_eq!(sp["route"]["auto_detect_interface"], false);
    }

    #[test]
    fn dns_profiles() {
        let a = hy2();
        let ml = generate(&input(vec![("a", &a)], Routing::Rule, DnsProfile::Mainland, true, Path::new("/r"))).unwrap();
        let os = generate(&input(vec![("a", &a)], Routing::Rule, DnsProfile::Overseas, true, Path::new("/r"))).unwrap();
        let server = |c: &Value, tag: &str| c["dns"]["servers"].as_array().unwrap().iter().find(|s| s["tag"] == tag).cloned().unwrap();
        assert_eq!(server(&ml, DNS_DIRECT)["server"], "223.5.5.5");
        assert_eq!(server(&os, DNS_DIRECT)["server"], "9.9.9.9");
        assert_eq!(server(&ml, DNS_PROXY)["detour"], TAG_PROXY);
        assert!(server(&ml, DNS_DIRECT).get("detour").is_none(), "direct resolver never via tunnel");
        assert!(server(&ml, DNS_BOOTSTRAP).get("detour").is_none(), "bootstrap never via tunnel");
        assert_eq!(ml["dns"]["final"], DNS_PROXY);
        assert_eq!(ml["dns"]["strategy"], "ipv4_only");
    }

    /// Snapshot of a full config with fake values. `UPDATE_SNAPSHOTS=1` rewrites.
    #[test]
    fn snapshots() {
        let (a, b, c) = (hy2(), hy2_hopping_obfs_ip(), vless());
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
        for (name, routing, dns, lan) in [
            ("rule-mainland", Routing::Rule, DnsProfile::Mainland, true),
            ("global-overseas", Routing::Global, DnsProfile::Overseas, false),
        ] {
            let cfg = generate(&input(vec![("a", &a), ("b", &b), ("c", &c)], routing, dns, lan, Path::new("/rules"))).unwrap();
            let text = serde_json::to_string_pretty(&cfg).unwrap() + "\n";
            let path = dir.join(format!("singbox-{name}.json"));
            if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &text).unwrap();
            } else {
                let expected = std::fs::read_to_string(&path).expect("snapshot missing: run with UPDATE_SNAPSHOTS=1");
                assert!(expected == text, "snapshot {name} differs: review, then UPDATE_SNAPSHOTS=1");
            }
        }
    }

    /// Validates generated configs with the real `sing-box check` (pinned
    /// version) when the core has been fetched; tiny rule-sets are compiled
    /// from JSON sources with `sing-box rule-set compile`.
    #[test]
    fn sing_box_check() {
        let Some(bin) = fetched_core() else {
            eprintln!("skipped: run `npm run fetch-core` to enable sing-box check");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        for (name, rules) in [
            ("geosite-cn", json!({"version": 3, "rules": [{"domain_suffix": ["cn"]}]})),
            ("geoip-cn", json!({"version": 3, "rules": [{"ip_cidr": ["1.0.1.0/24"]}]})),
        ] {
            let src = dir.path().join(format!("{name}.json"));
            std::fs::write(&src, rules.to_string()).unwrap();
            let out = std::process::Command::new(&bin)
                .args(["rule-set", "compile", "--output"])
                .arg(dir.path().join(format!("{name}.srs")))
                .arg(&src)
                .output()
                .unwrap();
            assert!(out.status.success(), "compile {name}: {}", String::from_utf8_lossy(&out.stderr));
        }
        let (a, b, c) = (hy2(), hy2_hopping_obfs_ip(), vless());
        for routing in [Routing::Rule, Routing::Global] {
            for dns in [DnsProfile::Mainland, DnsProfile::Overseas] {
                for lan in [true, false] {
                  for tun in [false, true] {
                    let mut i = input(vec![("a", &a), ("b", &b), ("c", &c)], routing, dns, lan, dir.path());
                    if tun {
                        i.capture = Capture::Tun;
                    }
                    i.direct_fallback = lan; // exercise both selector shapes
                    let cfg = generate(&i).unwrap();
                    let path = dir.path().join("config.json");
                    std::fs::write(&path, serde_json::to_vec(&cfg).unwrap()).unwrap();
                    let out = std::process::Command::new(&bin).args(["check", "-c"]).arg(&path).output().unwrap();
                    assert!(
                        out.status.success(),
                        "sing-box check failed for {routing:?}/{dns:?}/lan={lan}/tun={tun}: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                  }
                }
            }
        }
        // Negative controls: `check` must reject bad configs, otherwise the
        // checks above prove nothing. Unknown fields are rejected, which also
        // confirms sing-box 1.14 REALITY has no `spider_x` option.
        let good = generate(&input(vec![("c", &c)], Routing::Rule, DnsProfile::Mainland, true, dir.path())).unwrap();
        let mut bad_type = good.clone();
        bad_type["outbounds"][1]["type"] = json!("bogus");
        let mut spider = good.clone();
        spider["outbounds"][1]["tls"]["reality"]["spider_x"] = json!("/");
        for (what, cfg) in [("unknown outbound type", bad_type), ("reality.spider_x", spider)] {
            let path = dir.path().join("bad.json");
            std::fs::write(&path, serde_json::to_vec(&cfg).unwrap()).unwrap();
            let out = std::process::Command::new(&bin).args(["check", "-c"]).arg(&path).output().unwrap();
            assert!(!out.status.success(), "sing-box check accepted a config with {what}");
        }
    }

    fn fetched_core() -> Option<std::path::PathBuf> {
        let triple = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "aarch64-apple-darwin"
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            "x86_64-pc-windows-msvc.exe"
        } else {
            return None;
        };
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../src-tauri/binaries/sing-box-{triple}"));
        p.is_file().then_some(p)
    }
}
