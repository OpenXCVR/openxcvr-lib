use super::lower::LowerCommon;
use openxcvr_core::types::ModuleState;

/// Reads module state from the projected Lower-memory fields.
///
/// SFF-8636 byte 2 contains a status word:
///   bit 1: interrupt_deasserted (1=deasserted, 0=asserted)
///   bit 0: data_not_ready (0=ready, 1=not ready)
///
/// Alarm/warning flags are in the Lower memory page at fixed offsets. Each
/// generated array is ordered high alarm, low alarm, high warning, low
/// warning.
pub(super) fn module_state(lower: &LowerCommon) -> ModuleState {
    // Alarm/warning flags from Lower memory. ModuleState exposes a single
    // warning flag per parameter, so the high ([2]) and low ([3]) warning
    // bits are deliberately aggregated; [1] is the low alarm, not a warning.
    let temp_alarm = lower.temp_mon_alarm_warning_flag[0];
    let temp_warning = lower.temp_mon_alarm_warning_flag[2] || lower.temp_mon_alarm_warning_flag[3];
    let vcc_alarm = lower.vcc_mon_alarm_warning_flag[0];
    let vcc_warning = lower.vcc_mon_alarm_warning_flag[2] || lower.vcc_mon_alarm_warning_flag[3];

    // Synthesize status string from flags
    let mut status_parts = Vec::new();
    if lower.data_not_ready {
        status_parts.push("data_not_ready".to_string());
    }
    if !lower.interrupt_deasserted {
        status_parts.push("interrupt_asserted".to_string());
    }
    if temp_alarm {
        status_parts.push("temp_alarm".to_string());
    }
    if vcc_alarm {
        status_parts.push("vcc_alarm".to_string());
    }
    if status_parts.is_empty() {
        status_parts.push("operational".to_string());
    }

    ModuleState {
        status: status_parts.join(","),
        data_not_ready: Some(lower.data_not_ready),
        interrupt_asserted: Some(!lower.interrupt_deasserted),
        temp_alarm,
        temp_warning,
        vcc_alarm,
        vcc_warning,
    }
}
