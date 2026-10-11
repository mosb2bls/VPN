//! EdgeClient core: pure Rust, no OS side effects.
//!
//! Holds the product identity, stable error codes and restore orchestration.
//! Parsing, config generation, rules, DNS, failover logic and redaction arrive
//! in Phase B2+.

/// User-visible product name. Rename the product here and in
/// `src-tauri/tauri.conf.json` (`productName`, window `title`s); the
/// `name_matches_tauri_conf` test fails if the two drift apart.
pub const APP_NAME: &str = "EdgeClient";

/// Lowercase slug for file names, keychain service names, socket/pipe names.
pub const APP_SLUG: &str = "edgeclient";

pub mod error_code;
pub mod restore;

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
        for w in conf["app"]["windows"].as_array().unwrap() {
            assert!(w["title"].as_str().unwrap().starts_with(APP_NAME), "window title {w}");
        }
    }
}
