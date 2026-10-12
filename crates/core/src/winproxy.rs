//! Windows system proxy (per-user WinINet "LAN settings"): what we set and
//! when a restore is allowed. Pure — the Windows executor (crates/winsys)
//! reads/writes it with `InternetQueryOptionW` / `InternetSetOptionW`
//! (`INTERNET_OPTION_PER_CONNECTION_OPTION`) and then broadcasts
//! `INTERNET_OPTION_SETTINGS_CHANGED` + `INTERNET_OPTION_REFRESH`.
//!
//! Restore rule as on macOS: put the previous settings back only if the
//! current ones are still exactly ours; otherwise the user changed them while
//! connected and they are left alone.

use serde::{Deserialize, Serialize};

/// `INTERNET_PER_CONN_FLAGS` bits.
pub const PROXY_TYPE_DIRECT: u32 = 0x1;
pub const PROXY_TYPE_PROXY: u32 = 0x2;
pub const PROXY_TYPE_AUTO_PROXY_URL: u32 = 0x4;
pub const PROXY_TYPE_AUTO_DETECT: u32 = 0x8;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WinProxyState {
    /// `INTERNET_PER_CONN_FLAGS`.
    pub flags: u32,
    /// `INTERNET_PER_CONN_PROXY_SERVER`, e.g. `127.0.0.1:17890`.
    pub server: String,
    /// `INTERNET_PER_CONN_PROXY_BYPASS`, `;`-separated, `<local>` = plain hostnames.
    pub bypass: String,
    /// `INTERNET_PER_CONN_AUTOCONFIG_URL` (PAC).
    pub pac_url: String,
}

/// What we set: manual proxy on 127.0.0.1:`port` for all protocols, PAC and
/// auto-detect off (they would take precedence), local / LAN bypassed.
pub fn ours(prev: &WinProxyState, port: u16, allow_lan: bool) -> WinProxyState {
    let mut bypass = vec!["localhost", "127.*", "<local>"];
    if allow_lan {
        // 172.16.0.0/12 spelled out: a "172.2*" wildcard would also match
        // public 172.2.x.x / 172.200-255.x.x and send them direct.
        bypass.extend([
            "10.*", "172.16.*", "172.17.*", "172.18.*", "172.19.*", "172.20.*", "172.21.*", "172.22.*", "172.23.*",
            "172.24.*", "172.25.*", "172.26.*", "172.27.*", "172.28.*", "172.29.*", "172.30.*", "172.31.*",
            "192.168.*", "169.254.*",
        ]);
    }
    WinProxyState {
        flags: PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY,
        server: format!("127.0.0.1:{port}"),
        bypass: bypass.join(";"),
        // Keep the URL so the record is self-contained; the flag decides.
        pac_url: prev.pac_url.clone(),
    }
}

/// Settings that refuse a takeover: none today (WinINet proxy auth lives
/// elsewhere), kept for symmetry with macOS.
pub fn refuse_reason(_prev: &WinProxyState) -> Option<String> {
    None
}

/// Restore if the manual proxy still points at our server — even if other
/// fields drifted (Windows can turn "Automatically detect settings" back on by
/// itself, and may normalise strings); leaving a dead local proxy behind would
/// break networking. Leave alone if the user pointed it elsewhere / turned it off.
pub fn restore_target(prev: &WinProxyState, ours: &WinProxyState, current: &WinProxyState) -> RestoreDecision {
    let still_ours = current.flags & PROXY_TYPE_PROXY != 0 && current.server.trim().eq_ignore_ascii_case(ours.server.trim());
    if current == prev {
        RestoreDecision::AlreadyRestored
    } else if still_ours {
        RestoreDecision::Restore(prev.clone())
    } else {
        RestoreDecision::ChangedByUser
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreDecision {
    AlreadyRestored,
    Restore(WinProxyState),
    ChangedByUser,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_restore_decisions() {
        let prev = WinProxyState { flags: PROXY_TYPE_DIRECT | PROXY_TYPE_AUTO_DETECT, ..Default::default() };
        let o = ours(&prev, 17890, true);
        assert_eq!(o.flags, PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY);
        assert_eq!(o.server, "127.0.0.1:17890");
        assert!(o.bypass.contains("<local>") && o.bypass.contains("192.168.*"));
        assert!(!ours(&prev, 1, false).bypass.contains("192.168.*"));
        // No wildcard that matches public 172.x addresses.
        assert!(o.bypass.split(';').all(|e| e != "172.2*"));
        assert!(o.bypass.contains("172.25.*"));
        assert_eq!(restore_target(&prev, &o, &o), RestoreDecision::Restore(prev.clone()));
        assert_eq!(restore_target(&prev, &o, &prev), RestoreDecision::AlreadyRestored);
        let user = WinProxyState { flags: PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY, server: "10.0.0.9:3128".into(), ..Default::default() };
        assert_eq!(restore_target(&prev, &o, &user), RestoreDecision::ChangedByUser);
        // Windows re-enabled auto-detect on its own: still ours → restore.
        let drifted = WinProxyState { flags: o.flags | PROXY_TYPE_AUTO_DETECT, ..o.clone() };
        assert_eq!(restore_target(&prev, &o, &drifted), RestoreDecision::Restore(prev.clone()));
        // User turned the manual proxy off (server text kept): not ours.
        let off = WinProxyState { flags: PROXY_TYPE_DIRECT, ..o.clone() };
        assert_eq!(restore_target(&prev, &o, &off), RestoreDecision::ChangedByUser);
    }

    #[test]
    fn pac_flag_off_while_connected_url_kept() {
        let prev = WinProxyState { flags: PROXY_TYPE_DIRECT | PROXY_TYPE_AUTO_PROXY_URL, pac_url: "http://wpad/p.pac".into(), ..Default::default() };
        let o = ours(&prev, 1, true);
        assert_eq!(o.flags & PROXY_TYPE_AUTO_PROXY_URL, 0);
        assert_eq!(o.pac_url, prev.pac_url);
    }
}
