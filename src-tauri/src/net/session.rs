//! Connection orchestration (system proxy mode, macOS).
//!
//! Order on connect: journal (empty) → start core → journal(core pid) → wait
//! for Clash API → snapshot proxies → journal(previous values) → set system
//! proxy → read back → health check. Any failure after the journal exists rolls
//! back through the same restore path as disconnect.
//!
//! Long operations are serialised by [`OpLock`] (`BUSY` if taken) and never
//! hold the `AppState` mutex while waiting on the OS or the core.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use edge_core::link::NodeConfig;
use edge_core::redact::Redactor;
use edge_core::restore::{self, HeldState, RestoreItemKind, RestoreTrigger, Restorer, StepOutcome};
use edge_core::singbox::{self, Capture, GenInput, RuleSets};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::api::commands::AppState;
use crate::api::error::{ApiResult, AppError, ErrorCode};
use crate::api::events::{
    ActiveNodeChanged, FailoverEvent, FailoverKind, HealthCheck, NodeChangeCause, ProbeResult, RecoveryCompleted, RestoreReportChanged,
    StatusChanged, TrafficStatsUpdated,
};
use crate::api::types::*;
use crate::net::clash::{self, ClashApi};
use crate::net::coreproc::{self, CoreProcess};
use crate::net::journal::{self, CoreRecord, Journal, SystemProxyRecord};
use crate::net::logbuf;
use crate::net::osproxy::{self, Runner};
use crate::net::probe;
use edge_core::failover;
use edge_core::helper_proto::{HelperState, TunProfile, WireDns, WireNode, WireRestoreItem, WireRouting};

/// Serialises connect / disconnect / restore / mode switches.
pub struct OpLock(pub tokio::sync::Mutex<()>);

pub struct Session {
    /// Local core (system proxy mode); `None` when the helper runs the TUN core.
    pub core: Option<Arc<Mutex<CoreProcess>>>,
    pub tun: bool,
    pub api: ClashApi,
    pub mixed_port: u16,
    pub generation: u64,
    pub exe: PathBuf,
    /// Node ids present in the running config.
    pub node_ids: Vec<String>,
    pub latency_ms: Arc<Mutex<Option<u32>>>,
}

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_generation() -> u64 {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

fn runner() -> Box<dyn Runner> {
    #[cfg(target_os = "macos")]
    {
        Box::new(osproxy::Networksetup)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(Unsupported)
    }
}

#[cfg(not(target_os = "macos"))]
struct Unsupported;
#[cfg(not(target_os = "macos"))]
impl Runner for Unsupported {
    fn run(&self, _: &[String]) -> Result<String, String> {
        Err("system proxy on this OS is implemented in the Windows phase".into())
    }
}

// ---------------------------------------------------------------------------
// State helpers
// ---------------------------------------------------------------------------

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut crate::state::AppData) -> T) -> T {
    let s = app.state::<AppState>();
    let mut g = s.lock().unwrap_or_else(|p| p.into_inner());
    f(&mut g)
}

/// Mutate status, then emit `statusChanged` and refresh the tray.
fn update_status(app: &AppHandle, reason: Option<&str>, f: impl FnOnce(&mut Status)) -> Status {
    let status = with(app, |st| {
        f(&mut st.status);
        st.status.clone()
    });
    let _ = StatusChanged { status: status.clone(), reason: reason.map(Into::into) }.emit(app);
    crate::shell::refresh_tray(app);
    status
}

fn op_guard(app: &AppHandle) -> ApiResult<tokio::sync::MutexGuard<'_, ()>> {
    app.state::<OpLock>().inner().0.try_lock().map_err(|_| AppError::new(ErrorCode::Busy))
}

fn data_dir(app: &AppHandle) -> PathBuf {
    with(app, |st| st.data_dir.clone())
}

fn redactor_for(configs: &[(String, NodeConfig)]) -> Redactor {
    let mut r = Redactor::new();
    for (_, c) in configs {
        match c {
            NodeConfig::Hysteria2(h) => {
                r.add_secret(h.password.expose());
                if let Some(o) = &h.obfs_password {
                    r.add_secret(o.expose());
                }
            }
            NodeConfig::VlessReality(v) => {
                r.add_secret(v.uuid.expose());
                r.add_secret(v.public_key.expose());
                r.add_secret(v.short_id.expose());
            }
        }
    }
    r
}

// ---------------------------------------------------------------------------
// Restore (shared by disconnect, restore_networking, quit, recovery, CLI)
// ---------------------------------------------------------------------------

struct SysProxyRestorer<'a> {
    runner: &'a dyn Runner,
    journal: Option<&'a Journal>,
    /// Result of the helper's own restore (TUN sessions only).
    helper: Option<Result<Vec<WireRestoreItem>, AppError>>,
}

fn helper_outcome(helper: &Option<Result<Vec<WireRestoreItem>, AppError>>, kind: &str) -> Option<StepOutcome> {
    match helper.as_ref()? {
        Err(e) => Some(StepOutcome::Skipped {
            action: "The privileged helper could not be reached".into(),
            code: if e.code == ErrorCode::HelperNotInstalled { ErrorCode::HelperNotInstalled } else { ErrorCode::HelperUnreachable },
        }),
        Ok(items) => items.iter().find(|i| i.kind == kind).map(|i| match i.result.as_str() {
            "restored" => StepOutcome::Restored { action: i.action.clone() },
            "not_needed" => StepOutcome::NotNeeded { action: i.action.clone() },
            "skipped" => StepOutcome::Skipped { action: i.action.clone(), code: i.error_code.unwrap_or(ErrorCode::HelperUnreachable) },
            _ => StepOutcome::Failed { action: i.action.clone(), code: i.error_code.unwrap_or(ErrorCode::RestoreIncomplete), detail: None },
        }),
    }
}

impl Restorer for SysProxyRestorer<'_> {
    fn restore(&mut self, kind: RestoreItemKind) -> StepOutcome {
        match kind {
            RestoreItemKind::SystemProxy => match self.journal.and_then(|j| j.system_proxy.as_ref()) {
                None => StepOutcome::NotNeeded { action: "System proxy was not changed by this app".into() },
                Some(rec) => osproxy::restore(self.runner, &rec.services),
            },
            RestoreItemKind::TunSession => helper_outcome(&self.helper, "tun_session")
                .unwrap_or(StepOutcome::NotNeeded { action: "No TUN session".into() }),
            RestoreItemKind::Routes => helper_outcome(&self.helper, "routes")
                .unwrap_or(StepOutcome::NotNeeded { action: "Routes were not changed".into() }),
            RestoreItemKind::Dns => match &self.helper {
                Some(Ok(_)) => helper_outcome(&self.helper, "dns"),
                _ => None,
            }
            .unwrap_or(StepOutcome::NotNeeded { action: "DNS settings were not changed".into() }),
            RestoreItemKind::Ipv6 => match &self.helper {
                Some(Ok(_)) => helper_outcome(&self.helper, "ipv6"),
                _ => None,
            }
            .unwrap_or(StepOutcome::NotNeeded { action: "IPv6 settings were not changed".into() }),
            RestoreItemKind::Firewall => helper_outcome(&self.helper, "firewall")
                .unwrap_or(StepOutcome::NotNeeded { action: "No firewall rules were added".into() }),
        }
    }
}

const LAST_RESTORE: &str = "last_restore.json";

/// Restore everything the journal says we changed, stop the core, and
/// finalise journal / last-restore files. Pure filesystem + OS; no Tauri.
pub fn restore_all(
    dir: &Path,
    runner: &dyn Runner,
    trigger: RestoreTrigger,
    core: Option<&Arc<Mutex<CoreProcess>>>,
) -> RestoreReport {
    let journal = match journal::load(dir) {
        Ok(j) => j,
        Err(e) => {
            // Corrupt journal: nothing reliable to restore from. Report it.
            let report = restore::CoreRestoreReport {
                at: edge_core::clock::now_rfc3339(),
                trigger,
                fully_restored: false,
                items: RestoreItemKind::ORDER
                    .iter()
                    .map(|k| restore::RestoreItem {
                        kind: *k,
                        label: k.label().into(),
                        action: "Saved network state is unreadable".into(),
                        result: restore::RestoreResult::Skipped,
                        error_code: Some(e.code),
                        error_message: Some(e.message.clone()),
                    })
                    .collect(),
            };
            return report.into();
        }
    };
    // TUN: the helper ends its session, removes the kill switch and reports.
    let helper = journal
        .as_ref()
        .filter(|j| j.tun)
        .map(|_| crate::net::helper_client::restore().map(|(_, items)| items));
    let report: RestoreReport = restore::run_restore(
        &mut SysProxyRestorer { runner, journal: journal.as_ref(), helper },
        trigger,
        edge_core::clock::now_rfc3339(),
    )
    .into();
    // Stop the core after the proxy points away from it.
    match core {
        Some(c) => c.lock().unwrap_or_else(|p| p.into_inner()).stop(),
        None => {
            if let Some(CoreRecord { pid, exe }) = journal.as_ref().and_then(|j| j.core.clone()) {
                coreproc::kill_stale(pid, &exe);
            }
        }
    }
    let last = dir.join(LAST_RESTORE);
    if report.fully_restored {
        journal::delete(dir);
        let _ = std::fs::remove_file(&last);
    } else if let Ok(json) = serde_json::to_vec_pretty(&report) {
        let _ = crate::store::fsutil::write_atomic(&last, &json);
    }
    for item in &report.items {
        if !item.result.is_clean() {
            logbuf::app(LogLevel::Error, format!("restore {:?}: {}", item.kind, item.action));
        }
    }
    report
}

/// Persisted incomplete report from a previous run, if any.
pub fn load_last_restore(dir: &Path) -> Option<RestoreReport> {
    std::fs::read(dir.join(LAST_RESTORE)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

struct Plan {
    tun: bool,
    kill_switch: bool,
    direct_fallback: bool,
    configs: Vec<(String, NodeConfig)>,
    selected: String,
    routing: singbox::Routing,
    dns: singbox::DnsProfile,
    allow_lan: bool,
    rules: RuleSets,
    bin: PathBuf,
    dir: PathBuf,
    targets: Vec<String>,
    timeout_ms: u32,
}

fn to_core_dns(d: DnsProfile) -> singbox::DnsProfile {
    match d {
        DnsProfile::Mainland => singbox::DnsProfile::Mainland,
        DnsProfile::Overseas => singbox::DnsProfile::Overseas,
    }
}

/// Gather everything needed to start; errors here leave the state unchanged.
fn plan(app: &AppHandle) -> ApiResult<Plan> {
    with(app, |st| {
        let nodes = st.nodes.list();
        if nodes.is_empty() {
            return Err(AppError::new(ErrorCode::NoNodes));
        }
        let selected = st.nodes.selected_id().ok_or_else(|| AppError::new(ErrorCode::NoNodeSelected))?;
        let selected_cfg = st.nodes.config(&selected)?;
        let p = &st.prefs.prefs;
        let tun = p.capture_mode == CaptureMode::Tun;
        let routing = match p.routing_mode {
            RoutingMode::Rule => singbox::Routing::Rule,
            RoutingMode::Global => singbox::Routing::Global,
            RoutingMode::Direct => unreachable!("direct handled by caller"),
        };
        let rules = st.rules.clone()?;
        let mut configs: Vec<(String, NodeConfig)> =
            nodes.iter().filter(|n| n.id != selected).filter_map(|n| st.nodes.config(&n.id).ok().map(|c| (n.id.clone(), c))).collect();
        configs.insert(0, (selected.clone(), selected_cfg));
        Ok(Plan {
            tun,
            kill_switch: p.safety.kill_switch,
            direct_fallback: p.safety.direct_fallback,
            configs,
            selected,
            routing,
            dns: to_core_dns(p.dns_profile),
            allow_lan: p.safety.allow_lan,
            rules: RuleSets { geosite_cn: rules.geosite_cn, geoip_cn: rules.geoip_cn },
            // The helper runs its own installed core in TUN mode.
            bin: if tun { PathBuf::new() } else { crate::core_bin::locate_verified()? },
            dir: st.data_dir.clone(),
            targets: p.failover.probe_targets.clone(),
            timeout_ms: p.failover.probe_timeout_secs * 1000,
        })
    })
}

fn gen_config(p: &Plan, mixed: u16, ctl: u16, secret: &str) -> ApiResult<serde_json::Value> {
    singbox::generate(&GenInput {
        nodes: p.configs.iter().map(|(id, c)| (id.as_str(), c)).collect(),
        selected: &p.selected,
        routing: p.routing,
        dns: p.dns,
        capture: Capture::SystemProxy { port: mixed },
        allow_lan: p.allow_lan,
        controller_port: ctl,
        controller_secret: secret,
        rule_sets: p.rules.clone(),
        direct_fallback: p.direct_fallback,
    })
    .map_err(|e| AppError::new(ErrorCode::CoreConfigInvalid).with_detail(format!("{e:?}")))
}

fn port_err(e: std::io::Error) -> AppError {
    AppError::new(ErrorCode::PortInUse).with_detail(e.to_string())
}

/// Start the core for `p` (optionally on a fixed mixed port). Returns the
/// running process + API. Updates the journal's core record.
fn start_core(p: &Plan, mixed: Option<u16>) -> ApiResult<(CoreProcess, ClashApi, u16)> {
    let mixed = match mixed {
        Some(m) => m,
        None => clash::free_port().map_err(port_err)?,
    };
    let ctl = clash::free_port().map_err(port_err)?;
    let secret = clash::random_secret();
    logbuf::global().add_secret(&secret);
    let cfg = gen_config(p, mixed, ctl, &secret)?;
    let mut core = CoreProcess::spawn(&p.bin, &cfg, &p.dir.join("run"))?;
    let mut j = journal::load(&p.dir)?.unwrap_or_else(Journal::new);
    j.core = Some(CoreRecord { pid: core.pid, exe: p.bin.clone() });
    journal::save(&p.dir, &j)?;
    let api = ClashApi::new(ctl, &secret);
    if let Err(e) = api.wait_ready(Duration::from_secs(8)) {
        let tail = core.tail_text();
        core.stop();
        let code = if tail.to_ascii_lowercase().contains("address already in use") {
            ErrorCode::PortInUse
        } else {
            ErrorCode::CoreStartFailed
        };
        return Err(AppError::new(code).with_detail(format!("{e}\n{tail}")));
    }
    Ok((core, api, mixed))
}

fn health(api: &ClashApi, node_id: &str, targets: &[String], timeout_ms: u32) -> (bool, Vec<ProbeResult>) {
    let tag = singbox::node_tag(node_id);
    let probes: Vec<ProbeResult> = targets
        .iter()
        .map(|t| {
            let r = api.delay(&tag, t, timeout_ms);
            ProbeResult { target: probe::short_target(t), ok: r.is_ok(), latency_ms: r.ok() }
        })
        .collect();
    (probes.iter().any(|p| p.ok), probes)
}

fn median(probes: &[ProbeResult]) -> Option<u32> {
    let mut v: Vec<u32> = probes.iter().filter_map(|p| p.latency_ms).collect();
    v.sort_unstable();
    v.get(v.len().saturating_sub(1) / 2).copied()
}

pub async fn connect(app: &AppHandle) -> ApiResult<Status> {
    let _op = op_guard(app)?;
    let routing = with(app, |st| st.prefs.prefs.routing_mode);
    if with(app, |st| st.session.is_some()) {
        return Ok(with(app, |st| st.status.clone()));
    }
    if routing == RoutingMode::Direct {
        return Ok(update_status(app, Some("Direct mode: no proxy"), |s| {
            s.state = ConnectionState::Direct;
            s.last_error = None;
        }));
    }
    let p = plan(app)?;
    if p.tun {
        tauri::async_runtime::spawn_blocking(crate::net::helper_client::ensure_ready)
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal))??;
    }
    logbuf::global().set_redactor(redactor_for(&p.configs));
    update_status(app, Some(if p.tun { "Starting TUN (helper)" } else { "Starting proxy core" }), |s| {
        s.state = ConnectionState::Connecting;
        s.last_error = None;
    });
    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || connect_blocking(&app2, p))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal).with_detail("connect task failed"))?;
    match result {
        Ok(status) => Ok(status),
        Err(e) => {
            update_status(app, Some("Connection failed"), |s| {
                s.state = ConnectionState::Error;
                s.last_error = Some(e.clone());
                s.active_node_id = None;
                s.connected_since = None;
            });
            Err(e)
        }
    }
}

fn tun_profile(p: &Plan) -> TunProfile {
    TunProfile {
        nodes: p.configs.iter().map(|(id, c)| WireNode::from_config(id, c)).collect(),
        selected: p.selected.clone(),
        routing: match p.routing {
            singbox::Routing::Rule => WireRouting::Rule,
            singbox::Routing::Global => WireRouting::Global,
        },
        dns: match p.dns {
            singbox::DnsProfile::Mainland => WireDns::Mainland,
            singbox::DnsProfile::Overseas => WireDns::Overseas,
        },
        allow_lan: p.allow_lan,
        kill_switch: p.kill_switch,
        direct_fallback: p.direct_fallback,
    }
}

fn api_from(state: &HelperState) -> ApiResult<ClashApi> {
    match (state.controller_port, state.controller_secret.as_deref()) {
        (Some(port), Some(secret)) => {
            logbuf::global().add_secret(secret);
            Ok(ClashApi::new(port, secret))
        }
        _ => Err(AppError::new(ErrorCode::CoreApiUnreachable).with_detail("helper reported no controller")),
    }
}

fn connect_tun_blocking(app: &AppHandle, p: Plan) -> ApiResult<Status> {
    let dir = p.dir.clone();
    let mut j = Journal::new();
    j.tun = true;
    journal::save(&dir, &j)?;
    let started = crate::net::helper_client::start(tun_profile(&p)).and_then(|st| api_from(&st));
    let api = match started {
        Ok(api) => api,
        Err(e) => {
            let report = restore_all(&dir, runner().as_ref(), RestoreTrigger::Disconnect, None);
            with(app, |st| st.status.last_restore = Some(report.clone()));
            let _ = RestoreReportChanged { report: report.clone() }.emit(app);
            return Err(if report.fully_restored { e } else { AppError::restore_incomplete(report) });
        }
    };
    finish_connect(app, p, None, api, 0)
}

fn connect_blocking(app: &AppHandle, p: Plan) -> ApiResult<Status> {
    if p.tun {
        return connect_tun_blocking(app, p);
    }
    let dir = p.dir.clone();
    journal::save(&dir, &Journal::new())?;
    let rollback = |app: &AppHandle, core: Option<CoreProcess>, e: AppError| -> AppError {
        let core = core.map(|c| Arc::new(Mutex::new(c)));
        let report = restore_all(&dir, runner().as_ref(), RestoreTrigger::Disconnect, core.as_ref());
        let held = HeldState { system_proxy_set: true, ..Default::default() }.after(&to_core(&report));
        with(app, |st| {
            st.status.system_proxy_set = held.system_proxy_set;
            st.status.last_restore = Some(report.clone());
        });
        let _ = RestoreReportChanged { report: report.clone() }.emit(app);
        if report.fully_restored { e } else { AppError::restore_incomplete(report) }
    };
    let (core, api, mixed) = match start_core(&p, None) {
        Ok(x) => x,
        Err(e) => return Err(rollback(app, None, e)),
    };
    update_status(app, Some("Setting system proxy"), |_| {});
    let r = runner();
    let records = match osproxy::plan(r.as_ref(), mixed, p.allow_lan) {
        Ok(x) => x,
        Err(e) => return Err(rollback(app, Some(core), e)),
    };
    let mut j = journal::load(&dir)?.unwrap_or_else(Journal::new);
    j.system_proxy = Some(SystemProxyRecord { port: mixed, services: records.clone() });
    if let Err(e) = journal::save(&dir, &j) {
        return Err(rollback(app, Some(core), e));
    }
    if let Err(e) = osproxy::apply(r.as_ref(), &records, mixed, p.allow_lan) {
        return Err(rollback(app, Some(core), e));
    }
    update_status(app, Some("Running health check"), |s| s.system_proxy_set = true);
    finish_connect(app, p, Some(core), api, mixed)
}

/// Health check, store the session, report connected / degraded, start the monitor.
fn finish_connect(app: &AppHandle, p: Plan, core: Option<CoreProcess>, api: ClashApi, mixed: u16) -> ApiResult<Status> {
    if p.tun {
        update_status(app, Some("Running health check"), |s| s.tun_active = true);
    }
    let (ok, probes) = health(&api, &p.selected, &p.targets, p.timeout_ms);
    let latency = Arc::new(Mutex::new(median(&probes)));
    let generation = next_generation();
    let session = Session {
        core: core.map(|c| Arc::new(Mutex::new(c))),
        tun: p.tun,
        api,
        mixed_port: mixed,
        generation,
        exe: p.bin.clone(),
        node_ids: p.configs.iter().map(|(id, _)| id.clone()).collect(),
        latency_ms: latency,
    };
    with(app, |st| st.session = Some(session));
    let _ = HealthCheck {
        node_id: p.selected.clone(),
        round_ok: ok,
        probes,
        consecutive_failed_rounds: u32::from(!ok),
        at: edge_core::clock::now_rfc3339(),
    }
    .emit(app);
    let selected = p.selected.clone();
    let status = update_status(app, Some(if ok { "Connected" } else { "Connected, but health checks are failing" }), |s| {
        s.state = if ok { ConnectionState::Connected } else { ConnectionState::Degraded };
        s.active_node_id = Some(selected.clone());
        s.connected_since = Some(edge_core::clock::now_rfc3339());
        s.system_proxy_set = !p.tun;
        s.tun_active = p.tun;
        s.last_error = (!ok).then(|| AppError::new(ErrorCode::ProbeFailed));
    });
    let _ = ActiveNodeChanged { previous_node_id: None, node_id: Some(p.selected.clone()), cause: NodeChangeCause::Manual }.emit(app);
    spawn_monitor(app.clone(), generation, p);
    Ok(status)
}

fn to_core(r: &RestoreReport) -> restore::CoreRestoreReport {
    restore::CoreRestoreReport { at: r.at.clone(), trigger: r.trigger, fully_restored: r.fully_restored, items: r.items.clone() }
}

// ---------------------------------------------------------------------------
// Disconnect / restore
// ---------------------------------------------------------------------------

/// Restore networking and stop the core. `end_state` is `disconnected` or
/// `direct`. Returns the report; `Err(RESTORE_INCOMPLETE)` if partial.
pub async fn teardown(app: &AppHandle, trigger: RestoreTrigger, end_state: ConnectionState) -> ApiResult<RestoreReport> {
    let _op = op_guard(app)?;
    teardown_locked(app, trigger, end_state).await
}

async fn teardown_locked(app: &AppHandle, trigger: RestoreTrigger, end_state: ConnectionState) -> ApiResult<RestoreReport> {
    let session = with(app, |st| st.session.take());
    let dir = data_dir(app);
    update_status(app, Some("Restoring network settings"), |s| s.state = ConnectionState::Disconnecting);
    let held_before = with(app, |st| HeldState {
        kill_switch_active: st.status.kill_switch_active,
        system_proxy_set: st.status.system_proxy_set || journal::load(&st.data_dir).ok().flatten().is_some_and(|j| j.system_proxy.is_some()),
        tun_active: st.status.tun_active,
    });
    let core = session.as_ref().and_then(|s| s.core.clone());
    let report = tauri::async_runtime::spawn_blocking(move || restore_all(&dir, runner().as_ref(), trigger, core.as_ref()))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal).with_detail("restore task failed"))?;
    let held = held_before.after(&to_core(&report));
    let full = report.fully_restored;
    update_status(app, Some(if full { "Disconnected" } else { "Some network settings could not be restored" }), |s| {
        s.state = if full { end_state } else { ConnectionState::Error };
        s.active_node_id = None;
        s.connected_since = None;
        s.failover = None;
        s.system_proxy_set = held.system_proxy_set;
        s.kill_switch_active = held.kill_switch_active;
        s.tun_active = held.tun_active;
        s.last_restore = Some(report.clone());
        s.last_error = (!full).then(|| AppError::restore_incomplete(report.clone()));
    });
    let _ = RestoreReportChanged { report: report.clone() }.emit(app);
    if full { Ok(report) } else { Err(AppError::restore_incomplete(report)) }
}

pub async fn disconnect(app: &AppHandle) -> ApiResult<Status> {
    let nothing = with(app, |st| {
        st.session.is_none()
            && journal::load(&st.data_dir).ok().flatten().is_none()
            && matches!(st.status.state, ConnectionState::Disconnected | ConnectionState::Direct)
    });
    if nothing {
        let end = with(app, |st| st.status.state);
        if end == ConnectionState::Direct {
            return Ok(update_status(app, None, |s| s.state = ConnectionState::Disconnected));
        }
        return Ok(with(app, |st| st.status.clone()));
    }
    teardown(app, RestoreTrigger::Disconnect, ConnectionState::Disconnected).await?;
    Ok(with(app, |st| st.status.clone()))
}

pub async fn restore_networking(app: &AppHandle) -> ApiResult<RestoreReport> {
    teardown(app, RestoreTrigger::RestoreNetworking, ConnectionState::Disconnected).await
}

/// Mode changes while connected.
pub async fn after_mode_change(app: &AppHandle, capture_changed: bool) -> ApiResult<()> {
    let (connected, routing, state) =
        with(app, |st| (st.session.is_some(), st.prefs.prefs.routing_mode, st.status.state));
    if routing == RoutingMode::Direct {
        if connected || journal::load(&data_dir(app)).ok().flatten().is_some() {
            teardown(app, RestoreTrigger::DirectMode, ConnectionState::Direct).await?;
        } else if state != ConnectionState::Disconnected {
            update_status(app, Some("Direct mode: no proxy"), |s| s.state = ConnectionState::Direct);
        }
        return Ok(());
    }
    if state == ConnectionState::Direct && !connected {
        connect(app).await?;
        return Ok(());
    }
    if !connected {
        return Ok(());
    }
    if capture_changed {
        teardown(app, RestoreTrigger::CaptureModeSwitch, ConnectionState::Disconnected).await?;
        connect(app).await?;
    } else {
        restart_core(app).await?;
    }
    Ok(())
}

/// Regenerate the config and restart the core on the same local port (the
/// system proxy keeps pointing at it).
pub async fn restart_core(app: &AppHandle) -> ApiResult<()> {
    let _op = op_guard(app)?;
    let p = plan(app)?;
    if with(app, |st| st.session.as_ref().is_some_and(|s| s.tun)) {
        return restart_tun(app, p).await;
    }
    let Some((Some(old_core), mixed, active)) =
        with(app, |st| st.session.as_ref().map(|s| (s.core.clone(), s.mixed_port, st.status.active_node_id.clone())))
    else {
        return Ok(());
    };
    update_status(app, Some("Applying new settings"), |s| s.state = ConnectionState::Reconnecting);
    logbuf::global().set_redactor(redactor_for(&p.configs));
    let app2 = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        old_core.lock().unwrap_or_else(|e| e.into_inner()).stop();
        start_core(&p, Some(mixed)).map(|r| (r, p))
    })
    .await
    .map_err(|_| AppError::new(ErrorCode::Internal))?;
    match res {
        Ok(((core, api, _), p)) => {
            if let Some(a) = &active
                && a != &p.selected
                && p.configs.iter().any(|(id, _)| id == a)
            {
                let _ = api.select(singbox::TAG_PROXY, &singbox::node_tag(a));
            }
            let generation = next_generation();
            with(&app2, |st| {
                if let Some(s) = st.session.as_mut() {
                    s.core = Some(Arc::new(Mutex::new(core)));
                    s.api = api;
                    s.generation = generation;
                    s.node_ids = p.configs.iter().map(|(id, _)| id.clone()).collect();
                }
            });
            update_status(app, Some("Connected"), |s| s.state = ConnectionState::Connected);
            spawn_monitor(app.clone(), generation, p);
            Ok(())
        }
        Err(e) => {
            // System proxy still points at the (dead) local port: traffic fails
            // closed until the user disconnects or the next restart succeeds.
            update_status(app, Some("Could not restart the proxy core"), |s| {
                s.state = ConnectionState::Error;
                s.last_error = Some(e.clone());
            });
            Err(e)
        }
    }
}

/// TUN: the helper replaces its session with a new profile.
async fn restart_tun(app: &AppHandle, mut p: Plan) -> ApiResult<()> {
    if let Some(active) = with(app, |st| st.status.active_node_id.clone())
        && p.configs.iter().any(|(id, _)| *id == active)
    {
        p.selected = active;
    }
    update_status(app, Some("Applying new settings"), |s| s.state = ConnectionState::Reconnecting);
    let profile = tun_profile(&p);
    let res = tauri::async_runtime::spawn_blocking(move || crate::net::helper_client::start(profile))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal))?
        .and_then(|st| api_from(&st));
    match res {
        Ok(api) => {
            let generation = next_generation();
            with(app, |st| {
                if let Some(s) = st.session.as_mut() {
                    s.api = api;
                    s.generation = generation;
                    s.node_ids = p.configs.iter().map(|(id, _)| id.clone()).collect();
                }
            });
            update_status(app, Some("Connected"), |s| s.state = ConnectionState::Connected);
            spawn_monitor(app.clone(), generation, p);
            Ok(())
        }
        Err(e) => {
            update_status(app, Some("Could not restart the TUN session"), |s| {
                s.state = ConnectionState::Error;
                s.last_error = Some(e.clone());
            });
            Err(e)
        }
    }
}

/// Live node switch via the selector. Restarts the core if the node was added
/// after connecting.
pub async fn select_live(app: &AppHandle, id: &str) -> ApiResult<()> {
    let Some((api, known, prev)) =
        with(app, |st| st.session.as_ref().map(|s| (s.api.clone(), s.node_ids.contains(&id.to_string()), st.status.active_node_id.clone())))
    else {
        return Ok(());
    };
    if !known {
        return restart_core(app).await;
    }
    let tag = singbox::node_tag(id);
    tauri::async_runtime::spawn_blocking(move || api.select(singbox::TAG_PROXY, &tag))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal))?
        .map_err(|e| AppError::new(ErrorCode::CoreApiUnreachable).with_detail(e))?;
    update_status(app, None, |s| s.active_node_id = Some(id.to_string()));
    let _ = ActiveNodeChanged { previous_node_id: prev, node_id: Some(id.to_string()), cause: NodeChangeCause::Manual }.emit(app);
    Ok(())
}

pub fn is_active_node(app: &AppHandle, id: &str) -> bool {
    with(app, |st| st.session.is_some() && st.status.active_node_id.as_deref() == Some(id))
}

// ---------------------------------------------------------------------------
// Supervision: traffic stats, periodic health, crash restart
// ---------------------------------------------------------------------------

fn spawn_monitor(app: AppHandle, generation: u64, p: Plan) {
    std::thread::spawn(move || {
        let mut last: Option<(Instant, f64, f64)> = None;
        let mut base: Option<(f64, f64)> = None;
        let mut next_health = Instant::now() + Duration::from_secs(10);
        let started = Instant::now();
        let mut engine: Option<failover::Engine> = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Some((core, api, latency, active)) = with(&app, |st| {
                st.session
                    .as_ref()
                    .filter(|s| s.generation == generation)
                    .map(|s| (s.core.clone(), s.api.clone(), s.latency_ms.clone(), st.status.active_node_id.clone()))
            }) else {
                return; // disconnected or restarted
            };
            let api = match core {
                Some(core) => {
                    if core.lock().unwrap_or_else(|e| e.into_inner()).try_wait().is_some() {
                        logbuf::app(LogLevel::Error, "sing-box exited unexpectedly");
                        crash_restart(&app, generation, &p);
                        return;
                    }
                    api
                }
                // TUN: the helper supervises the core (kill switch, restart);
                // mirror its state and follow controller changes.
                None => match crate::net::helper_client::status() {
                    Ok(hs) => {
                        let state = with(&app, |st| st.status.state);
                        if hs.restarting || (!hs.running && hs.kill_switch_active) {
                            if state != ConnectionState::Reconnecting && state != ConnectionState::Error {
                                let msg = if hs.kill_switch_active {
                                    "Proxy core stopped — kill switch is blocking traffic while it restarts"
                                } else {
                                    "Proxy core stopped — restarting"
                                };
                                update_status(&app, Some(msg), |s| {
                                    s.state = if hs.restarting { ConnectionState::Reconnecting } else { ConnectionState::Error };
                                    s.kill_switch_active = hs.kill_switch_active;
                                    s.last_error = Some(AppError::new(ErrorCode::CoreCrashed));
                                });
                            }
                            continue;
                        }
                        if hs.running {
                            if state == ConnectionState::Reconnecting {
                                update_status(&app, Some("Proxy core restarted"), |s| {
                                    s.state = ConnectionState::Connected;
                                    s.kill_switch_active = false;
                                    s.last_error = None;
                                });
                            }
                            match api_from(&hs) {
                                Ok(fresh) => {
                                    with(&app, |st| {
                                        if let Some(s) = st.session.as_mut() {
                                            s.api = fresh.clone();
                                        }
                                    });
                                    fresh
                                }
                                Err(_) => api,
                            }
                        } else {
                            api
                        }
                    }
                    Err(e) => {
                        if with(&app, |st| st.status.state) != ConnectionState::Error {
                            update_status(&app, Some("The privileged helper is not responding"), |s| {
                                s.state = ConnectionState::Error;
                                s.last_error = Some(e.clone());
                            });
                        }
                        continue;
                    }
                },
            };
            if let Ok((up, down, conns)) = api.totals() {
                let now = Instant::now();
                let (bu, bd) = *base.get_or_insert((up, down));
                let (ru, rd) = match last {
                    Some((t, lu, ld)) => {
                        let dt = now.duration_since(t).as_secs_f64().max(0.001);
                        (((up - lu) / dt).max(0.0), ((down - ld) / dt).max(0.0))
                    }
                    None => (0.0, 0.0),
                };
                last = Some((now, up, down));
                let _ = TrafficStatsUpdated {
                    stats: TrafficStats {
                        up_bytes_per_sec: ru,
                        down_bytes_per_sec: rd,
                        up_bytes_total: (up - bu).max(0.0),
                        down_bytes_total: (down - bd).max(0.0),
                        active_connections: conns,
                        latency_ms: *latency.lock().unwrap_or_else(|e| e.into_inner()),
                    },
                }
                .emit(&app);
            }
            if Instant::now() >= next_health {
                let interval = with(&app, |st| st.prefs.prefs.failover.probe_interval_secs.max(5));
                next_health = Instant::now() + Duration::from_secs(interval as u64);
                if active.is_some() {
                    health_round(&app, &api, &p, &latency, &mut engine, started.elapsed().as_secs());
                }
            }
        }
    });
}

/// Fallback for `primary`: the configured node, else the first node of the
/// other protocol in list order. Must be part of the running config.
fn pick_fallback(st: &crate::state::AppData, primary: &str, in_config: &[String]) -> Option<failover::Node> {
    let nodes = st.nodes.list();
    let kind = |p: Protocol| match p {
        Protocol::Hysteria2 => failover::NodeKind::Hysteria2,
        Protocol::VlessReality => failover::NodeKind::VlessReality,
    };
    let primary_proto = nodes.iter().find(|n| n.id == primary)?.protocol;
    let usable = |n: &&Node| n.id != primary && !n.secret_missing && in_config.contains(&n.id);
    let chosen = match &st.prefs.prefs.failover.fallback_node_id {
        Some(id) => nodes.iter().filter(usable).find(|n| &n.id == id),
        None => nodes.iter().filter(usable).find(|n| n.protocol != primary_proto),
    }?;
    Some(failover::Node { id: chosen.id.clone(), kind: kind(chosen.protocol) })
}

/// One probe round: node in use (+ the other node when it matters), failover
/// decision, events, status.
fn health_round(
    app: &AppHandle,
    api: &ClashApi,
    p: &Plan,
    latency: &Arc<Mutex<Option<u32>>>,
    engine: &mut Option<failover::Engine>,
    now: u64,
) {
    let (selected, fallback, settings, targets, timeout_ms) = with(app, |st| {
        let f = &st.prefs.prefs.failover;
        let selected = st.status.selected_node_id.clone().unwrap_or_default();
        let in_config = st.session.as_ref().map(|s| s.node_ids.clone()).unwrap_or_default();
        (
            selected.clone(),
            pick_fallback(st, &selected, &in_config),
            failover::Settings {
                enabled: f.enabled,
                failure_rounds: f.failure_rounds.max(1),
                min_dwell_secs: f.min_dwell_secs as u64,
                return_stable_secs: f.return_stable_secs as u64,
                max_switches_per_hour: f.max_switches_per_hour.max(1),
                // Only effective if the running config has `direct` in the selector.
                direct_fallback: st.prefs.prefs.safety.direct_fallback && p.direct_fallback,
            },
            if f.probe_targets.len() >= 2 { f.probe_targets.clone() } else { p.targets.clone() },
            f.probe_timeout_secs.max(1) * 1000,
        )
    });
    // New selection (manual) → fresh engine with the selected node as primary.
    if engine.as_ref().is_none_or(|e| e.primary_id() != selected) {
        let kind = p
            .configs
            .iter()
            .find(|(id, _)| *id == selected)
            .map(|(_, c)| match c {
                NodeConfig::Hysteria2(_) => failover::NodeKind::Hysteria2,
                NodeConfig::VlessReality(_) => failover::NodeKind::VlessReality,
            })
            .unwrap_or(failover::NodeKind::Hysteria2);
        *engine = Some(failover::Engine::new(failover::Node { id: selected.clone(), kind }, fallback));
    }
    let e = engine.as_mut().expect("set above");
    if e.on_direct() {
        // Traffic goes direct: probe the nodes, leave direct as soon as one passes.
        let primary = e.primary_id().to_string();
        let (p_ok, _) = health(api, &primary, &targets, timeout_ms);
        let f_ok = if p_ok { None } else { e.other_id().map(|f| health(api, f, &targets, timeout_ms).0) };
        if let failover::Decision::LeaveDirect { to, reason } = e.on_direct_round(p_ok, f_ok)
            && api.select(singbox::TAG_PROXY, &singbox::node_tag(&to)).is_ok()
        {
            {
                let back_to_primary = to == primary;
                let _ = FailoverEvent {
                    kind: if back_to_primary { FailoverKind::ReturnedToPrimary } else { FailoverKind::SwitchedToFallback },
                    from_node_id: singbox::TAG_DIRECT.into(),
                    to_node_id: Some(to.clone()),
                    reason: reason.clone(),
                    at: edge_core::clock::now_rfc3339(),
                }
                .emit(app);
                let _ = ActiveNodeChanged {
                    previous_node_id: None,
                    node_id: Some(to.clone()),
                    cause: if back_to_primary { NodeChangeCause::Failback } else { NodeChangeCause::Failover },
                }
                .emit(app);
                logbuf::app(LogLevel::Warn, format!("failover: {reason}"));
                let now_s = edge_core::clock::now_rfc3339();
                update_status(app, Some(&reason), |s| {
                    s.state = ConnectionState::Connected;
                    s.active_node_id = Some(to.clone());
                    s.last_error = None;
                    s.failover = (!back_to_primary).then(|| FailoverStatus {
                        primary_node_id: primary.clone(),
                        fallback_node_id: to.clone(),
                        since: now_s.clone(),
                        reason: reason.clone(),
                        earliest_return_at: None,
                        direct: false,
                    });
                });
            }
        }
        return;
    }
    let active = e.active_id().to_string();
    let (ok, probes) = health(api, &active, &targets, timeout_ms);
    *latency.lock().unwrap_or_else(|x| x.into_inner()) = if ok { median(&probes) } else { None };
    let mut decision = e.on_active_round(now, ok, &settings);
    // Probe the other node when we are on the fallback (return) or the active
    // node is failing (is there somewhere to go?).
    if decision == failover::Decision::None
        && let Some(other) = e.other_id().map(str::to_string)
        && (e.on_fallback() || !ok)
    {
        let (other_ok, _) = health(api, &other, &targets, timeout_ms);
        decision = e.on_other_round(now, other_ok, &settings);
    }
    let _ = HealthCheck {
        node_id: active.clone(),
        round_ok: ok,
        probes,
        consecutive_failed_rounds: e.consecutive_failed(),
        at: edge_core::clock::now_rfc3339(),
    }
    .emit(app);
    let earliest = e.earliest_return(&settings).map(|t| {
        let secs_from_now = t.saturating_sub(now);
        edge_core::clock::format_unix(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) + secs_from_now,
        )
    });
    let primary = e.primary_id().to_string();
    match decision {
        failover::Decision::SwitchToFallback { to, reason } | failover::Decision::ReturnToPrimary { to, reason } => {
            let back = to == primary;
            update_status(app, Some(&reason), |s| s.state = ConnectionState::FailingOver);
            let switched = api.select(singbox::TAG_PROXY, &singbox::node_tag(&to));
            if let Err(err) = switched {
                logbuf::app(LogLevel::Error, format!("failover switch failed: {err}"));
                update_status(app, Some("Automatic switch failed"), |s| {
                    s.state = ConnectionState::Degraded;
                    s.last_error = Some(AppError::new(ErrorCode::CoreApiUnreachable));
                });
                return;
            }
            let now_s = edge_core::clock::now_rfc3339();
            let _ = FailoverEvent {
                kind: if back { FailoverKind::ReturnedToPrimary } else { FailoverKind::SwitchedToFallback },
                from_node_id: active.clone(),
                to_node_id: Some(to.clone()),
                reason: reason.clone(),
                at: now_s.clone(),
            }
            .emit(app);
            let _ = ActiveNodeChanged {
                previous_node_id: Some(active.clone()),
                node_id: Some(to.clone()),
                cause: if back { NodeChangeCause::Failback } else { NodeChangeCause::Failover },
            }
            .emit(app);
            logbuf::app(LogLevel::Warn, format!("failover: {reason}"));
            update_status(app, Some(&reason), |s| {
                s.state = ConnectionState::Connected;
                s.active_node_id = Some(to.clone());
                s.last_error = None;
                s.failover = (!back).then(|| FailoverStatus {
                    primary_node_id: primary.clone(),
                    fallback_node_id: to.clone(),
                    since: now_s.clone(),
                    reason: reason.clone(),
                    earliest_return_at: earliest.clone(),
                    direct: false,
                });
            });
        }
        failover::Decision::AllFailed { direct, reason } => {
            if direct && api.select(singbox::TAG_PROXY, singbox::TAG_DIRECT).is_ok() {
                let now_s = edge_core::clock::now_rfc3339();
                let _ = FailoverEvent {
                    kind: FailoverKind::SwitchedToDirect,
                    from_node_id: active.clone(),
                    to_node_id: None,
                    reason: reason.clone(),
                    at: now_s.clone(),
                }
                .emit(app);
                let _ = ActiveNodeChanged { previous_node_id: Some(active.clone()), node_id: None, cause: NodeChangeCause::Failover }.emit(app);
                logbuf::app(LogLevel::Warn, format!("failover: {reason}"));
                update_status(app, Some(&reason), |s| {
                    s.state = ConnectionState::Degraded;
                    s.active_node_id = None;
                    s.last_error = Some(AppError::new(ErrorCode::DirectFallbackActive));
                    s.failover = Some(FailoverStatus {
                        primary_node_id: primary.clone(),
                        fallback_node_id: String::new(),
                        since: now_s.clone(),
                        reason: reason.clone(),
                        earliest_return_at: None,
                        direct: true,
                    });
                });
            } else {
                let _ = FailoverEvent {
                    kind: FailoverKind::Suppressed,
                    from_node_id: active.clone(),
                    to_node_id: None,
                    reason: reason.clone(),
                    at: edge_core::clock::now_rfc3339(),
                }
                .emit(app);
                logbuf::app(LogLevel::Warn, format!("failover: {reason}"));
                update_status(app, Some(&reason), |s| {
                    s.state = ConnectionState::Degraded;
                    s.last_error = Some(AppError::new(ErrorCode::AllNodesFailed));
                });
            }
        }
        failover::Decision::LeaveDirect { .. } => {}
        failover::Decision::Suppressed { reason } => {
            let _ = FailoverEvent {
                kind: FailoverKind::Suppressed,
                from_node_id: active.clone(),
                to_node_id: None,
                reason: reason.clone(),
                at: edge_core::clock::now_rfc3339(),
            }
            .emit(app);
            logbuf::app(LogLevel::Warn, format!("failover suppressed: {reason}"));
            update_status(app, Some(&reason), |s| {
                s.state = ConnectionState::Degraded;
                s.last_error = Some(AppError::new(ErrorCode::ProbeFailed));
            });
        }
        failover::Decision::None => {
            let state = with(app, |st| st.status.state);
            if ok && state == ConnectionState::Degraded {
                update_status(app, Some("Health checks passing again"), |s| {
                    s.state = ConnectionState::Connected;
                    s.last_error = None;
                });
            } else if !ok && state == ConnectionState::Connected {
                update_status(app, Some("Health checks failing"), |s| {
                    s.state = ConnectionState::Degraded;
                    s.last_error = Some(AppError::new(ErrorCode::ProbeFailed));
                });
            }
            if let Some(f) = with(app, |st| st.status.failover.clone())
                && f.earliest_return_at != earliest
                && e.on_fallback()
            {
                with(app, |st| {
                    if let Some(x) = st.status.failover.as_mut() {
                        x.earliest_return_at = earliest.clone();
                    }
                });
            }
        }
    }
}

/// Core died while connected: restart with backoff (1,2,4,8,16,30… s) for up
/// to 5 minutes. The system proxy stays set meanwhile, so proxied traffic fails
/// closed. Gives up → `error` (or restores networking if direct fallback is on).
fn crash_restart(app: &AppHandle, generation: u64, p: &Plan) {
    update_status(app, Some("Proxy core stopped unexpectedly — restarting"), |s| {
        s.state = ConnectionState::Reconnecting;
        s.last_error = Some(AppError::new(ErrorCode::CoreCrashed));
    });
    let start = Instant::now();
    let mut delay = 1u64;
    loop {
        let Some(mixed) = with(app, |st| st.session.as_ref().filter(|s| s.generation == generation).map(|s| s.mixed_port)) else {
            return; // user disconnected meanwhile
        };
        let guard = app.state::<OpLock>().inner().0.try_lock();
        if let Ok(_g) = guard {
            match start_core(p, Some(mixed)) {
                Ok((core, api, _)) => {
                    let new_gen = next_generation();
                    with(app, |st| {
                        if let Some(s) = st.session.as_mut() {
                            s.core = Some(Arc::new(Mutex::new(core)));
                            s.api = api;
                            s.generation = new_gen;
                        }
                    });
                    update_status(app, Some("Proxy core restarted"), |s| {
                        s.state = ConnectionState::Connected;
                        s.last_error = None;
                    });
                    spawn_monitor(app.clone(), new_gen, clone_plan(p));
                    return;
                }
                Err(e) => logbuf::app(LogLevel::Error, format!("core restart failed: {:?} {}", e.code, e.detail.unwrap_or_default())),
            }
        }
        if start.elapsed() > Duration::from_secs(300) {
            let fallback = with(app, |st| st.prefs.prefs.safety.direct_fallback);
            if fallback {
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = teardown(&app2, RestoreTrigger::Disconnect, ConnectionState::Disconnected).await;
                });
            } else {
                update_status(app, Some("The proxy core could not be restarted"), |s| {
                    s.state = ConnectionState::Error;
                    s.last_error = Some(AppError::new(ErrorCode::CoreCrashed).fatal());
                });
            }
            return;
        }
        std::thread::sleep(Duration::from_secs(delay));
        delay = (delay * 2).min(30);
    }
}

fn clone_plan(p: &Plan) -> Plan {
    Plan {
        tun: p.tun,
        kill_switch: p.kill_switch,
        direct_fallback: p.direct_fallback,
        configs: p.configs.clone(),
        selected: p.selected.clone(),
        routing: p.routing,
        dns: p.dns,
        allow_lan: p.allow_lan,
        rules: p.rules.clone(),
        bin: p.bin.clone(),
        dir: p.dir.clone(),
        targets: p.targets.clone(),
        timeout_ms: p.timeout_ms,
    }
}

// ---------------------------------------------------------------------------
// Launch recovery and command line
// ---------------------------------------------------------------------------

/// At launch: if a journal exists, restore from it (system-proxy sessions are
/// not re-attached: the app owned the core). Reports via status + events.
pub fn recover_at_launch(app: &AppHandle) {
    let dir = data_dir(app);
    if !journal::path(&dir).exists() {
        if let Some(r) = load_last_restore(&dir) {
            with(app, |st| st.status.last_restore = Some(r));
        }
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _op = match op_guard(&app) {
            Ok(g) => g,
            Err(_) => return,
        };
        if reattach_tun(&app, &dir).await {
            return;
        }
        update_status(&app, Some("Restoring network settings from the previous session"), |s| {
            s.state = ConnectionState::Recovering;
            s.system_proxy_set = true;
        });
        let dir2 = dir.clone();
        let report = match tauri::async_runtime::spawn_blocking(move || {
            restore_all(&dir2, runner().as_ref(), RestoreTrigger::Recovery, None)
        })
        .await
        {
            Ok(r) => r,
            Err(_) => return,
        };
        let full = report.fully_restored;
        let recovery = RecoveryReport {
            outcome: if full { RecoveryOutcome::Restored } else { RecoveryOutcome::RestoreIncomplete },
            at: edge_core::clock::now_rfc3339(),
            restore: Some(report.clone()),
        };
        let held = HeldState { system_proxy_set: true, ..Default::default() }.after(&to_core(&report));
        update_status(&app, Some(if full { "Previous network settings restored" } else { "Some network settings could not be restored" }), |s| {
            s.state = if full { ConnectionState::Disconnected } else { ConnectionState::Error };
            s.system_proxy_set = held.system_proxy_set;
            s.last_recovery = Some(recovery.clone());
            s.last_restore = Some(report.clone());
            s.last_error = (!full).then(|| AppError::restore_incomplete(report.clone()));
        });
        let _ = RecoveryCompleted { report: recovery }.emit(&app);
        let _ = RestoreReportChanged { report }.emit(&app);
    });
}

/// TUN session still running in the helper after an app crash / restart:
/// re-attach instead of restoring. Returns true if re-attached.
async fn reattach_tun(app: &AppHandle, dir: &Path) -> bool {
    let tun = journal::load(dir).ok().flatten().is_some_and(|j| j.tun);
    if !tun {
        return false;
    }
    let hs = match tauri::async_runtime::spawn_blocking(crate::net::helper_client::status).await {
        Ok(Ok(hs)) if hs.running => hs,
        _ => return false,
    };
    let Ok(api) = api_from(&hs) else { return false };
    let Ok(p) = plan(app) else { return false };
    let selected = hs.selected.clone().unwrap_or_else(|| p.selected.clone());
    logbuf::global().set_redactor(redactor_for(&p.configs));
    let generation = next_generation();
    with(app, |st| {
        st.session = Some(Session {
            core: None,
            tun: true,
            api,
            mixed_port: 0,
            generation,
            exe: PathBuf::new(),
            node_ids: p.configs.iter().map(|(id, _)| id.clone()).collect(),
            latency_ms: Arc::new(Mutex::new(None)),
        })
    });
    let recovery = RecoveryReport { outcome: RecoveryOutcome::Reattached, at: edge_core::clock::now_rfc3339(), restore: None };
    update_status(app, Some("Re-attached to the running TUN session"), |s| {
        s.state = ConnectionState::Connected;
        s.tun_active = true;
        s.kill_switch_active = hs.kill_switch_active;
        s.active_node_id = Some(selected.clone());
        s.connected_since = Some(edge_core::clock::now_rfc3339());
        s.last_recovery = Some(recovery.clone());
    });
    let _ = RecoveryCompleted { report: recovery }.emit(app);
    spawn_monitor(app.clone(), generation, p);
    true
}

/// Auto-connect (setting, off by default): after launch recovery has
/// finished, connect with the persisted node, routing and capture mode — only
/// from a clean `disconnected` state (not after a re-attach or an incomplete
/// restore).
pub fn auto_connect_at_launch(app: &AppHandle) {
    if !with(app, |st| st.prefs.prefs.app.auto_connect) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let recovery (if any) take and release the operation lock first.
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(app.state::<OpLock>().inner().0.lock().await);
        if with(&app, |st| st.status.state) != ConnectionState::Disconnected {
            return;
        }
        logbuf::app(LogLevel::Info, "auto-connect at launch");
        if let Err(e) = connect(&app).await {
            logbuf::app(LogLevel::Warn, format!("auto-connect failed: {:?}", e.code));
            let _ = crate::api::events::ErrorRaised { error: e }.emit(&app);
        }
    });
}

/// Data directory without Tauri (for `--restore-networking`): same location
/// Tauri uses for `app_data_dir`.
pub fn cli_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support").join(edge_core::APP_IDENTIFIER))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(edge_core::APP_IDENTIFIER))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// `EdgeClient --restore-networking`: restore without the UI. Exit code 0 if
/// fully restored (or nothing to do), 2 if incomplete, 1 on usage errors.
pub fn restore_cli() -> i32 {
    let Some(dir) = cli_data_dir() else {
        eprintln!("restore-networking: unsupported platform");
        return 1;
    };
    logbuf::global().set_file_dir(&dir.join("logs"));
    if !journal::path(&dir).exists() {
        println!("Nothing to restore: no saved network state.");
        return 0;
    }
    let report = restore_all(&dir, runner().as_ref(), RestoreTrigger::CommandLine, None);
    for i in &report.items {
        println!("{:<24} {:<10} {}", i.label, format!("{:?}", i.result).to_lowercase(), i.action);
    }
    if report.fully_restored {
        println!("Networking restored.");
        0
    } else {
        println!("Some items could not be restored (see above).");
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::osproxy::fake::FakeMac;
    use edge_core::sysproxy::ServiceProxies;

    fn svc(name: &str) -> ServiceProxies {
        ServiceProxies { service: name.into(), ..Default::default() }
    }

    /// Full journal → OS change → restore cycle with a fake Mac, including a
    /// partial failure, persisted report and retry.
    #[test]
    fn journal_restore_cycle_with_partial_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mac = FakeMac::with(&[svc("Wi-Fi"), svc("Thunderbolt Bridge")]);
        let records = osproxy::plan(&mac, 17890, true).unwrap();
        let mut j = Journal::new();
        j.system_proxy = Some(SystemProxyRecord { port: 17890, services: records.clone() });
        journal::save(dir.path(), &j).unwrap();
        osproxy::apply(&mac, &records, 17890, true).unwrap();

        *mac.fail_on.lock().unwrap() = Some("-setproxybypassdomains".into());
        let r = restore_all(dir.path(), &mac, RestoreTrigger::Disconnect, None);
        assert!(!r.fully_restored);
        let sp = r.items.iter().find(|i| i.kind == RestoreItemKind::SystemProxy).unwrap();
        assert_eq!(sp.error_code, Some(ErrorCode::SystemProxyRestoreFailed));
        assert!(journal::path(dir.path()).exists(), "journal kept after partial failure");
        assert_eq!(load_last_restore(dir.path()).unwrap(), r, "incomplete report persisted");
        assert!(r.items.iter().filter(|i| i.kind != RestoreItemKind::SystemProxy).all(|i| i.result.is_clean()));

        *mac.fail_on.lock().unwrap() = None;
        let r2 = restore_all(dir.path(), &mac, RestoreTrigger::RestoreNetworking, None);
        assert!(r2.fully_restored);
        assert!(!journal::path(dir.path()).exists());
        assert!(load_last_restore(dir.path()).is_none());
        assert_eq!(mac.get("Wi-Fi"), svc("Wi-Fi"));
        assert_eq!(mac.get("Thunderbolt Bridge"), svc("Thunderbolt Bridge"));
    }

    #[test]
    fn nothing_to_restore_is_full_and_corrupt_journal_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mac = FakeMac::with(&[svc("Wi-Fi")]);
        let r = restore_all(dir.path(), &mac, RestoreTrigger::RestoreNetworking, None);
        assert!(r.fully_restored);
        assert_eq!(r.items.len(), 6);
        std::fs::write(journal::path(dir.path()), b"{bad").unwrap();
        let r = restore_all(dir.path(), &mac, RestoreTrigger::Recovery, None);
        assert!(!r.fully_restored);
        assert!(r.items.iter().all(|i| i.error_code == Some(ErrorCode::JournalCorrupt)));
    }
}
