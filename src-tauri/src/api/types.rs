//! Data types exchanged with the UI. These are the contract described in
//! `docs/SPEC.md`; any change here must follow the SPEC versioning rules.
//!
//! Conventions:
//! - Struct fields are camelCase in TypeScript; enum values are snake_case.
//! - No 64-bit integers (specta forbids them): counters use `u32`, byte totals `f64`.
//! - Timestamps are RFC 3339 UTC strings.
//! - Secrets never leave the backend in full: `*Masked` fields hold e.g. `a1b2…••••`.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::error::AppError;

pub use edge_core::restore::{RestoreItem, RestoreItemKind, RestoreResult, RestoreTrigger};

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Hysteria2,
    VlessReality,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Stable opaque ID (UUID v4), assigned by the backend.
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    /// Server hostname or IPv4 literal (shown to the user; not a secret).
    pub server: String,
    pub port: u16,
    pub details: NodeDetails,
    /// Result of the most recent `test_node`, if any.
    pub last_test: Option<NodeTestResult>,
    /// True if this node's credentials are missing from / unreadable in the OS
    /// keychain. Connecting fails with `SECRET_MISSING`; the user must edit the
    /// node and re-enter them. Masked fields are empty in that case.
    pub secret_missing: bool,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum NodeDetails {
    Hysteria2(Hy2Details),
    VlessReality(VlessDetails),
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Hy2Details {
    pub password_masked: String,
    /// Port-hopping ranges, e.g. `["20000-30000"]`. Empty = single `port`.
    pub server_ports: Vec<String>,
    pub sni: Option<String>,
    /// Skip TLS certificate verification. Dangerous; see SPEC "Notes for UI".
    pub insecure: bool,
    /// Salamander obfuscation password (masked) if obfs is enabled.
    pub obfs_password_masked: Option<String>,
    /// Certificate pin (SHA-256, hex). Not secret.
    pub pin_sha256: Option<String>,
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VlessDetails {
    pub uuid_masked: String,
    /// Always `xtls-rprx-vision` (other flows are rejected on import).
    pub flow: String,
    pub sni: String,
    /// uTLS fingerprint, e.g. `chrome`.
    pub fingerprint: String,
    pub public_key_masked: String,
    pub short_id_masked: String,
    /// REALITY spiderX path, if set. Not secret.
    pub spider_x: Option<String>,
}

/// Input for a secret field in a form. On add, required secrets must be `set`.
/// On update, `keep` leaves the stored value unchanged; `clear` is only valid
/// for optional secrets (HY2 obfs password).
#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SecretInput {
    Keep,
    Set(String),
    Clear,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum NodeForm {
    Hysteria2(Hy2Form),
    VlessReality(VlessForm),
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Hy2Form {
    pub name: String,
    pub server: String,
    pub port: u16,
    pub server_ports: Vec<String>,
    pub password: SecretInput,
    pub sni: Option<String>,
    pub insecure: bool,
    pub obfs_password: SecretInput,
    pub pin_sha256: Option<String>,
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VlessForm {
    pub name: String,
    pub server: String,
    pub port: u16,
    pub uuid: SecretInput,
    pub sni: String,
    pub fingerprint: String,
    pub public_key: SecretInput,
    pub short_id: SecretInput,
    pub spider_x: Option<String>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: Vec<Node>,
    pub errors: Vec<ImportLineError>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportLineError {
    /// 1-based line number in the input (always 1 for QR).
    pub line: u32,
    /// Detected scheme (`hy2`, `vless`, …) if any. Never the link itself.
    pub scheme: Option<String>,
    pub error: AppError,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QrImageSource {
    /// Absolute path to a PNG/JPEG file (e.g. from drag-and-drop).
    Path { path: String },
    /// Base64-encoded PNG/JPEG bytes (e.g. pasted image).
    Bytes { base64: String },
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeTestResult {
    pub node_id: String,
    pub ok: bool,
    /// Median HTTP round-trip through the node, if ok.
    pub latency_ms: Option<u32>,
    /// ISO 3166-1 alpha-2 country of the exit IP, if determined.
    pub exit_country: Option<String>,
    pub error: Option<AppError>,
    pub tested_at: String,
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    SystemProxy,
    Tun,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    Rule,
    Global,
    Direct,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DnsProfile {
    Mainland,
    Overseas,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Degraded,
    FailingOver,
    Reconnecting,
    Direct,
    Disconnecting,
    Recovering,
    Error,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: ConnectionState,
    pub capture_mode: CaptureMode,
    pub routing_mode: RoutingMode,
    pub dns_profile: DnsProfile,
    /// Node the user chose.
    pub selected_node_id: Option<String>,
    /// Node currently carrying traffic (differs from selected after failover).
    pub active_node_id: Option<String>,
    pub connected_since: Option<String>,
    /// Firewall kill switch currently engaged (TUN only).
    pub kill_switch_active: bool,
    /// OS system proxy currently set by this app.
    pub system_proxy_set: bool,
    /// Present while running on the fallback node after automatic failover.
    pub failover: Option<FailoverStatus>,
    /// Last error that put us in `degraded`/`error`/`reconnecting`.
    pub last_error: Option<AppError>,
    /// What happened at the last launch-time or helper-start recovery.
    pub last_recovery: Option<RecoveryReport>,
    /// Helper-run TUN session currently in effect (REQ-4: actual, not intended).
    pub tun_active: bool,
    /// Most recent restore (disconnect, restore_networking, quit, mode switch,
    /// recovery). An incomplete report persists across restarts until a later
    /// restore completes fully. (REQ-4)
    pub last_restore: Option<RestoreReport>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FailoverStatus {
    pub primary_node_id: String,
    pub fallback_node_id: String,
    pub since: String,
    pub reason: String,
    /// Earliest time automatic return to the primary is possible.
    pub earliest_return_at: Option<String>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrafficStats {
    pub up_bytes_per_sec: f64,
    pub down_bytes_per_sec: f64,
    /// Totals since connect.
    pub up_bytes_total: f64,
    pub down_bytes_total: f64,
    pub active_connections: u32,
    /// Median probe latency of the most recent *passing* health round on the
    /// active node; `None` if no round yet or the last round failed. (REQ-2)
    pub latency_ms: Option<u32>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    /// A healthy session survived (e.g. helper kept TUN up); app re-attached.
    Reattached,
    /// Previous OS state restored from the journal.
    Restored,
    /// Some items could not be restored; see `actions`.
    RestoreIncomplete,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub outcome: RecoveryOutcome,
    pub at: String,
    /// The restore that recovery ran; `None` when the session was re-attached. (REQ-4)
    pub restore: Option<RestoreReport>,
}

/// Per-item restore report (REQ-4). Produced identically by `disconnect`,
/// `restore_networking`, `quit_app`, mode switches and recovery.
#[derive(Serialize, Deserialize, Type, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub at: String,
    pub trigger: RestoreTrigger,
    /// True only if every item is `restored` or `not_needed`.
    pub fully_restored: bool,
    /// Exactly one entry per `RestoreItemKind`, in execution order:
    /// tun_session, routes, dns, ipv6, system_proxy, firewall.
    pub items: Vec<RestoreItem>,
}

impl From<edge_core::restore::CoreRestoreReport> for RestoreReport {
    fn from(r: edge_core::restore::CoreRestoreReport) -> Self {
        Self { at: r.at, trigger: r.trigger, fully_restored: r.fully_restored, items: r.items }
    }
}

// ---------------------------------------------------------------------------
// Failover / safety
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FailoverSettings {
    pub enabled: bool,
    /// Node to fail over to. `None` = automatic: first node of the other
    /// protocol in list order.
    pub fallback_node_id: Option<String>,
    /// Probe target URLs (HTTP 204/200). Must contain ≥ 2 distinct hosts.
    pub probe_targets: Vec<String>,
    pub probe_interval_secs: u32,
    pub probe_timeout_secs: u32,
    /// Consecutive failed rounds (all targets failed) before switching.
    pub failure_rounds: u32,
    /// Minimum time on a node after any switch.
    pub min_dwell_secs: u32,
    /// Primary must pass continuously this long before switching back.
    pub return_stable_secs: u32,
    pub max_switches_per_hour: u32,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SafetySettings {
    /// TUN only: firewall kill switch while the core is down. Default on.
    pub kill_switch: bool,
    /// If every proxy node fails, send traffic direct instead of blocking.
    /// Default off (fail closed).
    pub direct_fallback: bool,
    /// Allow LAN / private-range traffic to bypass the proxy. Default on.
    pub allow_lan: bool,
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RulesSource {
    Bundled,
    Downloaded,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetInfo {
    /// e.g. `geoip-cn`, `geosite-cn`.
    pub name: String,
    pub size_bytes: u32,
    pub sha256_prefix: String,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RulesStatus {
    pub version: String,
    pub source: RulesSource,
    pub updated_at: Option<String>,
    pub last_check_at: Option<String>,
    pub next_check_at: Option<String>,
    pub updating: bool,
    pub sets: Vec<RuleSetInfo>,
    pub last_error: Option<AppError>,
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub expected_version: String,
    pub needs_upgrade: bool,
}

// ---------------------------------------------------------------------------
// Windows (REQ-1)
// ---------------------------------------------------------------------------

/// Section of the settings window. (REQ-1)
#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettingsSection {
    Nodes,
    Connection,
    Failover,
    Safety,
    Rules,
    Helper,
    Startup,
    Logs,
    About,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub launch_at_login: bool,
    /// On launch, connect with the last node, routing mode and capture mode.
    pub auto_connect: bool,
    /// Closing the window hides it to the tray instead of quitting.
    pub close_to_tray: bool,
    pub rules_auto_update: bool,
    pub rules_update_interval_hours: u32,
    pub log_level: LogLevel,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    App,
    Core,
    Helper,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub ts: String,
    pub level: LogLevel,
    pub source: LogSource,
    /// Already redacted.
    pub message: String,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExport {
    /// Absolute path of the written (redacted) zip.
    pub path: String,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Macos,
    Windows,
    Other,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_name: String,
    pub app_version: String,
    pub spec_version: String,
    /// Pinned sing-box version, or `None` until bundled (Phase B2).
    pub core_version: Option<String>,
    pub platform: Platform,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Wire shapes must match the generated TS types (internally tagged unions).
    #[test]
    fn tagged_union_wire_shapes() {
        let d = NodeDetails::VlessReality(VlessDetails {
            uuid_masked: "u".into(),
            flow: "xtls-rprx-vision".into(),
            sni: "s".into(),
            fingerprint: "chrome".into(),
            public_key_masked: "k".into(),
            short_id_masked: "i".into(),
            spider_x: None,
        });
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["protocol"], "vless_reality");
        assert_eq!(v["uuidMasked"], "u");

        assert_eq!(serde_json::to_value(SecretInput::Keep).unwrap(), json!({"kind": "keep"}));
        assert_eq!(
            serde_json::to_value(SecretInput::Set("x".into())).unwrap(),
            json!({"kind": "set", "value": "x"})
        );
        let q: QrImageSource = serde_json::from_value(json!({"kind": "path", "path": "/a.png"})).unwrap();
        assert!(matches!(q, QrImageSource::Path { .. }));
        assert_eq!(serde_json::to_value(CaptureMode::SystemProxy).unwrap(), "system_proxy");
    }

    /// REQ-4: a partial failure surfaces as RESTORE_INCOMPLETE with the full
    /// per-item report attached.
    #[test]
    fn restore_incomplete_error_carries_per_item_report() {
        use edge_core::restore::{run_restore, Restorer, StepOutcome};
        struct DnsFails;
        impl Restorer for DnsFails {
            fn restore(&mut self, kind: RestoreItemKind) -> StepOutcome {
                match kind {
                    RestoreItemKind::Dns => StepOutcome::Failed {
                        action: "Restore DNS servers on Wi-Fi".into(),
                        code: crate::api::error::ErrorCode::DnsRestoreFailed,
                        detail: None,
                    },
                    _ => StepOutcome::NotNeeded { action: "Nothing to restore".into() },
                }
            }
        }
        let report: RestoreReport =
            run_restore(&mut DnsFails, RestoreTrigger::Disconnect, "2026-10-11T00:00:00Z".into()).into();
        let err = crate::api::error::AppError::restore_incomplete(report);
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["code"], "RESTORE_INCOMPLETE");
        assert_eq!(v["recoverable"], false);
        let r = &v["restoreReport"];
        assert_eq!(r["fullyRestored"], false);
        assert_eq!(r["trigger"], "disconnect");
        assert_eq!(r["items"].as_array().unwrap().len(), 6);
        assert_eq!(r["items"][2]["kind"], "dns");
        assert_eq!(r["items"][2]["result"], "failed");
        assert_eq!(r["items"][2]["errorCode"], "DNS_RESTORE_FAILED");
        assert!(r.get("actions").is_none());
    }
}
