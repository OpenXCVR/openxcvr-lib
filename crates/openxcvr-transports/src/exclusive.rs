//! Advisory cross-process exclusivity for device opens.
//!
//! One lock per key (a HID device path, an i2c adapter node). Held for the
//! lifetime of the device handle, so a second open of a held device reports
//! [`ErrorKind::Busy`], never [`ErrorKind::Io`], on every backend.
//!
//! The lock binds cooperating processes only: a foreign program that ignores
//! the lock file can still open the device (Windows shares hidraw-style device
//! opens, and i2c-dev has no kernel ownership). The receipt states busy
//! behaviour for cooperating processes, never universal exclusive ownership.
//!
//! On Unix, lock lifetime follows the open file description: the lock persists
//! while any descriptor, including one duplicated or inherited by a forked
//! child, refers to that description. Dropping a [`DeviceLock`] closes that
//! handle's descriptor; existing close-on-exec ends pre-exec copies on
//! successful `execve`.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::{Error, ErrorKind, Result};

/// An advisory per-key lock, released when the handle is dropped.
///
/// On Unix, the lock's lifetime follows the underlying open file description:
/// the lock persists as long as any open descriptor refers to that description,
/// including copies duplicated across `fork` or handle duplication. Dropping the
/// handle closes that handle's descriptor; if an inherited or duplicated copy
/// remains open (such as in a concurrently spawned child prior to `execve`),
/// the lock remains held until that copy is closed or closed on exec. No explicit
/// unlock is performed on drop, and no parent-only release or fork-refusal
/// guarantee is provided.
#[derive(Debug)]
pub struct DeviceLock {
    // Holding the file keeps the OS lock alive.
    _file: File,
}

impl DeviceLock {
    /// Takes the advisory lock for `key`. Reports `Busy` when another
    /// cooperating process already holds it.
    pub fn acquire(key: &OsStr) -> Result<Self> {
        let path = lock_path(key)?;
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| Error::new(ErrorKind::Io, format!("advisory lock file: {e}")))?;
        match try_lock(&file) {
            Ok(true) => Ok(Self { _file: file }),
            Ok(false) => Err(Error::new(
                ErrorKind::Busy,
                format!("device is held by another process (lock {:?})", path),
            )),
            Err(e) => Err(Error::new(ErrorKind::Io, format!("advisory lock: {e}"))),
        }
    }
}

/// Lock path is independent of a caller's runtime environment. Existing
/// device aliases resolve to the same node; absent paths retain their key.
fn lock_path(key: &OsStr) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(Path::new(key)).unwrap_or_else(|_| PathBuf::from(key));
    Ok(lock_dir()?.join(format!(
        "openxcvr-{:016x}.lock",
        fnv1a64(canonical.as_os_str().as_encoded_bytes())
    )))
}

#[cfg(unix)]
fn lock_dir() -> Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    extern "C" {
        fn geteuid() -> u32;
    }
    // A stable, private per-UID namespace: XDG_RUNTIME_DIR is per-session and
    // may not exist for a cooperating process. Never use an untrusted shared
    // lock file or silently fall back to a different namespace.
    let uid = unsafe { geteuid() };
    let dir = PathBuf::from(format!("/tmp/openxcvr-locks-{uid}"));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .or_else(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(e)
            }
        })
        .map_err(|e| Error::new(ErrorKind::Io, format!("advisory lock directory: {e}")))?;
    let meta = std::fs::symlink_metadata(&dir)
        .map_err(|e| Error::new(ErrorKind::Io, format!("advisory lock directory: {e}")))?;
    if !meta.file_type().is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "unsafe advisory lock directory",
        ));
    }
    Ok(dir)
}

#[cfg(not(unix))]
fn lock_dir() -> Result<PathBuf> {
    Ok(std::env::temp_dir())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `Ok(true)` = acquired, `Ok(false)` = held elsewhere, `Err` = OS failure.
///
/// A lock held by another open file description reports EAGAIN on Linux and
/// EWOULDBLOCK on macOS; `std` maps both to `io::ErrorKind::WouldBlock`.
/// (`File::try_lock` would do this too, but needs Rust 1.89; MSRV is 1.82.)
#[cfg(unix)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;

    // SAFETY: the descriptor is owned by `file` and valid for the call.
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(error)
    }
}

/// `Ok(true)` = acquired, `Ok(false)` = held elsewhere, `Err` = OS failure.
#[cfg(windows)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut std::ffi::c_void,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LockFileEx(
            file: *mut std::ffi::c_void,
            flags: u32,
            reserved: u32,
            low: u32,
            high: u32,
            overlapped: *mut Overlapped,
        ) -> i32;
    }
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;
    const ERROR_LOCK_VIOLATION: i32 = 33;

    // LockFileEx requires a valid OVERLAPPED even for a synchronous handle.
    let mut overlapped = Overlapped {
        internal: 0,
        internal_high: 0,
        offset: 0,
        offset_high: 0,
        event: std::ptr::null_mut(),
    };
    // SAFETY: the handle is owned by `file`; `overlapped` outlives the call.
    let acquired = unsafe {
        LockFileEx(
            file.as_raw_handle() as *mut _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if acquired != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_of_a_held_key_reports_busy() {
        let _guard = crate::test_guard();
        let key = OsStr::new("/dev/i2c-test-held");
        let held = DeviceLock::acquire(key).expect("first acquire");
        let error = DeviceLock::acquire(key).expect_err("second acquire must fail");
        assert_eq!(error.kind, ErrorKind::Busy);
        drop(held);
        DeviceLock::acquire(key).expect("released on drop");
    }

    #[test]
    fn different_keys_do_not_collide() {
        let a = DeviceLock::acquire(OsStr::new("/dev/i2c-test-a")).expect("lock a");
        let b = DeviceLock::acquire(OsStr::new("/dev/i2c-test-b")).expect("lock b");
        drop((a, b));
    }

    #[cfg(unix)]
    #[test]
    fn held_node_is_busy_across_runtime_dirs_and_path_aliases() {
        let _guard = crate::test_guard();
        use std::os::unix::fs::symlink;
        let dir = std::env::temp_dir().join(format!("openxcvr-lock-test-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let node = dir.join("node");
        let alias = dir.join("alias");
        std::fs::write(&node, []).unwrap();
        symlink(&node, &alias).unwrap();
        let held = DeviceLock::acquire(node.as_os_str()).unwrap();
        for path in [&node, &alias] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg("exclusive::tests::child_checks_lock")
                .arg("--ignored")
                .arg("--nocapture")
                .env("XDG_RUNTIME_DIR", dir.join("other-runtime"))
                .env("OPENXCVR_LOCK_TEST_ALIAS", path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(
                result.success(),
                "second process bypassed advisory lock for {path:?}"
            );
        }
        drop(held);
        DeviceLock::acquire(alias.as_os_str()).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_handle_preserves_lock_until_last_close() {
        let _guard = crate::test_guard();
        let key = OsStr::new("/dev/i2c-test-dup");
        let held = DeviceLock::acquire(key).expect("first acquire");
        let duplicate = held._file.try_clone().expect("try_clone file");
        drop(held);
        let error =
            DeviceLock::acquire(key).expect_err("lock must persist while duplicate is open");
        assert_eq!(error.kind, ErrorKind::Busy);
        drop(duplicate);
        DeviceLock::acquire(key).expect("released once all file copies close");
    }

    #[cfg(unix)]
    #[ignore = "spawned only by held_node_is_busy_across_runtime_dirs_and_path_aliases"]
    #[test]
    fn child_checks_lock() {
        if let Some(alias) = std::env::var_os("OPENXCVR_LOCK_TEST_ALIAS") {
            assert_eq!(
                DeviceLock::acquire(&alias).unwrap_err().kind,
                ErrorKind::Busy
            );
        }
    }
}
