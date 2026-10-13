//! One-time install / uninstall on Windows (run elevated through UAC).
//!
//! install: verify the bundled sing-box against the hash embedded in this
//! helper; stop and remove an older service; copy helper + sing-box into
//! `%ProgramFiles%\<App>\Helper` (admin-writable only) and the rules into
//! `%ProgramData%\<App>\Helper\rules` (protected DACL: SYSTEM +
//! Administrators); record the allowed user SID; create and start the service.

use std::path::Path;
use std::time::Duration;

use edge_core::helper_proto::*;
use edge_winsys::{paths, security, service};

use crate::{session, util};

/// Copy, retrying briefly: a just-stopped service may still hold its image.
fn copy_retry(src: &Path, dst: &Path) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..20 {
        match std::fs::copy(src, dst) {
            Ok(_) => return Ok(()),
            Err(e) => last = format!("{} → {}: {e}", src.display(), dst.display()),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(last)
}

pub fn install(sid: &str, singbox: &Path, rules_dir: &Path) -> Result<(), String> {
    if !edge_winsys::valid_sid(sid) {
        return Err("invalid --sid".into());
    }
    if session::EXPECTED_SINGBOX_SHA256.is_empty() {
        return Err("helper was built without the core hash (run npm run fetch-core, rebuild)".into());
    }
    if util::sha256_file(singbox).map_err(|e| e.to_string())? != session::EXPECTED_SINGBOX_SHA256 {
        return Err("sing-box hash mismatch; refusing to install".into());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    if paths::same_path(&me, &paths::helper_exe()) {
        return Err("run install from the app's bundled helper, not the installed copy".into());
    }
    // An older service: stopping it restores its session (see serve_loop).
    service::remove(WIN_SERVICE_NAME)?;
    let inst = paths::install_dir();
    std::fs::create_dir_all(&inst).map_err(|e| format!("{}: {e}", inst.display()))?;
    copy_retry(&me, &paths::helper_exe())?;
    copy_retry(singbox, &paths::singbox_exe())?;
    let data = session::data_dir();
    std::fs::create_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
    security::protect_dir(&data)?;
    std::fs::create_dir_all(data.join("rules")).map_err(|e| e.to_string())?;
    for name in ["geosite-cn.srs", "geoip-cn.srs"] {
        copy_retry(&rules_dir.join(name), &data.join("rules").join(name))?;
    }
    session::verified_rules().map_err(|(c, d)| format!("rules: {c:?} {}", d.unwrap_or_default()))?;
    let cfg = serde_json::json!({ "allowedSid": sid });
    util::write_private(&data.join("helper.json"), cfg.to_string().as_bytes()).map_err(|e| e.to_string())?;
    let cmd = paths::helper_service_command();
    service::install(
        WIN_SERVICE_NAME,
        WIN_SERVICE_DISPLAY,
        &format!("Runs sing-box in TUN mode for {}. Removed by uninstalling the helper in the app.", edge_core::APP_NAME),
        &cmd,
    )?;
    println!("installed");
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    // Stopping the service ends its session (core, kill switch).
    service::remove(WIN_SERVICE_NAME)?;
    // A journal left by a crashed service: clean up here as well.
    if session::load_journal().is_some() {
        let _ = session::Session::default().restore();
    }
    if let Err(e) = crate::sys::remove_tun_firewall_rule() {
        eprintln!("TUN firewall rule not removed: {e}");
    }
    let _ = std::fs::remove_dir_all(session::data_dir());
    let _ = std::fs::remove_file(paths::singbox_exe());
    let _ = std::fs::remove_file(paths::helper_exe());
    let _ = std::fs::remove_dir(paths::install_dir());
    println!("uninstalled");
    Ok(())
}
