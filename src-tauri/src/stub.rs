//! PHASE B1 PLACEHOLDER STATE — in-memory only, not persisted, touches no
//! OS state. Replaced piece by piece from Phase B2 on. Every placeholder node
//! is named "[placeholder] …" and uses documentation-only values.

use crate::api::types::*;

pub struct StubState {
    pub nodes: Vec<Node>,
    pub selected_node_id: Option<String>,
    pub status: Status,
    pub failover: FailoverSettings,
    pub safety: SafetySettings,
    pub settings: AppSettings,
}

pub const PLACEHOLDER_HY2_ID: &str = "00000000-0000-4000-8000-000000000001";
pub const PLACEHOLDER_VLESS_ID: &str = "00000000-0000-4000-8000-000000000002";

impl Default for StubState {
    fn default() -> Self {
        let nodes = vec![
            Node {
                id: PLACEHOLDER_HY2_ID.into(),
                name: "[placeholder] UK HY2".into(),
                protocol: Protocol::Hysteria2,
                server: "proxy.example.com".into(),
                port: 24443,
                details: NodeDetails::Hysteria2(Hy2Details {
                    password_masked: "a1b2…••••".into(),
                    server_ports: vec![],
                    sni: Some("proxy.example.com".into()),
                    insecure: false,
                    obfs_password_masked: None,
                    pin_sha256: None,
                    up_mbps: None,
                    down_mbps: None,
                }),
                last_test: None,
            },
            Node {
                id: PLACEHOLDER_VLESS_ID.into(),
                name: "[placeholder] UK REALITY".into(),
                protocol: Protocol::VlessReality,
                server: "proxy.example.com".into(),
                port: 443,
                details: NodeDetails::VlessReality(VlessDetails {
                    uuid_masked: "c3d4…••••".into(),
                    flow: "xtls-rprx-vision".into(),
                    sni: "www.example.org".into(),
                    fingerprint: "chrome".into(),
                    public_key_masked: "e5f6…••••".into(),
                    short_id_masked: "••••".into(),
                    spider_x: None,
                }),
                last_test: None,
            },
        ];
        let selected = Some(PLACEHOLDER_HY2_ID.to_string());
        Self {
            nodes,
            selected_node_id: selected.clone(),
            status: Status {
                state: ConnectionState::Disconnected,
                capture_mode: CaptureMode::SystemProxy,
                routing_mode: RoutingMode::Rule,
                dns_profile: DnsProfile::Mainland,
                selected_node_id: selected,
                active_node_id: None,
                connected_since: None,
                kill_switch_active: false,
                system_proxy_set: false,
                failover: None,
                last_error: None,
                last_recovery: None,
            },
            failover: default_failover_settings(),
            safety: SafetySettings { kill_switch: true, direct_fallback: false, allow_lan: true },
            settings: AppSettings {
                launch_at_login: false,
                auto_connect: false,
                close_to_tray: true,
                rules_auto_update: true,
                rules_update_interval_hours: 24,
                log_level: LogLevel::Info,
            },
        }
    }
}

pub fn default_failover_settings() -> FailoverSettings {
    FailoverSettings {
        enabled: true,
        fallback_node_id: None,
        probe_targets: vec![
            "https://www.gstatic.com/generate_204".into(),
            "https://cp.cloudflare.com/".into(),
        ],
        probe_interval_secs: 10,
        probe_timeout_secs: 5,
        failure_rounds: 3,
        min_dwell_secs: 300,
        return_stable_secs: 600,
        max_switches_per_hour: 4,
    }
}
