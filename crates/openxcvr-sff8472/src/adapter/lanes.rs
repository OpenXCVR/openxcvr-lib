use super::calibration::{apply_linear, cal_mode, linear_constants, rx_power_polynomial, CalMode};
use super::pages::read_a2h;
use openxcvr_core::{error::ModuleError, transport::Transport, types::LaneStatus};

use crate::ctx::Ctx;

/// Decode the single channel's status from the schema-declared A2h byte
/// 110 per-bit fields (Table 9-16) through the revision-specialised
/// generated reader — no hand-coded bit masks.
pub(super) fn lanes(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<Vec<LaneStatus>, ModuleError> {
    let mode = cal_mode(t)?;
    let consts = if matches!(mode, CalMode::External) {
        Some(linear_constants(t)?)
    } else {
        None
    };

    let tx_bias_raw = read_u16(t, 0xA2, 100)?;
    let tx_power_raw = read_u16(t, 0xA2, 102)?;
    let rx_power_raw = read_u16(t, 0xA2, 104)?;

    let tx_bias_ma = match &consts {
        None => tx_bias_raw as f32 * 0.002,
        Some(c) => apply_linear(tx_bias_raw, c[0].0, c[0].1) * 0.002,
    };
    let tx_power_mw = match &consts {
        None => tx_power_raw as f32 / 10000.0,
        Some(c) => apply_linear(tx_power_raw, c[1].0, c[1].1) / 10000.0,
    };
    let rx_power_mw = match &consts {
        None => rx_power_raw as f32 / 10000.0,
        Some(_) => rx_power_polynomial(t, rx_power_raw)? / 10000.0,
    };

    let a2h = read_a2h(t, ctx)?;

    Ok(vec![LaneStatus {
        index: 1,
        rx_power_mw: Some(rx_power_mw),
        tx_power_mw: Some(tx_power_mw),
        tx_bias_ma: Some(tx_bias_ma),
        // A2h:110.1 / 110.2 / 110.7 per Table 9-16: the pin and output
        // state mirrors, decoded through the generated fields (the earlier
        // hand-coded masks read bits 2/3, which are TX Fault and the soft
        // rate select — a decode defect this fixes).
        rx_los: Some(a2h.rx_los_state),
        tx_fault: Some(a2h.tx_fault_state),
        tx_disable: Some(a2h.tx_disable_state),
        // The only additional per-lane signal flag this spec defines is
        // the Adaptive Input EQ Fail Flag (A2h:118.4, Table 10-1, since
        // rev 12.4) — `None` before that revision, where the bit is not
        // modeled. SFF-8472 defines no Tx LOS and no CDR LOL flags (those
        // arrived with SFF-8636/8079 parallel modules), so those stay
        // `None` here.
        tx_los: None,
        tx_cdr_lol: None,
        rx_cdr_lol: None,
        tx_adaptive_eq_fault: a2h.adaptive_input_eq_fail_flag,
        // SFF-8472 has no data-path state machine (CMIS-only field).
        dp_state: None,
    }])
}

fn read_u16(t: &mut dyn Transport, slave_addr: u8, offset: u8) -> Result<u16, ModuleError> {
    let mut buf = [0u8; 2];
    t.read(slave_addr, 0, 0, offset, &mut buf)?;
    Ok(u16::from_be_bytes(buf))
}
