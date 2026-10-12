//! macOS system proxy: parsing `networksetup` output and planning the exact
//! commands to set our proxy and to restore the previous settings. Pure — the
//! app executes the plans (`/usr/sbin/networksetup`, arguments without a shell).
//!
//! Verified on macOS 27 (2026-10-12, owner's Mac, unused "Thunderbolt Bridge"
//! service): `-setwebproxy <svc> "" 0 off` + `-setwebproxystate <svc> off`
//! restores an empty, disabled proxy exactly; `-setproxybypassdomains <svc>
//! Empty` clears the bypass list; admin users need no sudo.
//!
//! Restore rule: an item is put back only if it still holds **our** value; if
//! the user changed it while connected, it is left alone (reported as such).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub enabled: bool,
    pub server: String,
    pub port: u16,
    pub authenticated: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Web,
    SecureWeb,
    Socks,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Web, Kind::SecureWeb, Kind::Socks];
    pub fn get_flag(self) -> &'static str {
        match self {
            Kind::Web => "-getwebproxy",
            Kind::SecureWeb => "-getsecurewebproxy",
            Kind::Socks => "-getsocksfirewallproxy",
        }
    }
    fn set_flag(self) -> &'static str {
        match self {
            Kind::Web => "-setwebproxy",
            Kind::SecureWeb => "-setsecurewebproxy",
            Kind::Socks => "-setsocksfirewallproxy",
        }
    }
    fn state_flag(self) -> &'static str {
        match self {
            Kind::Web => "-setwebproxystate",
            Kind::SecureWeb => "-setsecurewebproxystate",
            Kind::Socks => "-setsocksfirewallproxystate",
        }
    }
}

/// Complete proxy-related settings of one network service.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProxies {
    pub service: String,
    pub web: Endpoint,
    pub secure_web: Endpoint,
    pub socks: Endpoint,
    pub bypass: Vec<String>,
    pub pac_enabled: bool,
    pub pac_url: String,
    pub auto_discovery: bool,
}

impl ServiceProxies {
    pub fn endpoint(&self, k: Kind) -> &Endpoint {
        match k {
            Kind::Web => &self.web,
            Kind::SecureWeb => &self.secure_web,
            Kind::Socks => &self.socks,
        }
    }
    pub fn endpoint_mut(&mut self, k: Kind) -> &mut Endpoint {
        match k {
            Kind::Web => &mut self.web,
            Kind::SecureWeb => &mut self.secure_web,
            Kind::Socks => &mut self.socks,
        }
    }
}

pub const LOOPBACK: &str = "127.0.0.1";

/// Bypass entries we add (apps honouring the system proxy go direct for these).
pub fn our_bypass(allow_lan: bool) -> Vec<String> {
    let mut v: Vec<&str> = vec!["127.0.0.1", "localhost", "::1", "*.local"];
    if allow_lan {
        v.extend(["169.254/16", "10/8", "172.16/12", "192.168/16", "100.64/10"]);
    }
    v.into_iter().map(String::from).collect()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Enabled services from `-listallnetworkservices` (skips the header line and
/// disabled services marked with `*`).
pub fn parse_services(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !l.starts_with("An asterisk"))
        .filter(|l| !l.starts_with('*'))
        .map(|l| l.trim_end().to_string())
        .collect()
}

fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines().find_map(|l| l.strip_prefix(key).map(str::trim))
}

pub fn parse_endpoint(out: &str) -> Result<Endpoint, String> {
    let enabled = match field(out, "Enabled:") {
        Some("Yes") => true,
        Some("No") => false,
        _ => return Err("missing Enabled".into()),
    };
    let server = field(out, "Server:").ok_or("missing Server")?.to_string();
    let port = field(out, "Port:").ok_or("missing Port")?.parse::<u16>().map_err(|_| "bad Port")?;
    let authenticated = field(out, "Authenticated Proxy Enabled:").is_some_and(|v| v == "1");
    Ok(Endpoint { enabled, server, port, authenticated })
}

pub fn parse_bypass(out: &str) -> Vec<String> {
    if out.starts_with("There aren't any bypass domains") {
        return vec![];
    }
    out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// `-getautoproxyurl` → (enabled, url). `(null)` → "".
pub fn parse_pac(out: &str) -> Result<(bool, String), String> {
    let url = field(out, "URL:").ok_or("missing URL")?;
    let url = if url == "(null)" { String::new() } else { url.to_string() };
    let enabled = field(out, "Enabled:").ok_or("missing Enabled")? == "Yes";
    Ok((enabled, url))
}

pub fn parse_auto_discovery(out: &str) -> Result<bool, String> {
    match field(out, "Auto Proxy Discovery:") {
        Some("On") => Ok(true),
        Some("Off") => Ok(false),
        _ => Err("missing Auto Proxy Discovery".into()),
    }
}

// ---------------------------------------------------------------------------
// Plans (each command = argv without the program)
// ---------------------------------------------------------------------------

pub type Cmd = Vec<String>;

fn cmd(parts: &[&str]) -> Cmd {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Why setting the proxy on this service is refused (we never touch it).
pub fn refuse_reason(prev: &ServiceProxies) -> Option<String> {
    Kind::ALL
        .iter()
        .find(|k| prev.endpoint(**k).authenticated)
        .map(|_| format!("{}: an authenticated proxy is configured; not modified", prev.service))
}

/// Commands that make `svc` use our proxy on 127.0.0.1:`port`.
pub fn set_plan(prev: &ServiceProxies, port: u16, allow_lan: bool) -> Vec<Cmd> {
    let svc = prev.service.as_str();
    let p = port.to_string();
    let mut v: Vec<Cmd> = Kind::ALL.iter().map(|k| cmd(&[k.set_flag(), svc, LOOPBACK, &p, "off"])).collect();
    let mut bypass = prev.bypass.clone();
    for b in our_bypass(allow_lan) {
        if !bypass.contains(&b) {
            bypass.push(b);
        }
    }
    let mut c = cmd(&["-setproxybypassdomains", svc]);
    c.extend(bypass);
    v.push(c);
    // A PAC file or auto-discovery would take precedence over manual proxies.
    if prev.pac_enabled {
        v.push(cmd(&["-setautoproxystate", svc, "off"]));
    }
    if prev.auto_discovery {
        v.push(cmd(&["-setproxyautodiscovery", svc, "off"]));
    }
    v
}

/// What `set_plan` should leave in place (for read-back verification).
pub fn expected_after_set(prev: &ServiceProxies, port: u16, allow_lan: bool) -> ServiceProxies {
    let ours = Endpoint { enabled: true, server: LOOPBACK.into(), port, authenticated: false };
    let mut s = prev.clone();
    for k in Kind::ALL {
        *s.endpoint_mut(k) = ours.clone();
    }
    for b in our_bypass(allow_lan) {
        if !s.bypass.contains(&b) {
            s.bypass.push(b);
        }
    }
    s.pac_enabled = false;
    s.auto_discovery = false;
    s
}

/// Result of planning a restore for one service.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestorePlan {
    pub commands: Vec<Cmd>,
    /// Items left alone because they no longer hold our value (user changed them).
    pub left_alone: Vec<String>,
}

/// Commands that put `prev` back, for every item that still holds the value we
/// set (`ours`). Items the user changed meanwhile are left alone.
pub fn restore_plan(prev: &ServiceProxies, ours: &ServiceProxies, current: &ServiceProxies) -> RestorePlan {
    let svc = prev.service.as_str();
    let mut plan = RestorePlan::default();
    for k in Kind::ALL {
        let (p, o, c) = (prev.endpoint(k), ours.endpoint(k), current.endpoint(k));
        if c == p {
            continue; // already as before
        }
        if c != o {
            plan.left_alone.push(format!("{k:?} proxy"));
            continue;
        }
        let port = p.port.to_string();
        plan.commands.push(cmd(&[k.set_flag(), svc, &p.server, &port, "off"]));
        if !p.enabled {
            plan.commands.push(cmd(&[k.state_flag(), svc, "off"]));
        }
    }
    if current.bypass != prev.bypass {
        if current.bypass == ours.bypass {
            let mut c = cmd(&["-setproxybypassdomains", svc]);
            if prev.bypass.is_empty() {
                c.push("Empty".into());
            } else {
                c.extend(prev.bypass.clone());
            }
            plan.commands.push(c);
        } else {
            plan.left_alone.push("bypass list".into());
        }
    }
    if prev.pac_enabled && !current.pac_enabled {
        // We only ever turned PAC off; re-enable with the previous URL.
        plan.commands.push(cmd(&["-setautoproxyurl", svc, &prev.pac_url]));
    }
    if prev.auto_discovery && !current.auto_discovery {
        plan.commands.push(cmd(&["-setproxyautodiscovery", svc, "on"]));
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_EP: &str = "Enabled: No\nServer: \nPort: 0\nAuthenticated Proxy Enabled: 0\n";

    fn empty_service(name: &str) -> ServiceProxies {
        ServiceProxies { service: name.into(), ..Default::default() }
    }

    #[test]
    fn parsing_real_outputs() {
        // Captured from the owner's Mac (macOS 27), no secrets involved.
        let list = "An asterisk (*) denotes that a network service is disabled.\nThunderbolt Bridge\nWi-Fi\n*USB LAN\n";
        assert_eq!(parse_services(list), ["Thunderbolt Bridge", "Wi-Fi"]);
        assert_eq!(parse_endpoint(EMPTY_EP).unwrap(), Endpoint::default());
        let on = "Enabled: Yes\nServer: 127.0.0.1\nPort: 1\nAuthenticated Proxy Enabled: 0\n";
        assert_eq!(parse_endpoint(on).unwrap(), Endpoint { enabled: true, server: "127.0.0.1".into(), port: 1, authenticated: false });
        assert!(parse_endpoint("garbage").is_err());
        assert!(parse_bypass("There aren't any bypass domains set on Wi-Fi.\n").is_empty());
        assert_eq!(parse_bypass("*.local\n169.254/16\n127.0.0.1\n"), ["*.local", "169.254/16", "127.0.0.1"]);
        assert_eq!(parse_pac("URL: (null)\nEnabled: No\n").unwrap(), (false, String::new()));
        assert_eq!(parse_pac("URL: http://wpad/x.pac\nEnabled: Yes\n").unwrap(), (true, "http://wpad/x.pac".into()));
        assert!(!parse_auto_discovery("Auto Proxy Discovery: Off\n").unwrap());
        assert!(parse_auto_discovery("Auto Proxy Discovery: On\n").unwrap());
    }

    #[test]
    fn set_then_restore_round_trip_from_empty() {
        let prev = empty_service("Wi-Fi");
        let set = set_plan(&prev, 17890, true);
        assert_eq!(set[0], cmd(&["-setwebproxy", "Wi-Fi", "127.0.0.1", "17890", "off"]));
        assert_eq!(set.len(), 4, "3 endpoints + bypass; no PAC/auto-discovery changes");
        let ours = expected_after_set(&prev, 17890, true);
        let plan = restore_plan(&prev, &ours, &ours);
        assert!(plan.left_alone.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                cmd(&["-setwebproxy", "Wi-Fi", "", "0", "off"]),
                cmd(&["-setwebproxystate", "Wi-Fi", "off"]),
                cmd(&["-setsecurewebproxy", "Wi-Fi", "", "0", "off"]),
                cmd(&["-setsecurewebproxystate", "Wi-Fi", "off"]),
                cmd(&["-setsocksfirewallproxy", "Wi-Fi", "", "0", "off"]),
                cmd(&["-setsocksfirewallproxystate", "Wi-Fi", "off"]),
                cmd(&["-setproxybypassdomains", "Wi-Fi", "Empty"]),
            ]
        );
    }

    #[test]
    fn previous_manual_proxy_pac_and_discovery_are_restored() {
        let mut prev = empty_service("Ethernet");
        prev.web = Endpoint { enabled: true, server: "proxy.corp.example".into(), port: 8080, authenticated: false };
        prev.secure_web = Endpoint { enabled: false, server: "old.example".into(), port: 3128, authenticated: false };
        prev.bypass = vec!["*.corp.example".into()];
        prev.pac_enabled = true;
        prev.pac_url = "http://wpad.corp.example/proxy.pac".into();
        prev.auto_discovery = true;
        let set = set_plan(&prev, 1234, false);
        assert!(set.contains(&cmd(&["-setautoproxystate", "Ethernet", "off"])));
        assert!(set.contains(&cmd(&["-setproxyautodiscovery", "Ethernet", "off"])));
        let bypass = set.iter().find(|c| c[0] == "-setproxybypassdomains").unwrap();
        assert_eq!(bypass[2], "*.corp.example", "user's bypass entries kept first");
        let ours = expected_after_set(&prev, 1234, false);
        let plan = restore_plan(&prev, &ours, &ours);
        assert!(plan.commands.contains(&cmd(&["-setwebproxy", "Ethernet", "proxy.corp.example", "8080", "off"])));
        assert!(!plan.commands.contains(&cmd(&["-setwebproxystate", "Ethernet", "off"])), "web stays on");
        assert!(plan.commands.contains(&cmd(&["-setsecurewebproxy", "Ethernet", "old.example", "3128", "off"])));
        assert!(plan.commands.contains(&cmd(&["-setsecurewebproxystate", "Ethernet", "off"])));
        assert!(plan.commands.contains(&cmd(&["-setproxybypassdomains", "Ethernet", "*.corp.example"])));
        assert!(plan.commands.contains(&cmd(&["-setautoproxyurl", "Ethernet", "http://wpad.corp.example/proxy.pac"])));
        assert!(plan.commands.contains(&cmd(&["-setproxyautodiscovery", "Ethernet", "on"])));
    }

    #[test]
    fn user_changes_while_connected_are_left_alone() {
        let prev = empty_service("Wi-Fi");
        let ours = expected_after_set(&prev, 17890, true);
        let mut current = ours.clone();
        current.web = Endpoint { enabled: true, server: "10.0.0.9".into(), port: 3128, authenticated: false };
        current.bypass = vec!["something".into()];
        let plan = restore_plan(&prev, &ours, &current);
        assert_eq!(plan.left_alone, ["Web proxy", "bypass list"]);
        assert!(!plan.commands.iter().any(|c| c[0] == "-setwebproxy"));
        assert!(plan.commands.iter().any(|c| c[0] == "-setsecurewebproxy"));
    }

    #[test]
    fn already_restored_needs_nothing_and_auth_proxies_are_refused() {
        let prev = empty_service("Wi-Fi");
        let ours = expected_after_set(&prev, 1, true);
        assert_eq!(restore_plan(&prev, &ours, &prev), RestorePlan::default());
        let mut auth = empty_service("Wi-Fi");
        auth.socks.authenticated = true;
        assert!(refuse_reason(&auth).is_some());
        assert!(refuse_reason(&prev).is_none());
    }
}
