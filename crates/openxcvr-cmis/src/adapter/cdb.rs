use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE, Family, Rev};

use crate::module::CmisTransport;

const CDB_PAGE: u8 = 0x9F;
const CDB_BANK: u8 = 0;
const LOWER_PAGE: u8 = 0x00;
const CDB_CMD_OFFSET: u8 = 128;
const CDB_REPLY_HEADER_OFFSET: u8 = 134;
const CDB_LPL_OFFSET: u8 = 136;
const CDB_STATUS_OFFSET: u8 = 37;
const CDB_IS_BUSY: u8 = 0x80;
const CDB_FAILURE_START: u8 = 0x40;
const MAX_REQUEST_LPL_LEN: usize = 118;
const MAX_PAGE_LPL_LEN: usize = 120;

#[derive(Debug)]
#[non_exhaustive]
/// Why a typed CDB exchange failed.
pub enum CdbError {
    /// The module stayed busy past the caller's poll budget.
    Busy,
    /// The module returned a CDB error completion status.
    Failed {
        /// The raw completion status byte.
        status: u8,
    },
    /// The poll budget expired before the command completed.
    Timeout,
    /// The reply checksum did not match the reply bytes.
    ChecksumMismatch,
    /// The reply was shorter than the command's minimum reply length.
    TruncatedReply {
        /// The command's minimum reply length.
        expected: usize,
        /// The reply length actually received.
        actual: usize,
    },
    /// A fixed-layout codec requires exactly this many reply bytes.
    InvalidReplyLength {
        /// The exact reply length the codec requires.
        expected: usize,
        /// The reply length actually received.
        actual: usize,
    },
    /// The request's LPL payload exceeds the CDB LPL window.
    LplTooLong,
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The command is defined in the schema but was introduced in a CMIS
    /// revision newer than the module reports (`detected`). Raised by
    /// [`Cdb::send_checked`] / [`CdbOp::check_rev`] before any transport I/O,
    /// so no frame is written to a module that cannot service the command.
    UnsupportedInRev {
        /// The command that is not yet supported.
        op: CdbOp,
        /// The revision the command was introduced in.
        since: Rev,
        /// The revision the module reports.
        detected: Rev,
    },
    /// The supplied revision context is not one of the catalog CMIS
    /// revisions (3.0, 4.0, 5.0-5.4): a future revision the schema does not
    /// model (e.g. 5.5) or a foreign family. Every revision-qualified CDB
    /// entry point fails closed with this before any transport I/O — the
    /// per-op since/until windows only apply inside the catalog, so an
    /// open-ended "at least `since`" admission is never reachable.
    /// Sibling of `DpsmError::UnsupportedRevision` / `MsmError::UnsupportedRevision`.
    UnsupportedRevision {
        /// The revision (or family) the caller supplied.
        detected: Rev,
    },
    /// The command was removed in a CMIS revision newer than the last
    /// revision defining it (`until`); the module reports `detected` at or
    /// beyond the removal. Raised for CMD 0281h and CMD 0380h, both removed
    /// by CMIS 5.4.
    RemovedInRev {
        /// The command that was removed.
        op: CdbOp,
        /// The revision that removed it.
        until: Rev,
        /// The revision the module reports.
        detected: Rev,
    },
}

impl fmt::Display for CdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => write!(f, "CDB command is busy"),
            Self::Failed { status } => write!(f, "CDB command failed with status 0x{status:02x}"),
            Self::Timeout => write!(f, "CDB command timed out"),
            Self::ChecksumMismatch => write!(f, "CDB checksum mismatch"),
            Self::TruncatedReply { expected, actual } => write!(
                f,
                "CDB reply is truncated: expected at least {expected} LPL bytes, got {actual}"
            ),
            Self::InvalidReplyLength { expected, actual } => write!(
                f,
                "CDB reply length: expected exactly {expected} bytes, got {actual}"
            ),
            Self::LplTooLong => write!(f, "CDB LPL payload is too long"),
            Self::Transport(err) => write!(f, "CDB transport error: {err}"),
            Self::UnsupportedInRev {
                op,
                since,
                detected,
            } => write!(
                f,
                "CDB command {} (0x{:04x}) requires CMIS {}.{} but module reports CMIS {}.{}",
                op.name(),
                op.opcode(),
                since.major,
                since.minor,
                detected.major,
                detected.minor
            ),
            Self::UnsupportedRevision { detected } => write!(
                f,
                "CDB revision context {detected:?} is not a catalog CMIS revision \
                 (3.0, 4.0, 5.0-5.4)"
            ),
            Self::RemovedInRev {
                op,
                until,
                detected,
            } => write!(
                f,
                "CDB command {} (0x{:04x}) was removed after CMIS {}.{} but module reports CMIS {}.{}",
                op.name(),
                op.opcode(),
                until.major,
                until.minor,
                detected.major,
                detected.minor
            ),
        }
    }
}

impl Error for CdbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ModuleError> for CdbError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

/// Version-gated identity of a CMIS CDB command. Each variant corresponds 1:1
/// to an `openxcvr.cdb_command.v1` schema under `schemas/cmis/cdb-commands/`.
///
/// [`CdbOp::available_since`] mirrors each schema's `available_since:`
/// annotation and [`CdbOp::available_until`] its `available_until:`
/// annotation; the `cdb_op_available_since_matches_schema_annotations` drift
/// test (`tests/rev_gating.rs`) reads the schema tree and fails if the two
/// diverge, in either direction.
///
/// A variant carries identity and revision gating only. The typed request/
/// reply codecs are a separate layer ([`CdbCommand`] impls); the commands
/// added by the CMIS 5.4 delta work (0005h/0006h, 0050h/0051h, 0109h,
/// 010Bh/010Ch/010Dh, 0200h, 0212h-0217h, 0233h, 0281h) are modeled in the
/// schema tree and gated here, and their typed codecs are deferred until a
/// consumer needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbOp {
    /// 0040h Get Module Features.
    ModuleFeatures,
    /// 0042h Get Module Time.
    GetModuleTime,
    /// 0043h Set Module Time.
    SetModuleTime,
    /// 0044h Get Application Attributes.
    GetApplicationAttributes,
    /// 0045h Get Interface Code Description.
    GetInterfaceCodeDescription,
    /// 0041h Get Firmware Management Features.
    FirmwareManagementFeatures,
    /// 0100h Get Firmware Information.
    GetFirmwareInfo,
    /// 0101h Firmware Download Start.
    FirmwareDownloadStart,
    /// 0102h Firmware Write Block LPL.
    FirmwareWriteBlockLpl,
    /// 0103h Firmware Download Complete.
    FirmwareDownloadComplete,
    /// 0104h Run Firmware Load.
    RunFirmwareLoad,
    /// 0105h Check Firmware Activation Options.
    CheckFirmwareActivationOptions,
    /// 0106h Store Firmware Load Tag.
    StoreFirmwareLoadTag,
    /// 0107h Retrieve Firmware Load Tag.
    RetrieveFirmwareLoadTag,
    /// 0180h Control PM Statistics.
    ControlPmStatistics,
    /// 0181h Get PM Module LPL.
    PmModuleLpl,
    /// 0182h Get PM Module EPL.
    PmModuleEpl,
    /// 0183h Get PM Host Side LPL.
    GetPmHostSideLpl,
    /// 0184h Get PM Host Side EPL.
    GetPmHostSideEpl,
    /// 0185h Get PM Media Side LPL.
    GetPmMediaSideLpl,
    /// 0186h Get PM Media Side EPL.
    GetPmMediaSideEpl,
    /// 0187h Get PM Data Path LPL.
    GetPmDataPathLpl,
    /// 0188h Get PM Data Path EPL.
    GetPmDataPathEpl,
    /// 0200h RMON Statistics.
    RmonStatistics,
    /// 0202h FEC Symbol Error Histogram Control.
    FecSymbolErrorHistogramControl,
    /// 0203h Get Max FEC Symbol Error Weight.
    GetMaxFecSymbolErrorWeight,
    /// 020Ah Data Monitoring Controls.
    DataMonitoringControls,
    /// 020Bh Data Monitoring Advertisements.
    DataMonitoringAdvertisements,
    /// 020Ch Temperature Histogram.
    TemperatureHistogram,
    /// 0233h Loopbacks.
    Loopbacks,
    /// 0280h Device ID Certificate LPL.
    DeviceIdCertificateLpl,
    /// 0281h Set Digest To Sign LPL (removed in CMIS 5.4).
    SetDigestToSignLpl,
    /// 0282h Get Digest Signature LPL.
    GetDigestSignatureLpl,
}

impl CdbOp {
    /// Every known CDB opcode, in ascending opcode order. Kept in lockstep with
    /// the schema tree (enforced by the drift test); typed [`CdbCommand`] impls
    /// exist for the commands with runtime codecs today.
    pub const ALL: [CdbOp; 33] = [
        CdbOp::ModuleFeatures,
        CdbOp::GetModuleTime,
        CdbOp::SetModuleTime,
        CdbOp::GetApplicationAttributes,
        CdbOp::GetInterfaceCodeDescription,
        CdbOp::FirmwareManagementFeatures,
        CdbOp::GetFirmwareInfo,
        CdbOp::FirmwareDownloadStart,
        CdbOp::FirmwareWriteBlockLpl,
        CdbOp::FirmwareDownloadComplete,
        CdbOp::RunFirmwareLoad,
        CdbOp::CheckFirmwareActivationOptions,
        CdbOp::StoreFirmwareLoadTag,
        CdbOp::RetrieveFirmwareLoadTag,
        CdbOp::ControlPmStatistics,
        CdbOp::PmModuleLpl,
        CdbOp::PmModuleEpl,
        CdbOp::GetPmHostSideLpl,
        CdbOp::GetPmHostSideEpl,
        CdbOp::GetPmMediaSideLpl,
        CdbOp::GetPmMediaSideEpl,
        CdbOp::GetPmDataPathLpl,
        CdbOp::GetPmDataPathEpl,
        CdbOp::RmonStatistics,
        CdbOp::FecSymbolErrorHistogramControl,
        CdbOp::GetMaxFecSymbolErrorWeight,
        CdbOp::DataMonitoringControls,
        CdbOp::DataMonitoringAdvertisements,
        CdbOp::TemperatureHistogram,
        CdbOp::Loopbacks,
        CdbOp::DeviceIdCertificateLpl,
        CdbOp::SetDigestToSignLpl,
        CdbOp::GetDigestSignatureLpl,
    ];

    /// The 16-bit CDB command id (`cmd_id:` in the schema, `CMD_ID` on the
    /// matching [`CdbCommand`] impl).
    pub fn opcode(self) -> u16 {
        match self {
            CdbOp::GetModuleTime => 0x0005,
            CdbOp::SetModuleTime => 0x0006,
            CdbOp::ModuleFeatures => 0x0040,
            CdbOp::GetApplicationAttributes => 0x0050,
            CdbOp::GetInterfaceCodeDescription => 0x0051,
            CdbOp::FirmwareManagementFeatures => 0x0041,
            CdbOp::GetFirmwareInfo => 0x0100,
            CdbOp::FirmwareDownloadStart => 0x0101,
            CdbOp::FirmwareWriteBlockLpl => 0x0103,
            CdbOp::FirmwareDownloadComplete => 0x0107,
            CdbOp::RunFirmwareLoad => 0x0109,
            CdbOp::CheckFirmwareActivationOptions => 0x010b,
            CdbOp::StoreFirmwareLoadTag => 0x010c,
            CdbOp::RetrieveFirmwareLoadTag => 0x010d,
            CdbOp::ControlPmStatistics => 0x0200,
            CdbOp::PmModuleLpl => 0x0210,
            CdbOp::PmModuleEpl => 0x0211,
            CdbOp::GetPmHostSideLpl => 0x0212,
            CdbOp::GetPmHostSideEpl => 0x0213,
            CdbOp::GetPmMediaSideLpl => 0x0214,
            CdbOp::GetPmMediaSideEpl => 0x0215,
            CdbOp::GetPmDataPathLpl => 0x0216,
            CdbOp::GetPmDataPathEpl => 0x0217,
            CdbOp::RmonStatistics => 0x0220,
            CdbOp::FecSymbolErrorHistogramControl => 0x0230,
            CdbOp::GetMaxFecSymbolErrorWeight => 0x0233,
            CdbOp::DataMonitoringControls => 0x0280,
            CdbOp::DataMonitoringAdvertisements => 0x0281,
            CdbOp::TemperatureHistogram => 0x0290,
            CdbOp::Loopbacks => 0x0380,
            CdbOp::DeviceIdCertificateLpl => 0x0400,
            CdbOp::SetDigestToSignLpl => 0x0402,
            CdbOp::GetDigestSignatureLpl => 0x0404,
        }
    }

    /// The schema `id:` for this command — the stable identifier shared with
    /// codegen and the [`CdbCommand`] struct name.
    pub fn name(self) -> &'static str {
        match self {
            CdbOp::GetModuleTime => "CmisCdbGetModuleTime",
            CdbOp::SetModuleTime => "CmisCdbSetModuleTime",
            CdbOp::GetApplicationAttributes => "CmisCdbGetApplicationAttributes",
            CdbOp::GetInterfaceCodeDescription => "CmisCdbGetInterfaceCodeDescription",
            CdbOp::RunFirmwareLoad => "CmisCdbRunFirmwareLoad",
            CdbOp::CheckFirmwareActivationOptions => "CmisCdbCheckFirmwareActivationOptions",
            CdbOp::StoreFirmwareLoadTag => "CmisCdbStoreFirmwareLoadTag",
            CdbOp::RetrieveFirmwareLoadTag => "CmisCdbRetrieveFirmwareLoadTag",
            CdbOp::ControlPmStatistics => "CmisCdbControlPmStatistics",
            CdbOp::GetPmHostSideLpl => "CmisCdbGetPmHostSideLpl",
            CdbOp::GetPmHostSideEpl => "CmisCdbGetPmHostSideEpl",
            CdbOp::GetPmMediaSideLpl => "CmisCdbGetPmMediaSideLpl",
            CdbOp::GetPmMediaSideEpl => "CmisCdbGetPmMediaSideEpl",
            CdbOp::GetPmDataPathLpl => "CmisCdbGetPmDataPathLpl",
            CdbOp::GetPmDataPathEpl => "CmisCdbGetPmDataPathEpl",
            CdbOp::GetMaxFecSymbolErrorWeight => "CmisCdbGetMaxFecSymbolErrorWeight",
            CdbOp::DataMonitoringAdvertisements => "CmisCdbDataMonitoringAdvertisements",
            CdbOp::ModuleFeatures => "CmisCdbModuleFeatures",
            CdbOp::FirmwareManagementFeatures => "CmisCdbFirmwareManagementFeatures",
            CdbOp::GetFirmwareInfo => "CmisCdbGetFirmwareInfo",
            CdbOp::FirmwareDownloadStart => "CmisCdbFirmwareDownloadStart",
            CdbOp::FirmwareWriteBlockLpl => "CmisCdbFirmwareWriteBlockLpl",
            CdbOp::FirmwareDownloadComplete => "CmisCdbFirmwareDownloadComplete",
            CdbOp::PmModuleLpl => "CmisCdbPmModuleLpl",
            CdbOp::PmModuleEpl => "CmisCdbPmModuleEpl",
            CdbOp::RmonStatistics => "CmisCdbRmonStatistics",
            CdbOp::FecSymbolErrorHistogramControl => "CmisCdbFecSymbolErrorHistogramControl",
            CdbOp::DataMonitoringControls => "CmisCdbDataMonitoringControls",
            CdbOp::TemperatureHistogram => "CmisCdbTemperatureHistogram",
            CdbOp::Loopbacks => "CmisCdbLoopbacks",
            CdbOp::DeviceIdCertificateLpl => "CmisCdbDeviceIdCertificateLpl",
            CdbOp::SetDigestToSignLpl => "CmisCdbSetDigestToSignLpl",
            CdbOp::GetDigestSignatureLpl => "CmisCdbGetDigestSignatureLpl",
        }
    }

    /// The earliest qualified CMIS revision for this codec, mirroring
    /// the schema `available_since:` annotation.
    ///
    /// RMON statistics (0220h), FEC symbol-error histogram control (0230h), and
    /// the security group (0400h/0402h/0404h) were introduced in CMIS 5.3 — per
    /// the CMIS 5.3 revision-history "CDB Messages Changes" list and their
    /// absence from CMIS 5.2. The remaining ten date to the 4.0 introduction of
    /// the CDB (inferred per `docs/briefs/cmis-cross-rev-deltas.md` § 3 — no
    /// CMIS 4.0 PDF is cached and the ten schemas cite only CMIS 5.3 sections;
    /// gated on issue #185). Lockstep with the schema is enforced by the
    /// `cdb_op_available_since_matches_schema_annotations` drift test — that
    /// test proves code↔schema agreement, not schema↔spec truth.
    /// The 0041h layout is qualified only against 5.3; this is not a claim
    /// that the opcode first appeared in that revision.
    pub fn available_since(self) -> Rev {
        match self {
            // New in CMIS 5.4 (5.4 revision history 'CDB Messages Changes').
            CdbOp::GetModuleTime
            | CdbOp::SetModuleTime
            | CdbOp::CheckFirmwareActivationOptions
            | CdbOp::StoreFirmwareLoadTag
            | CdbOp::RetrieveFirmwareLoadTag => Rev::CMIS_5_4,
            // Added in CMIS 5.3 (5.3 revision history 'CDB Messages
            // Changes': 0050h, 0051h; E42: 0232h-0233h).
            CdbOp::FirmwareManagementFeatures
            | CdbOp::GetApplicationAttributes
            | CdbOp::GetInterfaceCodeDescription
            | CdbOp::GetMaxFecSymbolErrorWeight
            | CdbOp::RmonStatistics
            | CdbOp::FecSymbolErrorHistogramControl
            | CdbOp::DeviceIdCertificateLpl
            | CdbOp::SetDigestToSignLpl
            | CdbOp::GetDigestSignatureLpl => Rev::CMIS_5_3,
            // 0109h/0200h/0212h-0217h are named by the CMIS 4.0 table of
            // contents (4.0 § 9.6.10 Run FW Image, § 9.7.1 PM Controls,
            // § 9.7.4-9.7.6 per-lane/datapath PM), verified directly against
            // the cached CMIS 4.0 text; the ten pre-existing 4.0-era commands
            // keep the inference documented above.
            CdbOp::ModuleFeatures
            | CdbOp::GetFirmwareInfo
            | CdbOp::FirmwareDownloadStart
            | CdbOp::FirmwareWriteBlockLpl
            | CdbOp::FirmwareDownloadComplete
            | CdbOp::RunFirmwareLoad
            | CdbOp::ControlPmStatistics
            | CdbOp::PmModuleLpl
            | CdbOp::PmModuleEpl
            | CdbOp::GetPmHostSideLpl
            | CdbOp::GetPmHostSideEpl
            | CdbOp::GetPmMediaSideLpl
            | CdbOp::GetPmMediaSideEpl
            | CdbOp::GetPmDataPathLpl
            | CdbOp::GetPmDataPathEpl
            | CdbOp::DataMonitoringControls
            | CdbOp::DataMonitoringAdvertisements
            | CdbOp::TemperatureHistogram
            | CdbOp::Loopbacks => Rev::CMIS_4_0,
        }
    }

    /// The revision that last defines this command, mirroring the schema
    /// `available_until:` annotation. `None` means the command is still
    /// defined in the newest revision the schema tree models. Today only
    /// CMD 0281h and CMD 0380h carry a window: CMIS 5.4 removed the
    /// reserved-for-advertisement placeholder 0281h entirely (5.4 § 9.8
    /// group table lists 0281h-028Fh as Reserved), and dropped the 5.3
    /// § 9.11.1 / Table 9-49 definition of 0380h when chapter 9 folded
    /// 0300h-03FFh into "Reserved for diagnostics features" (Table 9-1;
    /// the string 0380h appears nowhere in OIF-CMIS-05.4).
    pub fn available_until(self) -> Option<Rev> {
        match self {
            CdbOp::DataMonitoringAdvertisements | CdbOp::Loopbacks => Some(Rev::CMIS_5_3),
            _ => None,
        }
    }

    /// Map a raw 16-bit opcode to its [`CdbOp`], or `None` if unknown.
    pub fn from_opcode(opcode: u16) -> Option<CdbOp> {
        CdbOp::ALL.into_iter().find(|op| op.opcode() == opcode)
    }

    /// Check whether this command is defined in the `detected` revision.
    /// Returns [`CdbError::UnsupportedRevision`] when `detected` is not a
    /// catalog CMIS revision (3.0, 4.0, 5.0-5.4 — `Rev::known_for`, the
    /// family's known-revision domain, which `Rev::from_major_byte_cmis`
    /// maps onto exactly), [`CdbError::UnsupportedInRev`] when the module
    /// predates the command's introduction, and
    /// [`CdbError::RemovedInRev`] when it reports a revision past the
    /// command's removal. The catalog gate is a runtime check — never a
    /// debug-only assertion — because [`Rev`] orders lexicographically by
    /// `(family, major, minor)`: a foreign-family or future revision sorts
    /// past every CMIS `available_since` and would otherwise ride the
    /// open-ended "at least `since`" admission straight to the wire.
    pub fn check_rev(self, detected: Rev) -> Result<(), CdbError> {
        validate_catalog_cmis_rev(detected)?;
        let since = self.available_since();
        if detected < since {
            Err(CdbError::UnsupportedInRev {
                op: self,
                since,
                detected,
            })
        } else if let Some(until) = self.available_until() {
            if detected > until {
                return Err(CdbError::RemovedInRev {
                    op: self,
                    until,
                    detected,
                });
            }
            Ok(())
        } else {
            Ok(())
        }
    }
}

/// The CDB gate for a revision context: CMIS-family and a member of the
/// family's known-revision domain (`Rev::known_for` — the same catalog the
/// DPSM/MSM/NPSM validators accept through their packed-byte round-trip),
/// or [`CdbError::UnsupportedRevision`]. Shared by
/// [`CdbOp::check_rev`] and [`Cdb::send_checked`] so a non-catalog context
/// can never reach a frame write.
fn validate_catalog_cmis_rev(detected: Rev) -> Result<(), CdbError> {
    if detected.family == Family::Cmis && Rev::known_for(Family::Cmis).contains(&detected) {
        Ok(())
    } else {
        Err(CdbError::UnsupportedRevision { detected })
    }
}

/// The typed codec of one CDB command: its request and response types,
/// opcode and reply contract.
pub trait CdbCommand {
    /// The request type the command encodes from.
    type Request;
    /// The response type the command decodes into.
    type Response;

    /// The command's CDB opcode.
    const CMD_ID: u16;

    /// Minimum REPLY payload length in bytes (counted from Page 9Fh byte 136)
    /// that a well-formed module must return for this command. A shorter reply
    /// is rejected with [`CdbError::TruncatedReply`] instead of being
    /// zero-padded into the decoded response. Commands with no fixed reply
    /// layout keep the default of 0.
    const MIN_REPLY_LPL_LEN: usize = 0;

    /// Encode the request into its LPL payload bytes.
    fn encode_lpl(req: &Self::Request) -> Vec<u8>;
    /// Decode a reply from its completion status and LPL bytes.
    fn decode_reply(status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError>;
}

/// The CDB framing helper: send typed commands over any CMIS transport.
pub struct Cdb;

impl Cdb {
    /// Send a typed CDB command and return the decoded response.
    /// Polls max_polls times with no sleep (caller is responsible for timing).
    pub fn send<C, T>(
        transport: &mut T,
        request: &C::Request,
        max_polls: u32,
    ) -> Result<C::Response, CdbError>
    where
        C: CdbCommand,
        T: CmisTransport,
    {
        let lpl = C::encode_lpl(request);
        if lpl.len() > MAX_REQUEST_LPL_LEN {
            return Err(CdbError::LplTooLong);
        }

        let cmd_msb = (C::CMD_ID >> 8) as u8;
        let cmd_lsb = C::CMD_ID as u8;
        let header_without_checksum = [cmd_msb, cmd_lsb, 0, 0, lpl.len() as u8];
        let request_checksum = chkcode(&header_without_checksum, &lpl);

        let mut frame = Vec::with_capacity(8 + lpl.len());
        frame.extend_from_slice(&[
            cmd_msb,
            cmd_lsb,
            0,
            0,
            lpl.len() as u8,
            request_checksum,
            0,
            0,
        ]);
        frame.extend_from_slice(&lpl);
        transport.write(I2C_ADDR_MODULE, CDB_PAGE, CDB_BANK, CDB_CMD_OFFSET, &frame)?;

        let mut status = 0;
        let mut completed = false;
        for _ in 0..max_polls {
            let mut status_buf = [0u8; 1];
            transport.read(
                I2C_ADDR_MODULE,
                LOWER_PAGE,
                0,
                CDB_STATUS_OFFSET,
                &mut status_buf,
            )?;
            status = status_buf[0];
            if status & CDB_IS_BUSY == 0 {
                completed = true;
                break;
            }
        }

        if !completed {
            return Err(CdbError::Timeout);
        }
        if status >= CDB_FAILURE_START {
            return Err(CdbError::Failed { status });
        }

        let mut reply_header = [0u8; 2];
        transport.read(
            I2C_ADDR_MODULE,
            CDB_PAGE,
            CDB_BANK,
            CDB_REPLY_HEADER_OFFSET,
            &mut reply_header,
        )?;
        let reply_len = reply_header[0] as usize;
        let reply_checksum = reply_header[1];
        validate_reply_length::<C>(reply_len)?;

        let mut reply_lpl = vec![0u8; reply_len];
        if reply_len > 0 {
            transport.read(
                I2C_ADDR_MODULE,
                CDB_PAGE,
                CDB_BANK,
                CDB_LPL_OFFSET,
                &mut reply_lpl,
            )?;
        }

        // CMIS 5.3 Table 8-179: RPLChkCode is the one's complement of the sum
        // of reply LPL bytes 136..(136+RPLLength-1); it is 0 only when there is
        // no reply message body, so verify exactly when a body is present.
        validate_reply_checksum(&reply_lpl, reply_checksum)?;
        C::decode_reply(status, &reply_lpl)
    }

    /// Like [`send`](Self::send) but first verifies the command is defined in
    /// `detected` — the module's frozen [`Rev`]. `detected` must be a
    /// catalog CMIS revision: a non-catalog context (a future revision such
    /// as 5.5, an unadmitted intermediate such as 3.1/4.1, or a foreign
    /// family) is refused with [`CdbError::UnsupportedRevision`] before any
    /// transport I/O, for mapped and unmapped opcodes alike — the gate
    /// cannot place an unmapped opcode on the revision timeline, but it
    /// still refuses to write to a module whose revision it does not model.
    /// For a catalog revision, opcodes with no known [`CdbOp`] mapping are
    /// sent unchecked, and mapped opcodes return
    /// [`CdbError::UnsupportedInRev`] / [`CdbError::RemovedInRev`] without
    /// touching the transport when the module predates or postdates the
    /// command's revision window. See [`CdbOp::check_rev`] for the gate.
    pub fn send_checked<C, T>(
        transport: &mut T,
        request: &C::Request,
        detected: Rev,
        max_polls: u32,
    ) -> Result<C::Response, CdbError>
    where
        C: CdbCommand,
        T: CmisTransport,
    {
        validate_catalog_cmis_rev(detected)?;
        if let Some(op) = CdbOp::from_opcode(C::CMD_ID) {
            op.check_rev(detected)?;
        }
        Self::send::<C, T>(transport, request, max_polls)
    }
}

/// Pure common reply framing checks; callers still own status/correlation.
pub(crate) fn validate_reply_length<C: CdbCommand>(len: usize) -> Result<(), CdbError> {
    if len > MAX_PAGE_LPL_LEN {
        return Err(CdbError::LplTooLong);
    }
    if len < C::MIN_REPLY_LPL_LEN {
        return Err(CdbError::TruncatedReply {
            expected: C::MIN_REPLY_LPL_LEN,
            actual: len,
        });
    }
    Ok(())
}
pub(crate) fn validate_reply_checksum(lpl: &[u8], checksum: u8) -> Result<(), CdbError> {
    if !lpl.is_empty() && chkcode(&[], lpl) != checksum {
        return Err(CdbError::ChecksumMismatch);
    }
    Ok(())
}
fn chkcode(header: &[u8], lpl: &[u8]) -> u8 {
    !header
        .iter()
        .chain(lpl)
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn rel(offset: usize) -> usize {
    offset - CDB_LPL_OFFSET as usize
}

fn put_u8(buf: &mut Vec<u8>, offset: usize, value: u8) {
    let at = rel(offset);
    if buf.len() <= at {
        buf.resize(at + 1, 0);
    }
    buf[at] = value;
}

fn put_u32be(buf: &mut Vec<u8>, offset: usize, value: u32) {
    put_bytes(buf, offset, &value.to_be_bytes());
}

fn put_bytes(buf: &mut Vec<u8>, offset: usize, bytes: &[u8]) {
    let at = rel(offset);
    if buf.len() < at + bytes.len() {
        buf.resize(at + bytes.len(), 0);
    }
    buf[at..at + bytes.len()].copy_from_slice(bytes);
}

fn put_fixed_vec(buf: &mut Vec<u8>, offset: usize, size: usize, bytes: &[u8]) {
    let at = rel(offset);
    if buf.len() < at + size {
        buf.resize(at + size, 0);
    }
    let copy_len = size.min(bytes.len());
    buf[at..at + copy_len].copy_from_slice(&bytes[..copy_len]);
}

fn put_variable_vec(buf: &mut Vec<u8>, offset: usize, max_size: usize, bytes: &[u8]) {
    let copy_len = max_size.min(bytes.len());
    put_bytes(buf, offset, &bytes[..copy_len]);
}

fn put_bit(buf: &mut Vec<u8>, offset: usize, bit_offset: u8, value: bool) {
    let at = rel(offset);
    if buf.len() <= at {
        buf.resize(at + 1, 0);
    }
    if value {
        buf[at] |= 1 << bit_offset;
    }
}

fn put_bitfield(buf: &mut Vec<u8>, offset: usize, bit_offset: u8, bit_width: u8, value: u8) {
    let at = rel(offset);
    if buf.len() <= at {
        buf.resize(at + 1, 0);
    }
    let mask = ((1u16 << bit_width) - 1) as u8;
    buf[at] |= (value & mask) << bit_offset;
}

fn fixed_bytes(lpl: &[u8], offset: usize, size: usize) -> Vec<u8> {
    let at = rel(offset);
    let mut out = vec![0u8; size];
    if at < lpl.len() {
        let copy_len = size.min(lpl.len() - at);
        out[..copy_len].copy_from_slice(&lpl[at..at + copy_len]);
    }
    out
}

fn variable_bytes(lpl: &[u8], offset: usize, max_size: usize) -> Vec<u8> {
    let at = rel(offset);
    if at >= lpl.len() {
        return Vec::new();
    }
    let copy_len = max_size.min(lpl.len() - at);
    lpl[at..at + copy_len].to_vec()
}

fn u8_at(lpl: &[u8], offset: usize) -> u8 {
    fixed_bytes(lpl, offset, 1)[0]
}

fn u16be_at(lpl: &[u8], offset: usize) -> u16 {
    let bytes = fixed_bytes(lpl, offset, 2);
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn u32be_at(lpl: &[u8], offset: usize) -> u32 {
    let bytes = fixed_bytes(lpl, offset, 4);
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

macro_rules! empty_request {
    ($request:ident) => {
        /// Empty request: the command carries no LPL payload.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $request;
    };
}

macro_rules! empty_response {
    ($response:ident) => {
        /// Empty response: the command returns no LPL payload.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $response;
    };
}

/// CDB 0040h (Get Module Features) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbModuleFeatures;
empty_request!(CmisCdbModuleFeaturesRequest);

/// Typed 0040h reply (CMIS 5.3 Table 9-8): raw bytes plus the one typed
/// field the managed surface consumes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbModuleFeaturesResponse {
    /// CDB flags byte (byte 136).
    pub cdb_flags: Vec<u8>,
    /// Reserved byte 137, verbatim.
    pub reserved_137: Vec<u8>,
    /// Supported-commands bitmap for opcodes 0000h-00FFh (bytes 138..169).
    pub supported_cmds_0000_00ff: Vec<u8>,
    /// Advertised maximum completion time, 100 ms units.
    pub max_completion_time: u16,
}

impl CdbCommand for CmisCdbModuleFeatures {
    type Request = CmisCdbModuleFeaturesRequest;
    type Response = CmisCdbModuleFeaturesResponse;
    const CMD_ID: u16 = 0x0040;
    const MIN_REPLY_LPL_LEN: usize = 36;

    fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
        Vec::new()
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbModuleFeaturesResponse {
            cdb_flags: fixed_bytes(lpl, 136, 1),
            reserved_137: fixed_bytes(lpl, 137, 1),
            supported_cmds_0000_00ff: fixed_bytes(lpl, 138, 32),
            max_completion_time: u16be_at(lpl, 170),
        })
    }
}

/// CMIS 5.3 §9.4.2 Table 9-9, pp.296–297; empty request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareManagementFeatures;
empty_request!(CmisCdbFirmwareManagementFeaturesRequest);

/// Exact Page 9Fh bytes 136..=153, including reserved/unknown values.
/// Supplied bytes alone are not evidence of correlated command completion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareManagementFeaturesResponse {
    /// The 18 reply bytes (Page 9Fh 136..=153), verbatim.
    pub bytes: [u8; 18],
}
impl CdbCommand for CmisCdbFirmwareManagementFeatures {
    type Request = CmisCdbFirmwareManagementFeaturesRequest;
    type Response = CmisCdbFirmwareManagementFeaturesResponse;
    const CMD_ID: u16 = 0x0041;
    const MIN_REPLY_LPL_LEN: usize = 18;

    fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
        Vec::new()
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        let bytes = lpl.try_into().map_err(|_| CdbError::InvalidReplyLength {
            expected: 18,
            actual: lpl.len(),
        })?;
        Ok(CmisCdbFirmwareManagementFeaturesResponse { bytes })
    }
}

/// CDB 0100h (Get Firmware Information) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbGetFirmwareInfo;
empty_request!(CmisCdbGetFirmwareInfoRequest);

/// Typed 0100h reply: firmware status, image advertisement and version
/// strings (36 bytes each, NUL-padded).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbGetFirmwareInfoResponse {
    /// Running/committed firmware status byte.
    pub firmware_status: Vec<u8>,
    /// Image information advertisement byte (bit 0 = image A, 1 = image B,
    /// 2 = factory boot).
    pub image_information: Vec<u8>,
    /// Image A version string, verbatim.
    pub image_a_firmware: Vec<u8>,
    /// Image B version string, verbatim.
    pub image_b_firmware: Vec<u8>,
    /// Factory-boot version string, verbatim.
    pub factory_boot_firmware: Vec<u8>,
}

impl CdbCommand for CmisCdbGetFirmwareInfo {
    type Request = CmisCdbGetFirmwareInfoRequest;
    type Response = CmisCdbGetFirmwareInfoResponse;
    const CMD_ID: u16 = 0x0100;
    const MIN_REPLY_LPL_LEN: usize = 110;

    fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
        Vec::new()
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbGetFirmwareInfoResponse {
            firmware_status: fixed_bytes(lpl, 136, 1),
            image_information: fixed_bytes(lpl, 137, 1),
            image_a_firmware: fixed_bytes(lpl, 138, 36),
            image_b_firmware: fixed_bytes(lpl, 174, 36),
            factory_boot_firmware: fixed_bytes(lpl, 210, 36),
        })
    }
}

/// CDB 0101h (Firmware Download Start) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareDownloadStart;

/// 0101h request: image size and vendor data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareDownloadStartRequest {
    /// Size of the image to download, bytes.
    pub image_size: u32,
    /// Reserved bytes 140-143, verbatim.
    pub reserved_140_143: Vec<u8>,
    /// Vendor-specific data (bytes 144..255).
    pub vendor_data: Vec<u8>,
}
empty_response!(CmisCdbFirmwareDownloadStartResponse);

impl CdbCommand for CmisCdbFirmwareDownloadStart {
    type Request = CmisCdbFirmwareDownloadStartRequest;
    type Response = CmisCdbFirmwareDownloadStartResponse;
    const CMD_ID: u16 = 0x0101;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_u32be(&mut lpl, 136, req.image_size);
        lpl.resize(8, 0);
        put_variable_vec(&mut lpl, 144, 112, &req.vendor_data);
        lpl
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbFirmwareDownloadStartResponse)
    }
}

/// CDB 0103h (Firmware Write Block LPL) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareWriteBlockLpl;

/// 0103h request: one firmware block's address and bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareWriteBlockLplRequest {
    /// Byte address the block writes at.
    pub block_address: u32,
    /// The block's bytes (up to 116).
    pub firmware_block: Vec<u8>,
}
empty_response!(CmisCdbFirmwareWriteBlockLplResponse);

impl CdbCommand for CmisCdbFirmwareWriteBlockLpl {
    type Request = CmisCdbFirmwareWriteBlockLplRequest;
    type Response = CmisCdbFirmwareWriteBlockLplResponse;
    const CMD_ID: u16 = 0x0103;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_u32be(&mut lpl, 136, req.block_address);
        put_variable_vec(&mut lpl, 140, 116, &req.firmware_block);
        lpl
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbFirmwareWriteBlockLplResponse)
    }
}

/// CDB 0107h (Firmware Download Complete) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbFirmwareDownloadComplete;
empty_request!(CmisCdbFirmwareDownloadCompleteRequest);
empty_response!(CmisCdbFirmwareDownloadCompleteResponse);

impl CdbCommand for CmisCdbFirmwareDownloadComplete {
    type Request = CmisCdbFirmwareDownloadCompleteRequest;
    type Response = CmisCdbFirmwareDownloadCompleteResponse;
    const CMD_ID: u16 = 0x0107;

    fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
        Vec::new()
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbFirmwareDownloadCompleteResponse)
    }
}

/// CDB 0210h (Get PM Module LPL) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbPmModuleLpl;

/// 0210h request: observables selector and clear-on-read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbPmModuleLplRequest {
    /// Clear the PM records after this read.
    pub clear_on_read: bool,
    /// Which record type the reply carries.
    pub record_type: bool,
    /// PM observables selector bytes.
    pub observables: Vec<u8>,
    /// Reserved bytes 138-140, verbatim.
    pub reserved_138_140: Vec<u8>,
}

/// 0210h reply: the raw PM record bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbPmModuleLplResponse {
    /// The PM records (bytes 136..255, variable).
    pub pm_records: Vec<u8>,
}

impl CdbCommand for CmisCdbPmModuleLpl {
    type Request = CmisCdbPmModuleLplRequest;
    type Response = CmisCdbPmModuleLplResponse;
    const CMD_ID: u16 = 0x0210;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_bit(&mut lpl, 136, 7, req.clear_on_read);
        put_bit(&mut lpl, 136, 0, req.record_type);
        put_fixed_vec(&mut lpl, 137, 1, &req.observables);
        lpl.resize(5, 0);
        lpl
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbPmModuleLplResponse {
            pm_records: variable_bytes(lpl, 136, 120),
        })
    }
}

/// CDB 0211h (Get PM Module EPL) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbPmModuleEpl;

/// 0211h request: same selector layout as 0210h, extended-payload reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbPmModuleEplRequest {
    /// Clear the PM records after this read.
    pub clear_on_read: bool,
    /// Which record type the reply carries.
    pub record_type: bool,
    /// PM observables selector bytes.
    pub observables: Vec<u8>,
    /// Reserved bytes 138-140, verbatim.
    pub reserved_138_140: Vec<u8>,
}
empty_response!(CmisCdbPmModuleEplResponse);

impl CdbCommand for CmisCdbPmModuleEpl {
    type Request = CmisCdbPmModuleEplRequest;
    type Response = CmisCdbPmModuleEplResponse;
    const CMD_ID: u16 = 0x0211;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_bit(&mut lpl, 136, 7, req.clear_on_read);
        put_bit(&mut lpl, 136, 0, req.record_type);
        put_fixed_vec(&mut lpl, 137, 1, &req.observables);
        lpl.resize(5, 0);
        lpl
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        // EPL response decoding needs the extended payload pages, not the LPL window.
        Ok(CmisCdbPmModuleEplResponse)
    }
}

/// CDB 0220h (RMON Statistics) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbRmonStatistics;

/// 0220h request: data path, monitor location and clear-on-read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbRmonStatisticsRequest {
    /// Clear the counters after this read.
    pub clear_on_read: bool,
    /// Which side to monitor (host transmit / media receive).
    pub monitor_location: bool,
    /// The data path to read statistics for.
    pub data_path_id: u8,
}

/// 0220h reply: raw 48-bit and 32-bit counter blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbRmonStatisticsResponse {
    /// The 48-bit counter block (bytes 136..207).
    pub rmon_u48_counters: Vec<u8>,
    /// The 32-bit counter block (bytes 208..255).
    pub rmon_u32_counters: Vec<u8>,
}

impl CdbCommand for CmisCdbRmonStatistics {
    type Request = CmisCdbRmonStatisticsRequest;
    type Response = CmisCdbRmonStatisticsResponse;
    const CMD_ID: u16 = 0x0220;
    const MIN_REPLY_LPL_LEN: usize = 120;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_bit(&mut lpl, 136, 7, req.clear_on_read);
        put_bit(&mut lpl, 136, 0, req.monitor_location);
        put_u8(&mut lpl, 137, req.data_path_id);
        lpl
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbRmonStatisticsResponse {
            rmon_u48_counters: fixed_bytes(lpl, 136, 72),
            rmon_u32_counters: fixed_bytes(lpl, 208, 48),
        })
    }
}

/// CDB 0230h (FEC Symbol Error Histogram Control) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbFecSymbolErrorHistogramControl;

/// 0230h request: histogram control command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbFecSymbolErrorHistogramControlRequest {
    /// The data path to control.
    pub data_path_id: u8,
    /// Which decoder location to control.
    pub decoder_location: bool,
    /// The control command (2 bits).
    pub command: u8,
}
empty_response!(CmisCdbFecSymbolErrorHistogramControlResponse);

impl CdbCommand for CmisCdbFecSymbolErrorHistogramControl {
    type Request = CmisCdbFecSymbolErrorHistogramControlRequest;
    type Response = CmisCdbFecSymbolErrorHistogramControlResponse;
    const CMD_ID: u16 = 0x0230;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_u8(&mut lpl, 136, req.data_path_id);
        put_bit(&mut lpl, 137, 0, req.decoder_location);
        put_bitfield(&mut lpl, 137, 1, 2, req.command);
        lpl
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbFecSymbolErrorHistogramControlResponse)
    }
}

/// CDB 0280h (Data Monitoring Controls) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbDataMonitoringControls;

/// 0280h request: clear-all-statistics control.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbDataMonitoringControlsRequest {
    /// Reserved bytes 136-137, verbatim.
    pub reserved_136_137: Vec<u8>,
    /// Clear all monitored statistics.
    pub clear_all_statistics: bool,
    /// Reserved byte 139, verbatim.
    pub reserved_139: Vec<u8>,
}
empty_response!(CmisCdbDataMonitoringControlsResponse);

impl CdbCommand for CmisCdbDataMonitoringControls {
    type Request = CmisCdbDataMonitoringControlsRequest;
    type Response = CmisCdbDataMonitoringControlsResponse;
    const CMD_ID: u16 = 0x0280;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_bit(&mut lpl, 138, 0, req.clear_all_statistics);
        lpl.resize(4, 0);
        lpl
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbDataMonitoringControlsResponse)
    }
}

/// CDB 0290h (Temperature Histogram) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbTemperatureHistogram;

/// 0290h request: subcommand selector.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbTemperatureHistogramRequest {
    /// The subcommand bytes (byte 136).
    pub subcommands: Vec<u8>,
}

/// 0290h reply: histogram seconds and bins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbTemperatureHistogramResponse {
    /// The echoed subcommand byte.
    pub subcommands_echo: Vec<u8>,
    /// Hours until the next NVR histogram write.
    pub hours_to_next_nvr_write: u8,
    /// Total seconds of accumulated histogram time.
    pub total_seconds: u32,
    /// The 44 ten-degree temperature bin counters.
    pub ten_degree_temperature_bins: Vec<u8>,
}

impl CdbCommand for CmisCdbTemperatureHistogram {
    type Request = CmisCdbTemperatureHistogramRequest;
    type Response = CmisCdbTemperatureHistogramResponse;
    const CMD_ID: u16 = 0x0290;
    const MIN_REPLY_LPL_LEN: usize = 52;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_fixed_vec(&mut lpl, 136, 1, &req.subcommands);
        lpl
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbTemperatureHistogramResponse {
            subcommands_echo: fixed_bytes(lpl, 136, 1),
            hours_to_next_nvr_write: u8_at(lpl, 137),
            total_seconds: u32be_at(lpl, 140),
            ten_degree_temperature_bins: fixed_bytes(lpl, 144, 44),
        })
    }
}

/// CDB 0380h (Loopbacks) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbLoopbacks;
empty_request!(CmisCdbLoopbacksRequest);
empty_response!(CmisCdbLoopbacksResponse);

impl CdbCommand for CmisCdbLoopbacks {
    type Request = CmisCdbLoopbacksRequest;
    type Response = CmisCdbLoopbacksResponse;
    const CMD_ID: u16 = 0x0380;

    fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
        Vec::new()
    }

    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbLoopbacksResponse)
    }
}

/// CDB 0400h (Device ID Certificate LPL) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbDeviceIdCertificateLpl;

/// 0400h request: which certificate segment to fetch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbDeviceIdCertificateLplRequest {
    /// The certificate index to read.
    pub certificate_index: u8,
    /// The segment index within that certificate.
    pub segment_index: u8,
}

/// 0400h reply: one certificate segment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbDeviceIdCertificateLplResponse {
    /// The module's return status for the request.
    pub return_status: u16,
    /// The echoed segment index.
    pub segment_index: u8,
    /// The certificate segment bytes (up to 117).
    pub certificate_segment: Vec<u8>,
}

impl CdbCommand for CmisCdbDeviceIdCertificateLpl {
    type Request = CmisCdbDeviceIdCertificateLplRequest;
    type Response = CmisCdbDeviceIdCertificateLplResponse;
    const CMD_ID: u16 = 0x0400;
    const MIN_REPLY_LPL_LEN: usize = 3;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_u8(&mut lpl, 136, req.certificate_index);
        put_u8(&mut lpl, 137, req.segment_index);
        lpl
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbDeviceIdCertificateLplResponse {
            return_status: u16be_at(lpl, 136),
            segment_index: u8_at(lpl, 138),
            certificate_segment: variable_bytes(lpl, 139, 117),
        })
    }
}

/// CDB 0402h (Set Digest To Sign LPL) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbSetDigestToSignLpl;

/// 0402h request: the digest bytes for the module to sign.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbSetDigestToSignLplRequest {
    /// The digest data (up to 120 bytes).
    pub digest_data: Vec<u8>,
}

/// 0402h reply: the module's acceptance status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbSetDigestToSignLplResponse {
    /// The module's result status.
    pub result_status: u16,
}

impl CdbCommand for CmisCdbSetDigestToSignLpl {
    type Request = CmisCdbSetDigestToSignLplRequest;
    type Response = CmisCdbSetDigestToSignLplResponse;
    const CMD_ID: u16 = 0x0402;
    const MIN_REPLY_LPL_LEN: usize = 2;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_variable_vec(&mut lpl, 136, 120, &req.digest_data);
        lpl
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbSetDigestToSignLplResponse {
            result_status: u16be_at(lpl, 136),
        })
    }
}

/// CDB 0403h (Get Digest Signature LPL) codec identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmisCdbGetDigestSignatureLpl;

/// 0403h request: which signature segment to fetch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbGetDigestSignatureLplRequest {
    /// The segment index within the signature.
    pub segment_index: u8,
}

/// 0403h reply: one signature segment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmisCdbGetDigestSignatureLplResponse {
    /// The module's return status for the request.
    pub return_status: u16,
    /// The echoed segment index.
    pub segment_index: u8,
    /// The signature segment bytes (up to 117).
    pub signature_segment: Vec<u8>,
}

impl CdbCommand for CmisCdbGetDigestSignatureLpl {
    type Request = CmisCdbGetDigestSignatureLplRequest;
    type Response = CmisCdbGetDigestSignatureLplResponse;
    const CMD_ID: u16 = 0x0404;
    const MIN_REPLY_LPL_LEN: usize = 3;

    fn encode_lpl(req: &Self::Request) -> Vec<u8> {
        let mut lpl = Vec::new();
        put_u8(&mut lpl, 136, req.segment_index);
        lpl
    }

    fn decode_reply(_status: u8, lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(CmisCdbGetDigestSignatureLplResponse {
            return_status: u16be_at(lpl, 136),
            segment_index: u8_at(lpl, 138),
            signature_segment: variable_bytes(lpl, 139, 117),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::transport::Transport;

    struct MockTransport {
        lower: [u8; 128],
        page_9f: [u8; 256],
        write_log: Vec<(u8, u8, u8, u8, Vec<u8>)>,
        read_call_count: u32,
        pending_reply: Vec<u8>,
        reply_checksum_override: Option<u8>,
    }

    impl MockTransport {
        fn new() -> Self {
            let mut lower = [0u8; 128];
            lower[37] = 0x01;
            Self {
                lower,
                page_9f: [0u8; 256],
                write_log: Vec::new(),
                read_call_count: 0,
                pending_reply: Vec::new(),
                reply_checksum_override: None,
            }
        }

        fn set_cdb_status(&mut self, status: u8) {
            self.lower[37] = status;
        }

        /// Arm a module reply to be published when the next command frame
        /// lands: RPLLength at 9Fh:134, RPLChkCode at 9Fh:135 (one's
        /// complement of the payload sum, per CMIS 5.3 Table 8-179), payload
        /// in the LPL area from 9Fh:136.
        fn stage_reply(&mut self, lpl: &[u8]) {
            self.pending_reply = lpl.to_vec();
        }
    }

    impl Transport for MockTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(bank, 0);
            if page == LOWER_PAGE && offset == CDB_STATUS_OFFSET {
                self.read_call_count += 1;
                let status = if self.lower[37] == 0x81 && self.read_call_count > 1 {
                    0x01
                } else {
                    self.lower[37]
                };
                buf[0] = status;
                return Ok(());
            }

            let start = offset as usize;
            if page == LOWER_PAGE {
                buf.copy_from_slice(&self.lower[start..start + buf.len()]);
            } else if page == CDB_PAGE {
                buf.copy_from_slice(&self.page_9f[start..start + buf.len()]);
            } else {
                return Err(ModuleError::Io(
                    format!("unexpected page 0x{page:02x}").into(),
                ));
            }
            Ok(())
        }

        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            self.write_log
                .push((slave_addr, page, bank, offset, data.to_vec()));
            let start = offset as usize;
            if page == CDB_PAGE {
                self.page_9f[start..start + data.len()].copy_from_slice(data);
                if offset == CDB_CMD_OFFSET && !self.pending_reply.is_empty() {
                    // The module executed the command: publish the reply
                    // header and payload (the host frame just overwrote the
                    // header range, as on real hardware).
                    let reply = std::mem::take(&mut self.pending_reply);
                    self.page_9f[134] = reply.len() as u8;
                    self.page_9f[135] = self
                        .reply_checksum_override
                        .take()
                        .unwrap_or_else(|| chkcode(&[], &reply));
                    self.page_9f[136..136 + reply.len()].copy_from_slice(&reply);
                }
                Ok(())
            } else {
                Err(ModuleError::Io(
                    format!("unexpected write page 0x{page:02x}").into(),
                ))
            }
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_module_features_success() {
        let mut transport = MockTransport::new();
        transport.stage_reply(&[0u8; 36]);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1);
        assert!(response.is_ok());
    }

    #[test]
    fn test_send_writes_correct_frame() {
        let mut transport = MockTransport::new();
        let request = CmisCdbDataMonitoringControlsRequest {
            clear_all_statistics: true,
            ..Default::default()
        };
        Cdb::send::<CmisCdbDataMonitoringControls, _>(&mut transport, &request, 1).unwrap();

        let (slave, page, bank, offset, data) = &transport.write_log[0];
        assert_eq!((*slave, *page, *bank, *offset), (0x50, 0x9F, 0, 128));
        assert_eq!(
            &data[..8],
            &[0x02, 0x80, 0x00, 0x00, 0x04, 0x78, 0x00, 0x00]
        );
        assert_eq!(&data[8..12], &[0, 0, 1, 0]);
    }

    #[test]
    fn test_busy_then_success() {
        let mut transport = MockTransport::new();
        transport.set_cdb_status(0x81);
        transport.stage_reply(&[0u8; 36]);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 2);
        assert!(response.is_ok());
        assert_eq!(transport.read_call_count, 2);
    }

    #[test]
    fn test_timeout() {
        let mut transport = MockTransport::new();
        transport.set_cdb_status(0x81);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1);
        assert!(matches!(response, Err(CdbError::Timeout)));
    }

    #[test]
    fn test_failed_status() {
        let mut transport = MockTransport::new();
        transport.set_cdb_status(0x42);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1);
        assert!(matches!(response, Err(CdbError::Failed { status: 0x42 })));
    }

    #[test]
    fn test_firmware_download_sequence() {
        let mut transport = MockTransport::new();
        Cdb::send::<CmisCdbFirmwareDownloadStart, _>(
            &mut transport,
            &CmisCdbFirmwareDownloadStartRequest {
                image_size: 1024,
                ..Default::default()
            },
            1,
        )
        .unwrap();
        assert_eq!(&transport.write_log[0].4[..2], &[0x01, 0x01]);

        Cdb::send::<CmisCdbFirmwareWriteBlockLpl, _>(
            &mut transport,
            &CmisCdbFirmwareWriteBlockLplRequest {
                block_address: 0,
                firmware_block: vec![0xA5; 16],
            },
            1,
        )
        .unwrap();
        assert_eq!(&transport.write_log[1].4[..2], &[0x01, 0x03]);

        Cdb::send::<CmisCdbFirmwareDownloadComplete, _>(
            &mut transport,
            &CmisCdbFirmwareDownloadCompleteRequest,
            1,
        )
        .unwrap();
        assert_eq!(&transport.write_log[2].4[..2], &[0x01, 0x07]);
    }

    #[test]
    fn test_send_checked_blocks_unsupported_rev() {
        let mut transport = MockTransport::new();
        // RmonStatistics (0x0220) was introduced in CMIS 5.3; a 4.0 module must
        // be refused before any frame reaches the transport.
        let err = Cdb::send_checked::<CmisCdbRmonStatistics, _>(
            &mut transport,
            &CmisCdbRmonStatisticsRequest::default(),
            Rev::CMIS_4_0,
            1,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CdbError::UnsupportedInRev {
                op: CdbOp::RmonStatistics,
                since: Rev::CMIS_5_3,
                detected: Rev::CMIS_4_0,
            }
        ));
        assert!(transport.write_log.is_empty());
    }

    #[test]
    fn test_send_checked_allows_supported_rev() {
        let mut transport = MockTransport::new();
        // ModuleFeatures (0x0040) dates to 4.0; a 5.3 module proceeds and the
        // frame is written.
        transport.stage_reply(&[0u8; 36]);
        Cdb::send_checked::<CmisCdbModuleFeatures, _>(
            &mut transport,
            &CmisCdbModuleFeaturesRequest,
            Rev::CMIS_5_3,
            1,
        )
        .unwrap();
        assert_eq!(transport.write_log.len(), 1);
    }

    /// Non-catalog revision contexts are refused by `check_rev` itself:
    /// the future 5.5, the unadmitted 3.1/4.1 intermediates, and foreign
    /// families. The per-op since/until windows keep their meaning inside
    /// the catalog (4.0 predates RMON's 5.3; 5.4 removed 0281h).
    #[test]
    fn test_check_rev_refuses_non_catalog_revisions() {
        let non_catalog = [
            Rev {
                family: Family::Cmis,
                major: 5,
                minor: 5,
            },
            Rev {
                family: Family::Cmis,
                major: 3,
                minor: 1,
            },
            Rev {
                family: Family::Cmis,
                major: 4,
                minor: 1,
            },
            Rev {
                family: Family::Sff8472,
                major: 12,
                minor: 5,
            },
            Rev {
                family: Family::Sff8636,
                major: 2,
                minor: 12,
            },
            Rev {
                family: Family::CCmis,
                major: 1,
                minor: 4,
            },
            Rev {
                family: Family::Elsfp,
                major: 1,
                minor: 0,
            },
        ];
        for rev in non_catalog {
            assert!(
                matches!(
                    CdbOp::ModuleFeatures.check_rev(rev),
                    Err(CdbError::UnsupportedRevision { detected }) if detected == rev
                ),
                "{rev:?}"
            );
            // Even the 5.4-introduced command refuses 5.5: "newer than
            // since" is not admission without catalog membership.
            assert!(
                matches!(
                    CdbOp::GetModuleTime.check_rev(rev),
                    Err(CdbError::UnsupportedRevision { detected }) if detected == rev
                ),
                "{rev:?}"
            );
        }
        // The windows still apply inside the catalog, both directions.
        assert!(matches!(
            CdbOp::RmonStatistics.check_rev(Rev::CMIS_4_0),
            Err(CdbError::UnsupportedInRev { .. })
        ));
        assert!(CdbOp::RmonStatistics.check_rev(Rev::CMIS_5_3).is_ok());
        assert!(matches!(
            CdbOp::DataMonitoringAdvertisements.check_rev(Rev::CMIS_5_4),
            Err(CdbError::RemovedInRev { .. })
        ));
    }

    /// The old open-ended admission bug, pinned shut: `available_until` is
    /// `None` for ModuleFeatures (0x0040, since 4.0), so CMIS 4.1 and 5.5
    /// used to pass `check_rev` by ordering and the frame reached the
    /// transport. The runtime catalog gate refuses them — and the foreign
    /// families that sort past every CMIS `since` — before any I/O: zero
    /// writes, zero reads.
    #[test]
    fn test_send_checked_refuses_non_catalog_revisions_without_io() {
        let non_catalog = [
            Rev {
                family: Family::Cmis,
                major: 5,
                minor: 5,
            },
            Rev {
                family: Family::Cmis,
                major: 4,
                minor: 1,
            },
            Rev {
                family: Family::Cmis,
                major: 3,
                minor: 1,
            },
            Rev {
                family: Family::Sff8472,
                major: 12,
                minor: 5,
            },
            Rev {
                family: Family::CCmis,
                major: 1,
                minor: 4,
            },
        ];
        for rev in non_catalog {
            let mut transport = MockTransport::new();
            let err = Cdb::send_checked::<CmisCdbModuleFeatures, _>(
                &mut transport,
                &CmisCdbModuleFeaturesRequest,
                rev,
                1,
            )
            .unwrap_err();
            assert!(
                matches!(err, CdbError::UnsupportedRevision { detected } if detected == rev),
                "{rev:?}: got {err:?}"
            );
            assert!(
                transport.write_log.is_empty(),
                "{rev:?}: no frame may reach the transport"
            );
            assert_eq!(
                transport.read_call_count, 0,
                "{rev:?}: no status poll may precede the refusal"
            );
        }
    }

    /// An opcode with no `CdbOp` mapping is sent unchecked on a catalog
    /// revision, but a non-catalog revision context still never reaches
    /// the wire: the gate validates the revision itself, not only the
    /// opcode's place on the timeline.
    struct UnmappedProbe;
    #[derive(Debug, Default)]
    struct UnmappedProbeRequest;
    #[derive(Debug)]
    struct UnmappedProbeResponse;
    impl CdbCommand for UnmappedProbe {
        type Request = UnmappedProbeRequest;
        type Response = UnmappedProbeResponse;
        // Not a `CdbOp` opcode: `from_opcode` yields None.
        const CMD_ID: u16 = 0x7F7F;
        fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
            Vec::new()
        }
        fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
            Ok(UnmappedProbeResponse)
        }
    }

    #[test]
    fn test_send_checked_refuses_unmapped_opcode_on_non_catalog_rev() {
        let mut transport = MockTransport::new();
        let err = Cdb::send_checked::<UnmappedProbe, _>(
            &mut transport,
            &UnmappedProbeRequest,
            Rev {
                family: Family::Cmis,
                major: 5,
                minor: 5,
            },
            1,
        )
        .unwrap_err();
        assert!(matches!(err, CdbError::UnsupportedRevision { .. }));
        assert!(transport.write_log.is_empty());
        assert_eq!(transport.read_call_count, 0);
    }

    #[test]
    fn test_reply_fields_decode_from_reply_bytes() {
        let mut transport = MockTransport::new();
        let mut lpl = [0u8; 36];
        lpl[0] = 0xA5; // cdb_flags
        lpl[170 - 136] = 0x12;
        lpl[171 - 136] = 0x34; // max_completion_time
        transport.stage_reply(&lpl);

        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1)
                .unwrap();
        assert_eq!(response.cdb_flags, vec![0xA5]);
        assert_eq!(response.max_completion_time, 0x1234);
    }

    #[test]
    fn test_truncated_reply_rejected() {
        let mut transport = MockTransport::new();
        // ModuleFeatures defines a 36-byte reply; a module reporting only 10
        // must be rejected, not zero-padded into a plausible-looking response.
        transport.stage_reply(&[0u8; 10]);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1);
        assert!(matches!(
            response,
            Err(CdbError::TruncatedReply {
                expected: 36,
                actual: 10
            })
        ));
    }

    #[test]
    fn test_reply_checksum_mismatch_rejected() {
        let mut transport = MockTransport::new();
        transport.stage_reply(&[0x5Au8; 36]);
        transport.reply_checksum_override = Some(0x00);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1);
        assert!(matches!(response, Err(CdbError::ChecksumMismatch)));
    }

    #[test]
    fn test_reply_checksum_of_zero_is_verified_when_body_present() {
        let mut transport = MockTransport::new();
        // A payload summing to 0xFF mod 256 has a legitimate RPLChkCode of 0.
        let mut lpl = [0u8; 36];
        lpl[0] = 0xFF;
        transport.stage_reply(&lpl);
        let response =
            Cdb::send::<CmisCdbModuleFeatures, _>(&mut transport, &CmisCdbModuleFeaturesRequest, 1);
        assert!(response.is_ok());
    }
    #[test]
    fn features_0041_checksum_covers_declared_18_bytes_only() {
        // Table 8-179 p.283: one's complement of bytes 136..153,
        // not the reply header or undefined tail starting at byte 154.
        let mut page = [0u8; 128];
        for (i, b) in page[8..26].iter_mut().enumerate() {
            *b = (i as u8) * 13;
        }
        let checksum = !page[8..26].iter().fold(0u8, |s, b| s.wrapping_add(*b));
        assert!(validate_reply_checksum(&page[8..26], checksum).is_ok());
        page[26] ^= 0xff;
        assert!(validate_reply_checksum(&page[8..26], checksum).is_ok());
        for i in 8..26 {
            page[i] ^= 1;
            assert!(matches!(
                validate_reply_checksum(&page[8..26], checksum),
                Err(CdbError::ChecksumMismatch)
            ));
            page[i] ^= 1;
        }
    }
}
