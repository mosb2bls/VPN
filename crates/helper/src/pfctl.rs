//! pf control via /sbin/pfctl (argv only). The kill switch lives in its own
//! anchor under `com.apple/*`; pf is enabled with a reference token (`-E`) and
//! released with `-X <token>` so other users of pf are unaffected.

use std::io::Write;
use std::process::{Command, Stdio};

use edge_core::pf::ANCHOR;

const PFCTL: &str = "/sbin/pfctl";

fn run(args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    let mut child = Command::new(PFCTL)
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(input) = stdin {
        child.stdin.take().expect("piped").write_all(input.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() { Ok(text) } else { Err(text.trim().to_string()) }
}

/// Load the anchor rules and take a pf enable reference. Returns the token.
pub fn engage(rules: &str) -> Result<String, String> {
    run(&["-a", ANCHOR, "-f", "-"], Some(rules))?;
    let out = run(&["-E"], None)?;
    out.lines()
        .find_map(|l| l.split("Token :").nth(1).map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("no enable token in pfctl output: {out}"))
}

/// Flush our anchor and release our enable reference (if any).
pub fn release(token: Option<&str>) -> Result<(), String> {
    let flushed = run(&["-a", ANCHOR, "-F", "all"], None);
    if let Some(t) = token {
        run(&["-X", t], None)?;
    }
    flushed.map(|_| ())
}

/// Rules currently in our anchor (empty if none).
pub fn current_rules() -> String {
    run(&["-a", ANCHOR, "-s", "rules"], None).unwrap_or_default()
}
