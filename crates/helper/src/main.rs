//! Privileged helper placeholder. Implemented in Phase B3 (see ADR, to be written).
//! Deliberately does nothing: it must never be installed or run with privileges
//! before IPC authentication and config validation exist.

fn main() {
    eprintln!("{} helper: not implemented (Phase B3)", edge_core::APP_NAME);
    std::process::exit(2);
}
