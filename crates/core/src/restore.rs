//! Restore orchestration (REQ-4). Pure logic: platform code (app for system
//! proxy, helper for TUN/routes/DNS/IPv6/firewall) implements [`Restorer`];
//! this module runs every item in a fixed order, never stops early, and builds
//! the per-item [`RestoreReport`]. A report is `fully_restored` only if every
//! item is `restored` or `not_needed` — `failed` and `skipped` both count as
//! incomplete, so a partial failure can never look like success.

use serde::{Deserialize, Serialize};

use crate::error_code::ErrorCode;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RestoreItemKind {
    /// Helper-run sing-box TUN session (stopping it removes the TUN interface).
    TunSession,
    /// Routes added for TUN / server bypass.
    Routes,
    /// DNS servers changed on network services / interfaces.
    Dns,
    /// IPv6 settings changed on network services / interfaces.
    Ipv6,
    /// OS system proxy (macOS per network service; Windows WinINet).
    SystemProxy,
    /// Firewall kill switch (macOS pf anchor; Windows WFP / firewall rules).
    Firewall,
}

impl RestoreItemKind {
    /// Execution order. The TUN session goes first so its routes disappear;
    /// the firewall goes last so the kill switch keeps blocking proxied traffic
    /// until everything else has been put back.
    pub const ORDER: [RestoreItemKind; 6] = [
        RestoreItemKind::TunSession,
        RestoreItemKind::Routes,
        RestoreItemKind::Dns,
        RestoreItemKind::Ipv6,
        RestoreItemKind::SystemProxy,
        RestoreItemKind::Firewall,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RestoreItemKind::TunSession => "TUN session (helper)",
            RestoreItemKind::Routes => "Routes",
            RestoreItemKind::Dns => "DNS servers",
            RestoreItemKind::Ipv6 => "IPv6 settings",
            RestoreItemKind::SystemProxy => "System proxy",
            RestoreItemKind::Firewall => "Firewall / kill switch",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RestoreResult {
    /// The app had changed this item and it is now back to its previous value.
    Restored,
    /// The app had not changed this item; nothing to do.
    NotNeeded,
    /// Restoring was attempted and failed; the item is still changed.
    Failed,
    /// Could not be attempted (e.g. helper unreachable); the item may still be changed.
    Skipped,
}

impl RestoreResult {
    pub fn is_clean(self) -> bool {
        matches!(self, RestoreResult::Restored | RestoreResult::NotNeeded)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RestoreTrigger {
    Disconnect,
    RestoreNetworking,
    Quit,
    /// Switching routing mode to `direct`.
    DirectMode,
    /// Tearing down the old capture mode before starting the new one.
    CaptureModeSwitch,
    /// Launch-time / helper-start recovery from the journal.
    Recovery,
    /// The `--restore-networking` command-line flag.
    CommandLine,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RestoreItem {
    pub kind: RestoreItemKind,
    /// Display label, e.g. "System proxy".
    pub label: String,
    /// Display-ready description of what was done or found,
    /// e.g. "Restored previous proxy settings on Wi-Fi and Ethernet".
    pub action: String,
    pub result: RestoreResult,
    /// Set when `result` is `failed` or `skipped`.
    pub error_code: Option<ErrorCode>,
    /// Display-ready error message when `result` is `failed` or `skipped`.
    pub error_message: Option<String>,
}

/// What a platform restorer reports for one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Restored { action: String },
    NotNeeded { action: String },
    Failed { action: String, code: ErrorCode, detail: Option<String> },
    Skipped { action: String, code: ErrorCode },
}

/// Implemented by platform code. Must not panic; report failures as outcomes.
pub trait Restorer {
    fn restore(&mut self, kind: RestoreItemKind) -> StepOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRestoreReport {
    pub at: String,
    pub trigger: RestoreTrigger,
    pub fully_restored: bool,
    /// Exactly one entry per [`RestoreItemKind`], in [`RestoreItemKind::ORDER`].
    pub items: Vec<RestoreItem>,
}

/// Run every item in order, continuing after failures, and build the report.
pub fn run_restore(
    restorer: &mut dyn Restorer,
    trigger: RestoreTrigger,
    at: String,
) -> CoreRestoreReport {
    let items: Vec<RestoreItem> = RestoreItemKind::ORDER
        .iter()
        .map(|&kind| {
            let (action, result, code) = match restorer.restore(kind) {
                StepOutcome::Restored { action } => (action, RestoreResult::Restored, None),
                StepOutcome::NotNeeded { action } => (action, RestoreResult::NotNeeded, None),
                StepOutcome::Failed { action, code, .. } => (action, RestoreResult::Failed, Some(code)),
                StepOutcome::Skipped { action, code } => (action, RestoreResult::Skipped, Some(code)),
            };
            RestoreItem {
                kind,
                label: kind.label().into(),
                action,
                result,
                error_code: code,
                error_message: code.map(|c| c.message().into()),
            }
        })
        .collect();
    let fully_restored = items.iter().all(|i| i.result.is_clean());
    CoreRestoreReport { at, trigger, fully_restored, items }
}

/// OS state that is actually in effect, for `Status` (REQ-4: report what is
/// true, not what was intended).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeldState {
    pub kill_switch_active: bool,
    pub system_proxy_set: bool,
    pub tun_active: bool,
}

impl HeldState {
    /// State after a restore, given what was held before it. An item that was
    /// held stays held unless its restore result is clean.
    pub fn after(self, report: &CoreRestoreReport) -> HeldState {
        let clean = |k: RestoreItemKind| {
            report.items.iter().find(|i| i.kind == k).is_some_and(|i| i.result.is_clean())
        };
        HeldState {
            kill_switch_active: self.kill_switch_active && !clean(RestoreItemKind::Firewall),
            system_proxy_set: self.system_proxy_set && !clean(RestoreItemKind::SystemProxy),
            tun_active: self.tun_active && !clean(RestoreItemKind::TunSession),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Fake restorer: every item "restored" unless overridden.
    struct Fake {
        outcomes: HashMap<RestoreItemKind, StepOutcome>,
        calls: Vec<RestoreItemKind>,
    }

    impl Fake {
        fn all_restored() -> Self {
            Self { outcomes: HashMap::new(), calls: vec![] }
        }
        fn with(mut self, k: RestoreItemKind, o: StepOutcome) -> Self {
            self.outcomes.insert(k, o);
            self
        }
    }

    impl Restorer for Fake {
        fn restore(&mut self, kind: RestoreItemKind) -> StepOutcome {
            self.calls.push(kind);
            self.outcomes
                .get(&kind)
                .cloned()
                .unwrap_or(StepOutcome::Restored { action: "restored".into() })
        }
    }

    const AT: &str = "2026-10-11T00:00:00Z";

    fn dns_fails() -> StepOutcome {
        StepOutcome::Failed {
            action: "Restore DNS servers on Wi-Fi".into(),
            code: ErrorCode::DnsRestoreFailed,
            detail: Some("networksetup exited with status 4".into()),
        }
    }

    #[test]
    fn all_items_restored_is_full() {
        let r = run_restore(&mut Fake::all_restored(), RestoreTrigger::Disconnect, AT.into());
        assert!(r.fully_restored);
        assert_eq!(r.items.len(), 6);
        assert!(r.items.iter().all(|i| i.error_code.is_none()));
    }

    #[test]
    fn not_needed_counts_as_clean() {
        let mut f = Fake::all_restored()
            .with(RestoreItemKind::TunSession, StepOutcome::NotNeeded { action: "No TUN session".into() });
        assert!(run_restore(&mut f, RestoreTrigger::Disconnect, AT.into()).fully_restored);
    }

    #[test]
    fn failing_dns_restore_is_incomplete_and_reported_per_item() {
        let mut f = Fake::all_restored().with(RestoreItemKind::Dns, dns_fails());
        let r = run_restore(&mut f, RestoreTrigger::RestoreNetworking, AT.into());
        assert!(!r.fully_restored);
        let dns = r.items.iter().find(|i| i.kind == RestoreItemKind::Dns).unwrap();
        assert_eq!(dns.result, RestoreResult::Failed);
        assert_eq!(dns.error_code, Some(ErrorCode::DnsRestoreFailed));
        assert!(dns.error_message.is_some());
        // Other items are still reported individually and as restored.
        let others: Vec<_> = r.items.iter().filter(|i| i.kind != RestoreItemKind::Dns).collect();
        assert_eq!(others.len(), 5);
        assert!(others.iter().all(|i| i.result == RestoreResult::Restored));
    }

    #[test]
    fn a_failure_does_not_stop_later_items() {
        let mut f = Fake::all_restored().with(RestoreItemKind::TunSession, StepOutcome::Failed {
            action: "Stop TUN session".into(),
            code: ErrorCode::HelperUnreachable,
            detail: None,
        });
        run_restore(&mut f, RestoreTrigger::Disconnect, AT.into());
        assert_eq!(f.calls, RestoreItemKind::ORDER.to_vec());
    }

    #[test]
    fn skipped_counts_as_incomplete() {
        let mut f = Fake::all_restored().with(RestoreItemKind::Firewall, StepOutcome::Skipped {
            action: "Remove pf anchor".into(),
            code: ErrorCode::HelperUnreachable,
        });
        let r = run_restore(&mut f, RestoreTrigger::Quit, AT.into());
        assert!(!r.fully_restored);
        assert_eq!(r.items.last().unwrap().result, RestoreResult::Skipped);
    }

    #[test]
    fn firewall_is_restored_last() {
        assert_eq!(RestoreItemKind::ORDER.last(), Some(&RestoreItemKind::Firewall));
        assert_eq!(RestoreItemKind::ORDER.first(), Some(&RestoreItemKind::TunSession));
    }

    #[test]
    fn held_state_reflects_truth_after_partial_failure() {
        let before = HeldState { kill_switch_active: true, system_proxy_set: true, tun_active: true };
        let mut f = Fake::all_restored()
            .with(RestoreItemKind::Firewall, StepOutcome::Failed {
                action: "Remove pf anchor".into(),
                code: ErrorCode::KillSwitchRemoveFailed,
                detail: None,
            })
            .with(RestoreItemKind::SystemProxy, StepOutcome::Skipped {
                action: "Restore proxy".into(),
                code: ErrorCode::SystemProxyRestoreFailed,
            });
        let after = before.after(&run_restore(&mut f, RestoreTrigger::Disconnect, AT.into()));
        assert!(after.kill_switch_active, "kill switch is still in effect");
        assert!(after.system_proxy_set, "proxy was not restored");
        assert!(!after.tun_active, "TUN session was stopped");
    }

    #[test]
    fn held_state_never_turns_on() {
        let before = HeldState::default();
        let mut f = Fake::all_restored().with(RestoreItemKind::Dns, dns_fails());
        assert_eq!(before.after(&run_restore(&mut f, RestoreTrigger::Disconnect, AT.into())), before);
    }
}
