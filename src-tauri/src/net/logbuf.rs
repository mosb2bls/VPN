//! Process-wide log: every line is redacted, kept in a 2000-entry ring buffer
//! (for `get_logs`) and appended to a rotating file `<data>/logs/edgeclient.log`
//! (1 MiB × 4 files, mode 0600).

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};

use edge_core::redact::Redactor;

use crate::api::types::{LogEntry, LogLevel, LogSource};

const CAPACITY: usize = 2000;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const KEEP_FILES: usize = 3;

pub struct LogBuf {
    entries: Mutex<VecDeque<LogEntry>>,
    file: Mutex<Option<PathBuf>>,
    redactor: RwLock<Redactor>,
}

pub fn global() -> &'static LogBuf {
    static LOG: OnceLock<LogBuf> = OnceLock::new();
    LOG.get_or_init(|| LogBuf {
        entries: Mutex::new(VecDeque::with_capacity(CAPACITY)),
        file: Mutex::new(None),
        redactor: RwLock::new(Redactor::new()),
    })
}

impl LogBuf {
    /// Start writing to `<dir>/edgeclient.log`.
    pub fn set_file_dir(&self, dir: &Path) {
        if crate::store::fsutil::ensure_private_dir(dir).is_ok() {
            *self.file.lock().unwrap_or_else(|p| p.into_inner()) = Some(dir.join("edgeclient.log"));
        }
    }

    /// Replace the set of secrets to redact (call whenever nodes / session change).
    pub fn set_redactor(&self, r: Redactor) {
        *self.redactor.write().unwrap_or_else(|p| p.into_inner()) = r;
    }

    /// Register extra secrets (e.g. the Clash API secret) without dropping others.
    pub fn add_secret(&self, s: &str) {
        self.redactor.write().unwrap_or_else(|p| p.into_inner()).add_secret(s);
    }

    pub fn redact(&self, text: &str) -> String {
        self.redactor.read().unwrap_or_else(|p| p.into_inner()).redact(text)
    }

    pub fn push(&self, level: LogLevel, source: LogSource, message: &str) {
        let entry = LogEntry {
            ts: edge_core::clock::now_rfc3339(),
            level,
            source,
            message: self.redact(message.trim_end()),
        };
        if let Some(path) = self.file.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            write_line(path, &entry);
        }
        let mut e = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        if e.len() == CAPACITY {
            e.pop_front();
        }
        e.push_back(entry);
    }

    /// Newest `limit` (≤ 1000) entries at or above `min_level`, oldest first.
    pub fn recent(&self, limit: u32, min_level: Option<LogLevel>) -> Vec<LogEntry> {
        let limit = limit.min(1000) as usize;
        let e = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let mut v: Vec<LogEntry> =
            e.iter().rev().filter(|x| min_level.is_none_or(|m| x.level >= m)).take(limit).cloned().collect();
        v.reverse();
        v
    }
}

fn write_line(path: &Path, e: &LogEntry) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_FILE_BYTES) {
        for i in (1..KEEP_FILES).rev() {
            let _ = std::fs::rename(path.with_extension(format!("log.{i}")), path.with_extension(format!("log.{}", i + 1)));
        }
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    if let Ok(mut f) = opts.open(path) {
        let _ = writeln!(f, "{} {:?} {:?} {}", e.ts, e.level, e.source, e.message);
    }
}

/// Convenience: app-level log line.
pub fn app(level: LogLevel, msg: impl AsRef<str>) {
    global().push(level, LogSource::App, msg.as_ref());
}

/// Map a sing-box output line to a level by its prefix (INFO / WARN / ERROR / FATAL / DEBUG).
pub fn core_level(line: &str) -> LogLevel {
    let l = line.to_ascii_uppercase();
    if l.contains("FATAL") || l.contains("ERROR") {
        LogLevel::Error
    } else if l.contains("WARN") {
        LogLevel::Warn
    } else if l.contains("DEBUG") || l.contains("TRACE") {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_redacts_filters_and_writes_file() {
        let log = LogBuf {
            entries: Mutex::new(VecDeque::new()),
            file: Mutex::new(None),
            redactor: RwLock::new(Redactor::new()),
        };
        let dir = tempfile::tempdir().unwrap();
        log.set_file_dir(dir.path());
        log.add_secret("FAKE-secret-value");
        log.push(LogLevel::Info, LogSource::Core, "using FAKE-secret-value now");
        log.push(LogLevel::Error, LogSource::App, "boom");
        let all = log.recent(10, None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].message, "using [redacted] now");
        assert_eq!(log.recent(10, Some(LogLevel::Warn)).len(), 1);
        let file = std::fs::read_to_string(dir.path().join("edgeclient.log")).unwrap();
        assert!(!file.contains("FAKE-secret-value"));
        assert!(file.contains("[redacted]"));
    }
}
