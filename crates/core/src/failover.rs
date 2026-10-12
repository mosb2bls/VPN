//! Automatic failover decisions (brief §7.8). Pure: fed with probe rounds and
//! a clock (seconds), returns decisions; the app performs the switch through
//! the sing-box selector (Clash API) and emits events.
//!
//! - Only **node-level probe rounds** count. A round = HTTP probes through the
//!   node to ≥ 2 independent targets; it fails only if **all** targets fail.
//!   One website failing, or user traffic errors, can never trigger a switch.
//! - Primary → fallback after `failure_rounds` consecutive failed rounds
//!   (default 3 × 10 s ≈ 30 s).
//! - Anti-flapping: no switch within `min_dwell` of the previous one (default
//!   5 min), at most `max_switches_per_hour`, and return to the primary only
//!   after it passes **continuously** for `return_stable` (default 10 min).
//! - Exception ("rescue"): if the node in use is failing and the other node is
//!   passing, switch even inside the dwell time (still rate-limited) — staying
//!   on a dead node is worse than a flap.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Hysteria2,
    VlessReality,
}

impl NodeKind {
    fn label(self) -> &'static str {
        match self {
            NodeKind::Hysteria2 => "HY2",
            NodeKind::VlessReality => "REALITY",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            NodeKind::Hysteria2 => "UDP may be blocked on this network",
            NodeKind::VlessReality => "TCP 443 to the server may be blocked or throttled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub failure_rounds: u32,
    pub min_dwell_secs: u64,
    pub return_stable_secs: u64,
    pub max_switches_per_hour: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    None,
    SwitchToFallback { to: String, reason: String },
    ReturnToPrimary { to: String, reason: String },
    /// A switch was warranted but not allowed (reported once per failure streak).
    Suppressed { reason: String },
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
}

#[derive(Debug, Clone)]
pub struct Engine {
    primary: Node,
    fallback: Option<Node>,
    on_fallback: bool,
    /// Consecutive failed rounds of the node in use.
    failed: u32,
    /// Last round result of the node *not* in use (background probe).
    other_ok: Option<bool>,
    /// While on the fallback: since when the primary has passed continuously.
    primary_pass_since: Option<u64>,
    last_switch: Option<u64>,
    switches: VecDeque<u64>,
    suppressed_reported: bool,
}

impl Engine {
    pub fn new(primary: Node, fallback: Option<Node>) -> Self {
        Self {
            primary,
            fallback,
            on_fallback: false,
            failed: 0,
            other_ok: None,
            primary_pass_since: None,
            last_switch: None,
            switches: VecDeque::new(),
            suppressed_reported: false,
        }
    }

    pub fn active_id(&self) -> &str {
        match (&self.fallback, self.on_fallback) {
            (Some(f), true) => &f.id,
            _ => &self.primary.id,
        }
    }

    pub fn primary_id(&self) -> &str {
        &self.primary.id
    }

    pub fn on_fallback(&self) -> bool {
        self.on_fallback
    }

    /// The node to probe in the background (the one not in use), if any.
    pub fn other_id(&self) -> Option<&str> {
        match (&self.fallback, self.on_fallback) {
            (Some(_), true) => Some(&self.primary.id),
            (Some(f), false) => Some(&f.id),
            (None, _) => None,
        }
    }

    pub fn consecutive_failed(&self) -> u32 {
        self.failed
    }

    /// Earliest time a return to the primary is possible (while on fallback).
    pub fn earliest_return(&self, s: &Settings) -> Option<u64> {
        if !self.on_fallback {
            return None;
        }
        let dwell = self.last_switch.map(|t| t + s.min_dwell_secs).unwrap_or(0);
        let stable = self.primary_pass_since.map(|t| t + s.return_stable_secs);
        Some(stable.map_or(dwell, |st| st.max(dwell)))
    }

    fn rate_ok(&mut self, now: u64, s: &Settings) -> bool {
        while self.switches.front().is_some_and(|t| now.saturating_sub(*t) >= 3600) {
            self.switches.pop_front();
        }
        (self.switches.len() as u32) < s.max_switches_per_hour
    }

    fn dwell_ok(&self, now: u64, s: &Settings) -> bool {
        self.last_switch.is_none_or(|t| now.saturating_sub(t) >= s.min_dwell_secs)
    }

    fn record_switch(&mut self, now: u64) {
        self.last_switch = Some(now);
        self.switches.push_back(now);
        self.failed = 0;
        self.other_ok = None;
        self.primary_pass_since = None;
        self.suppressed_reported = false;
    }

    fn suppressed(&mut self, reason: String) -> Decision {
        if self.suppressed_reported {
            Decision::None
        } else {
            self.suppressed_reported = true;
            Decision::Suppressed { reason }
        }
    }

    /// Background probe round of the node not in use.
    pub fn on_other_round(&mut self, now: u64, ok: bool, s: &Settings) -> Decision {
        self.other_ok = Some(ok);
        if self.on_fallback {
            if ok {
                self.primary_pass_since.get_or_insert(now);
            } else {
                self.primary_pass_since = None;
            }
            if !s.enabled {
                return Decision::None;
            }
            if let Some(since) = self.primary_pass_since
                && now.saturating_sub(since) >= s.return_stable_secs
                && self.dwell_ok(now, s)
            {
                if !self.rate_ok(now, s) {
                    return self.suppressed("Switch limit per hour reached; staying on the fallback node".into());
                }
                self.on_fallback = false;
                self.record_switch(now);
                let mins = s.return_stable_secs / 60;
                return Decision::ReturnToPrimary {
                    to: self.primary.id.clone(),
                    reason: format!("{}: healthy for {mins} min — switching back", self.primary.kind.label()),
                };
            }
        }
        Decision::None
    }

    /// Probe round of the node in use.
    pub fn on_active_round(&mut self, now: u64, ok: bool, s: &Settings) -> Decision {
        if ok {
            self.failed = 0;
            self.suppressed_reported = false;
            return Decision::None;
        }
        self.failed += 1;
        if !s.enabled || self.failed < s.failure_rounds {
            return Decision::None;
        }
        let active_kind = if self.on_fallback { self.fallback.as_ref().map(|f| f.kind) } else { Some(self.primary.kind) };
        let label = active_kind.map(|k| k.label()).unwrap_or("Node");
        let hint = active_kind.map(|k| k.hint()).unwrap_or("");
        let why = format!("{label}: {} consecutive probe failures — {hint}", self.failed);
        let Some(fallback) = self.fallback.clone() else {
            return self.suppressed(format!("{why}; no fallback node configured"));
        };
        let rescue = self.other_ok == Some(true);
        if !self.dwell_ok(now, s) && !rescue {
            return self.suppressed(format!("{why}; switched recently, waiting before switching again"));
        }
        if !self.rate_ok(now, s) {
            return self.suppressed(format!("{why}; switch limit per hour reached"));
        }
        if self.on_fallback {
            // Fallback failing: go back to the primary only if it is passing.
            if !rescue {
                return self.suppressed(format!("{why}; the primary node is failing too"));
            }
            self.on_fallback = false;
            self.record_switch(now);
            return Decision::ReturnToPrimary {
                to: self.primary.id.clone(),
                reason: format!("{why}; {} is passing again — switching back", self.primary.kind.label()),
            };
        }
        if self.other_ok == Some(false) {
            return self.suppressed(format!("{why}; the fallback node is failing too"));
        }
        self.on_fallback = true;
        self.record_switch(now);
        Decision::SwitchToFallback { to: fallback.id, reason: why }
    }
}

/// Did a probe round pass? A round passes if **any** target passed.
pub fn round_ok(target_results: &[bool]) -> bool {
    target_results.iter().any(|ok| *ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Settings {
        Settings { enabled: true, failure_rounds: 3, min_dwell_secs: 300, return_stable_secs: 600, max_switches_per_hour: 4 }
    }
    fn engine() -> Engine {
        Engine::new(
            Node { id: "hy2".into(), kind: NodeKind::Hysteria2 },
            Some(Node { id: "reality".into(), kind: NodeKind::VlessReality }),
        )
    }

    #[test]
    fn three_failed_rounds_switch_with_reason() {
        let mut e = engine();
        assert_eq!(e.on_active_round(0, false, &s()), Decision::None);
        assert_eq!(e.on_active_round(10, false, &s()), Decision::None);
        match e.on_active_round(20, false, &s()) {
            Decision::SwitchToFallback { to, reason } => {
                assert_eq!(to, "reality");
                assert_eq!(reason, "HY2: 3 consecutive probe failures — UDP may be blocked on this network");
            }
            d => panic!("{d:?}"),
        }
        assert_eq!(e.active_id(), "reality");
    }

    #[test]
    fn one_website_failing_never_switches() {
        let mut e = engine();
        for t in 0..100 {
            // gstatic fails, cloudflare passes → the round passes.
            let ok = round_ok(&[false, true]);
            assert_eq!(e.on_active_round(t * 10, ok, &s()), Decision::None);
        }
        assert_eq!(e.active_id(), "hy2");
        assert!(!round_ok(&[false, false]));
    }

    #[test]
    fn intermittent_failures_reset_the_streak() {
        let mut e = engine();
        for t in 0..30 {
            let ok = t % 3 == 2; // fail, fail, pass, …
            assert_eq!(e.on_active_round(t * 10, ok, &s()), Decision::None);
        }
    }

    #[test]
    fn return_needs_dwell_and_continuous_stability() {
        let mut e = engine();
        for t in [0, 10, 20] {
            e.on_active_round(t, false, &s());
        }
        assert!(e.on_fallback());
        // Primary passes from t=30 on; a single failure at t=200 resets stability.
        let mut returned_at = None;
        let mut t = 30;
        while t <= 2000 {
            let ok = t != 200;
            e.on_active_round(t, true, &s()); // fallback healthy
            if let Decision::ReturnToPrimary { to, reason } = e.on_other_round(t, ok, &s()) {
                assert_eq!(to, "hy2");
                assert_eq!(reason, "HY2: healthy for 10 min — switching back");
                returned_at = Some(t);
                break;
            }
            t += 10;
        }
        // Stability restarts at t=210 → earliest return 210 + 600 = 810 (dwell 20+300 already passed).
        assert_eq!(returned_at, Some(810));
        assert_eq!(e.active_id(), "hy2");
    }

    #[test]
    fn dwell_blocks_flapping_and_is_reported_once() {
        let mut e = engine();
        for t in [0, 10, 20] {
            e.on_active_round(t, false, &s());
        }
        // Fallback fails right after the switch while the primary is also failing.
        e.on_other_round(25, false, &s());
        let mut suppressed = 0;
        for t in [30, 40, 50, 60, 70] {
            match e.on_active_round(t, false, &s()) {
                Decision::Suppressed { reason } => {
                    suppressed += 1;
                    assert!(reason.contains("REALITY: 3 consecutive probe failures"), "{reason}");
                }
                Decision::None => {}
                d => panic!("unexpected {d:?}"),
            }
        }
        assert_eq!(suppressed, 1, "suppression reported once per streak");
        assert!(e.on_fallback());
    }

    #[test]
    fn rescue_switch_inside_dwell_when_other_node_passes() {
        let mut e = engine();
        for t in [0, 10, 20] {
            e.on_active_round(t, false, &s());
        }
        e.on_other_round(25, true, &s()); // primary passes again
        e.on_active_round(30, false, &s());
        e.on_active_round(40, false, &s());
        match e.on_active_round(50, false, &s()) {
            Decision::ReturnToPrimary { reason, .. } => assert!(reason.contains("is passing again"), "{reason}"),
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn rate_limit_and_disabled_and_no_fallback() {
        let mut st = s();
        st.max_switches_per_hour = 1;
        st.min_dwell_secs = 0;
        let mut e = engine();
        for t in [0, 10, 20] {
            e.on_active_round(t, false, &st);
        }
        e.on_other_round(25, true, &st);
        let mut last = Decision::None;
        for t in [30, 40, 50] {
            last = e.on_active_round(t, false, &st);
        }
        assert!(matches!(last, Decision::Suppressed { ref reason } if reason.contains("limit")), "{last:?}");

        let mut off = s();
        off.enabled = false;
        let mut e = engine();
        for t in 0..10 {
            assert_eq!(e.on_active_round(t * 10, false, &off), Decision::None);
        }

        let mut e = Engine::new(Node { id: "hy2".into(), kind: NodeKind::Hysteria2 }, None);
        let mut got = Decision::None;
        for t in [0, 10, 20] {
            got = e.on_active_round(t, false, &s());
        }
        assert!(matches!(got, Decision::Suppressed { ref reason } if reason.contains("no fallback")), "{got:?}");
    }

    #[test]
    fn earliest_return_reported() {
        let mut e = engine();
        for t in [0, 10, 20] {
            e.on_active_round(t, false, &s());
        }
        assert_eq!(e.earliest_return(&s()), Some(20 + 300));
        e.on_other_round(100, true, &s());
        assert_eq!(e.earliest_return(&s()), Some(100 + 600));
    }
}
