//! Pure alarm/warning evaluation of the SFF-8636 Lower-memory monitors
//! against the Upper Page 03h threshold sets (§ 6.6.1, Table 6-28),
//! through **caller-supplied Page 03h bytes**.
//!
//! The library decides, the daemon executes — and the library never
//! writes the page-select byte (Lower:127): switching to Page 03h is the
//! caller's call, so this surface takes the page as bytes the caller
//! already read. The only I/O it performs is the Lower-memory read every
//! monitor accessor already makes (offset 0, page 00h — visible at every
//! page selection). No clocks, no retained state: the same (monitors,
//! Page 03h bytes) pair always yields the same decisions.

use openxcvr_core::rev::Rev;
use openxcvr_core::{
    error::ModuleError,
    threshold::{ThresholdEvaluation, Thresholds},
};

use super::lower::LowerCommon;
use crate::ctx::Ctx;

/// The threshold decisions for the SFF-8636 module monitors (case
/// temperature, supply voltage) and the four per-lane channel monitors
/// (RX power, TX bias, TX power). A monitor whose threshold set the
/// module does not implement (an all-zero set, § 6.6.1 "for each monitor
/// that is implemented") stays within bounds — never a spurious alarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sff8636ThresholdEvaluations {
    /// Internally measured case temperature.
    pub temperature: ThresholdEvaluation,
    /// Internally measured supply voltage.
    pub supply_voltage: ThresholdEvaluation,
    /// Per-lane received optical power, lanes 1-4 in index order.
    pub rx_power: [ThresholdEvaluation; 4],
    /// Per-lane TX bias current, lanes 1-4 in index order.
    pub tx_bias: [ThresholdEvaluation; 4],
    /// Per-lane transmitted optical power, lanes 1-4 in index order.
    pub tx_power: [ThresholdEvaluation; 4],
}

/// Private common projection of the Page 03h threshold sets — the five
/// sets of Table 6-28 are present at every admitted revision, so the
/// projection compiles against each per-revision generated reader and the
/// rev-agnostic union view alike.
#[derive(Debug)]
pub(super) struct Page03Thresholds {
    /// 1/256 °C LSBs, severity order high_alarm, low_alarm,
    /// high_warning, low_warning.
    pub temp_mon_threshold: [i16; 4],
    /// 100 µV LSBs, severity order as above.
    pub vcc_mon_threshold: [u16; 4],
    /// 0.1 µW LSBs, severity order as above.
    pub rx_power_threshold: [u16; 4],
    /// 2 µA LSBs, severity order as above.
    pub tx_bias_threshold: [u16; 4],
    /// 0.1 µW LSBs, severity order as above.
    pub tx_power_threshold: [u16; 4],
}

/// Project the threshold sets from a revision-specialised generated
/// `Sff8636Page03hThresholdsAndControls` page; field access stays
/// compiler-checked against the concrete type.
macro_rules! project_page03 {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        Page03Thresholds {
            temp_mon_threshold: page.temp_mon_threshold,
            vcc_mon_threshold: page.vcc_mon_threshold,
            rx_power_threshold: page.rx_power_threshold,
            tx_bias_threshold: page.tx_bias_threshold,
            tx_power_threshold: page.tx_power_threshold,
        }
    }};
}

/// Decode the caller-supplied Page 03h bytes through the
/// revision-specialised reader selected by the frozen `ctx` (`None`
/// keeps the union view — see [`super::lower::decode_lower`]).
pub(super) fn decode_page03h(
    ctx: Option<Ctx>,
    buf: &[u8; 128],
) -> Result<Page03Thresholds, ModuleError> {
    use crate::generated as g;
    match ctx {
        None => Ok(project_page03!(
            buf,
            g::union::Sff8636Page03hThresholdsAndControls
        )),
        Some(ctx) => match ctx.detected_rev {
            Rev::SFF8636_1_3 => Ok(project_page03!(
                buf,
                g::v1_3::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_1_4 => Ok(project_page03!(
                buf,
                g::v1_4::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_1_5 => Ok(project_page03!(
                buf,
                g::v1_5::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_2_0 => Ok(project_page03!(
                buf,
                g::v2_0::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_2_5 => Ok(project_page03!(
                buf,
                g::v2_5::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_2_8 => Ok(project_page03!(
                buf,
                g::v2_8::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_2_11 => Ok(project_page03!(
                buf,
                g::v2_11::Sff8636Page03hThresholdsAndControls
            )),
            Rev::SFF8636_2_12 => Ok(project_page03!(
                buf,
                g::v2_12::Sff8636Page03hThresholdsAndControls
            )),
            // Fail closed: a detected revision this crate compiles no
            // decoder for is never decoded through a nearest-latest guess.
            other => {
                let _ = other;
                Err(ModuleError::Unsupported)
            }
        },
    }
}

/// Convert one raw unsigned threshold set (severity order per the
/// schema: high_alarm, low_alarm, high_warning, low_warning) through a
/// scale to physical units. An all-zero raw set is the not-implemented
/// encoding: every bound is disabled, because a zero high-alarm bound
/// would spuriously trip on every reading — the same rule the SFF-8472
/// threshold evaluation applies to its A2h sets.
fn scaled_thresholds_high_low(raw: [u16; 4], scale: f32) -> Thresholds {
    if raw.iter().all(|&v| v == 0) {
        return Thresholds::default();
    }
    Thresholds {
        high_alarm: Some(raw[0] as f32 * scale),
        low_alarm: Some(raw[1] as f32 * scale),
        high_warning: Some(raw[2] as f32 * scale),
        low_warning: Some(raw[3] as f32 * scale),
    }
}

/// The signed variant for the temperature set (1/256 °C LSBs).
fn temperature_thresholds(raw: [i16; 4]) -> Thresholds {
    if raw.iter().all(|&v| v == 0) {
        return Thresholds::default();
    }
    Thresholds {
        high_alarm: Some(raw[0] as f32 / 256.0),
        low_alarm: Some(raw[1] as f32 / 256.0),
        high_warning: Some(raw[2] as f32 / 256.0),
        low_warning: Some(raw[3] as f32 / 256.0),
    }
}

/// Pure evaluation of the Lower-memory monitors against the decoded
/// Page 03h threshold sets. The monitor scales mirror the lane/module
/// accessors: temperature 1/256 °C, supply voltage 100 µV, optical power
/// 0.1 µW, bias 2 µA.
pub(super) fn evaluate(
    lower: &LowerCommon,
    thresholds: &Page03Thresholds,
) -> Sff8636ThresholdEvaluations {
    Sff8636ThresholdEvaluations {
        temperature: openxcvr_core::threshold::evaluate(
            lower.temperature as f32 / 256.0,
            &temperature_thresholds(thresholds.temp_mon_threshold),
        ),
        supply_voltage: openxcvr_core::threshold::evaluate(
            lower.supply_voltage as f32 * 0.0001,
            &scaled_thresholds_high_low(thresholds.vcc_mon_threshold, 0.0001),
        ),
        rx_power: std::array::from_fn(|n| {
            openxcvr_core::threshold::evaluate(
                lower.rx_power_lane[n] as f32 / 10000.0,
                &scaled_thresholds_high_low(thresholds.rx_power_threshold, 0.0001),
            )
        }),
        tx_bias: std::array::from_fn(|n| {
            openxcvr_core::threshold::evaluate(
                lower.tx_bias_lane[n] as f32 * 0.002,
                &scaled_thresholds_high_low(thresholds.tx_bias_threshold, 0.002),
            )
        }),
        tx_power: std::array::from_fn(|n| {
            openxcvr_core::threshold::evaluate(
                lower.tx_power_lane[n] as f32 / 10000.0,
                &scaled_thresholds_high_low(thresholds.tx_power_threshold, 0.0001),
            )
        }),
    }
}
