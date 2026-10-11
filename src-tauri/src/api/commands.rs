//! Tauri commands. Every command returns `ApiResult<T>`, which the generated
//! bindings expose as `{ status: "ok", data } | { status: "error", error }`.
//!
//! PHASE B1: commands that would touch the network, OS state, keychain or
//! disk return `NOT_IMPLEMENTED`. Getters return placeholder data from
//! `crate::stub`; simple setters update in-memory state only (not persisted,
//! not applied to the OS).

use std::sync::Mutex;

use tauri::{AppHandle, State};
use tauri_specta::Event;

use super::error::{ApiResult, AppError, ErrorCode, FieldError};
use super::events::{NodesChanged, StatusChanged};
use super::types::*;
use crate::stub::StubState;

pub type AppState = Mutex<StubState>;

fn lock<'a>(s: &'a State<'_, AppState>) -> ApiResult<std::sync::MutexGuard<'a, StubState>> {
    s.lock().map_err(|_| AppError::new(ErrorCode::Internal).with_detail("state lock poisoned"))
}

fn emit_nodes(app: &AppHandle, st: &StubState) {
    let _ = NodesChanged { nodes: st.nodes.clone(), selected_node_id: st.selected_node_id.clone() }
        .emit(app);
}

fn emit_status(app: &AppHandle, st: &StubState, reason: Option<&str>) {
    let _ = StatusChanged { status: st.status.clone(), reason: reason.map(Into::into) }.emit(app);
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn list_nodes(state: State<'_, AppState>) -> ApiResult<Vec<Node>> {
    Ok(lock(&state)?.nodes.clone())
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
pub async fn delete_node(app: AppHandle, state: State<'_, AppState>, id: String) -> ApiResult<()> {
    let mut st = lock(&state)?;
    let before = st.nodes.len();
    st.nodes.retain(|n| n.id != id);
    if st.nodes.len() == before {
        return Err(AppError::new(ErrorCode::NodeNotFound));
    }
    if st.selected_node_id.as_deref() == Some(&id) {
        st.selected_node_id = st.nodes.first().map(|n| n.id.clone());
        st.status.selected_node_id = st.selected_node_id.clone();
    }
    emit_nodes(&app, &st);
    Ok(())
}

/// `ids` must be a permutation of all current node IDs.
#[tauri::command]
#[specta::specta]
pub async fn reorder_nodes(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> ApiResult<Vec<Node>> {
    let mut st = lock(&state)?;
    let mut current: Vec<&str> = st.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut wanted: Vec<&str> = ids.iter().map(String::as_str).collect();
    current.sort_unstable();
    wanted.sort_unstable();
    if current != wanted {
        return Err(AppError::new(ErrorCode::ReorderMismatch));
    }
    let mut reordered = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(n) = st.nodes.iter().find(|n| &n.id == id) {
            reordered.push(n.clone());
        }
    }
    st.nodes = reordered;
    emit_nodes(&app, &st);
    Ok(st.nodes.clone())
}

/// Select a node. When connected this switches the core's selector without a
/// restart; when disconnected it only changes the selection.
#[tauri::command]
#[specta::specta]
pub async fn select_node(app: AppHandle, state: State<'_, AppState>, id: String) -> ApiResult<Status> {
    let mut st = lock(&state)?;
    if !st.nodes.iter().any(|n| n.id == id) {
        return Err(AppError::new(ErrorCode::NodeNotFound));
    }
    st.selected_node_id = Some(id.clone());
    st.status.selected_node_id = Some(id);
    emit_nodes(&app, &st);
    emit_status(&app, &st, None);
    Ok(st.status.clone())
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
pub async fn connect() -> ApiResult<Status> {
    Err(AppError::not_implemented("connect"))
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect(state: State<'_, AppState>) -> ApiResult<Status> {
    // Nothing can be connected in B1, so disconnect is a no-op that reports status.
    Ok(lock(&state)?.status.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn get_status(state: State<'_, AppState>) -> ApiResult<Status> {
    Ok(lock(&state)?.status.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn set_capture_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: CaptureMode,
) -> ApiResult<Status> {
    let mut st = lock(&state)?;
    st.status.capture_mode = mode;
    emit_status(&app, &st, None);
    Ok(st.status.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn set_routing_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: RoutingMode,
) -> ApiResult<Status> {
    let mut st = lock(&state)?;
    st.status.routing_mode = mode;
    emit_status(&app, &st, None);
    Ok(st.status.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn set_dns_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: DnsProfile,
) -> ApiResult<Status> {
    let mut st = lock(&state)?;
    st.status.dns_profile = profile;
    emit_status(&app, &st, None);
    Ok(st.status.clone())
}

// ---------------------------------------------------------------------------
// Failover / safety
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_failover_settings(state: State<'_, AppState>) -> ApiResult<FailoverSettings> {
    Ok(lock(&state)?.failover.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn set_failover_settings(
    state: State<'_, AppState>,
    settings: FailoverSettings,
) -> ApiResult<FailoverSettings> {
    let mut st = lock(&state)?;
    let node_ids: Vec<&str> = st.nodes.iter().map(|n| n.id.as_str()).collect();
    let errors = validate_failover(&settings, &node_ids);
    if !errors.is_empty() {
        return Err(AppError::validation(errors));
    }
    st.failover = settings;
    Ok(st.failover.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn get_safety_settings(state: State<'_, AppState>) -> ApiResult<SafetySettings> {
    Ok(lock(&state)?.safety.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn set_safety_settings(
    state: State<'_, AppState>,
    settings: SafetySettings,
) -> ApiResult<SafetySettings> {
    let mut st = lock(&state)?;
    st.safety = settings;
    Ok(st.safety.clone())
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
// App
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, AppState>) -> ApiResult<AppSettings> {
    Ok(lock(&state)?.settings.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn set_settings(state: State<'_, AppState>, settings: AppSettings) -> ApiResult<AppSettings> {
    if !(1..=168).contains(&settings.rules_update_interval_hours) {
        return Err(AppError::validation(vec![FieldError::new(
            "rulesUpdateIntervalHours",
            ErrorCode::FieldOutOfRange,
        )]));
    }
    let mut st = lock(&state)?;
    st.settings = settings;
    Ok(st.settings.clone())
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
pub async fn restore_networking() -> ApiResult<RestoreReport> {
    Err(AppError::not_implemented("restore_networking"))
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

/// Disconnect, restore networking, then exit. In B1 there is no OS state to
/// restore, so this exits immediately.
#[tauri::command]
#[specta::specta]
pub async fn quit_app(app: AppHandle) -> ApiResult<()> {
    app.exit(0);
    Ok(())
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
