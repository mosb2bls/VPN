//! Small helpers: hashing, private atomic writes, logging.
//! Unix: explicit modes. Windows: files inherit the protected DACL of the
//! helper data directory (set at install, `edge_winsys::security::protect_dir`).

use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(unix)]
pub fn ensure_dir(dir: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode))
}

/// Directory readable only by the helper (0700 / inherited protected DACL).
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        ensure_dir(dir, 0o700)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Atomic write of a helper-private file (0600 / inherited protected DACL).
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    write_atomic(path, data, 0o600)
}

/// Atomic write; `mode` applies on Unix only.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(mode);
        }
        #[cfg(not(unix))]
        let _ = mode;
        let mut f = opts.open(&tmp)?;
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

/// Redacted log line to the helper log (helper-private file) and stderr.
pub fn log(msg: &str) {
    let red = REDACTOR.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).redact(msg);
    let line = format!("{} {}\n", edge_core::clock::now_rfc3339(), red.trim_end());
    eprint!("{line}");
    if let Some(p) = LOG.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if std::fs::metadata(p).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
            let _ = std::fs::rename(p, p.with_extension("log.1"));
        }
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        if let Ok(mut f) = opts.open(p) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}
