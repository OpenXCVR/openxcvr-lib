//! Pure alarm/warning evaluation of the SFF-8472 base monitors against
//! their A2h threshold sets (§ 9.4, Table 9-5).
//!
//! The library decides, the daemon executes: every entry pairs one
//! calibrated measurement with its threshold set through
//! [`openxcvr_core::threshold::evaluate`] and returns the decision. No
//! I/O beyond the reads the module accessors already make, no clocks, no
//! retained state — the same (measurement, thresholds) pair always yields
//! the same decision.

use openxcvr_core::{error::ModuleError, threshold::ThresholdEvaluation, transport::Transport};

use super::pages::read_a2h;
use crate::ctx::Ctx;

/// The threshold decisions for the five SFF-8472 base monitors
/// (temperature, supply voltage, TX bias, TX output power, received
/// optical power). An entry whose monitor or thresholds the module does
/// not implement stays within bounds (its bound set is `None`-disabled),
/// never a spurious alarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sff8472ThresholdEvaluations {
    /// Internally measured module temperature.
    pub temperature: ThresholdEvaluation,
    /// Internally measured supply voltage.
    pub supply_voltage: ThresholdEvaluation,
    /// Measured TX bias current.
    pub tx_bias: ThresholdEvaluation,
    /// Measured TX output power.
    pub tx_power: ThresholdEvaluation,
    /// Measured received optical power.
    pub rx_power: ThresholdEvaluation,
}

/// Convert one raw threshold set (severity order per the schema:
/// high_alarm, low_alarm, high_warning, low_warning) through a scale to
/// physical units. An all-zero raw set is the not-implemented encoding:
/// every bound is disabled, because a zero high-alarm bound would
/// spuriously trip on every reading.
fn scaled_thresholds_high_low(raw: [u16; 4], scale: f32) -> openxcvr_core::threshold::Thresholds {
    if raw.iter().all(|&v| v == 0) {
        return openxcvr_core::threshold::Thresholds::default();
    }
    openxcvr_core::threshold::Thresholds {
        high_alarm: Some(raw[0] as f32 * scale),
        low_alarm: Some(raw[1] as f32 * scale),
        high_warning: Some(raw[2] as f32 * scale),
        low_warning: Some(raw[3] as f32 * scale),
    }
}

/// The signed variant for the temperature set (1/256 °C LSBs).
fn temperature_thresholds(raw: [i16; 4]) -> openxcvr_core::threshold::Thresholds {
    if raw.iter().all(|&v| v == 0) {
        return openxcvr_core::threshold::Thresholds::default();
    }
    openxcvr_core::threshold::Thresholds {
        high_alarm: Some(raw[0] as f32 / 256.0),
        low_alarm: Some(raw[1] as f32 / 256.0),
        high_warning: Some(raw[2] as f32 / 256.0),
        low_warning: Some(raw[3] as f32 / 256.0),
    }
}

/// Evaluate the five base monitors. `measurement` of `None` (a monitor the
/// module does not implement) keeps its evaluation within bounds.
fn decide(
    measurement: Option<f32>,
    thresholds: &openxcvr_core::threshold::Thresholds,
) -> ThresholdEvaluation {
    match measurement {
        Some(value) => openxcvr_core::threshold::evaluate(value, thresholds),
        None => ThresholdEvaluation::default(),
    }
}

pub(super) fn evaluate_thresholds(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<Sff8472ThresholdEvaluations, ModuleError> {
    let a2h = read_a2h(t, ctx)?;
    // Calibrated measurements through the module accessors' own reads:
    // temperature/voltage from the monitoring helpers, the three lane
    // monitors from lanes() (which applies the calibration mode).
    let temperature = super::monitoring::temperature_celsius(t)?;
    let supply_voltage = super::monitoring::supply_voltage_v(t)?;
    let lane = super::lanes::lanes(t, ctx)?;
    let lane = lane
        .first()
        .ok_or_else(|| ModuleError::Io("SFF-8472 lane projection returned no channel".into()))?;

    Ok(Sff8472ThresholdEvaluations {
        temperature: decide(
            temperature,
            &temperature_thresholds(a2h.temperature_threshold),
        ),
        supply_voltage: decide(
            supply_voltage,
            &scaled_thresholds_high_low(a2h.supply_voltage_threshold, 0.0001),
        ),
        tx_bias: decide(
            lane.tx_bias_ma,
            &scaled_thresholds_high_low(a2h.tx_bias_threshold, 0.002),
        ),
        tx_power: decide(
            lane.tx_power_mw,
            &scaled_thresholds_high_low(a2h.tx_power_threshold, 0.0001),
        ),
        rx_power: decide(
            lane.rx_power_mw,
            &scaled_thresholds_high_low(a2h.rx_power_threshold, 0.0001),
        ),
    })
}
