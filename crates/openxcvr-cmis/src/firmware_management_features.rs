//! Pure CMIS **5.3** 0041h firmware-management features; no I/O or success claim.
//!
//! §9.4.2 Table 9-9, pp.296–297: Page 9Fh bytes 136..=153. Numerical
//! byte order is big endian (§8.1.3.5 p.138). Reserved values and unknown
//! codes are retained; these advertisements neither admit another command nor
//! expand the managed backend's transfer bounds. In particular there is no
//! 0100h support bit, and these five durations do not describe 0100h timing.
//! Like `firmware_info`, interpretation is library-owned; the current codegen
//! does not emit CDB codecs/projections. A raw value is not proof of completion.

use crate::adapter::cdb::CmisCdbFirmwareManagementFeaturesResponse;

/// Exact printed hex codes, NOT a two-bit mask (Table 9-9 p.297):
/// the firmware write/read mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// Firmware management is not supported.
    None,
    /// LPL writes only.
    Lpl,
    /// EPL writes only.
    Epl,
    /// Both LPL and EPL writes.
    LplAndEpl,
}
impl Mechanism {
    /// Decode the exact printed code; `None` for reserved combinations.
    pub fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0x00 => Some(Self::None),
            0x01 => Some(Self::Lpl),
            0x10 => Some(Self::Epl),
            0x11 => Some(Self::LplAndEpl),
            _ => None,
        }
    }
}

/// The module's hitless-restart behaviour class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitlessRestart {
    /// CMD Run Image causes a reset; traffic is affected.
    Reset,
    /// May reset; best effort to maintain traffic/management; data paths not reset.
    BestEffort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Typed 0041h firmware-management feature advertisement.
pub struct FirmwareManagementFeatures {
    /// Reserved byte 136, retained.
    pub reserved_136: u8,
    /// The raw capabilities byte (137).
    pub capabilities_raw: u8,
    /// Image readback of the running image is supported.
    pub image_readback: bool,
    /// Reserved bits 6..4, retained in their original positions.
    pub reserved_137_bits: u8,
    /// Durations are coded in 10 ms units rather than 1 ms.
    pub max_duration_coding: bool,
    /// M = 1 or 10; duration counts below remain unscaled, in units of M ms.
    pub duration_multiplier: u8,
    /// Skipping already-erased blocks is supported.
    pub skipping_erased_blocks: bool,
    /// CMD 0108h (Copy image) supported.
    pub copy_cmd: bool,
    /// CMD 0102h (Abort) supported.
    pub abort_cmd: bool,
    /// The vendor-data payload size the Start command accepts.
    pub start_cmd_payload_size: u8,
    /// The value the module erases blocks to.
    pub erased_byte: u8,
    /// The read/write length extension factor i.
    pub read_write_length_ext: u8,
    /// The raw write-mechanism code.
    pub write_mechanism_raw: u8,
    /// The raw read-mechanism code.
    pub read_mechanism_raw: u8,
    /// The raw hitless-restart code.
    pub hitless_restart_raw: u8,
    /// Maximum duration of Start, unscaled ms.
    pub max_duration_start: u16,
    /// Maximum duration of Abort, unscaled ms.
    pub max_duration_abort: u16,
    /// Maximum duration of Write Block, unscaled ms.
    pub max_duration_write: u16,
    /// Maximum duration of Complete, unscaled ms.
    pub max_duration_complete: u16,
    /// Maximum duration of Copy, unscaled ms.
    pub max_duration_copy: u16,
}
impl FirmwareManagementFeatures {
    /// The decoded write mechanism; `None` for reserved codes.
    pub fn write_mechanism(&self) -> Option<Mechanism> {
        Mechanism::from_raw(self.write_mechanism_raw)
    }
    /// The decoded read mechanism; `None` for reserved codes.
    pub fn read_mechanism(&self) -> Option<Mechanism> {
        Mechanism::from_raw(self.read_mechanism_raw)
    }
    /// The decoded hitless-restart class; `None` for reserved codes.
    pub fn hitless_restart(&self) -> Option<HitlessRestart> {
        match self.hitless_restart_raw {
            0 => Some(HitlessRestart::Reset),
            1 => Some(HitlessRestart::BestEffort),
            _ => None,
        }
    }
    /// Table 9-9 p.297: LPL extension is capped at min(i,15) octets.
    /// An advertisement, NOT authorization to change managed transfer limits.
    pub fn max_lpl_length(&self) -> u16 {
        8 * (1 + u16::from(self.read_write_length_ext.min(15)))
    }
    /// Table 9-9 p.297: 8*(1+i), up to 2048 bytes. Advertisement only.
    pub fn max_epl_length(&self) -> u16 {
        8 * (1 + u16::from(self.read_write_length_ext))
    }
}
impl From<&CmisCdbFirmwareManagementFeaturesResponse> for FirmwareManagementFeatures {
    fn from(reply: &CmisCdbFirmwareManagementFeaturesResponse) -> Self {
        let b = &reply.bytes;
        let duration = |i| u16::from_be_bytes([b[i], b[i + 1]]);
        Self {
            reserved_136: b[0],
            capabilities_raw: b[1],
            image_readback: b[1] & 0x80 != 0,
            reserved_137_bits: b[1] & 0x70,
            max_duration_coding: b[1] & 8 != 0,
            duration_multiplier: if b[1] & 8 == 0 { 1 } else { 10 },
            skipping_erased_blocks: b[1] & 4 != 0,
            copy_cmd: b[1] & 2 != 0,
            abort_cmd: b[1] & 1 != 0,
            start_cmd_payload_size: b[2],
            erased_byte: b[3],
            read_write_length_ext: b[4],
            write_mechanism_raw: b[5],
            read_mechanism_raw: b[6],
            hitless_restart_raw: b[7],
            max_duration_start: duration(8),     // 144–145
            max_duration_abort: duration(10),    // 146–147
            max_duration_write: duration(12),    // 148–149
            max_duration_complete: duration(14), // 150–151
            max_duration_copy: duration(16),     // 152–153
        }
    }
}

impl crate::managed_cdb::CdbOutcome {
    /// Features only for a Completed 0041h outcome; None for 0100h and every
    /// non-success outcome. Does not infer success from supplied raw bytes.
    pub fn firmware_management_features(&self) -> Option<FirmwareManagementFeatures> {
        match self {
            Self::CompletedFirmwareManagementFeatures { reply, .. } => Some(reply.into()),
            _ => None,
        }
    }
}
