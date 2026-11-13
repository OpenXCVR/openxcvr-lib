use super::lower::LowerCommon;
use openxcvr_core::types::{LaneFlags, ModuleFlags};

/// Per-lane alarm/warning flags from the Lower-memory channel-monitor
/// interrupt flag bitmaps (bytes 9-15; Table 6-7). Each 4-bit group is
/// ordered high alarm, low alarm, high warning, low warning.
///
/// Parity fix (W1): these flags were previously dropped — `ModuleFlags.lanes`
/// was returned empty for SFF-8636 while CMIS populated its per-lane flags.
pub(super) fn module_flags(lower: &LowerCommon) -> ModuleFlags {
    // Flattened flag index = 4 * (lane - 1) + severity, severity ordered
    // high alarm, low alarm, high warning, low warning (Table 6-7).
    let lanes = (0..4usize)
        .map(|lane| {
            let base = 4 * lane;
            LaneFlags {
                rx_power_high_alarm: lower.rx_power_alarm_warning_flag[base],
                rx_power_low_alarm: lower.rx_power_alarm_warning_flag[base + 1],
                rx_power_high_warning: lower.rx_power_alarm_warning_flag[base + 2],
                rx_power_low_warning: lower.rx_power_alarm_warning_flag[base + 3],
                tx_bias_high_alarm: lower.tx_bias_alarm_warning_flag[base],
                tx_bias_low_alarm: lower.tx_bias_alarm_warning_flag[base + 1],
                tx_bias_high_warning: lower.tx_bias_alarm_warning_flag[base + 2],
                tx_bias_low_warning: lower.tx_bias_alarm_warning_flag[base + 3],
                tx_power_high_alarm: lower.tx_power_alarm_warning_flag[base],
                tx_power_low_alarm: lower.tx_power_alarm_warning_flag[base + 1],
                tx_power_high_warning: lower.tx_power_alarm_warning_flag[base + 2],
                tx_power_low_warning: lower.tx_power_alarm_warning_flag[base + 3],
            }
        })
        .collect();
    ModuleFlags {
        data_not_ready: Some(lower.data_not_ready),
        interrupt_asserted: Some(!lower.interrupt_deasserted),
        temp_high_alarm: lower.temp_mon_alarm_warning_flag[0],
        temp_low_alarm: lower.temp_mon_alarm_warning_flag[1],
        supply_high_alarm: lower.vcc_mon_alarm_warning_flag[0],
        supply_low_alarm: lower.vcc_mon_alarm_warning_flag[1],
        lanes,
    }
}
