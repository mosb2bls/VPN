//! Manual node form validation (`add_node_manual` / `update_node`), using the
//! same field rules as link import (`edge_core::validate`).

use edge_core::validate::{self, NAME_MAX_CHARS};

use super::secrets::NodeSecrets;
use super::StoredParams;
use crate::api::error::{ErrorCode, FieldError};
use crate::api::types::{Hy2Form, NodeForm, SecretInput, VlessForm};

pub struct Validated {
    pub name: String,
    pub params: StoredParams,
    pub secrets: NodeSecrets,
}

/// `existing` = stored secrets of the node being updated (None on add, or if
/// the protocol changes).
pub fn validate_form(form: &NodeForm, existing: Option<&NodeSecrets>) -> Result<Validated, Vec<FieldError>> {
    let mut errs = Vec::new();
    let v = match form {
        NodeForm::Hysteria2(f) => hy2(f, existing, &mut errs),
        NodeForm::VlessReality(f) => vless(f, existing, &mut errs),
    };
    if errs.is_empty() { Ok(v) } else { Err(errs) }
}

fn check<T>(errs: &mut Vec<FieldError>, field: &str, r: Result<T, ErrorCode>) -> Option<T> {
    r.map_err(|c| errs.push(FieldError::new(field, c))).ok()
}

fn name(errs: &mut Vec<FieldError>, raw: &str) -> String {
    let n = validate::normalize_name(raw);
    if n.is_empty() {
        errs.push(FieldError::new("name", ErrorCode::FieldRequired));
    } else if raw.trim().chars().count() > NAME_MAX_CHARS {
        errs.push(FieldError::new("name", ErrorCode::FieldTooLong));
    }
    n
}

/// Resolve a secret input. `required`: must end up present (may be "" only if
/// `allow_empty`). Returns the resulting value (None = absent).
fn secret(
    errs: &mut Vec<FieldError>,
    field: &str,
    input: &SecretInput,
    existing: Option<&String>,
    required: bool,
    allow_empty: bool,
    validate: impl Fn(&str) -> Result<String, ErrorCode>,
) -> Option<String> {
    match input {
        SecretInput::Keep => match existing {
            Some(v) => Some(v.clone()),
            None if required => {
                errs.push(FieldError::new(field, ErrorCode::FieldRequired));
                None
            }
            None => None,
        },
        SecretInput::Clear if required => {
            errs.push(FieldError::new(field, ErrorCode::FieldCannotClear));
            None
        }
        SecretInput::Clear => None,
        SecretInput::Set(v) if v.is_empty() && !allow_empty => {
            errs.push(FieldError::new(field, ErrorCode::FieldRequired));
            None
        }
        SecretInput::Set(v) => check(errs, field, validate(v)),
    }
}

fn check_password(v: &str) -> Result<String, ErrorCode> {
    if v.chars().count() > 512 { Err(ErrorCode::FieldTooLong) } else { Ok(v.to_string()) }
}

fn opt_bandwidth(errs: &mut Vec<FieldError>, field: &str, v: Option<u32>) -> Option<u32> {
    v.and_then(|x| check(errs, field, validate::validate_bandwidth(&x.to_string())))
}

fn hy2(f: &Hy2Form, existing: Option<&NodeSecrets>, errs: &mut Vec<FieldError>) -> Validated {
    let name = name(errs, &f.name);
    let server = check(errs, "server", validate::validate_server(&f.server)).unwrap_or_default();
    let port = check(errs, "port", validate::validate_port(&f.port.to_string())).unwrap_or(f.port);
    let mut server_ports = Vec::new();
    if !f.server_ports.is_empty()
        && let Some(list) = check(errs, "serverPorts", validate::parse_port_list(&f.server_ports.join(",")))
    {
        server_ports = list.into_iter().map(|r| r.to_display()).collect();
    }
    let sni = match f.sni.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => check(errs, "sni", validate::validate_sni(s)),
    };
    if f.pin_sha256.as_deref().is_some_and(|p| !p.trim().is_empty()) {
        // Not supported with the pinned sing-box (SPEC §2.1.1).
        errs.push(FieldError::new("pinSha256", ErrorCode::FieldInvalid));
    }
    let up_mbps = opt_bandwidth(errs, "upMbps", f.up_mbps);
    let down_mbps = opt_bandwidth(errs, "downMbps", f.down_mbps);
    let ex = existing.cloned().unwrap_or_default();
    let password = secret(errs, "password", &f.password, ex.password.as_ref(), true, false, check_password);
    let obfs_password =
        secret(errs, "obfsPassword", &f.obfs_password, ex.obfs_password.as_ref(), false, false, check_password);
    Validated {
        name,
        params: StoredParams::Hysteria2 {
            server,
            port,
            server_ports,
            sni,
            insecure: f.insecure,
            obfs: obfs_password.is_some(),
            up_mbps,
            down_mbps,
        },
        secrets: NodeSecrets { password, obfs_password, ..Default::default() },
    }
}

fn vless(f: &VlessForm, existing: Option<&NodeSecrets>, errs: &mut Vec<FieldError>) -> Validated {
    let name = name(errs, &f.name);
    let server = check(errs, "server", validate::validate_server(&f.server)).unwrap_or_default();
    let port = check(errs, "port", validate::validate_port(&f.port.to_string())).unwrap_or(f.port);
    let sni = if f.sni.trim().is_empty() {
        errs.push(FieldError::new("sni", ErrorCode::FieldRequired));
        String::new()
    } else {
        check(errs, "sni", validate::validate_sni(&f.sni)).unwrap_or_default()
    };
    let fingerprint = check(errs, "fingerprint", validate::validate_fingerprint(&f.fingerprint)).unwrap_or_default();
    // Not supported by the sing-box core (SPEC §2.1.1): must be null/empty.
    if f.spider_x.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        errs.push(FieldError::new("spiderX", ErrorCode::FieldInvalid));
    }
    let spider_x = None;
    let ex = existing.cloned().unwrap_or_default();
    let uuid = secret(errs, "uuid", &f.uuid, ex.uuid.as_ref(), true, false, validate::validate_uuid);
    let public_key =
        secret(errs, "publicKey", &f.public_key, ex.public_key.as_ref(), true, false, validate::validate_public_key);
    let short_id = secret(errs, "shortId", &f.short_id, ex.short_id.as_ref(), true, true, validate::validate_short_id);
    Validated {
        name,
        params: StoredParams::VlessReality { server, port, sni, fingerprint, spider_x, alpn: vec![] },
        secrets: NodeSecrets { uuid, public_key, short_id, ..Default::default() },
    }
}
