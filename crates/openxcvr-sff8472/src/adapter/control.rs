//! Write/control surface for the SFF-8472 A2h control bytes.
//!
//! Pure read-modify-write over the schema-declared software control bits:
//! Soft TX Disable Select at A2h:110.6 and Soft Rate Select RS(0) at
//! A2h:110.3 (Table 9-16), Soft RS(1) Select at A2h:118.3 (Table 10-1).
//! Per § 8.5 these are the bits that "allow the host to control the
//! Rate_Select and TX_Disable functions"; the state mirrors at 110.7
//! (TX Disable pin), 110.5/110.4 (RS(1)/RS(0) pins), 110.2 (TX Fault) and
//! 110.1 (Rx_LOS) are read-only meanings and are never written. No
//! autonomous state, no clocks — the transport is the only effect channel,
//! and each write touches exactly one bit, read-modify-write, so sibling
//! bits in the byte survive.

use openxcvr_core::{error::ModuleError, transport::Transport};

/// A2h byte holding the optional status/control bits (Table 9-16).
const STATUS_CONTROL_BYTE: u8 = 110;
/// A2h byte holding the extended module control/status bits (Table 10-1).
const EXTENDED_CONTROL_BYTE: u8 = 118;

/// Soft TX Disable Select (A2h:110.6, schema row `soft_tx_disable_select`).
const SOFT_TX_DISABLE: u8 = 1 << 6;
/// Soft Rate Select RS(0) (A2h:110.3, schema row `soft_rate_select_rs0`).
const SOFT_RATE_SELECT_RS0: u8 = 1 << 3;
/// Soft RS(1) Select (A2h:118.3, schema row `soft_rate_select_rs1`).
const SOFT_RATE_SELECT_RS1: u8 = 1 << 3;

fn read_byte(t: &mut dyn Transport, offset: u8) -> Result<u8, ModuleError> {
    let mut buf = [0u8; 1];
    t.read(
        openxcvr_core::transport::I2C_ADDR_SFF8472_DIAG,
        0,
        0,
        offset,
        &mut buf,
    )?;
    Ok(buf[0])
}

fn write_byte(t: &mut dyn Transport, offset: u8, value: u8) -> Result<(), ModuleError> {
    t.write(
        openxcvr_core::transport::I2C_ADDR_SFF8472_DIAG,
        0,
        0,
        offset,
        &[value],
    )
}

/// Flip exactly one masked bit in one control byte, leaving every sibling
/// bit of the byte untouched.
fn rmw_bit(t: &mut dyn Transport, offset: u8, mask: u8, set: bool) -> Result<(), ModuleError> {
    let byte = read_byte(t, offset)?;
    let byte = if set { byte | mask } else { byte & !mask };
    write_byte(t, offset, byte)
}

/// Set the Soft TX Disable Select bit (A2h:110.6, Table 9-16). Writing '1'
/// disables the laser; the bit is OR'd with the hard TX_DISABLE pin value.
pub(super) fn set_tx_disable(t: &mut dyn Transport, disable: bool) -> Result<(), ModuleError> {
    rmw_bit(t, STATUS_CONTROL_BYTE, SOFT_TX_DISABLE, disable)
}

/// Set the Soft Rate Select RS(0) bit (A2h:110.3, Table 9-16). Writing '1'
/// selects full bandwidth operation; the bit is OR'd with the hard
/// Rate_Select / AS(0) / RS(0) pin value.
pub(super) fn set_rate_select_rs0(
    t: &mut dyn Transport,
    full_bandwidth: bool,
) -> Result<(), ModuleError> {
    rmw_bit(t, STATUS_CONTROL_BYTE, SOFT_RATE_SELECT_RS0, full_bandwidth)
}

/// Set the Soft RS(1) Select bit (A2h:118.3, Table 10-1). Writing '1'
/// selects full speed Tx operation; the bit is OR'd with the hard RS(1)
/// pin value. The bit is defined from Rev 11.4; a module that does not
/// implement it ignores the value (per the table), so the write is
/// delivered regardless of the detected revision.
pub(super) fn set_rate_select_rs1(
    t: &mut dyn Transport,
    full_speed: bool,
) -> Result<(), ModuleError> {
    rmw_bit(t, EXTENDED_CONTROL_BYTE, SOFT_RATE_SELECT_RS1, full_speed)
}
