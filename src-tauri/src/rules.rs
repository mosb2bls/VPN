//! Rule-sets (`geosite-cn`, `geoip-cn`, sing-box binary `.srs`).
//!
//! The manifest (`resources/rules/manifest.json`) is embedded at compile time
//! and is the integrity reference. Bundled files are copied into the app data
//! dir (`<data>/rules/`) — the *active* set the core uses and that automatic
//! updates will later replace atomically. Every file is SHA-256-checked
//! against the manifest before use.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::api::error::{AppError, ErrorCode};
use crate::api::types::{RuleSetInfo, RulesSource, RulesStatus};
use crate::store::fsutil;

const EMBEDDED_MANIFEST: &str = include_str!("../../resources/rules/manifest.json");

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: String,
    pub sets: Vec<SetEntry>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetEntry {
    pub name: String,
    pub path: String,
    pub size: u32,
    pub sha256: String,
    pub committed_at: String,
}

pub fn embedded_manifest() -> Manifest {
    serde_json::from_str(EMBEDDED_MANIFEST).expect("embedded rules manifest is valid (checked by tests)")
}

/// Where the bundled rule-sets live: next to the executable in packaged builds
/// (`resources/rules`, wired in B6) or, in debug builds, the repo's
/// `resources/rules/` (filled by `npm run fetch-core`).
pub fn bundled_dir(resource_dir: Option<PathBuf>) -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources/rules")
    } else {
        resource_dir.unwrap_or_default().join("resources/rules")
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn file_ok(path: &Path, set: &SetEntry) -> bool {
    std::fs::read(path).is_ok_and(|b| b.len() as u32 == set.size && sha256_hex(&b) == set.sha256)
}

/// Active rule-set paths for config generation.
#[derive(Debug, Clone)]
pub struct ActiveRules {
    pub geosite_cn: PathBuf,
    pub geoip_cn: PathBuf,
}

/// Make sure `active` holds verified copies of every set, copying from
/// `bundled` where missing or not matching the manifest.
pub fn ensure_active(manifest: &Manifest, bundled: &Path, active: &Path) -> Result<ActiveRules, AppError> {
    fsutil::ensure_private_dir(active)
        .map_err(|e| AppError::new(ErrorCode::RulesMissing).with_detail(e.to_string()))?;
    for set in &manifest.sets {
        let dest = active.join(&set.path);
        if file_ok(&dest, set) {
            continue;
        }
        let src = bundled.join(&set.path);
        let bytes = std::fs::read(&src).map_err(|_| {
            AppError::new(ErrorCode::RulesMissing)
                .with_detail(format!("bundled {} missing (dev: run `npm run fetch-core`)", set.name))
        })?;
        if bytes.len() as u32 != set.size || sha256_hex(&bytes) != set.sha256 {
            return Err(AppError::new(ErrorCode::RulesValidationFailed)
                .with_detail(format!("bundled {} does not match the manifest", set.name)));
        }
        fsutil::write_atomic(&dest, &bytes)
            .map_err(|e| AppError::new(ErrorCode::RulesMissing).with_detail(e.to_string()))?;
    }
    let path = |name: &str| {
        manifest.sets.iter().find(|s| s.name == name).map(|s| active.join(&s.path)).ok_or_else(|| {
            AppError::new(ErrorCode::RulesMissing).with_detail(format!("manifest has no {name}"))
        })
    };
    Ok(ActiveRules { geosite_cn: path("geosite-cn")?, geoip_cn: path("geoip-cn")? })
}

/// Status for `get_rules_status` (bundled set; updates arrive in a later step).
pub fn status(manifest: &Manifest, last_error: Option<AppError>) -> RulesStatus {
    RulesStatus {
        version: manifest.version.clone(),
        source: RulesSource::Bundled,
        updated_at: manifest.sets.iter().map(|s| s.committed_at.clone()).max(),
        last_check_at: None,
        next_check_at: None,
        updating: false,
        sets: manifest
            .sets
            .iter()
            .map(|s| RuleSetInfo { name: s.name.clone(), size_bytes: s.size, sha256_prefix: s.sha256[..12].to_string() })
            .collect(),
        last_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_manifest(a: &[u8], b: &[u8]) -> Manifest {
        let entry = |name: &str, data: &[u8]| SetEntry {
            name: name.into(),
            path: format!("{name}.srs"),
            size: data.len() as u32,
            sha256: sha256_hex(data),
            committed_at: "2026-10-10T00:00:00Z".into(),
        };
        Manifest { version: "test".into(), sets: vec![entry("geosite-cn", a), entry("geoip-cn", b)] }
    }

    #[test]
    fn embedded_manifest_is_valid() {
        let m = embedded_manifest();
        assert!(m.sets.iter().any(|s| s.name == "geosite-cn"));
        assert!(m.sets.iter().any(|s| s.name == "geoip-cn"));
        for s in &m.sets {
            assert_eq!(s.sha256.len(), 64);
        }
    }

    #[test]
    fn copies_verifies_and_repairs() {
        let (bundled, active) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let m = fake_manifest(b"site-data", b"ip-data");
        std::fs::write(bundled.path().join("geosite-cn.srs"), b"site-data").unwrap();
        std::fs::write(bundled.path().join("geoip-cn.srs"), b"ip-data").unwrap();
        let r = ensure_active(&m, bundled.path(), active.path()).unwrap();
        assert_eq!(std::fs::read(&r.geosite_cn).unwrap(), b"site-data");
        // A tampered active file is replaced from the bundle.
        std::fs::write(&r.geoip_cn, b"tampered").unwrap();
        ensure_active(&m, bundled.path(), active.path()).unwrap();
        assert_eq!(std::fs::read(&r.geoip_cn).unwrap(), b"ip-data");
    }

    #[test]
    fn refuses_tampered_or_missing_bundle() {
        let (bundled, active) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let m = fake_manifest(b"site-data", b"ip-data");
        assert_eq!(ensure_active(&m, bundled.path(), active.path()).unwrap_err().code, ErrorCode::RulesMissing);
        std::fs::write(bundled.path().join("geosite-cn.srs"), b"site-dat4").unwrap();
        std::fs::write(bundled.path().join("geoip-cn.srs"), b"ip-data").unwrap();
        assert_eq!(
            ensure_active(&m, bundled.path(), active.path()).unwrap_err().code,
            ErrorCode::RulesValidationFailed
        );
    }

    /// The real bundled files (if fetched) match the embedded manifest.
    #[test]
    fn fetched_bundle_matches_manifest() {
        let m = embedded_manifest();
        let dir = bundled_dir(None);
        if !dir.join(&m.sets[0].path).exists() {
            eprintln!("skipped: run `npm run fetch-core`");
            return;
        }
        let active = tempfile::tempdir().unwrap();
        ensure_active(&m, &dir, active.path()).expect("bundled rule-sets verify");
        let st = status(&m, None);
        assert_eq!(st.updated_at.as_deref(), Some("2026-10-10T05:07:39Z"));
        assert_eq!(st.sets.len(), 2);
    }
}
