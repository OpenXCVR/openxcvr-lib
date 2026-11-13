//! Pure alarm/warning threshold evaluation.
//!
//! The library parses and decides; the daemon owns execution. This module
//! turns one measurement plus a threshold set into the alarm/warning
//! decision, with no I/O, no clocks and no retained state — callers
//! (SFF-8636 Page 03h thresholds, SFF-8472 A2h alarm/warning threshold
//! tables, CMIS Page 02h) supply the decoded values.

use serde::{Deserialize, Serialize};

/// One parameter's alarm/warning threshold set, in the caller's physical
/// units. A `None` bound disables that decision (the family layer leaves it
/// unset when the module does not implement the monitor).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// High-alarm bound; `None` disables that decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_alarm: Option<f32>,
    /// Low-alarm bound; `None` disables that decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_alarm: Option<f32>,
    /// High-warning bound; `None` disables that decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_warning: Option<f32>,
    /// Low-warning bound; `None` disables that decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_warning: Option<f32>,
}

/// The threshold decisions for one measurement: which bounds the
/// measurement met or exceeded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct ThresholdEvaluation {
    /// The measurement met or exceeded the high-alarm bound.
    pub high_alarm: bool,
    /// The measurement met or fell below the low-alarm bound.
    pub low_alarm: bool,
    /// The measurement met or exceeded the high-warning bound.
    pub high_warning: bool,
    /// The measurement met or fell below the low-warning bound.
    pub low_warning: bool,
}

impl ThresholdEvaluation {
    /// True when no bound is met.
    #[must_use]
    pub fn is_within_bounds(&self) -> bool {
        !(self.high_alarm || self.low_alarm || self.high_warning || self.low_warning)
    }
}

/// Evaluate `measurement` against `thresholds` (inclusive bounds: a
/// measurement equal to a bound trips it, mirroring the SFF flag semantics
/// where a latched flag asserts once the monitor reaches the threshold).
///
/// Pure: the same (measurement, thresholds) pair always yields the same
/// decision, regardless of call order, history or time.
#[must_use = "the evaluation is a pure decision over the supplied measurement and thresholds"]
pub fn evaluate(measurement: f32, thresholds: &Thresholds) -> ThresholdEvaluation {
    ThresholdEvaluation {
        high_alarm: thresholds.high_alarm.is_some_and(|t| measurement >= t),
        low_alarm: thresholds.low_alarm.is_some_and(|t| measurement <= t),
        high_warning: thresholds.high_warning.is_some_and(|t| measurement >= t),
        low_warning: thresholds.low_warning.is_some_and(|t| measurement <= t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Thresholds {
        Thresholds {
            high_alarm: Some(80.0),
            low_alarm: Some(-10.0),
            high_warning: Some(70.0),
            low_warning: Some(0.0),
        }
    }

    #[test]
    fn in_bounds_measurement_trips_nothing() {
        let e = evaluate(25.0, &full());
        assert!(e.is_within_bounds());
    }

    #[test]
    fn inclusive_bounds_trip_at_equality() {
        assert!(evaluate(80.0, &full()).high_alarm);
        assert!(evaluate(-10.0, &full()).low_alarm);
        assert!(evaluate(70.0, &full()).high_warning);
        assert!(evaluate(0.0, &full()).low_warning);
    }

    #[test]
    fn alarm_dominates_warning_on_the_same_side() {
        let e = evaluate(95.0, &full());
        assert!(e.high_alarm && e.high_warning);
        let e = evaluate(-40.0, &full());
        assert!(e.low_alarm && e.low_warning);
    }

    #[test]
    fn unset_bounds_never_trip() {
        let t = Thresholds {
            high_warning: Some(70.0),
            ..Default::default()
        };
        let e = evaluate(1000.0, &t);
        assert!(e.high_warning);
        assert!(!e.high_alarm && !e.low_alarm && !e.low_warning);
    }

    #[test]
    fn evaluation_is_pure_and_order_independent() {
        let t = full();
        assert_eq!(evaluate(85.0, &t), evaluate(85.0, &t));
        assert_eq!(evaluate(85.0, &t), evaluate(85.0, &full()));
    }

    #[test]
    fn serializes_with_unset_bounds_skipped_like_vendorinfo() {
        let t = Thresholds {
            high_warning: Some(1.5),
            ..Default::default()
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"high_warning\":1.5"));
        assert!(!json.contains("high_alarm"));
    }
}
