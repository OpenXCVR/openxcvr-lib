#![cfg(all(target_os = "linux", any(feature = "hid", feature = "linux-i2c")))]

//! Host-state diagnosis against a fake `/sys` and `/dev`, per requested
//! transport and per adapter profile.
//!
//! Nothing here binds, unbinds, installs, loads a module or changes host
//! permissions: every tree is a plain directory under `std::env::temp_dir()`,
//! removed when the test ends.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use openxcvr_transports::exclusive::DeviceLock;
use openxcvr_transports::host_state::{BusyReason, Diagnosis, HostState, Identity, Transport};
use openxcvr_transports::ErrorKind;

#[allow(dead_code)]
mod support;

/// True when the effective user is root, per `id -u`. libc is not a
/// dev-dependency, so the id command stands in for `geteuid`.
fn running_as_root() -> bool {
    let output = std::process::Command::new("id")
        .arg("-u")
        .output()
        .expect("id -u must run");
    String::from_utf8_lossy(&output.stdout).trim() == "0"
}

/// Identity as each profile supplies it: `kernel_driver` is the profile's own
/// (module) name.
fn identity(profile: support::Profile) -> Identity {
    Identity {
        vendor_id: profile.vendor_id,
        product_id: profile.product_id,
        kernel_driver: profile.kernel_driver.to_owned(),
        hidraw_class: profile.hidraw_class.to_owned(),
        i2c_class: profile.i2c_adapter_class.to_owned(),
        module_address: 0x50,
    }
}

/// sysfs HID device directory name: `BBBB:VVVV:PPPP.NNNN`, uppercase hex.
fn device_name(profile: support::Profile) -> String {
    format!(
        "0003:{:04X}:{:04X}.0001",
        profile.vendor_id, profile.product_id
    )
}

/// The sysfs driver name is `kernel_driver` with its `hid_` prefix stripped
/// (the MCP2221A's driver is `mcp2221`, its module `hid_mcp2221`).
fn driver_name(profile: support::Profile) -> &'static str {
    profile
        .kernel_driver
        .strip_prefix("hid_")
        .unwrap_or(profile.kernel_driver)
}

/// A fake `/sys` and `/dev` in a fresh temporary directory, removed on drop.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "openxcvr-host-state-it-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("sys/bus/hid/devices")).unwrap();
        fs::create_dir_all(root.join("dev")).unwrap();
        Tree { root }
    }

    fn diagnosis(&self) -> Diagnosis {
        Diagnosis::new(self.root.join("sys"), self.root.join("dev"))
    }

    fn device(&self, name: &str) -> PathBuf {
        let dir = self.root.join("sys/bus/hid/devices").join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Binds `driver` (module `module`) to the device: `driver` symlink to the
    /// drivers directory, whose `module` symlink names the module.
    fn bind(&self, device: &Path, driver: &str, module: &str) {
        let drv = self.root.join("sys/bus/hid/drivers").join(driver);
        fs::create_dir_all(&drv).unwrap();
        let module_dir = self.root.join("sys/module").join(module);
        fs::create_dir_all(&module_dir).unwrap();
        symlink(&module_dir, drv.join("module")).unwrap();
        symlink(&drv, device.join("driver")).unwrap();
    }

    fn node(&self, name: &str, mode: u32) -> PathBuf {
        let path = self.root.join("dev").join(name);
        fs::write(&path, b"").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    /// `<device>/hidraw/<node>/` plus the `<dev>/<node>` device node.
    fn hidraw(&self, device: &Path, node: &str, mode: u32) -> PathBuf {
        fs::create_dir_all(device.join("hidraw").join(node)).unwrap();
        self.node(node, mode)
    }

    /// `<device>/i2c-<bus>/i2c-dev/i2c-<bus>/` plus the `<dev>/i2c-<bus>` node.
    fn i2c(&self, device: &Path, bus: u32, mode: u32) -> PathBuf {
        let name = format!("i2c-{bus}");
        fs::create_dir_all(device.join(&name).join("i2c-dev").join(&name)).unwrap();
        self.node(&name, mode)
    }

    /// A kernel client driver bound at `address` on the adapter: the
    /// `<device>/i2c-<bus>/<bus>-<addr>/driver` symlink.
    fn kernel_client(&self, device: &Path, bus: u32, address: u8, driver: &str) {
        let dir = device
            .join(format!("i2c-{bus}"))
            .join(format!("{bus}-{address:04x}"));
        fs::create_dir_all(&dir).unwrap();
        symlink(
            self.root.join("sys/bus/hid/drivers").join(driver),
            dir.join("driver"),
        )
        .unwrap();
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// No device with the caller's IDs enumerates: `Absent` for both transports.
#[test]
fn absent_device_is_absent_for_both_transports() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        for transport in [Transport::Hid, Transport::I2cDev] {
            let state = tree.diagnosis().diagnose(&id, transport);
            assert!(
                matches!(state, HostState::Absent),
                "{} {transport:?}: {state:?}",
                profile.kernel_driver
            );
            assert_eq!(
                state.error().unwrap().kind,
                ErrorKind::Disconnected,
                "{} {transport:?}",
                profile.kernel_driver
            );
        }
    }
}

/// Driver bound but no hidraw node: the keep-bound configuration. The HID
/// path is `NoHidrawNode`, never `Busy`; the i2c-dev path is `Usable`.
#[test]
fn bound_driver_without_a_hidraw_node_is_unsupported_for_hid_and_usable_for_i2c() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        let device = tree.device(&device_name(profile));
        tree.bind(&device, driver_name(profile), profile.kernel_driver);
        tree.i2c(&device, 3, 0o600);

        let hid = tree.diagnosis().diagnose(&id, Transport::Hid);
        match &hid {
            HostState::NoHidrawNode { device, driver } => {
                assert_eq!(device, &device_name(profile));
                let driver = driver
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: a driver is bound", profile.kernel_driver));
                assert!(driver.expected, "{}: {driver:?}", profile.kernel_driver);
                assert_eq!(driver.name, driver_name(profile));
                assert_eq!(driver.module.as_deref(), Some(profile.kernel_driver));
            }
            other => panic!(
                "{}: expected NoHidrawNode, got {other:?}",
                profile.kernel_driver
            ),
        }
        assert_eq!(hid.error().unwrap().kind, ErrorKind::Unsupported);

        let i2c = tree.diagnosis().diagnose(&id, Transport::I2cDev);
        assert!(i2c.is_usable(), "{}: {i2c:?}", profile.kernel_driver);
    }
}

/// An unpermitted hidraw node is `NotPermitted` for the HID path while the
/// i2c-dev path stays `Usable`.
#[test]
fn unpermitted_hidraw_node_is_not_permitted_for_hid_while_i2c_stays_usable() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        let device = tree.device(&device_name(profile));
        tree.bind(&device, driver_name(profile), profile.kernel_driver);
        tree.hidraw(&device, "hidraw0", 0o000);
        tree.i2c(&device, 3, 0o600);

        let hid = tree.diagnosis().diagnose(&id, Transport::Hid);
        if running_as_root() {
            eprintln!("skipped: root passes every permission check");
        } else {
            assert!(
                matches!(hid, HostState::NotPermitted { .. }),
                "{}: {hid:?}",
                profile.kernel_driver
            );
            assert_eq!(hid.error().unwrap().kind, ErrorKind::PermissionDenied);
        }

        let i2c = tree.diagnosis().diagnose(&id, Transport::I2cDev);
        assert!(i2c.is_usable(), "{}: {i2c:?}", profile.kernel_driver);
    }
}

/// An unpermitted i2c-dev node is `NotPermitted` for the i2c-dev path only;
/// the HID path is `Usable`.
#[test]
fn unpermitted_i2c_node_is_not_permitted_for_i2c_only() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        let device = tree.device(&device_name(profile));
        tree.bind(&device, driver_name(profile), profile.kernel_driver);
        tree.hidraw(&device, "hidraw0", 0o600);
        tree.i2c(&device, 3, 0o000);

        let hid = tree.diagnosis().diagnose(&id, Transport::Hid);
        assert!(hid.is_usable(), "{}: {hid:?}", profile.kernel_driver);

        let i2c = tree.diagnosis().diagnose(&id, Transport::I2cDev);
        if running_as_root() {
            eprintln!("skipped: root passes every permission check");
        } else {
            assert!(
                matches!(i2c, HostState::NotPermitted { .. }),
                "{}: {i2c:?}",
                profile.kernel_driver
            );
            assert_eq!(i2c.error().unwrap().kind, ErrorKind::PermissionDenied);
        }
    }
}

/// A node whose advisory lock another process holds is `Busy` for each
/// transport.
#[test]
fn held_advisory_lock_is_busy_for_each_transport() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        let device = tree.device(&device_name(profile));
        tree.bind(&device, driver_name(profile), profile.kernel_driver);
        let hidraw = tree.hidraw(&device, "hidraw0", 0o600);
        let i2c_node = tree.i2c(&device, 3, 0o600);

        let hid_lock = DeviceLock::acquire(hidraw.as_os_str()).expect("first advisory lock");
        let hid = tree.diagnosis().diagnose(&id, Transport::Hid);
        assert!(
            matches!(
                hid,
                HostState::Busy {
                    reason: BusyReason::AdvisoryLock,
                    ..
                }
            ),
            "{}: {hid:?}",
            profile.kernel_driver
        );
        assert_eq!(hid.error().unwrap().kind, ErrorKind::Busy);
        // Released first, so the i2c-dev answer can only come from its own lock.
        drop(hid_lock);

        let i2c_lock = DeviceLock::acquire(i2c_node.as_os_str()).expect("first advisory lock");
        let i2c = tree.diagnosis().diagnose(&id, Transport::I2cDev);
        assert!(
            matches!(
                i2c,
                HostState::Busy {
                    reason: BusyReason::AdvisoryLock,
                    ..
                }
            ),
            "{}: {i2c:?}",
            profile.kernel_driver
        );

        drop(i2c_lock);
    }
}

/// A kernel client driver bound at the module address would make `I2C_SLAVE`
/// fail `EBUSY`, so the i2c-dev path is `Busy(KernelClient)`.
#[test]
fn kernel_client_at_the_module_address_is_busy_for_i2c() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        let device = tree.device(&device_name(profile));
        tree.bind(&device, driver_name(profile), profile.kernel_driver);
        tree.i2c(&device, 3, 0o600);
        tree.kernel_client(&device, 3, 0x50, driver_name(profile));

        let i2c = tree.diagnosis().diagnose(&id, Transport::I2cDev);
        match &i2c {
            HostState::Busy {
                reason: BusyReason::KernelClient { driver },
                ..
            } => assert_eq!(driver, driver_name(profile), "{}", profile.kernel_driver),
            other => panic!(
                "{}: expected Busy(KernelClient), got {other:?}",
                profile.kernel_driver
            ),
        }
        assert_eq!(i2c.error().unwrap().kind, ErrorKind::Busy);
    }
}

/// Two devices matching the identity: `Ambiguous` for both transports,
/// nothing picked silently.
#[test]
fn two_matching_devices_are_ambiguous() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        let first = format!(
            "0003:{:04X}:{:04X}.0001",
            profile.vendor_id, profile.product_id
        );
        let second = format!(
            "0003:{:04X}:{:04X}.0002",
            profile.vendor_id, profile.product_id
        );
        tree.device(&first);
        tree.device(&second);

        for transport in [Transport::Hid, Transport::I2cDev] {
            let state = tree.diagnosis().diagnose(&id, transport);
            match &state {
                HostState::Ambiguous { devices } => {
                    assert_eq!(devices.len(), 2, "{} {transport:?}", profile.kernel_driver)
                }
                other => panic!(
                    "{} {transport:?}: expected Ambiguous, got {other:?}",
                    profile.kernel_driver
                ),
            }
            assert_eq!(state.error().unwrap().kind, ErrorKind::Invalid);
        }
    }
}

/// The HID bus directory missing means sysfs is not what this diagnosis reads:
/// `Inconclusive`, never usable.
#[test]
fn missing_hid_bus_directory_is_inconclusive() {
    for profile in support::PROFILES {
        let tree = Tree::new();
        let id = identity(profile);
        fs::remove_dir_all(tree.root.join("sys/bus/hid")).unwrap();
        for transport in [Transport::Hid, Transport::I2cDev] {
            let state = tree.diagnosis().diagnose(&id, transport);
            assert!(
                matches!(state, HostState::Inconclusive { .. }),
                "{} {transport:?}: {state:?}",
                profile.kernel_driver
            );
            assert_eq!(
                state.error().unwrap().kind,
                ErrorKind::Io,
                "{} {transport:?}",
                profile.kernel_driver
            );
        }
    }
}
