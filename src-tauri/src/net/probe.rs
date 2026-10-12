//! Real end-to-end node tests: a temporary sing-box for one node on random
//! local ports (no OS changes), HTTP probes **through the node** to the probe
//! targets via the Clash delay API, and the exit country from Cloudflare's
//! trace endpoint fetched through the node's local SOCKS inbound.
//! Used by `test_node` and the real-server harness.

use std::path::Path;
use std::time::Duration;

use edge_core::link::NodeConfig;
use edge_core::singbox::{self, Capture, DnsProfile, GenInput, Routing, RuleSets};

use crate::api::error::{AppError, ErrorCode};
use crate::net::clash::{self, ClashApi};
use crate::net::coreproc::CoreProcess;

pub const DEFAULT_TARGETS: [&str; 2] = ["https://www.gstatic.com/generate_204", "https://cp.cloudflare.com/"];
/// Exit-country lookup: Cloudflare trace on 1.1.1.1 (our proxied DNS provider).
const TRACE_URL: &str = "https://1.1.1.1/cdn-cgi/trace";

#[derive(Debug, Clone)]
pub struct TargetResult {
    pub target: String,
    pub latency_ms: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProbeReport {
    pub targets: Vec<TargetResult>,
    pub exit_country: Option<String>,
    /// Redacted reason when nothing passed.
    pub error: Option<AppError>,
}

impl ProbeReport {
    pub fn ok(&self) -> bool {
        self.targets.iter().any(|t| t.latency_ms.is_some())
    }
    pub fn median_latency(&self) -> Option<u32> {
        let mut v: Vec<u32> = self.targets.iter().filter_map(|t| t.latency_ms).collect();
        v.sort_unstable();
        v.get(v.len().saturating_sub(1) / 2).copied()
    }
}

pub fn short_target(url: &str) -> String {
    url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or(url).to_string()
}

/// Probe one node with a temporary core. Blocking; takes a few seconds.
pub fn probe_node(
    bin: &Path,
    node: &NodeConfig,
    rules: RuleSets,
    dns: DnsProfile,
    workdir: &Path,
    targets: &[&str],
    timeout_ms: u32,
) -> Result<ProbeReport, AppError> {
    let port = clash::free_port().map_err(|e| AppError::new(ErrorCode::PortInUse).with_detail(e.to_string()))?;
    let ctl = clash::free_port().map_err(|e| AppError::new(ErrorCode::PortInUse).with_detail(e.to_string()))?;
    let secret = clash::random_secret();
    crate::net::logbuf::global().add_secret(&secret);
    let cfg = singbox::generate(&GenInput {
        nodes: vec![("probe", node)],
        selected: "probe",
        // Global: probe traffic must go through the node even for CN targets.
        routing: Routing::Global,
        dns,
        capture: Capture::SystemProxy { port },
        allow_lan: true,
        controller_port: ctl,
        controller_secret: &secret,
        rule_sets: rules,
    })
    .map_err(|e| AppError::new(ErrorCode::Internal).with_detail(format!("{e:?}")))?;
    let mut core = CoreProcess::spawn(bin, &cfg, workdir)?;
    let api = ClashApi::new(ctl, &secret);
    if let Err(e) = api.wait_ready(Duration::from_secs(8)) {
        let tail = core.tail_text();
        core.stop();
        return Err(AppError::new(ErrorCode::CoreStartFailed).with_detail(format!("{e}\n{tail}")));
    }
    let tag = singbox::node_tag("probe");
    let results: Vec<TargetResult> = targets
        .iter()
        .map(|t| match api.delay(&tag, t, timeout_ms) {
            Ok(ms) => TargetResult { target: short_target(t), latency_ms: Some(ms), error: None },
            Err(e) => TargetResult { target: short_target(t), latency_ms: None, error: Some(e) },
        })
        .collect();
    let any_ok = results.iter().any(|r| r.latency_ms.is_some());
    let exit_country = if any_ok { exit_country(port, timeout_ms).ok() } else { None };
    let error = (!any_ok).then(|| {
        let detail = results.iter().filter_map(|r| r.error.clone()).collect::<Vec<_>>().join("; ");
        let tail = core.tail_text();
        AppError::new(classify(&detail, &tail)).with_detail(format!("{detail}\n{tail}"))
    });
    core.stop();
    Ok(ProbeReport { targets: results, exit_country, error })
}

/// Map probe / core errors to the closest stable code.
fn classify(detail: &str, tail: &str) -> ErrorCode {
    let t = format!("{detail}\n{tail}").to_ascii_lowercase();
    if t.contains("authentication") || t.contains("auth failed") || t.contains("unauthorized") {
        ErrorCode::NodeAuthFailed
    } else if t.contains("tls") || t.contains("handshake") || t.contains("reality") || t.contains("certificate") {
        ErrorCode::NodeHandshakeFailed
    } else if t.contains("timeout") || t.contains("deadline") {
        ErrorCode::ProbeTimeout
    } else if t.contains("no such host") || t.contains("lookup") {
        ErrorCode::DnsBootstrapFailed
    } else if t.contains("refused") || t.contains("unreachable") {
        ErrorCode::NodeUnreachable
    } else {
        ErrorCode::ProbeFailed
    }
}

/// Agent that sends plain HTTP through the local mixed inbound as an HTTP
/// proxy — the same path apps take when they honour the system proxy.
pub fn http_proxy(mixed_port: u16) -> Result<ureq::Agent, String> {
    let proxy = ureq::Proxy::new(&format!("http://127.0.0.1:{mixed_port}")).map_err(|e| e.to_string())?;
    Ok(ureq::Agent::config_builder()
        .proxy(Some(proxy))
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(12)))
        .build()
        .into())
}

/// ISO country of the exit IP, via the node's local SOCKS5 inbound. Only the
/// `loc=` field is read; the IP address is never stored or printed.
pub fn exit_country(mixed_port: u16, timeout_ms: u32) -> Result<String, String> {
    let proxy = ureq::Proxy::new(&format!("socks5://127.0.0.1:{mixed_port}")).map_err(|e| e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(Some(proxy))
        .timeout_global(Some(Duration::from_millis(timeout_ms as u64 * 2)))
        .build()
        .into();
    let body = agent
        .get(TRACE_URL)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    body.lines()
        .find_map(|l| l.strip_prefix("loc="))
        .map(|c| c.trim().to_string())
        .filter(|c| c.len() == 2 && c.chars().all(|x| x.is_ascii_uppercase()))
        .ok_or_else(|| "no loc= in trace".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whole pipeline against an unroutable TEST-NET address (no real server,
    /// no OS changes): core starts from stdin config, API answers, probes fail
    /// cleanly with a code. Skipped if the core / rules are not fetched.
    #[test]
    fn dead_node_fails_cleanly() {
        let bin = match crate::core_bin::locate_verified() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipped: core not available: {:?} {:?}", e.code, e.detail);
                return;
            }
        };
        let rules_dir = crate::rules::bundled_dir(None);
        let active = tempfile::tempdir().unwrap();
        let Ok(r) = crate::rules::ensure_active(&crate::rules::embedded_manifest(), &rules_dir, active.path()) else {
            eprintln!("skipped: rules not fetched");
            return;
        };
        let node = edge_core::link::parse_link("hy2://FAKE-pass@203.0.113.7:24443/?sni=proxy.example.com").unwrap().config;
        let work = tempfile::tempdir().unwrap();
        let rep = probe_node(
            &bin,
            &node,
            RuleSets { geosite_cn: r.geosite_cn, geoip_cn: r.geoip_cn },
            DnsProfile::Overseas,
            work.path(),
            &DEFAULT_TARGETS,
            1500,
        )
        .expect("core starts and API answers");
        assert!(!rep.ok());
        assert!(rep.exit_country.is_none());
        let e = rep.error.expect("error reported");
        assert_ne!(e.code, ErrorCode::CoreStartFailed);
        eprintln!("dead node → {:?}", e.code);
    }

    #[test]
    fn helpers() {
        assert_eq!(short_target("https://www.gstatic.com/generate_204"), "www.gstatic.com");
        let r = ProbeReport {
            targets: vec![
                TargetResult { target: "a".into(), latency_ms: Some(300), error: None },
                TargetResult { target: "b".into(), latency_ms: Some(100), error: None },
            ],
            exit_country: Some("GB".into()),
            error: None,
        };
        assert!(r.ok());
        assert_eq!(r.median_latency(), Some(100));
        assert_eq!(classify("context deadline exceeded", ""), ErrorCode::ProbeTimeout);
        assert_eq!(classify("", "reality verification failed"), ErrorCode::NodeHandshakeFailed);
    }
}
