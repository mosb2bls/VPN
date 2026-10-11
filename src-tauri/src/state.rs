//! Application state held behind `api::commands::AppState`.

use std::path::Path;

use crate::api::error::AppError;
use crate::api::types::{ConnectionState, Status};
use crate::store::NodeStore;
use crate::store::prefs::PrefsStore;
use crate::store::secrets::SecretStore;

pub struct AppData {
    pub nodes: NodeStore,
    pub prefs: PrefsStore,
    /// Runtime connection status (not persisted; modes mirror `prefs`).
    pub status: Status,
}

impl AppData {
    /// Load persisted nodes and preferences from `dir`. Returns load warnings.
    pub fn load(dir: &Path, secrets: Box<dyn SecretStore>) -> (Self, Vec<AppError>) {
        let (nodes, mut warnings) = NodeStore::open(dir, secrets);
        let (prefs, w) = PrefsStore::open(dir);
        warnings.extend(w);
        let p = &prefs.prefs;
        let status = Status {
            state: ConnectionState::Disconnected,
            capture_mode: p.capture_mode,
            routing_mode: p.routing_mode,
            dns_profile: p.dns_profile,
            selected_node_id: nodes.selected_id(),
            active_node_id: None,
            connected_since: None,
            kill_switch_active: false,
            system_proxy_set: false,
            failover: None,
            last_error: None,
            last_recovery: None,
            tun_active: false,
            last_restore: None,
        };
        (Self { nodes, prefs, status }, warnings)
    }
}
