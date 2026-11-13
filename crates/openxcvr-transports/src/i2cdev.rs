//! Pure-Rust i2c-dev [`I2cBus`] for the keep-bound Linux path.
//!
//! Opens the adapter node through [`I2cDev`], the i2c-dev access layer shared
//! with `openxcvr_core`'s page-blind [`LinuxI2cTransport`], and joins the same
//! `I2cBus` → `paged::Memory` path HID uses. The address is claimed with plain
//! `I2C_SLAVE` per transfer — `I2C_SLAVE_FORCE` is never used, so an address
//! owned by a kernel driver fails as `EBUSY` instead of being stolen — and
//! `I2C_RDWR` carries write and repeated-start write_read pairs.
//!
//! [`LinuxI2cTransport`]: openxcvr_core::transports::LinuxI2cTransport

use std::path::Path;
use std::time::Instant;

use openxcvr_core::transports::linux_i2c::I2cDev;

use crate::bus::{Address7, I2cBus, Limits};
use crate::exclusive::DeviceLock;
use crate::{remaining, Error, ErrorKind, Result};

/// One i2c-dev adapter node under an advisory per-adapter-node lock, so a
/// second open of a held adapter reports `Busy`, never `Io`.
pub struct I2cDevBus {
    node: I2cDev,
    // Held for the lifetime of the node, like the HID per-path lock.
    _lock: DeviceLock,
}

impl I2cDevBus {
    /// Opens `path` (e.g. `/dev/i2c-1`) and takes its advisory lock.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let lock = DeviceLock::acquire(path.as_os_str())?;
        let node = I2cDev::open(path).map_err(|e| map(e, Phase::Open))?;
        Ok(Self { node, _lock: lock })
    }
}

/// Conservative plain-i2c transfer sizes: one register page per transfer keeps
/// every adapter honest (the module's own 128-byte window is the memory
/// layer's bound, not the bus's).
const LIMITS: Limits = Limits {
    write: 256,
    read: 256,
    prefix: 8,
};

impl I2cBus for I2cDevBus {
    fn limits(&self) -> Limits {
        LIMITS
    }

    fn write(&mut self, address: Address7, bytes: &[u8], deadline: Instant) -> Result<()> {
        write_via(&mut self.node, address, bytes, deadline)
    }

    fn write_read(
        &mut self,
        address: Address7,
        prefix: &[u8],
        out: &mut [u8],
        deadline: Instant,
    ) -> Result<()> {
        write_read_via(&mut self.node, address, prefix, out, deadline)
    }
}

/// The node operations the bus needs: implemented by the real i2c-dev node,
/// and by a fake in tests so the error and uncertainty paths below run
/// without hardware.
trait Node {
    fn claim(&mut self, address: u8) -> std::io::Result<bool>;
    fn transfer_write(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    fn transfer_write_read(&mut self, prefix: &[u8], out: &mut [u8]) -> std::io::Result<()>;
}

impl Node for I2cDev {
    fn claim(&mut self, address: u8) -> std::io::Result<bool> {
        I2cDev::claim(self, address)
    }
    fn transfer_write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        I2cDev::transfer_write(self, bytes)
    }
    fn transfer_write_read(&mut self, prefix: &[u8], out: &mut [u8]) -> std::io::Result<()> {
        I2cDev::transfer_write_read(self, prefix, out)
    }
}

fn write_via(
    node: &mut impl Node,
    address: Address7,
    bytes: &[u8],
    deadline: Instant,
) -> Result<()> {
    remaining(deadline)?;
    // Claim fails before anything is sent; the transfer below is at/after
    // a write boundary, so its failures are marked uncertain.
    node.claim(address.get())
        .map_err(|e| map(e, Phase::Claim))?;
    node.transfer_write(bytes)
        .map_err(|e| map(e, Phase::Transfer).uncertain())
}

fn write_read_via(
    node: &mut impl Node,
    address: Address7,
    prefix: &[u8],
    out: &mut [u8],
    deadline: Instant,
) -> Result<()> {
    remaining(deadline)?;
    node.claim(address.get())
        .map_err(|e| map(e, Phase::Claim))?;
    // The prefix is written to the device, so a failure after submission
    // may have applied it.
    node.transfer_write_read(prefix, out)
        .map_err(|e| map(e, Phase::Transfer).uncertain())
}

/// Where a failure happened: `Open`/`Claim` prove nothing was sent, only
/// `Transfer` is at or after a write boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Open,
    Claim,
    Transfer,
}

fn map(error: std::io::Error, phase: Phase) -> Error {
    let detail = format!("i2c-dev {phase:?}: {error}");
    Error::new(kind_for(&error, phase), detail)
}

/// The i2c-dev errno mapping from the design:
/// `EBUSY` → `Busy`, `EACCES`/`EPERM` → `PermissionDenied`, `ENODEV` →
/// `Disconnected`, `ENXIO`/`EREMOTEIO` on a transfer → `Nack`, `ETIMEDOUT` →
/// `Timeout`, everything else → `Io`. Two named additions: an absent adapter
/// node (`ENOENT` at open) is `Disconnected`, and a short `I2C_RDWR` is
/// `ShortTransfer` — an actionable condition is never a generic I/O error.
fn kind_for(error: &std::io::Error, phase: Phase) -> ErrorKind {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        return ErrorKind::ShortTransfer;
    }
    match error.raw_os_error() {
        // libc constants, not literals: errno numbers differ across Linux
        // architectures (ETIMEDOUT is 110 on x86-64 but 145 on mips).
        Some(libc::EBUSY) => ErrorKind::Busy,
        Some(libc::EACCES) | Some(libc::EPERM) => ErrorKind::PermissionDenied,
        Some(libc::ENODEV) => ErrorKind::Disconnected,
        Some(libc::ENOENT) if phase == Phase::Open => ErrorKind::Disconnected,
        Some(libc::ENXIO) | Some(libc::EREMOTEIO) if phase == Phase::Transfer => ErrorKind::Nack,
        Some(libc::ETIMEDOUT) => ErrorKind::Timeout,
        _ => ErrorKind::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(errno: i32) -> std::io::Error {
        std::io::Error::from_raw_os_error(errno)
    }

    #[test]
    fn errno_table_is_pinned() {
        assert_eq!(
            kind_for(&err(libc::EBUSY), Phase::Transfer),
            ErrorKind::Busy
        );
        assert_eq!(
            kind_for(&err(libc::EACCES), Phase::Open),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            kind_for(&err(libc::EPERM), Phase::Claim),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            kind_for(&err(libc::ENODEV), Phase::Transfer),
            ErrorKind::Disconnected
        );
        assert_eq!(
            kind_for(&err(libc::ENXIO), Phase::Transfer),
            ErrorKind::Nack
        );
        assert_eq!(
            kind_for(&err(libc::EREMOTEIO), Phase::Transfer),
            ErrorKind::Nack
        );
        assert_eq!(
            kind_for(&err(libc::ETIMEDOUT), Phase::Transfer),
            ErrorKind::Timeout
        );
        assert_eq!(kind_for(&err(libc::EIO), Phase::Transfer), ErrorKind::Io);
    }

    #[test]
    fn enxio_off_the_transfer_and_enoent_elsewhere_stay_io() {
        // ENXIO/EREMOTEIO are NACK only on a transfer; elsewhere the table
        // sends them to Io.
        assert_eq!(kind_for(&err(libc::ENXIO), Phase::Claim), ErrorKind::Io);
        assert_eq!(kind_for(&err(libc::EREMOTEIO), Phase::Claim), ErrorKind::Io);
        assert_eq!(kind_for(&err(libc::ENOENT), Phase::Claim), ErrorKind::Io);
    }

    #[test]
    fn absent_adapter_node_at_open_is_disconnected() {
        assert_eq!(
            kind_for(&err(libc::ENOENT), Phase::Open),
            ErrorKind::Disconnected
        );
    }

    #[test]
    fn short_transfer_is_its_own_kind() {
        let error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "short");
        assert_eq!(kind_for(&error, Phase::Transfer), ErrorKind::ShortTransfer);
    }

    /// A node that fails where the test says, recording what it was asked.
    #[derive(Default)]
    struct FakeNode {
        claim_errno: Option<i32>,
        transfer_error: Option<std::io::Error>,
        claims: Vec<u8>,
        transfers: usize,
    }

    impl Node for FakeNode {
        fn claim(&mut self, address: u8) -> std::io::Result<bool> {
            self.claims.push(address);
            match self.claim_errno {
                Some(errno) => Err(err(errno)),
                None => Ok(true),
            }
        }
        fn transfer_write(&mut self, _bytes: &[u8]) -> std::io::Result<()> {
            self.transfers += 1;
            self.transfer_error.take().map_or(Ok(()), Err)
        }
        fn transfer_write_read(&mut self, _prefix: &[u8], out: &mut [u8]) -> std::io::Result<()> {
            self.transfers += 1;
            match self.transfer_error.take() {
                Some(e) => Err(e),
                None => {
                    out.fill(0xa5);
                    Ok(())
                }
            }
        }
    }

    fn deadline() -> Instant {
        Instant::now() + std::time::Duration::from_secs(1)
    }

    fn addr() -> Address7 {
        Address7::new(0x50).unwrap()
    }

    #[test]
    fn claim_failure_sends_nothing_and_is_not_uncertain() {
        for (errno, kind) in [
            (libc::EBUSY, ErrorKind::Busy),
            (libc::EACCES, ErrorKind::PermissionDenied),
            (libc::ENODEV, ErrorKind::Disconnected),
        ] {
            let mut node = FakeNode {
                claim_errno: Some(errno),
                ..FakeNode::default()
            };
            let e = write_via(&mut node, addr(), &[0x7f, 0x00], deadline()).unwrap_err();
            assert_eq!(e.kind, kind);
            assert!(
                !e.write_may_have_applied,
                "claim failed before any transfer"
            );
            assert_eq!(node.transfers, 0);
            let mut out = [0u8; 2];
            let e = write_read_via(&mut node, addr(), &[0x00], &mut out, deadline()).unwrap_err();
            assert_eq!(e.kind, kind);
            assert!(!e.write_may_have_applied);
            assert_eq!(node.transfers, 0);
        }
    }

    #[test]
    fn transfer_failure_is_uncertain_and_mapped() {
        let cases = [
            (err(libc::ENXIO), ErrorKind::Nack),
            (err(libc::EREMOTEIO), ErrorKind::Nack),
            (err(libc::ETIMEDOUT), ErrorKind::Timeout),
            (err(libc::EBUSY), ErrorKind::Busy),
            (err(libc::EIO), ErrorKind::Io),
            (
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "1 of 2"),
                ErrorKind::ShortTransfer,
            ),
        ];
        for (error, kind) in cases {
            let copy = std::io::Error::new(error.kind(), error.to_string());
            let raw = error.raw_os_error();
            let mut node = FakeNode {
                transfer_error: Some(error),
                ..FakeNode::default()
            };
            let e = write_via(&mut node, addr(), &[0x7f, 0x00], deadline()).unwrap_err();
            assert_eq!(e.kind, kind, "write {e}");
            assert!(
                e.write_may_have_applied,
                "a submitted write may have applied"
            );
            assert_eq!((node.claims.as_slice(), node.transfers), (&[0x50][..], 1));

            let again = match raw {
                Some(errno) => err(errno),
                None => copy,
            };
            let mut node = FakeNode {
                transfer_error: Some(again),
                ..FakeNode::default()
            };
            let mut out = [0x11u8; 2];
            let e = write_read_via(&mut node, addr(), &[0x00], &mut out, deadline()).unwrap_err();
            assert_eq!(e.kind, kind, "write_read {e}");
            assert!(e.write_may_have_applied, "the prefix was submitted");
        }
    }

    #[test]
    fn expired_deadline_sends_nothing() {
        let mut node = FakeNode::default();
        let past = Instant::now() - std::time::Duration::from_millis(1);
        let e = write_via(&mut node, addr(), &[0x00], past).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Timeout);
        assert!(!e.write_may_have_applied);
        assert_eq!((node.claims.len(), node.transfers), (0, 0));
        let mut out = [0x11u8; 2];
        let e = write_read_via(&mut node, addr(), &[0x00], &mut out, past).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Timeout);
        assert!(!e.write_may_have_applied);
        assert_eq!((node.claims.len(), node.transfers), (0, 0));
        assert_eq!(out, [0x11; 2], "nothing was read");
    }

    #[test]
    fn successful_write_read_claims_then_transfers_once() {
        let mut node = FakeNode::default();
        let mut out = [0u8; 3];
        write_read_via(&mut node, addr(), &[0x00], &mut out, deadline()).unwrap();
        assert_eq!(out, [0xa5; 3]);
        assert_eq!((node.claims.as_slice(), node.transfers), (&[0x50][..], 1));
    }
}
