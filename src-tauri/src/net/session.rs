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
    ActiveNodeChanged, HealthCheck, NodeChangeCause, ProbeResult, RecoveryCompleted, RestoreReportChanged,
    StatusChanged, TrafficStatsUpdated,
};
use crate::api::types::*;
use crate::net::clash::{self, ClashApi};
use crate::net::coreproc::{self, CoreProcess};
use crate::net::journal::{self, CoreRecord, Journal, SystemProxyRecord};
use crate::net::logbuf;
use crate::net::osproxy::{self, Runner};
use crate::net::probe;

/// Serialises connect / disconnect / restore / mode switches.
pub struct OpLock(pub tokio::sync::Mutex<()>);

pub struct Session {
    pub core: Arc<Mutex<CoreProcess>>,
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
}

impl Restorer for SysProxyRestorer<'_> {
    fn restore(&mut self, kind: RestoreItemKind) -> StepOutcome {
        match kind {
            RestoreItemKind::SystemProxy => match self.journal.and_then(|j| j.system_proxy.as_ref()) {
                None => StepOutcome::NotNeeded { action: "System proxy was not changed by this app".into() },
                Some(rec) => osproxy::restore(self.runner, &rec.services),
            },
            RestoreItemKind::TunSession => StepOutcome::NotNeeded { action: "No TUN session (system proxy mode)".into() },
            RestoreItemKind::Routes => StepOutcome::NotNeeded { action: "Routes were not changed".into() },
            RestoreItemKind::Dns => StepOutcome::NotNeeded { action: "DNS settings were not changed".into() },
            RestoreItemKind::Ipv6 => StepOutcome::NotNeeded { action: "IPv6 settings were not changed".into() },
            RestoreItemKind::Firewall => StepOutcome::NotNeeded { action: "No firewall rules were added".into() },
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
    let report: RestoreReport = restore::run_restore(
        &mut SysProxyRestorer { runner, journal: journal.as_ref() },
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
        if p.capture_mode == CaptureMode::Tun {
            return Err(AppError::new(ErrorCode::HelperNotInstalled));
        }
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
            configs,
            selected,
            routing,
            dns: to_core_dns(p.dns_profile),
            allow_lan: p.safety.allow_lan,
            rules: RuleSets { geosite_cn: rules.geosite_cn, geoip_cn: rules.geoip_cn },
            bin: crate::core_bin::locate_verified()?,
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
    logbuf::global().set_redactor(redactor_for(&p.configs));
    update_status(app, Some("Starting proxy core"), |s| {
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

fn connect_blocking(app: &AppHandle, p: Plan) -> ApiResult<Status> {
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
    let (ok, probes) = health(&api, &p.selected, &p.targets, p.timeout_ms);
    let latency = Arc::new(Mutex::new(median(&probes)));
    let generation = next_generation();
    let session = Session {
        core: Arc::new(Mutex::new(core)),
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
        s.system_proxy_set = true;
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
    let core = session.as_ref().map(|s| s.core.clone());
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
    let Some((old_core, mixed, active)) =
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
                    s.core = Arc::new(Mutex::new(core));
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
        let mut failed_rounds = 0u32;
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
            if core.lock().unwrap_or_else(|e| e.into_inner()).try_wait().is_some() {
                logbuf::app(LogLevel::Error, "sing-box exited unexpectedly");
                crash_restart(&app, generation, &p);
                return;
            }
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
                if let Some(node) = active {
                    let (ok, probes) = health(&api, &node, &p.targets, p.timeout_ms);
                    failed_rounds = if ok { 0 } else { failed_rounds + 1 };
                    *latency.lock().unwrap_or_else(|e| e.into_inner()) = if ok { median(&probes) } else { None };
                    let _ = HealthCheck {
                        node_id: node,
                        round_ok: ok,
                        probes,
                        consecutive_failed_rounds: failed_rounds,
                        at: edge_core::clock::now_rfc3339(),
                    }
                    .emit(&app);
                    // Automatic failover between nodes arrives in B4; here only
                    // connected ↔ degraded.
                    let state = with(&app, |st| st.status.state);
                    if ok && state == ConnectionState::Degraded {
                        update_status(&app, Some("Health checks passing again"), |s| {
                            s.state = ConnectionState::Connected;
                            s.last_error = None;
                        });
                    } else if !ok && state == ConnectionState::Connected {
                        update_status(&app, Some("Health checks failing"), |s| {
                            s.state = ConnectionState::Degraded;
                            s.last_error = Some(AppError::new(ErrorCode::ProbeFailed));
                        });
                    }
                }
            }
        }
    });
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
                            s.core = Arc::new(Mutex::new(core));
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
