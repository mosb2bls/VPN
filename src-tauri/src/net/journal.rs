//! State journal: written **before** any OS change, with the previous values,
//! so a crash at any point can be undone (on next launch or via
//! `--restore-networking`). Deleted only after a complete restore.

use std::path::{Path, PathBuf};

use edge_core::sysproxy::ServiceProxies;
use serde::{Deserialize, Serialize};

use crate::api::error::{AppError, ErrorCode};
use crate::store::fsutil;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CoreRecord {
    pub pid: u32,
    pub exe: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRecord {
    /// Settings before we changed anything.
    pub prev: ServiceProxies,
    /// What we set (used to tell our values from later user changes).
    pub ours: ServiceProxies,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyRecord {
    pub port: u16,
    pub services: Vec<ServiceRecord>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    pub version: u32,
    pub created_at: String,
    pub core: Option<CoreRecord>,
    pub system_proxy: Option<SystemProxyRecord>,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Journal {
    fn default() -> Self {
        Self { version: 1, created_at: edge_core::clock::now_rfc3339(), core: None, system_proxy: None }
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join("journal.json")
}

pub fn load(dir: &Path) -> Result<Option<Journal>, AppError> {
    match std::fs::read(path(dir)) {
        Ok(b) => serde_json::from_slice(&b)
            .map(Some)
            .map_err(|e| AppError::new(ErrorCode::JournalCorrupt).fatal().with_detail(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::new(ErrorCode::JournalCorrupt).fatal().with_detail(e.to_string())),
    }
}

pub fn save(dir: &Path, j: &Journal) -> Result<(), AppError> {
    let json = serde_json::to_vec_pretty(j).expect("journal serializes");
    fsutil::write_atomic(&path(dir), &json)
        .map_err(|e| AppError::new(ErrorCode::SettingsSaveFailed).with_detail(format!("journal: {e}")))
}

pub fn delete(dir: &Path) {
    let _ = std::fs::remove_file(path(dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_missing_and_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), None);
        let mut j = Journal::new();
        j.core = Some(CoreRecord { pid: 42, exe: "/x/sing-box".into() });
        save(dir.path(), &j).unwrap();
        assert_eq!(load(dir.path()).unwrap(), Some(j));
        std::fs::write(path(dir.path()), b"{oops").unwrap();
        assert_eq!(load(dir.path()).unwrap_err().code, ErrorCode::JournalCorrupt);
        delete(dir.path());
        assert_eq!(load(dir.path()).unwrap(), None);
    }
}
