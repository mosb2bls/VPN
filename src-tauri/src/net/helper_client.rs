//! Client for the privileged helper: macOS Unix socket / Windows named pipe.
//! Install / uninstall run the helper binary's own install code through the
//! OS elevation prompt (macOS `osascript … with administrator privileges`,
//! Windows UAC `runas`). ADR-0005, ADR-0007.

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use edge_core::helper_proto::*;

use crate::api::error::{ApiResult, AppError, ErrorCode};
use crate::api::types::HelperStatus;

pub fn installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        Path::new(LAUNCHD_PLIST).exists()
    }
    #[cfg(windows)]
    {
        edge_winsys::service::is_installed(WIN_SERVICE_NAME)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        false
    }
}

fn unreachable(detail: String) -> AppError {
    let code = if installed() { ErrorCode::HelperUnreachable } else { ErrorCode::HelperNotInstalled };
    AppError::new(code).with_detail(detail)
}

#[cfg(unix)]
pub fn request(req: &Request, timeout: Duration) -> ApiResult<Response> {
    use std::os::unix::net::UnixStream;
    let mut s = UnixStream::connect(SOCKET_PATH).map_err(|e| unreachable(e.to_string()))?;
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

#[cfg(windows)]
pub fn request(req: &Request, timeout: Duration) -> ApiResult<Response> {
    use std::io::{BufRead, BufReader, Write};
    let f = edge_winsys::pipe::connect(WIN_PIPE_NAME, Duration::from_secs(3)).map_err(|e| unreachable(e.to_string()))?;
    // Talk only to the installed service binary, never to a process that
    // grabbed the pipe name first.
    let server = edge_winsys::pipe::server_pid(&f).and_then(edge_winsys::process::image_path);
    if !server.as_deref().is_some_and(|p| edge_winsys::paths::same_path(p, &edge_winsys::paths::helper_exe())) {
        return Err(AppError::new(ErrorCode::HelperAuthFailed).with_detail("pipe server is not the installed helper"));
    }
    let mut line = serde_json::to_string(req).expect("request serializes");
    line.push('\n');
    // Synchronous pipes have no read timeout: do the exchange on a thread.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = (|| -> std::io::Result<String> {
            (&f).write_all(line.as_bytes())?;
            let mut out = String::new();
            BufReader::new(&f).read_line(&mut out)?;
            Ok(out)
        })();
        let _ = tx.send(r);
    });
    let out = rx
        .recv_timeout(timeout)
        .map_err(|_| AppError::new(ErrorCode::HelperUnreachable).with_detail("helper did not answer in time"))?
        .map_err(|e| AppError::new(ErrorCode::HelperUnreachable).with_detail(e.to_string()))?;
    serde_json::from_str(out.trim())
        .map_err(|e| AppError::new(ErrorCode::HelperUnreachable).with_detail(format!("bad response: {e}")))
}

#[cfg(not(any(unix, windows)))]
pub fn request(_: &Request, _: Duration) -> ApiResult<Response> {
    Err(AppError::new(ErrorCode::UnsupportedPlatform))
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
#[cfg(any(target_os = "macos", windows))]
fn bundled_helper() -> ApiResult<PathBuf> {
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let p = dir.join(if cfg!(windows) { "edge-helper.exe" } else { "edge-helper" });
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

/// Run `argv` as root via the macOS administrator prompt. `cd /` first: the
/// root shell inherits our working directory, which may be in a privacy-
/// protected folder (Desktop / Documents / Downloads) it cannot access.
#[cfg(target_os = "macos")]
fn run_as_admin(argv: &[String], fail: ErrorCode) -> ApiResult<()> {
    let cmd = format!("cd / && {}", argv.iter().map(|a| sh_quote(a)).collect::<Vec<_>>().join(" "));
    let script = format!(
        "do shell script \"{}\" with prompt \"{} wants to install or remove its network helper.\" with administrator privileges",
        applescript_string(&cmd),
        applescript_string(edge_core::APP_NAME)
    );
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

/// Copy what the root installer reads into a private temp folder (0700).
/// macOS privacy protection (TCC) blocks the root process behind the admin
/// prompt from reading files in Desktop / Documents / Downloads — where a dev
/// checkout or a downloaded app may live. The installer still verifies
/// sing-box and the rule-sets against the hashes embedded in the helper.
#[cfg(target_os = "macos")]
fn stage_for_install(helper: &Path, singbox: &Path, rules: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{}-helper-install-{}-{nonce}", edge_core::APP_SLUG, std::process::id()));
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    std::fs::DirBuilder::new().mode(0o700).create(dir.join("rules"))?;
    for (src, name) in [(helper, "edge-helper"), (singbox, "sing-box")] {
        std::fs::copy(src, dir.join(name))?;
        std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o700))?;
    }
    for name in ["geosite-cn.srs", "geoip-cn.srs"] {
        std::fs::copy(rules.join(name), dir.join("rules").join(name))?;
    }
    Ok(dir)
}

#[cfg(target_os = "macos")]
pub fn install(resource_dir: Option<PathBuf>) -> ApiResult<HelperStatus> {
    let helper = bundled_helper()?;
    let singbox = crate::core_bin::locate_verified()?;
    let rules = crate::rules::bundled_dir(resource_dir);
    let uid = unsafe { libc::getuid() };
    let stage = stage_for_install(&helper, &singbox, &rules)
        .map_err(|e| AppError::new(ErrorCode::HelperInstallFailed).with_detail(format!("staging the helper files: {e}")))?;
    let result = run_as_admin(
        &[
            stage.join("edge-helper").to_string_lossy().into_owned(),
            "install".into(),
            "--uid".into(),
            uid.to_string(),
            "--sing-box".into(),
            stage.join("sing-box").to_string_lossy().into_owned(),
            "--rules".into(),
            stage.join("rules").to_string_lossy().into_owned(),
        ],
        ErrorCode::HelperInstallFailed,
    );
    let _ = std::fs::remove_dir_all(&stage);
    result?;
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

/// Run the bundled helper elevated through UAC. Its error text comes back
/// through a `--report` file (an elevated process has no console we can read).
#[cfg(windows)]
fn run_elevated(args: &[String], fail: ErrorCode) -> ApiResult<()> {
    use edge_winsys::elevate::{ElevateError, run_elevated};
    let helper = bundled_helper()?;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let report = std::env::temp_dir().join(format!("edge-helper-{}-{nonce}.txt", std::process::id()));
    let mut argv = args.to_vec();
    argv.extend(["--report".into(), report.to_string_lossy().into_owned()]);
    let r = run_elevated(&helper, &argv);
    let text = std::fs::read_to_string(&report).unwrap_or_default();
    let _ = std::fs::remove_file(&report);
    match r {
        Ok(0) => Ok(()),
        Ok(code) => Err(AppError::new(fail).with_detail(if text.trim().is_empty() { format!("exit code {code}") } else { text.trim().to_string() })),
        Err(ElevateError::Cancelled) => Err(AppError::new(ErrorCode::HelperInstallCancelled)),
        Err(ElevateError::Failed(e)) => Err(AppError::new(fail).with_detail(e)),
    }
}

#[cfg(windows)]
pub fn install(resource_dir: Option<PathBuf>) -> ApiResult<HelperStatus> {
    let singbox = crate::core_bin::locate_verified()?;
    let rules = crate::rules::bundled_dir(resource_dir);
    let sid = edge_winsys::security::current_user_sid().map_err(|e| AppError::new(ErrorCode::HelperInstallFailed).with_detail(e))?;
    run_elevated(
        &[
            "install".into(),
            "--sid".into(),
            sid,
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

#[cfg(windows)]
pub fn uninstall() -> ApiResult<HelperStatus> {
    if !installed() {
        return Ok(dto());
    }
    run_elevated(&["uninstall".into()], ErrorCode::HelperUninstallFailed)?;
    Ok(dto())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn install(_resource_dir: Option<PathBuf>) -> ApiResult<HelperStatus> {
    Err(AppError::new(ErrorCode::UnsupportedPlatform))
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn uninstall() -> ApiResult<HelperStatus> {
    Err(AppError::new(ErrorCode::UnsupportedPlatform))
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
