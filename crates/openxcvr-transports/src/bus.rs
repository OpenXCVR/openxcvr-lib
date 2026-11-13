//! The backend-agnostic I2C bus contract: addressing, transfer limits, and
//! the single-owner write / repeated-start read primitives every backend
//! (Linux i2c-dev, HID, a test double) implements. Page, bank and window
//! selection live in [`crate::paged`], not here.

use crate::{Error, ErrorKind, Result};
use std::time::Instant;

/// A seven-bit I2C slave address. Device protocols address the same module at
/// 0x50 (lower memory) and 0x51 (upper memory); both are plain addresses here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address7(u8);

impl Address7 {
    /// Accept a seven-bit address; anything above 0x7F is
    /// [`ErrorKind::Invalid`], never truncated or masked.
    pub fn new(value: u8) -> Result<Self> {
        if value > 0x7f {
            return Err(Error::new(ErrorKind::Invalid, "not a seven-bit address"));
        }
        Ok(Self(value))
    }
    /// Only the two historical SFF aliases are normalized; arbitrary high bytes fail.
    pub fn from_legacy(value: u8) -> Result<Self> {
        Self::new(match value {
            0xa0 => 0x50,
            0xa2 => 0x51,
            other => other,
        })
    }
    /// The normalized seven-bit address.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }
}

/// What one bus transfer can carry. The memory layer splits larger accesses
/// into fragments that fit these; the module's 128-byte window bound is the
/// memory layer's business, not the bus's.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Includes any memory-pointer prefix, excludes the I2C slave address.
    pub write: usize,
    /// Data bytes one `write_read` returns, pointer prefix excluded.
    pub read: usize,
    /// Register-pointer bytes a `write_read` writes before reading.
    pub prefix: usize,
}

/// A single owner must hold the bus throughout selection and dependent transfers.
/// Implementations never retry a possibly applied write, nor insert a STOP into
/// write_read. Limits describe bus transfers, not OS/report buffer sizes.
///
/// # Contract shared with [`crate::paged::Memory`]
///
/// - `write` and `write_read` either complete or fail; a failure is reported
///   through [`Error`], with `detail` for logs only. No implementation retries
///   internally.
/// - A write the implementation cannot prove did not reach the device is
///   returned via [`Error::uncertain`], so `write_may_have_applied` is set at
///   every write boundary the bus itself owns.
/// - `write_read` performs one addressed write followed by a repeated-start
///   read; it never inserts a STOP between them and never writes data the
///   caller did not supply.
/// - `limits` describes what one bus transfer can carry, so the memory layer
///   can split a transfer: `write` counts the whole written packet, register
///   pointer prefix included; `read` counts the data bytes one `write_read`
///   returns, prefix excluded. The module's own 128-byte window bound is the
///   memory layer's business, not the bus's.
/// - Implementations hold no bus state the caller cannot see: selection is
///   published by [`crate::paged::Memory`] and never guessed by the bus.
pub trait I2cBus {
    /// What one bus transfer can carry; used to fragment larger accesses.
    #[must_use]
    fn limits(&self) -> Limits;
    /// One addressed write; either completes or fails, never retried by the
    /// implementation. A write the implementation cannot prove did not
    /// reach the device is returned marked with [`Error::uncertain`].
    fn write(&mut self, address: Address7, bytes: &[u8], deadline: Instant) -> Result<()>;
    /// One addressed register-pointer write followed by a repeated-start
    /// read of `out` — no STOP between them, no data the caller did not
    /// supply. Either the whole transfer completes or it fails.
    fn write_read(
        &mut self,
        address: Address7,
        prefix: &[u8],
        out: &mut [u8],
        deadline: Instant,
    ) -> Result<()>;
}

impl<T: I2cBus + ?Sized> I2cBus for &mut T {
    fn limits(&self) -> Limits {
        (**self).limits()
    }
    fn write(&mut self, address: Address7, bytes: &[u8], deadline: Instant) -> Result<()> {
        (**self).write(address, bytes, deadline)
    }
    fn write_read(
        &mut self,
        address: Address7,
        prefix: &[u8],
        out: &mut [u8],
        deadline: Instant,
    ) -> Result<()> {
        (**self).write_read(address, prefix, out, deadline)
    }
}
