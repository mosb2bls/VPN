//! macOS pf kill-switch rules (anchor `com.apple/250.EdgeClient`, loaded by
//! the stock `/etc/pf.conf` via `anchor "com.apple/*"` — pf.conf is never
//! edited). Engaged by the helper only while the TUN core is down
//! unexpectedly; removed when it is back or the user disconnects.
//!
//! Allows: loopback; DHCP; link-local / multicast; LAN ranges (if allowed);
//! the proxy servers on their exact ports; the bootstrap DoH resolvers on 443
//! (needed to resolve a domain server address on restart). Everything else
//! outbound is **rejected** (`block return`: TCP RST / ICMP unreachable) so
//! apps fail fast rather than hang.

use std::net::IpAddr;

pub const ANCHOR: &str = "com.apple/250.EdgeClient";

/// Bootstrap resolvers (must match `singbox` DoH constants).
pub const BOOTSTRAP_RESOLVERS: &[&str] = &["223.5.5.5", "9.9.9.9"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRule {
    pub ip: IpAddr,
    /// e.g. ("udp", "24443") or ("udp", "20000:30000") or ("tcp", "443").
    pub proto: &'static str,
    pub ports: String,
}

pub fn rules(servers: &[ServerRule], allow_lan: bool) -> String {
    let mut r = String::new();
    r.push_str("# EdgeClient kill switch (generated; removed on disconnect)\n");
    r.push_str("pass out quick on lo0 all\n");
    r.push_str("pass out quick inet proto udp from any port 68 to any port 67\n");
    r.push_str("pass out quick inet to { 169.254.0.0/16, 224.0.0.0/4, 255.255.255.255 }\n");
    r.push_str("pass out quick inet6 to { fe80::/10, ff00::/8 }\n");
    if allow_lan {
        r.push_str("pass out quick inet to { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 100.64.0.0/10 }\n");
    }
    for s in servers {
        let fam = if s.ip.is_ipv6() { "inet6" } else { "inet" };
        r.push_str(&format!("pass out quick {fam} proto {} to {} port {}\n", s.proto, s.ip, s.ports));
    }
    for ip in BOOTSTRAP_RESOLVERS {
        r.push_str(&format!("pass out quick inet proto tcp to {ip} port 443\n"));
    }
    r.push_str("block return out quick all\n");
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_text() {
        let servers = vec![
            ServerRule { ip: "203.0.113.7".parse().unwrap(), proto: "udp", ports: "24443".into() },
            ServerRule { ip: "203.0.113.7".parse().unwrap(), proto: "tcp", ports: "443".into() },
            ServerRule { ip: "203.0.113.8".parse().unwrap(), proto: "udp", ports: "20000:30000".into() },
        ];
        let t = rules(&servers, true);
        assert!(t.contains("pass out quick inet proto udp to 203.0.113.7 port 24443\n"));
        assert!(t.contains("pass out quick inet proto udp to 203.0.113.8 port 20000:30000\n"));
        assert!(t.contains("192.168.0.0/16"));
        assert!(t.trim_end().ends_with("block return out quick all"), "reject last");
        assert!(!rules(&servers, false).contains("192.168.0.0/16"));
        assert!(t.lines().position(|l| l.contains("lo0")) < t.lines().position(|l| l.starts_with("block")));
    }
}
