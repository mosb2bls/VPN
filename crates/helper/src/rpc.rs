//! Request dispatch shared by the macOS socket server and the Windows pipe
//! server: one JSON line in, one JSON response out. Only the closed set of
//! `Request` variants exists; anything else is rejected.

use edge_core::ErrorCode;
use edge_core::helper_proto::*;

use crate::session::Session;

/// Max request size (bytes).
pub const MAX_REQUEST: u64 = 1 << 20;

pub fn dispatch(line: &str, session: &Session) -> Response {
    let req: Request = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(_) => return Response::Error { code: ErrorCode::InvalidArgument, detail: Some("unknown request".into()) },
    };
    match req {
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
    }
}

pub fn encode(r: &Response) -> Vec<u8> {
    let mut line = serde_json::to_string(r).unwrap_or_else(|_| "{}".into());
    line.push('\n');
    line.into_bytes()
}
