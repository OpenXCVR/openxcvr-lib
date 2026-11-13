#![cfg(all(target_os = "linux", feature = "linux-i2c"))]

//! `I2cDevBus` open and claim error paths over plain files, and the proof
//! that an `I2cDevBus` joins the same `I2cBus` -> `paged::Memory` path HID
//! uses.
//!
//! Named gap: re-running the paged suite against a live i2c-dev node needs an
//! i2c adapter this host lacks (no /dev/i2c-* for a module — its only node,
//! /dev/i2c-0, is root-owned with no module behind it), so that run is a named
//! gap for the evidence task and only the file-backed error paths are
//! exercised here.

use std::time::Duration;

use openxcvr_transports::exclusive::DeviceLock;
use openxcvr_transports::i2cdev::I2cDevBus;
use openxcvr_transports::paged::{Layout, Memory, Timing};
use openxcvr_transports::{ErrorKind, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

#[allow(dead_code)]
mod support;

/// True when the effective user is root, per `id -u`. libc is not a
/// dev-dependency, so the id command stands in for `geteuid`.
fn running_as_root() -> bool {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .expect("id -u must run");
    String::from_utf8_lossy(&output.stdout).trim() == "0"
}

fn timing() -> Timing {
    Timing {
        write_block: 8,
        write_settle: Duration::ZERO,
        select_settle: Duration::ZERO,
        timeout: Duration::from_secs(1),
    }
}

/// A plain file under `std::env::temp_dir()`, created at `mode`, removed by
/// the caller once the open is observed.
fn temp_file(kind: &str, profile: support::Profile, mode: u32) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "openxcvr-i2cdev-bus-{kind}-{}-{}",
        std::process::id(),
        profile.kernel_driver
    ));
    fs::write(&path, b"").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

/// An `I2cDevBus` is an `I2cBus`, so `Memory::new` takes it unchanged.
fn memory_over(bus: I2cDevBus) -> Result<Memory<I2cDevBus>> {
    Memory::new(bus, Layout::Cmis { flat: true }, timing())
}

/// An absent adapter node is `Disconnected` at open.
#[test]
fn open_of_an_absent_node_is_disconnected() {
    for profile in support::PROFILES {
        let absent = std::env::temp_dir().join(format!(
            "openxcvr-i2cdev-bus-absent-{}",
            profile.kernel_driver
        ));
        match I2cDevBus::open(&absent) {
            Err(error) => assert_eq!(
                error.kind,
                ErrorKind::Disconnected,
                "{}: {error}",
                profile.kernel_driver
            ),
            Ok(_) => panic!(
                "{}: open of an absent node must fail",
                profile.kernel_driver
            ),
        }
    }
}

/// A mode-0000 node is `PermissionDenied` at open (skipped as root).
#[test]
fn open_of_an_unpermitted_node_is_permission_denied() {
    if running_as_root() {
        eprintln!("skipped: root passes every permission check");
        return;
    }
    for profile in support::PROFILES {
        let denied = temp_file("denied", profile, 0o000);
        let result = I2cDevBus::open(&denied);
        let _ = fs::remove_file(&denied);
        match result {
            Err(error) => assert_eq!(
                error.kind,
                ErrorKind::PermissionDenied,
                "{}: {error}",
                profile.kernel_driver
            ),
            Ok(_) => panic!(
                "{}: open of an unpermitted node must fail",
                profile.kernel_driver
            ),
        }
    }
}

/// A second open of a path whose advisory lock is held reports `Busy`, never
/// `Io`.
#[test]
fn second_open_of_a_held_path_is_busy() {
    for profile in support::PROFILES {
        let path = temp_file("held", profile, 0o600);
        let held = DeviceLock::acquire(path.as_os_str()).expect("first advisory lock");
        let result = I2cDevBus::open(&path);
        let _ = fs::remove_file(&path);
        match result {
            Err(error) => assert_eq!(
                error.kind,
                ErrorKind::Busy,
                "{}: {error}",
                profile.kernel_driver
            ),
            Ok(_) => panic!(
                "{}: a second open of a held path must fail",
                profile.kernel_driver
            ),
        }
        drop(held);
    }
}

/// A node that is not an i2c-dev adapter fails at open: `I2cDev::open` issues
/// the plain `I2C_SLAVE` ioctl (placeholder address) when it opens the node, so
/// a regular file reports ENOTTY as `Io` before any transfer, and nothing was
/// sent, so the error is not marked uncertain.
#[test]
fn non_adapter_node_fails_at_open_without_uncertainty() {
    for profile in support::PROFILES {
        let path = temp_file("regular", profile, 0o600);
        let opened = I2cDevBus::open(&path);
        let _ = fs::remove_file(&path);
        let error = match opened {
            Err(error) => error,
            Ok(_) => panic!(
                "{}: a regular file is not an adapter",
                profile.kernel_driver
            ),
        };
        assert_eq!(
            error.kind,
            ErrorKind::Io,
            "{}: {error}",
            profile.kernel_driver
        );
        assert!(
            !error.write_may_have_applied,
            "{}: open sends nothing",
            profile.kernel_driver
        );
    }
}

/// `Memory` takes an `I2cDevBus` unchanged: `memory_over` compiles, so the
/// i2c-dev bus joins the same paged path HID uses. Running the paged suite
/// against a live adapter needs an i2c-dev node with a module behind it, which
/// the development and CI hosts lack; that run is a named gap for the evidence task, not a test here.
#[test]
fn i2cdev_bus_joins_the_paged_path() {
    let join: fn(I2cDevBus) -> Result<Memory<I2cDevBus>> = memory_over;
    let _ = join;
}
