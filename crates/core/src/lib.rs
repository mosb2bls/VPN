//! EdgeClient core: pure Rust, no OS side effects.
//!
//! Phase B0 holds only the product identity. Parsing, config generation,
//! rules, DNS, failover logic and redaction arrive in Phase B2.

/// User-visible product name. Rename the product here and in
/// `src-tauri/tauri.conf.json` (`productName`, window `title`); the
/// `name_matches_tauri_conf` test fails if the two drift apart.
pub const APP_NAME: &str = "EdgeClient";

/// Lowercase slug for file names, keychain service names, socket/pipe names.
pub const APP_SLUG: &str = "edgeclient";

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
        assert_eq!(conf["app"]["windows"][0]["title"], APP_NAME);
    }
}
