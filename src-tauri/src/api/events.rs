//! Backend → UI events. Event names are the kebab-case struct names
//! (e.g. `status-changed`); the generated bindings expose them as
//! `events.statusChanged.listen(...)`.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use super::error::AppError;
use super::types::{HelperStatus, Node, RecoveryReport, RulesStatus, Status, TrafficStats};

/// Emitted on every connection state or mode change, with the full status.
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusChanged {
    pub status: Status,
    /// Human-readable reason for the transition, if any.
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeChangeCause {
    /// User selected a node (UI or tray).
    Manual,
    /// Automatic failover to the fallback node.
    Failover,
    /// Automatic return to the primary node.
    Failback,
    /// Restored from saved state at launch (auto-connect / re-attach).
    Restore,
}

/// The node carrying traffic changed.
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActiveNodeChanged {
    pub previous_node_id: Option<String>,
    pub node_id: Option<String>,
    pub cause: NodeChangeCause,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailoverKind {
    SwitchedToFallback,
    ReturnedToPrimary,
    /// A switch was warranted but suppressed (dwell time / rate limit / no fallback).
    Suppressed,
}

/// Automatic failover decision, always with a human-readable reason, e.g.
/// "HY2: 3 consecutive probe failures — UDP may be blocked on this network".
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FailoverEvent {
    pub kind: FailoverKind,
    pub from_node_id: String,
    pub to_node_id: Option<String>,
    pub reason: String,
    pub at: String,
}

/// Roughly once per second while connected.
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrafficStatsUpdated {
    pub stats: TrafficStats,
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// Short label such as `gstatic`, `cloudflare`. Never includes credentials.
    pub target: String,
    pub ok: bool,
    pub latency_ms: Option<u32>,
}

/// One health-probe round through the active node.
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub node_id: String,
    /// A round passes if at least one target passes.
    pub round_ok: bool,
    pub probes: Vec<ProbeResult>,
    pub consecutive_failed_rounds: u32,
    pub at: String,
}

/// Result of a rule-set update attempt (scheduled or manual).
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RulesUpdated {
    pub ok: bool,
    /// True if new rules were installed; false if already current or failed.
    pub changed: bool,
    pub status: RulesStatus,
}

#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatusChanged {
    pub status: HelperStatus,
}

/// Node list changed (import, edit, delete, reorder, tray selection).
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodesChanged {
    pub nodes: Vec<Node>,
    pub selected_node_id: Option<String>,
}

/// Launch-time / helper-start recovery finished.
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCompleted {
    pub report: RecoveryReport,
}

/// An error not tied to a command call (background failures).
/// `error.recoverable == false` means user action is needed.
#[derive(Serialize, Deserialize, Type, Event, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRaised {
    pub error: AppError,
}
