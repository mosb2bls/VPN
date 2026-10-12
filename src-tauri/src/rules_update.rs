//! Rule-set updates (brief §7.3): download the newest `geosite-cn` /
//! `geoip-cn` from SagerNet's official `rule-set` branches, validate, swap
//! atomically; on any failure keep the current rules and report a non-fatal
//! error. sing-box reloads local rule-set files when they change (since 1.10),
//! so a running core picks the new files up without a restart.
//!
//! Validation: size bounds; sing-box `rule-set match` sanity (CN sites / IPs
//! match, Google / 8.8.8.8 do not); `sing-box check` of a full config using
//! the new files. Downloads go through the local proxy when a system-proxy
//! session is up (GitHub may be slow / blocked directly).

use std::path::{Path, PathBuf};
use std::time::Duration;

use edge_core::singbox::{self, Capture, DnsProfile, GenInput, Routing, RuleSets};
use serde_json::Value;

use crate::api::error::{AppError, ErrorCode};
use crate::api::types::RulesSource;
use crate::rules::{self, Manifest, SetEntry};
use crate::store::fsutil;

const MAX_BYTES: usize = 8 * 1024 * 1024;

struct Source {
    name: &'static str,
    repo: &'static str,
    file: &'static str,
    must_match: &'static [&'static str],
    must_not_match: &'static [&'static str],
}

const SOURCES: [Source; 2] = [
    Source {
        name: "geosite-cn",
        repo: "SagerNet/sing-geosite",
        file: "geosite-cn.srs",
        must_match: &["www.baidu.com", "qq.com"],
        must_not_match: &["www.google.com"],
    },
    Source {
        name: "geoip-cn",
        repo: "SagerNet/sing-geoip",
        file: "geoip-cn.srs",
        must_match: &["114.114.114.114", "223.5.5.5"],
        must_not_match: &["8.8.8.8"],
    },
];

pub struct Outcome {
    pub changed: bool,
    pub manifest: Manifest,
}

fn agent(proxy_port: Option<u16>) -> Result<ureq::Agent, AppError> {
    let proxy = match proxy_port {
        Some(p) => Some(
            ureq::Proxy::new(&format!("http://127.0.0.1:{p}"))
                .map_err(|e| AppError::new(ErrorCode::RulesDownloadFailed).with_detail(e.to_string()))?,
        ),
        None => None,
    };
    Ok(ureq::Agent::config_builder()
        .proxy(proxy)
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(format!("{}/{}", edge_core::APP_NAME, env!("CARGO_PKG_VERSION")))
        .build()
        .into())
}

fn dl_err(e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::RulesDownloadFailed).with_detail(e.to_string())
}

fn invalid(e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::RulesValidationFailed).with_detail(e.to_string())
}

/// Check, download, validate and install. Blocking.
pub fn update(bin: &Path, active: &Path, proxy_port: Option<u16>) -> Result<Outcome, AppError> {
    let agent = agent(proxy_port)?;
    let current = rules::active_manifest(active);
    let now = edge_core::clock::now_rfc3339();
    let staging = active.join("staging");
    let _ = std::fs::remove_dir_all(&staging);
    fsutil::ensure_private_dir(&staging).map_err(dl_err)?;

    let mut new_sets = Vec::new();
    for src in &SOURCES {
        let branch: Value = agent
            .get(format!("https://api.github.com/repos/{}/branches/rule-set", src.repo))
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(dl_err)?
            .body_mut()
            .read_json()
            .map_err(dl_err)?;
        let commit = branch["commit"]["sha"].as_str().unwrap_or_default().to_string();
        let date = branch["commit"]["commit"]["committer"]["date"].as_str().unwrap_or_default().to_string();
        if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(dl_err("unexpected branch response"));
        }
        let bytes = agent
            .get(format!("https://raw.githubusercontent.com/{}/{commit}/{}", src.repo, src.file))
            .call()
            .map_err(dl_err)?
            .body_mut()
            .with_config()
            .limit(MAX_BYTES as u64)
            .read_to_vec()
            .map_err(dl_err)?;
        if bytes.len() < 1024 {
            return Err(invalid(format!("{}: file too small", src.name)));
        }
        std::fs::write(staging.join(src.file), &bytes).map_err(dl_err)?;
        new_sets.push(SetEntry {
            name: src.name.into(),
            path: src.file.into(),
            size: bytes.len() as u32,
            sha256: rules::sha256_hex(&bytes),
            committed_at: date,
            repo: src.repo.into(),
            commit,
        });
    }

    let unchanged = new_sets
        .iter()
        .all(|n| current.sets.iter().any(|c| c.name == n.name && c.sha256 == n.sha256));
    if unchanged {
        let _ = std::fs::remove_dir_all(&staging);
        let mut m = current;
        m.last_check_at = Some(now);
        rules::save_active_manifest(active, &m)?;
        return Ok(Outcome { changed: false, manifest: m });
    }

    validate(bin, &staging)?;
    install(active, &staging, &new_sets)?;
    let version = new_sets.iter().map(|s| s.committed_at.get(..10).unwrap_or("").replace('-', ".")).max().unwrap_or_default();
    let m = Manifest {
        version,
        sets: new_sets,
        source: Some(RulesSource::Downloaded),
        updated_at: Some(now.clone()),
        last_check_at: Some(now),
    };
    rules::save_active_manifest(active, &m)?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(Outcome { changed: true, manifest: m })
}

fn validate(bin: &Path, staging: &Path) -> Result<(), AppError> {
    for src in &SOURCES {
        let file = staging.join(src.file);
        for (probe, expect) in src.must_match.iter().map(|p| (p, true)).chain(src.must_not_match.iter().map(|p| (p, false))) {
            let out = std::process::Command::new(bin)
                .args(["rule-set", "match", "-f", "binary"])
                .arg(&file)
                .arg(probe)
                .output()
                .map_err(invalid)?;
            if !out.status.success() {
                return Err(invalid(format!("{}: unreadable rule-set", src.name)));
            }
            // sing-box prints the match result on stderr (verified 1.14.3).
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            let matched = text.contains("match rules");
            if matched != expect {
                return Err(invalid(format!("{}: sanity check failed for {probe}", src.name)));
            }
        }
    }
    // Full config check with the new files.
    let node = edge_core::link::parse_link("hy2://check@203.0.113.1:443").expect("static link").config;
    let cfg = singbox::generate(&GenInput {
        nodes: vec![("check", &node)],
        selected: "check",
        routing: Routing::Rule,
        dns: DnsProfile::Mainland,
        capture: Capture::SystemProxy { port: 1 },
        allow_lan: true,
        controller_port: 2,
        controller_secret: "check",
        rule_sets: RuleSets { geosite_cn: staging.join("geosite-cn.srs"), geoip_cn: staging.join("geoip-cn.srs") },
        direct_fallback: false,
    })
    .map_err(|e| invalid(format!("{e:?}")))?;
    let path = staging.join("check.json");
    std::fs::write(&path, serde_json::to_vec(&cfg).expect("config")).map_err(invalid)?;
    let out = std::process::Command::new(bin).args(["check", "-c"]).arg(&path).output().map_err(invalid)?;
    if !out.status.success() {
        return Err(invalid(format!("sing-box check: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(())
}

/// Swap staged files in; on failure put the previous files back.
fn install(active: &Path, staging: &Path, sets: &[SetEntry]) -> Result<(), AppError> {
    let mut backed_up: Vec<(PathBuf, PathBuf)> = Vec::new();
    for s in sets {
        let dest = active.join(&s.path);
        let prev = active.join(format!("{}.prev", s.path));
        if dest.exists() {
            std::fs::copy(&dest, &prev).map_err(invalid)?;
            backed_up.push((prev, dest.clone()));
        }
    }
    for s in sets {
        if let Err(e) = std::fs::rename(staging.join(&s.path), active.join(&s.path)) {
            for (prev, dest) in &backed_up {
                let _ = std::fs::rename(prev, dest);
            }
            return Err(invalid(format!("swap failed: {e}")));
        }
    }
    for (prev, _) in backed_up {
        let _ = std::fs::remove_file(prev);
    }
    Ok(())
}

/// Next due time for the scheduler: last check + interval.
pub fn due(last_check: Option<&str>, interval_hours: u32, now_unix: u64) -> bool {
    let Some(last) = last_check.and_then(parse_rfc3339) else { return true };
    now_unix >= last + interval_hours as u64 * 3600
}

/// Minimal parser for our own `YYYY-MM-DDTHH:MM:SSZ` timestamps.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20 || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    // days from civil (inverse of clock::civil_from_days)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hh * 3600 + mm * 60 + ss) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opt-in: real download from GitHub into a temp dir, full validation.
    /// Run: `cargo test -p edge-app --lib real_rules_update -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_rules_update() {
        let bin = crate::core_bin::locate_verified().expect("core fetched");
        let active = tempfile::tempdir().unwrap();
        let bundle = rules::bundled_dir(None);
        rules::ensure_active(&rules::embedded_manifest(), &bundle, active.path()).expect("bundle");
        let o = update(&bin, active.path(), None).expect("update");
        println!("changed={} version={} sets={:?}", o.changed, o.manifest.version,
            o.manifest.sets.iter().map(|s| (&s.name, &s.commit[..7], s.size)).collect::<Vec<_>>());
        let again = update(&bin, active.path(), None).expect("second update");
        assert!(!again.changed, "second check finds nothing new");
        let r = rules::ensure_active(&rules::embedded_manifest(), &bundle, active.path()).unwrap();
        assert!(r.geosite_cn.exists());
        assert_eq!(rules::active_manifest(active.path()).source, Some(RulesSource::Downloaded).filter(|_| o.changed).or(Some(RulesSource::Bundled)));
    }

    #[test]
    fn timestamps_and_due() {
        let t = edge_core::clock::format_unix(1_791_763_200 + 3_723);
        assert_eq!(parse_rfc3339(&t), Some(1_791_763_200 + 3_723));
        assert!(due(None, 24, 0));
        assert!(!due(Some(&t), 24, 1_791_763_200 + 3_723 + 3600));
        assert!(due(Some(&t), 24, 1_791_763_200 + 3_723 + 24 * 3600));
        assert!(due(Some("garbage"), 24, 0));
    }

    /// Real sing-box: the bundled rule-sets pass validation; a corrupted file
    /// fails it; a failed swap leaves the old files in place.
    #[test]
    fn validation_and_swap() {
        let Ok(bin) = crate::core_bin::locate_verified() else {
            eprintln!("skipped: core not fetched");
            return;
        };
        let bundle = rules::bundled_dir(None);
        if !bundle.join("geosite-cn.srs").exists() {
            eprintln!("skipped: rules not fetched");
            return;
        }
        let staging = tempfile::tempdir().unwrap();
        for f in ["geosite-cn.srs", "geoip-cn.srs"] {
            std::fs::copy(bundle.join(f), staging.path().join(f)).unwrap();
        }
        validate(&bin, staging.path()).expect("bundled rules validate");
        // Swap geosite with geoip content → sanity checks must fail.
        std::fs::copy(bundle.join("geoip-cn.srs"), staging.path().join("geosite-cn.srs")).unwrap();
        assert_eq!(validate(&bin, staging.path()).unwrap_err().code, ErrorCode::RulesValidationFailed);
        std::fs::write(staging.path().join("geoip-cn.srs"), b"not a rule set").unwrap();
        assert_eq!(validate(&bin, staging.path()).unwrap_err().code, ErrorCode::RulesValidationFailed);

        // Swap: missing staged file → previous files restored.
        let active = tempfile::tempdir().unwrap();
        std::fs::write(active.path().join("geosite-cn.srs"), b"old-site").unwrap();
        std::fs::write(active.path().join("geoip-cn.srs"), b"old-ip").unwrap();
        let empty = tempfile::tempdir().unwrap();
        let entry = |n: &str, p: &str| SetEntry {
            name: n.into(),
            path: p.into(),
            size: 0,
            sha256: String::new(),
            committed_at: String::new(),
            repo: String::new(),
            commit: String::new(),
        };
        let sets = [entry("geosite-cn", "geosite-cn.srs"), entry("geoip-cn", "geoip-cn.srs")];
        assert!(install(active.path(), empty.path(), &sets).is_err());
        assert_eq!(std::fs::read(active.path().join("geosite-cn.srs")).unwrap(), b"old-site");
        assert_eq!(std::fs::read(active.path().join("geoip-cn.srs")).unwrap(), b"old-ip");
    }
}
