use super::pages::read_a2h;
use openxcvr_core::{
    error::ModuleError,
    transport::Transport,
    types::{LaneFlags, ModuleFlags},
};

use crate::ctx::Ctx;

pub(super) fn module_flags(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<ModuleFlags, ModuleError> {
    // alarm_warning_flag indices (per SFF-8472 v12.5a § 8.9):
    //   [0..7]  = A2h:112 bits 7..0  (temp/vcc/bias/power alarms)
    //   [8..15] = A2h:113 bits 7..0  (rx_power alarms + reserved)
    //   [16..23]= A2h:116 bits 7..0  (temp/vcc/bias/power warnings)
    //   [24..31]= A2h:117 bits 7..0  (rx_power warnings + reserved)
    let a2h = read_a2h(t, ctx)?;
    let f = &a2h.alarm_warning_flag;
    Ok(ModuleFlags {
        // A2h:110.0 Data_Not_Ready (Table 9-16): the module does not yet
        // have valid monitor data while the bit is 1.
        data_not_ready: Some(a2h.data_not_ready),
        interrupt_asserted: None,
        temp_high_alarm: f[0],
        temp_low_alarm: f[1],
        supply_high_alarm: f[2],
        supply_low_alarm: f[3],
        lanes: vec![LaneFlags {
            tx_bias_high_alarm: f[4],
            tx_bias_low_alarm: f[5],
            tx_power_high_alarm: f[6],
            tx_power_low_alarm: f[7],
            rx_power_high_alarm: f[8],
            rx_power_low_alarm: f[9],
            tx_bias_high_warning: f[20],
            tx_bias_low_warning: f[21],
            tx_power_high_warning: f[22],
            tx_power_low_warning: f[23],
            rx_power_high_warning: f[24],
            rx_power_low_warning: f[25],
        }],
    })
}
