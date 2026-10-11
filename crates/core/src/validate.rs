//! Field validators shared by link import and the manual node form. Each
//! returns the normalized value or a field-level [`ErrorCode`]. Validators
//! never put the value itself into an error.

use base64::Engine;

use crate::error_code::ErrorCode;

/// TLS fingerprints accepted by sing-box `utls.fingerprint` (pinned 1.14.x).
/// Source: https://sing-box.sagernet.org/configuration/shared/tls/ (uTLS).
pub const FINGERPRINTS: &[&str] =
    &["chrome", "firefox", "edge", "safari", "360", "qq", "ios", "android", "random", "randomized"];

pub const VISION_FLOW: &str = "xtls-rprx-vision";
pub const NAME_MAX_CHARS: usize = 64;
pub const BANDWIDTH_MAX_MBPS: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Domain(String),
    Ipv4(std::net::Ipv4Addr),
    Ipv6(std::net::Ipv6Addr),
}

impl Host {
    pub fn as_string(&self) -> String {
        match self {
            Host::Domain(d) => d.clone(),
            Host::Ipv4(a) => a.to_string(),
            Host::Ipv6(a) => a.to_string(),
        }
    }
}

/// Classify a host. IPv6 may be given with or without brackets.
pub fn parse_host(raw: &str) -> Result<Host, ErrorCode> {
    let raw = raw.trim();
    let unbracketed = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')).unwrap_or(raw);
    if let Ok(v6) = unbracketed.parse::<std::net::Ipv6Addr>() {
        return Ok(Host::Ipv6(v6));
    }
    if let Ok(v4) = raw.parse::<std::net::Ipv4Addr>() {
        return Ok(Host::Ipv4(v4));
    }
    validate_domain(raw).map(Host::Domain)
}

/// Server host for a node: a domain or IPv4 literal. IPv6 literals are
/// rejected because all proxy traffic is IPv4-only for now (the VPS has no
/// public IPv6).
pub fn validate_server(raw: &str) -> Result<String, ErrorCode> {
    match parse_host(raw)? {
        Host::Ipv6(_) => Err(ErrorCode::FieldInvalidHost),
        h => Ok(h.as_string()),
    }
}

/// ASCII hostname (IDN must be given in `xn--` form), lowercased.
pub fn validate_domain(raw: &str) -> Result<String, ErrorCode> {
    let d = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let ok = !d.is_empty()
        && d.len() <= 253
        && d.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        // A purely numeric dotted name is a malformed IPv4 address, not a domain.
        && !d.split('.').all(|l| l.chars().all(|c| c.is_ascii_digit()));
    if ok { Ok(d) } else { Err(ErrorCode::FieldInvalidHost) }
}

pub fn validate_sni(raw: &str) -> Result<String, ErrorCode> {
    validate_domain(raw).map_err(|_| ErrorCode::FieldInvalidSni)
}

pub fn validate_port(raw: &str) -> Result<u16, ErrorCode> {
    match raw.trim().parse::<u32>() {
        Ok(p) if (1..=65535).contains(&p) => Ok(p as u16),
        _ => Err(ErrorCode::FieldInvalidPort),
    }
}

/// One port or an inclusive range, e.g. `443` or `20000-30000`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

impl PortRange {
    /// Display / storage form: `443` or `20000-30000`.
    pub fn to_display(self) -> String {
        if self.start == self.end { self.start.to_string() } else { format!("{}-{}", self.start, self.end) }
    }
}

/// Comma-separated ports / ranges: `443,20000-30000`.
pub fn parse_port_list(raw: &str) -> Result<Vec<PortRange>, ErrorCode> {
    raw.split(',')
        .map(|part| {
            let part = part.trim();
            match part.split_once('-') {
                None => validate_port(part).map(|p| PortRange { start: p, end: p }),
                Some((a, b)) => {
                    let (a, b) = (validate_port(a), validate_port(b));
                    match (a, b) {
                        (Ok(a), Ok(b)) if a <= b => Ok(PortRange { start: a, end: b }),
                        _ => Err(ErrorCode::FieldInvalidPortRange),
                    }
                }
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| if e == ErrorCode::FieldInvalidPort { ErrorCode::FieldInvalidPortRange } else { e })
}

/// RFC 4122 textual UUID (any version), returned lowercased.
pub fn validate_uuid(raw: &str) -> Result<String, ErrorCode> {
    let u = raw.trim().to_ascii_lowercase();
    let groups: Vec<&str> = u.split('-').collect();
    let ok = groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()));
    if ok { Ok(u) } else { Err(ErrorCode::FieldInvalidUuid) }
}

/// REALITY public key: base64url (no padding) of 32 bytes (X25519).
pub fn validate_public_key(raw: &str) -> Result<String, ErrorCode> {
    let k = raw.trim();
    match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(k) {
        Ok(bytes) if bytes.len() == 32 => Ok(k.to_string()),
        _ => Err(ErrorCode::FieldInvalidPublicKey),
    }
}

/// REALITY short ID: 0–16 hex characters, even length, lowercased.
pub fn validate_short_id(raw: &str) -> Result<String, ErrorCode> {
    let s = raw.trim().to_ascii_lowercase();
    if s.len() <= 16 && s.len().is_multiple_of(2) && s.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(s)
    } else {
        Err(ErrorCode::FieldInvalidShortId)
    }
}

pub fn validate_fingerprint(raw: &str) -> Result<String, ErrorCode> {
    let f = raw.trim();
    if FINGERPRINTS.contains(&f) { Ok(f.to_string()) } else { Err(ErrorCode::FieldInvalidFingerprint) }
}

pub fn validate_bandwidth(raw: &str) -> Result<u32, ErrorCode> {
    match raw.trim().parse::<u32>() {
        Ok(v) if (1..=BANDWIDTH_MAX_MBPS).contains(&v) => Ok(v),
        _ => Err(ErrorCode::FieldInvalidBandwidth),
    }
}

/// Display name for import: control characters become spaces, whitespace is
/// collapsed and trimmed, then truncated to
/// [`NAME_MAX_CHARS`]. (Forms reject over-long names with `FIELD_TOO_LONG` instead.)
pub fn normalize_name(raw: &str) -> String {
    let cleaned: String = raw.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NAME_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(validate_server("Example.COM"), Ok("example.com".into()));
        assert_eq!(validate_server("203.0.113.7"), Ok("203.0.113.7".into()));
        assert_eq!(validate_server("xn--fiqs8s.example"), Ok("xn--fiqs8s.example".into()));
        assert_eq!(validate_server("[2001:db8::1]"), Err(ErrorCode::FieldInvalidHost));
        assert_eq!(validate_server("2001:db8::1"), Err(ErrorCode::FieldInvalidHost));
        assert!(matches!(parse_host("[2001:db8::1]"), Ok(Host::Ipv6(_))));
        for bad in ["", "-a.com", "a..com", "例子.com", "a b.com", "999.1.1.1", "1.2.3", "a_b.com"] {
            assert_eq!(validate_server(bad), Err(ErrorCode::FieldInvalidHost), "{bad}");
        }
    }

    #[test]
    fn ports() {
        assert_eq!(validate_port("443"), Ok(443));
        for bad in ["0", "65536", "-1", "abc", ""] {
            assert_eq!(validate_port(bad), Err(ErrorCode::FieldInvalidPort), "{bad}");
        }
        let r = parse_port_list("443,20000-30000").unwrap();
        assert_eq!(r, vec![PortRange { start: 443, end: 443 }, PortRange { start: 20000, end: 30000 }]);
        assert_eq!(r[1].to_display(), "20000-30000");
        for bad in ["30000-20000", "1-70000", "a-b", "", "443,"] {
            assert_eq!(parse_port_list(bad), Err(ErrorCode::FieldInvalidPortRange), "{bad}");
        }
    }

    #[test]
    fn uuid_key_sid_fp() {
        assert!(validate_uuid("00000000-0000-4000-8000-00000000ABCD").is_ok());
        assert!(validate_uuid("not-a-uuid").is_err());
        assert!(validate_uuid("00000000000040008000000000000000").is_err());
        assert!(validate_public_key(&"A".repeat(43)).is_ok());
        assert!(validate_public_key(&"A".repeat(42)).is_err());
        assert!(validate_public_key("+/+/").is_err());
        assert_eq!(validate_short_id(""), Ok("".into()));
        assert_eq!(validate_short_id("0123456789ABCDEF"), Ok("0123456789abcdef".into()));
        assert!(validate_short_id("abc").is_err());
        assert!(validate_short_id("0123456789abcdef00").is_err());
        assert!(validate_short_id("zz").is_err());
        assert!(validate_fingerprint("chrome").is_ok());
        assert!(validate_fingerprint("chrome_pq").is_err());
    }

    #[test]
    fn names_and_bandwidth() {
        assert_eq!(normalize_name("  UK\tHY2 \n"), "UK HY2");
        assert_eq!(normalize_name(&"x".repeat(100)).chars().count(), NAME_MAX_CHARS);
        assert_eq!(validate_bandwidth("100"), Ok(100));
        assert!(validate_bandwidth("0").is_err());
    }
}
