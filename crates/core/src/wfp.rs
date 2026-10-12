//! Windows kill-switch plan (Windows Filtering Platform, ADR-0007) — the
//! Windows counterpart of [`crate::pf`]. Pure: `edge_winsys::wfp` turns each
//! [`FilterSpec`] into a filter at `FWPM_LAYER_ALE_AUTH_CONNECT_V4/V6` in our
//! own sublayer, inside a *dynamic* WFP session (filters vanish if the helper
//! process dies, so a crash never strands the PC offline).
//!
//! Engaged only while the TUN core is down unexpectedly. Allows: loopback;
//! the installed sing-box binary (by app ID — covers the proxy servers and the
//! bootstrap DoH resolvers it needs to reconnect); DHCP; link-local /
//! multicast / broadcast; LAN ranges (if allowed). Everything else outbound
//! is blocked (connect fails immediately with an access error).

use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    V4,
    V6,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    /// `FWP_CONDITION_FLAG_IS_LOOPBACK`.
    Loopback,
    /// `FWPM_CONDITION_ALE_APP_ID` of the installed sing-box.
    CoreApp,
    /// UDP from local port to remote port (DHCP).
    Udp { local_port: u16, remote_port: u16 },
    RemoteV4 { addr: Ipv4Addr, prefix: u8 },
    RemoteV6 { addr: Ipv6Addr, prefix: u8 },
    /// No condition.
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterSpec {
    pub family: Family,
    pub permit: bool,
    pub matches: Match,
    /// Higher wins inside our sublayer; the catch-all block has weight 0.
    pub weight: u8,
}

pub fn plan(allow_lan: bool) -> Vec<FilterSpec> {
    let permit = |family, matches| FilterSpec { family, permit: true, matches, weight: 10 };
    let v4 = |a: [u8; 4], prefix| Match::RemoteV4 { addr: Ipv4Addr::from(a), prefix };
    let mut out = Vec::new();
    for family in [Family::V4, Family::V6] {
        out.push(permit(family, Match::Loopback));
        out.push(permit(family, Match::CoreApp));
    }
    out.push(permit(Family::V4, Match::Udp { local_port: 68, remote_port: 67 }));
    out.push(permit(Family::V6, Match::Udp { local_port: 546, remote_port: 547 }));
    for (a, p) in [([169, 254, 0, 0], 16), ([224, 0, 0, 0], 4), ([255, 255, 255, 255], 32)] {
        out.push(permit(Family::V4, v4(a, p)));
    }
    for (a, p) in [(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10), (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8)] {
        out.push(permit(Family::V6, Match::RemoteV6 { addr: a, prefix: p }));
    }
    if allow_lan {
        for (a, p) in [([10, 0, 0, 0], 8), ([172, 16, 0, 0], 12), ([192, 168, 0, 0], 16), ([100, 64, 0, 0], 10)] {
            out.push(permit(Family::V4, v4(a, p)));
        }
    }
    for family in [Family::V4, Family::V6] {
        out.push(FilterSpec { family, permit: false, matches: Match::All, weight: 0 });
    }
    out
}

/// IPv4 prefix length → netmask (host order, as `FWP_V4_ADDR_AND_MASK` wants).
pub fn v4_mask(prefix: u8) -> u32 {
    if prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(prefix.min(32))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_permits_then_blocks_each_family() {
        let p = plan(true);
        for fam in [Family::V4, Family::V6] {
            let blocks: Vec<_> = p.iter().filter(|f| f.family == fam && !f.permit).collect();
            assert_eq!(blocks.len(), 1);
            assert_eq!((blocks[0].matches.clone(), blocks[0].weight), (Match::All, 0));
            assert!(p.iter().filter(|f| f.family == fam && f.permit).all(|f| f.weight > 0));
            assert!(p.contains(&FilterSpec { family: fam, permit: true, matches: Match::CoreApp, weight: 10 }));
            assert!(p.contains(&FilterSpec { family: fam, permit: true, matches: Match::Loopback, weight: 10 }));
        }
        let lan = Match::RemoteV4 { addr: Ipv4Addr::new(192, 168, 0, 0), prefix: 16 };
        assert!(p.iter().any(|f| f.matches == lan));
        assert!(!plan(false).iter().any(|f| f.matches == lan));
        // No permit for arbitrary internet addresses.
        assert!(!p.iter().any(|f| f.permit && f.matches == Match::All));
    }

    #[test]
    fn masks() {
        assert_eq!(v4_mask(0), 0);
        assert_eq!(v4_mask(8), 0xff00_0000);
        assert_eq!(v4_mask(12), 0xfff0_0000);
        assert_eq!(v4_mask(32), u32::MAX);
    }
}
