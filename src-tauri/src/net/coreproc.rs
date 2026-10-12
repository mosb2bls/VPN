//! Running the bundled sing-box. The config (which contains credentials) is
//! passed on **stdin** (`run -c stdin`) and never written to disk. Output lines
//! are redacted and logged.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

use crate::api::error::{AppError, ErrorCode};
use crate::api::types::LogSource;
use crate::net::logbuf;

pub struct CoreProcess {
    child: Child,
    pub pid: u32,
    /// Last few redacted output lines (for error details).
    pub tail: Arc<Mutex<Vec<String>>>,
}

impl CoreProcess {
    pub fn spawn(bin: &Path, config: &serde_json::Value, workdir: &Path) -> Result<Self, AppError> {
        crate::store::fsutil::ensure_private_dir(workdir)
            .map_err(|e| AppError::new(ErrorCode::CoreStartFailed).with_detail(e.to_string()))?;
        let mut child = Command::new(bin)
            .args(["run", "-c", "stdin", "--disable-color", "-D"])
            .arg(workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::new(ErrorCode::CoreStartFailed).with_detail(e.to_string()))?;
        let pid = child.id();
        {
            let mut stdin = child.stdin.take().expect("piped stdin");
            let bytes = serde_json::to_vec(config).expect("config serializes");
            if let Err(e) = stdin.write_all(&bytes) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::new(ErrorCode::CoreStartFailed).with_detail(format!("writing config: {e}")));
            }
            // stdin dropped here → EOF, sing-box parses the config.
        }
        let tail = Arc::new(Mutex::new(Vec::new()));
        for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
                       child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
            .into_iter()
            .flatten()
        {
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let log = logbuf::global();
                    log.push(logbuf::core_level(&line), LogSource::Core, &line);
                    let mut t = tail.lock().unwrap_or_else(|p| p.into_inner());
                    if t.len() == 20 {
                        t.remove(0);
                    }
                    t.push(log.redact(&line));
                }
            });
        }
        logbuf::app(crate::api::types::LogLevel::Info, format!("sing-box started (pid {pid})"));
        Ok(Self { child, pid, tail })
    }

    pub fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Last redacted output lines, joined (for error details).
    pub fn tail_text(&self) -> String {
        self.tail.lock().unwrap_or_else(|p| p.into_inner()).join("\n")
    }

    pub fn stop(&mut self) {
        if self.try_wait().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            logbuf::app(crate::api::types::LogLevel::Info, format!("sing-box stopped (pid {})", self.pid));
        }
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Kill a sing-box left over from a crashed app, but only if `pid` is still a
/// process running exactly `exe` (guards against PID reuse).
pub fn kill_stale(pid: u32, exe: &Path) -> bool {
    #[cfg(unix)]
    {
        let out = Command::new("/bin/ps").args(["-o", "comm=", "-p", &pid.to_string()]).output();
        let running = out.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        if !running.is_empty() && Path::new(&running) == exe {
            let _ = Command::new("/bin/kill").args(["-KILL", &pid.to_string()]).status();
            logbuf::app(crate::api::types::LogLevel::Warn, format!("killed stale sing-box (pid {pid})"));
            return true;
        }
        false
    }
    #[cfg(not(unix))]
    {
        // Windows: implemented with the Windows phase (B3).
        let _ = (pid, exe);
        false
    }
}
