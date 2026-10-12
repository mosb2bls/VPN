//! Unix-socket server. Authentication is twofold: the socket file is owned by
//! the installing user with mode 0600, and every connection's peer uid
//! (getpeereid) must be that user or root. Requests are a closed set of
//! structured operations (`edge_core::helper_proto::Request`); anything else is
//! rejected. One request per connection, max 1 MiB.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use edge_core::ErrorCode;
use edge_core::helper_proto::*;

use crate::session::Session;
use crate::util;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_term(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperConfig {
    pub allowed_uid: u32,
}

pub fn load_config() -> Result<HelperConfig, String> {
    let p = crate::session::data_dir().join("helper.json");
    let b = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_slice(&b).map_err(|e| e.to_string())
}

fn peer_uid(s: &UnixStream) -> Option<u32> {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let r = unsafe { libc::getpeereid(s.as_raw_fd(), &mut uid, &mut gid) };
    (r == 0).then_some(uid)
}

pub fn serve() -> Result<(), String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("serve must run as root (launchd)".into());
    }
    util::ensure_dir(&crate::session::data_dir(), 0o700).map_err(|e| e.to_string())?;
    util::set_log_file(&crate::session::data_dir().join("helper.log"));
    let cfg = load_config()?;
    unsafe {
        libc::signal(libc::SIGTERM, on_term as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_term as *const () as libc::sighandler_t);
    }
    let session = Session::default();
    session.recover_at_start();

    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH).map_err(|e| e.to_string())?;
    std::os::unix::fs::chown(SOCKET_PATH, Some(cfg.allowed_uid), None).map_err(|e| e.to_string())?;
    std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    util::log(&format!("helper {} listening (uid {} allowed)", env!("CARGO_PKG_VERSION"), cfg.allowed_uid));

    while !STOP.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let session = session.clone();
                let allowed = cfg.allowed_uid;
                std::thread::spawn(move || handle(stream, &session, allowed));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => util::log(&format!("accept: {e}")),
        }
    }
    util::log("helper stopping: ending session");
    let _ = session.restore();
    let _ = std::fs::remove_file(SOCKET_PATH);
    Ok(())
}

fn respond(mut s: &UnixStream, r: &Response) {
    if let Ok(mut line) = serde_json::to_string(r) {
        line.push('\n');
        let _ = s.write_all(line.as_bytes());
    }
}

fn handle(stream: UnixStream, session: &Session, allowed_uid: u32) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    match peer_uid(&stream) {
        Some(uid) if uid == allowed_uid || uid == 0 => {}
        other => {
            util::log(&format!("rejected connection from uid {other:?}"));
            respond(&stream, &Response::Error { code: ErrorCode::HelperAuthFailed, detail: None });
            return;
        }
    }
    let mut line = String::new();
    if BufReader::new((&stream).take(1 << 20)).read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let req: Request = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(_) => {
            respond(&stream, &Response::Error { code: ErrorCode::InvalidArgument, detail: Some("unknown request".into()) });
            return;
        }
    };
    let resp = match req {
        Request::Ping | Request::Status => Response::Ok { state: session.state() },
        Request::Start { profile } => match session.start(*profile) {
            Ok(state) => Response::Ok { state },
            Err((code, detail)) => Response::Error { code, detail },
        },
        Request::Stop => match session.stop() {
            Ok(state) => Response::Ok { state },
            Err((code, detail)) => Response::Error { code, detail },
        },
        Request::Restore => {
            let (state, items) = session.restore();
            Response::Restored { state, items }
        }
    };
    respond(&stream, &resp);
}
