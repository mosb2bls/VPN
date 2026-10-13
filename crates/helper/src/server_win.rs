//! Windows service + named-pipe server (ADR-0007).
//!
//! Authentication is twofold: the pipe's DACL grants only SYSTEM and the
//! installing user's SID (read + write data, not create-instance), and every
//! connection's client process must run as that user or as SYSTEM.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use edge_core::ErrorCode;
use edge_core::helper_proto::*;
use edge_winsys::{pipe, security};

use crate::session::Session;
use crate::{rpc, util};

static STOP: AtomicBool = AtomicBool::new(false);

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperConfig {
    pub allowed_sid: String,
}

pub fn load_config() -> Result<HelperConfig, String> {
    let p = crate::session::data_dir().join("helper.json");
    let b = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let c: HelperConfig = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
    if !edge_winsys::valid_sid(&c.allowed_sid) {
        return Err("helper.json: invalid SID".into());
    }
    Ok(c)
}

/// Pipe DACL: SYSTEM full; the user FILE_GENERIC_READ | FILE_WRITE_DATA
/// (0x0012008B) — deliberately without FILE_APPEND_DATA, which on a pipe is
/// FILE_CREATE_PIPE_INSTANCE.
pub fn pipe_sddl(user_sid: &str) -> String {
    format!("D:P(A;;GA;;;SY)(A;;0x{:08x};;;{user_sid})", edge_winsys::PIPE_USER_GRANT)
}

/// `edge-helper serve`: run under the Service Control Manager.
pub fn serve() -> Result<(), String> {
    edge_winsys::service::run(WIN_SERVICE_NAME, service_main, on_stop)
}

fn on_stop() {
    STOP.store(true, Ordering::SeqCst);
    // Unblock the accept loop.
    let _ = pipe::connect(WIN_PIPE_NAME, std::time::Duration::from_millis(500));
}

fn service_main() {
    if let Err(e) = serve_loop() {
        util::log(&format!("helper stopped with error: {e}"));
    }
}

fn serve_loop() -> Result<(), String> {
    util::ensure_private_dir(&crate::session::data_dir()).map_err(|e| e.to_string())?;
    util::set_log_file(&crate::session::data_dir().join("helper.log"));
    let cfg = load_config()?;
    let session = Session::default();
    session.recover_at_start();
    let mut listener = pipe::Listener::new(WIN_PIPE_NAME, &pipe_sddl(&cfg.allowed_sid))?;
    util::log(&format!("helper {} listening on the pipe", env!("CARGO_PKG_VERSION")));
    while !STOP.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok(conn) => {
                if STOP.load(Ordering::SeqCst) {
                    break;
                }
                let session = session.clone();
                let allowed = cfg.allowed_sid.clone();
                std::thread::spawn(move || handle(conn, &session, &allowed));
            }
            Err(e) => {
                util::log(&format!("accept: {e}"));
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
    util::log("helper stopping: ending session");
    let _ = session.restore();
    Ok(())
}

fn handle(conn: pipe::Conn, session: &Session, allowed_sid: &str) {
    let sid = conn.client_pid().ok_or_else(|| "no client pid".to_string()).and_then(security::process_user_sid);
    match sid {
        Ok(s) if s == allowed_sid || s == security::SYSTEM_SID => {}
        other => {
            util::log(&format!("rejected pipe client ({})", if other.is_ok() { "other user" } else { "unknown" }));
            let _ = conn.file().write_all(&rpc::encode(&Response::Error { code: ErrorCode::HelperAuthFailed, detail: None }));
            return;
        }
    }
    let mut line = String::new();
    if BufReader::new(conn.file().take(rpc::MAX_REQUEST)).read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let _ = conn.file().write_all(&rpc::encode(&rpc::dispatch(&line, session)));
}

#[cfg(test)]
mod tests {
    #[test]
    fn pipe_dacl_has_no_create_instance_for_user() {
        let sddl = super::pipe_sddl("S-1-5-21-1-2-3-1001");
        assert_eq!(sddl, "D:P(A;;GA;;;SY)(A;;0x0012008b;;;S-1-5-21-1-2-3-1001)");
        assert_eq!(0x0012008b & 0x4, 0, "FILE_CREATE_PIPE_INSTANCE must not be granted");
    }
}
