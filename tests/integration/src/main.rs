//! Real-server harness (local only — never in CI).
//!
//! Reads share links from files in the directory given by the environment
//! variable `EDGE_TEST_LINKS` (by default only `iphone-hy2.txt` and
//! `iphone-reality.txt`; override with `EDGE_TEST_LINK_FILES=a.txt,b.txt`).
//! For every link it starts the bundled sing-box with the app's own config
//! generator in the system-proxy shape (local mixed HTTP+SOCKS inbound, Rule
//! routing; **no OS settings are changed**), then:
//!   1. real HTTP probes through the node to both probe targets (Clash delay API),
//!   2. an HTTP-proxy request through the local inbound (cp.cloudflare.com, 204) —
//!      the path apps use when they honour the system proxy,
//!   3. the exit-IP **country** via Cloudflare trace through the SOCKS inbound.
//!
//! Output: a redacted summary only — node type, pass/fail, latencies, exit
//! country, error codes. Never hostnames, IPs, passwords, UUIDs, keys, short
//! IDs or links. The config reaches sing-box on stdin; the only files written
//! are sing-box's cache in a temp dir, deleted afterwards.
//!
//! Self-test on other files: `--links-dir <dir> --files a.txt,b.txt`.
//! Optional: `EDGE_TEST_DNS=overseas` (default mainland);
//! `EDGE_TEST_DETAILS=1` adds a redacted technical detail column.

#![cfg_attr(not(target_os = "macos"), allow(unused))]

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("edge-harness runs on macOS only (local real-server testing)");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    mac::main();
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use edge_app_lib::net::{clash, coreproc::CoreProcess, logbuf, probe};
    use edge_core::link::{self, NodeConfig};
    use edge_core::redact::Redactor;
    use edge_core::singbox::{self, Capture, DnsProfile, GenInput, Routing, RuleSets};

    const DEFAULT_FILES: &[&str] = &["iphone-hy2.txt", "iphone-reality.txt"];

    pub fn main() {
        std::process::exit(match run() {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(msg) => {
                eprintln!("harness: {msg}");
                1
            }
        });
    }

    struct Outcome {
        probes_ok: bool,
        probes: String,
        http_proxy_ok: bool,
        http_ms: Option<u128>,
        country: Option<String>,
        codes: Vec<String>,
        detail: String,
    }

    fn run() -> Result<bool, String> {
        // `--links-dir <dir>` / `--files a,b` override the environment (used to
        // self-test the harness on fake links).
        let args: Vec<String> = std::env::args().collect();
        let flag = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let dir = match flag("--links-dir") {
            Some(d) => PathBuf::from(d),
            None => std::env::var_os("EDGE_TEST_LINKS")
                .map(PathBuf::from)
                .ok_or("EDGE_TEST_LINKS is not set")?,
        };
        let list = flag("--files").or_else(|| std::env::var("EDGE_TEST_LINK_FILES").ok());
        let files: Vec<String> = match list {
            Some(v) => v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            None => DEFAULT_FILES.iter().map(|s| s.to_string()).collect(),
        };
        let dns = match std::env::var("EDGE_TEST_DNS").as_deref() {
            Ok("overseas") => DnsProfile::Overseas,
            _ => DnsProfile::Mainland,
        };
        let details = std::env::var_os("EDGE_TEST_DETAILS").is_some();
        let bin = edge_app_lib::core_bin::locate_verified()
            .map_err(|e| format!("{:?}: run `npm run fetch-core` first", e.code))?;
        let rules_tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let rules_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/rules");
        let rules = edge_app_lib::rules::ensure_active(
            &edge_app_lib::rules::embedded_manifest(),
            &rules_src,
            rules_tmp.path(),
        )
        .map_err(|e| format!("{:?}: run `npm run fetch-core` first", e.code))?;

        // Parse everything first so every secret and endpoint is registered with
        // the redactor before anything is printed.
        let mut nodes: Vec<(String, Result<NodeConfig, String>)> = Vec::new();
        let mut redactor = Redactor::new().hide_all_ips(true);
        for (fi, f) in files.iter().enumerate() {
            let text = std::fs::read_to_string(dir.join(f))
                .map_err(|e| format!("link file #{}: {:?}", fi + 1, e.kind()))?;
            for r in link::parse_links(&text) {
                let label = format!("file{} line{}", fi + 1, r.line);
                match r.result {
                    Ok(p) => {
                        register(&mut redactor, &p.config);
                        nodes.push((label, Ok(p.config)));
                    }
                    Err(e) => nodes.push((
                        label,
                        Err(format!(
                            "{}{}",
                            serde_json::to_string(&e.code)
                                .unwrap_or_default()
                                .trim_matches('"'),
                            e.param.map(|p| format!(" (param {p})")).unwrap_or_default()
                        )),
                    )),
                }
            }
        }
        logbuf::global().set_redactor(redactor.clone());

        println!(
            "EdgeClient harness | {} | sing-box {} | DNS profile {:?} | system-proxy shape, Rule routing, OS settings untouched",
            edge_core::clock::now_rfc3339(),
            edge_app_lib::core_bin::CORE_VERSION,
            dns
        );
        println!(
            "{:<14} {:<14} {:<6} {:<30} {:<12} {:<5} codes",
            "link", "type", "result", "probes (ms)", "http-proxy", "exit"
        );
        let mut all_ok = true;
        for (label, cfg) in &nodes {
            let cfg = match cfg {
                Ok(c) => c,
                Err(code) => {
                    all_ok = false;
                    println!(
                        "{label:<14} {:<14} {:<6} {:<30} {:<12} {:<5} {code}",
                        "-", "SKIP", "-", "-", "-"
                    );
                    continue;
                }
            };
            let kind = match cfg {
                NodeConfig::Hysteria2(_) => "hysteria2",
                NodeConfig::VlessReality(_) => "vless-reality",
            };
            let rs = RuleSets {
                geosite_cn: rules.geosite_cn.clone(),
                geoip_cn: rules.geoip_cn.clone(),
            };
            let o = test_node(&bin, cfg, rs, dns);
            let pass = o.probes_ok && o.http_proxy_ok;
            all_ok &= pass;
            println!(
                "{label:<14} {kind:<14} {:<6} {:<30} {:<12} {:<5} {}",
                if pass { "PASS" } else { "FAIL" },
                o.probes,
                match o.http_ms {
                    Some(ms) => format!("ok {ms}ms"),
                    None => "fail".into(),
                },
                o.country.as_deref().unwrap_or("-"),
                o.codes.join(","),
            );
            if details && !o.detail.is_empty() {
                println!("    detail: {}", redactor.redact(&o.detail));
            }
        }
        Ok(all_ok)
    }

    fn register(r: &mut Redactor, c: &NodeConfig) {
        match c {
            NodeConfig::Hysteria2(h) => {
                r.add_secret(h.password.expose());
                if let Some(o) = &h.obfs_password {
                    r.add_secret(o.expose());
                }
                r.add_endpoint(&h.server);
                if let Some(s) = &h.sni {
                    r.add_endpoint(s);
                }
            }
            NodeConfig::VlessReality(v) => {
                r.add_secret(v.uuid.expose());
                r.add_secret(v.public_key.expose());
                r.add_secret(v.short_id.expose());
                r.add_endpoint(&v.server);
                r.add_endpoint(&v.sni);
            }
        }
    }

    fn test_node(bin: &Path, cfg: &NodeConfig, rules: RuleSets, dns: DnsProfile) -> Outcome {
        let fail = |code: &str, detail: String| Outcome {
            probes_ok: false,
            probes: "-".into(),
            http_proxy_ok: false,
            http_ms: None,
            country: None,
            codes: vec![code.into()],
            detail,
        };
        let (Ok(port), Ok(ctl)) = (clash::free_port(), clash::free_port()) else {
            return fail("PORT_IN_USE", String::new());
        };
        let secret = clash::random_secret();
        logbuf::global().add_secret(&secret);
        let config = match singbox::generate(&GenInput {
            nodes: vec![("n", cfg)],
            selected: "n",
            routing: Routing::Rule,
            dns,
            capture: Capture::SystemProxy { port },
            allow_lan: true,
            controller_port: ctl,
            controller_secret: &secret,
            rule_sets: rules,
            direct_fallback: false,
        }) {
            Ok(c) => c,
            Err(e) => return fail("CORE_CONFIG_INVALID", format!("{e:?}")),
        };
        let Ok(work) = tempfile::tempdir() else {
            return fail("INTERNAL", "temp dir".into());
        };
        let mut core = match CoreProcess::spawn(bin, &config, work.path()) {
            Ok(c) => c,
            Err(e) => return fail("CORE_START_FAILED", e.detail.unwrap_or_default()),
        };
        let api = clash::ClashApi::new(ctl, &secret);
        if let Err(e) = api.wait_ready(Duration::from_secs(8)) {
            let tail = core.tail_text();
            core.stop();
            return fail("CORE_START_FAILED", format!("{e} | {tail}"));
        }
        let tag = singbox::node_tag("n");
        let mut parts = Vec::new();
        let mut errs = Vec::new();
        let mut codes = Vec::new();
        let mut any = false;
        for t in probe::DEFAULT_TARGETS {
            let short = probe::short_target(t);
            match api.delay(&tag, t, 6000) {
                Ok(ms) => {
                    any = true;
                    parts.push(format!("{short}={ms}"));
                }
                Err(e) => {
                    parts.push(format!("{short}=fail"));
                    errs.push(format!("{short}: {e}"));
                }
            }
        }
        if !any {
            codes.push("PROBE_FAILED".to_string());
        }
        let start = Instant::now();
        let http = http_via_proxy(port);
        let http_ms = http.is_ok().then(|| start.elapsed().as_millis());
        if let Err(e) = &http {
            codes.push("HTTP_PROXY_FAILED".into());
            errs.push(format!("http-proxy: {e}"));
        }
        let country = if any {
            match probe::exit_country(port, 6000) {
                Ok(c) => Some(c),
                Err(e) => {
                    codes.push("EXIT_COUNTRY_UNKNOWN".into());
                    errs.push(format!("exit-country: {e}"));
                    None
                }
            }
        } else {
            None
        };
        let detail = if errs.is_empty() {
            String::new()
        } else {
            format!("{} | core: {}", errs.join("; "), core.tail_text()).replace('\n', " / ")
        };
        core.stop();
        Outcome {
            probes_ok: any,
            probes: parts.join(" "),
            http_proxy_ok: http.is_ok(),
            http_ms,
            country,
            codes,
            detail,
        }
    }

    /// Plain HTTP through the local mixed inbound used as an HTTP proxy. The
    /// target must NOT be in geosite-cn (www.gstatic.com is, so Rule mode would
    /// send it direct and the check would not exercise the node).
    fn http_via_proxy(port: u16) -> Result<(), String> {
        let agent = probe::http_proxy(port)?;
        let mut resp = agent
            .get("http://cp.cloudflare.com/")
            .call()
            .map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let _ = resp.body_mut().read_to_string();
        if status == 204 {
            Ok(())
        } else {
            Err(format!("status {status}"))
        }
    }
}
