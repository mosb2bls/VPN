//! Tauri commands. Every command returns `ApiResult<T>`, which the generated
//! bindings expose as `{ status: "ok", data } | { status: "error", error }`.
//!
//! Logic shared with the tray menu lives in [`ops`]; commands are thin wrappers.
//!
//! PHASE B1: commands that would touch the network, OS state, keychain or
//! disk return `NOT_IMPLEMENTED`. Getters return placeholder data from
//! `crate::stub`; simple setters update in-memory state only (not persisted,
//! not applied to the OS).

use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use super::error::{ApiResult, AppError, ErrorCode, FieldError};
use super::events::{NodesChanged, StatusChanged};
use super::types::*;
use crate::shell;
use crate::stub::StubState;

pub type AppState = Mutex<StubState>;

/// Run `f` with the state locked. The lock is released before returning, so
/// callers can emit events / refresh the tray afterwards without deadlocking.
fn with_state<T>(app: &AppHandle, f: impl FnOnce(&mut StubState) -> ApiResult<T>) -> ApiResult<T> {
    let state = app.state::<AppState>();
    let mut guard = state
        .lock()
        .map_err(|_| AppError::new(ErrorCode::Internal).with_detail("state lock poisoned"))?;
    f(&mut guard)
}

fn notify_nodes(app: &AppHandle) {
    if let Ok((nodes, selected_node_id)) =
        with_state(app, |st| Ok((st.nodes.clone(), st.selected_node_id.clone())))
    {
        let _ = NodesChanged { nodes, selected_node_id }.emit(app);
    }
    shell::refresh_tray(app);
}

fn notify_status(app: &AppHandle, reason: Option<&str>) {
    if let Ok(status) = with_state(app, |st| Ok(st.status.clone())) {
        let _ = StatusChanged { status, reason: reason.map(Into::into) }.emit(app);
    }
    shell::refresh_tray(app);
}

/// Operations shared by commands and the tray menu.
pub mod ops {
    use super::*;

    pub fn select_node(app: &AppHandle, id: String) -> ApiResult<Status> {
        let status = with_state(app, |st| {
            if !st.nodes.iter().any(|n| n.id == id) {
                return Err(AppError::new(ErrorCode::NodeNotFound));
            }
            st.selected_node_id = Some(id.clone());
            st.status.selected_node_id = Some(id);
            Ok(st.status.clone())
        })?;
        notify_nodes(app);
        notify_status(app, None);
        Ok(status)
    }

    pub fn set_capture_mode(app: &AppHandle, mode: CaptureMode) -> ApiResult<Status> {
        let status = with_state(app, |st| {
            st.status.capture_mode = mode;
            Ok(st.status.clone())
        })?;
        notify_status(app, None);
        Ok(status)
    }

    pub fn set_routing_mode(app: &AppHandle, mode: RoutingMode) -> ApiResult<Status> {
        let status = with_state(app, |st| {
            st.status.routing_mode = mode;
            Ok(st.status.clone())
        })?;
        notify_status(app, None);
        Ok(status)
    }

    pub fn set_dns_profile(app: &AppHandle, profile: DnsProfile) -> ApiResult<Status> {
        let status = with_state(app, |st| {
            st.status.dns_profile = profile;
            Ok(st.status.clone())
        })?;
        notify_status(app, None);
        Ok(status)
    }

    pub async fn connect(_app: &AppHandle) -> ApiResult<Status> {
        Err(AppError::not_implemented("connect"))
    }

    pub async fn disconnect(app: &AppHandle) -> ApiResult<Status> {
        // Nothing can be connected in B1, so disconnect is a no-op that reports status.
        with_state(app, |st| Ok(st.status.clone()))
    }

    pub async fn restore_networking(_app: &AppHandle) -> ApiResult<RestoreReport> {
        Err(AppError::not_implemented("restore_networking"))
    }

    /// Disconnect, restore networking, then exit. In B1 there is no OS state
    /// to restore, so this exits immediately.
    pub async fn quit(app: &AppHandle) -> ApiResult<()> {
        app.exit(0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn list_nodes(app: AppHandle) -> ApiResult<Vec<Node>> {
    with_state(&app, |st| Ok(st.nodes.clone()))
}

/// Parse one or more share links (one per line) and add the valid ones.
#[tauri::command]
#[specta::specta]
pub async fn import_links(text: String) -> ApiResult<ImportResult> {
    let _ = text;
    Err(AppError::not_implemented("import_links"))
}

/// Read text from the clipboard (only when the user explicitly asks) and import it.
#[tauri::command]
#[specta::specta]
pub async fn import_from_clipboard() -> ApiResult<ImportResult> {
    Err(AppError::not_implemented("import_from_clipboard"))
}

#[tauri::command]
#[specta::specta]
pub async fn import_qr_image(source: QrImageSource) -> ApiResult<ImportResult> {
    let _ = source;
    Err(AppError::not_implemented("import_qr_image"))
}

#[tauri::command]
#[specta::specta]
pub async fn add_node_manual(form: NodeForm) -> ApiResult<Node> {
    let _ = form;
    Err(AppError::not_implemented("add_node_manual"))
}

#[tauri::command]
#[specta::specta]
pub async fn update_node(id: String, form: NodeForm) -> ApiResult<Node> {
    let _ = (id, form);
    Err(AppError::not_implemented("update_node"))
}

#[tauri::command]
#[specta::specta]
pub async fn delete_node(app: AppHandle, id: String) -> ApiResult<()> {
    with_state(&app, |st| {
        let before = st.nodes.len();
        st.nodes.retain(|n| n.id != id);
        if st.nodes.len() == before {
            return Err(AppError::new(ErrorCode::NodeNotFound));
        }
        if st.selected_node_id.as_deref() == Some(&id) {
            st.selected_node_id = st.nodes.first().map(|n| n.id.clone());
            st.status.selected_node_id = st.selected_node_id.clone();
        }
        Ok(())
    })?;
    notify_nodes(&app);
    Ok(())
}

/// `ids` must be a permutation of all current node IDs.
#[tauri::command]
#[specta::specta]
pub async fn reorder_nodes(app: AppHandle, ids: Vec<String>) -> ApiResult<Vec<Node>> {
    let nodes = with_state(&app, |st| {
        let mut current: Vec<&str> = st.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut wanted: Vec<&str> = ids.iter().map(String::as_str).collect();
        current.sort_unstable();
        wanted.sort_unstable();
        if current != wanted {
            return Err(AppError::new(ErrorCode::ReorderMismatch));
        }
        let reordered = ids
            .iter()
            .filter_map(|id| st.nodes.iter().find(|n| &n.id == id).cloned())
            .collect();
        st.nodes = reordered;
        Ok(st.nodes.clone())
    })?;
    notify_nodes(&app);
    Ok(nodes)
}

/// Select a node. When connected this switches the core's selector without a
/// restart; when disconnected it only changes the selection.
#[tauri::command]
#[specta::specta]
pub async fn select_node(app: AppHandle, id: String) -> ApiResult<Status> {
    ops::select_node(&app, id)
}

/// Real connectivity test through the node (not a TCP ping).
#[tauri::command]
#[specta::specta]
pub async fn test_node(id: String) -> ApiResult<NodeTestResult> {
    let _ = id;
    Err(AppError::not_implemented("test_node"))
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn connect(app: AppHandle) -> ApiResult<Status> {
    ops::connect(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect(app: AppHandle) -> ApiResult<Status> {
    ops::disconnect(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_status(app: AppHandle) -> ApiResult<Status> {
    with_state(&app, |st| Ok(st.status.clone()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_capture_mode(app: AppHandle, mode: CaptureMode) -> ApiResult<Status> {
    ops::set_capture_mode(&app, mode)
}

#[tauri::command]
#[specta::specta]
pub async fn set_routing_mode(app: AppHandle, mode: RoutingMode) -> ApiResult<Status> {
    ops::set_routing_mode(&app, mode)
}

#[tauri::command]
#[specta::specta]
pub async fn set_dns_profile(app: AppHandle, profile: DnsProfile) -> ApiResult<Status> {
    ops::set_dns_profile(&app, profile)
}

// ---------------------------------------------------------------------------
// Failover / safety
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_failover_settings(app: AppHandle) -> ApiResult<FailoverSettings> {
    with_state(&app, |st| Ok(st.failover.clone()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_failover_settings(app: AppHandle, settings: FailoverSettings) -> ApiResult<FailoverSettings> {
    with_state(&app, |st| {
        let node_ids: Vec<&str> = st.nodes.iter().map(|n| n.id.as_str()).collect();
        let errors = validate_failover(&settings, &node_ids);
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        st.failover = settings;
        Ok(st.failover.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_safety_settings(app: AppHandle) -> ApiResult<SafetySettings> {
    with_state(&app, |st| Ok(st.safety.clone()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_safety_settings(app: AppHandle, settings: SafetySettings) -> ApiResult<SafetySettings> {
    with_state(&app, |st| {
        st.safety = settings;
        Ok(st.safety.clone())
    })
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_rules_status() -> ApiResult<RulesStatus> {
    Ok(RulesStatus {
        version: "placeholder".into(),
        source: RulesSource::Bundled,
        updated_at: None,
        last_check_at: None,
        next_check_at: None,
        updating: false,
        sets: vec![],
        last_error: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn update_rules_now() -> ApiResult<RulesStatus> {
    Err(AppError::not_implemented("update_rules_now"))
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_helper_status() -> ApiResult<HelperStatus> {
    Ok(HelperStatus {
        installed: false,
        running: false,
        version: None,
        expected_version: env!("CARGO_PKG_VERSION").into(),
        needs_upgrade: false,
    })
}

/// Shows the OS admin prompt (macOS) / UAC prompt (Windows).
#[tauri::command]
#[specta::specta]
pub async fn install_helper() -> ApiResult<HelperStatus> {
    Err(AppError::not_implemented("install_helper"))
}

/// Shows the OS admin prompt (macOS) / UAC prompt (Windows).
#[tauri::command]
#[specta::specta]
pub async fn uninstall_helper() -> ApiResult<HelperStatus> {
    Err(AppError::not_implemented("uninstall_helper"))
}

// ---------------------------------------------------------------------------
// Windows (REQ-1)
// ---------------------------------------------------------------------------

/// Hide the popover (e.g. on Esc). No-op if already hidden.
#[tauri::command]
#[specta::specta]
pub async fn hide_popover(app: AppHandle) -> ApiResult<()> {
    shell::hide_popover(&app)
}

/// Show and focus the settings window, hide the popover, and (if `section` is
/// set) send `settingsSectionRequested` to the settings window.
#[tauri::command]
#[specta::specta]
pub async fn open_settings(app: AppHandle, section: Option<SettingsSection>) -> ApiResult<()> {
    shell::open_settings(&app, section)
}

/// Reveal the most recent diagnostics export in Finder / Explorer. Only that
/// file can be revealed; there is no arbitrary-path reveal.
#[tauri::command]
#[specta::specta]
pub async fn reveal_diagnostics(app: AppHandle) -> ApiResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let path = app
        .state::<shell::ShellState>()
        .last_diagnostics
        .lock()
        .ok()
        .and_then(|p| p.clone())
        .filter(|p| p.exists())
        .ok_or_else(|| AppError::new(ErrorCode::DiagnosticsNotFound))?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| AppError::new(ErrorCode::DiagnosticsNotFound).with_detail(e.to_string()))
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_settings(app: AppHandle) -> ApiResult<AppSettings> {
    with_state(&app, |st| Ok(st.settings.clone()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_settings(app: AppHandle, settings: AppSettings) -> ApiResult<AppSettings> {
    if !(1..=168).contains(&settings.rules_update_interval_hours) {
        return Err(AppError::validation(vec![FieldError::new(
            "rulesUpdateIntervalHours",
            ErrorCode::FieldOutOfRange,
        )]));
    }
    with_state(&app, |st| {
        st.settings = settings;
        Ok(st.settings.clone())
    })
}

/// Most recent log entries (already redacted), newest last.
#[tauri::command]
#[specta::specta]
pub async fn get_logs(limit: u32, min_level: Option<LogLevel>) -> ApiResult<Vec<LogEntry>> {
    let _ = (limit, min_level);
    Ok(vec![])
}

#[tauri::command]
#[specta::specta]
pub async fn export_diagnostics() -> ApiResult<DiagnosticsExport> {
    Err(AppError::not_implemented("export_diagnostics"))
}

/// Restore system proxy, routes, DNS, IPv6 and firewall from the journal and
/// stop any core. Safe to call at any time.
#[tauri::command]
#[specta::specta]
pub async fn restore_networking(app: AppHandle) -> ApiResult<RestoreReport> {
    ops::restore_networking(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_app_info() -> ApiResult<AppInfo> {
    Ok(AppInfo {
        app_name: edge_core::APP_NAME.into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        spec_version: super::SPEC_VERSION.into(),
        core_version: None,
        platform: if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Other
        },
    })
}

/// Disconnect, restore networking, then exit.
#[tauri::command]
#[specta::specta]
pub async fn quit_app(app: AppHandle) -> ApiResult<()> {
    ops::quit(&app).await
}

// ---------------------------------------------------------------------------
// Validation (moves to crates/core with the real failover engine in B4)
// ---------------------------------------------------------------------------

fn url_host(u: &str) -> Option<&str> {
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://"))?;
    let host = rest.split(['/', ':', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

pub(crate) fn validate_failover(s: &FailoverSettings, node_ids: &[&str]) -> Vec<FieldError> {
    let mut e = Vec::new();
    let range = |e: &mut Vec<FieldError>, field: &str, v: u32, lo: u32, hi: u32| {
        if !(lo..=hi).contains(&v) {
            e.push(FieldError::new(field, ErrorCode::FieldOutOfRange));
        }
    };
    let hosts: Option<Vec<&str>> = s.probe_targets.iter().map(|t| url_host(t)).collect();
    match hosts {
        None => e.push(FieldError::new("probeTargets", ErrorCode::FieldInvalid)),
        Some(mut h) => {
            h.sort_unstable();
            h.dedup();
            if h.len() < 2 || s.probe_targets.len() > 5 {
                e.push(FieldError::new("probeTargets", ErrorCode::FieldInvalid));
            }
        }
    }
    range(&mut e, "probeIntervalSecs", s.probe_interval_secs, 5, 300);
    range(&mut e, "probeTimeoutSecs", s.probe_timeout_secs, 1, 30);
    if s.probe_timeout_secs >= s.probe_interval_secs {
        e.push(FieldError::new("probeTimeoutSecs", ErrorCode::FieldOutOfRange));
    }
    range(&mut e, "failureRounds", s.failure_rounds, 1, 20);
    range(&mut e, "minDwellSecs", s.min_dwell_secs, 0, 3600);
    range(&mut e, "returnStableSecs", s.return_stable_secs, 60, 86_400);
    range(&mut e, "maxSwitchesPerHour", s.max_switches_per_hour, 1, 60);
    if let Some(id) = &s.fallback_node_id
        && !node_ids.contains(&id.as_str())
    {
        e.push(FieldError::new("fallbackNodeId", ErrorCode::NodeNotFound));
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::{default_failover_settings, PLACEHOLDER_HY2_ID};

    #[test]
    fn default_failover_settings_are_valid() {
        assert!(validate_failover(&default_failover_settings(), &[]).is_empty());
    }

    #[test]
    fn failover_needs_two_distinct_probe_hosts() {
        let mut s = default_failover_settings();
        s.probe_targets = vec![
            "https://www.gstatic.com/generate_204".into(),
            "https://www.gstatic.com/other".into(),
        ];
        let e = validate_failover(&s, &[]);
        assert_eq!(e[0].field, "probeTargets");
    }

    #[test]
    fn failover_timeout_must_be_below_interval_and_fallback_must_exist() {
        let mut s = default_failover_settings();
        s.probe_timeout_secs = 10;
        s.fallback_node_id = Some("missing".into());
        let fields: Vec<_> = validate_failover(&s, &[PLACEHOLDER_HY2_ID]).into_iter().map(|f| f.field).collect();
        assert!(fields.contains(&"probeTimeoutSecs".to_string()));
        assert!(fields.contains(&"fallbackNodeId".to_string()));
    }
}
