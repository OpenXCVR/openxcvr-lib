#![cfg(all(target_os = "linux", feature = "hid", feature = "linux-i2c"))]

//! The public error model end to end: every `HostState`'s `error()` kind,
//! `Error::uncertain`, and the open-path kinds of `NativeHid` and `I2cDevBus`.
//!
//! Scope note: the MCP2221A HID report converter lives in the parent's
//! xcvr-bridge repository, and the i2c-dev errno table is private to
//! `src/i2cdev.rs`, pinned there by its unit tests. This file covers what is
//! public from `openxcvr_transports`.

use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use openxcvr_transports::exclusive::DeviceLock;
use openxcvr_transports::hid::NativeHid;
use openxcvr_transports::host_state::{BoundDriver, BusyReason, HostState, Transport};
use openxcvr_transports::i2cdev::I2cDevBus;
use openxcvr_transports::{Error, ErrorKind};

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

fn device_name(profile: support::Profile) -> String {
    format!(
        "0003:{:04X}:{:04X}.0001",
        profile.vendor_id, profile.product_id
    )
}

fn driver_name(profile: support::Profile) -> String {
    profile.kernel_driver.trim_start_matches("hid_").to_owned()
}

fn temp_path(kind: &str, profile: support::Profile, suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openxcvr-error-mapping-{kind}-{}-{suffix}",
        profile.kernel_driver
    ))
}

/// Every `HostState` names the error an open must report for it, and only
/// `Usable` reports none.
#[test]
fn host_state_error_kinds_match_the_public_error_model() {
    for profile in support::PROFILES {
        let device = device_name(profile);
        let node = PathBuf::from("/dev/hidraw0");
        let driver = BoundDriver {
            name: driver_name(profile),
            module: Some(profile.kernel_driver.to_owned()),
            expected: true,
        };
        let cases: [(HostState, ErrorKind); 8] = [
            (HostState::Absent, ErrorKind::Disconnected),
            (
                HostState::NotPermitted { node: node.clone() },
                ErrorKind::PermissionDenied,
            ),
            (
                HostState::Busy {
                    node: node.clone(),
                    reason: BusyReason::AdvisoryLock,
                },
                ErrorKind::Busy,
            ),
            (
                HostState::Busy {
                    node: node.clone(),
                    reason: BusyReason::KernelClient {
                        driver: driver_name(profile),
                    },
                },
                ErrorKind::Busy,
            ),
            (
                HostState::NoHidrawNode {
                    device: device.clone(),
                    driver: Some(driver.clone()),
                },
                ErrorKind::Unsupported,
            ),
            (
                HostState::NoI2cAdapter {
                    device: device.clone(),
                    driver: Some(driver.clone()),
                    adapter: Some("i2c-3".to_owned()),
                },
                ErrorKind::Unsupported,
            ),
            (
                HostState::Ambiguous {
                    devices: vec![device.clone(), device.clone()],
                },
                ErrorKind::Invalid,
            ),
            (
                HostState::Inconclusive {
                    path: node.clone(),
                    detail: "unreadable".to_owned(),
                },
                ErrorKind::Io,
            ),
        ];
        for (state, expected) in cases {
            assert!(!state.is_usable(), "{state:?} must not be usable");
            let error = state
                .error()
                .unwrap_or_else(|| panic!("{state:?} must name an error"));
            assert_eq!(error.kind, expected, "{}: {state:?}", profile.kernel_driver);
            assert!(!error.write_may_have_applied);
        }
        let usable = HostState::Usable {
            transport: Transport::Hid,
            node,
        };
        assert!(usable.is_usable());
        assert!(usable.error().is_none());
    }
}

/// `uncertain` flips `write_may_have_applied` and nothing else.
#[test]
fn uncertain_marks_only_write_may_have_applied() {
    for profile in support::PROFILES {
        let error = Error::new(ErrorKind::Timeout, "lost completion");
        assert!(!error.write_may_have_applied);
        let marked = error.uncertain();
        assert!(marked.write_may_have_applied, "{}", profile.kernel_driver);
        assert_eq!(marked.kind, ErrorKind::Timeout);
        assert_eq!(marked.detail, "lost completion");
    }
}

/// `NativeHid::open_path` kinds: an absent node is `Disconnected`, a held
/// advisory lock is `Busy` before hidapi is reached.
#[test]
fn native_hid_open_path_error_kinds() {
    for profile in support::PROFILES {
        let absent = temp_path("hid", profile, "absent");
        let absent = CString::new(absent.into_os_string().into_vec()).unwrap();
        match NativeHid::open_path(&absent) {
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

        let held_path = temp_path("hid", profile, "held");
        let held = DeviceLock::acquire(held_path.as_os_str()).expect("first advisory lock");
        let held_c = CString::new(held_path.into_os_string().into_vec()).unwrap();
        match NativeHid::open_path(&held_c) {
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

/// `I2cDevBus::open` kinds: an absent node is `Disconnected`, an unpermitted
/// node is `PermissionDenied`, a held advisory lock is `Busy`.
#[test]
fn i2c_dev_bus_open_error_kinds() {
    for profile in support::PROFILES {
        let absent = temp_path("i2cdev", profile, "absent");
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

        // A mode-0000 node is only unpermitted for a non-root user.
        if !running_as_root() {
            let denied = temp_path("i2cdev", profile, "denied");
            std::fs::write(&denied, b"").unwrap();
            std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000)).unwrap();
            let result = I2cDevBus::open(&denied);
            let _ = std::fs::remove_file(&denied);
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

        let held_path = temp_path("i2cdev", profile, "held");
        let held = DeviceLock::acquire(held_path.as_os_str()).expect("first advisory lock");
        match I2cDevBus::open(&held_path) {
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
