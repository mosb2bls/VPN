//! Client for the privileged helper (macOS Unix socket). Install / uninstall
//! go through the standard macOS administrator prompt (`osascript … with
//! administrator privileges`), running the helper binary's own install code.

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use edge_core::helper_proto::*;

use crate::api::error::{ApiResult, AppError, ErrorCode};
use crate::api::types::HelperStatus;

pub fn installed() -> bool {
    Path::new(LAUNCHD_PLIST).exists()
}

#[cfg(unix)]
pub fn request(req: &Request, timeout: Duration) -> ApiResult<Response> {
    use std::os::unix::net::UnixStream;
    let mut s = UnixStream::connect(SOCKET_PATH).map_err(|e| {
        let code = if installed() { ErrorCode::HelperUnreachable } else { ErrorCode::HelperNotInstalled };
        AppError::new(code).with_detail(e.to_string())
    })?;
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
    let mut line = serde_json::to_string(req).expect("request serializes");
    line.push('\n');
    s.write_all(line.as_bytes()).map_err(|e| AppError::new(ErrorCode::HelperUnreachable).with_detail(e.to_string()))?;
    let mut out = String::new();
    BufReader::new(&s)
        .read_line(&mut out)
        .map_err(|e| AppError::new(ErrorCode::HelperUnreachable).with_detail(e.to_string()))?;
    serde_json::from_str(out.trim())
        .map_err(|e| AppError::new(ErrorCode::HelperUnreachable).with_detail(format!("bad response: {e}")))
}

#[cfg(not(unix))]
pub fn request(_: &Request, _: Duration) -> ApiResult<Response> {
    Err(AppError::new(ErrorCode::UnsupportedPlatform).with_detail("Windows service: Windows phase"))
}

fn into_state(r: Response) -> ApiResult<HelperState> {
    match r {
        Response::Ok { state } | Response::Restored { state, .. } => Ok(state),
        Response::Error { code, detail } => Err(AppError { detail, ..AppError::new(code) }),
    }
}

pub fn status() -> ApiResult<HelperState> {
    into_state(request(&Request::Status, Duration::from_secs(3))?)
}

pub fn start(profile: TunProfile) -> ApiResult<HelperState> {
    into_state(request(&Request::Start { profile: Box::new(profile) }, Duration::from_secs(30))?)
}

pub fn restore() -> ApiResult<(HelperState, Vec<WireRestoreItem>)> {
    match request(&Request::Restore, Duration::from_secs(30))? {
        Response::Restored { state, items } => Ok((state, items)),
        Response::Ok { state } => Ok((state, vec![])),
        Response::Error { code, detail } => Err(AppError { detail, ..AppError::new(code) }),
    }
}

/// Helper usable for a TUN connect (precondition check).
pub fn ensure_ready() -> ApiResult<HelperState> {
    if !installed() {
        return Err(AppError::new(ErrorCode::HelperNotInstalled));
    }
    let st = status()?;
    if st.protocol != PROTOCOL_VERSION
        || st.version != env!("CARGO_PKG_VERSION")
        || st.core_sha256 != crate::core_bin::expected_sha256()
    {
        return Err(AppError::new(ErrorCode::HelperVersionMismatch)
            .with_detail(format!("helper {} protocol {}", st.version, st.protocol)));
    }
    Ok(st)
}

pub fn dto() -> HelperStatus {
    let expected = env!("CARGO_PKG_VERSION").to_string();
    match (installed(), status()) {
        (_, Ok(st)) => HelperStatus {
            installed: true,
            running: true,
            needs_upgrade: st.version != expected
                || st.protocol != PROTOCOL_VERSION
                || st.core_sha256 != crate::core_bin::expected_sha256(),
            version: Some(st.version),
            expected_version: expected,
        },
        (installed, Err(_)) => HelperStatus { installed, running: false, version: None, expected_version: expected, needs_upgrade: false },
    }
}

/// Helper binary shipped with the app (dev: `target/<profile>/edge-helper`).
#[cfg(target_os = "macos")]
fn bundled_helper() -> ApiResult<PathBuf> {
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let p = dir.join("edge-helper");
    if p.is_file() {
        Ok(p)
    } else {
        Err(AppError::new(ErrorCode::HelperInstallFailed)
            .with_detail("helper binary not found next to the app (dev: run `cargo build -p edge-helper`)"))
    }
}

#[cfg(target_os = "macos")]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Run `argv` as root via the macOS administrator prompt.
#[cfg(target_os = "macos")]
fn run_as_admin(argv: &[String], fail: ErrorCode) -> ApiResult<()> {
    let cmd = argv.iter().map(|a| sh_quote(a)).collect::<Vec<_>>().join(" ");
    let script = format!("do shell script \"{}\" with administrator privileges", applescript_string(&cmd));
    let out = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| AppError::new(fail).with_detail(e.to_string()))?;
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() {
        Ok(())
    } else if err.contains("-128") || err.to_ascii_lowercase().contains("user canceled") {
        Err(AppError::new(ErrorCode::HelperInstallCancelled))
    } else {
        Err(AppError::new(fail).with_detail(err.trim().to_string()))
    }
}

#[cfg(target_os = "macos")]
pub fn install(resource_dir: Option<PathBuf>) -> ApiResult<HelperStatus> {
    let helper = bundled_helper()?;
    let singbox = crate::core_bin::locate_verified()?;
    let rules = crate::rules::bundled_dir(resource_dir);
    let uid = unsafe { libc::getuid() };
    run_as_admin(
        &[
            helper.to_string_lossy().into_owned(),
            "install".into(),
            "--uid".into(),
            uid.to_string(),
            "--sing-box".into(),
            singbox.to_string_lossy().into_owned(),
            "--rules".into(),
            rules.to_string_lossy().into_owned(),
        ],
        ErrorCode::HelperInstallFailed,
    )?;
    for _ in 0..50 {
        if status().is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(dto())
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> ApiResult<HelperStatus> {
    if !installed() {
        return Ok(dto());
    }
    run_as_admin(&[HELPER_BIN.into(), "uninstall".into()], ErrorCode::HelperUninstallFailed)?;
    Ok(dto())
}

#[cfg(not(target_os = "macos"))]
pub fn install(_resource_dir: Option<PathBuf>) -> ApiResult<HelperStatus> {
    Err(AppError::new(ErrorCode::UnsupportedPlatform).with_detail("Windows service: Windows phase"))
}

#[cfg(not(target_os = "macos"))]
pub fn uninstall() -> ApiResult<HelperStatus> {
    Err(AppError::new(ErrorCode::UnsupportedPlatform).with_detail("Windows service: Windows phase"))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn quoting_is_injection_safe() {
        assert_eq!(sh_quote("/Users/a b/x"), "'/Users/a b/x'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        let cmd = sh_quote("/tmp/\"x\"; rm -rf ~");
        assert_eq!(applescript_string(&cmd), "'/tmp/\\\"x\\\"; rm -rf ~'");
    }
}
