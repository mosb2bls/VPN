//! Non-secret preferences in `<config dir>/settings.json`: modes, failover,
//! safety and app settings. Missing fields fall back to defaults so older
//! files keep loading.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::fsutil;
use crate::api::error::{ApiResult, AppError, ErrorCode};
use crate::api::types::{
    AppSettings, CaptureMode, DnsProfile, FailoverSettings, LogLevel, RoutingMode, SafetySettings,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub version: u32,
    pub capture_mode: CaptureMode,
    pub routing_mode: RoutingMode,
    pub dns_profile: DnsProfile,
    pub failover: FailoverSettings,
    pub safety: SafetySettings,
    pub app: AppSettings,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            version: 1,
            capture_mode: CaptureMode::SystemProxy,
            routing_mode: RoutingMode::Rule,
            dns_profile: DnsProfile::Mainland,
            failover: default_failover_settings(),
            safety: SafetySettings { kill_switch: true, direct_fallback: false, allow_lan: true },
            app: AppSettings {
                launch_at_login: false,
                auto_connect: false,
                close_to_tray: true,
                rules_auto_update: true,
                rules_update_interval_hours: 24,
                log_level: LogLevel::Info,
            },
        }
    }
}

pub fn default_failover_settings() -> FailoverSettings {
    FailoverSettings {
        enabled: true,
        fallback_node_id: None,
        probe_targets: vec![
            "https://www.gstatic.com/generate_204".into(),
            "https://cp.cloudflare.com/".into(),
        ],
        probe_interval_secs: 10,
        probe_timeout_secs: 5,
        failure_rounds: 3,
        min_dwell_secs: 300,
        return_stable_secs: 600,
        max_switches_per_hour: 4,
    }
}

pub struct PrefsStore {
    path: PathBuf,
    pub prefs: Prefs,
}

impl PrefsStore {
    pub fn open(dir: &Path) -> (Self, Vec<AppError>) {
        let path = dir.join("settings.json");
        let mut warnings = Vec::new();
        let prefs = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                fsutil::quarantine(&path);
                warnings.push(
                    AppError::new(ErrorCode::SettingsInvalid)
                        .with_detail(format!("settings.json unreadable, moved to settings.json.corrupt: {e}")),
                );
                Prefs::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Prefs::default(),
            Err(e) => {
                warnings.push(AppError::new(ErrorCode::SettingsInvalid).with_detail(e.to_string()));
                Prefs::default()
            }
        };
        (Self { path, prefs }, warnings)
    }

    /// Apply `f` to a copy, persist it, then keep it. On failure nothing changes.
    pub fn update(&mut self, f: impl FnOnce(&mut Prefs)) -> ApiResult<&Prefs> {
        let mut next = self.prefs.clone();
        f(&mut next);
        let json = serde_json::to_vec_pretty(&next)
            .map_err(|_| AppError::new(ErrorCode::Internal).with_detail("settings serialization"))?;
        fsutil::write_atomic(&self.path, &json)
            .map_err(|e| AppError::new(ErrorCode::SettingsSaveFailed).with_detail(e.to_string()))?;
        self.prefs = next;
        Ok(&self.prefs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_defaults_and_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, w) = PrefsStore::open(dir.path());
        assert!(w.is_empty());
        s.update(|p| p.routing_mode = RoutingMode::Global).unwrap();
        let (s2, _) = PrefsStore::open(dir.path());
        assert_eq!(s2.prefs.routing_mode, RoutingMode::Global);
        // A file with only some fields still loads, rest default.
        std::fs::write(dir.path().join("settings.json"), br#"{"dnsProfile":"overseas"}"#).unwrap();
        let (s3, w) = PrefsStore::open(dir.path());
        assert!(w.is_empty());
        assert_eq!(s3.prefs.dns_profile, DnsProfile::Overseas);
        assert_eq!(s3.prefs.routing_mode, RoutingMode::Rule);
        assert!(s3.prefs.safety.kill_switch);
    }

    #[test]
    fn corrupt_settings_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), b"nope").unwrap();
        let (s, w) = PrefsStore::open(dir.path());
        assert_eq!(w[0].code, ErrorCode::SettingsInvalid);
        assert!(s.prefs.failover.enabled);
    }
}
