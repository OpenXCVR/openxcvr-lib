use super::pages::read_a0h;
use openxcvr_core::{error::ModuleError, transport::Transport, types::ModuleState};

use crate::ctx::Ctx;

/// Reads module state from the projected A0h and A2h pages.
///
/// `data_not_ready` decodes the explicit A2h:110.0 Data_Not_Ready bit
/// (Table 9-16). SFF-8472 has no interrupt line, so `interrupt_asserted`
/// stays `None`.
///
/// Status string is synthesized from options bits in the A0h projection
/// (digital_diagnostics_implemented at A0h:92 bit 6; options at A0h:120+).
/// Temperature and VCC alarm/warning flags come from the A2h diagnostics
/// projection.
pub(super) fn module_state(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<ModuleState, ModuleError> {
    let page = read_a0h(t, ctx)?;
    let a2h = super::pages::read_a2h(t, ctx)?;
    let f = &a2h.alarm_warning_flag;

    // Module-level temp/vcc alarms from A2h:112 bits 7-6
    let temp_alarm = f[0];
    // VCC high/low alarm from A2h:112 bits 4-3
    let vcc_alarm = f[4];
    // Module-level temp/vcc warnings from A2h:116 bits 7-6
    let temp_warning = f[16];
    // VCC high/low warning from A2h:116 bits 4-3
    let vcc_warning = f[20];

    let has_ddi = page.digital_diagnostics_implemented;
    // options bits for TX fault / RX LOS / TX disable
    // For the generated struct: options[6]=TX fault, options[5]=RX LOS, options[3]=TX Disable
    let tx_fault = page.options.get(6).copied().unwrap_or(false);
    let rx_los = page.options.get(5).copied().unwrap_or(false);
    let tx_disable = page.options.get(3).copied().unwrap_or(false);

    // Synthesize status string
    let mut status_parts = Vec::new();
    if tx_fault || rx_los {
        status_parts.push("fault".to_string());
    }
    if tx_disable {
        status_parts.push("tx_disabled".to_string());
    }
    if !has_ddi {
        status_parts.push("no_ddi".to_string());
    }
    if status_parts.is_empty() {
        status_parts.push("operational".to_string());
    }

    Ok(ModuleState {
        status: status_parts.join(","),
        data_not_ready: Some(a2h.data_not_ready), // A2h:110.0, Table 9-16
        interrupt_asserted: None,                 // SFF-8472 has no explicit interrupt bit
        temp_alarm,
        temp_warning,
        vcc_alarm,
        vcc_warning,
    })
}
