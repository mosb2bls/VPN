//! One-time install / uninstall (run as root via the macOS admin prompt).
//!
//! install: verify the bundled sing-box against the hash embedded in this
//! helper and the rule-sets against the embedded manifest, copy helper +
//! sing-box into /Library/PrivilegedHelperTools (root:wheel 0755) and the rules
//! into the helper data dir (root 0700), record the allowed uid, write the
//! LaunchDaemon plist and bootstrap it.

use std::path::Path;
use std::process::Command;

use edge_core::helper_proto::*;

use crate::{session, util};

fn root() -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 { Ok(()) } else { Err("must run as root".into()) }
}

fn copy_root(src: &Path, dst: &str, mode: u32) -> Result<(), String> {
    let data = std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
    util::write_atomic(Path::new(dst), &data, mode).map_err(|e| format!("{dst}: {e}"))?;
    std::os::unix::fs::chown(dst, Some(0), Some(0)).map_err(|e| format!("chown {dst}: {e}"))
}

fn plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key><array><string>{HELPER_BIN}</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
    )
}

pub fn install(uid: u32, singbox: &Path, rules_dir: &Path) -> Result<(), String> {
    root()?;
    if session::EXPECTED_SINGBOX_SHA256.is_empty() {
        return Err("helper was built without the core hash (run npm run fetch-core, rebuild)".into());
    }
    let h = util::sha256_file(singbox).map_err(|e| format!("reading {}: {e}", singbox.display()))?;
    if h != session::EXPECTED_SINGBOX_SHA256 {
        return Err("sing-box hash mismatch; refusing to install".into());
    }
    let me = std::env::current_exe().map_err(|e| format!("locating the helper binary: {e}"))?;
    // Stop an existing instance first (ignore errors: may not be loaded).
    let _ = Command::new("/bin/launchctl").args(["bootout", &format!("system/{LAUNCHD_LABEL}")]).status();
    util::ensure_dir(Path::new(INSTALL_DIR), 0o755).map_err(|e| format!("{INSTALL_DIR}: {e}"))?;
    copy_root(&me, HELPER_BIN, 0o755)?;
    copy_root(singbox, HELPER_SINGBOX, 0o755)?;
    let data = session::data_dir();
    util::ensure_dir(&data, 0o700).map_err(|e| format!("{}: {e}", data.display()))?;
    util::ensure_dir(&data.join("rules"), 0o700).map_err(|e| format!("{}/rules: {e}", data.display()))?;
    for name in ["geosite-cn.srs", "geoip-cn.srs"] {
        copy_root(&rules_dir.join(name), &data.join("rules").join(name).to_string_lossy(), 0o644)?;
    }
    session::verified_rules().map_err(|(c, d)| format!("rules: {c:?} {}", d.unwrap_or_default()))?;
    let cfg = serde_json::json!({ "allowedUid": uid });
    util::write_atomic(&data.join("helper.json"), cfg.to_string().as_bytes(), 0o600).map_err(|e| format!("helper.json: {e}"))?;
    util::write_atomic(Path::new(LAUNCHD_PLIST), plist().as_bytes(), 0o644).map_err(|e| format!("{LAUNCHD_PLIST}: {e}"))?;
    std::os::unix::fs::chown(LAUNCHD_PLIST, Some(0), Some(0)).map_err(|e| format!("chown {LAUNCHD_PLIST}: {e}"))?;
    let st = Command::new("/bin/launchctl").args(["bootstrap", "system", LAUNCHD_PLIST]).status().map_err(|e| format!("launchctl: {e}"))?;
    if !st.success() {
        return Err(format!("launchctl bootstrap failed ({st})"));
    }
    println!("installed");
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    root()?;
    // End any session and remove the kill switch before stopping the daemon.
    let s = session::Session::default();
    let _ = s.restore();
    let _ = Command::new("/bin/launchctl").args(["bootout", &format!("system/{LAUNCHD_LABEL}")]).status();
    for p in [LAUNCHD_PLIST, HELPER_SINGBOX, SOCKET_PATH] {
        let _ = std::fs::remove_file(p);
    }
    let _ = std::fs::remove_dir_all(session::data_dir());
    let _ = std::fs::remove_file(HELPER_BIN);
    println!("uninstalled");
    Ok(())
}
