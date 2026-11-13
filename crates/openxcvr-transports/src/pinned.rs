//! Internal Linux pinned-selection prototype (bounded — not integrated, and
//! not a proof of the reviewed contract).
//!
//! The connector locking design (`docs/connector-interfaces/locking-design.md`)
//! was approved by
//! `docs/connector-interfaces/evidence/locking-design-review-2026-09-28.md`
//! "for implementation of the Linux design only", on the condition that the
//! selected instance be witnessed by an independently retained,
//! **non-reusable** value. This module does not provide that witness (see
//! below). It is deliberately **internal** (`pub(crate)`, `dead_code`-allowed):
//! the published open paths ([`crate::hid::NativeHid::open_path`],
//! [`crate::i2cdev::I2cDevBus::open`]) keep their existing, explicitly weaker
//! contract, and no integration of this module is authorized by anything here.
//! Nothing in this module takes an advisory lock (the OFD guard is out of
//! scope), supports Windows, or provisions anything; it never names an
//! adapter identity.
//!
//! # Bounded evidence, not an enumeration-to-instance witness
//!
//! The retained real character-device fd is a continuity anchor **after** the
//! first open: a later retarget of the selected pathname cannot change that
//! fd. `fstat(fd).st_rdev` locates the current sysfs object, whose `dev`
//! attribute, shape and (for hidraw) fd-direct ioctls are consistency checks.
//! The canonical sysfs pathname, HID bus instance suffix, adapter ancestry,
//! `st_rdev`, VID/PID and serial are all reusable. They do **not** establish
//! that the device opened is the same instance previously enumerated. A
//! replacement with the same name, number and static fields before the first
//! open passes these checks (see the synthetic ABA regression test).
//!
//! Missing or inconsistent observable metadata refuses; agreement is *not*
//! proof of an independent, non-reusable enumeration-to-instance identity.
//! This module cannot implement the reviewed `open_selected` contract, and
//! must not be integrated into public opens or used to justify a transfer to
//! an earlier enumerated instance without a separately proven mechanism.
//!
//! # Scope and lifetime sources
//!
//! Ordinary fixtures use `/dev/zero` and `/dev/null` as distinguishable
//! character fds with a controlled sysfs copy; they do not exercise a HID or
//! I²C adapter. Explicit ignored tests require real privileged fixtures and
//! are not part of ordinary verification. No hardware result is claimed.
//! Linux v6.12 `drivers/hid/hidraw.c` (`hidraw_open`, `drop_ref`,
//! `hidraw_release`, `hidraw_connect`) retains the open hidraw object and
//! reserves its minor until the last old open closes after disconnect.
//! Linux v6.12 `drivers/i2c/i2c-dev.c` (`i2cdev_open`/`release`) stores an
//! adapter reference in the open file's private client and releases it on
//! close. These source-backed lifetimes support *post-open fd continuity*,
//! not attribution of an earlier enumeration to that first open; neither a
//! sysfs path comparison nor a matching `dev` attribute closes that gap.
//! Vendor kernels and real-device execution are unverified here.

// The component is an internal prototype, not referenced by any public open
// path; any future integration would need a separately proven instance-witness
// mechanism this module does not provide. The dead_code lint is silenced for
// the module; the reason is recorded here.
// (`reason` in `allow` needs Rust 1.81; this crate's MSRV is 1.82.)
#![allow(dead_code)]

use std::fs;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::{Error, ErrorKind, Result};

/// `sys/sysmacros.h` (glibc): `major(dev) = ((dev >> 8) & 0xfff) | ((dev >> 32) & ~0xfff)`.
const fn dev_major(dev: u64) -> u32 {
    (((dev >> 8) & 0xfff) as u32) | (((dev >> 32) as u32) & 0xffff_f000)
}

/// `sys/sysmacros.h` (glibc): `minor(dev) = (dev & 0xff) | ((dev >> 12) & ~0xff)`,
/// with the cast to `unsigned int` glibc applies before the mask.
const fn dev_minor(dev: u64) -> u32 {
    ((dev & 0xff) as u32) | (((dev >> 12) as u32) & 0xffff_ff00)
}

/// The sysfs object pair string (`"major:minor"`) for a `dev_t`.
fn dev_pair(rdev: u64) -> String {
    format!("{}:{}", dev_major(rdev), dev_minor(rdev))
}

/// `linux/ioctl.h`: `_IOC(dir, type, nr, size)`, `_IOC_READ = 2`.
const fn ioc_read(size: usize, ty: u8, nr: u8) -> libc::c_ulong {
    (2u64 << 30) | ((size as u64) << 16) | ((ty as u64) << 8) | (nr as u64)
}

/// `HIDIOCGRDESCSIZE` — `_IOR('H', 0x01, int)` (`linux/hidraw.h`).
const HIDIOCGRDESCSIZE: libc::c_ulong = ioc_read(4, b'H', 0x01);
/// `HIDIOCGRAWINFO` — `_IOR('H', 0x03, struct hidraw_devinfo)` (`linux/hidraw.h`).
const HIDIOCGRAWINFO: libc::c_ulong = ioc_read(8, b'H', 0x03);

/// A character-device descriptor opened **before** any selection metadata is
/// collected, and retained for the selection's whole lifetime. The fd is the
/// continuity anchor: it is never reopened from a path, and every observation
/// below is keyed off it.
pub(crate) struct RetainedCharFd {
    fd: OwnedFd,
    /// `fstat` of the opened descriptor, collected once at open.
    rdev: u64,
    /// The path this descriptor was opened through. Diagnostic only: never
    /// consulted for identity, never reopened.
    opened_from: PathBuf,
}

impl RetainedCharFd {
    /// Opens `path` (following a caller-supplied symlink to the actual
    /// character device, as the design's boundary requires) read/write with
    /// `O_CLOEXEC`, fstats the descriptor and refuses anything that is not a
    /// character device. The descriptor is retained from this call onward.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            // std sets CLOEXEC itself; the flag is named because the design
            // requires it, not because std would omit it.
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .map_err(|e| open_error(path, e))?;
        let meta = file
            .metadata()
            .map_err(|e| Error::new(ErrorKind::Io, format!("{}: fstat: {e}", path.display())))?;
        if !meta.file_type().is_char_device() {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!("{}: not a character device", path.display()),
            ));
        }
        Ok(Self {
            fd: file.into(),
            rdev: meta.rdev(),
            opened_from: path.to_path_buf(),
        })
    }

    /// The retained descriptor.
    pub(crate) fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// The opened descriptor's `st_rdev` (`major:minor` form).
    pub(crate) fn dev(&self) -> String {
        dev_pair(self.rdev)
    }

    /// Yields the retained descriptor to the selection object.
    fn into_fd(self) -> OwnedFd {
        self.fd
    }

    /// Collects a fd-keyed sysfs observation: the canonical sysfs object the
    /// opened descriptor's `st_rdev` names, verified against the descriptor
    /// through the object's `dev` attribute. The selection path plays no part;
    /// a device that disappeared between open and this call has no object and
    /// is [`ErrorKind::Disconnected`] when its number is not reused; a
    /// same-number replacement cannot be distinguished by this lookup.
    pub(crate) fn sysfs_object_path(&self, sys_root: &Path) -> Result<String> {
        derive_sysfs_object_path(self.rdev, sys_root, &self.opened_from)
    }
}

/// Derives a reusable sysfs observation from a descriptor's `st_rdev`: the canonical
/// sysfs object under `sys_root`, relative to the (canonicalized) sys root.
/// `context` names the selection for diagnostics only.
fn derive_sysfs_object_path(rdev: u64, sys_root: &Path, context: &Path) -> Result<String> {
    let pair = dev_pair(rdev);
    let link_path = sys_root.join("dev/char").join(&pair);
    let _target = fs::read_link(&link_path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::Disconnected,
                format!(
                    "{}: the selected device {} has no sysfs object (instance disappeared?)",
                    context.display(),
                    pair
                ),
            )
        } else {
            Error::new(
                ErrorKind::Io,
                format!("{}: sysfs lookup {}: {e}", context.display(), pair),
            )
        }
    })?;
    let object = fs::canonicalize(&link_path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::Disconnected,
                format!(
                    "{}: the sysfs object of {} is unreachable (instance disappeared?)",
                    context.display(),
                    pair
                ),
            )
        } else {
            Error::new(
                ErrorKind::Io,
                format!(
                    "{}: resolve sysfs object of {}: {e}",
                    context.display(),
                    pair
                ),
            )
        }
    })?;
    let sys_real = fs::canonicalize(sys_root).map_err(|e| {
        Error::new(
            ErrorKind::Io,
            format!("sysfs root {}: {e}", sys_root.display()),
        )
    })?;
    let rel = object.strip_prefix(&sys_real).map_err(|_| {
        Error::new(
            ErrorKind::Invalid,
            format!(
                "{}: sysfs object {} escapes the sysfs root {}",
                context.display(),
                object.display(),
                sys_root.display()
            ),
        )
    })?;
    let rel = rel.to_str().ok_or_else(|| {
        Error::new(
            ErrorKind::Invalid,
            format!("{}: non-UTF-8 sysfs object path", context.display()),
        )
    })?;
    // The object must agree with the descriptor it was reached through.
    let attr = fs::read_to_string(object.join("dev")).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::Invalid,
                format!(
                    "{}: sysfs object {} has no dev attribute",
                    context.display(),
                    rel
                ),
            )
        } else {
            Error::new(
                ErrorKind::Io,
                format!("{}: read dev attribute of {}: {e}", context.display(), rel),
            )
        }
    })?;
    let attr = attr.trim();
    if attr != pair {
        return Err(Error::new(
            ErrorKind::Invalid,
            format!(
                "{}: sysfs object {} reports dev {attr} but the opened descriptor is {pair}",
                context.display(),
                rel
            ),
        ));
    }
    Ok(rel.to_owned())
}

/// `struct hidraw_devinfo` (`linux/hidraw.h`): `{ __u32 bustype; __s16 vendor; __s16 product; }`.
#[repr(C)]
struct HidrawDevInfo {
    bustype: u32,
    vendor: i16,
    product: i16,
}

/// A hidraw descriptor-identity error: `ENOTTY` means the descriptor is not a
/// hidraw node (wrong class, fail closed as `Unsupported`); `ENODEV` means the
/// instance behind it disappeared (`Disconnected`).
fn hidraw_ioctl_error(context: &Path, ioctl: &str, e: io::Error) -> Error {
    match e.raw_os_error() {
        Some(libc::ENOTTY) => Error::new(
            ErrorKind::Unsupported,
            format!(
                "{}: the opened descriptor does not answer {ioctl} (not a hidraw node?)",
                context.display()
            ),
        ),
        Some(libc::ENODEV) => Error::new(
            ErrorKind::Disconnected,
            format!("{context:?}: the hidraw instance behind the descriptor disappeared"),
        ),
        _ => Error::new(
            ErrorKind::Io,
            format!("{}: {ioctl}: {e}", context.display()),
        ),
    }
}

/// `HIDIOCGRDESCSIZE` on the retained descriptor.
fn hidraw_descriptor_size(fd: BorrowedFd<'_>, context: &Path) -> Result<u32> {
    let mut size: libc::c_int = 0;
    // SAFETY: `fd` is owned and valid; `size` is a valid out-pointer for the
    // int this `_IOR(..., int)` ioctl fills.
    if unsafe { libc::ioctl(fd.as_raw_fd(), HIDIOCGRDESCSIZE, &mut size) } != 0 {
        return Err(hidraw_ioctl_error(
            context,
            "HIDIOCGRDESCSIZE",
            io::Error::last_os_error(),
        ));
    }
    Ok(size.max(0) as u32)
}

/// `HIDIOCGRAWINFO` on the retained descriptor: `(bustype, vendor, product)`.
fn hidraw_raw_info(fd: BorrowedFd<'_>, context: &Path) -> Result<(u32, u16, u16)> {
    let mut info = HidrawDevInfo {
        bustype: 0,
        vendor: 0,
        product: 0,
    };
    // SAFETY: `fd` is owned and valid; `info` is a valid out-pointer matching
    // struct hidraw_devinfo's layout (u32, i16, i16, 8 bytes).
    if unsafe { libc::ioctl(fd.as_raw_fd(), HIDIOCGRAWINFO, &mut info) } != 0 {
        return Err(hidraw_ioctl_error(
            context,
            "HIDIOCGRAWINFO",
            io::Error::last_os_error(),
        ));
    }
    Ok((info.bustype, info.vendor as u16, info.product as u16))
}

/// Parses a HID bus instance directory name `BBBB:VVVV:PPPP.NNNN` into
/// `(vendor, product)`, with the same shape rules as `host_state::hid_ids`:
/// three colon-separated fields, the last split once on `.`, every part a hex
/// `u16`, nothing after the instance number.
fn hid_instance_name(name: &str) -> Option<(u16, u16)> {
    let mut parts = name.split(':');
    let _bus = parts.next()?;
    let vendor = u16::from_str_radix(parts.next()?, 16).ok()?;
    let product_field = parts.next()?;
    let (product, instance) = product_field.split_once('.')?;
    if instance.is_empty()
        || !instance.bytes().all(|b| b.is_ascii_hexdigit())
        || parts.next().is_some()
    {
        return None;
    }
    Some((vendor, u16::from_str_radix(product, 16).ok()?))
}

/// The hidraw shape of a sysfs object path: `.../<BBBB:VVVV:PPPP.NNNN>/hidraw/hidrawN`.
/// Returns the HID instance directory name and the `(vendor, product)` its
/// name declares.
fn hidraw_parts(rel: &str) -> Option<(String, u16, u16)> {
    let names: Vec<&str> = Path::new(rel)
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<_>>()?;
    if names.len() < 4 {
        return None;
    }
    let node = *names.last()?;
    let parent = names[names.len() - 2];
    let instance = names[names.len() - 3];
    let minor = node.strip_prefix("hidraw")?;
    if minor.is_empty() || !minor.bytes().all(|b| b.is_ascii_digit()) {
        return None; // hidrawx / bare "hidraw" are not hidraw nodes
    }
    if parent != "hidraw" {
        return None;
    }
    let (vendor, product) = hid_instance_name(instance)?;
    Some((instance.to_owned(), vendor, product))
}

/// An i2c-dev node name `i2c-N`.
fn is_i2c_name(name: &str) -> bool {
    name.strip_prefix("i2c-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The i2c-dev shape of a sysfs object path: `.../<adapter>/i2c-dev/i2c-N`
/// with the node named after its adapter. Returns the adapter's sysfs path
/// (reusable ancestry, not generation identity).
fn i2c_parts(rel: &str) -> Option<String> {
    let names: Vec<&str> = Path::new(rel)
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<_>>()?;
    if names.len() < 4 {
        return None;
    }
    let node = *names.last()?;
    let class = names[names.len() - 2];
    let adapter = names[names.len() - 3];
    if class != "i2c-dev" || !is_i2c_name(adapter) || node != adapter {
        return None;
    }
    Some(names[..names.len() - 2].join("/"))
}

/// What enumeration observed for the selected hidraw node: a reusable sysfs
/// object path and static identity. Agreement with the
/// opened descriptor is a consistency check, not proof of instance equality.
pub(crate) struct HidrawExpectation {
    /// The canonical sysfs object path relative to the sys root (reusable),
    /// e.g. `devices/virtual/misc/uhid/0003:1D50:777E.0006/hidraw/hidraw0`.
    pub sysfs_object: String,
    /// Vendor ID the enumeration recorded.
    pub vendor_id: u16,
    /// Product ID the enumeration recorded.
    pub product_id: u16,
}

/// What enumeration observed for the selected i2c adapter node: the adapter's
/// sysfs ancestry, which can also be reused across replug.
pub(crate) struct I2cDevExpectation {
    /// The adapter's canonical sysfs path relative to the sys root (reusable), e.g.
    /// `devices/pci0000:00/0000:00:1f.3/i2c-0`.
    pub adapter: String,
}

/// A non-clonable selection: it owns the opened descriptor and the fd-keyed
/// sysfs observation collected from it. Consuming it never reopens the selection path —
/// the path is not even stored.
pub(crate) struct PinnedSelection {
    fd: OwnedFd,
    /// The fd-keyed sysfs observation, re-derived at consume time; not a
    /// non-reusable instance identity.
    sysfs_object: String,
    /// Sys root for the consume-time re-derivation (a directory, never the
    /// device path).
    sys_root: PathBuf,
    /// The consistency recheck to run at consume, per selection class.
    class: Class,
}

enum Class {
    Hidraw { vendor_id: u16, product_id: u16 },
    I2cDev,
}

impl PinnedSelection {
    /// Opens the selected hidraw node, retains the real descriptor, then
    /// collects the selection metadata **from that descriptor**: the fd-keyed
    /// sysfs object (which must exist, be hidraw-shaped and carry a
    /// `dev` attribute that matches the descriptor), the descriptor's
    /// `HIDIOCGRDESCSIZE`/`HIDIOCGRAWINFO` answers, and agreement between the
    /// sysfs name, the descriptor and the caller's expectation. These checks
    /// do not establish equality to an instance enumerated before the open.
    /// Any missing or inconsistent observation refuses.
    pub(crate) fn select_hidraw(
        path: &Path,
        expected: &HidrawExpectation,
        sys_root: &Path,
    ) -> Result<Self> {
        let retained = RetainedCharFd::open(path)?;
        let rel = retained.sysfs_object_path(sys_root)?;
        let (instance, name_vendor, name_product) = hidraw_parts(&rel).ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                format!("{path:?}: the selected device's sysfs object {rel} is not a hidraw node"),
            )
        })?;
        if (name_vendor, name_product) != (expected.vendor_id, expected.product_id) {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "{path:?}: the sysfs instance {instance} declares {:04x}:{:04x}, \
                     but the selection expects {:04x}:{:04x}",
                    name_vendor, name_product, expected.vendor_id, expected.product_id
                ),
            ));
        }
        hidraw_descriptor_size(retained.as_fd(), path)?;
        let (_bustype, vendor, product) = hidraw_raw_info(retained.as_fd(), path)?;
        if (vendor, product) != (expected.vendor_id, expected.product_id) {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "{path:?}: the opened hidraw descriptor reports {:04x}:{:04x}, \
                     but the selection expects {:04x}:{:04x}",
                    vendor, product, expected.vendor_id, expected.product_id
                ),
            ));
        }
        if (vendor, product) != (name_vendor, name_product) {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "{path:?}: the sysfs instance {instance} disagrees with the opened \
                     descriptor ({}:{:04x}:{:04x})",
                    instance, vendor, product
                ),
            ));
        }
        if rel != expected.sysfs_object {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "{path:?}: the opened device's sysfs object is {rel}, but the \
                     expected {}",
                    expected.sysfs_object
                ),
            ));
        }
        Ok(Self {
            fd: retained.into_fd(),
            sysfs_object: rel,
            sys_root: sys_root.to_path_buf(),
            class: Class::Hidraw {
                vendor_id: expected.vendor_id,
                product_id: expected.product_id,
            },
        })
    }

    /// Opens the selected i2c adapter node, retains the real descriptor, then
    /// collects the selection metadata from that descriptor: the fd-keyed
    /// sysfs object (existing, i2c-dev-shaped, `dev` attribute matching)
    /// whose reusable adapter ancestry must equal the caller's expectation.
    /// An i2c-dev descriptor exposes identity only through `st_rdev` — its
    /// ioctls are transfer operations — so the class check is the sysfs
    /// ancestry, which is reusable and is not an independent generation witness.
    pub(crate) fn select_i2c_dev(
        path: &Path,
        expected: &I2cDevExpectation,
        sys_root: &Path,
    ) -> Result<Self> {
        let retained = RetainedCharFd::open(path)?;
        let rel = retained.sysfs_object_path(sys_root)?;
        let adapter = i2c_parts(&rel).ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                format!(
                    "{path:?}: the selected device's sysfs object {rel} is not an \
                     i2c-dev node"
                ),
            )
        })?;
        if adapter != expected.adapter {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "{path:?}: the opened adapter is {adapter}, but the selection \
                     expected {}",
                    expected.adapter
                ),
            ));
        }
        Ok(Self {
            fd: retained.into_fd(),
            sysfs_object: rel,
            sys_root: sys_root.to_path_buf(),
            class: Class::I2cDev,
        })
    }

    /// The sysfs object path observed at first open (diagnostics; a
    /// reusable pathname, not identity).
    pub(crate) fn sysfs_object(&self) -> &str {
        &self.sysfs_object
    }

    /// Consumes the selection: re-derives the sysfs observation **from the
    /// retained descriptor** — the selection path is never reopened, and is
    /// not stored at all — and refuses unless the fd-keyed lookup still
    /// names the same sysfs pathname. On success the caller owns the pinned
    /// descriptor; a retargeted pathname cannot redirect it. A
    /// same-name/same-number replacement is NOT detected by this
    /// comparison — neither before the first open (the caller's enumeration
    /// cannot be tied to the fd) nor after it. After the open the pinned fd,
    /// not this comparison, is what keeps a transfer from reaching a
    /// replacement instance.
    pub(crate) fn consume(self) -> Result<OwnedFd> {
        let rdev = fstat_rdev(self.fd.as_fd())?;
        let rel = derive_sysfs_object_path(rdev, &self.sys_root, Path::new("<pinned selection>"))?;
        if rel != self.sysfs_object {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "the pinned descriptor's sysfs object is now {rel}, but the selection \
                     pinned {}",
                    self.sysfs_object
                ),
            ));
        }
        match self.class {
            Class::Hidraw {
                vendor_id,
                product_id,
            } => {
                hidraw_descriptor_size(self.fd.as_fd(), Path::new("<pinned selection>"))?;
                let (_bustype, vendor, product) =
                    hidraw_raw_info(self.fd.as_fd(), Path::new("<pinned selection>"))?;
                if (vendor, product) != (vendor_id, product_id) {
                    return Err(Error::new(
                        ErrorKind::Invalid,
                        format!(
                            "the pinned hidraw descriptor now reports {:04x}:{:04x}, \
                             but the selection pinned {:04x}:{:04x}",
                            vendor, product, vendor_id, product_id
                        ),
                    ));
                }
            }
            Class::I2cDev => {}
        }
        Ok(self.fd)
    }
}

/// `fstat` on a live descriptor.
fn fstat_rdev(fd: BorrowedFd<'_>) -> Result<u64> {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is owned and valid for the call; `stat` is a valid
    // out-pointer of the right layout for this target.
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) } != 0 {
        return Err(Error::new(
            ErrorKind::Io,
            format!(
                "fstat of the pinned descriptor: {}",
                io::Error::last_os_error()
            ),
        ));
    }
    Ok(stat.st_rdev as u64)
}

/// The open-path error classification the design fixes: an absent node is
/// `Disconnected`, a node with no device behind it (`ENXIO`) is
/// `Disconnected`, an unpermitted node is `PermissionDenied`, anything else
/// stays `Io`.
fn open_error(path: &Path, e: io::Error) -> Error {
    let kind = match e.raw_os_error() {
        Some(libc::ENXIO) => ErrorKind::Disconnected,
        _ => match e.kind() {
            io::ErrorKind::NotFound => ErrorKind::Disconnected,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Io,
        },
    };
    Error::new(kind, format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ---- helpers ---------------------------------------------------------

    fn err_kind(result: Result<impl Sized>) -> ErrorKind {
        result.err().expect("the operation must refuse").kind
    }

    /// `stat` of a path (not an open), for expectations.
    fn stat_rdev(path: &Path) -> u64 {
        fs::metadata(path)
            .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
            .rdev()
    }

    /// A controlled copy of the sysfs layout: `sys/dev/char/<major>:<minor>`
    /// symlinks resolving to object directories with `dev` attributes, under
    /// a fresh temporary root. The same approach as the `host_state` tests:
    /// real symlinks and attribute files, real resolution semantics, no
    /// kernel objects claimed.
    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            loop {
                let root = std::env::temp_dir().join(format!(
                    "openxcvr-pinned-{tag}-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match Self::create_at(root) {
                    Ok(tree) => return tree,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create owned pinned fixture: {error}"),
                }
            }
        }

        fn create_at(root: PathBuf) -> io::Result<Self> {
            fs::create_dir(&root)?; // Exclusive: never adopt an existing tree.
            let tree = Tree { root }; // Own cleanup before creating descendants.
            fs::create_dir_all(tree.root.join("sys/dev/char"))?;
            Ok(tree)
        }

        fn sys(&self) -> PathBuf {
            self.root.join("sys")
        }

        /// Adds a synthetic sysfs object at `rel` whose `dev` attribute and
        /// `/sys/dev/char` link both name `major:minor`.
        fn add_object(&self, rel: &str, major: u32, minor: u32) {
            let object = self.sys().join(rel);
            fs::create_dir_all(&object).unwrap();
            fs::write(object.join("dev"), format!("{major}:{minor}\n")).unwrap();
            let link = self.sys().join(format!("dev/char/{major}:{minor}"));
            let _ = fs::remove_file(&link);
            symlink(format!("../../{rel}"), &link).unwrap();
        }

        fn remove_object(&self, rel: &str) {
            fs::remove_dir_all(self.sys().join(rel)).unwrap();
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn existing_fixture_root_is_never_adopted_or_removed() {
        let scratch = Tree::new("collision-owner");
        let occupied = scratch.root.join("occupied");
        fs::create_dir(&occupied).unwrap();
        let sentinel = occupied.join("sentinel");
        fs::write(&sentinel, b"belongs to the earlier owner").unwrap();
        assert_eq!(
            Tree::create_at(occupied.clone()).err().unwrap().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            b"belongs to the earlier owner"
        );
        let fresh = scratch.root.join("fresh");
        let owned = Tree::create_at(fresh.clone()).unwrap();
        assert!(owned.sys().join("dev/char").is_dir());
        drop(owned);
        assert!(!fresh.exists());
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            b"belongs to the earlier owner"
        );
    }

    /// A retargetable pathname: a symlink the selection is made through,
    /// which the adversary can repoint at a replacement device.
    struct NodeLink {
        path: PathBuf,
    }

    impl NodeLink {
        fn new(tag: &str, target: &Path) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "openxcvr-node-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            symlink(target, &path).unwrap();
            NodeLink { path }
        }

        fn retarget(&self, target: &Path) {
            fs::remove_file(self).unwrap();
            symlink(target, self).unwrap();
        }
    }

    impl AsRef<Path> for NodeLink {
        fn as_ref(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for NodeLink {
        fn drop(&mut self) {
            let _ = fs::remove_file(self);
        }
    }

    /// Two real, distinguishable character devices every user can open:
    /// A = `/dev/zero` (reads return bytes), B = `/dev/null` (reads return
    /// EOF). They stand in for two instances with the same static identity;
    /// the read behaviour is how the tests tell them apart.
    const A: &str = "/dev/zero";
    const B: &str = "/dev/null";

    // ---- pure helpers ----------------------------------------------------

    /// Pure dev_t decode fixtures; no host adapter or sysfs lookup. The pairs
    /// are the real `(major, minor)` of the host's own char nodes (measured
    /// once on this host by `stat`: `/dev/zero` 1:5, `/dev/null` 1:3,
    /// `/dev/ptmx` 5:2, `/dev/i2c-0` 89:0 — the old fixture read these live)
    /// plus the design's OFD-range boundary (4095, 1048575). The `dev_t`
    /// values are encoded with the glibc `makedev` layout
    /// (`sys/sysmacros.h`) the decoder mirrors.
    #[test]
    fn dev_t_decode_matches_encoded_pairs() {
        for (major, minor) in [(1u64, 5u64), (1, 3), (5, 2), (89, 0), (4095, 1048575)] {
            let dev = ((major & 0xfff) << 8)
                | (minor & 0xff)
                | ((minor & 0xffff_ff00) << 12)
                | ((major & 0xffff_f000) << 32);
            assert_eq!(
                (dev_major(dev), dev_minor(dev)),
                (major as u32, minor as u32)
            );
        }
    }

    /// The ioctl request numbers are the ones `linux/hidraw.h` computes.
    #[test]
    fn ioctl_numbers_match_linux_hidraw_header() {
        assert_eq!(HIDIOCGRDESCSIZE, 0x8004_4801); // _IOR('H', 0x01, int)
        assert_eq!(HIDIOCGRAWINFO, 0x8008_4803); // _IOR('H', 0x03, struct hidraw_devinfo)
    }

    /// The HID instance-name parse keeps `host_state::hid_ids`'s rules.
    #[test]
    fn hid_instance_name_parses_like_host_state() {
        assert_eq!(
            hid_instance_name("0003:1D50:777E.0006"),
            Some((0x1d50, 0x777e))
        );
        assert_eq!(
            hid_instance_name("0003:04D8:00DD.0001"),
            Some((0x04d8, 0x00dd))
        );
        assert_eq!(hid_instance_name("0003:04D8:00DD"), None); // no instance
        assert_eq!(hid_instance_name("0003:04D8:00DD.0001:x"), None); // extra field
        assert_eq!(hid_instance_name("hidraw0"), None);
        assert_eq!(hid_instance_name("0003:XXXX:00DD.0001"), None); // not hex
    }

    #[test]
    fn hidraw_shape_requires_a_hidraw_class_node_under_a_hid_instance() {
        assert_eq!(
            hidraw_parts("devices/virtual/misc/uhid/0003:1D50:777E.0006/hidraw/hidraw0"),
            Some(("0003:1D50:777E.0006".to_owned(), 0x1d50, 0x777e))
        );
        assert_eq!(hidraw_parts("devices/virtual/mem/zero"), None);
        assert_eq!(
            hidraw_parts("devices/x/0003:1D50:777E.0006/hidraw/hidraw"),
            None
        );
        // hidrawx is a different class directory: not a hidraw node.
        assert_eq!(
            hidraw_parts("devices/x/0003:1D50:777E.0006/hidrawx/hidraw0"),
            None
        );
        // A node with letters in its minor is not a hidraw node.
        assert_eq!(
            hidraw_parts("devices/x/0003:1D50:777E.0006/hidraw/hidrawx"),
            None
        );
        // Too short to carry an instance-bearing ancestor.
        assert_eq!(hidraw_parts("hidraw/hidraw0"), None);
    }

    #[test]
    fn i2c_shape_requires_an_i2c_dev_node_named_after_its_adapter() {
        assert_eq!(
            i2c_parts("devices/pci0000:00/0000:00:1f.3/i2c-0/i2c-dev/i2c-0"),
            Some("devices/pci0000:00/0000:00:1f.3/i2c-0".to_owned())
        );
        assert_eq!(
            i2c_parts("devices/synth/i2c-8/i2c-dev/i2c-8"),
            Some("devices/synth/i2c-8".to_owned())
        );
        // Node not named after its adapter.
        assert_eq!(i2c_parts("devices/x/i2c-8/i2c-dev/i2c-9"), None);
        // Not under the i2c-dev class.
        assert_eq!(i2c_parts("devices/x/i2c-8/i2c-devx/i2c-8"), None);
        // Require an ancestor for the expected path shape (not identity).
        assert_eq!(i2c_parts("i2c-8/i2c-dev/i2c-8"), None);
        assert_eq!(i2c_parts("devices/virtual/mem/zero"), None);
    }

    // ---- retained open ---------------------------------------------------

    #[test]
    fn retained_open_rejects_a_non_character_device() {
        let tree = Tree::new("regular-file");
        let file = tree.root.join("ordinary-file");
        fs::write(&file, b"").unwrap();
        assert_eq!(err_kind(RetainedCharFd::open(&file)), ErrorKind::Invalid);
    }

    #[test]
    fn retained_open_of_an_absent_node_is_disconnected() {
        let tree = Tree::new("absent");
        assert_eq!(
            err_kind(RetainedCharFd::open(&tree.root.join("absent-node"))),
            ErrorKind::Disconnected
        );
    }

    #[test]
    fn open_error_classifies_permission_denial_without_a_host_adapter() {
        let path = Path::new("<owned permission fixture>");
        assert_eq!(
            open_error(path, io::Error::from_raw_os_error(libc::EACCES)).kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            open_error(path, io::Error::from_raw_os_error(libc::EPERM)).kind,
            ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn retained_open_follows_a_symlink_to_a_real_character_device() {
        let link = NodeLink::new("open", Path::new(A));
        let retained = RetainedCharFd::open(link.as_ref()).unwrap();
        assert_eq!(retained.dev(), "1:5");
    }

    // ---- sysfs observation --------------------------------------------------

    #[test]
    fn sysfs_object_path_of_a_real_char_fd_matches_real_sysfs() {
        let retained = RetainedCharFd::open(Path::new(A)).unwrap();
        assert_eq!(
            retained.sysfs_object_path(Path::new("/sys")).unwrap(),
            "devices/virtual/mem/zero"
        );
    }

    #[test]
    fn sysfs_object_path_without_an_object_refuses() {
        let tree = Tree::new("empty");
        let retained = RetainedCharFd::open(Path::new(A)).unwrap();
        assert_eq!(
            err_kind(retained.sysfs_object_path(&tree.sys())),
            ErrorKind::Disconnected
        );
    }

    #[test]
    fn sysfs_object_with_a_disagreeing_dev_attribute_refuses() {
        // An object that lies about which device number it is: its link is
        // found through the fd's pair, but its dev attribute names another.
        let tree = Tree::new("devattr");
        tree.add_object("devices/synth/y", 1, 5);
        fs::write(tree.sys().join("devices/synth/y/dev"), "9:9\n").unwrap();
        let retained = RetainedCharFd::open(Path::new(A)).unwrap(); // really 1:5
        assert_eq!(
            err_kind(retained.sysfs_object_path(&tree.sys())),
            ErrorKind::Invalid
        );
    }

    #[test]
    fn sysfs_object_escaping_the_sysfs_root_refuses() {
        let tree = Tree::new("escape");
        let link = tree.sys().join("dev/char/1:5");
        symlink("../../../../../../../../etc", &link).unwrap();
        let retained = RetainedCharFd::open(Path::new(A)).unwrap();
        assert_eq!(
            err_kind(retained.sysfs_object_path(&tree.sys())),
            ErrorKind::Invalid
        );
    }

    /// The fd is retained first; the metadata collection happens on it. A
    /// device that disappears between the two phases must refuse, never
    /// observe something else.
    #[test]
    fn disappearance_during_metadata_collection_refuses() {
        let tree = Tree::new("vanish");
        tree.add_object("devices/synth/i2c-8/i2c-dev/i2c-8", 1, 5);
        let link = NodeLink::new("vanish", Path::new(A));
        let retained = RetainedCharFd::open(link.as_ref()).unwrap(); // phase 1: fd retained
        tree.remove_object("devices/synth/i2c-8/i2c-dev/i2c-8"); // instance vanishes
        assert_eq!(
            err_kind(retained.sysfs_object_path(&tree.sys())),
            ErrorKind::Disconnected
        );
    }

    // ---- i2c-dev selection ------------------------------------------------

    /// The post-open fixture: pin A's fd, retarget its path to B, then
    /// consume — the retained fd remains A and the path is not reopened.
    #[test]
    fn i2c_selection_uses_the_pinned_fd_after_the_path_is_retargeted() {
        let tree = Tree::new("i2c-retarget");
        tree.add_object("devices/synth/i2c-8/i2c-dev/i2c-8", 1, 5);
        let link = NodeLink::new("i2c-a", Path::new(A));
        let expected = I2cDevExpectation {
            adapter: "devices/synth/i2c-8".to_owned(),
        };
        let selection =
            PinnedSelection::select_i2c_dev(link.as_ref(), &expected, &tree.sys()).unwrap();
        assert_eq!(
            selection.sysfs_object(),
            "devices/synth/i2c-8/i2c-dev/i2c-8"
        );

        link.retarget(Path::new(B));
        let fd = selection.consume().unwrap();

        // The consumed descriptor is A: reading it yields bytes.
        let mut file = File::from(fd);
        let meta = file.metadata().unwrap();
        assert_eq!((dev_major(meta.rdev()), dev_minor(meta.rdev())), (1, 5));
        let mut buf = [0u8; 8];
        let n = file.read(&mut buf).unwrap();
        assert!(n > 0, "the pinned selection reads A, never B");

        // Threat model, pinned as evidence: a *fresh* open of the retargeted
        // path does reach B now — which is exactly why a pinned selection may
        // never reopen the path.
        let mut fresh = File::open(link.as_ref()).unwrap();
        assert_eq!(
            fresh.read(&mut buf).unwrap(),
            0,
            "the retargeted path names B (EOF)"
        );
        assert_eq!(
            (
                dev_major(stat_rdev(link.as_ref())),
                dev_minor(stat_rdev(link.as_ref()))
            ),
            (1, 3)
        );
    }

    /// A substitution with a *different* sysfs path and number refuses.
    /// This does not cover a replacement that reuses both observations.
    #[test]
    fn i2c_selection_of_a_retargeted_path_refuses() {
        let tree = Tree::new("i2c-substitute");
        tree.add_object("devices/synth/i2c-8/i2c-dev/i2c-8", 1, 5);
        tree.add_object("devices/synth/i2c-9/i2c-dev/i2c-9", 1, 3);
        let link = NodeLink::new("i2c-b", Path::new(B)); // already retargeted
        let expected = I2cDevExpectation {
            adapter: "devices/synth/i2c-8".to_owned(),
        };
        assert_eq!(
            err_kind(PinnedSelection::select_i2c_dev(
                link.as_ref(),
                &expected,
                &tree.sys()
            )),
            ErrorKind::Invalid
        );
    }

    /// A different sysfs pathname at the pinned device number makes
    /// consume refuse; identical pathname reuse is not detected.
    #[test]
    fn i2c_consume_refuses_when_another_instance_takes_the_device_number() {
        let tree = Tree::new("i2c-impersonate");
        tree.add_object("devices/synth/i2c-8/i2c-dev/i2c-8", 1, 5);
        let link = NodeLink::new("i2c-c", Path::new(A));
        let expected = I2cDevExpectation {
            adapter: "devices/synth/i2c-8".to_owned(),
        };
        let selection =
            PinnedSelection::select_i2c_dev(link.as_ref(), &expected, &tree.sys()).unwrap();
        // The sysfs object at the pinned device number is replaced by a
        // different instance with a consistent dev attribute.
        fs::remove_file(tree.sys().join("dev/char/1:5")).unwrap();
        tree.add_object("devices/synth/i2c-99/i2c-dev/i2c-99", 1, 5);
        assert_eq!(err_kind(selection.consume()), ErrorKind::Invalid);
    }

    /// Same-name/same-number ABA before the first open: A is enumerated,
    /// then its controlled sysfs entry is replaced by B at the identical
    /// location with identical `dev`. The caller's expectation is unchanged.
    /// There is no independent generation witness, so the checks accept B.
    /// This test documents the GAP; it does not validate an identity-safe open.
    #[test]
    fn i2c_same_name_same_number_before_open_is_not_distinguishable() {
        let tree = Tree::new("same-number-aba");
        const REL: &str = "devices/synth/i2c-8/i2c-dev/i2c-8";
        tree.add_object(REL, 1, 5); // enumeration of A
        let expected = I2cDevExpectation {
            adapter: "devices/synth/i2c-8".to_owned(),
        };
        tree.remove_object(REL); // A disappears before the fd can be pinned
        tree.add_object(REL, 1, 5); // B reuses A's pathname and number
        let selection = PinnedSelection::select_i2c_dev(Path::new(A), &expected, &tree.sys())
            .expect("reused observations cannot distinguish B from enumerated A");
        assert_eq!(selection.sysfs_object(), REL);
        let fd = selection
            .consume()
            .expect("reused observations pass the recheck; the gap is accepted, not covered");
        assert_eq!(dev_pair(fstat_rdev(fd.as_fd()).unwrap()), "1:5");
    }

    /// A wrong-class shape (a real char fd that is not an i2c-dev node)
    /// refuses as `Unsupported`, never selects.
    #[test]
    fn i2c_selection_of_a_non_i2c_object_refuses() {
        let expected = I2cDevExpectation {
            adapter: "devices/virtual/mem".to_owned(),
        };
        assert_eq!(
            err_kind(PinnedSelection::select_i2c_dev(
                Path::new(A),
                &expected,
                Path::new("/sys"),
            )),
            ErrorKind::Unsupported
        );
    }

    // ---- hidraw selection --------------------------------------------------

    /// The fd-direct hidraw identity cannot be satisfied by a lying sysfs
    /// copy: the descriptor must answer the hidraw ioctls, and `/dev/zero`
    /// does not.
    #[test]
    fn hidraw_selection_requires_the_descriptor_to_answer_hidraw_ioctls() {
        let tree = Tree::new("hidraw-lie");
        tree.add_object(
            "devices/virtual/misc/uhid/0003:1D50:777E.0006/hidraw/hidraw0",
            1,
            5,
        );
        let link = NodeLink::new("hid-a", Path::new(A));
        let expected = HidrawExpectation {
            sysfs_object: "devices/virtual/misc/uhid/0003:1D50:777E.0006/hidraw/hidraw0".to_owned(),
            vendor_id: 0x1d50,
            product_id: 0x777e,
        };
        assert_eq!(
            err_kind(PinnedSelection::select_hidraw(
                link.as_ref(),
                &expected,
                &tree.sys()
            )),
            ErrorKind::Unsupported
        );
    }

    /// A sysfs name that disagrees with the expectation refuses before any
    /// descriptor answer is trusted.
    #[test]
    fn hidraw_selection_of_a_mismatched_instance_name_refuses() {
        let tree = Tree::new("hidraw-name");
        tree.add_object(
            "devices/virtual/misc/uhid/0003:04D8:00DD.0001/hidraw/hidraw0",
            1,
            5,
        );
        let link = NodeLink::new("hid-b", Path::new(A));
        let expected = HidrawExpectation {
            sysfs_object: "devices/virtual/misc/uhid/0003:04D8:00DD.0001/hidraw/hidraw0".to_owned(),
            vendor_id: 0x1d50, // expected 1d50:777e; the tree says 04d8:00dd
            product_id: 0x777e,
        };
        assert_eq!(
            err_kind(PinnedSelection::select_hidraw(
                link.as_ref(),
                &expected,
                &tree.sys()
            )),
            ErrorKind::Invalid
        );
    }

    // ---- real-fd tier (root-gated) -----------------------------------------

    /// A byte-level mirror of `struct uhid_event` (`linux/uhid.h`), offsets
    /// verified against the header on this kernel: `type` at 0 (u32), the
    /// union at 4, `uhid_create2_req` fields at name +0, phys +128, uniq
    /// +192, rd_size +256 (u16), bus +258 (u16), vendor +260, product +264,
    /// version +268, country +272, rd_data +276; `uhid_output_req` data +0,
    /// size +4096 (u16). `sizeof(struct uhid_event) == 4380`.
    mod uhid {
        use std::fs;
        use std::io;
        use std::os::fd::{AsRawFd, OwnedFd};
        use std::os::unix::fs::OpenOptionsExt;
        use std::path::{Path, PathBuf};

        pub const EV_SIZE: usize = 4380;
        const UHID_CREATE2: u32 = 11;
        const UHID_DESTROY: u32 = 1;
        const UHID_START: u32 = 2;
        const UHID_OUTPUT: u32 = 6;
        const BUS_USB: u16 = 0x0003;
        /// A neutral VID/PID: the real MCP2221A pair would make the kernel's
        /// mcp2221 driver bind and unregister the hidraw node.
        pub const VENDOR: u32 = 0x1d50;
        pub const PRODUCT: u32 = 0x777e;

        fn put32(buf: &mut [u8; EV_SIZE], off: usize, v: u32) {
            buf[off..off + 4].copy_from_slice(&v.to_ne_bytes());
        }
        fn put16(buf: &mut [u8; EV_SIZE], off: usize, v: u16) {
            buf[off..off + 2].copy_from_slice(&v.to_ne_bytes());
        }

        pub struct Uhid {
            fd: OwnedFd,
        }

        impl Uhid {
            /// `None` if opening uhid fails; explicit evidence tests assert
            /// the prerequisite and fail rather than silently skipping.
            pub fn open() -> Option<Self> {
                let file = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(libc::O_CLOEXEC)
                    .open("/dev/uhid")
                    .ok()?;
                Some(Uhid { fd: file.into() })
            }

            /// Creates a synthetic HID device; udev/devtmpfs mints its
            /// `/dev/hidrawN` node. `product` isolates the *test*: it names
            /// the sysfs bus id (`0003:VVVV:PPPP.NNNN`), so each test counts
            /// only its own devices while devtmpfs asynchronously cleans up a
            /// previous test's. Within one test every device is created with
            /// the same `product`, so A and B share the full static identity
            /// the enumeration APIs expose — the A→B substitution the fixture
            /// must catch. (The HID bus sysfs directory exposes no `uniq`
            /// attribute, so the bus id is the only per-device selector
            /// sysfs offers here.)
            pub fn create(&self, product: u16, report_descriptor: &[u8]) -> io::Result<()> {
                let name = b"openxcvr-pinned-proof\0";
                let phys = b"proof-phys\0";
                let uniq = b"proof-serial\0";
                let mut ev = [0u8; EV_SIZE];
                put32(&mut ev, 0, UHID_CREATE2);
                ev[4..4 + name.len()].copy_from_slice(name);
                ev[132..132 + phys.len()].copy_from_slice(phys);
                ev[196..196 + uniq.len()].copy_from_slice(uniq);
                put16(&mut ev, 260, report_descriptor.len() as u16);
                put16(&mut ev, 262, BUS_USB);
                put32(&mut ev, 264, VENDOR);
                put32(&mut ev, 268, product as u32);
                put32(&mut ev, 272, 0x0100); // version
                put32(&mut ev, 276, 0); // country
                ev[280..280 + report_descriptor.len()].copy_from_slice(report_descriptor);
                write_all(&self.fd, &ev)
            }

            pub fn destroy(&self) -> io::Result<()> {
                let mut ev = [0u8; EV_SIZE];
                put32(&mut ev, 0, UHID_DESTROY);
                write_all(&self.fd, &ev)
            }

            /// Reads uhid events for up to `ms`, returning the payloads of
            /// every `UHID_OUTPUT` (the report the hidraw write delivered).
            pub fn drain_outputs(&self, ms: i32) -> io::Result<Vec<Vec<u8>>> {
                let mut outputs = Vec::new();
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(ms as u64);
                loop {
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        return Ok(outputs);
                    }
                    let mut pfd = libc::pollfd {
                        fd: self.fd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: `pfd` is a valid single-element poll array.
                    let rc = unsafe { libc::poll(&mut pfd, 1, 20) };
                    if rc <= 0 {
                        continue;
                    }
                    let mut ev = [0u8; EV_SIZE];
                    // SAFETY: reading from an owned, readable descriptor.
                    let n =
                        unsafe { libc::read(self.fd.as_raw_fd(), ev.as_mut_ptr().cast(), EV_SIZE) };
                    if n <= 0 {
                        return Ok(outputs);
                    }
                    let ty = u32::from_ne_bytes([ev[0], ev[1], ev[2], ev[3]]);
                    if ty == UHID_OUTPUT {
                        let size = u16::from_ne_bytes([ev[4 + 4096], ev[4 + 4097]]) as usize;
                        outputs.push(ev[4..4 + size].to_vec());
                    }
                    if ty == UHID_START || ty == UHID_OUTPUT {
                        let _ = ty; // both are expected along the way
                    }
                }
            }
        }

        fn write_all(fd: &OwnedFd, bytes: &[u8]) -> io::Result<()> {
            let mut off = 0;
            while off < bytes.len() {
                // SAFETY: writing to an owned, writable descriptor.
                let n = unsafe {
                    libc::write(
                        fd.as_raw_fd(),
                        bytes[off..].as_ptr().cast(),
                        bytes.len() - off,
                    )
                };
                if n < 0 {
                    return Err(io::Error::last_os_error());
                }
                off += n as usize;
            }
            Ok(())
        }

        /// The synthetic HID instances of `product` that currently enumerate,
        /// by bus name. Only this test's devices count.
        pub fn instances(product: u16) -> Vec<String> {
            let prefix = format!("0003:{VENDOR:04X}:{product:04X}.");
            let mut names: Vec<String> = fs::read_dir("/sys/bus/hid/devices")
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.starts_with(&prefix))
                .collect();
            names.sort();
            names
        }

        /// Waits until exactly one new instance of `product` enumerates and
        /// returns its hidraw node path and sysfs object path.
        pub fn wait_new_instance(product: u16, before: &[String]) -> Option<(PathBuf, String)> {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
            loop {
                let now = instances(product);
                let new: Vec<&String> = now.iter().filter(|n| !before.contains(n)).collect();
                // The bus id, its hidraw class subdir and the /dev node
                // appear asynchronously; keep polling until the instance is
                // fully registered rather than treating the window as
                // failure.
                if new.len() == 1 {
                    let instance = new[0].clone();
                    let dir = Path::new("/sys/bus/hid/devices").join(&instance);
                    if let Some(found) = fs::read_dir(dir.join("hidraw")).ok().and_then(|entries| {
                        entries.flatten().find(|e| {
                            e.file_name()
                                .to_str()
                                .is_some_and(|n| n.starts_with("hidraw"))
                        })
                    }) {
                        let name = found.file_name().into_string().ok()?;
                        let node = PathBuf::from("/dev").join(&name);
                        if fs::metadata(&node).is_ok() {
                            let object = fs::canonicalize(found.path()).ok()?;
                            let rel = object.strip_prefix("/sys").ok()?.to_str()?.to_owned();
                            return Some((node, rel));
                        }
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    /// A minimal valid report descriptor with one 8-bit input report, one
    /// 8-bit output report (the fd write path) and X/Y axes.
    /// Per-test product IDs isolate each test's synthetic devices in the
    /// sysfs bus id; within a test, A and B share the same product.
    const PRODUCT_A: u16 = 0x777e;
    const PRODUCT_B: u16 = 0x777f;
    const PRODUCT_C: u16 = 0x7780;

    const DESC: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, // Generic Desktop / Mouse / App
        0x09, 0x01, 0xa1, 0x00, // Pointer / Physical
        0x05, 0x09, 0x19, 0x01, 0x29, 0x03, // Buttons 1..3
        0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x03, 0x81, 0x02, 0x75, 0x05, 0x95, 0x01, 0x81,
        0x03, // padding
        0x75, 0x08, 0x95, 0x01, 0x91, 0x02, // 8-bit Output report
        0x95, 0x02, 0x75, 0x08, 0x09, 0x30, 0x09, 0x31, 0x81, 0x02, // X, Y
        0xc0, 0xc0,
    ];

    fn require_uhid() {
        assert!(
            uhid::Uhid::open().is_some(),
            "explicit privileged fixture requested without access to /dev/uhid"
        );
    }

    /// The explicitly privileged post-open fixture: two uhid devices with
    /// identical static fields. A's fd is pinned before retargeting its path
    /// to B; this does not test enumeration-to-first-open identity.
    #[test]
    #[ignore = "needs root and /dev/uhid; run: sudo <test binary> pinned::tests -- --ignored"]
    fn ignored_real_hidraw_pinned_selection_survives_retarget_to_identical_b() {
        require_uhid();
        let product = PRODUCT_A;
        let a = uhid::Uhid::open().unwrap();
        let before = uhid::instances(PRODUCT_A);
        a.create(PRODUCT_A, DESC).unwrap();
        let (a_node, a_object) = uhid::wait_new_instance(PRODUCT_A, &before).expect("A enumerates");
        let link = NodeLink::new("uhid-a", &a_node);
        let expected = HidrawExpectation {
            sysfs_object: a_object.clone(),
            vendor_id: uhid::VENDOR as u16,
            product_id: product,
        };
        let selection =
            PinnedSelection::select_hidraw(link.as_ref(), &expected, Path::new("/sys")).unwrap();

        // B: identical static identity in every enumeration-visible field.
        let b = uhid::Uhid::open().unwrap();
        let mid = uhid::instances(PRODUCT_A);
        b.create(PRODUCT_A, DESC).unwrap();
        let (b_node, b_object) = uhid::wait_new_instance(PRODUCT_A, &mid).expect("B enumerates");
        assert_ne!(a_object, b_object, "the instances must be distinct objects");
        link.retarget(&b_node);

        let fd = selection
            .consume()
            .expect("A is still alive: consume uses A");
        let a_rdev = stat_rdev(&a_node);
        assert_eq!(
            fstat_rdev(fd.as_fd()).unwrap(),
            a_rdev,
            "the consumed descriptor is A"
        );

        // The report goes to the pinned instance only.
        File::from(fd).write_all(&[0x00, 0x42]).unwrap();
        assert_eq!(a.drain_outputs(400).unwrap(), vec![vec![0x00, 0x42]]);
        let b_outputs = b.drain_outputs(300).unwrap();
        assert!(
            b_outputs.is_empty(),
            "B must receive nothing: {b_outputs:?}"
        );

        // Threat model, pinned as evidence: the retargeted path itself now
        // resolves to B for anyone who reopens it.
        assert_eq!(stat_rdev(link.as_ref()), stat_rdev(&b_node));
        a.destroy().unwrap();
        b.destroy().unwrap();
    }

    /// The pinned instance vanishing between the retained open and metadata
    /// collection must refuse, never observe a replacement.
    #[test]
    #[ignore = "needs root and /dev/uhid; run: sudo <test binary> pinned::tests -- --ignored"]
    fn ignored_real_hidraw_disappearance_during_metadata_refuses() {
        require_uhid();
        let a = uhid::Uhid::open().unwrap();
        let before = uhid::instances(PRODUCT_B);
        a.create(PRODUCT_B, DESC).unwrap();
        let (a_node, _) = uhid::wait_new_instance(PRODUCT_B, &before).expect("A enumerates");
        let retained = RetainedCharFd::open(&a_node).expect("fd retained first");
        a.destroy().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            err_kind(retained.sysfs_object_path(Path::new("/sys"))),
            ErrorKind::Disconnected
        );
    }

    /// After the pinned instance is destroyed and the path retargeted to an
    /// identical-identity replacement, consume refuses: A is gone, and B is
    /// never a substitute for it.
    #[test]
    #[ignore = "needs root and /dev/uhid; run: sudo <test binary> pinned::tests -- --ignored"]
    fn ignored_real_hidraw_consume_after_destroy_refuses_never_b() {
        require_uhid();
        let product = PRODUCT_C;
        let a = uhid::Uhid::open().unwrap();
        let before = uhid::instances(PRODUCT_C);
        a.create(PRODUCT_C, DESC).unwrap();
        let (a_node, a_object) = uhid::wait_new_instance(PRODUCT_C, &before).expect("A enumerates");
        let link = NodeLink::new("uhid-gone", &a_node);
        let expected = HidrawExpectation {
            sysfs_object: a_object.clone(),
            vendor_id: uhid::VENDOR as u16,
            product_id: product,
        };
        let selection =
            PinnedSelection::select_hidraw(link.as_ref(), &expected, Path::new("/sys")).unwrap();

        let b = uhid::Uhid::open().unwrap();
        let mid = uhid::instances(PRODUCT_C);
        b.create(PRODUCT_C, DESC).unwrap();
        let (b_node, _) = uhid::wait_new_instance(PRODUCT_C, &mid).expect("B enumerates");
        a.destroy().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        link.retarget(&b_node);

        match selection.consume() {
            Ok(_) => panic!("consume must refuse: the pinned instance is gone"),
            Err(e) => assert_eq!(e.kind, ErrorKind::Disconnected, "{e}"),
        }
        b.destroy().unwrap();
    }

    /// Explicitly privileged host-adapter fixture; not run by ordinary tests.
    /// Selection only — a capability query, never `I2C_SLAVE` or a transfer.
    /// It does not establish enumeration-to-first-open identity.
    #[test]
    #[ignore = "needs root to open the host adapter; run: sudo <test binary> pinned::tests -- --ignored"]
    fn ignored_real_i2c_adapter_selection_survives_retarget() {
        let node = Path::new("/dev/i2c-0");
        RetainedCharFd::open(node)
            .expect("explicit privileged fixture requested without access to /dev/i2c-0");
        let rdev = stat_rdev(node);
        let object = fs::canonicalize(format!(
            "/sys/dev/char/{}:{}",
            dev_major(rdev),
            dev_minor(rdev)
        ))
        .unwrap();
        let rel = object
            .strip_prefix("/sys")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        // The expected adapter ancestry, derived independently of the module's
        // shape helper: the object path minus its i2c-dev tail.
        let expected = I2cDevExpectation {
            adapter: rel
                .rsplit_once("/i2c-dev/")
                .map(|(head, _)| head.to_owned())
                .expect("a real i2c-dev object carries the i2c-dev tail"),
        };

        let link = NodeLink::new("i2c-real", node);
        let selection =
            PinnedSelection::select_i2c_dev(link.as_ref(), &expected, Path::new("/sys")).unwrap();
        link.retarget(Path::new(B));

        let fd = selection
            .consume()
            .expect("A is still alive: consume uses A");
        assert_eq!(
            fstat_rdev(fd.as_fd()).unwrap(),
            rdev,
            "the consumed descriptor is the host adapter"
        );
        // I2C_FUNCS is a capability query (linux/i2c-dev.h 0x0705): the
        // pinned descriptor still answers it.
        let mut funcs: libc::c_ulong = 0;
        // SAFETY: `fd` is owned and valid; `funcs` is a valid out-pointer.
        let rc = unsafe { libc::ioctl(fd.as_raw_fd(), 0x0705, &mut funcs) };
        assert_eq!(rc, 0, "the pinned adapter answers I2C_FUNCS");
        assert!(funcs != 0, "a real adapter reports a nonzero function mask");

        // Threat model, pinned as evidence: the retargeted path itself now
        // opens a device that is not an i2c adapter at all.
        let fresh = RetainedCharFd::open(link.as_ref()).unwrap();
        let mut mask: libc::c_ulong = 0;
        // SAFETY: fresh is owned and valid; `mask` is a valid out-pointer.
        let rc = unsafe { libc::ioctl(fresh.as_fd().as_raw_fd(), 0x0705, &mut mask) };
        assert!(
            rc != 0,
            "a fresh open of the retargeted path is not the adapter"
        );
    }
}
