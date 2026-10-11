//! Persistent node list. Non-secret data in `<config dir>/nodes.json`
//! (0600, atomic writes); secrets in the OS keychain (see [`secrets`]).
//!
//! Write order for every change: keychain first, then file; if the file write
//! fails the keychain is rolled back (best effort). If the keychain could not
//! be read at startup, any change that would rewrite it is refused, so
//! existing secrets can never be overwritten with an incomplete map.

pub mod form;
pub mod fsutil;
pub mod prefs;
pub mod secrets;

use std::path::{Path, PathBuf};

use edge_core::link::{self, NodeConfig};
use edge_core::secret::{Secret, mask};
use edge_core::validate::{self, PortRange};
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiResult, AppError, ErrorCode};
use crate::api::types::{
    Hy2Details, ImportLineError, ImportResult, Node, NodeDetails, NodeForm, NodeTestResult, Protocol,
    VlessDetails,
};
use secrets::{NodeSecrets, SecretMap, SecretStore};

/// Non-secret, persisted node parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum StoredParams {
    #[serde(rename_all = "camelCase")]
    Hysteria2 {
        server: String,
        port: u16,
        server_ports: Vec<String>,
        sni: Option<String>,
        insecure: bool,
        /// Salamander obfs on (password in the keychain).
        obfs: bool,
        up_mbps: Option<u32>,
        down_mbps: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    VlessReality {
        server: String,
        port: u16,
        sni: String,
        fingerprint: String,
        spider_x: Option<String>,
        alpn: Vec<String>,
    },
}

impl StoredParams {
    fn protocol(&self) -> Protocol {
        match self {
            StoredParams::Hysteria2 { .. } => Protocol::Hysteria2,
            StoredParams::VlessReality { .. } => Protocol::VlessReality,
        }
    }
    fn server_port(&self) -> (&str, u16) {
        match self {
            StoredParams::Hysteria2 { server, port, .. } | StoredParams::VlessReality { server, port, .. } => {
                (server, *port)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StoredNode {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub params: StoredParams,
    #[serde(default)]
    pub last_test: Option<NodeTestResult>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct NodesFile {
    version: u32,
    selected_node_id: Option<String>,
    nodes: Vec<StoredNode>,
}

const FILE_VERSION: u32 = 1;

pub struct NodeStore {
    path: PathBuf,
    backend: Box<dyn SecretStore>,
    file: NodesFile,
    secrets: SecretMap,
    /// Set if the keychain could not be read at startup; secret writes refused.
    secrets_error: Option<AppError>,
}

impl NodeStore {
    /// Load from `dir`. Problems are returned as warnings (the store still opens).
    pub fn open(dir: &Path, backend: Box<dyn SecretStore>) -> (Self, Vec<AppError>) {
        let path = dir.join("nodes.json");
        let mut warnings = Vec::new();
        let file = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<NodesFile>(&bytes) {
                Ok(f) => f,
                Err(e) => {
                    fsutil::quarantine(&path);
                    warnings.push(
                        AppError::new(ErrorCode::SettingsInvalid)
                            .with_detail(format!("nodes.json unreadable, moved to nodes.json.corrupt: {e}")),
                    );
                    NodesFile::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => NodesFile::default(),
            Err(e) => {
                warnings.push(AppError::new(ErrorCode::SettingsInvalid).with_detail(format!("nodes.json: {e}")));
                NodesFile::default()
            }
        };
        let (secrets, secrets_error) = match secrets::load_map(backend.as_ref()) {
            Ok(m) => (m, None),
            Err(e) => {
                warnings.push(e.clone());
                (SecretMap::new(), Some(e))
            }
        };
        let mut store = Self { path, backend, file, secrets, secrets_error };
        store.file.version = FILE_VERSION;
        if store.file.selected_node_id.as_ref().is_some_and(|id| store.find(id).is_none()) {
            store.file.selected_node_id = store.file.nodes.first().map(|n| n.id.clone());
        }
        (store, warnings)
    }

    fn find(&self, id: &str) -> Option<&StoredNode> {
        self.file.nodes.iter().find(|n| n.id == id)
    }

    pub fn list(&self) -> Vec<Node> {
        self.file.nodes.iter().map(|n| self.to_dto(n)).collect()
    }

    pub fn selected_id(&self) -> Option<String> {
        self.file.selected_node_id.clone()
    }

    pub fn name_of(&self, id: &str) -> Option<String> {
        self.find(id).map(|n| n.name.clone())
    }

    fn to_dto(&self, n: &StoredNode) -> Node {
        let s = self.secrets.get(&n.id);
        let m = |v: Option<&String>| v.map(|x| mask(x)).unwrap_or_default();
        let (server, port) = n.params.server_port();
        let details = match &n.params {
            StoredParams::Hysteria2 { server_ports, sni, insecure, obfs, up_mbps, down_mbps, .. } => {
                NodeDetails::Hysteria2(Hy2Details {
                    password_masked: m(s.and_then(|s| s.password.as_ref())),
                    server_ports: server_ports.clone(),
                    sni: sni.clone(),
                    insecure: *insecure,
                    obfs_password_masked: obfs.then(|| m(s.and_then(|s| s.obfs_password.as_ref()))),
                    pin_sha256: None,
                    up_mbps: *up_mbps,
                    down_mbps: *down_mbps,
                })
            }
            StoredParams::VlessReality { sni, fingerprint, spider_x, .. } => NodeDetails::VlessReality(VlessDetails {
                uuid_masked: m(s.and_then(|s| s.uuid.as_ref())),
                flow: validate::VISION_FLOW.into(),
                sni: sni.clone(),
                fingerprint: fingerprint.clone(),
                public_key_masked: m(s.and_then(|s| s.public_key.as_ref())),
                // Short IDs are short: always fully masked.
                short_id_masked: if s.and_then(|s| s.short_id.as_ref()).is_some_and(|v| !v.is_empty()) {
                    "••••".into()
                } else {
                    String::new()
                },
                spider_x: spider_x.clone(),
            }),
        };
        Node {
            id: n.id.clone(),
            name: n.name.clone(),
            protocol: n.params.protocol(),
            server: server.to_string(),
            port,
            details,
            last_test: n.last_test.clone(),
            secret_missing: !self.secrets_complete(n),
        }
    }

    fn secrets_complete(&self, n: &StoredNode) -> bool {
        let Some(s) = self.secrets.get(&n.id) else { return false };
        match &n.params {
            StoredParams::Hysteria2 { obfs, .. } => s.password.is_some() && (!obfs || s.obfs_password.is_some()),
            StoredParams::VlessReality { .. } => s.uuid.is_some() && s.public_key.is_some() && s.short_id.is_some(),
        }
    }

    /// Identity for duplicate detection: protocol + server + port + credential.
    fn identity(params: &StoredParams, secrets: Option<&NodeSecrets>) -> (Protocol, String, u16, Option<String>) {
        let (server, port) = params.server_port();
        let cred = secrets.and_then(|s| s.password.clone().or_else(|| s.uuid.clone()));
        (params.protocol(), server.to_string(), port, cred)
    }

    fn is_duplicate(&self, params: &StoredParams, secrets: &NodeSecrets, except: Option<&str>) -> bool {
        let id = Self::identity(params, Some(secrets));
        self.file
            .nodes
            .iter()
            .filter(|n| Some(n.id.as_str()) != except)
            .any(|n| Self::identity(&n.params, self.secrets.get(&n.id)) == id)
    }

    /// Persist `file` + `secrets` (keychain first, file second, rollback on failure).
    fn commit(&mut self, file: NodesFile, secrets: Option<SecretMap>) -> ApiResult<()> {
        if let Some(new_secrets) = &secrets {
            if let Some(e) = &self.secrets_error {
                return Err(e.clone().with_detail(
                    "keychain could not be read at startup; refusing to overwrite stored secrets",
                ));
            }
            secrets::save_map(self.backend.as_ref(), new_secrets)?;
        }
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|_| AppError::new(ErrorCode::Internal).with_detail("nodes.json serialization"))?;
        if let Err(e) = fsutil::write_atomic(&self.path, &json) {
            if secrets.is_some() {
                let _ = secrets::save_map(self.backend.as_ref(), &self.secrets);
            }
            return Err(AppError::new(ErrorCode::SettingsSaveFailed).with_detail(e.to_string()));
        }
        self.file = file;
        if let Some(s) = secrets {
            self.secrets = s;
        }
        Ok(())
    }

    // --- Mutations ---------------------------------------------------------

    pub fn import(&mut self, text: &str) -> ApiResult<ImportResult> {
        let results = link::parse_links(text);
        if results.is_empty() {
            return Err(AppError::new(ErrorCode::ImportEmpty));
        }
        let mut file = self.file.clone();
        let mut secrets = self.secrets.clone();
        let mut added_ids = Vec::new();
        let mut errors = Vec::new();
        for r in results {
            let line_err = |error: AppError| ImportLineError { line: r.line, scheme: r.scheme.clone(), error };
            let parsed = match r.result {
                Ok(p) => p,
                Err(e) => {
                    errors.push(line_err(AppError::new(e.code).with_detail(e.detail())));
                    continue;
                }
            };
            let (params, node_secrets) = from_config(&parsed.config);
            let id_key = Self::identity(&params, Some(&node_secrets));
            let dup = file.nodes.iter().any(|n| Self::identity(&n.params, secrets.get(&n.id)) == id_key);
            if dup {
                errors.push(line_err(AppError::new(ErrorCode::NodeDuplicate)));
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            secrets.insert(id.clone(), node_secrets);
            file.nodes.push(StoredNode { id: id.clone(), name: parsed.name, params, last_test: None });
            added_ids.push(id);
        }
        if !added_ids.is_empty() {
            if file.selected_node_id.is_none() {
                file.selected_node_id = added_ids.first().cloned();
            }
            self.commit(file, Some(secrets))?;
        }
        let added = added_ids.iter().filter_map(|id| self.find(id)).map(|n| self.to_dto(n)).collect();
        Ok(ImportResult { added, errors })
    }

    pub fn add(&mut self, form: &NodeForm) -> ApiResult<Node> {
        let v = form::validate_form(form, None).map_err(AppError::validation)?;
        if self.is_duplicate(&v.params, &v.secrets, None) {
            return Err(AppError::new(ErrorCode::NodeDuplicate));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut file = self.file.clone();
        let mut secrets = self.secrets.clone();
        secrets.insert(id.clone(), v.secrets);
        file.nodes.push(StoredNode { id: id.clone(), name: v.name, params: v.params, last_test: None });
        if file.selected_node_id.is_none() {
            file.selected_node_id = Some(id.clone());
        }
        self.commit(file, Some(secrets))?;
        Ok(self.to_dto(self.find(&id).expect("just added")))
    }

    pub fn update(&mut self, id: &str, form: &NodeForm) -> ApiResult<Node> {
        let current = self.find(id).ok_or_else(|| AppError::new(ErrorCode::NodeNotFound))?;
        let same_protocol = matches!(
            (&current.params, form),
            (StoredParams::Hysteria2 { .. }, NodeForm::Hysteria2(_))
                | (StoredParams::VlessReality { .. }, NodeForm::VlessReality(_))
        );
        let existing = if same_protocol { self.secrets.get(id) } else { None };
        let v = form::validate_form(form, existing).map_err(AppError::validation)?;
        if self.is_duplicate(&v.params, &v.secrets, Some(id)) {
            return Err(AppError::new(ErrorCode::NodeDuplicate));
        }
        let secrets_changed = self.secrets.get(id) != Some(&v.secrets);
        let mut file = self.file.clone();
        let n = file.nodes.iter_mut().find(|n| n.id == id).expect("checked above");
        let endpoint_changed = n.params != v.params || secrets_changed;
        n.name = v.name;
        n.params = v.params;
        if endpoint_changed {
            n.last_test = None;
        }
        let secrets = secrets_changed.then(|| {
            let mut s = self.secrets.clone();
            s.insert(id.to_string(), v.secrets);
            s
        });
        self.commit(file, secrets)?;
        Ok(self.to_dto(self.find(id).expect("exists")))
    }

    pub fn delete(&mut self, id: &str) -> ApiResult<()> {
        if self.find(id).is_none() {
            return Err(AppError::new(ErrorCode::NodeNotFound));
        }
        let mut file = self.file.clone();
        file.nodes.retain(|n| n.id != id);
        if file.selected_node_id.as_deref() == Some(id) {
            file.selected_node_id = file.nodes.first().map(|n| n.id.clone());
        }
        // Secrets are removed only if the keychain is readable; otherwise the
        // orphaned entry is harmless and cleaned up on a later change.
        let secrets = self.secrets_error.is_none().then(|| {
            let mut s = self.secrets.clone();
            s.remove(id);
            s
        });
        self.commit(file, secrets)
    }

    pub fn reorder(&mut self, ids: &[String]) -> ApiResult<Vec<Node>> {
        let mut current: Vec<&str> = self.file.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut wanted: Vec<&str> = ids.iter().map(String::as_str).collect();
        current.sort_unstable();
        wanted.sort_unstable();
        if current != wanted {
            return Err(AppError::new(ErrorCode::ReorderMismatch));
        }
        let mut file = self.file.clone();
        file.nodes = ids.iter().filter_map(|id| self.find(id).cloned()).collect();
        self.commit(file, None)?;
        Ok(self.list())
    }

    pub fn select(&mut self, id: &str) -> ApiResult<()> {
        if self.find(id).is_none() {
            return Err(AppError::new(ErrorCode::NodeNotFound));
        }
        let mut file = self.file.clone();
        file.selected_node_id = Some(id.to_string());
        self.commit(file, None)
    }

    /// Full connection config (with secrets) for the core.
    pub fn config(&self, id: &str) -> ApiResult<NodeConfig> {
        let n = self.find(id).ok_or_else(|| AppError::new(ErrorCode::NodeNotFound))?;
        if !self.secrets_complete(n) {
            return Err(self.secrets_error.clone().unwrap_or_else(|| AppError::new(ErrorCode::SecretMissing)));
        }
        to_config(&n.params, &self.secrets[id])
    }
}

/// Split a parsed config into persisted params + keychain secrets.
pub fn from_config(c: &NodeConfig) -> (StoredParams, NodeSecrets) {
    match c {
        NodeConfig::Hysteria2(h) => (
            StoredParams::Hysteria2 {
                server: h.server.clone(),
                port: h.port,
                server_ports: h.server_ports.iter().map(|r| r.to_display()).collect(),
                sni: h.sni.clone(),
                insecure: h.insecure,
                obfs: h.obfs_password.is_some(),
                up_mbps: h.up_mbps,
                down_mbps: h.down_mbps,
            },
            NodeSecrets {
                password: Some(h.password.expose().into()),
                obfs_password: h.obfs_password.as_ref().map(|s| s.expose().into()),
                ..Default::default()
            },
        ),
        NodeConfig::VlessReality(v) => (
            StoredParams::VlessReality {
                server: v.server.clone(),
                port: v.port,
                sni: v.sni.clone(),
                fingerprint: v.fingerprint.clone(),
                spider_x: v.spider_x.clone(),
                alpn: v.alpn.clone(),
            },
            NodeSecrets {
                uuid: Some(v.uuid.expose().into()),
                public_key: Some(v.public_key.expose().into()),
                short_id: Some(v.short_id.expose().into()),
                ..Default::default()
            },
        ),
    }
}

/// Rebuild the full config from persisted params + secrets.
pub fn to_config(p: &StoredParams, s: &NodeSecrets) -> ApiResult<NodeConfig> {
    let missing = || AppError::new(ErrorCode::SecretMissing);
    Ok(match p {
        StoredParams::Hysteria2 { server, port, server_ports, sni, insecure, obfs, up_mbps, down_mbps } => {
            let ports: Vec<PortRange> = if server_ports.is_empty() {
                vec![]
            } else {
                validate::parse_port_list(&server_ports.join(","))
                    .map_err(|c| AppError::new(c).with_detail("stored serverPorts"))?
            };
            NodeConfig::Hysteria2(link::Hy2Config {
                server: server.clone(),
                port: *port,
                server_ports: ports,
                password: Secret::new(s.password.clone().ok_or_else(missing)?),
                sni: sni.clone(),
                insecure: *insecure,
                obfs_password: if *obfs { Some(Secret::new(s.obfs_password.clone().ok_or_else(missing)?)) } else { None },
                up_mbps: *up_mbps,
                down_mbps: *down_mbps,
            })
        }
        StoredParams::VlessReality { server, port, sni, fingerprint, spider_x, alpn } => {
            NodeConfig::VlessReality(link::VlessRealityConfig {
                server: server.clone(),
                port: *port,
                uuid: Secret::new(s.uuid.clone().ok_or_else(missing)?),
                sni: sni.clone(),
                fingerprint: fingerprint.clone(),
                public_key: Secret::new(s.public_key.clone().ok_or_else(missing)?),
                short_id: Secret::new(s.short_id.clone().ok_or_else(missing)?),
                spider_x: spider_x.clone(),
                alpn: alpn.clone(),
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::secrets::MemoryStore;
    use super::*;
    use crate::api::types::{Hy2Form, SecretInput, VlessForm};
    use std::sync::Arc;

    // Fake values only.
    const PASS: &str = "FAKE-hy2-password-0001";
    const UUID: &str = "00000000-0000-4000-8000-0000000000aa";
    fn pbk() -> String {
        // 43 base64url chars = 32 bytes; last char must have its low 2 bits zero.
        format!("{}E", "B".repeat(42))
    }
    fn hy2_link() -> String {
        format!("hysteria2://{PASS}@proxy.example.com:24443/?sni=proxy.example.com&insecure=0#HY2-Example")
    }
    fn vless_link() -> String {
        format!(
            "vless://{UUID}@203.0.113.7:443?type=raw&security=reality&fp=chrome&sni=www.example.org&pbk={}&sid=0123abcd&flow=xtls-rprx-vision#REALITY-Example",
            pbk()
        )
    }
    fn open(dir: &Path, mem: &Arc<MemoryStore>) -> NodeStore {
        let (s, w) = NodeStore::open(dir, Box::new(mem.clone()));
        assert!(w.is_empty(), "{w:?}");
        s
    }
    fn hy2_form(password: SecretInput) -> NodeForm {
        NodeForm::Hysteria2(Hy2Form {
            name: "Manual HY2".into(),
            server: "proxy.example.com".into(),
            port: 24443,
            server_ports: vec![],
            password,
            sni: Some("proxy.example.com".into()),
            insecure: false,
            obfs_password: SecretInput::Keep,
            pin_sha256: None,
            up_mbps: None,
            down_mbps: None,
        })
    }

    #[test]
    fn import_persist_reload_and_no_secrets_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Arc::new(MemoryStore::default());
        let mut s = open(dir.path(), &mem);
        let r = s.import(&format!("{}\n\nvmess://x\n{}\n{}", hy2_link(), vless_link(), hy2_link())).unwrap();
        assert_eq!(r.added.len(), 2);
        assert_eq!(r.errors.len(), 2);
        assert_eq!((r.errors[0].line, r.errors[0].error.code), (3, ErrorCode::LinkUnsupportedScheme));
        assert_eq!((r.errors[1].line, r.errors[1].error.code), (5, ErrorCode::NodeDuplicate));
        assert_eq!(s.selected_id(), Some(r.added[0].id.clone()));
        let NodeDetails::Hysteria2(d) = &r.added[0].details else { panic!() };
        assert_eq!(d.password_masked, "FAKE…••••");
        assert!(!r.added[0].secret_missing);

        // On disk: no secret material at all.
        let on_disk = std::fs::read_to_string(dir.path().join("nodes.json")).unwrap();
        for secret in [PASS, UUID, &pbk(), "0123abcd"] {
            assert!(!on_disk.contains(secret), "secret on disk");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("nodes.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        // Reload: same list, configs round-trip to what the parser produced.
        let s2 = open(dir.path(), &mem);
        assert_eq!(s2.list().len(), 2);
        let parsed = edge_core::link::parse_link(&hy2_link()).unwrap().config;
        assert_eq!(s2.config(&r.added[0].id).unwrap(), parsed);
        let parsed = edge_core::link::parse_link(&vless_link()).unwrap().config;
        assert_eq!(s2.config(&r.added[1].id).unwrap(), parsed);
    }

    #[test]
    fn import_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path(), &Arc::new(MemoryStore::default()));
        assert_eq!(s.import("  \n ").unwrap_err().code, ErrorCode::ImportEmpty);
    }

    #[test]
    fn manual_add_update_keep_set_clear() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Arc::new(MemoryStore::default());
        let mut s = open(dir.path(), &mem);
        // Add requires the password to be set.
        let e = s.add(&hy2_form(SecretInput::Keep)).unwrap_err();
        assert_eq!(e.code, ErrorCode::ValidationFailed);
        assert_eq!(e.fields[0].field, "password");
        let n = s.add(&hy2_form(SecretInput::Set(PASS.into()))).unwrap();
        assert_eq!(s.add(&hy2_form(SecretInput::Set(PASS.into()))).unwrap_err().code, ErrorCode::NodeDuplicate);

        // Update with Keep keeps the secret; Clear on a required secret is refused.
        let mut f = hy2_form(SecretInput::Keep);
        if let NodeForm::Hysteria2(h) = &mut f {
            h.name = "Renamed".into();
            h.obfs_password = SecretInput::Set("FAKE-obfs-secret".into());
        }
        let u = s.update(&n.id, &f).unwrap();
        assert_eq!(u.name, "Renamed");
        let NodeDetails::Hysteria2(d) = &u.details else { panic!() };
        assert_eq!(d.obfs_password_masked.as_deref(), Some("FAKE…••••"));
        let cfg = s.config(&n.id).unwrap();
        let NodeConfig::Hysteria2(h) = cfg else { panic!() };
        assert_eq!(h.password.expose(), PASS);
        let e = s.update(&n.id, &hy2_form(SecretInput::Clear)).unwrap_err();
        assert_eq!((e.fields[0].field.as_str(), e.fields[0].code), ("password", ErrorCode::FieldCannotClear));
        // Clear the optional obfs password.
        let mut f = hy2_form(SecretInput::Keep);
        if let NodeForm::Hysteria2(h) = &mut f {
            h.obfs_password = SecretInput::Clear;
        }
        let NodeDetails::Hysteria2(d) = s.update(&n.id, &f).unwrap().details else { panic!() };
        assert!(d.obfs_password_masked.is_none());
    }

    #[test]
    fn form_field_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = open(dir.path(), &Arc::new(MemoryStore::default()));
        let f = NodeForm::VlessReality(VlessForm {
            name: "".into(),
            server: "[2001:db8::1]".into(),
            port: 0,
            uuid: SecretInput::Set("nope".into()),
            sni: "".into(),
            fingerprint: "chrome_pq".into(),
            public_key: SecretInput::Set("short".into()),
            short_id: SecretInput::Set("".into()), // empty short ID is valid
            spider_x: None,
        });
        let e = s.add(&f).unwrap_err();
        let fields: Vec<(&str, ErrorCode)> = e.fields.iter().map(|f| (f.field.as_str(), f.code)).collect();
        assert_eq!(
            fields,
            [
                ("name", ErrorCode::FieldRequired),
                ("server", ErrorCode::FieldInvalidHost),
                ("port", ErrorCode::FieldInvalidPort),
                ("sni", ErrorCode::FieldRequired),
                ("fingerprint", ErrorCode::FieldInvalidFingerprint),
                ("uuid", ErrorCode::FieldInvalidUuid),
                ("publicKey", ErrorCode::FieldInvalidPublicKey),
            ]
        );
        // HY2 pin must be null for now.
        let mut f = hy2_form(SecretInput::Set(PASS.into()));
        if let NodeForm::Hysteria2(h) = &mut f {
            h.pin_sha256 = Some("ab".repeat(32));
        }
        assert_eq!(s.add(&f).unwrap_err().fields[0].field, "pinSha256");
    }

    #[test]
    fn delete_reorder_select() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Arc::new(MemoryStore::default());
        let mut s = open(dir.path(), &mem);
        let r = s.import(&format!("{}\n{}", hy2_link(), vless_link())).unwrap();
        let (a, b) = (r.added[0].id.clone(), r.added[1].id.clone());
        assert_eq!(s.reorder(std::slice::from_ref(&a)).unwrap_err().code, ErrorCode::ReorderMismatch);
        s.reorder(&[b.clone(), a.clone()]).unwrap();
        s.select(&b).unwrap();
        assert_eq!(s.select("missing").unwrap_err().code, ErrorCode::NodeNotFound);
        s.delete(&b).unwrap();
        assert_eq!(s.selected_id(), Some(a.clone()));
        let s2 = open(dir.path(), &mem);
        assert_eq!(s2.list().iter().map(|n| n.id.clone()).collect::<Vec<_>>(), [a]);
        assert!(!mem.blob.lock().unwrap().clone().unwrap().contains(UUID), "deleted node's secrets removed");
    }

    #[test]
    fn keychain_failure_rolls_back_and_unreadable_keychain_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Arc::new(MemoryStore::default());
        let mut s = open(dir.path(), &mem);
        s.import(&hy2_link()).unwrap();

        // Save failure: nothing changes, error surfaces.
        *mem.fail_save.lock().unwrap() = Some(ErrorCode::KeychainAccessDenied);
        assert_eq!(s.import(&vless_link()).unwrap_err().code, ErrorCode::KeychainAccessDenied);
        assert_eq!(s.list().len(), 1);
        assert_eq!(open(dir.path(), &mem).list().len(), 1);
        *mem.fail_save.lock().unwrap() = None;

        // Unreadable keychain at startup: nodes listed as secret-missing, and
        // adding a node must NOT overwrite the stored secrets.
        let before = mem.blob.lock().unwrap().clone();
        *mem.blob.lock().unwrap() = Some("not json".into());
        let (mut broken, warnings) = NodeStore::open(dir.path(), Box::new(mem.clone()));
        assert_eq!(warnings.len(), 1);
        assert!(broken.list()[0].secret_missing);
        assert!(broken.import(&vless_link()).is_err());
        assert_eq!(mem.blob.lock().unwrap().as_deref(), Some("not json"));
        *mem.blob.lock().unwrap() = before;
    }

    #[test]
    fn corrupt_nodes_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("nodes.json"), b"{ broken").unwrap();
        let (s, w) = NodeStore::open(dir.path(), Box::new(Arc::new(MemoryStore::default())));
        assert_eq!(w[0].code, ErrorCode::SettingsInvalid);
        assert!(s.list().is_empty());
        assert!(dir.path().join("nodes.json.corrupt").exists());
    }
}
