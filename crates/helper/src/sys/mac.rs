//! macOS: root-owned paths, pf kill switch (ADR-0005, ADR-0006).

use std::path::PathBuf;
use std::process::Command;

use edge_core::helper_proto::{HELPER_DATA, HELPER_SINGBOX};
use edge_core::pf::{self, ServerRule};

use super::pfctl;

pub fn data_dir() -> PathBuf {
    PathBuf::from(HELPER_DATA)
}

pub fn singbox_path() -> PathBuf {
    PathBuf::from(HELPER_SINGBOX)
}

/// Kill a leftover core only if `pid` still runs our installed binary.
pub fn kill_if_ours(pid: u32) -> bool {
    let out = Command::new("/bin/ps").args(["-o", "comm=", "-p", &pid.to_string()]).output();
    let exe = out.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if exe == HELPER_SINGBOX {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        true
    } else {
        false
    }
}

/// Engage the kill switch; returns a token for `ks_release`.
pub fn ks_engage(servers: &[ServerRule], allow_lan: bool) -> Result<String, String> {
    pfctl::engage(&pf::rules(servers, allow_lan))
}

pub fn ks_release(token: Option<&str>) -> Result<(), String> {
    pfctl::release(token)
}

/// Kill-switch rules left behind by a crashed helper.
pub fn ks_leftover() -> bool {
    !pfctl::current_rules().trim().is_empty()
}

/// Windows only (sing-box's TUN firewall rule); nothing to do on macOS.
pub fn remove_tun_firewall_rule() -> Result<bool, String> {
    Ok(false)
}

pub fn core_command() -> Command {
    Command::new(singbox_path())
}
