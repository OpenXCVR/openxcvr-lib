//! Report IDs are explicit on writes. Input bytes/count follow HIDAPI semantics:
//! numbered reports include their ID; unnumbered reports contain payload only.
use crate::Result;
use std::time::Instant;

/// HID report-level I/O behind a module-facing bus: writes address a
/// report by explicit ID; reads follow HIDAPI input semantics (see the
/// module docs).
pub trait Reports {
    /// Returns payload bytes written (the separately supplied ID is excluded).
    fn write_report(&mut self, id: u8, payload: &[u8], deadline: Instant) -> Result<usize>;
    /// Read one input report into `out`; returns the payload bytes read (the
    /// report-ID byte is excluded from the count, per HIDAPI semantics).
    fn read_report(&mut self, out: &mut [u8], deadline: Instant) -> Result<usize>;
}

/// [`Reports`] forwards through a boxed backend, so a caller can hold a test
/// double or a live backend behind one type.
impl<T: Reports + ?Sized> Reports for Box<T> {
    fn write_report(&mut self, id: u8, payload: &[u8], deadline: Instant) -> Result<usize> {
        (**self).write_report(id, payload, deadline)
    }
    fn read_report(&mut self, out: &mut [u8], deadline: Instant) -> Result<usize> {
        (**self).read_report(out, deadline)
    }
}

#[cfg(feature = "hid")]
mod native {
    use super::*;
    use crate::exclusive::DeviceLock;
    use crate::{remaining, Error, ErrorKind};
    use std::ffi::{CStr, CString, OsString};

    #[derive(Debug, Clone)]
    pub struct Device {
        pub path: CString,
        pub vendor_id: u16,
        pub product_id: u16,
        pub usage_page: u16,
        pub usage: u16,
        pub interface: i32,
        pub serial: Option<String>,
    }

    /// Owned bus classification, independent of hidapi's public types.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HeldBusType {
        Unknown,
        Usb,
        Bluetooth,
        I2c,
        Spi,
    }

    /// Platform facts observable about this held handle, not opener defaults.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HeldPlatformFacts {
        Macos {
            location_id: u32,
            open_exclusive: bool,
        },
        /// This platform cannot observe the macOS location/seize facts.
        /// This is not a claim that the handle is shared or exclusive.
        NotObservable,
    }

    /// Owned metadata returned by [`NativeHid::held_info`]. Field names shared
    /// with [`Device`] have the same meaning; values retain hidapi's optional
    /// serial and unknown bus/interface conventions.
    #[derive(Debug, Clone)]
    pub struct HeldInfo {
        pub path: CString,
        pub vendor_id: u16,
        pub product_id: u16,
        pub release_number: u16,
        pub usage_page: u16,
        pub usage: u16,
        pub interface: i32,
        pub serial: Option<String>,
        pub bus_type: HeldBusType,
        pub platform: HeldPlatformFacts,
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub struct Filter {
        pub vendor_id: Option<u16>,
        pub product_id: Option<u16>,
        pub usage_page: Option<u16>,
        pub usage: Option<u16>,
    }

    pub fn enumerate(filter: Filter) -> Result<Vec<Device>> {
        let api = hidapi::HidApi::new().map_err(convert)?;
        Ok(api
            .device_list()
            .filter(|d| {
                filter.vendor_id.is_none_or(|v| v == d.vendor_id())
                    && filter.product_id.is_none_or(|v| v == d.product_id())
                    && filter.usage_page.is_none_or(|v| v == d.usage_page())
                    && filter.usage.is_none_or(|v| v == d.usage())
            })
            .map(|d| Device {
                path: d.path().to_owned(),
                vendor_id: d.vendor_id(),
                product_id: d.product_id(),
                usage_page: d.usage_page(),
                usage: d.usage(),
                interface: d.interface_number(),
                serial: d.serial_number().map(str::to_owned),
            })
            .collect())
    }

    /// A HID handle holding the device's advisory [`DeviceLock`] for its
    /// lifetime: a second cooperating open reports [`ErrorKind::Busy`], never
    /// [`ErrorKind::Io`], on every OS.
    pub struct NativeHid {
        device: hidapi::HidDevice,
        _lock: DeviceLock,
    }

    impl NativeHid {
        pub fn open_path(path: &CStr) -> Result<Self> {
            let _lock = DeviceLock::acquire(&lock_key(path))?;
            #[cfg(target_os = "linux")]
            classify_node(path)?;
            let api = hidapi::HidApi::new().map_err(convert)?;
            let device = api.open_path(path).map_err(convert)?;
            Ok(Self { device, _lock })
        }

        /// Inspect this held handle without sending or receiving any input,
        /// output or feature report. Returns owned values only; no handle
        /// escapes, no enumeration or second open occurs, and the advisory
        /// lock remains held. A failed hidapi call fails the entire inspection
        /// through the transport's typed error mapping, never a default value.
        ///
        /// Metadata has hidapi's semantics (including cached/unknown values),
        /// not an independent identity or device-liveness proof. macOS location
        /// and exclusive-open facts come from this handle; all other platforms
        /// return [`HeldPlatformFacts::NotObservable`], never a fabricated bool.
        ///
        /// Reportless source audit, pinned hidapi **2.6.7**:
        /// - macOS `etc/hidapi/mac/hid.c:1467-1476` gets metadata from the held
        ///   IOHIDDevice (`539-660`: IOHID/registry properties; property helpers
        ///   `302-419`). `1488-1499` reads its location property, and `1511-1517`
        ///   reads stored open options. None performs report I/O.
        /// - Linux hidraw `etc/hidapi/linux/hid.c:1317-1325` gets cached metadata
        ///   or uses `836-874`: fstat of the held fd then udev/sysfs properties
        ///   (`631-834`, descriptor file read at `414-455`, not a report read).
        /// - Windows native `src/windows_native/mod.rs:328-330` clones metadata
        ///   cached for this handle. `device_info.rs:61-123` populated it from
        ///   HID attributes, preparsed data, strings and registry properties
        ///   (`hid.rs:18-46`: HidD_GetAttributes/HidD_GetPreparsedData), not
        ///   input, output or feature reports.
        pub fn held_info(&self) -> Result<HeldInfo> {
            held_info_via(&self.device)
        }
    }

    // hidapi::DeviceInfo has no public constructor, so the test seam uses the
    // existing owned Device plus the additional get_device_info fields.
    struct DeviceFacts {
        device: Device,
        release_number: u16,
        bus_type: HeldBusType,
    }

    /// Property reads only. Deliberately exposes no report or handle operation.
    trait InspectIo {
        fn device_info(&self) -> hidapi::HidResult<DeviceFacts>;
        #[cfg(target_os = "macos")]
        fn location_id(&self) -> hidapi::HidResult<u32>;
        #[cfg(target_os = "macos")]
        fn open_exclusive(&self) -> hidapi::HidResult<bool>;
    }

    impl InspectIo for hidapi::HidDevice {
        fn device_info(&self) -> hidapi::HidResult<DeviceFacts> {
            let info = self.get_device_info()?;
            Ok(DeviceFacts {
                device: Device {
                    path: info.path().to_owned(),
                    vendor_id: info.vendor_id(),
                    product_id: info.product_id(),
                    usage_page: info.usage_page(),
                    usage: info.usage(),
                    interface: info.interface_number(),
                    serial: info.serial_number().map(str::to_owned),
                },
                release_number: info.release_number(),
                bus_type: held_bus_type(info.bus_type()),
            })
        }

        #[cfg(target_os = "macos")]
        fn location_id(&self) -> hidapi::HidResult<u32> {
            self.get_location_id()
        }

        #[cfg(target_os = "macos")]
        fn open_exclusive(&self) -> hidapi::HidResult<bool> {
            self.is_open_exclusive()
        }
    }

    fn held_bus_type(bus: hidapi::BusType) -> HeldBusType {
        match bus {
            hidapi::BusType::Unknown => HeldBusType::Unknown,
            hidapi::BusType::Usb => HeldBusType::Usb,
            hidapi::BusType::Bluetooth => HeldBusType::Bluetooth,
            hidapi::BusType::I2c => HeldBusType::I2c,
            hidapi::BusType::Spi => HeldBusType::Spi,
        }
    }

    fn held_info_via(device: &impl InspectIo) -> Result<HeldInfo> {
        let facts = device.device_info().map_err(convert)?;
        #[cfg(target_os = "macos")]
        let platform = HeldPlatformFacts::Macos {
            location_id: device.location_id().map_err(convert)?,
            open_exclusive: device.open_exclusive().map_err(convert)?,
        };
        #[cfg(not(target_os = "macos"))]
        let platform = HeldPlatformFacts::NotObservable;
        Ok(HeldInfo {
            path: facts.device.path,
            vendor_id: facts.device.vendor_id,
            product_id: facts.device.product_id,
            release_number: facts.release_number,
            usage_page: facts.device.usage_page,
            usage: facts.device.usage,
            interface: facts.device.interface,
            serial: facts.device.serial,
            bus_type: facts.bus_type,
            platform,
        })
    }

    impl Reports for NativeHid {
        fn write_report(&mut self, id: u8, payload: &[u8], deadline: Instant) -> Result<usize> {
            write_report_via(&self.device, id, payload, deadline)
        }
        fn read_report(&mut self, out: &mut [u8], deadline: Instant) -> Result<usize> {
            read_report_via(&self.device, out, deadline)
        }
    }

    /// The device operations the transport needs: implemented by hidapi's
    /// device, and by a fake in tests so the error and uncertainty paths below
    /// run without hardware.
    trait ReportIo {
        fn write(&self, report: &[u8]) -> std::result::Result<usize, hidapi::HidError>;
        fn read_timeout(
            &self,
            out: &mut [u8],
            timeout_ms: i32,
        ) -> std::result::Result<usize, hidapi::HidError>;
    }

    impl ReportIo for hidapi::HidDevice {
        fn write(&self, report: &[u8]) -> std::result::Result<usize, hidapi::HidError> {
            hidapi::HidDevice::write(self, report)
        }
        fn read_timeout(
            &self,
            out: &mut [u8],
            timeout_ms: i32,
        ) -> std::result::Result<usize, hidapi::HidError> {
            hidapi::HidDevice::read_timeout(self, out, timeout_ms)
        }
    }

    fn write_report_via(
        device: &impl ReportIo,
        id: u8,
        payload: &[u8],
        deadline: Instant,
    ) -> Result<usize> {
        remaining(deadline)?;
        let mut report = Vec::with_capacity(payload.len() + 1);
        report.push(id);
        report.extend_from_slice(payload);
        // HIDAPI's write has no portable cancellation primitive. We retain
        // ownership until it returns; a deadline is not hard real-time.
        // From here the report has been submitted: every failure is
        // marked uncertain, including the deadline re-check below.
        let count = device.write(&report).map_err(|e| convert(e).uncertain())?;
        remaining(deadline).map_err(Error::uncertain)?;
        if count != report.len() {
            return Err(
                Error::new(ErrorKind::ShortTransfer, "incomplete HID report write").uncertain(),
            );
        }
        Ok(payload.len())
    }

    fn read_report_via(device: &impl ReportIo, out: &mut [u8], deadline: Instant) -> Result<usize> {
        let timeout = remaining(deadline)?.as_millis().clamp(1, i32::MAX as u128) as i32;
        let count = device.read_timeout(out, timeout).map_err(convert)?;
        if count == 0 {
            return Err(Error::new(ErrorKind::Timeout, "HID input deadline expired"));
        }
        remaining(deadline)?;
        Ok(count)
    }

    /// Advisory-lock key for a device path: byte-exact on Unix. On Windows
    /// hidapi reports ASCII device paths, so the lossy form names the same key.
    fn lock_key(path: &CStr) -> OsString {
        #[cfg(unix)]
        let key: OsString = {
            use std::os::unix::ffi::OsStrExt;
            std::ffi::OsStr::from_bytes(path.to_bytes()).to_owned()
        };
        #[cfg(not(unix))]
        let key: OsString = OsString::from(path.to_string_lossy().into_owned());
        key
    }

    /// Linux: classify the hidraw node before hidapi sees it. The pinned
    /// `linux-static-hidraw` backend reports failures as message text, or as
    /// no text at all (`HidApiErrorEmpty`), so an absent or unpermitted node
    /// is named here from the node itself: absent is
    /// [`ErrorKind::Disconnected`], no read-write permission for the effective
    /// IDs is [`ErrorKind::PermissionDenied`]. A held advisory lock was already
    /// reported as [`ErrorKind::Busy`]. A kernel driver bound without exposing
    /// a node has no path to open; callers name that case with
    /// [`crate::host_state::Diagnosis`] before choosing a path.
    #[cfg(target_os = "linux")]
    fn classify_node(path: &CStr) -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let node = std::path::Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()));
        if let Err(e) = std::fs::metadata(node) {
            let kind = match e.kind() {
                std::io::ErrorKind::NotFound => ErrorKind::Disconnected,
                _ => kind_from_io(&e),
            };
            return Err(Error::new(kind, format!("{}: {e}", node.display())));
        }
        // SAFETY: `path` is NUL-terminated and outlives the call.
        let rc = unsafe {
            libc::faccessat(
                libc::AT_FDCWD,
                path.as_ptr(),
                libc::R_OK | libc::W_OK,
                libc::AT_EACCESS,
            )
        };
        if rc != 0 {
            let e = std::io::Error::last_os_error();
            if matches!(e.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM)) {
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    format!("{}: no read-write permission for this user", node.display()),
                ));
            }
        }
        Ok(())
    }

    fn convert(error: hidapi::HidError) -> Error {
        let kind = match &error {
            hidapi::HidError::IoError { error } => kind_from_io(error),
            hidapi::HidError::HidApiError { message } => kind_from_message(message),
            _ => ErrorKind::Io,
        };
        Error::new(kind, error.to_string())
    }

    /// Message-text classification for the pinned hidapi 2.6.7 backends
    /// (`linux-static-hidraw`, macOS IOKit), where the `HidApiError` text is
    /// the only signal the backend gives. Unmatched text stays [`ErrorKind::Io`].
    fn kind_from_message(message: &str) -> ErrorKind {
        // etc/hidapi/mac/hid.c:1055-1058 registers
        // "hid_open_path: failed to open IOHIDDevice from mach entry: (0x%08X) %s"
        // with the IOReturn code. Under hidapi's seize default,
        // kIOReturnExclusiveAccess (0xE00002C5) means another process holds
        // the device.
        const IOKIT_EXCLUSIVE_ACCESS: &str =
            "failed to open IOHIDDevice from mach entry: (0xE00002C5)";
        if message.contains(IOKIT_EXCLUSIVE_ACCESS) {
            return ErrorKind::Busy;
        }
        // Linux hidraw failures carry strerror text (etc/hidapi/linux/hid.c).
        if message.contains("Permission denied") {
            return ErrorKind::PermissionDenied;
        }
        if message.contains("No such file")
            || message.contains("No such device")
            || message.contains("device disconnected")
            || message.contains("Device is disconnected")
            || message.contains("device mach entry not found with the given path")
        {
            return ErrorKind::Disconnected;
        }
        if message.contains("Device or resource busy") {
            return ErrorKind::Busy;
        }
        ErrorKind::Io
    }

    /// `std::io::Error` classification for `HidError::IoError`, including raw
    /// OS error 32 (`ERROR_SHARING_VIOLATION`), the windows-native open's
    /// sharing failure. The numeric check is unconditional so the table is
    /// pinned on every host; POSIX raw 32 (`EPIPE`) reaching here is the only
    /// known collision and HID paths do not raise it.
    fn kind_from_io(error: &std::io::Error) -> ErrorKind {
        const ERROR_SHARING_VIOLATION: i32 = 32;
        if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) {
            return ErrorKind::Busy;
        }
        match error.kind() {
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            std::io::ErrorKind::NotFound | std::io::ErrorKind::BrokenPipe => {
                ErrorKind::Disconnected
            }
            std::io::ErrorKind::TimedOut => ErrorKind::Timeout,
            _ => ErrorKind::Io,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::{Cell, RefCell};
        use std::time::Duration;

        /// A device that answers from a script, recording the reports it got.
        #[derive(Default)]
        struct FakeDevice {
            write_result: RefCell<Option<std::result::Result<usize, hidapi::HidError>>>,
            read_result: RefCell<Option<std::result::Result<usize, hidapi::HidError>>>,
            write_delay: Cell<Duration>,
            written: RefCell<Vec<Vec<u8>>>,
        }

        impl ReportIo for FakeDevice {
            fn write(&self, report: &[u8]) -> std::result::Result<usize, hidapi::HidError> {
                self.written.borrow_mut().push(report.to_vec());
                std::thread::sleep(self.write_delay.get());
                self.write_result
                    .borrow_mut()
                    .take()
                    .unwrap_or(Ok(report.len()))
            }
            fn read_timeout(
                &self,
                out: &mut [u8],
                _timeout_ms: i32,
            ) -> std::result::Result<usize, hidapi::HidError> {
                match self.read_result.borrow_mut().take() {
                    Some(result) => result,
                    None => {
                        out.fill(0x5a);
                        Ok(out.len())
                    }
                }
            }
        }

        /// Scripted metadata plus report methods that must never be reached.
        struct FakeInspection {
            info: RefCell<Option<hidapi::HidResult<DeviceFacts>>>,
            #[cfg(target_os = "macos")]
            location: RefCell<Option<hidapi::HidResult<u32>>>,
            #[cfg(target_os = "macos")]
            exclusive: RefCell<Option<hidapi::HidResult<bool>>>,
            calls: RefCell<Vec<&'static str>>,
            reports: Cell<usize>,
        }

        impl FakeInspection {
            fn new(serial: Option<&str>) -> Self {
                Self {
                    info: RefCell::new(Some(Ok(DeviceFacts {
                        device: Device {
                            path: CString::new("held-path").unwrap(),
                            vendor_id: 0x04d8,
                            product_id: 0x00dd,
                            usage_page: 0xff00,
                            usage: 1,
                            interface: 2,
                            serial: serial.map(str::to_owned),
                        },
                        release_number: 0x0101,
                        bus_type: HeldBusType::Usb,
                    }))),
                    #[cfg(target_os = "macos")]
                    location: RefCell::new(Some(Ok(0x12340000))),
                    #[cfg(target_os = "macos")]
                    exclusive: RefCell::new(Some(Ok(true))),
                    calls: RefCell::new(Vec::new()),
                    reports: Cell::new(0),
                }
            }

            fn assert_no_reports(&self) {
                assert_eq!(self.reports.get(), 0);
            }
        }

        impl InspectIo for FakeInspection {
            fn device_info(&self) -> hidapi::HidResult<DeviceFacts> {
                self.calls.borrow_mut().push("info");
                self.info.borrow_mut().take().unwrap()
            }

            #[cfg(target_os = "macos")]
            fn location_id(&self) -> hidapi::HidResult<u32> {
                self.calls.borrow_mut().push("location");
                self.location.borrow_mut().take().unwrap()
            }

            #[cfg(target_os = "macos")]
            fn open_exclusive(&self) -> hidapi::HidResult<bool> {
                self.calls.borrow_mut().push("exclusive");
                self.exclusive.borrow_mut().take().unwrap()
            }
        }

        impl ReportIo for FakeInspection {
            fn write(&self, _report: &[u8]) -> hidapi::HidResult<usize> {
                self.reports.set(self.reports.get() + 1);
                panic!("inspection sent an output report");
            }

            fn read_timeout(&self, _out: &mut [u8], _timeout_ms: i32) -> hidapi::HidResult<usize> {
                self.reports.set(self.reports.get() + 1);
                panic!("inspection read an input report");
            }
        }

        #[test]
        fn held_info_returns_owned_metadata_and_no_reports() {
            for serial in [Some("serial"), None] {
                let device = FakeInspection::new(serial);
                let info = held_info_via(&device).unwrap();
                device.assert_no_reports();
                drop(device);
                assert_eq!(info.path.to_bytes(), b"held-path");
                assert_eq!(info.vendor_id, 0x04d8);
                assert_eq!(info.product_id, 0x00dd);
                assert_eq!(info.release_number, 0x0101);
                assert_eq!(info.usage_page, 0xff00);
                assert_eq!(info.usage, 1);
                assert_eq!(info.interface, 2);
                assert_eq!(info.serial.as_deref(), serial);
                assert_eq!(info.bus_type, HeldBusType::Usb);
                #[cfg(target_os = "macos")]
                assert_eq!(
                    info.platform,
                    HeldPlatformFacts::Macos {
                        location_id: 0x12340000,
                        open_exclusive: true,
                    }
                );
                #[cfg(not(target_os = "macos"))]
                assert_eq!(info.platform, HeldPlatformFacts::NotObservable);
            }
        }

        #[test]
        fn held_info_device_info_failure_is_typed_and_stops_inspection() {
            let device = FakeInspection::new(None);
            *device.info.borrow_mut() = Some(Err(api_error("No such device")));
            let error = held_info_via(&device).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Disconnected);
            assert!(!error.write_may_have_applied);
            assert_eq!(device.calls.borrow().as_slice(), &["info"]);
            device.assert_no_reports();
        }

        #[cfg(not(target_os = "macos"))]
        #[test]
        fn held_info_other_platform_does_not_invent_exclusivity() {
            let device = FakeInspection::new(None);
            assert_eq!(
                held_info_via(&device).unwrap().platform,
                HeldPlatformFacts::NotObservable
            );
            assert_eq!(device.calls.borrow().as_slice(), &["info"]);
            device.assert_no_reports();
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn held_info_macos_reads_both_facts_and_preserves_false() {
            for exclusive in [true, false] {
                let device = FakeInspection::new(None);
                *device.exclusive.borrow_mut() = Some(Ok(exclusive));
                assert_eq!(
                    held_info_via(&device).unwrap().platform,
                    HeldPlatformFacts::Macos {
                        location_id: 0x12340000,
                        open_exclusive: exclusive,
                    }
                );
                assert_eq!(
                    device.calls.borrow().as_slice(),
                    &["info", "location", "exclusive"]
                );
                device.assert_no_reports();
            }
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn held_info_macos_each_failed_read_fails_the_whole_call() {
            for location_fails in [true, false] {
                let device = FakeInspection::new(None);
                if location_fails {
                    *device.location.borrow_mut() = Some(Err(api_error("Permission denied")));
                } else {
                    *device.exclusive.borrow_mut() = Some(Err(api_error("Permission denied")));
                }
                let error = held_info_via(&device).unwrap_err();
                assert_eq!(error.kind, ErrorKind::PermissionDenied);
                assert!(!error.write_may_have_applied);
                let expected = if location_fails {
                    vec!["info", "location"]
                } else {
                    vec!["info", "location", "exclusive"]
                };
                assert_eq!(*device.calls.borrow(), expected);
                device.assert_no_reports();
            }
        }

        #[test]
        fn held_bus_type_preserves_every_pinned_hidapi_variant() {
            for (raw, owned) in [
                (hidapi::BusType::Unknown, HeldBusType::Unknown),
                (hidapi::BusType::Usb, HeldBusType::Usb),
                (hidapi::BusType::Bluetooth, HeldBusType::Bluetooth),
                (hidapi::BusType::I2c, HeldBusType::I2c),
                (hidapi::BusType::Spi, HeldBusType::Spi),
            ] {
                assert_eq!(held_bus_type(raw), owned);
            }
        }

        fn soon() -> Instant {
            Instant::now() + Duration::from_secs(1)
        }

        fn api_error(message: &str) -> hidapi::HidError {
            hidapi::HidError::HidApiError {
                message: message.to_owned(),
            }
        }

        #[test]
        fn a_failed_submitted_write_is_uncertain_and_classified() {
            for (error, kind) in [
                (api_error("Permission denied"), ErrorKind::PermissionDenied),
                (api_error("No such device"), ErrorKind::Disconnected),
                (api_error("Device or resource busy"), ErrorKind::Busy),
                (api_error("unexpected"), ErrorKind::Io),
                (
                    hidapi::HidError::IoError {
                        error: std::io::Error::from(std::io::ErrorKind::TimedOut),
                    },
                    ErrorKind::Timeout,
                ),
            ] {
                let device = FakeDevice::default();
                *device.write_result.borrow_mut() = Some(Err(error));
                let e = write_report_via(&device, 0, &[0x10, 0x20], soon()).unwrap_err();
                assert_eq!(e.kind, kind, "{e}");
                assert!(e.write_may_have_applied, "the report was submitted: {e}");
                assert_eq!(device.written.borrow().as_slice(), &[vec![0, 0x10, 0x20]]);
            }
        }

        #[test]
        fn a_write_that_omits_the_report_id_is_a_short_uncertain_transfer() {
            let device = FakeDevice::default();
            *device.write_result.borrow_mut() = Some(Ok(0));
            let e = write_report_via(&device, 0, &[0x10], soon()).unwrap_err();
            assert_eq!(e.kind, ErrorKind::ShortTransfer);
            assert!(e.write_may_have_applied);
        }

        #[test]
        fn positive_short_write_is_an_uncertain_short_transfer() {
            let device = FakeDevice::default();
            *device.write_result.borrow_mut() = Some(Ok(2));
            let e = write_report_via(&device, 3, &[1, 2, 3], soon()).unwrap_err();
            assert_eq!(e.kind, ErrorKind::ShortTransfer);
            assert!(e.write_may_have_applied);
        }

        #[test]
        fn a_deadline_passed_during_the_write_is_uncertain() {
            let device = FakeDevice::default();
            device.write_delay.set(Duration::from_millis(30));
            let deadline = Instant::now() + Duration::from_millis(10);
            let e = write_report_via(&device, 0, &[0x10], deadline).unwrap_err();
            assert_eq!(e.kind, ErrorKind::Timeout);
            assert!(
                e.write_may_have_applied,
                "the write returned after the deadline"
            );
        }

        #[test]
        fn an_expired_deadline_sends_nothing() {
            let device = FakeDevice::default();
            let past = Instant::now() - Duration::from_millis(1);
            let e = write_report_via(&device, 0, &[0x10], past).unwrap_err();
            assert_eq!(e.kind, ErrorKind::Timeout);
            assert!(!e.write_may_have_applied);
            assert!(device.written.borrow().is_empty());
        }

        #[test]
        fn a_successful_write_reports_payload_bytes() {
            let device = FakeDevice::default();
            assert_eq!(write_report_via(&device, 3, &[1, 2, 3], soon()).unwrap(), 3);
            assert_eq!(device.written.borrow().as_slice(), &[vec![3, 1, 2, 3]]);
        }

        #[test]
        fn read_errors_are_classified_and_never_uncertain() {
            let device = FakeDevice::default();
            *device.read_result.borrow_mut() = Some(Err(api_error("No such device")));
            let mut out = [0u8; 4];
            let e = read_report_via(&device, &mut out, soon()).unwrap_err();
            assert_eq!(e.kind, ErrorKind::Disconnected);
            assert!(!e.write_may_have_applied);

            *device.read_result.borrow_mut() = Some(Ok(0));
            let e = read_report_via(&device, &mut out, soon()).unwrap_err();
            assert_eq!(e.kind, ErrorKind::Timeout);

            assert_eq!(read_report_via(&device, &mut out, soon()).unwrap(), 4);
            assert_eq!(out, [0x5a; 4]);
        }

        /// Pins the exact seize-message format at hidapi 2.6.7
        /// (etc/hidapi/mac/hid.c:1055-1058):
        /// `"hid_open_path: failed to open IOHIDDevice from mach entry: (0x%08X) %s"`
        /// with `kIOReturnExclusiveAccess` = `0xE00002C5`.
        #[test]
        fn macos_seize_message_format_maps_to_busy() {
            let k_ioreturn_exclusive_access: u32 = 0xE00002C5;
            let message = format!(
                "hid_open_path: failed to open IOHIDDevice from mach entry: (0x{:08X}) {}",
                k_ioreturn_exclusive_access, "(iokit/common) exclusive access"
            );
            assert_eq!(
                message,
                "hid_open_path: failed to open IOHIDDevice from mach entry: (0xE00002C5) (iokit/common) exclusive access"
            );
            assert_eq!(kind_from_message(&message), ErrorKind::Busy);
        }

        #[test]
        fn linux_open_messages_map_to_their_kinds() {
            assert_eq!(
                kind_from_message("open failed (/dev/hidraw0): Permission denied"),
                ErrorKind::PermissionDenied
            );
            assert_eq!(
                kind_from_message("open failed (/dev/hidraw0): No such file or directory"),
                ErrorKind::Disconnected
            );
            assert_eq!(
                kind_from_message("open failed (/dev/hidraw0): No such device"),
                ErrorKind::Disconnected
            );
            assert_eq!(
                kind_from_message("open failed (/dev/hidraw0): Device or resource busy"),
                ErrorKind::Busy
            );
        }

        #[test]
        fn pinned_backend_disconnect_messages_are_disconnected() {
            // hidapi 2.6.7 etc/hidapi/linux/hid.c and mac/hid.c.
            for message in [
                "hid_read_timeout: unexpected poll error (device disconnected)",
                "Device is disconnected",
                "hid_read_timeout: device disconnected",
                "hid_open_path: device mach entry not found with the given path",
            ] {
                assert_eq!(
                    kind_from_message(message),
                    ErrorKind::Disconnected,
                    "{message}"
                );
            }
        }

        #[test]
        fn unclassified_message_stays_io() {
            assert_eq!(
                kind_from_message("hidapi error: something else entirely"),
                ErrorKind::Io
            );
        }

        #[test]
        fn io_errors_map_by_kind_and_sharing_violation() {
            let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
            assert_eq!(kind_from_io(&denied), ErrorKind::PermissionDenied);
            let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
            assert_eq!(kind_from_io(&missing), ErrorKind::Disconnected);
            let timed_out = std::io::Error::from(std::io::ErrorKind::TimedOut);
            assert_eq!(kind_from_io(&timed_out), ErrorKind::Timeout);
            let sharing = std::io::Error::from_raw_os_error(32); // ERROR_SHARING_VIOLATION
            assert_eq!(kind_from_io(&sharing), ErrorKind::Busy);
            let other = std::io::Error::from(std::io::ErrorKind::Other);
            assert_eq!(kind_from_io(&other), ErrorKind::Io);
        }

        /// A held advisory lock makes a second cooperating open Busy before
        /// hidapi is reached; no hardware involved.
        #[cfg(unix)]
        #[test]
        fn second_open_of_a_held_path_reports_busy() {
            let path = CStr::from_bytes_with_nul(b"/dev/openxcvr-hid-lock-test\0").unwrap();
            let held = DeviceLock::acquire(&lock_key(path)).expect("first lock");
            let error = match NativeHid::open_path(path) {
                Err(error) => error,
                Ok(_) => panic!("second open must fail"),
            };
            assert_eq!(error.kind, ErrorKind::Busy);
            drop(held);
        }

        #[cfg(target_os = "linux")]
        fn temp_node(name: &str, mode: u32) -> CString {
            use std::os::unix::ffi::OsStringExt;
            use std::os::unix::fs::PermissionsExt;
            let path =
                std::env::temp_dir().join(format!("openxcvr-hid-{name}-{}", std::process::id()));
            std::fs::write(&path, b"").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            CString::new(path.into_os_string().into_vec()).unwrap()
        }

        /// An absent node is Disconnected through the whole open path.
        #[cfg(target_os = "linux")]
        #[test]
        fn open_of_an_absent_node_is_disconnected() {
            let path = CStr::from_bytes_with_nul(b"/dev/openxcvr-hid-absent-test\0").unwrap();
            match NativeHid::open_path(path) {
                Err(error) => assert_eq!(error.kind, ErrorKind::Disconnected, "{error}"),
                Ok(_) => panic!("open of an absent node must fail"),
            }
        }

        /// An unpermitted node is PermissionDenied through the whole open path.
        #[cfg(target_os = "linux")]
        #[test]
        fn open_of_an_unpermitted_node_is_permission_denied() {
            use std::os::unix::ffi::OsStrExt;
            // SAFETY: geteuid has no preconditions.
            if unsafe { libc::geteuid() } == 0 {
                eprintln!("skipped: root passes every permission check");
                return;
            }
            let path = temp_node("denied", 0o000);
            let result = NativeHid::open_path(&path);
            let _ = std::fs::remove_file(std::ffi::OsStr::from_bytes(path.to_bytes()));
            match result {
                Err(error) => assert_eq!(error.kind, ErrorKind::PermissionDenied, "{error}"),
                Ok(_) => panic!("open of an unpermitted node must fail"),
            }
        }

        /// The pre-check itself, independent of hidapi's text: hidapi here
        /// also names these cases from strerror text, so only a direct call
        /// proves the classification holds when the backend gives no text.
        #[cfg(target_os = "linux")]
        #[test]
        fn node_classification_does_not_depend_on_hidapi_text() {
            use std::os::unix::ffi::OsStrExt;
            let absent = CStr::from_bytes_with_nul(b"/dev/openxcvr-hid-absent-test\0").unwrap();
            assert_eq!(
                classify_node(absent).unwrap_err().kind,
                ErrorKind::Disconnected
            );
            let open = temp_node("open", 0o600);
            let ok = classify_node(&open);
            // SAFETY: geteuid has no preconditions.
            let root = unsafe { libc::geteuid() } == 0;
            let denied = temp_node("denied-direct", 0o000);
            let refused = classify_node(&denied);
            for p in [&open, &denied] {
                let _ = std::fs::remove_file(std::ffi::OsStr::from_bytes(p.to_bytes()));
            }
            assert!(ok.is_ok(), "{ok:?}");
            if !root {
                assert_eq!(refused.unwrap_err().kind, ErrorKind::PermissionDenied);
            }
        }
    }
}

#[cfg(feature = "hid")]
pub use native::{enumerate, Device, Filter, HeldBusType, HeldInfo, HeldPlatformFacts, NativeHid};
