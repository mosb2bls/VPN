//! Application state held behind `api::commands::AppState`.

use std::path::{Path, PathBuf};

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
    /// Verified active rule-sets, or why they are unavailable.
    pub rules: Result<crate::rules::ActiveRules, AppError>,
    /// App data dir (journal, logs, run dir, active rules).
    pub data_dir: PathBuf,
    /// Running connection, if any.
    pub session: Option<crate::net::session::Session>,
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
        let rules = Err(AppError::new(crate::api::error::ErrorCode::RulesMissing).with_detail("not loaded yet"));
        (Self { nodes, prefs, status, rules, data_dir: dir.to_path_buf(), session: None }, warnings)
    }
}
