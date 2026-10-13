//! Windows: Program Files / ProgramData locations, process-image checked
//! cleanup, WFP kill switch in a dynamic session (ADR-0007).

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use edge_core::pf::ServerRule;
use edge_winsys::{paths, process, wfp};

/// No console window for the core.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn data_dir() -> PathBuf {
    paths::data_dir()
}

pub fn singbox_path() -> PathBuf {
    paths::singbox_exe()
}

pub fn kill_if_ours(pid: u32) -> bool {
    process::kill_if_image(pid, &paths::singbox_exe())
}

/// The engaged kill switch: its dynamic WFP session lives as long as this.
static KILL_SWITCH: Mutex<Option<wfp::KillSwitch>> = Mutex::new(None);
const TOKEN: &str = "wfp-dynamic-session";

/// Server addresses are not needed: the installed sing-box is permitted by
/// app ID (covers servers and bootstrap resolvers).
pub fn ks_engage(_servers: &[ServerRule], allow_lan: bool) -> Result<String, String> {
    let ks = wfp::engage(&edge_core::wfp::plan(allow_lan), &paths::singbox_exe())?;
    *KILL_SWITCH.lock().unwrap_or_else(|p| p.into_inner()) = Some(ks);
    Ok(TOKEN.into())
}

/// Closing the dynamic session removes every filter it added.
pub fn ks_release(_token: Option<&str>) -> Result<(), String> {
    drop(KILL_SWITCH.lock().unwrap_or_else(|p| p.into_inner()).take());
    Ok(())
}

/// Dynamic-session filters never outlive the helper process.
pub fn ks_leftover() -> bool {
    false
}

/// sing-box's Windows "system" TCP stack adds a persistent inbound-allow rule for
/// its own binary, `sing-tun (<abs path>)` (sing-tun `stack_system_windows.go`,
/// `fixWindowsFirewall`), and never removes it. It is only needed while the TUN
/// runs, so remove it after every session, at recovery and on uninstall
/// (Windows round 3, F-4). Exact name + program match; netsh argv, no shell.
/// Ok(true) = removed, Ok(false) = there was none.
pub fn remove_tun_firewall_rule() -> Result<bool, String> {
    let exe = paths::singbox_exe();
    let name = format!("name=sing-tun ({})", exe.display());
    let program = format!("program={}", exe.display());
    let netsh = std::path::Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\netsh.exe");
    let run = |args: &[&str]| {
        let mut c = Command::new(&netsh);
        c.args(args).creation_flags(CREATE_NO_WINDOW);
        c.output().map_err(|e| e.to_string())
    };
    let shown = run(&["advfirewall", "firewall", "show", "rule", &name])?;
    if !shown.status.success() {
        return Ok(false); // no such rule
    }
    let deleted = run(&["advfirewall", "firewall", "delete", "rule", &name, "dir=in", &program])?;
    if deleted.status.success() {
        Ok(true)
    } else {
        Err(format!("netsh delete rule failed ({})", deleted.status))
    }
}

pub fn core_command() -> Command {
    let mut c = Command::new(singbox_path());
    c.creation_flags(CREATE_NO_WINDOW);
    c
}
