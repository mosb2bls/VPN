//! Small file helpers: owner-only permissions and atomic replace.

use std::io::Write;
use std::path::Path;

/// Create `dir` (and parents) with owner-only permissions on Unix.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write `data` to `path` atomically (temp file in the same dir, fsync,
/// rename), with mode 0600 on Unix.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    ensure_private_dir(dir)?;
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Move an unreadable file aside (`<name>.corrupt`) so it is kept for
/// diagnosis and a fresh file can be written.
pub fn quarantine(path: &Path) {
    let _ = std::fs::rename(path, path.with_extension("json.corrupt"));
}
