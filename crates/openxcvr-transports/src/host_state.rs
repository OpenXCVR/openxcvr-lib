//! Read-only Linux host-state diagnosis for the userspace transports.
//!
//! Answers, for one requested [`Transport`], whether this host can reach a
//! device with the caller's identity and, if not, exactly what is missing. The
//! answer is per transport: a kernel driver bound to the HID interface without
//! a hidraw node makes the HID path unavailable, but is the normal keep-bound
//! configuration for the i2c-dev path, and a hidraw node this user may not open
//! never blocks a usable i2c-dev adapter. Nothing here falls back from one
//! transport to the other.
//!
//! The diagnosis never changes host state: it reads sysfs, checks node
//! permission with `faccessat(2)` against the effective IDs, and probes the advisory lock the transports
//! themselves take (acquired and released at once; see [`crate::exclusive`]).
//! Identity comes from the caller, so nothing here is adapter-specific. An
//! unreadable sysfs entry makes the answer [`HostState::Inconclusive`], never
//! usable. On macOS and Windows the answer is [`HostState::NotApplicable`].
//!
//! sysfs layout read (all under `sys_root`, default `/sys`):
//! - `bus/hid/devices/BBBB:VVVV:PPPP.NNNN` — one HID device per entry;
//! - `<device>/driver` — link to the bound driver, whose `module` link names
//!   the kernel module;
//! - `<device>/<hidraw_class>/<node>` — the hidraw node, e.g. `hidraw/hidraw0`;
//! - `<device>/i2c-N/<i2c_class>/<node>` — the i2c adapter the driver
//!   registered and its i2c-dev node, e.g. `i2c-3/i2c-dev/i2c-3`;
//! - `<device>/i2c-N/N-00AA/driver` — a kernel client bound at address `AA`.
//!
//! Nodes are looked up by name under `dev_root` (default `/dev`).

#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;

use crate::{Error, ErrorKind};

/// The transport the caller intends to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// HID reports through a hidraw node (hidapi's `linux-static-hidraw`).
    Hid,
    /// The keep-bound path: the kernel driver's i2c adapter through i2c-dev.
    I2cDev,
}

/// What the caller says the adapter is. Nothing in this module assumes a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub vendor_id: u16,
    pub product_id: u16,
    /// Kernel driver or module that may claim the HID interface, matched
    /// against both the sysfs driver name and its module name (the MCP2221A's
    /// driver is `mcp2221`, its module `hid_mcp2221`).
    pub kernel_driver: String,
    /// sysfs class directory holding the hidraw node (normally `hidraw`).
    pub hidraw_class: String,
    /// sysfs class directory holding the i2c-dev node (normally `i2c-dev`).
    pub i2c_class: String,
    /// 7-bit address of the module memory on the adapter's bus; a larger
    /// value makes every answer inconclusive rather than silently unmatched.
    pub module_address: u8,
}

/// The kernel driver bound to the HID interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundDriver {
    /// sysfs driver name.
    pub name: String,
    /// Kernel module providing it, when sysfs names one.
    pub module: Option<String>,
    /// Whether `name` or `module` equals [`Identity::kernel_driver`].
    pub expected: bool,
}

/// Why a node is owned by someone else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusyReason {
    /// Another cooperating process holds the transport's advisory lock.
    AdvisoryLock,
    /// A kernel client driver is bound at the module address, so `I2C_SLAVE`
    /// would be refused with `EBUSY`.
    KernelClient { driver: String },
}

/// One answer for one requested transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostState {
    /// Not Linux: this diagnosis does not apply and does not guess.
    NotApplicable,
    /// No HID device with the caller's vendor and product ID enumerates.
    Absent,
    /// More than one device matches the identity; nothing is picked silently.
    Ambiguous { devices: Vec<String> },
    /// HID requested, but the device has no hidraw node. With `driver` set this
    /// is the driver-bound-without-node case (the keep-bound configuration),
    /// never `Busy`. Not reported for [`Transport::I2cDev`].
    NoHidrawNode {
        device: String,
        driver: Option<BoundDriver>,
    },
    /// i2c-dev requested, but no i2c adapter is registered under the device
    /// (`adapter: None`: no driver bound, or the driver registered none), or it
    /// has no i2c-dev node (`adapter: Some`: the i2c-dev module is not loaded).
    NoI2cAdapter {
        device: String,
        driver: Option<BoundDriver>,
        adapter: Option<String>,
    },
    /// The node exists but is owned by someone else.
    Busy { node: PathBuf, reason: BusyReason },
    /// The node exists but this user may not open it for reading and writing.
    NotPermitted { node: PathBuf },
    /// The requested transport can be opened through `node`.
    Usable { transport: Transport, node: PathBuf },
    /// Host state could not be read; never treated as usable.
    Inconclusive { path: PathBuf, detail: String },
}

impl HostState {
    pub fn is_usable(&self) -> bool {
        matches!(self, HostState::Usable { .. })
    }

    /// The shared error an open reports for this state, or `None` when usable.
    ///
    /// Absent is `Disconnected`, busy is `Busy`, not permitted is
    /// `PermissionDenied`; a missing node or adapter is `Unsupported` (the
    /// requested path does not exist in this host configuration, which is not a
    /// transient failure); an ambiguous identity is `Invalid`; an unreadable
    /// host state is `Io`.
    pub fn error(&self) -> Option<Error> {
        let (kind, detail) = match self {
            HostState::Usable { .. } => return None,
            HostState::NotApplicable => (
                ErrorKind::Unsupported,
                "host-state diagnosis applies to Linux only".to_owned(),
            ),
            HostState::Absent => (
                ErrorKind::Disconnected,
                "no HID device with the requested vendor and product ID".to_owned(),
            ),
            HostState::Ambiguous { devices } => (
                ErrorKind::Invalid,
                format!(
                    "identity matches {} devices: {}",
                    devices.len(),
                    devices.join(", ")
                ),
            ),
            HostState::NoHidrawNode { device, driver } => (
                ErrorKind::Unsupported,
                match driver {
                    Some(d) => format!(
                        "{device}: driver {} is bound and exposes no hidraw node; \
                         the HID path needs the driver detached",
                        d.name
                    ),
                    None => format!("{device}: no driver bound and no hidraw node"),
                },
            ),
            HostState::NoI2cAdapter {
                device,
                driver,
                adapter,
            } => (
                ErrorKind::Unsupported,
                match (adapter, driver) {
                    (Some(a), _) => {
                        format!("{device}: adapter {a} has no i2c-dev node; load i2c-dev")
                    }
                    (None, Some(d)) => {
                        format!("{device}: driver {} registered no i2c adapter", d.name)
                    }
                    (None, None) => format!(
                        "{device}: no driver bound, so no i2c adapter; the i2c-dev path \
                         needs the kernel driver bound"
                    ),
                },
            ),
            HostState::Busy { node, reason } => (
                ErrorKind::Busy,
                match reason {
                    BusyReason::AdvisoryLock => {
                        format!("{}: held by another process", node.display())
                    }
                    BusyReason::KernelClient { driver } => format!(
                        "{}: module address claimed by kernel driver {driver}",
                        node.display()
                    ),
                },
            ),
            HostState::NotPermitted { node } => (
                ErrorKind::PermissionDenied,
                format!("{}: no read-write permission for this user", node.display()),
            ),
            HostState::Inconclusive { path, detail } => (
                ErrorKind::Io,
                format!("{}: host state unreadable: {detail}", path.display()),
            ),
        };
        Some(Error::new(kind, detail))
    }
}

/// Where the diagnosis reads host state. Tests point both roots at a fake tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    pub sys_root: PathBuf,
    pub dev_root: PathBuf,
}

impl Default for Diagnosis {
    fn default() -> Self {
        Self {
            sys_root: PathBuf::from("/sys"),
            dev_root: PathBuf::from("/dev"),
        }
    }
}

impl Diagnosis {
    pub fn new(sys_root: impl Into<PathBuf>, dev_root: impl Into<PathBuf>) -> Self {
        Self {
            sys_root: sys_root.into(),
            dev_root: dev_root.into(),
        }
    }

    /// Diagnoses the host for one requested transport. Reads only.
    pub fn diagnose(&self, identity: &Identity, transport: Transport) -> HostState {
        #[cfg(target_os = "linux")]
        {
            linux::diagnose(self, identity, transport)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (identity, transport);
            HostState::NotApplicable
        }
    }
}

#[cfg(target_os = "linux")]
/// Parses a HID device directory name `BBBB:VVVV:PPPP.NNNN`.
fn hid_ids(name: &str) -> Option<(u16, u16)> {
    let mut parts = name.split(':');
    let _bus = parts.next()?;
    let vendor = u16::from_str_radix(parts.next()?, 16).ok()?;
    let (product, _instance) = parts.next()?.split_once('.')?;
    if parts.next().is_some() {
        return None;
    }
    Some((vendor, u16::from_str_radix(product, 16).ok()?))
}

#[cfg(target_os = "linux")]
fn inconclusive(path: &Path, error: std::io::Error) -> HostState {
    HostState::Inconclusive {
        path: path.to_path_buf(),
        detail: error.to_string(),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::exclusive::DeviceLock;
    use std::ffi::CString;
    use std::fs;
    use std::io;
    use std::os::unix::ffi::OsStrExt;

    type Step<T> = std::result::Result<T, HostState>;

    pub(super) fn diagnose(d: &Diagnosis, id: &Identity, transport: Transport) -> HostState {
        match run(d, id, transport) {
            Ok(state) | Err(state) => state,
        }
    }

    fn run(d: &Diagnosis, id: &Identity, transport: Transport) -> Step<HostState> {
        if id.module_address > 0x7f {
            return Ok(HostState::Inconclusive {
                path: d.sys_root.clone(),
                detail: format!(
                    "module address {:#04x} is not a 7-bit address",
                    id.module_address
                ),
            });
        }
        let bus = d.sys_root.join("bus/hid/devices");
        let mut matches = Vec::new();
        for name in list(&bus, true)? {
            if hid_ids(&name) == Some((id.vendor_id, id.product_id)) {
                matches.push(name);
            }
        }
        let device = match matches.len() {
            0 => return Ok(HostState::Absent),
            1 => matches.pop().expect("one match"),
            _ => return Ok(HostState::Ambiguous { devices: matches }),
        };
        let dir = bus.join(&device);
        let driver = bound_driver(&dir, id)?;

        let (node_name, adapter_dir) = match transport {
            Transport::Hid => match one(&dir.join(&id.hidraw_class), &device)? {
                Some(node) => (node, None),
                None => return Ok(HostState::NoHidrawNode { device, driver }),
            },
            Transport::I2cDev => {
                let adapters: Vec<String> = list(&dir, false)?
                    .into_iter()
                    .filter(|n| is_i2c_adapter(n))
                    .collect();
                let adapter = match adapters.len() {
                    0 => {
                        return Ok(HostState::NoI2cAdapter {
                            device,
                            driver,
                            adapter: None,
                        })
                    }
                    1 => adapters.into_iter().next().expect("one adapter"),
                    _ => return Ok(HostState::Ambiguous { devices: adapters }),
                };
                let adapter_dir = dir.join(&adapter);
                match one(&adapter_dir.join(&id.i2c_class), &device)? {
                    Some(node) => (node, Some(adapter_dir)),
                    None => {
                        return Ok(HostState::NoI2cAdapter {
                            device,
                            driver,
                            adapter: Some(adapter),
                        })
                    }
                }
            }
        };

        let node = d.dev_root.join(&node_name);
        // Follows links, so a dangling /dev link is missing, not present.
        if let Err(e) = fs::metadata(&node) {
            return Ok(HostState::Inconclusive {
                path: node,
                detail: format!("sysfs lists {node_name} but the node is missing: {e}"),
            });
        }
        if let Some(adapter_dir) = adapter_dir {
            if let Some(driver) = kernel_client(&adapter_dir, id.module_address)? {
                return Ok(HostState::Busy {
                    node,
                    reason: BusyReason::KernelClient { driver },
                });
            }
        }
        match DeviceLock::acquire(node.as_os_str()) {
            Ok(lock) => drop(lock),
            Err(e) if e.kind == ErrorKind::Busy => {
                return Ok(HostState::Busy {
                    node,
                    reason: BusyReason::AdvisoryLock,
                })
            }
            Err(e) => {
                return Ok(HostState::Inconclusive {
                    path: node,
                    detail: format!("advisory lock probe: {}", e.detail),
                })
            }
        }
        Ok(match access_rw(&node) {
            Ok(()) => HostState::Usable { transport, node },
            Err(e) if is_permission(&e) => HostState::NotPermitted { node },
            Err(e) => inconclusive(&node, e),
        })
    }

    /// Entry names of `dir`. A missing directory is empty, except where
    /// `required`: the HID bus directory itself missing means sysfs is not
    /// what this diagnosis reads, so the answer is inconclusive.
    fn list(dir: &Path, required: bool) -> Step<Vec<String>> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound && !required => return Ok(Vec::new()),
            Err(e) => return Err(inconclusive(dir, e)),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| inconclusive(dir, e))?;
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    /// The single node under a class directory; more than one is ambiguous.
    fn one(class_dir: &Path, device: &str) -> Step<Option<String>> {
        let mut nodes = list(class_dir, false)?;
        match nodes.len() {
            0 => Ok(None),
            1 => Ok(nodes.pop()),
            _ => Err(HostState::Ambiguous {
                devices: nodes.into_iter().map(|n| format!("{device}/{n}")).collect(),
            }),
        }
    }

    fn is_i2c_adapter(name: &str) -> bool {
        name.strip_prefix("i2c-")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }

    fn link_name(path: &Path) -> Step<Option<String>> {
        match fs::read_link(path) {
            Ok(target) => Ok(target
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(inconclusive(path, e)),
        }
    }

    fn bound_driver(dir: &Path, id: &Identity) -> Step<Option<BoundDriver>> {
        let name = match link_name(&dir.join("driver"))? {
            Some(name) => name,
            None => return Ok(None),
        };
        let module = link_name(&dir.join("driver/module"))?;
        let expected = name == id.kernel_driver || module.as_deref() == Some(&*id.kernel_driver);
        Ok(Some(BoundDriver {
            name,
            module,
            expected,
        }))
    }

    /// The kernel driver bound at `address` on the adapter, if any.
    fn kernel_client(adapter_dir: &Path, address: u8) -> Step<Option<String>> {
        let bus = adapter_dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("i2c-"))
            .unwrap_or_default();
        link_name(&adapter_dir.join(format!("{bus}-{address:04x}/driver")))
    }

    fn access_rw(node: &Path) -> io::Result<()> {
        let path = CString::new(node.as_os_str().as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        // SAFETY: `path` is a valid NUL-terminated string for the call's duration.
        // AT_EACCESS: check the effective IDs, which open(2) uses, not the
        // real ones plain access(2) checks.
        let rc = unsafe {
            libc::faccessat(
                libc::AT_FDCWD,
                path.as_ptr(),
                libc::R_OK | libc::W_OK,
                libc::AT_EACCESS,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn is_permission(e: &io::Error) -> bool {
        matches!(e.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// The two adapter identities every test runs against: the MCP2221A and a
    /// synthetic second adapter with other IDs and another driver name.
    fn profiles() -> [(Identity, &'static str, &'static str); 2] {
        let id = |vendor_id, product_id, kernel_driver: &str| Identity {
            vendor_id,
            product_id,
            kernel_driver: kernel_driver.to_owned(),
            hidraw_class: "hidraw".to_owned(),
            i2c_class: "i2c-dev".to_owned(),
            module_address: 0x50,
        };
        [
            // (identity, sysfs driver name, sysfs device name)
            (
                id(0x04d8, 0x00dd, "hid_mcp2221"),
                "mcp2221",
                "0003:04D8:00DD.0001",
            ),
            (
                id(0xdead, 0xbeef, "hid_synthetic_bridge"),
                "synthetic_bridge",
                "0003:DEAD:BEEF.0007",
            ),
        ]
    }

    /// A fake `/sys` and `/dev` in a fresh temporary directory.
    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "openxcvr-host-state-{}-{}",
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
        fn hidraw(&self, device: &Path, node: &str, mode: u32) -> PathBuf {
            fs::create_dir_all(device.join("hidraw").join(node)).unwrap();
            self.node(node, mode)
        }
        fn i2c(&self, device: &Path, bus: u32, mode: u32) -> PathBuf {
            let name = format!("i2c-{bus}");
            fs::create_dir_all(device.join(&name).join("i2c-dev").join(&name)).unwrap();
            self.node(&name, mode)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn root() -> bool {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }

    #[test]
    fn device_name_parses_bus_vendor_product_instance() {
        assert_eq!(hid_ids("0003:04D8:00DD.0001"), Some((0x04d8, 0x00dd)));
        assert_eq!(hid_ids("0003:dead:beef.00ff"), Some((0xdead, 0xbeef)));
        assert_eq!(hid_ids("0003:04D8:00DD"), None);
        assert_eq!(hid_ids("0003:04D8:00DD.0001:x"), None);
        assert_eq!(hid_ids("module"), None);
    }

    #[test]
    fn no_matching_device_is_absent_for_both_transports() {
        for (id, _, _) in profiles() {
            let tree = Tree::new();
            tree.device("0003:1234:5678.0001");
            for t in [Transport::Hid, Transport::I2cDev] {
                let state = tree.diagnosis().diagnose(&id, t);
                assert_eq!(state, HostState::Absent);
                assert_eq!(state.error().unwrap().kind, ErrorKind::Disconnected);
            }
        }
    }

    #[test]
    fn driver_bound_without_hidraw_blocks_hid_but_is_normal_for_i2c_dev() {
        for (id, driver, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            tree.bind(&dev, driver, &id.kernel_driver);
            let i2c = tree.i2c(&dev, 3, 0o600);

            let hid = tree.diagnosis().diagnose(&id, Transport::Hid);
            match &hid {
                HostState::NoHidrawNode {
                    driver: Some(d), ..
                } => {
                    assert_eq!(d.name, driver);
                    assert_eq!(d.module.as_deref(), Some(&*id.kernel_driver));
                    assert!(d.expected, "caller's module name must match");
                }
                other => panic!("{other:?}"),
            }
            assert_eq!(hid.error().unwrap().kind, ErrorKind::Unsupported);
            assert_eq!(
                tree.diagnosis().diagnose(&id, Transport::I2cDev),
                HostState::Usable {
                    transport: Transport::I2cDev,
                    node: i2c
                }
            );
        }
    }

    #[test]
    fn unbound_device_has_neither_node_nor_adapter() {
        for (id, _, name) in profiles() {
            let tree = Tree::new();
            tree.device(name);
            assert!(matches!(
                tree.diagnosis().diagnose(&id, Transport::Hid),
                HostState::NoHidrawNode { driver: None, .. }
            ));
            assert!(matches!(
                tree.diagnosis().diagnose(&id, Transport::I2cDev),
                HostState::NoI2cAdapter {
                    driver: None,
                    adapter: None,
                    ..
                }
            ));
        }
    }

    #[test]
    fn adapter_without_i2c_dev_node_names_the_adapter() {
        for (id, driver, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            tree.bind(&dev, driver, &id.kernel_driver);
            fs::create_dir_all(dev.join("i2c-4")).unwrap();
            let state = tree.diagnosis().diagnose(&id, Transport::I2cDev);
            assert!(
                matches!(&state, HostState::NoI2cAdapter { adapter: Some(a), .. } if a == "i2c-4"),
                "{state:?}"
            );
            assert!(state.error().unwrap().detail.contains("i2c-dev"));
        }
    }

    #[test]
    fn hidraw_usable_when_no_driver_holds_the_interface() {
        for (id, _, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            let node = tree.hidraw(&dev, "hidraw2", 0o600);
            let state = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert_eq!(
                state,
                HostState::Usable {
                    transport: Transport::Hid,
                    node
                }
            );
            assert!(state.error().is_none());
        }
    }

    #[test]
    fn unpermitted_hidraw_does_not_block_a_usable_i2c_dev_path() {
        if root() {
            eprintln!("skipped: access(2) grants root every node");
            return;
        }
        for (id, driver, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            tree.bind(&dev, driver, &id.kernel_driver);
            let hidraw = tree.hidraw(&dev, "hidraw0", 0o000);
            let i2c = tree.i2c(&dev, 3, 0o600);
            let hid = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert_eq!(hid, HostState::NotPermitted { node: hidraw });
            assert_eq!(hid.error().unwrap().kind, ErrorKind::PermissionDenied);
            assert_eq!(
                tree.diagnosis().diagnose(&id, Transport::I2cDev),
                HostState::Usable {
                    transport: Transport::I2cDev,
                    node: i2c
                }
            );
        }
    }

    #[test]
    fn unpermitted_i2c_node_is_reported_for_i2c_dev_only() {
        if root() {
            eprintln!("skipped: access(2) grants root every node");
            return;
        }
        for (id, driver, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            tree.bind(&dev, driver, &id.kernel_driver);
            let hidraw = tree.hidraw(&dev, "hidraw0", 0o600);
            let i2c = tree.i2c(&dev, 3, 0o000);
            assert_eq!(
                tree.diagnosis().diagnose(&id, Transport::I2cDev),
                HostState::NotPermitted { node: i2c }
            );
            assert_eq!(
                tree.diagnosis().diagnose(&id, Transport::Hid),
                HostState::Usable {
                    transport: Transport::Hid,
                    node: hidraw
                }
            );
        }
    }

    #[test]
    fn held_advisory_lock_is_busy_before_permission() {
        for (id, driver, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            tree.bind(&dev, driver, &id.kernel_driver);
            let hidraw = tree.hidraw(&dev, "hidraw0", 0o000);
            let i2c = tree.i2c(&dev, 3, 0o000);
            for (t, node) in [(Transport::Hid, hidraw), (Transport::I2cDev, i2c)] {
                let _held = crate::exclusive::DeviceLock::acquire(node.as_os_str()).unwrap();
                let state = tree.diagnosis().diagnose(&id, t);
                assert_eq!(
                    state,
                    HostState::Busy {
                        node,
                        reason: BusyReason::AdvisoryLock
                    }
                );
                assert_eq!(state.error().unwrap().kind, ErrorKind::Busy);
            }
        }
    }

    #[test]
    fn released_probe_leaves_the_lock_free() {
        let _guard = crate::test_guard();
        for (id, _, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            let node = tree.hidraw(&dev, "hidraw0", 0o600);
            assert!(tree.diagnosis().diagnose(&id, Transport::Hid).is_usable());
            crate::exclusive::DeviceLock::acquire(node.as_os_str())
                .expect("the diagnosis must not keep the lock");
        }
    }

    #[test]
    fn kernel_client_at_the_module_address_is_busy_for_i2c_dev() {
        for (id, driver, name) in profiles() {
            let tree = Tree::new();
            let dev = tree.device(name);
            tree.bind(&dev, driver, &id.kernel_driver);
            let i2c = tree.i2c(&dev, 3, 0o600);
            let client = dev.join("i2c-3/3-0050");
            fs::create_dir_all(&client).unwrap();
            let optoe = tree.root.join("sys/bus/i2c/drivers/optoe");
            fs::create_dir_all(&optoe).unwrap();
            symlink(&optoe, client.join("driver")).unwrap();
            assert_eq!(
                tree.diagnosis().diagnose(&id, Transport::I2cDev),
                HostState::Busy {
                    node: i2c,
                    reason: BusyReason::KernelClient {
                        driver: "optoe".to_owned()
                    }
                }
            );
        }
    }

    #[test]
    fn two_matching_devices_are_ambiguous() {
        for (id, _, name) in profiles() {
            let tree = Tree::new();
            tree.device(name);
            let other = name.replace(".000", ".100");
            tree.device(&other);
            let state = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert!(
                matches!(&state, HostState::Ambiguous { devices } if devices.len() == 2),
                "{state:?}"
            );
            assert_eq!(state.error().unwrap().kind, ErrorKind::Invalid);
        }
    }

    #[test]
    fn unreadable_host_state_is_inconclusive_never_usable() {
        for (id, _, name) in profiles() {
            // The HID bus directory missing entirely.
            let tree = Tree::new();
            fs::remove_dir_all(tree.root.join("sys/bus/hid")).unwrap();
            let state = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert!(matches!(state, HostState::Inconclusive { .. }), "{state:?}");
            assert_eq!(state.error().unwrap().kind, ErrorKind::Io);

            // sysfs lists a node that /dev lacks.
            let tree = Tree::new();
            let dev = tree.device(name);
            fs::create_dir_all(dev.join("hidraw/hidraw9")).unwrap();
            let state = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert!(matches!(state, HostState::Inconclusive { .. }), "{state:?}");

            // A dangling /dev link is missing, not present.
            let tree = Tree::new();
            let dev = tree.device(name);
            fs::create_dir_all(dev.join("hidraw/hidraw5")).unwrap();
            symlink(tree.root.join("nowhere"), tree.root.join("dev/hidraw5")).unwrap();
            let state = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert!(matches!(state, HostState::Inconclusive { .. }), "{state:?}");

            // An address that is not 7-bit is refused, not silently unmatched.
            let tree = Tree::new();
            let mut wide = id.clone();
            wide.module_address = 0xa0;
            let state = tree.diagnosis().diagnose(&wide, Transport::I2cDev);
            assert!(matches!(state, HostState::Inconclusive { .. }), "{state:?}");

            // The class directory is a file, not a directory.
            let tree = Tree::new();
            let dev = tree.device(name);
            fs::write(dev.join("hidraw"), b"").unwrap();
            let state = tree.diagnosis().diagnose(&id, Transport::Hid);
            assert!(matches!(state, HostState::Inconclusive { .. }), "{state:?}");
        }
    }

    #[test]
    fn a_foreign_driver_is_reported_but_not_expected() {
        let (id, _, name) = profiles()[0].clone();
        let tree = Tree::new();
        let dev = tree.device(name);
        tree.bind(&dev, "hid-generic", "hid_generic");
        match tree.diagnosis().diagnose(&id, Transport::Hid) {
            HostState::NoHidrawNode {
                driver: Some(d), ..
            } => {
                assert_eq!(d.name, "hid-generic");
                assert!(!d.expected);
            }
            other => panic!("{other:?}"),
        }
    }
}
