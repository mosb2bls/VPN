//! Share-link parsing: `hy2://`, `hysteria2://`, `vless://` (REALITY + Vision).
//!
//! Rules:
//! - Unsupported links, parameters or values are **rejected** with a clear
//!   error, never silently ignored. A parameter is accepted only if we can
//!   honour it, or if it is given at its no-op default value.
//! - Errors carry the parameter **name** only, never a value, so they are safe
//!   to log and show (links contain credentials).
//!
//! References (checked 2026-10-11):
//! - Hysteria 2 URI scheme: https://v2.hysteria.network/docs/developers/URI-Scheme/
//! - VLESS share link proposal: https://github.com/XTLS/Xray-core/discussions/716
//! - sing-box TLS / uTLS / REALITY fields: https://sing-box.sagernet.org/configuration/shared/tls/

use std::collections::BTreeMap;

use percent_encoding::percent_decode_str;

use crate::error_code::ErrorCode;
use crate::secret::Secret;
use crate::validate::{self, PortRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hy2Config {
    pub server: String,
    /// First port (used when `server_ports` is empty, and for display).
    pub port: u16,
    /// Port-hopping ports/ranges; empty = single `port`.
    pub server_ports: Vec<PortRange>,
    pub password: Secret,
    pub sni: Option<String>,
    pub insecure: bool,
    /// Salamander obfuscation password, if obfs is enabled.
    pub obfs_password: Option<Secret>,
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlessRealityConfig {
    pub server: String,
    pub port: u16,
    pub uuid: Secret,
    pub sni: String,
    pub fingerprint: String,
    pub public_key: Secret,
    pub short_id: Secret,
    pub spider_x: Option<String>,
    pub alpn: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeConfig {
    Hysteria2(Hy2Config),
    VlessReality(VlessRealityConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLink {
    pub name: String,
    pub config: NodeConfig,
}

/// A link error. `param` is a parameter / component **name** (never a value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkError {
    pub code: ErrorCode,
    pub param: Option<String>,
    /// Extra explanation without any value from the link.
    pub reason: Option<&'static str>,
}

impl LinkError {
    fn new(code: ErrorCode) -> Self {
        Self { code, param: None, reason: None }
    }
    fn param(code: ErrorCode, param: &str) -> Self {
        Self { code, param: Some(param.into()), reason: None }
    }
    fn because(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    /// Redaction-safe one-line detail, e.g. "parameter `pqv`: …".
    pub fn detail(&self) -> String {
        let mut s = self.code.message().to_string();
        if let Some(p) = &self.param {
            s.push_str(&format!(" Parameter: {p}."));
        }
        if let Some(r) = self.reason {
            s.push(' ');
            s.push_str(r);
        }
        s
    }
}

/// Result for one non-blank input line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineResult {
    /// 1-based line number in the input.
    pub line: u32,
    /// Detected scheme (lowercase), if any.
    pub scheme: Option<String>,
    pub result: Result<ParsedLink, LinkError>,
}

/// Parse every non-blank line. Order is preserved.
pub fn parse_links(text: &str) -> Vec<LineResult> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            let l = l.trim();
            LineResult { line: i as u32 + 1, scheme: scheme_of(l), result: parse_link(l) }
        })
        .collect()
}

fn scheme_of(link: &str) -> Option<String> {
    let (s, _) = link.split_once("://")?;
    let s = s.to_ascii_lowercase();
    (!s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)))
        .then_some(s)
}

pub fn parse_link(link: &str) -> Result<ParsedLink, LinkError> {
    let link = link.trim();
    let Some((scheme, rest)) = link.split_once("://") else {
        return Err(LinkError::new(ErrorCode::LinkMalformed).because("Missing scheme (e.g. hy2://)."));
    };
    match scheme.to_ascii_lowercase().as_str() {
        "hy2" | "hysteria2" => parse_hy2(rest),
        "vless" => parse_vless(rest),
        _ => Err(LinkError::new(ErrorCode::LinkUnsupportedScheme)),
    }
}

// ---------------------------------------------------------------------------
// Generic URI pieces
// ---------------------------------------------------------------------------

struct Parts<'a> {
    userinfo: Option<&'a str>,
    hostport: &'a str,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn split_uri(rest: &str) -> Result<Parts<'_>, LinkError> {
    let (rest, fragment) = match rest.split_once('#') {
        Some((a, f)) => (a, Some(f)),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (rest, None),
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    // Userinfo ends at the last '@' (an unencoded '@' in a password is malformed
    // per RFC 3986, but be tolerant of exactly that one case).
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    if hostport.is_empty() {
        return Err(LinkError::param(ErrorCode::LinkMissingParam, "host"));
    }
    Ok(Parts { userinfo, hostport, path, query, fragment })
}

/// Parameter name as shown in errors: only if it looks like a parameter name
/// (letters, `-`, `_`; ≤ 24 chars). Anything else might be a pasted secret.
fn safe_param(k: &str) -> String {
    let looks_like_name = !k.is_empty()
        && k.len() <= 24
        && k.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && k.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_');
    if looks_like_name { k.to_string() } else { "(unrecognised parameter)".into() }
}

fn decode(raw: &str, what: &str) -> Result<String, LinkError> {
    // Every '%' must start a valid escape (percent_decode passes bad ones through).
    let b = raw.as_bytes();
    let bad_escape = b.iter().enumerate().any(|(i, &c)| {
        c == b'%' && !(b.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
            && b.get(i + 2).is_some_and(u8::is_ascii_hexdigit))
    });
    if bad_escape {
        return Err(LinkError::param(ErrorCode::LinkMalformed, &safe_param(what)).because("Invalid percent-encoding."));
    }
    percent_decode_str(raw)
        .decode_utf8()
        .map(|c| c.into_owned())
        .map_err(|_| LinkError::param(ErrorCode::LinkMalformed, &safe_param(what)).because("Invalid percent-encoding."))
}

/// Split `host[:portspec]`, handling `[ipv6]` literals. Returns the decoded host
/// and the raw port spec (if any).
fn split_hostport(hp: &str) -> Result<(String, Option<&str>), LinkError> {
    if let Some(rest) = hp.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err(LinkError::param(ErrorCode::LinkMalformed, "host").because("Unclosed IPv6 bracket."));
        };
        let host = format!("[{}]", &rest[..end]);
        let after = &rest[end + 1..];
        return match after.strip_prefix(':') {
            Some(p) => Ok((host, Some(p))),
            None if after.is_empty() => Ok((host, None)),
            None => Err(LinkError::param(ErrorCode::LinkMalformed, "host")),
        };
    }
    match hp.split_once(':') {
        Some((h, p)) => Ok((decode(h, "host")?, Some(p))),
        None => Ok((decode(hp, "host")?, None)),
    }
}

fn check_server(host: &str) -> Result<String, LinkError> {
    match validate::parse_host(host) {
        Ok(validate::Host::Ipv6(_)) => Err(LinkError::param(ErrorCode::LinkInvalidValue, "host")
            .because("IPv6 server addresses are not supported (IPv4 only).")),
        Ok(h) => Ok(h.as_string()),
        Err(_) => Err(LinkError::param(ErrorCode::LinkInvalidValue, "host")),
    }
}

/// Parse the query into decoded key → value. Duplicate keys are malformed.
fn parse_query(q: Option<&str>) -> Result<BTreeMap<String, String>, LinkError> {
    let mut map = BTreeMap::new();
    for pair in q.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k = decode(k, "query")?;
        let v = decode(v, &k)?;
        if map.insert(k.clone(), v).is_some() {
            return Err(LinkError::param(ErrorCode::LinkMalformed, &safe_param(&k))
                .because("Parameter appears more than once."));
        }
    }
    Ok(map)
}

fn reject_unknown(q: &BTreeMap<String, String>, allowed: &[&str]) -> Result<(), LinkError> {
    match q.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(LinkError::param(ErrorCode::LinkUnsupportedParam, &safe_param(k))),
        None => Ok(()),
    }
}

fn parse_bool(v: &str, param: &str) -> Result<bool, LinkError> {
    match v {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(LinkError::param(ErrorCode::LinkInvalidValue, param)),
    }
}

fn field<T>(r: Result<T, ErrorCode>, param: &str) -> Result<T, LinkError> {
    r.map_err(|_| LinkError::param(ErrorCode::LinkInvalidValue, param))
}

fn name_from(fragment: Option<&str>, default: String) -> Result<String, LinkError> {
    let name = match fragment {
        Some(f) => validate::normalize_name(&decode(f, "name")?),
        None => String::new(),
    };
    Ok(if name.is_empty() { default } else { name })
}

// ---------------------------------------------------------------------------
// Hysteria 2
// ---------------------------------------------------------------------------

const HY2_PARAMS: &[&str] =
    &["sni", "insecure", "obfs", "obfs-password", "pinSHA256", "ech", "upmbps", "downmbps", "mport"];

fn parse_hy2(rest: &str) -> Result<ParsedLink, LinkError> {
    let p = split_uri(rest)?;
    if !(p.path.is_empty() || p.path == "/") {
        return Err(LinkError::param(ErrorCode::LinkMalformed, "path"));
    }
    let password = match p.userinfo {
        Some(u) if !u.is_empty() => decode(u, "auth")?,
        _ => return Err(LinkError::param(ErrorCode::LinkMissingParam, "auth")),
    };
    let (host, portspec) = split_hostport(p.hostport)?;
    let server = check_server(&host)?;

    // Port: optional (default 443); may be a multi-port list for hopping.
    let mut ports = match portspec {
        None | Some("") => vec![PortRange { start: 443, end: 443 }],
        Some(spec) => field(validate::parse_port_list(spec), "port")?,
    };
    let q = parse_query(p.query)?;
    reject_unknown(&q, HY2_PARAMS)?;

    if q.contains_key("pinSHA256") {
        return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "pinSHA256").because(
            "Certificate pinning needs sing-box 1.15 (certificate_sha256); the bundled core does not support it yet.",
        ));
    }
    if q.get("ech").is_some_and(|v| !v.is_empty()) {
        return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "ech"));
    }
    if let Some(m) = q.get("mport") {
        // v2rayN-style hopping ports, in addition to the authority port.
        ports.extend(field(validate::parse_port_list(m), "mport")?);
    }
    let port = ports[0].start;
    let server_ports = if ports.len() == 1 && ports[0].start == ports[0].end { vec![] } else { ports };

    let sni = match q.get("sni").map(String::as_str) {
        None | Some("") => None,
        Some(s) => Some(field(validate::validate_sni(s), "sni")?),
    };
    let insecure = match q.get("insecure") {
        Some(v) => parse_bool(v, "insecure")?,
        None => false,
    };
    let obfs_password = match (q.get("obfs").map(String::as_str), q.get("obfs-password")) {
        (None | Some("") | Some("none"), None) => None,
        (None | Some("") | Some("none"), Some(_)) => {
            return Err(LinkError::param(ErrorCode::LinkMissingParam, "obfs"));
        }
        (Some("salamander"), Some(pw)) if !pw.is_empty() => Some(Secret::new(pw.clone())),
        (Some("salamander"), _) => return Err(LinkError::param(ErrorCode::LinkMissingParam, "obfs-password")),
        (Some(_), _) => {
            return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "obfs")
                .because("Only salamander obfuscation is supported."));
        }
    };
    let bw = |k: &str| -> Result<Option<u32>, LinkError> {
        match q.get(k) {
            None => Ok(None),
            Some(v) => Ok(Some(field(validate::validate_bandwidth(v), k)?)),
        }
    };
    let (up_mbps, down_mbps) = (bw("upmbps")?, bw("downmbps")?);

    let name = name_from(p.fragment, format!("HY2 {server}"))?;
    Ok(ParsedLink {
        name,
        config: NodeConfig::Hysteria2(Hy2Config {
            server,
            port,
            server_ports,
            password: Secret::new(password),
            sni,
            insecure,
            obfs_password,
            up_mbps,
            down_mbps,
        }),
    })
}

// ---------------------------------------------------------------------------
// VLESS + REALITY + Vision
// ---------------------------------------------------------------------------

const VLESS_PARAMS: &[&str] = &[
    "type", "encryption", "security", "flow", "sni", "fp", "pbk", "sid", "spx", "pqv", "alpn",
    "headerType",
];

fn parse_vless(rest: &str) -> Result<ParsedLink, LinkError> {
    let p = split_uri(rest)?;
    if !(p.path.is_empty() || p.path == "/") {
        return Err(LinkError::param(ErrorCode::LinkMalformed, "path"));
    }
    let uuid = match p.userinfo {
        Some(u) if !u.is_empty() => field(validate::validate_uuid(&decode(u, "uuid")?), "uuid")?,
        _ => return Err(LinkError::param(ErrorCode::LinkMissingParam, "uuid")),
    };
    let (host, portspec) = split_hostport(p.hostport)?;
    let server = check_server(&host)?;
    let port = match portspec {
        None | Some("") => return Err(LinkError::param(ErrorCode::LinkMissingParam, "port")),
        Some(s) => field(validate::validate_port(s), "port")?,
    };
    let q = parse_query(p.query)?;
    reject_unknown(&q, VLESS_PARAMS)?;

    let get = |k: &str| q.get(k).map(String::as_str);
    // Fixed values for this client: anything else is rejected.
    match get("type") {
        None | Some("tcp") | Some("raw") => {}
        Some(_) => {
            return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "type")
                .because("Only TCP (raw) transport is supported."));
        }
    }
    match get("headerType") {
        None | Some("") | Some("none") => {}
        Some(_) => return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "headerType")),
    }
    match get("encryption") {
        None | Some("none") => {}
        Some(_) => return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "encryption")),
    }
    match get("security") {
        Some("reality") => {}
        None | Some("") => return Err(LinkError::param(ErrorCode::LinkMissingParam, "security")),
        Some(_) => {
            return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "security")
                .because("Only REALITY is supported."));
        }
    }
    match get("flow") {
        Some(validate::VISION_FLOW) => {}
        None | Some("") => return Err(LinkError::param(ErrorCode::LinkMissingParam, "flow")),
        Some(_) => return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "flow")),
    }
    if get("pqv").is_some_and(|v| !v.is_empty()) {
        return Err(LinkError::param(ErrorCode::LinkUnsupportedParam, "pqv"));
    }

    let sni = match get("sni") {
        Some(s) if !s.is_empty() => field(validate::validate_sni(s), "sni")?,
        // Spec default: reuse the host — only meaningful if it is a domain.
        _ => match validate::validate_domain(&server) {
            Ok(d) => d,
            Err(_) => return Err(LinkError::param(ErrorCode::LinkMissingParam, "sni")),
        },
    };
    let fingerprint = match get("fp") {
        None => "chrome".to_string(), // spec default
        Some(f) => field(validate::validate_fingerprint(f), "fp")?,
    };
    let public_key = match get("pbk") {
        Some(k) if !k.is_empty() => field(validate::validate_public_key(k), "pbk")?,
        _ => return Err(LinkError::param(ErrorCode::LinkMissingParam, "pbk")),
    };
    let short_id = field(validate::validate_short_id(get("sid").unwrap_or("")), "sid")?;
    let spider_x = get("spx").filter(|s| !s.is_empty()).map(str::to_string);
    let alpn = match get("alpn") {
        None => vec![],
        Some(a) => {
            let list: Vec<String> = a.split(',').map(str::to_string).collect();
            if list.iter().any(|x| x.is_empty() || x.contains(' ')) {
                return Err(LinkError::param(ErrorCode::LinkInvalidValue, "alpn"));
            }
            list
        }
    };

    let name = name_from(p.fragment, format!("REALITY {server}"))?;
    Ok(ParsedLink {
        name,
        config: NodeConfig::VlessReality(VlessRealityConfig {
            server,
            port,
            uuid: Secret::new(uuid),
            sni,
            fingerprint,
            public_key: Secret::new(public_key),
            short_id: Secret::new(short_id),
            spider_x,
            alpn,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fake values only.
    const UUID: &str = "00000000-0000-4000-8000-0000000000aa";
    fn pbk() -> String {
        "A".repeat(43)
    }
    fn vless(q: &str) -> String {
        format!("vless://{UUID}@203.0.113.7:443?{q}#REALITY-Example")
    }
    fn good_vless_q() -> String {
        format!("type=raw&security=reality&fp=chrome&sni=www.example.org&pbk={}&sid=0123abcd&flow=xtls-rprx-vision", pbk())
    }
    fn hy2(l: &str) -> Hy2Config {
        match parse_link(l).unwrap().config {
            NodeConfig::Hysteria2(c) => c,
            _ => panic!("not hy2"),
        }
    }
    fn vl(l: &str) -> VlessRealityConfig {
        match parse_link(l).unwrap().config {
            NodeConfig::VlessReality(c) => c,
            _ => panic!("not vless"),
        }
    }
    fn err(l: &str) -> (ErrorCode, Option<String>) {
        let e = parse_link(l).unwrap_err();
        (e.code, e.param)
    }

    // --- Hysteria 2 -------------------------------------------------------

    #[test]
    fn hy2_owner_example_format() {
        // Shape of examples/v2rayn-hysteria2.example.md, fake values.
        let p = parse_link("hysteria2://fake-pass@proxy.example.com:24443/?sni=proxy.example.com&insecure=0#HY2-Example").unwrap();
        assert_eq!(p.name, "HY2-Example");
        let NodeConfig::Hysteria2(c) = p.config else { panic!() };
        assert_eq!(c.server, "proxy.example.com");
        assert_eq!(c.port, 24443);
        assert!(c.server_ports.is_empty());
        assert_eq!(c.password.expose(), "fake-pass");
        assert_eq!(c.sni.as_deref(), Some("proxy.example.com"));
        assert!(!c.insecure);
        assert!(c.obfs_password.is_none());
    }

    #[test]
    fn hy2_scheme_aliases_case_and_default_port() {
        assert_eq!(hy2("hy2://p@example.com").port, 443);
        assert_eq!(hy2("HY2://p@example.com/").port, 443);
        assert_eq!(hy2("Hysteria2://p@example.com:8443").port, 8443);
    }

    #[test]
    fn hy2_url_encoded_password_and_name() {
        let p = parse_link("hy2://p%40ss%3Aw%2Fo%20rd%2B@example.com:443?insecure=1#%E8%8B%B1%E5%9B%BD%20HY2").unwrap();
        assert_eq!(p.name, "英国 HY2");
        let NodeConfig::Hysteria2(c) = p.config else { panic!() };
        assert_eq!(c.password.expose(), "p@ss:w/o rd+");
        assert!(c.insecure);
    }

    #[test]
    fn hy2_userpass_form_kept_whole() {
        assert_eq!(hy2("hy2://user:pass@example.com").password.expose(), "user:pass");
    }

    #[test]
    fn hy2_port_hopping() {
        let c = hy2("hy2://p@example.com:443,20000-30000/");
        assert_eq!(c.port, 443);
        let shown: Vec<_> = c.server_ports.iter().map(|r| r.to_display()).collect();
        assert_eq!(shown, ["443", "20000-30000"]);
        let c = hy2("hy2://p@example.com:443/?mport=20000-30000");
        assert_eq!(c.server_ports.len(), 2);
        assert_eq!(err("hy2://p@example.com:30000-20000"), (ErrorCode::LinkInvalidValue, Some("port".into())));
    }

    #[test]
    fn hy2_obfs() {
        let c = hy2("hy2://p@example.com?obfs=salamander&obfs-password=fake-obfs");
        assert_eq!(c.obfs_password.unwrap().expose(), "fake-obfs");
        assert_eq!(err("hy2://p@example.com?obfs=salamander"), (ErrorCode::LinkMissingParam, Some("obfs-password".into())));
        assert_eq!(err("hy2://p@example.com?obfs-password=x"), (ErrorCode::LinkMissingParam, Some("obfs".into())));
        assert_eq!(err("hy2://p@example.com?obfs=gecko&obfs-password=x"), (ErrorCode::LinkUnsupportedParam, Some("obfs".into())));
    }

    #[test]
    fn hy2_bandwidth_hints() {
        let c = hy2("hy2://p@example.com?upmbps=50&downmbps=200");
        assert_eq!((c.up_mbps, c.down_mbps), (Some(50), Some(200)));
        assert_eq!(err("hy2://p@example.com?upmbps=0"), (ErrorCode::LinkInvalidValue, Some("upmbps".into())));
    }

    #[test]
    fn hy2_rejections() {
        assert_eq!(err("hy2://example.com:443"), (ErrorCode::LinkMissingParam, Some("auth".into())));
        assert_eq!(err("hy2://@example.com:443"), (ErrorCode::LinkMissingParam, Some("auth".into())));
        assert_eq!(err("hy2://p@example.com?pinSHA256=deadbeef").0, ErrorCode::LinkUnsupportedParam);
        assert_eq!(err("hy2://p@example.com?ech=AAAA"), (ErrorCode::LinkUnsupportedParam, Some("ech".into())));
        assert_eq!(err("hy2://p@example.com?foo=bar"), (ErrorCode::LinkUnsupportedParam, Some("foo".into())));
        assert_eq!(err("hy2://p@example.com?insecure=yes"), (ErrorCode::LinkInvalidValue, Some("insecure".into())));
        assert_eq!(err("hy2://p@example.com?sni=a&sni=b").0, ErrorCode::LinkMalformed);
        assert_eq!(err("hy2://p@example.com:0"), (ErrorCode::LinkInvalidValue, Some("port".into())));
        assert_eq!(err("hy2://p@example.com:99999"), (ErrorCode::LinkInvalidValue, Some("port".into())));
        assert_eq!(err("hy2://p@example.com/some/path").0, ErrorCode::LinkMalformed);
        assert_eq!(err("hy2://p%ZZ@example.com").0, ErrorCode::LinkMalformed);
        assert_eq!(err("hy2://p@exa_mple.com"), (ErrorCode::LinkInvalidValue, Some("host".into())));
    }

    #[test]
    fn ipv6_literals_parse_but_are_rejected_ipv4_only() {
        let e = parse_link("hy2://p@[2001:db8::1]:443/").unwrap_err();
        assert_eq!((e.code, e.param.as_deref()), (ErrorCode::LinkInvalidValue, Some("host")));
        assert!(e.detail().contains("IPv6"));
        assert_eq!(err(&format!("vless://{UUID}@[2001:db8::1]:443?{}", good_vless_q())).0, ErrorCode::LinkInvalidValue);
        // Bracket handling itself.
        assert_eq!(err("hy2://p@[2001:db8::1:443").0, ErrorCode::LinkMalformed);
        assert_eq!(err("hy2://p@[::1]x").0, ErrorCode::LinkMalformed);
    }

    // --- VLESS ------------------------------------------------------------

    #[test]
    fn vless_owner_example_format() {
        // Shape of examples/v2rayn-reality-vision.example.md, fake values.
        let p = parse_link(&vless(&good_vless_q())).unwrap();
        assert_eq!(p.name, "REALITY-Example");
        let NodeConfig::VlessReality(c) = p.config else { panic!() };
        assert_eq!(c.server, "203.0.113.7");
        assert_eq!(c.port, 443);
        assert_eq!(c.uuid.expose(), UUID);
        assert_eq!(c.sni, "www.example.org");
        assert_eq!(c.fingerprint, "chrome");
        assert_eq!(c.public_key.expose(), pbk());
        assert_eq!(c.short_id.expose(), "0123abcd");
        assert!(c.spider_x.is_none());
    }

    #[test]
    fn vless_accepts_tcp_alias_defaults_and_noop_params() {
        let q = format!("type=tcp&headerType=none&encryption=none&security=reality&sni=www.example.org&pbk={}&flow=xtls-rprx-vision&spx=%2F&pqv=", pbk());
        let c = vl(&vless(&q));
        assert_eq!(c.fingerprint, "chrome"); // default when fp omitted
        assert_eq!(c.short_id.expose(), ""); // sid may be empty
        assert_eq!(c.spider_x.as_deref(), Some("/"));
    }

    #[test]
    fn vless_sni_defaults_to_domain_host_only() {
        let q = format!("security=reality&pbk={}&flow=xtls-rprx-vision", pbk());
        assert_eq!(vl(&format!("vless://{UUID}@www.example.org:443?{q}")).sni, "www.example.org");
        assert_eq!(err(&format!("vless://{UUID}@203.0.113.7:443?{q}")), (ErrorCode::LinkMissingParam, Some("sni".into())));
    }

    #[test]
    fn vless_rejections() {
        let base = good_vless_q();
        let with = |extra: &str| vless(&format!("{base}&{extra}"));
        let replace = |from: &str, to: &str| vless(&base.replace(from, to));
        assert_eq!(err(&replace("security=reality", "security=tls")), (ErrorCode::LinkUnsupportedParam, Some("security".into())));
        assert_eq!(err(&replace("type=raw", "type=ws")), (ErrorCode::LinkUnsupportedParam, Some("type".into())));
        assert_eq!(err(&replace("flow=xtls-rprx-vision", "flow=xtls-rprx-direct")), (ErrorCode::LinkUnsupportedParam, Some("flow".into())));
        assert_eq!(err(&replace("&flow=xtls-rprx-vision", "")), (ErrorCode::LinkMissingParam, Some("flow".into())));
        assert_eq!(err(&replace("fp=chrome", "fp=chrome_pq")), (ErrorCode::LinkInvalidValue, Some("fp".into())));
        assert_eq!(err(&replace(&format!("pbk={}", pbk()), "pbk=short")), (ErrorCode::LinkInvalidValue, Some("pbk".into())));
        assert_eq!(err(&replace(&format!("&pbk={}", pbk()), "")), (ErrorCode::LinkMissingParam, Some("pbk".into())));
        assert_eq!(err(&replace("sid=0123abcd", "sid=xyz")), (ErrorCode::LinkInvalidValue, Some("sid".into())));
        assert_eq!(err(&with("encryption=mlkem768x25519plus")), (ErrorCode::LinkUnsupportedParam, Some("encryption".into())));
        assert_eq!(err(&with("pqv=AAAA")), (ErrorCode::LinkUnsupportedParam, Some("pqv".into())));
        assert_eq!(err(&with("headerType=http")), (ErrorCode::LinkUnsupportedParam, Some("headerType".into())));
        assert_eq!(err(&with("packetEncoding=xudp")), (ErrorCode::LinkUnsupportedParam, Some("packetEncoding".into())));
        assert_eq!(err(&format!("vless://not-a-uuid@203.0.113.7:443?{base}")), (ErrorCode::LinkInvalidValue, Some("uuid".into())));
        assert_eq!(err(&format!("vless://{UUID}@203.0.113.7?{base}")), (ErrorCode::LinkMissingParam, Some("port".into())));
        assert_eq!(err(&format!("vless://203.0.113.7:443?{base}")), (ErrorCode::LinkMissingParam, Some("uuid".into())));
    }

    // --- General ----------------------------------------------------------

    #[test]
    fn unsupported_and_malformed() {
        assert_eq!(err("vmess://eyJhZGQiOiJ4In0=").0, ErrorCode::LinkUnsupportedScheme);
        assert_eq!(err("ss://YWVzOnB3@example.com:8388").0, ErrorCode::LinkUnsupportedScheme);
        assert_eq!(err("just some text").0, ErrorCode::LinkMalformed);
        assert_eq!(err("hy2://").0, ErrorCode::LinkMissingParam);
    }

    #[test]
    fn errors_never_contain_secret_values() {
        let secret = "VERY-SECRET-VALUE-123";
        let links = [
            format!("hy2://{secret}@example.com?pinSHA256=x"),
            format!("hy2://{secret}@example.com?insecure=maybe"),
            format!("hy2://{secret}@example.com:{secret}"),
            format!("vless://{secret}@203.0.113.7:443?{}", good_vless_q()),
            format!("vless://{UUID}@203.0.113.7:443?{}&{secret}", good_vless_q()),
            format!("hy2://p@example.com?{secret}=1"),
            format!("hy2://p@example.com?sni=a%ZZ{secret}"),
            format!("vless://{UUID}@203.0.113.7:443?{}", good_vless_q().replace(&pbk(), secret)),
        ];
        for l in links {
            let e = parse_link(&l).unwrap_err();
            let shown = format!("{:?} {}", e, e.detail());
            assert!(!shown.contains(secret), "leaked in: {shown}");
        }
        // Debug of a parsed config does not leak either.
        let ok = parse_link(&format!("hy2://{secret}@example.com")).unwrap();
        assert!(!format!("{ok:?}").contains(secret));
    }

    #[test]
    fn multiple_lines_with_numbers_blanks_and_crlf() {
        let text = format!(
            "\r\n  hy2://p@example.com:24443#A  \r\n\r\nvmess://x\r\n{}\nnot a link\n",
            vless(&good_vless_q())
        );
        let r = parse_links(&text);
        assert_eq!(r.len(), 4);
        assert_eq!((r[0].line, r[0].scheme.as_deref(), r[0].result.is_ok()), (2, Some("hy2"), true));
        assert_eq!(r[0].result.as_ref().unwrap().name, "A");
        assert_eq!((r[1].line, r[1].scheme.as_deref()), (4, Some("vmess")));
        assert_eq!(r[1].result.as_ref().unwrap_err().code, ErrorCode::LinkUnsupportedScheme);
        assert_eq!((r[2].line, r[2].result.is_ok()), (5, true));
        assert_eq!((r[3].line, r[3].scheme.as_deref()), (6, None));
        assert!(parse_links(" \n\t\n").is_empty());
    }

    #[test]
    fn default_names() {
        assert_eq!(parse_link("hy2://p@example.com").unwrap().name, "HY2 example.com");
        assert_eq!(parse_link("hy2://p@example.com#%20%20").unwrap().name, "HY2 example.com");
    }
}
