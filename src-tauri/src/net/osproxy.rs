//! macOS system proxy via `/usr/sbin/networksetup` (argv only, no shell).
//! Plans come from `edge_core::sysproxy`; this module runs them, verifies by
//! reading back, and reports per-service results. Windows (WinINet) is
//! implemented in the Windows phase.

use edge_core::restore::StepOutcome;
use edge_core::sysproxy::{self, Kind, ServiceProxies};

use crate::api::error::{AppError, ErrorCode};
use crate::net::journal::ServiceRecord;

pub trait Runner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String, String>;
}

/// The real thing. Only macOS.
pub struct Networksetup;

impl Runner for Networksetup {
    fn run(&self, args: &[String]) -> Result<String, String> {
        let out = std::process::Command::new("/usr/sbin/networksetup")
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        // networksetup sometimes exits 0 with an error message on stdout.
        let looks_failed = stdout.contains("** Error") || stdout.contains("is not a recognized network service");
        if out.status.success() && !looks_failed {
            Ok(stdout)
        } else {
            Err(format!("{} {}", stdout.trim(), String::from_utf8_lossy(&out.stderr).trim()))
        }
    }
}

fn a(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

pub fn list_services(r: &dyn Runner) -> Result<Vec<String>, String> {
    r.run(&a(&["-listallnetworkservices"])).map(|o| sysproxy::parse_services(&o))
}

pub fn snapshot(r: &dyn Runner, service: &str) -> Result<ServiceProxies, String> {
    let mut s = ServiceProxies { service: service.into(), ..Default::default() };
    for k in Kind::ALL {
        *s.endpoint_mut(k) = sysproxy::parse_endpoint(&r.run(&a(&[k.get_flag(), service]))?)?;
    }
    s.bypass = sysproxy::parse_bypass(&r.run(&a(&["-getproxybypassdomains", service]))?);
    let (pac_enabled, pac_url) = sysproxy::parse_pac(&r.run(&a(&["-getautoproxyurl", service]))?)?;
    s.pac_enabled = pac_enabled;
    s.pac_url = pac_url;
    s.auto_discovery = sysproxy::parse_auto_discovery(&r.run(&a(&["-getproxyautodiscovery", service]))?)?;
    Ok(s)
}

/// Snapshot every enabled service and compute what we will set. Nothing is
/// changed. Refuses if any service uses an authenticated proxy.
pub fn plan(r: &dyn Runner, port: u16, allow_lan: bool) -> Result<Vec<ServiceRecord>, AppError> {
    let fail = |d: String| AppError::new(ErrorCode::SystemProxySetFailed).with_detail(d);
    let services = list_services(r).map_err(fail)?;
    if services.is_empty() {
        return Err(fail("no enabled network services".into()));
    }
    let mut records = Vec::new();
    for svc in services {
        let prev = snapshot(r, &svc).map_err(|e| fail(format!("{svc}: {e}")))?;
        if let Some(reason) = sysproxy::refuse_reason(&prev) {
            return Err(fail(reason));
        }
        let ours = sysproxy::expected_after_set(&prev, port, allow_lan);
        records.push(ServiceRecord { prev, ours });
    }
    Ok(records)
}

/// Like [`plan`] but for the named services only (used by the opt-in real test).
pub fn plan_services(r: &dyn Runner, services: &[&str], port: u16, allow_lan: bool) -> Result<Vec<ServiceRecord>, AppError> {
    let fail = |d: String| AppError::new(ErrorCode::SystemProxySetFailed).with_detail(d);
    services
        .iter()
        .map(|svc| {
            let prev = snapshot(r, svc).map_err(|e| fail(format!("{svc}: {e}")))?;
            if let Some(reason) = sysproxy::refuse_reason(&prev) {
                return Err(fail(reason));
            }
            let ours = sysproxy::expected_after_set(&prev, port, allow_lan);
            Ok(ServiceRecord { prev, ours })
        })
        .collect()
}

/// Apply the plan and verify by reading back. On error the caller restores.
pub fn apply(r: &dyn Runner, records: &[ServiceRecord], port: u16, allow_lan: bool) -> Result<(), AppError> {
    let fail = |d: String| AppError::new(ErrorCode::SystemProxySetFailed).with_detail(d);
    for rec in records {
        for c in sysproxy::set_plan(&rec.prev, port, allow_lan) {
            r.run(&c).map_err(|e| fail(format!("{}: {e}", rec.prev.service)))?;
        }
        let now = snapshot(r, &rec.prev.service).map_err(|e| fail(format!("{}: {e}", rec.prev.service)))?;
        if now != rec.ours {
            return Err(fail(format!("{}: settings did not take effect", rec.prev.service)));
        }
    }
    Ok(())
}

/// Restore every recorded service (items the user changed meanwhile are left
/// alone). Continues after per-service failures.
pub fn restore(r: &dyn Runner, records: &[ServiceRecord]) -> StepOutcome {
    let mut done = Vec::new();
    let mut left = Vec::new();
    let mut failed = Vec::new();
    for rec in records {
        let svc = &rec.prev.service;
        let current = match snapshot(r, svc) {
            Ok(c) => c,
            Err(e) => {
                failed.push(format!("{svc}: {e}"));
                continue;
            }
        };
        let plan = sysproxy::restore_plan(&rec.prev, &rec.ours, &current);
        left.extend(plan.left_alone.iter().map(|x| format!("{svc} {x}")));
        if plan.commands.is_empty() {
            continue;
        }
        if let Some(e) = plan.commands.iter().find_map(|c| r.run(c).err()) {
            failed.push(format!("{svc}: {e}"));
            continue;
        }
        // Verify: every item we were allowed to restore now equals `prev`.
        match snapshot(r, svc) {
            Ok(after) => {
                let ok = Kind::ALL.iter().all(|k| {
                    after.endpoint(*k) == rec.prev.endpoint(*k) || current.endpoint(*k) != rec.ours.endpoint(*k)
                }) && (after.bypass == rec.prev.bypass || current.bypass != rec.ours.bypass)
                    && after.pac_enabled == rec.prev.pac_enabled
                    && after.auto_discovery == rec.prev.auto_discovery;
                if ok {
                    done.push(svc.clone());
                } else {
                    failed.push(format!("{svc}: settings did not return to their previous values"));
                }
            }
            Err(e) => failed.push(format!("{svc}: {e}")),
        }
    }
    let mut action = if done.is_empty() && failed.is_empty() {
        "System proxy was already back to its previous settings".to_string()
    } else {
        format!("Restored previous proxy settings on {}", if done.is_empty() { "no services".into() } else { done.join(", ") })
    };
    if !left.is_empty() {
        action.push_str(&format!("; left unchanged because they were modified while connected: {}", left.join(", ")));
    }
    if failed.is_empty() {
        if done.is_empty() {
            StepOutcome::NotNeeded { action }
        } else {
            StepOutcome::Restored { action }
        }
    } else {
        StepOutcome::Failed {
            action: format!("{action}; failed: {}", failed.join("; ")),
            code: ErrorCode::SystemProxyRestoreFailed,
            detail: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Windows (WinINet). Pure decisions in edge_core::winproxy; OS calls in
// crates/winsys (compiled only on Windows).
// ---------------------------------------------------------------------------

/// Read + compute what we will set. Nothing is changed.
pub fn win_plan(port: u16, allow_lan: bool) -> Result<crate::net::journal::WinRecord, AppError> {
    let fail = |d: String| AppError::new(ErrorCode::SystemProxySetFailed).with_detail(d);
    #[cfg(windows)]
    {
        let prev = edge_winsys::read_proxy().map_err(fail)?;
        if let Some(r) = edge_core::winproxy::refuse_reason(&prev) {
            return Err(fail(r));
        }
        let ours = edge_core::winproxy::ours(&prev, port, allow_lan);
        Ok(crate::net::journal::WinRecord { prev, ours })
    }
    #[cfg(not(windows))]
    {
        let _ = (port, allow_lan);
        Err(fail("Windows system proxy on a non-Windows OS".into()))
    }
}

pub fn win_apply(rec: &crate::net::journal::WinRecord) -> Result<(), AppError> {
    let fail = |d: String| AppError::new(ErrorCode::SystemProxySetFailed).with_detail(d);
    #[cfg(windows)]
    {
        edge_winsys::write_proxy(&rec.ours).map_err(fail)?;
        let now = edge_winsys::read_proxy().map_err(fail)?;
        if now.flags != rec.ours.flags || now.server != rec.ours.server {
            return Err(fail("settings did not take effect".into()));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = rec;
        Err(fail("Windows system proxy on a non-Windows OS".into()))
    }
}

pub fn win_restore(rec: &crate::net::journal::WinRecord) -> StepOutcome {
    #[cfg(windows)]
    {
        use edge_core::winproxy::{RestoreDecision, restore_target};
        let failed = |action: String| StepOutcome::Failed { action, code: ErrorCode::SystemProxyRestoreFailed, detail: None };
        let current = match edge_winsys::read_proxy() {
            Ok(c) => c,
            Err(e) => return failed(format!("Could not read the Windows proxy settings: {e}")),
        };
        match restore_target(&rec.prev, &rec.ours, &current) {
            RestoreDecision::AlreadyRestored => {
                StepOutcome::NotNeeded { action: "Windows proxy settings were already back to their previous values".into() }
            }
            RestoreDecision::ChangedByUser => StepOutcome::Restored {
                action: "Left the Windows proxy settings unchanged because they were modified while connected".into(),
            },
            RestoreDecision::Restore(prev) => match edge_winsys::write_proxy(&prev).and_then(|_| edge_winsys::read_proxy()) {
                Ok(after) if after.flags == prev.flags && after.server == prev.server => {
                    StepOutcome::Restored { action: "Restored the previous Windows proxy settings".into() }
                }
                Ok(_) => failed("The Windows proxy settings did not return to their previous values".into()),
                Err(e) => failed(format!("Could not restore the Windows proxy settings: {e}")),
            },
        }
    }
    #[cfg(not(windows))]
    {
        let _ = rec;
        StepOutcome::Skipped { action: "Windows proxy record on a non-Windows OS".into(), code: ErrorCode::UnsupportedPlatform }
    }
}

/// In-memory stand-in for `networksetup` used by tests: keeps per-service
/// settings and interprets the subset of commands we use.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct FakeMac {
        pub services: Mutex<BTreeMap<String, ServiceProxies>>,
        pub disabled: Vec<String>,
        /// Fail any command whose argv contains this string.
        pub fail_on: Mutex<Option<String>>,
        pub calls: Mutex<Vec<Vec<String>>>,
    }

    impl FakeMac {
        pub fn with(services: &[ServiceProxies]) -> Self {
            let f = FakeMac::default();
            for s in services {
                f.services.lock().unwrap().insert(s.service.clone(), s.clone());
            }
            f
        }
        pub fn get(&self, svc: &str) -> ServiceProxies {
            self.services.lock().unwrap()[svc].clone()
        }
    }

    fn ep_text(e: &sysproxy::Endpoint) -> String {
        format!(
            "Enabled: {}\nServer: {}\nPort: {}\nAuthenticated Proxy Enabled: {}\n",
            if e.enabled { "Yes" } else { "No" },
            e.server,
            e.port,
            if e.authenticated { 1 } else { 0 }
        )
    }

    impl Runner for FakeMac {
        fn run(&self, args: &[String]) -> Result<String, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            if let Some(f) = self.fail_on.lock().unwrap().as_ref()
                && args.iter().any(|x| x.contains(f.as_str()))
            {
                return Err("** Error: simulated failure".into());
            }
            let mut m = self.services.lock().unwrap();
            let flag = args[0].as_str();
            if flag == "-listallnetworkservices" {
                let mut out = "An asterisk (*) denotes that a network service is disabled.\n".to_string();
                for k in m.keys() {
                    out.push_str(&format!("{k}\n"));
                }
                for d in &self.disabled {
                    out.push_str(&format!("*{d}\n"));
                }
                return Ok(out);
            }
            let svc = m.get_mut(&args[1]).ok_or("not a recognized network service")?;
            let kind = |f: &str| match f {
                "-getwebproxy" | "-setwebproxy" | "-setwebproxystate" => Some(Kind::Web),
                "-getsecurewebproxy" | "-setsecurewebproxy" | "-setsecurewebproxystate" => Some(Kind::SecureWeb),
                "-getsocksfirewallproxy" | "-setsocksfirewallproxy" | "-setsocksfirewallproxystate" => Some(Kind::Socks),
                _ => None,
            };
            match flag {
                f if f.starts_with("-get") && kind(f).is_some() => Ok(ep_text(svc.endpoint(kind(f).unwrap()))),
                f if f.ends_with("state") && kind(f).is_some() => {
                    svc.endpoint_mut(kind(f).unwrap()).enabled = args[2] == "on";
                    Ok(String::new())
                }
                f if kind(f).is_some() => {
                    // Real networksetup: -set*proxy turns the proxy on.
                    let e = svc.endpoint_mut(kind(f).unwrap());
                    e.server = args[2].clone();
                    e.port = args[3].parse().unwrap_or(0);
                    e.enabled = true;
                    e.authenticated = false;
                    Ok(String::new())
                }
                "-getproxybypassdomains" => Ok(if svc.bypass.is_empty() {
                    format!("There aren't any bypass domains set on {}.\n", svc.service)
                } else {
                    svc.bypass.join("\n") + "\n"
                }),
                "-setproxybypassdomains" => {
                    svc.bypass = if args[2] == "Empty" { vec![] } else { args[2..].to_vec() };
                    Ok(String::new())
                }
                "-getautoproxyurl" => Ok(format!(
                    "URL: {}\nEnabled: {}\n",
                    if svc.pac_url.is_empty() { "(null)" } else { &svc.pac_url },
                    if svc.pac_enabled { "Yes" } else { "No" }
                )),
                "-setautoproxyurl" => {
                    svc.pac_url = args[2].clone();
                    svc.pac_enabled = true;
                    Ok(String::new())
                }
                "-setautoproxystate" => {
                    svc.pac_enabled = args[2] == "on";
                    Ok(String::new())
                }
                "-getproxyautodiscovery" => {
                    Ok(format!("Auto Proxy Discovery: {}\n", if svc.auto_discovery { "On" } else { "Off" }))
                }
                "-setproxyautodiscovery" => {
                    svc.auto_discovery = args[2] == "on";
                    Ok(String::new())
                }
                _ => Err(format!("fake: unsupported {flag}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeMac;
    use super::*;
    use edge_core::sysproxy::Endpoint;

    fn svc(name: &str) -> ServiceProxies {
        ServiceProxies { service: name.into(), ..Default::default() }
    }

    #[test]
    fn plan_apply_restore_is_exact() {
        let mut eth = svc("Ethernet");
        eth.web = Endpoint { enabled: true, server: "proxy.corp.example".into(), port: 8080, authenticated: false };
        eth.pac_enabled = true;
        eth.pac_url = "http://wpad.example/p.pac".into();
        eth.auto_discovery = true;
        eth.bypass = vec!["*.corp.example".into()];
        let before = vec![svc("Wi-Fi"), eth];
        let mac = FakeMac::with(&before);
        let recs = plan(&mac, 17890, true).unwrap();
        assert_eq!(recs.len(), 2);
        apply(&mac, &recs, 17890, true).unwrap();
        assert_eq!(mac.get("Wi-Fi").web.server, "127.0.0.1");
        assert!(!mac.get("Ethernet").pac_enabled);
        assert!(matches!(restore(&mac, &recs), StepOutcome::Restored { .. }));
        for s in &before {
            assert_eq!(&mac.get(&s.service), s, "{} restored exactly", s.service);
        }
        // Second restore: nothing to do.
        assert!(matches!(restore(&mac, &recs), StepOutcome::NotNeeded { .. }));
    }

    #[test]
    fn disabled_services_skipped_and_auth_proxy_refused() {
        let mut mac = FakeMac::with(&[svc("Wi-Fi")]);
        mac.disabled = vec!["USB LAN".into()];
        assert_eq!(plan(&mac, 1, true).unwrap().len(), 1);
        let mut auth = svc("Wi-Fi");
        auth.web.authenticated = true;
        let mac = FakeMac::with(&[auth]);
        assert_eq!(plan(&mac, 1, true).unwrap_err().code, ErrorCode::SystemProxySetFailed);
    }

    #[test]
    fn restore_failure_is_reported_and_retry_succeeds() {
        let mac = FakeMac::with(&[svc("Wi-Fi")]);
        let recs = plan(&mac, 1234, true).unwrap();
        apply(&mac, &recs, 1234, true).unwrap();
        *mac.fail_on.lock().unwrap() = Some("-setsecurewebproxy".into());
        match restore(&mac, &recs) {
            StepOutcome::Failed { code, .. } => assert_eq!(code, ErrorCode::SystemProxyRestoreFailed),
            other => panic!("expected failure, got {other:?}"),
        }
        *mac.fail_on.lock().unwrap() = None;
        assert!(matches!(restore(&mac, &recs), StepOutcome::Restored { .. }));
        assert_eq!(mac.get("Wi-Fi"), svc("Wi-Fi"));
    }

    #[test]
    fn user_change_while_connected_is_kept() {
        let mac = FakeMac::with(&[svc("Wi-Fi")]);
        let recs = plan(&mac, 1234, true).unwrap();
        apply(&mac, &recs, 1234, true).unwrap();
        mac.run(&a(&["-setwebproxy", "Wi-Fi", "10.0.0.9", "3128", "off"])).unwrap();
        match restore(&mac, &recs) {
            StepOutcome::Restored { action } => assert!(action.contains("left unchanged"), "{action}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(mac.get("Wi-Fi").web.server, "10.0.0.9");
        assert_eq!(mac.get("Wi-Fi").secure_web, Endpoint::default());
    }

    /// Opt-in, **changes real macOS settings** on one service (default the
    /// usually-unused "Thunderbolt Bridge"; override with EDGE_TEST_SERVICE),
    /// then restores them and checks they match the snapshot exactly.
    /// Run: `cargo test -p edge-app --lib real_networksetup_roundtrip -- --ignored --nocapture`
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn real_networksetup_roundtrip() {
        let service = std::env::var("EDGE_TEST_SERVICE").unwrap_or_else(|_| "Thunderbolt Bridge".into());
        let r = Networksetup;
        let before = snapshot(&r, &service).expect("service exists");
        let recs = plan_services(&r, &[service.as_str()], 17999, true).expect("plan");
        let apply_result = apply(&r, &recs, 17999, true);
        let during = snapshot(&r, &service).unwrap();
        let outcome = restore(&r, &recs);
        let after = snapshot(&r, &service).unwrap();
        println!("service: {service}\napply: {apply_result:?}\nrestore: {outcome:?}");
        apply_result.expect("apply");
        assert_eq!(during.web.server, "127.0.0.1");
        assert!(during.web.enabled && during.secure_web.enabled && during.socks.enabled);
        assert_eq!(after, before, "settings restored exactly");
    }

    #[test]
    fn apply_detects_settings_that_do_not_stick() {
        let mac = FakeMac::with(&[svc("Wi-Fi")]);
        let recs = plan(&mac, 1234, true).unwrap();
        *mac.fail_on.lock().unwrap() = Some("-setsocksfirewallproxy".into());
        assert_eq!(apply(&mac, &recs, 1234, true).unwrap_err().code, ErrorCode::SystemProxySetFailed);
    }
}
