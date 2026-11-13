//! Synchronous transport building blocks. Device protocols belong to their adapters.
//!
//! # Error contract
//!
//! [`Error`] is the one error type at this crate's transport boundary, so every
//! backend — HID, Linux i2c-dev, or a test double — is classified once by the
//! caller. The kinds name the condition a caller can act on, independently of
//! the backend that reported it (`EBUSY` for i2c-dev, a bound kernel driver or a
//! held advisory lock for HID, a message string on the pinned hidapi backend);
//! `detail` carries the backend's own text for logs and never decides control
//! flow. [`Error::write_may_have_applied`] is the one piece of state a caller
//! must respect: once set, the device may already hold the requested write, so
//! the write is never replayed implicitly and the caller re-reads the affected
//! bytes before deciding what to do next.
//!
//! [`bus::I2cBus`] and [`paged::Memory`] share this error type on the memory
//! path: the bus maps its backend failures and marks the ones it cannot prove
//! did not reach the device, and the memory layer marks the selection writes it
//! submits plus any write boundary it crossed before failing.
//!
//! `pinned` is an internal Linux x86_64 prototype (feature `linux-i2c`,
//! including builds that also enable `hid`; `hid` alone excludes it). Its
//! retained fd proves post-open pathname-retarget continuity, not
//! enumeration-to-open identity. Published open paths keep their weaker
//! contract; no identity-sensitive integration is authorized by this proof.
#![warn(missing_docs)]
pub mod bus;
pub mod classify;
pub mod exclusive;
pub mod hid;
#[cfg(any(feature = "hid", feature = "linux-i2c"))]
pub mod host_state;
#[cfg(all(feature = "linux-i2c", target_os = "linux"))]
pub mod i2cdev;
pub mod paged;
// Experimental ioctl/dev_t layout is verified only for Linux x86_64.
// `hid` alone never enables the i2c-dev-shaped pinned prototype.
#[cfg(all(feature = "linux-i2c", target_os = "linux", target_arch = "x86_64"))]
mod pinned;
// Internal, unintegrated Linux x86_64 shared-inode OFD lock primitive.
#[cfg(all(
    any(feature = "hid", feature = "linux-i2c"),
    target_os = "linux",
    target_arch = "x86_64"
))]
mod ofd_lock;

use std::{fmt, time::Instant};

/// The condition a transport failure names, independent of the backend that
/// reported it. Callers branch on the kind; the backend's own text stays in
/// [`Error::detail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A caller-supplied argument is out of range or self-contradictory; nothing was sent.
    Invalid,
    /// The address, page, bank, or operation does not exist for the supplied layout.
    Unsupported,
    /// The device, address, or device node is owned by someone else (i2c-dev `EBUSY`,
    /// a bound kernel driver, another process's lock, or an unpowered module).
    Busy,
    /// The device exists but this process may not open or address it
    /// (i2c-dev `EACCES`/`EPERM`).
    PermissionDenied,
    /// No such device, or it was removed or lost power during the transfer
    /// (i2c-dev `ENODEV`).
    Disconnected,
    /// The transfer deadline expired before the operation completed
    /// (i2c-dev `ETIMEDOUT`).
    Timeout,
    /// The slave did not acknowledge the transfer (i2c-dev `ENXIO`/`EREMOTEIO`).
    Nack,
    /// The device answered, but the bytes violate the protocol.
    Protocol,
    /// Fewer bytes moved than were requested.
    ShortTransfer,
    /// Any other backend failure, carried as text in `detail`.
    Io,
}

/// The one error type at this crate's transport boundary (see the crate-level
/// error contract). `kind` decides control flow; `detail` is for logs only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The actionable condition, classified independently of the backend.
    pub kind: ErrorKind,
    /// Backend-supplied diagnostic text for logs; never decides control flow.
    pub detail: String,
    /// A submitted write may have reached the device. Never replay it implicitly:
    /// set at every write boundary, including page and bank select writes.
    pub write_may_have_applied: bool,
}

impl Error {
    /// Classify a failure: an [`ErrorKind`] a caller can act on, plus the
    /// backend's own diagnostic text. The error starts certain that nothing
    /// was applied — mark it with [`Error::uncertain`] at a write boundary.
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            write_may_have_applied: false,
        }
    }
    /// Marks a failure at or after a write boundary. Idempotent, so a layer may
    /// mark an error a lower layer already marked.
    pub fn uncertain(mut self) -> Self {
        self.write_may_have_applied = true;
        self
    }

    /// True when the transfer deadline expired before the operation
    /// completed. The caller may retry with the same or a later deadline;
    /// the backend never retries internally.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.kind == ErrorKind::Timeout
    }

    /// True when resubmitting the **identical** operation is not known-harmful
    /// and may succeed once the underlying condition clears.
    ///
    /// Retryable kinds are [`ErrorKind::Busy`] (another owner holds the
    /// device and may release it), [`ErrorKind::Timeout`] (the deadline
    /// expired; a resubmission may finish in time), and [`ErrorKind::Nack`]
    /// (the slave did not acknowledge — often an unpowered or momentarily
    /// unready module; a persistent Nack behaves like
    /// [`ErrorKind::Disconnected`]).
    ///
    /// A failure whose write may have reached the device is **never**
    /// retryable: replaying it could double-apply the write, which the
    /// crate's error contract forbids. Retry policy — delay, backoff, budget —
    /// stays with the caller; this predicate only says resubmission is safe.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        !self.write_may_have_applied
            && matches!(
                self.kind,
                ErrorKind::Busy | ErrorKind::Timeout | ErrorKind::Nack
            )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)?;
        if self.write_may_have_applied {
            write!(f, " (write may have applied)")?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

/// Result alias for the transport boundary: every fallible transport-path
/// operation fails with [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Time left before `deadline`, or [`ErrorKind::Timeout`] once it has passed
/// (a deadline with exactly zero time left has already expired). Backends use
/// this to derive each fragment's timeout from one caller-supplied deadline,
/// so a multi-fragment transfer never outlives the caller's intent.
#[must_use = "the remaining time is a pure read of the supplied deadline"]
pub fn remaining(deadline: Instant) -> Result<std::time::Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| Error::new(ErrorKind::Timeout, "operation deadline expired"))
}

#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, PoisonError};

    // Linux `fork()` duplicates all open file descriptors into the child;
    // `clone()` does the same unless `CLONE_FILES` is specified. Even
    // with `O_CLOEXEC` / `FD_CLOEXEC` set, the child retains open descriptor
    // copies referencing the parent's open file descriptions (OFDs) during the
    // fork window until `execve` completes or the child exits.
    //
    // Linux advisory locks (`flock(2)` and OFD locks) are associated with the
    // underlying open file description. If a test thread holds an advisory lock
    // while another test concurrently forks or spawns a child, the child inherits
    // an open descriptor referencing that same OFD. If the parent test drops its
    // guard (closing the parent's descriptor) and immediately attempts to
    // re-acquire the lock on the same key before the child execs or exits, the
    // re-acquire fails with `ErrorKind::Busy` because the lock remains held by the
    // child's inherited OFD copy.
    //
    // This crate-private coordination mutex serializes tests within this unit-test
    // binary that drop a lock and expect an immediate post-drop acquire on the same
    // key, or that fork/spawn subprocesses while descriptors may be open. Tests hold
    // this guard across their entire sequence (before opening descriptors or
    // fixtures, through any child completion and post-drop assertions).
    //
    // Raw-fork child paths must never acquire or unlock this mutex; exec-selected
    // helpers have fresh process state and fresh mutex state.
    static COORDINATION: Mutex<()> = Mutex::new(());
    COORDINATION.lock().unwrap_or_else(PoisonError::into_inner)
}
