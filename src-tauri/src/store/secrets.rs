//! Secret storage. All node secrets are kept in **one** OS keychain item (a JSON
//! map node-id → secrets) so macOS asks for keychain access at most once per
//! launch, not once per node. Values never appear in logs or errors.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::api::error::{AppError, ErrorCode};

/// Secrets of one node. Only one protocol's fields are set.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obfs_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_id: Option<String>,
}

impl std::fmt::Debug for NodeSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NodeSecrets(••••)")
    }
}

pub type SecretMap = BTreeMap<String, NodeSecrets>;

/// Backend holding the serialized [`SecretMap`].
pub trait SecretStore: Send {
    /// `Ok(None)` if nothing has been stored yet.
    fn load(&self) -> Result<Option<String>, AppError>;
    fn save(&self, blob: &str) -> Result<(), AppError>;
}

pub fn load_map(store: &dyn SecretStore) -> Result<SecretMap, AppError> {
    match store.load()? {
        None => Ok(SecretMap::new()),
        Some(blob) => serde_json::from_str(&blob).map_err(|_| {
            AppError::new(ErrorCode::SecretMissing).with_detail("keychain item is not valid JSON")
        }),
    }
}

pub fn save_map(store: &dyn SecretStore, map: &SecretMap) -> Result<(), AppError> {
    let blob = serde_json::to_string(map)
        .map_err(|_| AppError::new(ErrorCode::Internal).with_detail("secret map serialization"))?;
    store.save(&blob)
}

// ---------------------------------------------------------------------------
// OS keychain (macOS Keychain / Windows Credential Manager) via keyring-core
// ---------------------------------------------------------------------------

pub struct KeychainStore {
    service: String,
}

const ACCOUNT: &str = "node-secrets";

impl KeychainStore {
    /// Install the platform credential store as keyring-core's default and
    /// return a handle. Call once at startup.
    pub fn init(service: &str) -> Result<Self, AppError> {
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(target_os = "windows")]
        let store = windows_native_keyring_store::Store::new();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let store = store.map_err(map_err)?;
            keyring_core::set_default_store(store);
            Ok(Self { service: service.into() })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = service;
            Err(AppError::new(ErrorCode::UnsupportedPlatform))
        }
    }

    fn entry(&self) -> Result<keyring_core::Entry, AppError> {
        keyring_core::Entry::new(&self.service, ACCOUNT).map_err(map_err)
    }

    /// Remove the item (used by the opt-in real-keychain test).
    #[cfg(test)]
    pub fn delete(&self) -> Result<(), AppError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

impl SecretStore for KeychainStore {
    fn load(&self) -> Result<Option<String>, AppError> {
        match self.entry()?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn save(&self, blob: &str) -> Result<(), AppError> {
        self.entry()?.set_password(blob).map_err(map_err)
    }
}

/// Keychain errors → stable codes. The platform error text is kept as detail
/// (it never contains the secret).
fn map_err(e: keyring_core::Error) -> AppError {
    let code = match &e {
        keyring_core::Error::NoStorageAccess(_) => ErrorCode::KeychainAccessDenied,
        keyring_core::Error::NoEntry => ErrorCode::SecretMissing,
        _ => ErrorCode::KeychainUnavailable,
    };
    AppError::new(code).with_detail(e.to_string())
}

/// Used when the platform keychain cannot be initialised: every access fails
/// with the original error (nodes then show `secretMissing`).
pub struct Unavailable(pub AppError);

impl SecretStore for Unavailable {
    fn load(&self) -> Result<Option<String>, AppError> {
        Err(self.0.clone())
    }
    fn save(&self, _: &str) -> Result<(), AppError> {
        Err(self.0.clone())
    }
}

// ---------------------------------------------------------------------------
// In-memory store for tests
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct MemoryStore {
    pub blob: Mutex<Option<String>>,
    /// When set, `save` fails with this code (to test rollback).
    pub fail_save: Mutex<Option<ErrorCode>>,
}

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<String>, AppError> {
        Ok(self.blob.lock().unwrap().clone())
    }
    fn save(&self, blob: &str) -> Result<(), AppError> {
        if let Some(code) = *self.fail_save.lock().unwrap() {
            return Err(AppError::new(code));
        }
        *self.blob.lock().unwrap() = Some(blob.into());
        Ok(())
    }
}

impl SecretStore for std::sync::Arc<MemoryStore> {
    fn load(&self) -> Result<Option<String>, AppError> {
        self.as_ref().load()
    }
    fn save(&self, blob: &str) -> Result<(), AppError> {
        self.as_ref().save(blob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opt-in: talks to the **real** OS keychain under a separate test service
    /// name and deletes the item afterwards. May show a macOS keychain prompt.
    /// Run: `cargo test -p edge-app --lib real_keychain_roundtrip -- --ignored`
    #[test]
    #[ignore]
    fn real_keychain_roundtrip() {
        let store = KeychainStore::init("io.github.mosb2bls.edgeclient.selftest").expect("init keychain");
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
        let mut map = SecretMap::new();
        map.insert("node-1".into(), NodeSecrets { password: Some("FAKE-selftest".into()), ..Default::default() });
        save_map(&store, &map).unwrap();
        assert_eq!(load_map(&store).unwrap(), map);
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }
}
