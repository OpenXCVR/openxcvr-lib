//! Pure CMIS **5.3** 0100h firmware-information projection; no I/O or success claim.
//!
//! Layout: §9.7.1 Table 9-19 (PDF pp.309–310), Page 9Fh bytes 136–245.
//! Numerical byte order: §8.1.3.5 (p.138), big endian. The schema names a
//! `CmisFirmwareImageInfo` composite, but codegen does not resolve composites;
//! this additive interpretation therefore lives here, not in generated output.
//!
//! The raw response and `CdbOutcome::firmware_images` remain unchanged. This
//! projection validates the five response field lengths, not wire framing,
//! checksum, command status, or provenance. Those remain the command owner's
//! responsibility; in particular a manually constructed response is not proof
//! of completion. The managed path supplies a validated CMIS 5.3 reply.

use std::{error::Error, fmt};

use crate::{adapter::cdb::CmisCdbGetFirmwareInfoResponse, managed_cdb::CdbOutcome};

/// Whether a bank's image is executing (Table 9-19 bit 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalStatus {
    /// The image is not executing.
    NotRunning,
    /// The image is executing.
    Running,
}

/// Whether a bank's image has been committed by the host (bit 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdministrativeStatus {
    /// The image is not committed.
    Uncommitted,
    /// The image is committed.
    Committed,
}

/// A bank's image validity check result (bit 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidityStatus {
    /// The image passed the validity check.
    Valid,
    /// The image failed the validity check.
    Invalid,
}

/// Status of bank A or B, independent of version-information availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareImageStatus {
    /// Whether the image is executing.
    pub operational: OperationalStatus,
    /// Whether the image is committed.
    pub administrative: AdministrativeStatus,
    /// The image's validity check result.
    pub validity: ValidityStatus,
}

/// FirmwareStatus (9Fh.136), **not** command-completion `CdbStatus`.
///
/// Table 9-19 defines status bits for A and B only; factory/boot validity and
/// administrative status are not independently defined here and stay unknown.
/// Codes 00h/04h/40h/44h indicate a factory image or boot loader running if
/// supported (Table 7-4, p.114); they do not identify which of the two it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareStatus {
    /// Bank A's status nibble.
    pub image_a: FirmwareImageStatus,
    /// Bank B's status nibble.
    pub image_b: FirmwareImageStatus,
    /// Reserved bits 3 and 7, retained in their original bit positions.
    pub reserved_bits: u8,
}

/// ImageInformation (9Fh.137): whether each image's information is available.
/// This is not image validity, update support, or proof that an image exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInformation {
    /// Image A's information is available.
    pub image_a: bool,
    /// Image B's information is available.
    pub image_b: bool,
    /// Factory/boot image's information is available.
    pub factory_boot: bool,
    /// Undefined bits 3–7, retained raw, without assigning meaning.
    pub reserved_bits: u8,
}

/// One available 36-byte image-information record from Table 9-19.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareImageInfo {
    /// Major version.
    pub major: u8,
    /// Minor version.
    pub minor: u8,
    /// Build number (big endian).
    pub build: u16,
    /// All 32 ExtraString/ExtraStr bytes, unchanged.
    ///
    /// Table 9-19 calls these "Additional information" but specifies neither
    /// character encoding nor padding/termination. No trimming, NUL termination,
    /// lossy replacement, or printable-character filtering is performed: spaces,
    /// zeros (including embedded NUL), and non-printable bytes are preserved.
    /// An all-zero/all-space array is available information, not an absent image.
    pub extra: [u8; 32],
}

/// Typed 0100h firmware-information projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareInfo {
    /// The FirmwareStatus byte's projection.
    pub status: FirmwareStatus,
    /// Which images' information is available.
    pub image_information: ImageInformation,
    /// `None` makes *all* fields unavailable, regardless of their raw bytes.
    pub image_a: Option<FirmwareImageInfo>,
    /// Image B's record, when available.
    pub image_b: Option<FirmwareImageInfo>,
    /// Factory/boot image's record, when available.
    pub factory_boot: Option<FirmwareImageInfo>,
}

/// A response field does not have the exact Table 9-19 width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareInfoError {
    /// The response field with the wrong width.
    pub field: &'static str,
    /// The Table 9-19 width expected.
    pub expected: usize,
    /// The width the response carried.
    pub actual: usize,
}

impl fmt::Display for FirmwareInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CMIS 5.3 0100h {}: expected {} bytes, got {}",
            self.field, self.expected, self.actual
        )
    }
}
impl Error for FirmwareInfoError {}

fn exact<'a, const N: usize>(
    bytes: &'a [u8],
    field: &'static str,
) -> Result<&'a [u8; N], FirmwareInfoError> {
    bytes.try_into().map_err(|_| FirmwareInfoError {
        field,
        expected: N,
        actual: bytes.len(),
    })
}

fn image_status(bits: u8) -> FirmwareImageStatus {
    // Table 9-19 p.309 byte 136: bits 0/1/2 within each bank's nibble.
    FirmwareImageStatus {
        operational: if bits & 1 != 0 {
            OperationalStatus::Running
        } else {
            OperationalStatus::NotRunning
        },
        administrative: if bits & 2 != 0 {
            AdministrativeStatus::Committed
        } else {
            AdministrativeStatus::Uncommitted
        },
        validity: if bits & 4 != 0 {
            ValidityStatus::Invalid
        } else {
            ValidityStatus::Valid
        },
    }
}

fn image_info(bytes: &[u8; 36]) -> FirmwareImageInfo {
    // Table 9-19 pp.309–310: major +0, minor +1, build +2..3, extra +4..35
    // at bases A=138, B=174, factory/boot=210. §8.1.3.5 p.138: big endian.
    let mut extra = [0; 32];
    extra.copy_from_slice(&bytes[4..]);
    FirmwareImageInfo {
        major: bytes[0],
        minor: bytes[1],
        build: u16::from_be_bytes([bytes[2], bytes[3]]),
        extra,
    }
}

impl TryFrom<&CmisCdbGetFirmwareInfoResponse> for FirmwareInfo {
    type Error = FirmwareInfoError;

    fn try_from(reply: &CmisCdbGetFirmwareInfoResponse) -> Result<Self, Self::Error> {
        // Table 9-19 p.309: bytes 136 and 137, one byte each.
        let status = exact::<1>(&reply.firmware_status, "firmware_status")?[0];
        let info = exact::<1>(&reply.image_information, "image_information")?[0];
        // Validate *every* record before projecting anything, including absent
        // images. Total data width is exactly 1+1+36+36+36 = 110 bytes.
        let a = exact::<36>(&reply.image_a_firmware, "image_a_firmware")?;
        let b = exact::<36>(&reply.image_b_firmware, "image_b_firmware")?;
        // The byte-137 description says "201–245", but the named factory/boot
        // rows on p.310 specify 210–245, consistent with the 110-byte RPL.
        let factory = exact::<36>(&reply.factory_boot_firmware, "factory_boot_firmware")?;
        let image_information = ImageInformation {
            image_a: info & 1 != 0,
            image_b: info & 2 != 0,
            factory_boot: info & 4 != 0,
            reserved_bits: info & 0xf8,
        };
        Ok(Self {
            status: FirmwareStatus {
                image_a: image_status(status),
                image_b: image_status(status >> 4),
                reserved_bits: status & 0x88,
            },
            image_information,
            image_a: image_information.image_a.then(|| image_info(a)),
            image_b: image_information.image_b.then(|| image_info(b)),
            factory_boot: image_information.factory_boot.then(|| image_info(factory)),
        })
    }
}

impl CdbOutcome {
    /// Typed firmware information only for `Completed`; no information for any
    /// other outcome. Malformed completed-response fields are errors, never a
    /// partial value. This accessor does not validate or promote command status.
    pub fn firmware_info(&self) -> Result<Option<FirmwareInfo>, FirmwareInfoError> {
        match self {
            Self::Completed { reply, .. } => FirmwareInfo::try_from(reply).map(Some),
            _ => Ok(None),
        }
    }
}
