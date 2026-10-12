//! Per-OS pieces of the helper session: locations, stale-core cleanup, the
//! kill switch and process spawning. Everything else in `session` is shared.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
mod pfctl;
#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;
