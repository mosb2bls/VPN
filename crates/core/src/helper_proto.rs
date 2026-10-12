//! App ↔ privileged-helper protocol (macOS Unix socket; Windows named pipe
//! later). One JSON request per line, one JSON response per line.
//!
//! The app never sends a sing-box config: it sends a [`TunProfile`] (nodes,
//! modes) and the helper builds and validates the config itself with
//! `edge_core::singbox::generate`. Secrets in the profile only travel over the
//! authenticated socket and are never logged or persisted by the helper.

use serde::{Deserialize, Serialize};

use crate::error_code::ErrorCode;
use crate::link::{Hy2Config, NodeConfig, VlessRealityConfig};
use crate::secret::Secret;
use crate::validate::PortRange;

pub const PROTOCOL_VERSION: u32 = 1;

/// Socket path (macOS). Owned by the installing user, mode 0600.
pub const SOCKET_PATH: &str = "/var/run/io.github.mosb2bls.edgeclient.helper.sock";
/// Root-owned install locations (macOS).
pub const INSTALL_DIR: &str = "/Library/PrivilegedHelperTools";
pub const HELPER_BIN: &str = "/Library/PrivilegedHelperTools/io.github.mosb2bls.edgeclient.helper";
pub const HELPER_SINGBOX: &str = "/Library/PrivilegedHelperTools/io.github.mosb2bls.edgeclient.sing-box";
pub const HELPER_DATA: &str = "/Library/Application Support/io.github.mosb2bls.edgeclient.helper";
pub const LAUNCHD_LABEL: &str = "io.github.mosb2bls.edgeclient.helper";
pub const LAUNCHD_PLIST: &str = "/Library/LaunchDaemons/io.github.mosb2bls.edgeclient.helper.plist";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum WireNode {
    Hysteria2 {
        id: String,
        server: String,
        port: u16,
        server_ports: Vec<String>,
        password: String,
        sni: Option<String>,
        insecure: bool,
        obfs_password: Option<String>,
        up_mbps: Option<u32>,
        down_mbps: Option<u32>,
    },
    VlessReality {
        id: String,
        server: String,
        port: u16,
        uuid: String,
        sni: String,
        fingerprint: String,
        public_key: String,
        short_id: String,
        alpn: Vec<String>,
    },
}

impl std::fmt::Debug for TunProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TunProfile")
            .field("nodes", &self.nodes.len())
            .field("selected", &self.selected)
            .field("routing", &self.routing)
            .field("dns", &self.dns)
            .finish_non_exhaustive()
    }
}

impl WireNode {
    pub fn id(&self) -> &str {
        match self {
            WireNode::Hysteria2 { id, .. } | WireNode::VlessReality { id, .. } => id,
        }
    }

    pub fn from_config(id: &str, c: &NodeConfig) -> Self {
        match c {
            NodeConfig::Hysteria2(h) => WireNode::Hysteria2 {
                id: id.into(),
                server: h.server.clone(),
                port: h.port,
                server_ports: h.server_ports.iter().map(|r| r.to_display()).collect(),
                password: h.password.expose().into(),
                sni: h.sni.clone(),
                insecure: h.insecure,
                obfs_password: h.obfs_password.as_ref().map(|s| s.expose().into()),
                up_mbps: h.up_mbps,
                down_mbps: h.down_mbps,
            },
            NodeConfig::VlessReality(v) => WireNode::VlessReality {
                id: id.into(),
                server: v.server.clone(),
                port: v.port,
                uuid: v.uuid.expose().into(),
                sni: v.sni.clone(),
                fingerprint: v.fingerprint.clone(),
                public_key: v.public_key.expose().into(),
                short_id: v.short_id.expose().into(),
                alpn: v.alpn.clone(),
            },
        }
    }

    /// Re-validate every field (the helper trusts nothing from the socket).
    pub fn to_config(&self) -> Result<NodeConfig, ErrorCode> {
        use crate::validate as v;
        Ok(match self {
            WireNode::Hysteria2 { server, port, server_ports, password, sni, insecure, obfs_password, up_mbps, down_mbps, .. } => {
                if password.is_empty() {
                    return Err(ErrorCode::FieldRequired);
                }
                let ports: Vec<PortRange> =
                    if server_ports.is_empty() { vec![] } else { v::parse_port_list(&server_ports.join(","))? };
                NodeConfig::Hysteria2(Hy2Config {
                    server: v::validate_server(server)?,
                    port: v::validate_port(&port.to_string())?,
                    server_ports: ports,
                    password: Secret::new(password.clone()),
                    sni: sni.as_deref().map(v::validate_sni).transpose()?,
                    insecure: *insecure,
                    obfs_password: obfs_password.clone().filter(|p| !p.is_empty()).map(Secret::new),
                    up_mbps: up_mbps.map(|x| v::validate_bandwidth(&x.to_string())).transpose()?,
                    down_mbps: down_mbps.map(|x| v::validate_bandwidth(&x.to_string())).transpose()?,
                })
            }
            WireNode::VlessReality { server, port, uuid, sni, fingerprint, public_key, short_id, alpn, .. } => {
                if alpn.iter().any(|a| a.is_empty() || a.contains(|c: char| c.is_whitespace() || c == ',')) {
                    return Err(ErrorCode::FieldInvalid);
                }
                NodeConfig::VlessReality(VlessRealityConfig {
                    server: v::validate_server(server)?,
                    port: v::validate_port(&port.to_string())?,
                    uuid: Secret::new(v::validate_uuid(uuid)?),
                    sni: v::validate_sni(sni)?,
                    fingerprint: v::validate_fingerprint(fingerprint)?,
                    public_key: Secret::new(v::validate_public_key(public_key)?),
                    short_id: Secret::new(v::validate_short_id(short_id)?),
                    spider_x: None,
                    alpn: alpn.clone(),
                })
            }
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireRouting {
    Rule,
    Global,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireDns {
    Mainland,
    Overseas,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TunProfile {
    pub nodes: Vec<WireNode>,
    pub selected: String,
    pub routing: WireRouting,
    pub dns: WireDns,
    pub allow_lan: bool,
    pub kill_switch: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Version handshake / liveness.
    Ping,
    Status,
    /// Start (or replace) the TUN session.
    Start { profile: Box<TunProfile> },
    /// Stop the TUN session and remove the kill switch.
    Stop,
    /// Like Stop, plus clean up anything left from a crashed session; returns a
    /// per-item report.
    Restore,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HelperState {
    pub version: String,
    pub protocol: u32,
    /// SHA-256 of the sing-box binary the helper runs (app compares with its own).
    pub core_sha256: String,
    /// sing-box TUN core running.
    pub running: bool,
    /// Restarting after a crash (kill switch engaged if enabled).
    pub restarting: bool,
    pub kill_switch_active: bool,
    /// Clash API of the running core (for node switching / health / stats).
    pub controller_port: Option<u16>,
    pub controller_secret: Option<String>,
    pub selected: Option<String>,
    /// What the helper did at its own start (journal found → restored).
    pub startup_recovery: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WireRestoreItem {
    /// "tun_session" | "routes" | "dns" | "ipv6" | "firewall"
    pub kind: String,
    /// "restored" | "not_needed" | "failed" | "skipped"
    pub result: String,
    pub action: String,
    pub error_code: Option<ErrorCode>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok { state: HelperState },
    Restored { state: HelperState, items: Vec<WireRestoreItem> },
    Error { code: ErrorCode, detail: Option<String> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::parse_link;

    #[test]
    fn wire_roundtrip_and_validation() {
        let hy = parse_link("hy2://FAKE-pass@proxy.example.com:443,20000-30000/?obfs=salamander&obfs-password=FAKE-o&upmbps=50")
            .unwrap()
            .config;
        let w = WireNode::from_config("a", &hy);
        let json = serde_json::to_string(&Request::Start {
            profile: Box::new(TunProfile {
                nodes: vec![w.clone()],
                selected: "a".into(),
                routing: WireRouting::Rule,
                dns: WireDns::Mainland,
                allow_lan: true,
                kill_switch: true,
            }),
        })
        .unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        let Request::Start { profile } = back else { panic!() };
        assert_eq!(profile.nodes[0].to_config().unwrap(), hy);
        // Debug never prints secrets.
        assert!(!format!("{profile:?}").contains("FAKE"));

        let mut bad = w;
        if let WireNode::Hysteria2 { server, .. } = &mut bad {
            *server = "[2001:db8::1]".into();
        }
        assert_eq!(bad.to_config().unwrap_err(), ErrorCode::FieldInvalidHost);
    }

    #[test]
    fn unknown_ops_are_rejected() {
        assert!(serde_json::from_str::<Request>(r#"{"op":"exec","cmd":"rm -rf /"}"#).is_err());
        assert!(serde_json::from_str::<Request>(r#"{"op":"ping"}"#).is_ok());
    }
}
