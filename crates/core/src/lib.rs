//! Lynnk core: pure Rust, no OS side effects.
//!
//! Holds the product identity, stable error codes and restore orchestration.
//! Parsing, config generation, rules, DNS, failover logic and redaction arrive
//! in Phase B2+.

/// User-visible product name. Rename the product here and in
/// `src-tauri/tauri.conf.json` (`productName`, window `title`s); the
/// `name_matches_tauri_conf` test fails if the two drift apart.
pub const APP_NAME: &str = "Lynnk";

/// Lowercase slug for file names, keychain service names, socket/pipe names.
pub const APP_SLUG: &str = "lynnk";

/// Bundle identifier (must equal `identifier` in `src-tauri/tauri.conf.json`).
/// Also the name of the app's config/data folder and keychain service.
pub const APP_IDENTIFIER: &str = "io.github.mosb2bls.edgeclient";

pub mod clock;
pub mod error_code;
pub mod failover;
pub mod helper_proto;
pub mod link;
pub mod pf;
pub mod wfp;
pub mod qr;
pub mod redact;
pub mod restore;
pub mod secret;
pub mod singbox;
pub mod sysproxy;
pub mod validate;
pub mod winproxy;

pub use error_code::ErrorCode;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_lowercase_name() {
        assert_eq!(APP_SLUG, APP_NAME.to_lowercase());
    }

    #[test]
    fn name_matches_tauri_conf() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/tauri.conf.json");
        let conf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(conf["productName"], APP_NAME);
        assert_eq!(conf["identifier"], APP_IDENTIFIER);
        for w in conf["app"]["windows"].as_array().unwrap() {
            assert!(w["title"].as_str().unwrap().starts_with(APP_NAME), "window title {w}");
        }
    }
}
