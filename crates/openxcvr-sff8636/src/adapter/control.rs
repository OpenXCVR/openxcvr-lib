//! Write/control surface for SFF-8636 Lower-memory control bytes.
//!
//! Pure read-modify-write over the schema-declared control bytes (Table
//! 6-9): Tx Disable at byte 86 bits 3-0, Rx Rate Select at byte 87 (2 bits
//! per lane), Tx Rate Select at byte 88 (2 bits per lane). No autonomous
//! state, no clocks — the transport is the only effect channel.

use openxcvr_core::{error::ModuleError, transport::Transport};

pub(super) enum RateSelectTarget {
    Tx,
    Rx,
}

/// Byte offset of a target's rate-select register.
fn rate_select_byte(target: RateSelectTarget) -> u8 {
    match target {
        RateSelectTarget::Tx => 88,
        RateSelectTarget::Rx => 87,
    }
}

fn read_byte(t: &mut dyn Transport, offset: u8) -> Result<u8, ModuleError> {
    let mut buf = [0u8; 1];
    t.read(
        openxcvr_core::transport::I2C_ADDR_MODULE,
        0x00,
        0,
        offset,
        &mut buf,
    )?;
    Ok(buf[0])
}

fn write_byte(t: &mut dyn Transport, offset: u8, value: u8) -> Result<(), ModuleError> {
    t.write(
        openxcvr_core::transport::I2C_ADDR_MODULE,
        0x00,
        0,
        offset,
        &[value],
    )
}

fn check_lane(lane: u8) -> Result<u8, ModuleError> {
    if !(1..=4).contains(&lane) {
        return Err(ModuleError::Io(
            format!("SFF-8636 lane must be 1-4, got {lane}").into(),
        ));
    }
    Ok(lane)
}

pub(super) fn set_tx_disable(
    t: &mut dyn Transport,
    lane: u8,
    disable: bool,
) -> Result<(), ModuleError> {
    let lane = check_lane(lane)?;
    let byte = read_byte(t, 86)?;
    let bit = 1u8 << (lane - 1);
    let byte = if disable { byte | bit } else { byte & !bit };
    write_byte(t, 86, byte)
}

pub(super) fn set_tx_disable_all(
    t: &mut dyn Transport,
    disable: [bool; 4],
) -> Result<(), ModuleError> {
    let mut byte = 0u8;
    for (i, &d) in disable.iter().enumerate() {
        if d {
            byte |= 1 << i;
        }
    }
    write_byte(t, 86, byte)
}

pub(super) fn set_rate_select_raw(
    t: &mut dyn Transport,
    target: RateSelectTarget,
    lane: u8,
    value: u8,
) -> Result<(), ModuleError> {
    let lane = check_lane(lane)?;
    if value > 3 {
        return Err(ModuleError::Io(
            format!("rate select code must be 0-3 (2 bits), got {value}").into(),
        ));
    }
    let offset = rate_select_byte(target);
    let byte = read_byte(t, offset)?;
    let shift = 2 * (lane - 1);
    let mask = 0x03 << shift;
    let byte = (byte & !mask) | (value << shift);
    write_byte(t, offset, byte)
}
