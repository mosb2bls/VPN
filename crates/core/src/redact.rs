//! Redaction of credentials (and, in harness mode, endpoints) from free text:
//! sing-box output, logs, diagnostics, error details.
//!
//! Always redacted: every registered secret value (passwords, UUIDs, REALITY
//! keys, short IDs, obfs passwords, Clash API secret), any UUID-shaped token,
//! and any share link (`hy2://…`, `hysteria2://…`, `vless://…`, `vmess://…`,
//! `ss://…`, `trojan://…`). Optionally also server hostnames / IP addresses.

#[derive(Debug, Clone, Default)]
pub struct Redactor {
    /// Exact strings to remove (longest first).
    secrets: Vec<String>,
    /// Hostnames / IPs to remove (harness mode, or always for server names if set).
    endpoints: Vec<String>,
    /// Also replace every IPv4 / IPv6-looking token.
    hide_all_ips: bool,
}

pub const REDACTED: &str = "[redacted]";
pub const HOST_REDACTED: &str = "[host]";

const LINK_SCHEMES: &[&str] = &["hy2://", "hysteria2://", "vless://", "vmess://", "ss://", "trojan://", "tuic://"];

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a secret value. Values shorter than 4 characters are ignored
    /// (too likely to collide with normal text); short IDs of that length are
    /// not meaningful secrets.
    pub fn add_secret(&mut self, s: &str) {
        if s.chars().count() >= 4 && !self.secrets.iter().any(|x| x == s) {
            self.secrets.push(s.to_string());
            self.secrets.sort_by_key(|x| std::cmp::Reverse(x.len()));
        }
    }

    pub fn add_endpoint(&mut self, host: &str) {
        if !host.is_empty() && !self.endpoints.iter().any(|x| x == host) {
            self.endpoints.push(host.to_string());
            self.endpoints.sort_by_key(|x| std::cmp::Reverse(x.len()));
        }
    }

    pub fn hide_all_ips(mut self, on: bool) -> Self {
        self.hide_all_ips = on;
        self
    }

    pub fn redact(&self, text: &str) -> String {
        let mut out = redact_links(text);
        for s in &self.secrets {
            out = out.replace(s.as_str(), REDACTED);
        }
        for h in &self.endpoints {
            out = out.replace(h.as_str(), HOST_REDACTED);
        }
        out = redact_uuids(&out);
        if self.hide_all_ips {
            out = redact_ips(&out);
        }
        out
    }
}

/// Replace whole share links (up to the next whitespace or quote).
fn redact_links(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let hit = LINK_SCHEMES.iter().find(|s| lower[i..].starts_with(*s));
        if let Some(scheme) = hit {
            let end = text[i..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>')
                .map(|e| i + e)
                .unwrap_or(text.len());
            out.push_str(&scheme[..scheme.len() - 3]);
            out.push_str("://");
            out.push_str(REDACTED);
            i = end;
        } else {
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_hex(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Replace 8-4-4-4-12 hex tokens.
fn redact_uuids(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if i + 36 <= text.len() && text.is_char_boundary(i + 36) {
            let cand = &text[i..i + 36];
            let parts: Vec<&str> = cand.split('-').collect();
            let shape = parts.len() == 5
                && parts.iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && is_hex(p));
            let left_ok = i == 0 || !b[i - 1].is_ascii_alphanumeric();
            let right_ok = i + 36 == b.len() || !b[i + 36].is_ascii_alphanumeric();
            if shape && left_ok && right_ok {
                out.push_str(REDACTED);
                i += 36;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Replace IPv4 dotted quads and IPv6-looking tokens (harness mode).
fn redact_ips(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        let t = token.trim_matches(|c| c == '[' || c == ']');
        let core = t.rsplit_once(':').filter(|(h, p)| p.chars().all(|c| c.is_ascii_digit()) && h.contains('.')).map(|(h, _)| h).unwrap_or(t);
        let is_v4 = core.parse::<std::net::Ipv4Addr>().is_ok();
        let is_v6 = t.parse::<std::net::Ipv6Addr>().is_ok()
            || t.rsplit_once(':').is_some_and(|(h, _)| h.trim_matches(|c| c == '[' || c == ']').parse::<std::net::Ipv6Addr>().is_ok());
        if is_v4 || is_v6 {
            out.push_str(HOST_REDACTED);
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_hexdigit() || ch == '.' || ch == ':' || ch == '[' || ch == ']' {
            token.push(ch);
        } else {
            flush(&mut token, &mut out);
            out.push(ch);
        }
    }
    flush(&mut token, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_links_and_uuids() {
        let mut r = Redactor::new();
        r.add_secret("FAKE-hy2-password");
        r.add_secret("abc"); // too short: ignored
        let text = "auth failed for FAKE-hy2-password; link hy2://FAKE-x@h:1/?a=b#n and \
                    VLESS://00000000-0000-4000-8000-0000000000aa@1.2.3.4:443?x=y next \
                    uuid 12345678-1234-1234-1234-123456789abc ok abc";
        let out = r.redact(text);
        assert!(!out.contains("FAKE"), "{out}");
        assert!(!out.contains("12345678-1234"), "{out}");
        assert!(!out.contains("00000000-0000"), "{out}");
        assert!(out.contains("hy2://[redacted]"));
        assert!(out.contains("vless://[redacted] next"));
        assert!(out.ends_with("ok abc"), "short values are not treated as secrets");
    }

    #[test]
    fn endpoints_and_ips_in_harness_mode() {
        let mut r = Redactor::new().hide_all_ips(true);
        r.add_endpoint("proxy.example.com");
        let out = r.redact("dial tcp 203.0.113.7:443 via proxy.example.com, v6 [2001:db8::1]:443, version 1.14.3");
        assert!(!out.contains("203.0.113.7"), "{out}");
        assert!(!out.contains("proxy.example.com"), "{out}");
        assert!(!out.contains("2001:db8"), "{out}");
        assert!(out.contains("version 1.14.3"), "version numbers are not IPs: {out}");
    }

    #[test]
    fn plain_text_unchanged() {
        let r = Redactor::new();
        let s = "INFO inbound/mixed[mixed-in]: tcp server started at 127.0.0.1:17890";
        assert_eq!(r.redact(s), s);
    }
}
