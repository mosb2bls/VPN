//! Locating and verifying the bundled sing-box binary. The app only ever runs
//! the binary whose SHA-256 matches the hash embedded at build time (see
//! build.rs / scripts/sing-box.lock.json); there is no user-configurable path.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::api::error::{ApiResult, AppError, ErrorCode};

/// Pinned sing-box version (from scripts/sing-box.lock.json).
pub const CORE_VERSION: &str = env!("EDGE_SINGBOX_VERSION");
/// Expected SHA-256 of the binary for this build target ("" if not fetched).
const EXPECTED_SHA256: &str = env!("EDGE_SINGBOX_SHA256");
const TARGET: &str = env!("EDGE_TARGET_TRIPLE");
const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

/// Candidate locations, in order: next to the app executable (bundled builds;
/// Tauri strips the target triple), then — debug builds only — the dev
/// download location `src-tauri/binaries/`.
fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        v.push(dir.join(format!("sing-box{EXE}")));
    }
    if cfg!(debug_assertions) {
        v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!("sing-box-{TARGET}{EXE}")));
    }
    v
}

/// Expected SHA-256 of the core for this build ("" if not fetched).
pub fn expected_sha256() -> &'static str {
    EXPECTED_SHA256
}

/// Find the core binary and verify its hash. Returns its path.
pub fn locate_verified() -> ApiResult<PathBuf> {
    if EXPECTED_SHA256.is_empty() {
        return Err(AppError::new(ErrorCode::CoreBinaryMissing)
            .with_detail("built without the core: run `npm run fetch-core`, then rebuild"));
    }
    let path = candidates()
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| AppError::new(ErrorCode::CoreBinaryMissing).with_detail("sing-box binary not found"))?;
    verify_file(&path, EXPECTED_SHA256)?;
    Ok(path)
}

pub fn verify_file(path: &Path, expected_hex: &str) -> ApiResult<()> {
    let bytes = std::fs::read(path)
        .map_err(|e| AppError::new(ErrorCode::CoreBinaryMissing).with_detail(e.to_string()))?;
    let got = hex(&Sha256::digest(&bytes));
    if got.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(AppError::new(ErrorCode::CoreChecksumMismatch)
            .fatal()
            .with_detail(format!("expected {expected_hex}, got {got}")))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("bin");
        std::fs::write(&f, b"hello").unwrap();
        let good = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_file(&f, good).is_ok());
        std::fs::write(&f, b"hellO").unwrap();
        assert_eq!(verify_file(&f, good).unwrap_err().code, ErrorCode::CoreChecksumMismatch);
        assert_eq!(verify_file(&dir.path().join("nope"), good).unwrap_err().code, ErrorCode::CoreBinaryMissing);
    }

    #[test]
    fn lock_file_is_consistent() {
        let lock: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/sing-box.lock.json")).unwrap(),
        )
        .unwrap();
        let version = lock["version"].as_str().unwrap();
        assert_eq!(version, CORE_VERSION);
        for triple in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let t = &lock["targets"][triple];
            assert!(t["asset"].as_str().unwrap().contains(version), "{triple} asset matches version");
            let h = t["sha256"].as_str().unwrap();
            assert!(h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()), "{triple} sha256");
        }
    }

    /// Runs only when the core has been fetched for this target.
    #[test]
    fn fetched_core_verifies() {
        if EXPECTED_SHA256.is_empty() {
            eprintln!("skipped: core not fetched");
            return;
        }
        let p = locate_verified().expect("fetched core must verify");
        let out = std::process::Command::new(p).arg("version").output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains(CORE_VERSION));
    }
}
