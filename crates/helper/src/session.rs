//! The TUN session run by the helper (root on macOS, LocalSystem on Windows).
//! Shared by both OSes; per-OS pieces are in `crate::sys`.
//!
//! Start: validate the profile → verify core + rule hashes → resolve server
//! IPs (for the kill switch) → write journal → start sing-box (config on
//! stdin, generated here) → wait for its Clash API. A supervisor thread waits
//! on the child; if it exits unexpectedly the kill switch (pf anchor) is
//! engaged (pf / WFP) and the core is restarted with backoff for up to 5 minutes; the
//! kill switch is released once the core is back. Stop / Restore end the
//! session, release the kill switch and delete the journal.

use std::io::{BufRead, BufReader, Write};
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use edge_core::ErrorCode;
use edge_core::helper_proto::*;
use edge_core::link::NodeConfig;
use edge_core::pf::ServerRule;
use edge_core::singbox::{self, Capture, GenInput, RuleSets};
use serde::{Deserialize, Serialize};

use crate::{sys, util};

pub const EXPECTED_SINGBOX_SHA256: &str = env!("HELPER_SINGBOX_SHA256");
const MANIFEST: &str = include_str!("../../../resources/rules/manifest.json");

pub fn data_dir() -> PathBuf {
    sys::data_dir()
}

fn journal_path() -> PathBuf {
    data_dir().join("journal.json")
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    pub created_at: String,
    pub pid: Option<u32>,
    pub pf_token: Option<String>,
    pub kill_switch: bool,
}

fn save_journal(j: &Journal) {
    if let Ok(b) = serde_json::to_vec_pretty(j) {
        let _ = util::write_private(&journal_path(), &b);
    }
}

pub fn load_journal() -> Option<Journal> {
    std::fs::read(journal_path()).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub type HResult<T> = Result<T, (ErrorCode, Option<String>)>;

fn err<T>(code: ErrorCode, detail: impl Into<String>) -> HResult<T> {
    Err((code, Some(detail.into())))
}

struct Running {
    child: Child,
    controller_port: u16,
    controller_secret: String,
    selected: String,
}

#[derive(Default)]
struct Inner {
    running: Option<Running>,
    profile: Option<TunProfile>,
    /// Generation of the current session; supervisors of old sessions exit.
    generation: u64,
    restarting: bool,
    kill_switch_active: bool,
    pf_token: Option<String>,
    servers: Vec<ServerRule>,
    startup_recovery: Option<String>,
}

#[derive(Clone, Default)]
pub struct Session(Arc<Mutex<Inner>>);

impl Session {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn state(&self) -> HelperState {
        let g = self.lock();
        HelperState {
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL_VERSION,
            core_sha256: EXPECTED_SINGBOX_SHA256.into(),
            running: g.running.is_some(),
            restarting: g.restarting,
            kill_switch_active: g.kill_switch_active,
            controller_port: g.running.as_ref().map(|r| r.controller_port),
            controller_secret: g.running.as_ref().map(|r| r.controller_secret.clone()),
            selected: g.running.as_ref().map(|r| r.selected.clone()),
            startup_recovery: g.startup_recovery.clone(),
        }
    }

    pub fn start(&self, profile: TunProfile) -> HResult<HelperState> {
        let configs = validate(&profile)?;
        verify_core()?;
        let rules = verified_rules()?;
        if let (_, Err(e)) = self.stop_inner() {
            util::log(&format!("previous kill switch could not be removed: {e}"));
        }
        let servers = resolve_servers(&configs);
        let mut redactor = edge_core::redact::Redactor::new();
        for (_, c) in &configs {
            register_secrets(&mut redactor, c);
        }
        util::set_redactor(redactor);
        save_journal(&Journal { created_at: edge_core::clock::now_rfc3339(), kill_switch: profile.kill_switch, ..Default::default() });
        let running = spawn_core(&profile, &configs, &rules)?;
        let pid = running.child.id();
        save_journal(&Journal {
            created_at: edge_core::clock::now_rfc3339(),
            pid: Some(pid),
            pf_token: None,
            kill_switch: profile.kill_switch,
        });
        let generation = {
            let mut g = self.lock();
            g.generation += 1;
            g.running = Some(running);
            g.profile = Some(profile);
            g.servers = servers;
            g.restarting = false;
            g.generation
        };
        let me = self.clone();
        std::thread::spawn(move || me.supervise(generation));
        util::log(&format!("TUN session started (pid {pid})"));
        Ok(self.state())
    }

    /// Stop the core and release the kill switch. Returns (core was running,
    /// kill switch removed / not needed / error).
    fn stop_inner(&self) -> (bool, Result<bool, String>) {
        let (had_core, token, ks) = {
            let mut g = self.lock();
            g.generation += 1; // stop supervisors
            g.profile = None;
            g.restarting = false;
            let had = if let Some(mut r) = g.running.take() {
                let _ = r.child.kill();
                let _ = r.child.wait();
                true
            } else {
                false
            };
            (had, g.pf_token.take(), std::mem::take(&mut g.kill_switch_active))
        };
        let pf_result = if ks || token.is_some() {
            sys::ks_release(token.as_deref()).map(|_| true)
        } else {
            Ok(false)
        };
        if pf_result.is_err() {
            // Keep the token so a later Restore can retry.
            let mut g = self.lock();
            g.kill_switch_active = true;
        }
        (had_core, pf_result)
    }

    pub fn stop(&self) -> HResult<HelperState> {
        let (_, pf) = self.stop_inner();
        if let Err(e) = pf {
            return err(ErrorCode::KillSwitchRemoveFailed, e);
        }
        let _ = std::fs::remove_file(journal_path());
        util::log("TUN session stopped");
        Ok(self.state())
    }

    /// Stop + clean up leftovers (journal from a crashed helper, stale pf
    /// anchor, stale core). Per-item report.
    pub fn restore(&self) -> (HelperState, Vec<WireRestoreItem>) {
        let journal = load_journal();
        let (had_core, pf) = self.stop_inner();
        let mut stale = false;
        if let Some(pid) = journal.as_ref().and_then(|j| j.pid)
            && sys::kill_if_ours(pid)
        {
            stale = true;
        }
        // Stale anchor rules / token from a crashed helper.
        let mut pf_items = pf;
        let leftover_rules = sys::ks_leftover();
        let journal_token = journal.as_ref().and_then(|j| j.pf_token.clone());
        if pf_items.is_ok() && (leftover_rules || journal_token.is_some()) {
            pf_items = sys::ks_release(journal_token.as_deref()).map(|_| true);
        }
        let core_item = if had_core || stale {
            item("tun_session", "restored", "Stopped the TUN session; its routes were removed with the interface", None)
        } else {
            item("tun_session", "not_needed", "No TUN session was running", None)
        };
        let routes_item = if had_core || stale {
            item("routes", "restored", "TUN routes removed with the interface", None)
        } else {
            item("routes", "not_needed", "Routes were not changed", None)
        };
        let fw_item = match &pf_items {
            Ok(true) => item("firewall", "restored", "Removed the kill-switch firewall rules", None),
            Ok(false) => item("firewall", "not_needed", "No kill-switch rules were active", None),
            Err(e) => item("firewall", "failed", &format!("Could not remove the kill-switch rules: {e}"), Some(ErrorCode::KillSwitchRemoveFailed)),
        };
        if pf_items.is_ok() {
            let _ = std::fs::remove_file(journal_path());
        }
        let items = vec![
            core_item,
            routes_item,
            item("dns", "not_needed", "DNS settings were not changed (DNS is captured inside the TUN)", None),
            item("ipv6", "not_needed", "IPv6 settings were not changed (IPv6 is captured and rejected inside the TUN)", None),
            fw_item,
        ];
        (self.state(), items)
    }

    /// At helper start: a journal means the previous helper died mid-session.
    /// Its core died with it, so restore (no re-attach possible).
    pub fn recover_at_start(&self) {
        if load_journal().is_some() {
            let (_, items) = self.restore();
            let summary = items.iter().map(|i| format!("{}={}", i.kind, i.result)).collect::<Vec<_>>().join(", ");
            util::log(&format!("startup recovery: {summary}"));
            self.lock().startup_recovery = Some(format!("Previous TUN session cleaned up at helper start ({summary})"));
        }
    }

    fn supervise(&self, generation: u64) {
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let exited = {
                let mut g = self.lock();
                if g.generation != generation {
                    return;
                }
                match g.running.as_mut() {
                    Some(r) => r.child.try_wait().ok().flatten().is_some(),
                    None => return,
                }
            };
            if exited {
                util::log("sing-box exited unexpectedly");
                self.crash_restart(generation);
                return;
            }
        }
    }

    fn crash_restart(&self, generation: u64) {
        let (profile, servers) = {
            let mut g = self.lock();
            if g.generation != generation {
                return;
            }
            g.running = None;
            g.restarting = true;
            (g.profile.clone(), g.servers.clone())
        };
        let Some(profile) = profile else { return };
        if profile.kill_switch {
            match sys::ks_engage(&servers, profile.allow_lan) {
                Ok(token) => {
                    let mut g = self.lock();
                    g.kill_switch_active = true;
                    g.pf_token = Some(token.clone());
                    let mut j = load_journal().unwrap_or_default();
                    j.pf_token = Some(token);
                    save_journal(&j);
                    util::log("kill switch engaged");
                }
                Err(e) => util::log(&format!("kill switch FAILED to engage: {e}")),
            }
        }
        let start = Instant::now();
        let mut delay = 1u64;
        loop {
            if self.lock().generation != generation {
                return;
            }
            let attempt = validate(&profile).and_then(|c| {
                verify_core()?;
                let rules = verified_rules()?;
                spawn_core(&profile, &c, &rules)
            });
            match attempt {
                Ok(running) => {
                    let pid = running.child.id();
                    let token = {
                        let mut g = self.lock();
                        if g.generation != generation {
                            let mut r = running;
                            let _ = r.child.kill();
                            return;
                        }
                        g.running = Some(running);
                        g.restarting = false;
                        g.kill_switch_active = false;
                        g.pf_token.take()
                    };
                    if let Err(e) = sys::ks_release(token.as_deref()) {
                        util::log(&format!("kill switch release failed: {e}"));
                        self.lock().kill_switch_active = true;
                    }
                    save_journal(&Journal { created_at: edge_core::clock::now_rfc3339(), pid: Some(pid), pf_token: None, kill_switch: profile.kill_switch });
                    util::log(&format!("sing-box restarted (pid {pid}); kill switch released"));
                    let me = self.clone();
                    std::thread::spawn(move || me.supervise(generation));
                    return;
                }
                Err((code, d)) => util::log(&format!("restart failed: {code:?} {}", d.unwrap_or_default())),
            }
            if start.elapsed() > Duration::from_secs(300) {
                util::log("giving up restarting; kill switch stays until the user disconnects");
                self.lock().restarting = false;
                return;
            }
            std::thread::sleep(Duration::from_secs(delay));
            delay = (delay * 2).min(30);
        }
    }
}

fn item(kind: &str, result: &str, action: &str, code: Option<ErrorCode>) -> WireRestoreItem {
    WireRestoreItem { kind: kind.into(), result: result.into(), action: action.into(), error_code: code }
}

fn validate(p: &TunProfile) -> HResult<Vec<(String, NodeConfig)>> {
    if p.nodes.is_empty() || p.nodes.len() > 64 {
        return err(ErrorCode::ValidationFailed, "node count");
    }
    let mut out = Vec::new();
    for n in &p.nodes {
        let id = n.id();
        if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return err(ErrorCode::ValidationFailed, "node id");
        }
        let c = n.to_config().map_err(|code| (ErrorCode::ValidationFailed, Some(format!("node field: {code:?}"))))?;
        out.push((id.to_string(), c));
    }
    if !out.iter().any(|(id, _)| *id == p.selected) {
        return err(ErrorCode::NodeNotFound, "selected node not in profile");
    }
    Ok(out)
}

pub fn verify_core() -> HResult<()> {
    if EXPECTED_SINGBOX_SHA256.is_empty() {
        return err(ErrorCode::CoreBinaryMissing, "helper built without the core");
    }
    match util::sha256_file(&sys::singbox_path()) {
        Ok(h) if h == EXPECTED_SINGBOX_SHA256 => Ok(()),
        Ok(_) => err(ErrorCode::CoreChecksumMismatch, "installed core hash mismatch"),
        Err(e) => err(ErrorCode::CoreBinaryMissing, e.to_string()),
    }
}

pub fn verified_rules() -> HResult<RuleSets> {
    let m: serde_json::Value = serde_json::from_str(MANIFEST).expect("embedded manifest");
    let dir = data_dir().join("rules");
    let mut path_of = std::collections::HashMap::new();
    for set in m["sets"].as_array().into_iter().flatten() {
        let name = set["name"].as_str().unwrap_or_default();
        let file = dir.join(set["path"].as_str().unwrap_or_default());
        match util::sha256_file(&file) {
            Ok(h) if Some(h.as_str()) == set["sha256"].as_str() => {
                path_of.insert(name.to_string(), file);
            }
            _ => return err(ErrorCode::RulesValidationFailed, format!("{name} missing or modified")),
        }
    }
    match (path_of.remove("geosite-cn"), path_of.remove("geoip-cn")) {
        (Some(a), Some(b)) => Ok(RuleSets { geosite_cn: a, geoip_cn: b }),
        _ => err(ErrorCode::RulesMissing, "manifest incomplete"),
    }
}

fn register_secrets(r: &mut edge_core::redact::Redactor, c: &NodeConfig) {
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

/// Server IPs + ports for the kill switch (resolved now, while the system
/// resolver still works directly).
fn resolve_servers(configs: &[(String, NodeConfig)]) -> Vec<ServerRule> {
    let mut out = Vec::new();
    for (_, c) in configs {
        let (server, rules): (&str, Vec<(&'static str, String)>) = match c {
            NodeConfig::Hysteria2(h) => {
                let ports = if h.server_ports.is_empty() {
                    vec![("udp", h.port.to_string())]
                } else {
                    h.server_ports.iter().map(|r| ("udp", if r.start == r.end { r.start.to_string() } else { format!("{}:{}", r.start, r.end) })).collect()
                };
                (&h.server, ports)
            }
            NodeConfig::VlessReality(v) => (&v.server, vec![("tcp", v.port.to_string())]),
        };
        let ips: Vec<std::net::IpAddr> = match server.parse() {
            Ok(ip) => vec![ip],
            Err(_) => (server, 0).to_socket_addrs().map(|it| it.map(|a| a.ip()).filter(|ip| ip.is_ipv4()).collect()).unwrap_or_default(),
        };
        for ip in ips {
            for (proto, ports) in &rules {
                let r = ServerRule { ip, proto, ports: ports.clone() };
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

fn free_port() -> HResult<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| (ErrorCode::PortInUse, Some(e.to_string())))
}

fn spawn_core(p: &TunProfile, configs: &[(String, NodeConfig)], rules: &RuleSets) -> HResult<Running> {
    let port = free_port()?;
    let secret = uuid::Uuid::new_v4().simple().to_string();
    let cfg = singbox::generate(&GenInput {
        nodes: configs.iter().map(|(id, c)| (id.as_str(), c)).collect(),
        selected: &p.selected,
        routing: match p.routing {
            WireRouting::Rule => singbox::Routing::Rule,
            WireRouting::Global => singbox::Routing::Global,
        },
        dns: match p.dns {
            WireDns::Mainland => singbox::DnsProfile::Mainland,
            WireDns::Overseas => singbox::DnsProfile::Overseas,
        },
        capture: Capture::Tun,
        allow_lan: p.allow_lan,
        controller_port: port,
        controller_secret: &secret,
        rule_sets: rules.clone(),
        direct_fallback: p.direct_fallback,
    })
    .map_err(|e| (ErrorCode::CoreConfigInvalid, Some(format!("{e:?}"))))?;
    let work = data_dir().join("run");
    util::ensure_private_dir(&work).map_err(|e| (ErrorCode::TunStartFailed, Some(e.to_string())))?;
    let mut child = sys::core_command()
        .args(["run", "-c", "stdin", "--disable-color", "-D"])
        .arg(&work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| (ErrorCode::TunStartFailed, Some(e.to_string())))?;
    child
        .stdin
        .take()
        .expect("piped")
        .write_all(&serde_json::to_vec(&cfg).expect("config"))
        .map_err(|e| (ErrorCode::TunStartFailed, Some(e.to_string())))?;
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    for s in [child.stdout.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>)]
        .into_iter()
        .flatten()
    {
        let tail = tail.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                util::log(&format!("core: {line}"));
                let mut t = tail.lock().unwrap_or_else(|p| p.into_inner());
                if t.len() == 10 {
                    t.remove(0);
                }
                t.push(line);
            }
        });
    }
    // Wait for the Clash API.
    let agent: ureq::Agent =
        ureq::Agent::config_builder().proxy(None).timeout_global(Some(Duration::from_secs(2))).build().into();
    let start = Instant::now();
    loop {
        let ok = agent
            .get(format!("http://127.0.0.1:{port}/version"))
            .header("Authorization", format!("Bearer {secret}"))
            .call()
            .is_ok();
        if ok {
            break;
        }
        if child.try_wait().ok().flatten().is_some() || start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            std::thread::sleep(Duration::from_millis(100));
            let t = tail.lock().unwrap_or_else(|p| p.into_inner()).join(" / ");
            return err(ErrorCode::TunStartFailed, format!("core did not start: {t}"));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Ok(Running { child, controller_port: port, controller_secret: secret, selected: p.selected.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use edge_core::link::parse_link;

    fn profile(nodes: Vec<WireNode>, selected: &str) -> TunProfile {
        TunProfile { nodes, selected: selected.into(), routing: WireRouting::Rule, dns: WireDns::Mainland, allow_lan: true, kill_switch: true, direct_fallback: false }
    }

    #[test]
    fn profile_validation() {
        let hy = parse_link("hy2://FAKE-pass@203.0.113.7:443,20000-30000/?sni=proxy.example.com").unwrap().config;
        let ok = profile(vec![WireNode::from_config("node-1", &hy)], "node-1");
        assert_eq!(validate(&ok).unwrap().len(), 1);
        assert_eq!(validate(&profile(vec![], "x")).unwrap_err().0, ErrorCode::ValidationFailed);
        assert_eq!(validate(&profile(ok.nodes.clone(), "other")).unwrap_err().0, ErrorCode::NodeNotFound);
        let mut bad_id = ok.clone();
        if let WireNode::Hysteria2 { id, .. } = &mut bad_id.nodes[0] {
            *id = "../etc".into();
        }
        bad_id.selected = "../etc".into();
        assert_eq!(validate(&bad_id).unwrap_err().0, ErrorCode::ValidationFailed);
    }

    #[test]
    fn kill_switch_servers_from_ip_literals() {
        let hy = parse_link("hy2://FAKE-pass@203.0.113.7:443,20000-30000/").unwrap().config;
        let vl = parse_link(&format!(
            "vless://00000000-0000-4000-8000-0000000000aa@203.0.113.8:443?security=reality&sni=www.example.org&pbk={}E&flow=xtls-rprx-vision",
            "B".repeat(42)
        ))
        .unwrap()
        .config;
        let rules = resolve_servers(&[("a".into(), hy), ("b".into(), vl)]);
        let text: Vec<String> = rules.iter().map(|r| format!("{} {} {}", r.ip, r.proto, r.ports)).collect();
        assert_eq!(text, ["203.0.113.7 udp 443", "203.0.113.7 udp 20000:30000", "203.0.113.8 tcp 443"]);
    }
}
