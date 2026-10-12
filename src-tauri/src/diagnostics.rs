//! `export_diagnostics`: a zip in the user's Downloads folder with everything
//! useful for troubleshooting, **redacted**: credentials (all node secrets,
//! UUID-shaped tokens, share links) and the proxy servers' hostnames / IPs.
//! Never uploaded anywhere.

use std::io::Write;
use std::path::{Path, PathBuf};

use edge_core::link::NodeConfig;
use edge_core::redact::Redactor;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::api::error::{ApiResult, AppError, ErrorCode};

pub struct Entry {
    pub name: String,
    pub text: String,
}

/// Redactor for diagnostics: secrets + every server host / SNI.
pub fn redactor(configs: &[(String, NodeConfig)]) -> Redactor {
    let mut r = Redactor::new();
    for (_, c) in configs {
        match c {
            NodeConfig::Hysteria2(h) => {
                r.add_secret(h.password.expose());
                if let Some(o) = &h.obfs_password {
                    r.add_secret(o.expose());
                }
                r.add_endpoint(&h.server);
                if let Some(s) = &h.sni {
                    r.add_endpoint(s);
                }
            }
            NodeConfig::VlessReality(v) => {
                r.add_secret(v.uuid.expose());
                r.add_secret(v.public_key.expose());
                r.add_secret(v.short_id.expose());
                r.add_endpoint(&v.server);
                r.add_endpoint(&v.sni);
            }
        }
    }
    r
}

pub fn write_zip(path: &Path, entries: &[Entry], redactor: &Redactor) -> ApiResult<()> {
    let fail = |e: String| AppError::new(ErrorCode::DiagnosticsExportFailed).with_detail(e);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(path).map_err(|e| fail(e.to_string()))?;
    let mut zip = zip::ZipWriter::new(file);
    let o = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).unix_permissions(0o600);
    for e in entries {
        zip.start_file(e.name.as_str(), o).map_err(|e| fail(e.to_string()))?;
        zip.write_all(redactor.redact(&e.text).as_bytes()).map_err(|e| fail(e.to_string()))?;
    }
    zip.finish().map_err(|e| fail(e.to_string()))?;
    Ok(())
}

pub fn downloads_dir(fallback: &Path) -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home.map(|h| h.join("Downloads")).filter(|d| d.is_dir()) {
        Some(d) => d,
        None => fallback.to_path_buf(),
    }
}

pub fn file_name() -> String {
    let t = edge_core::clock::now_rfc3339().replace([':', '-'], "").replace('T', "-").replace('Z', "");
    format!("{}-diagnostics-{t}.zip", edge_core::APP_NAME)
}

pub fn os_info() -> String {
    #[cfg(target_os = "macos")]
    {
        let v = std::process::Command::new("/usr/bin/sw_vers").output();
        v.map(|o| String::from_utf8_lossy(&o.stdout).trim().replace('\n', "; ")).unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::consts::OS.to_string()
    }
}

pub fn read_text(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn zip_contains_no_secrets_or_servers() {
        let hy = edge_core::link::parse_link("hy2://FAKE-diag-password@proxy.example.com:24443/?sni=sni.example.com").unwrap().config;
        let vl = edge_core::link::parse_link(&format!(
            "vless://00000000-0000-4000-8000-0000000000aa@203.0.113.7:443?security=reality&sni=www.example.org&pbk={}E&sid=0123abcd&flow=xtls-rprx-vision",
            "B".repeat(42)
        ))
        .unwrap()
        .config;
        let r = redactor(&[("a".into(), hy), ("b".into(), vl)]);
        let entries = vec![
            Entry { name: "log.txt".into(), text: "dial proxy.example.com:24443 auth FAKE-diag-password uuid 00000000-0000-4000-8000-0000000000aa via 203.0.113.7 sid 0123abcd".into() },
            Entry { name: "config.json".into(), text: format!("{{\"public_key\":\"{}E\",\"server_name\":\"www.example.org\"}}", "B".repeat(42)) },
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(file_name());
        write_zip(&path, &entries, &r).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut all = String::new();
        for i in 0..archive.len() {
            archive.by_index(i).unwrap().read_to_string(&mut all).unwrap();
        }
        for leak in ["FAKE-diag-password", "proxy.example.com", "203.0.113.7", "00000000-0000-4000", "0123abcd", "BBBBBBBB", "www.example.org", "sni.example.com"] {
            assert!(!all.contains(leak), "leaked {leak}: {all}");
        }
        assert!(all.contains("[redacted]") && all.contains("[host]"));
        assert!(write_zip(&path, &entries, &r).is_err(), "never overwrites an existing file");
    }
}
