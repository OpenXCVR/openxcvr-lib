//! Pure presence/lifecycle steps (, task 62 — bounded
//! slice).
//!
//! Debounce counters, reclassification budgets, the write-cycle guard and
//! probe cadence as pure reductions over supplied samples, history, time
//! and policy. No type here probes, sleeps, drains, mutates live counters
//! or claims electrical presence capability: the daemon owns the live
//! history, timers and detection, and re-supplies both on every call.
//!
//! The donor-derived policy numbers (kmod at ca8283e: present/absent
//! debounce 2/3, reclassification stable/poll-cycle/attempt 2/50/3, probe
//! interval up to 200 ms, write-cycle guard 50 ms) are carried as
//! [`PresencePolicy::DONOR`] constants with their provenance, not as new
//! standard timers; callers may supply their own. Everything else task 62
//! names — `lifecycle_step` fencing/retire effect ordering, asymmetric
//! non-CMIS→CMIS self-heal, MgmtInit-NACK-vs-removal distinction,
//! uncountable-sample freshness refusal — is owed by the defining task,
//! not invented here.

use super::{MonoDuration, MonoTime};

/// One presence observation as the caller classified it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceSample {
    /// A countable observation that the module is present.
    Present,
    /// A countable observation that the module is absent.
    Absent,
    /// The probe answered but its result does not count (e.g. a bus the
    /// caller cannot arbitrate). Holds lifecycle state: counters do not
    /// move, and no verdict changes.
    Uncountable,
    /// No observation was taken this cycle. Same hold semantics as
    /// [`PresenceSample::Uncountable`].
    Unavailable,
}

/// The debounce counters one module's lifecycle history carries. The daemon
/// owns the live instance and re-supplies it on every call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PresenceHistory {
    /// Consecutive `Present` samples so far.
    pub present_streak: u32,
    /// Consecutive `Absent` samples so far.
    pub absent_streak: u32,
}

/// What one presence step concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceVerdict {
    /// The module is present (the present debounce reached its threshold).
    Present,
    /// The module is absent (the absent debounce reached its threshold).
    Absent,
    /// No threshold was crossed this step.
    Unchanged,
}

/// Debounce thresholds, donor-derived by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresencePolicy {
    /// Consecutive `Present` samples required to conclude present.
    pub present_debounce: u32,
    /// Consecutive `Absent` samples required to conclude absent.
    pub absent_debounce: u32,
}

impl PresencePolicy {
    /// kmod donor policy: 2 consecutive present samples, 3 consecutive
    /// absent samples.
    pub const DONOR: PresencePolicy = PresencePolicy {
        present_debounce: 2,
        absent_debounce: 3,
    };
}

/// One pure presence-debounce step.
///
/// A `Present` sample grows the present streak and clears the absent
/// streak, and vice versa; `Uncountable`/`Unavailable` hold both (an
/// uncountable sample never validates or invalidates anything). The
/// verdict fires on the step that reaches a threshold and stays that
/// verdict on later samples of the same kind (a later opposite verdict
/// requires the opposite debounce again).
pub fn presence_step(
    history: PresenceHistory,
    sample: PresenceSample,
    policy: PresencePolicy,
) -> (PresenceHistory, PresenceVerdict) {
    match sample {
        PresenceSample::Present => {
            let present_streak = history.present_streak.saturating_add(1);
            let next = PresenceHistory {
                present_streak,
                absent_streak: 0,
            };
            let verdict = if present_streak >= policy.present_debounce {
                PresenceVerdict::Present
            } else {
                PresenceVerdict::Unchanged
            };
            (next, verdict)
        }
        PresenceSample::Absent => {
            let absent_streak = history.absent_streak.saturating_add(1);
            let next = PresenceHistory {
                present_streak: 0,
                absent_streak,
            };
            let verdict = if absent_streak >= policy.absent_debounce {
                PresenceVerdict::Absent
            } else {
                PresenceVerdict::Unchanged
            };
            (next, verdict)
        }
        PresenceSample::Uncountable | PresenceSample::Unavailable => {
            (history, PresenceVerdict::Unchanged)
        }
    }
}

/// Reclassification budget counters (donor 2/50/3): after a removal, a
/// module may be re-detected as the same supported kind when it shows
/// `stable_needed` stable checks, examined every `check_interval` poll
/// cycles, within `max_attempts` failed examinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclassifyPolicy {
    /// Consecutive stable examinations required to re-detect.
    pub stable_needed: u32,
    /// Poll cycles between two examinations.
    pub check_interval: u32,
    /// Failed examinations after which re-detection is refused.
    pub max_attempts: u32,
}

impl ReclassifyPolicy {
    /// Donor policy: 2 stable examinations, 50 poll cycles, 3 attempts.
    pub const DONOR: ReclassifyPolicy = ReclassifyPolicy {
        stable_needed: 2,
        check_interval: 50,
        max_attempts: 3,
    };
}

/// The reclassification counters one module's lifecycle history carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReclassifyHistory {
    /// Poll cycles counted since the last examination.
    pub cycles: u32,
    /// Consecutive stable examinations so far.
    pub stable: u32,
    /// Failed examinations so far (an examination whose observed state
    /// was not stable).
    pub attempts: u32,
}

/// What one reclassification step concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclassifyDecision {
    /// Not an examination cycle: counters advanced, nothing concluded.
    Continue,
    /// The stable threshold was reached: the module may be re-detected as
    /// this supported kind. Terminal for this round.
    Ready,
    /// The attempt budget is exhausted: re-detection as this kind is
    /// refused (the caller owes its re-detect/retire ordering).
    BudgetExhausted,
}

/// One pure reclassification step.
///
/// `observed_stable` is the caller's already-decoded answer to "is the
/// module stable as this kind right now" — the library never decodes it.
/// Every `check_interval`-th call is an examination: a stable observation
/// grows the stable streak, an unstable one consumes an attempt and resets
/// the streak. Reaching `stable_needed` returns [`ReclassifyDecision::Ready`]
/// (terminal); exceeding `max_attempts` returns
/// [`ReclassifyDecision::BudgetExhausted`].
pub fn reclassify_step(
    history: ReclassifyHistory,
    observed_stable: bool,
    policy: ReclassifyPolicy,
) -> (ReclassifyHistory, ReclassifyDecision) {
    let cycles = history.cycles + 1;
    if cycles < policy.check_interval {
        return (
            ReclassifyHistory { cycles, ..history },
            ReclassifyDecision::Continue,
        );
    }
    // Examination cycle.
    if observed_stable {
        let stable = history.stable + 1;
        let next = ReclassifyHistory {
            cycles: 0,
            stable,
            attempts: history.attempts,
        };
        if stable >= policy.stable_needed {
            (ReclassifyHistory::default(), ReclassifyDecision::Ready)
        } else {
            (next, ReclassifyDecision::Continue)
        }
    } else {
        let attempts = history.attempts + 1;
        let next = ReclassifyHistory {
            cycles: 0,
            stable: 0,
            attempts,
        };
        if attempts > policy.max_attempts {
            (next, ReclassifyDecision::BudgetExhausted)
        } else {
            (next, ReclassifyDecision::Continue)
        }
    }
}

/// The write-cycle guard window (donor 50 ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteCyclePolicy {
    /// How long after a completed write the module is treated as still in
    /// its write cycle, during which a new write must wait.
    pub guard: MonoDuration,
}

impl WriteCyclePolicy {
    /// Donor policy: 50 ms after a completed write.
    pub const DONOR: WriteCyclePolicy = WriteCyclePolicy {
        guard: MonoDuration(50_000_000),
    };
}

/// Whether `now` still falls inside the write-cycle guard of the write
/// that completed at `last_write_completed`. Pure time comparison; the
/// caller owns the clock and the recorded completion time. A `now` before
/// the completion is a caller error reported as in-cycle (fail closed).
pub fn in_write_cycle(
    last_write_completed: MonoTime,
    now: MonoTime,
    policy: WriteCyclePolicy,
) -> bool {
    now.0.saturating_sub(last_write_completed.0) < policy.guard.0
}

/// The probe cadence (donor up to 200 ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbePolicy {
    /// Minimum time between two presence probes.
    pub interval: MonoDuration,
}

impl ProbePolicy {
    /// Donor policy: at most 200 ms between probes.
    pub const DONOR: ProbePolicy = ProbePolicy {
        interval: MonoDuration(200_000_000),
    };
}

/// Whether a new presence probe is due: the time since the last probe has
/// reached the policy interval. Pure comparison; the caller owns the
/// schedule.
pub fn probe_due(since_last_probe: MonoDuration, policy: ProbePolicy) -> bool {
    since_last_probe.0 >= policy.interval.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_debounces_at_donor_thresholds() {
        let policy = PresencePolicy::DONOR;
        let (h1, v1) = presence_step(PresenceHistory::default(), PresenceSample::Present, policy);
        assert_eq!(v1, PresenceVerdict::Unchanged);
        let (h2, v2) = presence_step(h1, PresenceSample::Present, policy);
        assert_eq!(v2, PresenceVerdict::Present);
        // A later Present sample stays Present.
        let (_, v3) = presence_step(h2, PresenceSample::Present, policy);
        assert_eq!(v3, PresenceVerdict::Present);
        // One Absent sample clears the streak: two more needed for Absent.
        let (h4, _) = presence_step(h2, PresenceSample::Absent, policy);
        let (h5, v5) = presence_step(h4, PresenceSample::Absent, policy);
        assert_eq!(v5, PresenceVerdict::Unchanged);
        let (_, v6) = presence_step(h5, PresenceSample::Absent, policy);
        assert_eq!(v6, PresenceVerdict::Absent);
    }

    #[test]
    fn uncountable_and_unavailable_samples_hold_state() {
        for sample in [PresenceSample::Uncountable, PresenceSample::Unavailable] {
            let history = PresenceHistory {
                present_streak: 1,
                absent_streak: 2,
            };
            let (next, verdict) = presence_step(history, sample, PresencePolicy::DONOR);
            assert_eq!(next, history, "{sample:?} must not move counters");
            assert_eq!(verdict, PresenceVerdict::Unchanged);
        }
    }

    #[test]
    fn reclassify_needs_two_stable_examinations_every_50_cycles() {
        let policy = ReclassifyPolicy::DONOR;
        let mut history = ReclassifyHistory::default();
        let mut readies = 0;
        for cycle in 0..100 {
            let (next, decision) = reclassify_step(history, true, policy);
            // Examinations happen exactly on multiples of 50.
            let is_exam = (cycle + 1) % 50 == 0;
            assert_eq!(next.cycles, if is_exam { 0 } else { (cycle + 1) % 50 });
            if matches!(decision, ReclassifyDecision::Ready) {
                readies += 1;
            }
            history = next;
        }
        // Two examinations happen (cycle 50 and cycle 100), both stable:
        // Ready fires on the second (stable streak 2) and resets history.
        assert_eq!(readies, 1);
        assert_eq!(history, ReclassifyHistory::default());
    }

    #[test]
    fn reclassify_exhausts_its_attempt_budget() {
        let policy = ReclassifyPolicy::DONOR;
        let mut history = ReclassifyHistory::default();
        let mut exhausted = 0;
        for _ in 0..200 {
            let (next, decision) = reclassify_step(history, false, policy);
            if matches!(decision, ReclassifyDecision::BudgetExhausted) {
                exhausted += 1;
            }
            history = next;
        }
        // Four examinations in 200 cycles; the fourth consumes the last
        // attempt (4 > 3) and every later one stays refused.
        assert_eq!(exhausted, 1);
        assert_eq!(history.attempts, 4);
        assert_eq!(history.stable, 0);
    }

    #[test]
    fn an_unstable_examination_resets_the_stable_streak() {
        let policy = ReclassifyPolicy::DONOR;
        let mut history = ReclassifyHistory::default();
        for _ in 0..50 {
            let (next, _) = reclassify_step(history, true, policy);
            history = next;
        }
        assert_eq!(history.stable, 1);
        for _ in 0..50 {
            let (next, _) = reclassify_step(history, false, policy);
            history = next;
        }
        assert_eq!(history.stable, 0);
        assert_eq!(history.attempts, 1);
    }

    #[test]
    fn write_cycle_guard_covers_50ms() {
        let completed = MonoTime(1_000_000);
        assert!(in_write_cycle(
            completed,
            MonoTime(1_000_000),
            WriteCyclePolicy::DONOR
        ));
        assert!(in_write_cycle(
            completed,
            MonoTime(1_000_000 + 49_999_999),
            WriteCyclePolicy::DONOR
        ));
        assert!(!in_write_cycle(
            completed,
            MonoTime(1_000_000 + 50_000_000),
            WriteCyclePolicy::DONOR
        ));
        // Reversed supplied times fail closed as in-cycle.
        assert!(in_write_cycle(
            completed,
            MonoTime(999_999),
            WriteCyclePolicy::DONOR
        ));
    }

    #[test]
    fn probe_is_due_at_the_policy_interval() {
        assert!(!probe_due(MonoDuration(199_999_999), ProbePolicy::DONOR));
        assert!(probe_due(MonoDuration(200_000_000), ProbePolicy::DONOR));
        assert!(probe_due(MonoDuration(1_000_000_000), ProbePolicy::DONOR));
    }
}
