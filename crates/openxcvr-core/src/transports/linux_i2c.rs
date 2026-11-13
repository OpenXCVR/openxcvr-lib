//! Linux `/dev/i2c-N` transport via the `i2cdev` userspace ioctl wrapper.
//!
//! Cfg-gated to `feature = "linux-i2c"` and `target_os = "linux"`.
//!
//! # Slave-address conventions
//!
//! The [`Transport`] trait's `slave_addr: u8` accepts either:
//! - **7-bit form** (`0x00`–`0x7F`) — the raw i2c slave address. This is what
//!   the Linux kernel's `I2C_SLAVE` ioctl expects directly.
//! - **8-bit form** (`0x80`–`0xFF`) — the SFF-8472 spec convention where
//!   `A0h`/`A2h` are the address byte with the R/W bit at LSB cleared.
//!
//! Values above `0x7F` are unambiguously 8-bit form and are converted to
//! 7-bit by a single right shift before the ioctl. Values at or below
//! `0x7F` are passed through unchanged. This lets CMIS / SFF-8636 callers
//! pass `0x50` and SFF-8472 callers pass `0xA0` / `0xA2` without either
//! having to translate.

use crate::error::{IoError, ModuleError};
use crate::transport::Transport;

use i2cdev::core::{I2CDevice, I2CMessage, I2CTransfer};
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError, LinuxI2CMessage};

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Hold time after writing a page-select byte before subsequent reads
/// can be issued (CMIS 5.3 § 8.2.2). The transport is page-blind by
/// design — adapters that switch pages MUST call [`page_select_hold`]
/// after writing byte 127 of the lower memory.
pub const PAGE_SELECT_HOLD: Duration = Duration::from_millis(1);

/// i2c-spec Standard-mode minimum bus-free time between transactions
/// (CMIS 5.3 Appendix B / SFF-8472 § 5). Used as a safety margin on
/// addr switches and between SMBus block chunks. 5 µs ≥ the spec
/// minimum 4.7 µs.
pub const TBUF_STANDARD: Duration = Duration::from_micros(5);

/// Upper bound on the EEPROM internal write cycle (t_wr). After a write
/// to an NVR-backed byte the module NACKs new transactions until the
/// cycle finishes; [`wait_write_cycle`] polls within this deadline before
/// giving up. Mirrors `XCVR_WRITE_CYCLE_TIMEOUT_MS` in the kernel module
/// (the kernel module's `core/io.c`). 25 ms is comfortable
/// headroom over the ~5 ms typical t_wr of transceiver EEPROMs.
pub const WRITE_CYCLE_TIMEOUT: Duration = Duration::from_millis(25);

/// Inter-probe spacing for the write-cycle acknowledge poll. Mirrors
/// `XCVR_WRITE_CYCLE_POLL_US` in the kernel module's `core/io.c`.
/// Small relative to [`WRITE_CYCLE_TIMEOUT`] so a genuine NVR cycle is
/// caught within a few probes while a volatile-RAM write still returns on
/// the first.
pub const WRITE_CYCLE_POLL: Duration = Duration::from_micros(200);

/// Sleep for one [`PAGE_SELECT_HOLD`] interval. Adapters call this
/// immediately after writing the page-select byte and before issuing
/// the next read against the new page. v0.1 of this transport is
/// page-blind — orchestration lives in the adapter layer.
pub fn page_select_hold() {
    std::thread::sleep(PAGE_SELECT_HOLD);
}

/// Outcome of a single write-cycle acknowledge probe.
///
/// Distinguishes the three cases [`wait_write_cycle`] treats differently,
/// mirroring the errno classification in the kernel module's `core/io.c`:
/// - [`Acked`](ProbeOutcome::Acked): the device answered — the write cycle
///   is done; stop polling and report success.
/// - [`Nacked`](ProbeOutcome::Nacked): the device refused the address
///   transfer (still in t_wr) or the bus saw a transient arbitration loss
///   — keep polling until ACK or deadline.
/// - [`HardError`](ProbeOutcome::HardError): a real controller/bus fault
///   that spinning the deadline will not heal — abort the poll early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeOutcome {
    Acked,
    Nacked,
    HardError,
}

/// Classify a failed probe read into [`Nacked`](ProbeOutcome::Nacked)
/// (retry) vs [`HardError`](ProbeOutcome::HardError) (abort).
///
/// This is an *abort allowlist*, not a retry allowlist: keep polling on any
/// errno except the handful that polling can never heal. A device still in
/// its write cycle NACKs the address transfer, which the kernel I2C core may
/// surface as `ENXIO`, `EREMOTEIO`, `ETIMEDOUT` or `EIO` depending on the
/// controller; `EAGAIN` is a transient multi-master arbitration loss. All of
/// these can mean "still busy, retry". The kernel module's own paged-read
/// path treats `{EIO, ENXIO, ETIMEDOUT}` as a retryable NACK
/// (`optoe_compat.c:762`) and collapses every failed transfer to `EIO`, so a
/// bare errno cannot be read as "definitely not a NACK". Abort early only on
/// `ENODEV` (device/adapter gone), `EOPNOTSUPP` (the adapter lacks this SMBus
/// op — a static property, not a busy state) and `EINVAL` (malformed request).
///
/// Erring toward keep-polling is the safe bias: a false `HardError` aborts a
/// poll a still-busy device needed (risking a partial write on the next
/// chunk), whereas a false `Nacked` merely spins to the deadline (slow,
/// harmless). An unreadable errno therefore maps to `Nacked`. If a controller
/// is found to report an unhealable fault as some *other* errno, add it to the
/// abort set rather than aborting by default.
///
/// errno values are positive in userspace (`i2cdev` stores them via
/// `nix::Error as i32`); the kernel module uses the negated forms. Values
/// from `asm-generic/errno-base.h` (ENODEV, EINVAL) and `asm-generic/errno.h`
/// (EOPNOTSUPP). This module is `cfg(target_os = "linux")`, so they are fixed.
fn classify_probe_error(e: &LinuxI2CError) -> ProbeOutcome {
    const ENODEV: i32 = 19;
    const EINVAL: i32 = 22;
    const EOPNOTSUPP: i32 = 95;

    let errno = match e {
        LinuxI2CError::Errno(n) => Some(*n),
        LinuxI2CError::Io(io) => io.raw_os_error(),
    };
    match errno {
        Some(n) if n == ENODEV || n == EOPNOTSUPP || n == EINVAL => ProbeOutcome::HardError,
        _ => ProbeOutcome::Nacked,
    }
}

/// Bounded acknowledge-poll for the EEPROM write cycle (issue #196).
///
/// After a committed write to an NVR-backed byte the module NACKs every
/// new transaction until its internal write cycle (t_wr) completes. This
/// loop re-issues a cheap probe until the device ACKs, a hard bus fault is
/// seen, or the deadline passes — mirroring `xcvr_i2c_wait_write_cycle` in
/// the kernel module's `core/io.c`. It is intentionally I/O-agnostic so it is
/// unit-testable:
///
/// - `probe` performs one acknowledge probe (in production, a 1-byte read
///   at the just-written register) and returns a [`ProbeOutcome`].
///   [`Acked`](ProbeOutcome::Acked) ends the poll; a volatile-RAM write
///   ACKs on the first probe, so the poll self-tunes — no spin for
///   control-byte / page-select writes. [`HardError`](ProbeOutcome::HardError)
///   aborts immediately rather than spinning a fault out to the deadline.
/// - `now` returns elapsed time since an arbitrary epoch (production:
///   `Instant::elapsed`). Using `Duration` rather than `Instant` keeps the
///   clock injectable — `Instant` has no public constructor.
/// - `sleep` waits between probes (production: `std::thread::sleep`).
///
/// Returns `true` if the device ACKed within [`WRITE_CYCLE_TIMEOUT`],
/// `false` on a hard error or timeout. The write itself is already
/// committed before this is called, so a non-ACK is advisory — the caller
/// proceeds either way, exactly as the kernel module treats it.
/// Probe-then-check-then-sleep ordering matches the kmod loop so behaviour
/// is analogous.
fn wait_write_cycle<P, N, S>(mut probe: P, now: N, mut sleep: S) -> bool
where
    P: FnMut() -> ProbeOutcome,
    N: Fn() -> Duration,
    S: FnMut(Duration),
{
    let deadline = now() + WRITE_CYCLE_TIMEOUT;
    loop {
        match probe() {
            ProbeOutcome::Acked => return true,
            ProbeOutcome::HardError => return false,
            ProbeOutcome::Nacked => {}
        }
        if now() >= deadline {
            return false;
        }
        sleep(WRITE_CYCLE_POLL);
    }
}

/// Linux `/dev/i2c-N` transport.
///
/// Opens an i2c-dev character device once and switches between slave
/// addresses per call (caching the last-set address to elide redundant
/// `I2C_SLAVE` ioctls when consecutive calls target the same slave).
///
/// # Example
///
/// ```ignore
/// use openxcvr_core::transports::LinuxI2cTransport;
/// use openxcvr_core::transport::{Transport, I2C_ADDR_SFF8472_BASE};
///
/// let mut t = LinuxI2cTransport::new("/dev/i2c-1")?;
/// let mut buf = [0u8; 16];
/// t.read(I2C_ADDR_SFF8472_BASE, 0, 0, 20, &mut buf)?;  // vendor name
/// ```
/// The i2c-dev access layer shared by [`LinuxI2cTransport`] and
/// `openxcvr_transports::i2cdev::I2cDevBus`: one open adapter node, the
/// address claimed with plain `I2C_SLAVE` (never `I2C_SLAVE_FORCE`, so an
/// address a kernel driver owns fails with `EBUSY` instead of being taken),
/// and raw `I2C_RDWR` transfers. Errors keep their OS errno so callers can
/// classify them.
pub struct I2cDev {
    dev: LinuxI2CDevice,
    /// Last 7-bit address claimed. `None` until the first claim.
    claimed: Option<u8>,
}

impl I2cDev {
    /// Opens the adapter node at `path` (e.g. `/dev/i2c-1`). The kernel wants
    /// an address at open; `0x00` is a placeholder until the first claim.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let dev = LinuxI2CDevice::new(path.as_ref(), 0x00).map_err(io::Error::from)?;
        Ok(Self { dev, claimed: None })
    }

    /// Claims the 7-bit `address` with `I2C_SLAVE`, skipping the ioctl when
    /// it is already claimed. Nothing reaches the bus. Returns whether the
    /// address changed, so a caller can observe bus-free time on a re-aim.
    pub fn claim(&mut self, address: u8) -> io::Result<bool> {
        if self.claimed == Some(address) {
            return Ok(false);
        }
        self.claimed = None;
        self.dev
            .set_slave_address(u16::from(address))
            .map_err(io::Error::from)?;
        self.claimed = Some(address);
        Ok(true)
    }

    /// One `I2C_RDWR` write message to the claimed address. A transfer the
    /// kernel reports as incomplete is `UnexpectedEof`.
    pub fn transfer_write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut messages = [LinuxI2CMessage::write(bytes)];
        let done = self.dev.transfer(&mut messages).map_err(io::Error::from)?;
        complete(done, 1)
    }

    /// A write of `prefix` followed by a repeated-start read into `out`, as
    /// one `I2C_RDWR` call with no STOP between them.
    pub fn transfer_write_read(&mut self, prefix: &[u8], out: &mut [u8]) -> io::Result<()> {
        let mut messages = [LinuxI2CMessage::write(prefix), LinuxI2CMessage::read(out)];
        let done = self.dev.transfer(&mut messages).map_err(io::Error::from)?;
        complete(done, 2)
    }
}

fn complete(done: u32, expected: u32) -> io::Result<()> {
    if done == expected {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("i2c-dev I2C_RDWR completed {done} of {expected} messages"),
        ))
    }
}

pub struct LinuxI2cTransport {
    dev: I2cDev,
}

impl LinuxI2cTransport {
    /// Open the i2c-dev character device at `path` (e.g. `/dev/i2c-1`).
    ///
    /// The kernel requires a slave address at open time; we pass `0x00`
    /// as a placeholder and switch to the real address on the first
    /// `read` / `write` / `probe` call.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, ModuleError> {
        let dev = I2cDev::open(path.as_ref()).map_err(|e| {
            ModuleError::Io(IoError::with_source(
                format!("linux-i2c opening {}: {e}", path.as_ref().display()),
                e,
            ))
        })?;
        Ok(Self { dev })
    }

    /// Convert any-form slave address to 7-bit and set it on the device,
    /// caching to skip the ioctl when unchanged. Inserts a tBuf-style
    /// delay after a real addr switch — many transceivers NACK without
    /// recovery time when the master re-aims between A0h and A2h.
    fn select_slave(&mut self, slave_addr: u8) -> Result<(), ModuleError> {
        let changed = self.dev.claim(to_7bit(slave_addr)).map_err(|e| {
            ModuleError::Io(IoError::with_source(
                format!("linux-i2c set_slave_address: {e}"),
                e,
            ))
        })?;
        if changed {
            // tBuf delay before the next transaction (CMIS 5.3 Appendix
            // B / SFF-8472 § 5). 2 × TBUF_STANDARD gives extra margin
            // on addr switches because some transceivers need more
            // recovery time between A0h ↔ A2h re-aims than between
            // back-to-back same-addr reads.
            std::thread::sleep(2 * TBUF_STANDARD);
        }
        Ok(())
    }
}

/// Convert a 7-bit-or-8-bit slave address to 7-bit form.
///
/// Values `0x00..=0x7F` are already 7-bit; values `0x80..=0xFF` are 8-bit
/// (SFF-8472 spec form with the R/W bit position included) and need a
/// single right shift.
fn to_7bit(addr: u8) -> u8 {
    if addr > 0x7F {
        addr >> 1
    } else {
        addr
    }
}

impl Transport for LinuxI2cTransport {
    fn read(
        &mut self,
        slave_addr: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        // Page selection is not handled here: per the spec's design
        // decisions, the adapter writes the page-select byte before
        // calling read/write. v0.1 of this transport is page-blind.
        check_range(offset, buf.len())?;
        self.select_slave(slave_addr)?;

        // SMBus block-read does write(offset) + repeated-start read(N)
        // atomically. SMBus caps `len` at 32 bytes, so chunk longer
        // reads. Most transceiver field reads (vendor name = 16 B, OUI
        // = 3 B, lane status = 1 B) fit in one call.
        let mut filled = 0usize;
        while filled < buf.len() {
            let chunk_len = (buf.len() - filled).min(32) as u8;
            let chunk_offset = chunk_offset(offset, filled).ok_or_else(|| {
                ModuleError::Io(
                    format!("linux-i2c: read offset overflow at filled={filled}, offset={offset}")
                        .into(),
                )
            })?;
            let chunk = self
                .dev
                .dev
                .smbus_read_i2c_block_data(chunk_offset, chunk_len)
                .map_err(|e| io_err("smbus_read_i2c_block_data", e))?;
            if chunk.len() != usize::from(chunk_len) {
                return Err(ModuleError::Io(format!(
                    "linux-i2c: short read at offset 0x{chunk_offset:02x}: requested {chunk_len}, got {}",
                    chunk.len()
                ).into()));
            }
            buf[filled..filled + chunk.len()].copy_from_slice(&chunk);
            filled += chunk.len();
            // Inter-chunk tBuf for back-to-back SMBus block reads —
            // skip after the last chunk.
            if filled < buf.len() {
                std::thread::sleep(TBUF_STANDARD);
            }
        }
        Ok(())
    }

    fn write(
        &mut self,
        slave_addr: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<(), ModuleError> {
        check_range(offset, data.len())?;
        self.select_slave(slave_addr)?;

        // SMBus block-write does write(offset) + write(N data bytes)
        // atomically. Same 32-byte cap as read.
        let mut sent = 0usize;
        while sent < data.len() {
            let chunk_len = (data.len() - sent).min(32);
            let chunk_offset = chunk_offset(offset, sent).ok_or_else(|| {
                ModuleError::Io(
                    format!("linux-i2c: write offset overflow at sent={sent}, offset={offset}")
                        .into(),
                )
            })?;
            self.dev
                .dev
                .smbus_write_i2c_block_data(chunk_offset, &data[sent..sent + chunk_len])
                .map_err(|e| io_err("smbus_write_i2c_block_data", e))?;
            sent += chunk_len;

            // Issue #196: wait out the device's internal write cycle (t_wr)
            // before the next transaction. After a write to an NVR-backed
            // byte the module NACKs new transactions until the cycle
            // completes, so probe a 1-byte read at the just-written register
            // until it ACKs (or the deadline expires). This subsumes the old
            // fixed inter-chunk tBuf delay: a volatile-RAM write ACKs on the
            // first probe (zero added latency), a genuine NVR write spins a
            // few probes. Runs after every chunk — including the last — so
            // the transport always leaves the device ready for the caller's
            // next op. The write already committed, so a timeout is advisory
            // (no error), exactly as the kernel module treats it.
            let start = Instant::now();
            let dev = &mut self.dev.dev;
            let _acked = wait_write_cycle(
                || match dev.smbus_read_i2c_block_data(chunk_offset, 1) {
                    Ok(_) => ProbeOutcome::Acked,
                    Err(e) => classify_probe_error(&e),
                },
                || start.elapsed(),
                std::thread::sleep,
            );
        }
        Ok(())
    }

    fn probe(&mut self) -> Result<(), ModuleError> {
        // Cheapest aliveness check: read byte 0 at the standard module
        // slave address. NACK / EIO surfaces as ModuleError::Io.
        let mut buf = [0u8; 1];
        self.read(crate::transport::I2C_ADDR_MODULE, 0, 0, 0, &mut buf)
    }
}

/// Register address of the chunk starting `done` bytes into a transfer
/// that began at `offset`.
///
/// Returns `None` when the address would not fit in the 8-bit register
/// space. `done` is checked *before* narrowing to `u8`: an `as u8` cast
/// would truncate modulo 256 and silently aim a >255-byte transfer at the
/// wrong register instead of tripping the overflow guard.
fn chunk_offset(offset: u8, done: usize) -> Option<u8> {
    u8::try_from(done).ok().and_then(|d| offset.checked_add(d))
}

/// Rejects a transfer whose *last* byte would fall outside the 8-bit
/// register space, before any bus traffic is issued.
///
/// `chunk_offset` is not sufficient on its own: it validates each chunk's
/// start address, so a transfer whose tail crosses 0xFF has its leading
/// chunks issued — and, on the write path, already committed to the module —
/// before the overflow is detected. Both `read` and `write` preflight the
/// full range here so an out-of-range request touches nothing at all.
fn check_range(offset: u8, len: usize) -> Result<(), ModuleError> {
    if len == 0 {
        return Ok(());
    }
    u8::try_from(len - 1)
        .ok()
        .and_then(|last| offset.checked_add(last))
        .map(|_| ())
        .ok_or_else(|| {
            ModuleError::Io(
                format!(
                    "linux-i2c: transfer of {len} B at offset 0x{offset:02x} runs past \
                 the end of the 8-bit register space (last addressable byte is 0xff)"
                )
                .into(),
            )
        })
}

fn io_err(context: &str, e: LinuxI2CError) -> ModuleError {
    ModuleError::Io(IoError::with_source(format!("linux-i2c {context}: {e}"), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn to_7bit_passes_through_7bit_values() {
        assert_eq!(to_7bit(0x00), 0x00);
        assert_eq!(to_7bit(0x50), 0x50);
        assert_eq!(to_7bit(0x7F), 0x7F);
    }

    #[test]
    fn to_7bit_shifts_8bit_values() {
        // SFF-8472 conventions: 0xA0 ↔ 0x50, 0xA2 ↔ 0x51.
        assert_eq!(to_7bit(0xA0), 0x50);
        assert_eq!(to_7bit(0xA2), 0x51);
        assert_eq!(to_7bit(0xFE), 0x7F);
    }

    #[test]
    fn chunk_offset_advances_within_register_space() {
        assert_eq!(chunk_offset(0x00, 0), Some(0x00));
        assert_eq!(chunk_offset(0x80, 32), Some(0xA0));
        assert_eq!(chunk_offset(0x80, 127), Some(0xFF));
    }

    #[test]
    fn chunk_offset_rejects_wrap_past_register_space() {
        // Sum overflows u8.
        assert_eq!(chunk_offset(0x80, 128), None);
        assert_eq!(chunk_offset(0xFF, 1), None);
        // Cursor alone exceeds u8: must fail rather than truncate. With an
        // `as u8` cast these would have wrapped to Some(0x00)/Some(0x01).
        assert_eq!(chunk_offset(0x00, 256), None);
        assert_eq!(chunk_offset(0x00, 257), None);
        assert_eq!(chunk_offset(0x00, usize::MAX), None);
    }

    #[test]
    fn check_range_accepts_transfers_ending_at_the_last_register() {
        assert!(check_range(0x00, 0).is_ok());
        assert!(check_range(0x00, 256).is_ok());
        assert!(check_range(0xFF, 1).is_ok());
        assert!(check_range(0xF0, 16).is_ok());
        assert!(check_range(0x80, 128).is_ok());
    }

    #[test]
    fn check_range_rejects_a_transfer_whose_tail_crosses_the_register_space() {
        // The regression this guard exists for: `chunk_offset` alone accepts
        // this because the FIRST chunk starts at a valid 0xF0. The 32-byte
        // chunk it would issue spans 0xF0..=0x10F, so on the write path the
        // module was already mutated by the time the second chunk tripped the
        // per-chunk guard. Preflighting the range rejects it before any bus
        // traffic.
        assert!(chunk_offset(0xF0, 0).is_some());
        assert!(check_range(0xF0, 33).is_err());

        // A single sub-32-byte chunk that crosses the boundary never trips
        // the per-chunk guard at all: it has exactly one valid start address.
        assert!(chunk_offset(0xFA, 0).is_some());
        assert!(check_range(0xFA, 7).is_err());

        assert!(check_range(0x00, 257).is_err());
        assert!(check_range(0xFF, 2).is_err());
        assert!(check_range(0x01, 256).is_err());
        assert!(check_range(0x00, usize::MAX).is_err());
    }

    // --- write-cycle acknowledge poll (issue #196) ---------------------
    //
    // These drive wait_write_cycle with injected probe/clock/sleep closures
    // so the logic is exercised deterministically — no real sleeping, no
    // I2C device. The fake clock is a Cell<Duration> the fake sleep advances.

    /// Volatile-RAM write: the device ACKs the first probe. The poll must
    /// return success after exactly one probe and zero sleeps, so the common
    /// page/bank-select / control-byte write pays no spin.
    #[test]
    fn wait_write_cycle_returns_on_first_ack() {
        let probes = Cell::new(0u32);
        let sleeps = Cell::new(0u32);

        let acked = wait_write_cycle(
            || {
                probes.set(probes.get() + 1);
                ProbeOutcome::Acked
            },
            || Duration::ZERO,
            |_| sleeps.set(sleeps.get() + 1),
        );

        assert!(acked);
        assert_eq!(probes.get(), 1);
        assert_eq!(sleeps.get(), 0);
    }

    /// NVR write cycle: the device NACKs a few probes, then ACKs. The poll
    /// must spin past the NACKs and return success on the ACK — probe count
    /// is nacks + 1, with one inter-probe sleep per NACK. The clock stays far
    /// below the deadline here, so the ACK (not the timeout) ends the spin.
    #[test]
    fn wait_write_cycle_spins_until_ack() {
        const NACKS: u32 = 3;
        let probes = Cell::new(0u32);
        let sleeps = Cell::new(0u32);
        let clock = Cell::new(Duration::ZERO);

        let acked = wait_write_cycle(
            || {
                let n = probes.get();
                probes.set(n + 1);
                // NACK probes 0..NACKS, ACK on probe NACKS
                if n >= NACKS {
                    ProbeOutcome::Acked
                } else {
                    ProbeOutcome::Nacked
                }
            },
            || clock.get(),
            |d| {
                sleeps.set(sleeps.get() + 1);
                clock.set(clock.get() + d);
            },
        );

        assert!(acked);
        assert_eq!(probes.get(), NACKS + 1);
        assert_eq!(sleeps.get(), NACKS);
    }

    /// Wedged device: the poll never sees an ACK. It must bound out at the
    /// deadline and report failure (not hang, not falsely succeed). With the
    /// fake clock advancing one WRITE_CYCLE_POLL per sleep, the probe count is
    /// exactly deadline/poll + 1 — the +1 is the final probe at t == deadline
    /// that precedes the terminating now() >= deadline check.
    #[test]
    fn wait_write_cycle_bounds_out_on_timeout() {
        let probes = Cell::new(0u32);
        let clock = Cell::new(Duration::ZERO);

        let acked = wait_write_cycle(
            || {
                probes.set(probes.get() + 1);
                ProbeOutcome::Nacked // never ACKs
            },
            || clock.get(),
            |d| clock.set(clock.get() + d),
        );

        assert!(!acked);
        let expected = (WRITE_CYCLE_TIMEOUT.as_micros() / WRITE_CYCLE_POLL.as_micros()) as u32 + 1;
        assert_eq!(probes.get(), expected);
    }

    /// Hard bus fault during the poll: the probe returns
    /// [`ProbeOutcome::HardError`]. The poll must abort after the first
    /// probe — not mistake it for a still-busy device and spin to the
    /// deadline — returning `false` with zero sleeps. This is the Rust
    /// mirror of the kmod `test_writecycle_aborts_on_bus_error` case: a
    /// poll-time controller/bus fault that spinning cannot heal.
    #[test]
    fn wait_write_cycle_aborts_on_hard_error() {
        let probes = Cell::new(0u32);
        let sleeps = Cell::new(0u32);
        let clock = Cell::new(Duration::ZERO);

        let acked = wait_write_cycle(
            || {
                probes.set(probes.get() + 1);
                ProbeOutcome::HardError
            },
            || clock.get(),
            |d| {
                sleeps.set(sleeps.get() + 1);
                clock.set(clock.get() + d);
            },
        );

        assert!(!acked);
        assert_eq!(probes.get(), 1);
        assert_eq!(sleeps.get(), 0);
    }

    /// errno classification is an abort *allowlist*: the only hard errors are
    /// {ENODEV, EOPNOTSUPP, EINVAL}; every other errno (including EIO and an
    /// unreadable errno) keeps polling. Guards the asymmetric-cost decision
    /// against accidental widening of the abort set.
    #[test]
    fn classify_probe_error_partitions_errnos() {
        use std::io::Error as IoError;

        // Keep-polling errnos: EIO(5), EREMOTEIO(121), EAGAIN(11), ENXIO(6),
        // ETIMEDOUT(110) — all "could still be a NACK".
        for n in [5, 121, 11, 6, 110] {
            assert_eq!(
                classify_probe_error(&LinuxI2CError::Errno(n)),
                ProbeOutcome::Nacked,
                "errno {n} should be Nacked"
            );
        }
        // Unhealable faults: ENODEV(19), EOPNOTSUPP(95), EINVAL(22).
        for n in [19, 95, 22] {
            assert_eq!(
                classify_probe_error(&LinuxI2CError::Errno(n)),
                ProbeOutcome::HardError,
                "errno {n} should be HardError"
            );
        }
        // Io arm: errno read via raw_os_error(); EIO keeps polling, ENODEV aborts.
        assert_eq!(
            classify_probe_error(&LinuxI2CError::Io(IoError::from_raw_os_error(5))),
            ProbeOutcome::Nacked
        );
        assert_eq!(
            classify_probe_error(&LinuxI2CError::Io(IoError::from_raw_os_error(19))),
            ProbeOutcome::HardError
        );
    }
}
