//! Small helpers: hashing, root-owned atomic writes, logging.

use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

pub fn ensure_dir(dir: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode))
}

pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

static LOG: OnceLock<Mutex<Option<std::path::PathBuf>>> = OnceLock::new();
static REDACTOR: OnceLock<Mutex<edge_core::redact::Redactor>> = OnceLock::new();

pub fn set_log_file(path: &Path) {
    *LOG.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()) = Some(path.to_path_buf());
}

pub fn set_redactor(r: edge_core::redact::Redactor) {
    *REDACTOR.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()) = r;
}

/// Redacted log line to the helper log (root-only file) and stderr.
pub fn log(msg: &str) {
    let red = REDACTOR.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).redact(msg);
    let line = format!("{} {}\n", edge_core::clock::now_rfc3339(), red.trim_end());
    eprint!("{line}");
    if let Some(p) = LOG.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        use std::os::unix::fs::OpenOptionsExt;
        if std::fs::metadata(p).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
            let _ = std::fs::rename(p, p.with_extension("log.1"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(p) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}
