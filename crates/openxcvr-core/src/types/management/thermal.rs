//! Pure thermal evaluation over supplied observations.
//!
//! One pure function, [`evaluate`], turns caller-supplied facts — a
//! high-alarm flag the caller already read, its own latched-protection
//! record, a temperature, and the advertised thresholds — into a
//! [`ThermalVerdict`] plus the protection action the kmod donor would take.
//! Nothing here reads a module, starts a timer, or latches anything: the
//! caller owns acquisition, history and execution. `latched` is a caller
//! record of an earlier protection, like kmod's `overtemp_latched`,
//! distinguished from fresh observations.

/// The high-alarm flag as the caller can supply it. A missing, failed or
/// stale flag is `NotSupplied` — never silently `Supplied(false)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// No usable flag observation exists this cycle.
    NotSupplied,
    /// A usable flag observation: `true` = high alarm asserted.
    Supplied(bool),
}

/// Advertised thermal levels (CMIS page 02h bytes 128–135), Celsius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Temperature at/above which the module declares a high-alarm warning.
    pub alarm: f32,
    /// Temperature at/above which the module declares a (pre-alarm) warning.
    pub warning: f32,
}

/// The supplied facts of one thermal evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observations {
    /// The module's high-alarm flag, as supplied.
    pub high_alarm_flag: Flag,
    /// The caller's record of an earlier protection (kmod
    /// `overtemp_latched`). Never decides the verdict; only the donor
    /// action.
    pub latched: bool,
    /// Last supplied case temperature, Celsius.
    pub temperature: Option<f32>,
    /// The advertised thermal thresholds, when supplied.
    pub thresholds: Option<Thresholds>,
}

/// The thermal severity the supplied facts decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Within the advertised warning level.
    Normal,
    /// At or above the advertised warning level but below alarm.
    Warning,
    /// At or above the advertised alarm level, or the flag is asserted.
    Alarm,
}

/// Which input decided the verdict: the module's own high-alarm flag
/// whenever it is supplied (D4 precedence), else the thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecidedBy {
    /// The module's own high-alarm flag decided.
    Flag,
    /// The supplied temperature against the thresholds decided.
    Thresholds,
}

/// The protection action the kmod donor (`cmis_thermal_decide`) would take,
/// computed only when the flag decided. `None` when thresholds decided —
/// the donor never acts on thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionAction {
    /// Alarm asserted and not yet latched: enter protection.
    Protect,
    /// Alarm cleared while latched: release protection (re-arming
    /// monitoring; a renewed alarm may protect again).
    Release,
}

/// Why no verdict could be decided. The caller supplied neither a flag nor
/// a sufficient temperature/threshold pair, and a decided
/// Normal/Warning/Alarm must never be manufactured from insufficient
/// input (fail closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "insufficient thermal observation: no supplied high-alarm flag and no \
     temperature/threshold pair to decide with"
)]
pub struct InsufficientObservation;

/// The outcome of one pure thermal evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalVerdict {
    /// The decided severity.
    pub verdict: Severity,
    /// Which input decided the verdict.
    pub decided_by: DecidedBy,
    /// The donor action for the supplied flag and latch record; `None`
    /// whenever the thresholds decided (the donor never acts on
    /// thresholds) and when the flag leaves nothing to do.
    pub kmod_action: Option<ProtectionAction>,
    /// Whether the supplied temperature crosses the warning threshold
    /// while the verdict itself is decided by the flag: the pre-alarm
    /// hint the D4 contract keeps visible. `false` when the temperature or
    /// thresholds are not supplied.
    pub warning_crossed: bool,
}

/// The donor's flag/latch action table (kmod `cmis_thermal_decide`),
/// verbatim: protect on a fresh alarm, release on a cleared latch, and no
/// action when the flag state and latch record already agree.
fn donor_action(alarm: bool, latched: bool) -> Option<ProtectionAction> {
    match (alarm, latched) {
        (true, false) => Some(ProtectionAction::Protect),
        (false, true) => Some(ProtectionAction::Release),
        _ => None,
    }
}

/// Pure thermal verdict from supplied observations.
///
/// Precedence, exactly as D4 specifies:
///
/// * The flag decides whenever it is supplied: `Alarm` when set, `Normal`
///   when clear, and a threshold warning then appears only in
///   `warning_crossed` (the pre-alarm hint). `latched` never changes the
///   verdict; it only participates in `kmod_action`.
/// * Thresholds decide (`Normal`/`Warning`/`Alarm` by temperature against
///   the advertised levels) only when the flag is not supplied, and the
///   verdict says so; `kmod_action` is `None` there — the donor never acts
///   on thresholds.
/// * Neither path available → [`InsufficientObservation`], never a
///   manufactured Normal.
pub fn evaluate(observations: &Observations) -> Result<ThermalVerdict, InsufficientObservation> {
    let warning_crossed = match (observations.temperature, observations.thresholds) {
        (Some(temp), Some(thresholds)) => temp >= thresholds.warning,
        _ => false,
    };
    match observations.high_alarm_flag {
        Flag::Supplied(alarm) => Ok(ThermalVerdict {
            verdict: if alarm {
                Severity::Alarm
            } else {
                Severity::Normal
            },
            decided_by: DecidedBy::Flag,
            kmod_action: donor_action(alarm, observations.latched),
            warning_crossed,
        }),
        Flag::NotSupplied => {
            let (temp, thresholds) = match (observations.temperature, observations.thresholds) {
                (Some(temp), Some(thresholds)) => (temp, thresholds),
                _ => return Err(InsufficientObservation),
            };
            let verdict = if temp >= thresholds.alarm {
                Severity::Alarm
            } else if temp >= thresholds.warning {
                Severity::Warning
            } else {
                Severity::Normal
            };
            Ok(ThermalVerdict {
                verdict,
                decided_by: DecidedBy::Thresholds,
                kmod_action: None,
                warning_crossed,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLDS: Thresholds = Thresholds {
        alarm: 70.0,
        warning: 65.0,
    };

    fn obs(flag: Flag, latched: bool, temp: Option<f32>) -> Observations {
        Observations {
            high_alarm_flag: flag,
            latched,
            temperature: temp,
            thresholds: Some(THRESHOLDS),
        }
    }

    #[test]
    fn a_supplied_set_flag_decides_alarm_and_protects_when_not_latched() {
        let verdict = evaluate(&obs(Flag::Supplied(true), false, Some(60.0))).unwrap();
        assert_eq!(verdict.verdict, Severity::Alarm);
        assert_eq!(verdict.decided_by, DecidedBy::Flag);
        assert_eq!(verdict.kmod_action, Some(ProtectionAction::Protect));
        // Below the warning threshold, no pre-alarm hint.
        assert!(!verdict.warning_crossed);
    }

    #[test]
    fn a_supplied_set_flag_with_latch_already_set_decides_alarm_without_action() {
        let verdict = evaluate(&obs(Flag::Supplied(true), true, Some(80.0))).unwrap();
        assert_eq!(verdict.verdict, Severity::Alarm);
        assert_eq!(verdict.kmod_action, None);
    }

    #[test]
    fn a_supplied_clear_flag_releases_a_latched_protection() {
        let verdict = evaluate(&obs(Flag::Supplied(false), true, Some(60.0))).unwrap();
        assert_eq!(verdict.verdict, Severity::Normal);
        assert_eq!(verdict.kmod_action, Some(ProtectionAction::Release));
    }

    #[test]
    fn a_supplied_clear_flag_with_no_latch_is_normal_with_no_action() {
        let verdict = evaluate(&obs(Flag::Supplied(false), false, Some(60.0))).unwrap();
        assert_eq!(verdict.verdict, Severity::Normal);
        assert_eq!(verdict.kmod_action, None);
    }

    #[test]
    fn latched_never_changes_the_flag_decided_verdict() {
        for latched in [false, true] {
            let set = evaluate(&obs(Flag::Supplied(true), latched, Some(60.0))).unwrap();
            assert_eq!(set.verdict, Severity::Alarm);
            let clear = evaluate(&obs(Flag::Supplied(false), latched, Some(60.0))).unwrap();
            assert_eq!(clear.verdict, Severity::Normal);
        }
    }

    #[test]
    fn a_flag_decided_normal_still_exposes_the_warning_hint() {
        // Flag clear but temperature above the warning threshold: the
        // verdict stays Normal (the flag decides), the crossing appears
        // only as the pre-alarm hint.
        let verdict = evaluate(&obs(Flag::Supplied(false), false, Some(66.0))).unwrap();
        assert_eq!(verdict.verdict, Severity::Normal);
        assert!(verdict.warning_crossed);
    }

    #[test]
    fn thresholds_decide_only_when_the_flag_is_not_supplied() {
        let hot = evaluate(&obs(Flag::NotSupplied, false, Some(72.0))).unwrap();
        assert_eq!(hot.verdict, Severity::Alarm);
        assert_eq!(hot.decided_by, DecidedBy::Thresholds);
        assert_eq!(hot.kmod_action, None, "the donor never acts on thresholds");

        let warm = evaluate(&obs(Flag::NotSupplied, false, Some(66.0))).unwrap();
        assert_eq!(warm.verdict, Severity::Warning);
        assert!(warm.warning_crossed);

        let cool = evaluate(&obs(Flag::NotSupplied, false, Some(20.0))).unwrap();
        assert_eq!(cool.verdict, Severity::Normal);
        assert!(!cool.warning_crossed);
    }

    #[test]
    fn insufficient_input_fails_closed() {
        // No flag, no temperature.
        assert_eq!(
            evaluate(&obs(Flag::NotSupplied, false, None)),
            Err(InsufficientObservation)
        );
        // No flag, temperature but no thresholds.
        let no_thresholds = Observations {
            high_alarm_flag: Flag::NotSupplied,
            latched: false,
            temperature: Some(72.0),
            thresholds: None,
        };
        assert_eq!(evaluate(&no_thresholds), Err(InsufficientObservation));
    }

    #[test]
    fn a_supplied_flag_decides_even_without_temperature_or_thresholds() {
        let minimal = Observations {
            high_alarm_flag: Flag::Supplied(true),
            latched: false,
            temperature: None,
            thresholds: None,
        };
        let verdict = evaluate(&minimal).unwrap();
        assert_eq!(verdict.verdict, Severity::Alarm);
        assert_eq!(verdict.kmod_action, Some(ProtectionAction::Protect));
        assert!(!verdict.warning_crossed);
    }
}
