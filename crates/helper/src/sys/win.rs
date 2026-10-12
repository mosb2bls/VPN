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

pub fn core_command() -> Command {
    let mut c = Command::new(singbox_path());
    c.creation_flags(CREATE_NO_WINDOW);
    c
}
