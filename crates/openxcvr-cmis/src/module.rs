use std::cell::RefCell;

use openxcvr_core::{
    error::{IoError, ModuleError},
    module::Module,
    rev::Rev,
    transport::{Transport, I2C_ADDR_MODULE},
    types::{ChecksumStatus, LaneFlags, LaneStatus, ModuleFlags, VendorInfo},
};

use crate::adapter::{Cdb, CdbCommand};
use crate::ctx::Ctx;
use crate::error::CmisError;
// Lower memory, Page 00h (Administrative), Page 11h (Lane Status) and the
// Page 01h (Advertising) bias factor are VERSIONED surfaces: the flat
// union types mix fields across CMIS 3.0..5.3 with no type-level say in
// which is real for the module in hand. `read_lower`, `read_page_00h`,
// `read_page_11h` and `tx_bias_multiplier` never decode the flat union
// types (or raw byte shifts); instead they select the
// revision-specialised generated reader for the frozen `ctx.detected_rev`
// and project only the fields this module consumes — see
// [`LowerCommon`]/`project_lower!`, [`Page00Common`]/`project_page_00!`,
// [`Page11Common`]/`project_page_11!` and `project_page_01_bias!` below.
// Physical 128-byte upper-page acquisition qualification remains separate
// from this decoder type dispatch: dispatch decides WHICH generated reader
// decodes a page, not how the page's bytes are acquired — non-identity
// Page00 fields and legacy 128-byte upper-read acquisition collateral
// (bytes 223..255) remain uncertified/open.

/// Private common projection of the Lower-memory page: the ten raw fields
/// this module's public accessors consume, projected from the
/// revision-specialised generated reader
/// (`generated::v{rev}::CmisLowerMemory`) selected by the frozen detect-once
/// `ctx.detected_rev`. The field names and Rust types are identical across
/// all six admitted revision readers — implementation/type compatibility,
/// NOT a claim of identical protocol semantics across revisions.
#[derive(Debug)]
struct LowerCommon {
    sff8024_identifier: crate::generated::Sff8024Identifier,
    cmis_revision: u8,
    memory_model: bool,
    temp_mon_value: i16,
    vcc_mon_voltage: u16,
    interrupt_deasserted: bool,
    temp_mon_high_alarm_flag: bool,
    temp_mon_low_alarm_flag: bool,
    vcc_mon_high_alarm_flag: bool,
    vcc_mon_low_alarm_flag: bool,
}

/// Project the ten Lower-memory fields this module actually consumes from
/// a revision-specialised generated `CmisLowerMemory` page. `$page` names
/// the concrete generated type (e.g. `generated::v4_0::CmisLowerMemory`);
/// field access stays compiler-checked against that type — no offsets,
/// masks, scaling or semantic remaps are duplicated here. The `cfg(test)`
/// statement records the actually decoded page's compiler-derived type at
/// this real boundary for the dispatch tests; release builds carry no
/// witness.
macro_rules! project_lower {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        record_decoded_lower_type(std::any::type_name_of_val(&page));
        LowerCommon {
            sff8024_identifier: page.sff8024_identifier,
            cmis_revision: page.cmis_revision,
            memory_model: page.memory_model,
            temp_mon_value: page.temp_mon_value,
            vcc_mon_voltage: page.vcc_mon_voltage,
            interrupt_deasserted: page.interrupt_deasserted,
            temp_mon_high_alarm_flag: page.temp_mon_high_alarm_flag,
            temp_mon_low_alarm_flag: page.temp_mon_low_alarm_flag,
            vcc_mon_high_alarm_flag: page.vcc_mon_high_alarm_flag,
            vcc_mon_low_alarm_flag: page.vcc_mon_low_alarm_flag,
        }
    }};
}

/// Private common projection of Page 00h (Administrative): the nine raw
/// identity fields consumed by [`Module::vendor_info`], projected from the
/// revision-specialised generated reader
/// (`generated::v{rev}::CmisPage00hAdministrative`) selected by the frozen
/// detect-once `ctx.detected_rev`. The field names and Rust types are
/// identical across all six admitted revision readers. Non-identity Page00
/// fields and legacy 128-byte upper-read acquisition collateral
/// (bytes 223..255) remain uncertified/open; `read_lower`, `read_page_11h`
/// and `tx_bias_multiplier` dispatch the per-revision readers the same way
/// (see the module docs above).
#[derive(Debug)]
struct Page00Common {
    vendor_name: [u8; 16],
    vendor_oui: [u8; 3],
    vendor_pn: [u8; 16],
    vendor_rev: [u8; 2],
    vendor_sn: [u8; 16],
    date_code_year: [u8; 2],
    date_code_month: [u8; 2],
    date_code_day_of_month: [u8; 2],
    date_code_lot_code: [u8; 2],
}

/// Project the nine Page 00h identity fields this module actually consumes
/// from a revision-specialised generated `CmisPage00hAdministrative` page.
/// `$page` names the concrete generated type (e.g.
/// `generated::v4_0::CmisPage00hAdministrative`); field access stays
/// compiler-checked against that type — no offsets, masks, or semantic
/// remaps are duplicated here. The `cfg(test)` statement records the
/// actually decoded page's compiler-derived type at this real boundary for
/// the dispatch tests; release builds carry no witness.
macro_rules! project_page_00 {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        record_decoded_page00_type(std::any::type_name_of_val(&page));
        Page00Common {
            vendor_name: page.vendor_name,
            vendor_oui: page.vendor_oui,
            vendor_pn: page.vendor_pn,
            vendor_rev: page.vendor_rev,
            vendor_sn: page.vendor_sn,
            date_code_year: page.date_code_year,
            date_code_month: page.date_code_month,
            date_code_day_of_month: page.date_code_day_of_month,
            date_code_lot_code: page.date_code_lot_code,
        }
    }};
}

// cfg(test) observer for the Lower dispatch tests: records the
// compiler-derived type name of the ACTUAL Lower page value decoded on the
// real `read_lower` path, so tests compare compiler-derived types rather
// than hand-written labels. Compiled out of release builds entirely.
#[cfg(test)]
thread_local! {
    static DECODED_LOWER_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn record_decoded_lower_type(ty: &'static str) {
    DECODED_LOWER_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
fn take_decoded_lower_types() -> Vec<&'static str> {
    DECODED_LOWER_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

// cfg(test) observer for the Page 00h dispatch tests: records the
// compiler-derived type name of the ACTUAL Page 00h value decoded on the
// real `read_page_00h` path, so tests compare compiler-derived types rather
// than hand-written labels. Compiled out of release builds entirely.
#[cfg(test)]
thread_local! {
    static DECODED_PAGE00_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn record_decoded_page00_type(ty: &'static str) {
    DECODED_PAGE00_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
fn take_decoded_page00_types() -> Vec<&'static str> {
    DECODED_PAGE00_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Private common projection of Page 11h (Lane Status): the eighteen raw
/// per-lane fields consumed by [`Module::lanes`] and [`Module::flags`],
/// projected from the revision-specialised generated reader
/// (`generated::v{rev}::CmisPage11hLaneStatus`) selected by the frozen
/// detect-once `ctx.detected_rev`. The field names and Rust types are
/// identical across all seven admitted revision readers —
/// implementation/type compatibility, NOT a claim of identical protocol
/// semantics across revisions: the revision window differences (e.g.
/// `dp_state_host_lane` raw 5h..7h reserved at 3.0 and named since 4.0;
/// the output-status registers and `dp_init_pending_lane` introduced in
/// 5.0) live in the per-revision readers, and this projection deliberately
/// re-derives none of them — it carries only fields present in all seven.
/// `failure_flag_tx` (Table 8-87) and `los_flag_rx` (Table 8-88) belong to
/// that carried set: per-lane latched clear-on-read status flags present
/// in every admitted revision's reader.
#[derive(Debug)]
struct Page11Common {
    dp_state_host_lane: [crate::generated::CmisDataPathState; 8],
    los_flag_tx: [bool; 8],
    cdr_lol_flag_tx: [bool; 8],
    adaptive_input_eq_fail_flag_tx: [bool; 8],
    cdr_lol_flag_rx: [bool; 8],
    optical_power_tx: [u16; 8],
    optical_power_rx: [u16; 8],
    laser_bias_tx: [u16; 8],
    optical_power_high_alarm_flag_tx: [bool; 8],
    optical_power_low_alarm_flag_tx: [bool; 8],
    optical_power_high_warning_flag_tx: [bool; 8],
    optical_power_low_warning_flag_tx: [bool; 8],
    optical_power_high_alarm_flag_rx: [bool; 8],
    optical_power_low_alarm_flag_rx: [bool; 8],
    optical_power_high_warning_flag_rx: [bool; 8],
    optical_power_low_warning_flag_rx: [bool; 8],
    laser_bias_high_alarm_flag_tx: [bool; 8],
    laser_bias_low_alarm_flag_tx: [bool; 8],
    laser_bias_high_warning_flag_tx: [bool; 8],
    laser_bias_low_warning_flag_tx: [bool; 8],
    failure_flag_tx: [bool; 8],
    los_flag_rx: [bool; 8],
}

/// Project the twenty-two Page 11h fields this module actually consumes from
/// a revision-specialised generated `CmisPage11hLaneStatus` page. `$page`
/// names the concrete generated type (e.g.
/// `generated::v4_0::CmisPage11hLaneStatus`); field access stays
/// compiler-checked against that type — no offsets, masks, scaling or
/// semantic remaps are duplicated here. The `cfg(test)` statement records
/// the actually decoded page's compiler-derived type at this real boundary
/// for the dispatch tests; release builds carry no witness.
macro_rules! project_page_11 {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        record_decoded_page11_type(std::any::type_name_of_val(&page));
        Page11Common {
            dp_state_host_lane: page.dp_state_host_lane,
            los_flag_tx: page.los_flag_tx,
            cdr_lol_flag_tx: page.cdr_lol_flag_tx,
            adaptive_input_eq_fail_flag_tx: page.adaptive_input_eq_fail_flag_tx,
            cdr_lol_flag_rx: page.cdr_lol_flag_rx,
            optical_power_tx: page.optical_power_tx,
            optical_power_rx: page.optical_power_rx,
            laser_bias_tx: page.laser_bias_tx,
            optical_power_high_alarm_flag_tx: page.optical_power_high_alarm_flag_tx,
            optical_power_low_alarm_flag_tx: page.optical_power_low_alarm_flag_tx,
            optical_power_high_warning_flag_tx: page.optical_power_high_warning_flag_tx,
            optical_power_low_warning_flag_tx: page.optical_power_low_warning_flag_tx,
            optical_power_high_alarm_flag_rx: page.optical_power_high_alarm_flag_rx,
            optical_power_low_alarm_flag_rx: page.optical_power_low_alarm_flag_rx,
            optical_power_high_warning_flag_rx: page.optical_power_high_warning_flag_rx,
            optical_power_low_warning_flag_rx: page.optical_power_low_warning_flag_rx,
            laser_bias_high_alarm_flag_tx: page.laser_bias_high_alarm_flag_tx,
            laser_bias_low_alarm_flag_tx: page.laser_bias_low_alarm_flag_tx,
            laser_bias_high_warning_flag_tx: page.laser_bias_high_warning_flag_tx,
            laser_bias_low_warning_flag_tx: page.laser_bias_low_warning_flag_tx,
            failure_flag_tx: page.failure_flag_tx,
            los_flag_rx: page.los_flag_rx,
        }
    }};
}

/// Project the single Page 01h (Advertising) field this module consumes —
/// `tx_bias_current_scaling_factor` — from a revision-specialised
/// generated `CmisPage01hAdvertising` page, so the schema-owned field
/// replaces the hand-rolled `(buf[160 - 128] >> 3) & 0x03` byte shift.
/// `$page` names the concrete generated type (e.g.
/// `generated::v4_0::CmisPage01hAdvertising`); field access stays
/// compiler-checked against that type. The `cfg(test)` statement records
/// the actually decoded page's compiler-derived type at this real boundary
/// for the dispatch tests; release builds carry no witness.
macro_rules! project_page_01_bias {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        record_decoded_page01_type(std::any::type_name_of_val(&page));
        page.tx_bias_current_scaling_factor
    }};
}

// cfg(test) observer for the Page 11h dispatch tests: records the
// compiler-derived type name of the ACTUAL Page 11h value decoded on the
// real `read_page_11h` path, so tests compare compiler-derived types rather
// than hand-written labels. Compiled out of release builds entirely.
#[cfg(test)]
thread_local! {
    static DECODED_PAGE11_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn record_decoded_page11_type(ty: &'static str) {
    DECODED_PAGE11_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
fn take_decoded_page11_types() -> Vec<&'static str> {
    DECODED_PAGE11_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

// cfg(test) observer for the Page 01h dispatch tests: records the
// compiler-derived type name of the ACTUAL Page 01h value decoded on the
// real `tx_bias_multiplier` path, so tests compare compiler-derived types
// rather than hand-written labels. Compiled out of release builds entirely.
#[cfg(test)]
thread_local! {
    static DECODED_PAGE01_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn record_decoded_page01_type(ty: &'static str) {
    DECODED_PAGE01_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
fn take_decoded_page01_types() -> Vec<&'static str> {
    DECODED_PAGE01_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Marker trait for transports usable by [`CmisModule`].
pub trait CmisTransport: Transport {}

// Orphan rules forbid `impl CmisTransport for BufTransport` inside
// openxcvr-core (neither the marker trait nor the type would be local to
// that crate), so the single-line marker impl lives here next to the trait.
impl CmisTransport for openxcvr_core::transport::BufTransport {}

/// A revision-frozen CMIS module handle over any [`CmisTransport`].
pub struct CmisModule<T: CmisTransport> {
    transport: RefCell<T>,
    ctx: Ctx,
}

impl<T: CmisTransport> CmisModule<T> {
    /// Build a module handle, reading the 128-byte Lower memory page once via
    /// `Transport::read`, then freezing the detected rev for the lifetime of
    /// the handle. Returns [`openxcvr_core::DetectError`] when the rev byte
    /// (Lower:1, CMIS 5.3 § 8.2.1) is missing or unknown; the `?` on the read
    /// propagates `ModuleError` into `DetectError::Transport` via `#[from]`.
    pub fn new(mut transport: T) -> Result<Self, openxcvr_core::DetectError> {
        let mut buf = [0u8; 128];
        transport.read(I2C_ADDR_MODULE, 0x00, 0, 0, &mut buf)?;
        let ctx = Ctx::from_lower_memory(&buf)?;
        Ok(Self {
            transport: RefCell::new(transport),
            ctx,
        })
    }

    /// The CMIS revision detected once at construction (Lower:1).
    pub fn detected_rev(&self) -> openxcvr_core::Rev {
        self.ctx.detected_rev
    }

    /// The frozen detect-once context.
    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    /// Send a typed CDB command, gated by the module's detected revision.
    /// Wraps [`Cdb::send_checked`] with the frozen `detected_rev`, so a command
    /// newer than the module is refused with
    /// [`CmisError::Cdb`]`(`[`crate::CdbError::UnsupportedInRev`]`)` before any
    /// I/O. The private `transport` field keeps this wrapper in `module.rs`.
    pub fn cdb_send<C>(
        &self,
        request: &C::Request,
        max_polls: u32,
    ) -> Result<C::Response, CmisError>
    where
        C: CdbCommand,
    {
        Ok(Cdb::send_checked::<C, T>(
            &mut *self.transport.borrow_mut(),
            request,
            self.ctx.detected_rev,
            max_polls,
        )?)
    }

    /// Private read of the 128-byte Lower page, decoded by the
    /// revision-specialised generated reader for the frozen
    /// `ctx.detected_rev` and projected onto the ten fields this module
    /// consumes. The decoder is resolved — or refused — BEFORE the data
    /// read, so an unadmitted context never touches the Transport; the
    /// constructor's detect-once gate remains the public unknown-revision
    /// boundary. Acquisition footprint is unchanged: exactly one 128-byte
    /// Lower read per call.
    fn read_lower(&self) -> Result<LowerCommon, ModuleError> {
        use Rev as R;
        // Resolve the revision's decoder before any I/O. Each arm is a
        // non-capturing closure over that revision's generated reader,
        // coerced to a fn pointer; no latest-revision fallback exists.
        let decode: fn(&[u8; 128]) -> LowerCommon = match self.ctx.detected_rev {
            R::CMIS_3_0 => |buf| project_lower!(buf, crate::generated::v3_0::CmisLowerMemory),
            R::CMIS_4_0 => |buf| project_lower!(buf, crate::generated::v4_0::CmisLowerMemory),
            R::CMIS_5_0 => |buf| project_lower!(buf, crate::generated::v5_0::CmisLowerMemory),
            R::CMIS_5_1 => |buf| project_lower!(buf, crate::generated::v5_1::CmisLowerMemory),
            R::CMIS_5_2 => |buf| project_lower!(buf, crate::generated::v5_2::CmisLowerMemory),
            R::CMIS_5_3 => |buf| project_lower!(buf, crate::generated::v5_3::CmisLowerMemory),
            R::CMIS_5_4 => |buf| project_lower!(buf, crate::generated::v5_4::CmisLowerMemory),
            _ => return Err(ModuleError::Unsupported),
        };
        let mut buf = [0u8; 128];
        self.transport
            .borrow_mut()
            .read(I2C_ADDR_MODULE, 0x00, 0, 0, &mut buf)?;
        Ok(decode(&buf))
    }

    /// Private read of the 128-byte upper Page 00h (Administrative),
    /// decoded by the revision-specialised generated reader for the frozen
    /// `ctx.detected_rev` and projected onto the nine identity fields
    /// consumed by [`Module::vendor_info`].
    ///
    /// The decoder is resolved — or refused — BEFORE the data read, so an
    /// unadmitted context never touches the Transport; the constructor's
    /// detect-once gate remains the public unknown-revision boundary.
    /// Acquisition footprint is unchanged: exactly one 128-byte upper read
    /// (slave address 0x50, page 0x00, bank 0, offset 128, len 128) per call.
    /// Non-identity Page00 fields and legacy 128-byte upper-read acquisition
    /// collateral (bytes 223..255) remain uncertified/open.
    fn read_page_00h(&self) -> Result<Page00Common, ModuleError> {
        use Rev as R;
        // Resolve the revision's decoder before any I/O. Each arm is a
        // non-capturing closure over that revision's generated reader,
        // coerced to a fn pointer; no latest-revision fallback exists.
        let decode: fn(&[u8; 128]) -> Page00Common = match self.ctx.detected_rev {
            R::CMIS_3_0 => {
                |buf| project_page_00!(buf, crate::generated::v3_0::CmisPage00hAdministrative)
            }
            R::CMIS_4_0 => {
                |buf| project_page_00!(buf, crate::generated::v4_0::CmisPage00hAdministrative)
            }
            R::CMIS_5_0 => {
                |buf| project_page_00!(buf, crate::generated::v5_0::CmisPage00hAdministrative)
            }
            R::CMIS_5_1 => {
                |buf| project_page_00!(buf, crate::generated::v5_1::CmisPage00hAdministrative)
            }
            R::CMIS_5_2 => {
                |buf| project_page_00!(buf, crate::generated::v5_2::CmisPage00hAdministrative)
            }
            R::CMIS_5_3 => {
                |buf| project_page_00!(buf, crate::generated::v5_3::CmisPage00hAdministrative)
            }
            R::CMIS_5_4 => {
                |buf| project_page_00!(buf, crate::generated::v5_4::CmisPage00hAdministrative)
            }
            _ => return Err(ModuleError::Unsupported),
        };
        let mut buf = [0u8; 128];
        self.transport
            .borrow_mut()
            .read(I2C_ADDR_MODULE, 0x00, 0, 128, &mut buf)?;
        Ok(decode(&buf))
    }

    /// Banked Page 11h (bank 0) — per-lane monitors, flags, and Data-Path
    /// state (CMIS 5.3 § 8.10, Table 8-89). Only meaningful on paged
    /// modules; flat-memory modules don't expose it. Page 11h is a
    /// versioned page (`dp_init_pending_lane`, the 11h:132-133/153
    /// output-status registers and bytes 235-239 gained `since: "5.0"`),
    /// so this reader selects the revision-specialised generated reader
    /// (`generated::v{rev}::CmisPage11hLaneStatus`) for the frozen
    /// `ctx.detected_rev` and projects the sixteen fields consumed by
    /// [`Module::lanes`] and [`Module::flags`] into [`Page11Common`] —
    /// never the rev-agnostic union view.
    ///
    /// The decoder is resolved — or refused — BEFORE the data read, so an
    /// unadmitted context never touches the Transport; the constructor's
    /// detect-once gate remains the public unknown-revision boundary.
    /// Acquisition footprint is unchanged: exactly one 128-byte upper read
    /// (page 0x11, bank 0, offset 128, len 128) per call. Physical
    /// 128-byte upper-page acquisition qualification remains separate from
    /// this decoder type dispatch.
    fn read_page_11h(&self) -> Result<Page11Common, ModuleError> {
        use Rev as R;
        // Resolve the revision's decoder before any I/O. Each arm is a
        // non-capturing closure over that revision's generated reader,
        // coerced to a fn pointer; no latest-revision fallback exists.
        let decode: fn(&[u8; 128]) -> Page11Common = match self.ctx.detected_rev {
            R::CMIS_3_0 => {
                |buf| project_page_11!(buf, crate::generated::v3_0::CmisPage11hLaneStatus)
            }
            R::CMIS_4_0 => {
                |buf| project_page_11!(buf, crate::generated::v4_0::CmisPage11hLaneStatus)
            }
            R::CMIS_5_0 => {
                |buf| project_page_11!(buf, crate::generated::v5_0::CmisPage11hLaneStatus)
            }
            R::CMIS_5_1 => {
                |buf| project_page_11!(buf, crate::generated::v5_1::CmisPage11hLaneStatus)
            }
            R::CMIS_5_2 => {
                |buf| project_page_11!(buf, crate::generated::v5_2::CmisPage11hLaneStatus)
            }
            R::CMIS_5_3 => {
                |buf| project_page_11!(buf, crate::generated::v5_3::CmisPage11hLaneStatus)
            }
            R::CMIS_5_4 => {
                |buf| project_page_11!(buf, crate::generated::v5_4::CmisPage11hLaneStatus)
            }
            _ => return Err(ModuleError::Unsupported),
        };
        let mut buf = [0u8; 128];
        self.transport
            .borrow_mut()
            .read(I2C_ADDR_MODULE, 0x11, 0, 128, &mut buf)?;
        Ok(decode(&buf))
    }

    /// TxBiasCurrentScalingFactor multiplier from Page 01h:160 bits 4-3
    /// (CMIS 5.3 Table 8-52): 00b→×1, 01b→×2, 10b→×4, 11b reserved. Applied
    /// to the LaserBiasTx LSB (2 µA) so reported bias is in mA. Page 01h
    /// is decoded by the revision-specialised generated reader
    /// (`generated::v{rev}::CmisPage01hAdvertising`) for the frozen
    /// `ctx.detected_rev`, so the schema-owned
    /// `tx_bias_current_scaling_factor` field replaces the hand-rolled
    /// byte shift. The reserved 0b11 code leaves the multiplier undefined,
    /// so it returns `None` (the caller renders `tx_bias_ma: null` and keeps
    /// the rest of the lane block) rather than failing the whole per-lane
    /// decode.
    ///
    /// The decoder is resolved — or refused — BEFORE the data read, so an
    /// unadmitted context never touches the Transport; the constructor's
    /// detect-once gate remains the public unknown-revision boundary.
    /// Acquisition footprint is unchanged: exactly one 128-byte upper read
    /// (page 0x01, bank 0, offset 128, len 128) per call. Physical
    /// 128-byte upper-page acquisition qualification remains separate from
    /// this decoder type dispatch.
    fn tx_bias_multiplier(&self) -> Result<Option<f32>, ModuleError> {
        use Rev as R;
        // Resolve the revision's decoder before any I/O. Each arm is a
        // non-capturing closure over that revision's generated reader,
        // coerced to a fn pointer; no latest-revision fallback exists.
        let decode_factor: fn(&[u8; 128]) -> u8 = match self.ctx.detected_rev {
            R::CMIS_3_0 => {
                |buf| project_page_01_bias!(buf, crate::generated::v3_0::CmisPage01hAdvertising)
            }
            R::CMIS_4_0 => {
                |buf| project_page_01_bias!(buf, crate::generated::v4_0::CmisPage01hAdvertising)
            }
            R::CMIS_5_0 => {
                |buf| project_page_01_bias!(buf, crate::generated::v5_0::CmisPage01hAdvertising)
            }
            R::CMIS_5_1 => {
                |buf| project_page_01_bias!(buf, crate::generated::v5_1::CmisPage01hAdvertising)
            }
            R::CMIS_5_2 => {
                |buf| project_page_01_bias!(buf, crate::generated::v5_2::CmisPage01hAdvertising)
            }
            R::CMIS_5_3 => {
                |buf| project_page_01_bias!(buf, crate::generated::v5_3::CmisPage01hAdvertising)
            }
            R::CMIS_5_4 => {
                |buf| project_page_01_bias!(buf, crate::generated::v5_4::CmisPage01hAdvertising)
            }
            _ => return Err(ModuleError::Unsupported),
        };
        let mut buf = [0u8; 128];
        self.transport
            .borrow_mut()
            .read(I2C_ADDR_MODULE, 0x01, 0, 128, &mut buf)?;
        Ok(match decode_factor(&buf) {
            0 => Some(1.0),
            1 => Some(2.0),
            2 => Some(4.0),
            _ => None,
        })
    }

    /// CMIS revision compliance byte (lower memory offset 1).
    /// High nibble = major version, low nibble = minor (e.g. 0x52 = CMIS 5.2).
    pub fn cmis_revision(&self) -> Result<u8, ModuleError> {
        Ok(self.read_lower()?.cmis_revision)
    }
}

/// Buffer-backed convenience constructor for callers that already have a
/// complete EEPROM dump in memory (unit tests, offline fixture decode).
/// sysfs/HIL callers continue using `new` with their own transport.
impl CmisModule<openxcvr_core::transport::BufTransport> {
    /// Build a module handle from a complete 128-byte lower-memory dump.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, openxcvr_core::DetectError> {
        Self::new(openxcvr_core::transport::BufTransport::from_slice(buf))
    }
}

impl<T: CmisTransport> Module for CmisModule<T> {
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
        let p = self.read_page_00h()?;
        let mut date_bytes = [0u8; 8];
        date_bytes[0..2].copy_from_slice(&p.date_code_year);
        date_bytes[2..4].copy_from_slice(&p.date_code_month);
        date_bytes[4..6].copy_from_slice(&p.date_code_day_of_month);
        date_bytes[6..8].copy_from_slice(&p.date_code_lot_code);
        Ok(VendorInfo {
            name: ascii_trim(&p.vendor_name),
            oui: p.vendor_oui,
            part_number: ascii_trim(&p.vendor_pn),
            rev: ascii_trim(&p.vendor_rev),
            serial_number: ascii_trim(&p.vendor_sn),
            date_code: ascii_trim(&date_bytes),
            // Byte 0 is not part of this read; see VendorInfo::identifier.
            identifier: None,
            // The ctx-detected revision is exposed via `detected_rev()`;
            // this construction site does not duplicate it (W1 cascade).
            spec_rev: None,
        })
    }

    fn identifier(&self) -> Result<u8, ModuleError> {
        Ok(self.read_lower()?.sff8024_identifier.repr())
    }

    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
        Ok(Some(self.read_lower()?.temp_mon_value as f32 / 256.0))
    }

    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
        Ok(Some(self.read_lower()?.vcc_mon_voltage as f32 * 0.0001))
    }

    fn lane_count(&self) -> Result<u8, ModuleError> {
        // Page 11h reports a full 8-lane status block; flat-memory modules
        // (no banked upper memory, CMIS 5.3 § 8.2.2) expose no per-lane view.
        if self.read_lower()?.memory_model {
            return Err(ModuleError::Unsupported);
        }
        Ok(8)
    }

    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
        let lower = self.read_lower()?;
        if lower.memory_model {
            // Flat-memory: no banked Page 11h, so no per-lane status.
            return Err(ModuleError::Unsupported);
        }
        let p11 = self.read_page_11h()?;
        let mult = self.tx_bias_multiplier()?;
        // Bind the schema-owned DPSM name projection ONCE from the frozen
        // detect-once context, before iteration: `read_lower` above already
        // refused any unadmitted frozen rev, so a valid frozen context must
        // bind the generated factory — refusal here is unreachable, never a
        // permissive fallback. Packing is the single local operation on the
        // frozen rev's components (MajorRevision byte, Lower:1).
        let rev = self.detected_rev();
        let rev_byte = (rev.major << 4) | rev.minor;
        let project =
            crate::generated::CmisDataPathState::for_revision(rev_byte).unwrap_or_else(|| {
                unreachable!(
                    "frozen rev {rev:?} (byte {rev_byte:#04x}) must bind the \
                     schema-owned DPSM projection; read_lower refuses unadmitted \
                     contexts before this point"
                )
            });
        let lanes = (0..8)
            .map(|i| LaneStatus {
                index: (i + 1) as u8,
                // OpticalPower{Tx,Rx} LSB = 0.1 µW → mW (linear, NOT f16be);
                // LaserBiasTx LSB = 2 µA × multiplier → mA. CMIS 5.3 § 8.10.4.
                rx_power_mw: Some(round_to(p11.optical_power_rx[i] as f32 * 0.0001, 4)),
                tx_power_mw: Some(round_to(p11.optical_power_tx[i] as f32 * 0.0001, 4)),
                tx_bias_ma: mult.map(|m| round_to(p11.laser_bias_tx[i] as f32 * 0.002 * m, 3)),
                // Per-lane latched clear-on-read status flags (§ 8.10.3):
                // rx_los ← LOSFlagRx<lane> (Table 8-88), tx_fault ←
                // FailureFlagTx<lane> (Table 8-87). Both exist in every
                // admitted revision's generated reader, so they always
                // surface — never None on a paged module.
                rx_los: Some(p11.los_flag_rx[i]),
                // CMIS Page 11h has no TxDisable status bit (it is a
                // control bit on Page 10h), so tx_disable stays None.
                tx_disable: None,
                tx_fault: Some(p11.failure_flag_tx[i]),
                // Public dp_state label: the schema-owned projection bound
                // above is the single name authority for the frozen revision
                // — its literal string for named raws (including raw 0's
                // "Reserved0" display), and the existing `Unknown<n>`
                // formatting for unnamed raws (reserved ranges; pre-4.0
                // 5h-7h per CMIS 3.0 Table 66).
                dp_state: Some(match project(p11.dp_state_host_lane[i].repr()) {
                    Some(name) => name.to_string(),
                    None => crate::generated::CmisDataPathState::Unknown(
                        p11.dp_state_host_lane[i].repr(),
                    )
                    .name(),
                }),
                // Per-lane signal flags, the same Page 11h bytes `flags()`
                // already reads through the alarm/warning trees: latched
                // Tx LOS / Tx CDR LOL / Tx Adaptive Input EQ Fail, host
                // lane i (§ 8.10.3 Table 8-87), and latched Rx CDR LOL,
                // media lane i (§ 8.10.3 Table 8-88). Every generated
                // revision reader (v3_0…v5_4) decodes them, so every
                // admitted frozen revision surfaces them — same-index
                // host/media lanes, the existing `flags()` convention.
                tx_los: Some(p11.los_flag_tx[i]),
                tx_cdr_lol: Some(p11.cdr_lol_flag_tx[i]),
                rx_cdr_lol: Some(p11.cdr_lol_flag_rx[i]),
                tx_adaptive_eq_fault: Some(p11.adaptive_input_eq_fail_flag_tx[i]),
            })
            .collect();
        Ok(lanes)
    }

    fn flags(&self) -> Result<ModuleFlags, ModuleError> {
        let lower = self.read_lower()?;
        if lower.memory_model {
            // Flat-memory: the per-lane flag tree lives on Page 11h, which is
            // absent. Keep `flags` null at the JSON layer (parity contract).
            return Err(ModuleError::Unsupported);
        }
        let p11 = self.read_page_11h()?;
        // Remap CMIS source flags to the contract's per-lane keys: tx_power ←
        // OpticalPower*Tx, tx_bias ← LaserBias*Tx, rx_power ← OpticalPower*Rx
        // (LaserBias is Tx-only, so there are no rx_bias flags). Per.
        let lanes = (0..8)
            .map(|i| LaneFlags {
                tx_power_high_alarm: p11.optical_power_high_alarm_flag_tx[i],
                tx_power_low_alarm: p11.optical_power_low_alarm_flag_tx[i],
                tx_power_high_warning: p11.optical_power_high_warning_flag_tx[i],
                tx_power_low_warning: p11.optical_power_low_warning_flag_tx[i],
                tx_bias_high_alarm: p11.laser_bias_high_alarm_flag_tx[i],
                tx_bias_low_alarm: p11.laser_bias_low_alarm_flag_tx[i],
                tx_bias_high_warning: p11.laser_bias_high_warning_flag_tx[i],
                tx_bias_low_warning: p11.laser_bias_low_warning_flag_tx[i],
                rx_power_high_alarm: p11.optical_power_high_alarm_flag_rx[i],
                rx_power_low_alarm: p11.optical_power_low_alarm_flag_rx[i],
                rx_power_high_warning: p11.optical_power_high_warning_flag_rx[i],
                rx_power_low_warning: p11.optical_power_low_warning_flag_rx[i],
            })
            .collect();
        Ok(ModuleFlags {
            data_not_ready: None,
            interrupt_asserted: Some(!lower.interrupt_deasserted),
            temp_high_alarm: lower.temp_mon_high_alarm_flag,
            temp_low_alarm: lower.temp_mon_low_alarm_flag,
            supply_high_alarm: lower.vcc_mon_high_alarm_flag,
            supply_low_alarm: lower.vcc_mon_low_alarm_flag,
            lanes,
        })
    }

    fn checksums(&self) -> Result<ChecksumStatus, ModuleError> {
        // CMIS has no identity-byte checksum (unlike SFF-8472/SFF-8636).
        Ok(ChecksumStatus::default())
    }
}

/// Round `x` to `places` decimal digits (matches the reference decoder's
/// `round(x, places)` and the C/Go aggregators, so the cross-binding JSON
/// agrees on the same decimal precision).
fn round_to(x: f32, places: u32) -> f32 {
    let factor = 10f32.powi(places as i32);
    (x * factor).round() / factor
}

fn ascii_trim(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0x00)
        .map(|i| i + 1)
        .unwrap_or(0);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

impl CmisModule<crate::CmisMmapTransport> {
    /// Attach the bank-0 lane/status capture to an offline module.
    pub fn with_page_11h(self, path: impl AsRef<std::path::Path>) -> Result<Self, ModuleError> {
        let transport = self
            .transport
            .into_inner()
            .with_page_11h(path)
            .map_err(|e| ModuleError::Io(IoError::with_source(e.to_string(), e)))?;
        Self::new(transport).map_err(ModuleError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Ctx;
    use openxcvr_core::{DetectError, Rev};
    use std::rc::Rc;

    /// One representative named Lower-memory fixture per admitted CMIS
    /// revision — the same corpus tests/dispatch_by_rev.rs pins the
    /// revision byte on. Synthetic fixtures are corpus parity evidence,
    /// NOT live-optic qualification; type/name parity is not physical
    /// semantics qualification.
    const REV_FIXTURES: [(&str, Rev); 7] = [
        ("synthetic-cmis-3-0-qsfp-dd", Rev::CMIS_3_0),
        ("fboss-cmis-4-0-qsfpdd-400g-dr4", Rev::CMIS_4_0),
        ("fboss-cmis-5-0-osfp-2x400g-fr4", Rev::CMIS_5_0),
        ("fboss-cmis-5-1-osfp-2x800g-dr4", Rev::CMIS_5_1),
        ("synthetic-cmis-5-2-qsfp-dd", Rev::CMIS_5_2),
        ("fboss-cmis-5-3-osfp-800g-zr", Rev::CMIS_5_3),
        ("synthetic-cmis-5-4-qsfp-dd-vdm", Rev::CMIS_5_4),
    ];

    pub(crate) fn fixture_lower(stem: &str) -> [u8; 128] {
        let path = format!("../../tests/fixtures/cmis/{stem}.lower.bin");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("could not read {path}: {e}"));
        bytes
            .try_into()
            .unwrap_or_else(|v: Vec<u8>| panic!("{path}: expected 128 bytes, got {}", v.len()))
    }

    /// Recorded Transport operation: (op, page, offset, length), shared so
    /// the log survives the transport being consumed by `CmisModule::new`.
    type OpLog = Rc<RefCell<Vec<(&'static str, u8, u8, usize)>>>;

    /// Deterministic supplied transport: a fixed 256-byte image (Lower + one
    /// upper page) with every Transport read/write recorded. Reads past the
    /// image are served as zeroes so upper-page consumers stay deterministic.
    struct CountingTransport {
        image: [u8; 256],
        log: OpLog,
    }

    impl CountingTransport {
        fn new(lower: [u8; 128]) -> (Self, OpLog) {
            let mut image = [0u8; 256];
            image[..128].copy_from_slice(&lower);
            let log = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    image,
                    log: Rc::clone(&log),
                },
                log,
            )
        }
    }

    impl Transport for CountingTransport {
        fn read(
            &mut self,
            _slave_addr: u8,
            page: u8,
            _bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            self.log
                .borrow_mut()
                .push(("read", page, offset, buf.len()));
            let start = if offset < 128 {
                offset as usize
            } else {
                128 + page as usize * 128 + (offset as usize - 128)
            };
            for (i, b) in buf.iter_mut().enumerate() {
                *b = self.image.get(start + i).copied().unwrap_or(0);
            }
            Ok(())
        }

        fn write(
            &mut self,
            _slave_addr: u8,
            page: u8,
            _bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            self.log
                .borrow_mut()
                .push(("write", page, offset, data.len()));
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }
    impl CmisTransport for CountingTransport {}

    #[derive(Debug, PartialEq, Eq, Clone)]
    enum SpyOp {
        Read {
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            len: usize,
        },
        Write {
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            len: usize,
        },
        Probe,
    }

    type SpyLog = Rc<RefCell<Vec<SpyOp>>>;

    /// Test-only dedicated spy wrapping CountingTransport to record full
    /// transport tuples (slave_addr, page, bank, offset, len) and writes/probes
    /// without disturbing existing CountingTransport/Lower test assertions.
    struct Page00SpyTransport {
        inner: CountingTransport,
        spy_log: SpyLog,
        shared_image: Rc<RefCell<[u8; 256]>>,
        fault_on_page00: bool,
    }

    impl Page00SpyTransport {
        fn new(lower: [u8; 128]) -> (Self, OpLog, SpyLog, Rc<RefCell<[u8; 256]>>) {
            let mut image = [0u8; 256];
            image[..128].copy_from_slice(&lower);
            Self::from_256(image)
        }

        fn from_256(image: [u8; 256]) -> (Self, OpLog, SpyLog, Rc<RefCell<[u8; 256]>>) {
            let shared_image = Rc::new(RefCell::new(image));
            let log = Rc::new(RefCell::new(Vec::new()));
            let inner = CountingTransport {
                image,
                log: Rc::clone(&log),
            };
            let spy_log = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    inner,
                    spy_log: Rc::clone(&spy_log),
                    shared_image: Rc::clone(&shared_image),
                    fault_on_page00: false,
                },
                log,
                spy_log,
                shared_image,
            )
        }
    }

    impl Transport for Page00SpyTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            self.spy_log.borrow_mut().push(SpyOp::Read {
                slave_addr,
                page,
                bank,
                offset,
                len: buf.len(),
            });
            if self.fault_on_page00 && page == 0x00 && offset == 128 {
                return Err(ModuleError::Io("injected page 00h failure".into()));
            }
            self.inner.image = *self.shared_image.borrow();
            self.inner.read(slave_addr, page, bank, offset, buf)
        }

        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            self.spy_log.borrow_mut().push(SpyOp::Write {
                slave_addr,
                page,
                bank,
                offset,
                len: data.len(),
            });
            self.inner.write(slave_addr, page, bank, offset, data)
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            self.spy_log.borrow_mut().push(SpyOp::Probe);
            self.inner.probe()
        }
    }
    impl CmisTransport for Page00SpyTransport {}

    fn synthetic_page00_identity_bytes() -> [u8; 128] {
        let mut buf = [0u8; 128];
        // buf[1..17]: vendor_name = "PETABIT-SCALE   " (16 bytes, ASCII space padded)
        buf[1..17].copy_from_slice(b"PETABIT-SCALE   ");
        // buf[17..20]: vendor_oui = [0x00, 0x1A, 0x2B]
        buf[17..20].copy_from_slice(&[0x00, 0x1A, 0x2B]);
        // buf[20..36]: vendor_pn = "PS-400G-DR4\0\0\0\0\0" (16 bytes, NUL padded)
        buf[20..36].copy_from_slice(b"PS-400G-DR4\0\0\0\0\0");
        // buf[36..38]: vendor_rev = "A1"
        buf[36..38].copy_from_slice(b"A1");
        // buf[38..54]: vendor_sn = "SN1234567890ABCD"
        buf[38..54].copy_from_slice(b"SN1234567890ABCD");
        // buf[54..56]: date_code_year = "26"
        buf[54..56].copy_from_slice(b"26");
        // buf[56..58]: date_code_month = "04"
        buf[56..58].copy_from_slice(b"04");
        // buf[58..60]: date_code_day_of_month = "15"
        buf[58..60].copy_from_slice(b"15");
        // buf[60..62]: date_code_lot_code = "L1"
        buf[60..62].copy_from_slice(b"L1");
        buf
    }

    fn counted_module(lower: [u8; 128]) -> (CmisModule<CountingTransport>, OpLog) {
        let (t, log) = CountingTransport::new(lower);
        let m = CmisModule::new(t).expect("CMIS detect");
        (m, log)
    }

    fn module_from_fixture(stem: &str) -> CmisModule<CountingTransport> {
        let (m, _log) = counted_module(fixture_lower(stem));
        m
    }

    /// The ten Lower fields this module consumes, compared field-by-field
    /// against the corresponding ACTUAL generated revision reader decoding
    /// the same bytes. Compiles against the union view (pre-dispatch) and the
    /// common projection (post-dispatch) alike — the field names and types
    /// are identical in both.
    macro_rules! assert_common_fields {
        ($lower:expr, $direct:expr) => {{
            let (l, d) = ($lower, $direct);
            assert_eq!(
                l.sff8024_identifier, d.sff8024_identifier,
                "sff8024_identifier"
            );
            assert_eq!(l.cmis_revision, d.cmis_revision, "cmis_revision");
            assert_eq!(l.memory_model, d.memory_model, "memory_model");
            assert_eq!(
                l.interrupt_deasserted, d.interrupt_deasserted,
                "interrupt_deasserted"
            );
            assert_eq!(
                l.temp_mon_high_alarm_flag, d.temp_mon_high_alarm_flag,
                "temp_mon_high_alarm_flag"
            );
            assert_eq!(
                l.temp_mon_low_alarm_flag, d.temp_mon_low_alarm_flag,
                "temp_mon_low_alarm_flag"
            );
            assert_eq!(
                l.vcc_mon_high_alarm_flag, d.vcc_mon_high_alarm_flag,
                "vcc_mon_high_alarm_flag"
            );
            assert_eq!(
                l.vcc_mon_low_alarm_flag, d.vcc_mon_low_alarm_flag,
                "vcc_mon_low_alarm_flag"
            );
            assert_eq!(l.temp_mon_value, d.temp_mon_value, "temp_mon_value");
            assert_eq!(l.vcc_mon_voltage, d.vcc_mon_voltage, "vcc_mon_voltage");
        }};
    }

    /// Public single-read outputs against the corresponding generated
    /// revision reader's values, conversion arithmetic unchanged.
    macro_rules! assert_public_matches_reader {
        ($m:expr, $direct:expr, $stem:expr) => {{
            let d = $direct;
            assert_eq!(
                $m.identifier().unwrap(),
                d.sff8024_identifier.repr(),
                "{}: identifier",
                $stem
            );
            assert_eq!(
                $m.cmis_revision().unwrap(),
                d.cmis_revision,
                "{}: cmis_revision",
                $stem
            );
            assert_eq!(
                $m.temperature_celsius().unwrap(),
                Some(d.temp_mon_value as f32 / 256.0),
                "{}: temperature_celsius",
                $stem
            );
            assert_eq!(
                $m.supply_voltage_v().unwrap(),
                Some(d.vcc_mon_voltage as f32 * 0.0001),
                "{}: supply_voltage_v",
                $stem
            );
        }};
    }

    /// Dispatch oracle: the type recorded at the real `read_lower` boundary
    /// must be the frozen revision's generated reader type, on BOTH sides
    /// compiler-derived (`type_name_of_val` of the actually decoded page vs
    /// `type_name` of the concrete generated type). Not a rev label, not a
    /// hand-written string.
    #[test]
    fn read_lower_dispatches_the_revision_specialised_generated_decoder() {
        for (stem, rev) in REV_FIXTURES {
            let m = module_from_fixture(stem);
            assert_eq!(m.detected_rev(), rev, "{stem}: frozen rev from Lower:1");
            let _ = take_decoded_lower_types(); // isolate this read's record
            m.read_lower()
                .unwrap_or_else(|e| panic!("{stem}: read_lower: {e:?}"));
            let recorded = take_decoded_lower_types();
            assert_eq!(
                recorded.len(),
                1,
                "{stem}: exactly one Lower decode per read_lower, got {recorded:?}"
            );
            let expected = match rev {
                Rev::CMIS_3_0 => std::any::type_name::<crate::generated::v3_0::CmisLowerMemory>(),
                Rev::CMIS_4_0 => std::any::type_name::<crate::generated::v4_0::CmisLowerMemory>(),
                Rev::CMIS_5_0 => std::any::type_name::<crate::generated::v5_0::CmisLowerMemory>(),
                Rev::CMIS_5_1 => std::any::type_name::<crate::generated::v5_1::CmisLowerMemory>(),
                Rev::CMIS_5_2 => std::any::type_name::<crate::generated::v5_2::CmisLowerMemory>(),
                Rev::CMIS_5_3 => std::any::type_name::<crate::generated::v5_3::CmisLowerMemory>(),
                Rev::CMIS_5_4 => std::any::type_name::<crate::generated::v5_4::CmisLowerMemory>(),
                other => panic!("{stem}: unexpected rev {other:?}"),
            };
            assert_eq!(
                recorded[0], expected,
                "{stem}: read_lower selected the wrong generated decoder"
            );
        }
    }

    /// Defensive inner boundary: a ctx frozen to a rev with no generated
    /// reader (here 5.5, a not-yet-specified revision whose rev byte is
    /// 0x55) is refused BEFORE the data read — zero Transport I/O. The
    /// public constructor remains the unknown-revision gate; this covers a
    /// hand-built foreign ctx reaching the private boundary anyway.
    #[test]
    fn read_lower_refuses_an_unadmitted_frozen_rev_before_the_data_read() {
        let (t, log) = CountingTransport::new([0u8; 128]);
        let m = CmisModule {
            transport: RefCell::new(t),
            ctx: Ctx {
                detected_rev: Rev::parse_cmis("5.5").expect("parse 5.5"),
            },
        };
        match m.read_lower() {
            Err(ModuleError::Unsupported) => {}
            other => panic!("expected Err(ModuleError::Unsupported), got {other:?}"),
        }
        assert!(
            log.borrow().is_empty(),
            "decoder refusal must precede the data read; Transport saw {:?}",
            log.borrow()
        );
    }

    /// Unknown rev byte 0x55 (CMIS 5.5): construction performs exactly the
    /// required detection read (one 128-byte Lower read) and refuses with no
    /// later reads and no writes. Not "before any I/O" — the detection read
    /// happens.
    #[test]
    fn construction_with_rev_byte_55h_stops_after_the_detection_read() {
        let mut lower = [0u8; 128];
        lower[0] = 0x18;
        lower[1] = 0x55;
        let (t, log) = CountingTransport::new(lower);
        match CmisModule::new(t) {
            Err(DetectError::UnsupportedRev {
                detected_byte: 0x55,
                ..
            }) => {}
            Err(other) => panic!("expected UnsupportedRev(0x55), got {other:?}"),
            Ok(_) => panic!("expected UnsupportedRev(0x55), but construction succeeded"),
        }
        assert_eq!(
            *log.borrow(),
            vec![("read", 0x00, 0, 128)],
            "exactly the detection read; nothing after the refusal"
        );
    }

    /// Field-for-field agreement between `read_lower`'s ten consumed fields
    /// and the corresponding revision's ACTUAL generated reader decoding the
    /// same fixture bytes.
    #[test]
    fn read_lower_common_fields_match_the_revision_generated_reader() {
        for (stem, rev) in REV_FIXTURES {
            let bytes = fixture_lower(stem);
            let m = module_from_fixture(stem);
            let lower = m
                .read_lower()
                .unwrap_or_else(|e| panic!("{stem}: read_lower: {e:?}"));
            match rev {
                Rev::CMIS_3_0 => assert_common_fields!(
                    lower,
                    crate::generated::v3_0::CmisLowerMemory::decode(&bytes)
                ),
                Rev::CMIS_4_0 => assert_common_fields!(
                    lower,
                    crate::generated::v4_0::CmisLowerMemory::decode(&bytes)
                ),
                Rev::CMIS_5_0 => assert_common_fields!(
                    lower,
                    crate::generated::v5_0::CmisLowerMemory::decode(&bytes)
                ),
                Rev::CMIS_5_1 => assert_common_fields!(
                    lower,
                    crate::generated::v5_1::CmisLowerMemory::decode(&bytes)
                ),
                Rev::CMIS_5_2 => assert_common_fields!(
                    lower,
                    crate::generated::v5_2::CmisLowerMemory::decode(&bytes)
                ),
                Rev::CMIS_5_3 => assert_common_fields!(
                    lower,
                    crate::generated::v5_3::CmisLowerMemory::decode(&bytes)
                ),
                Rev::CMIS_5_4 => assert_common_fields!(
                    lower,
                    crate::generated::v5_4::CmisLowerMemory::decode(&bytes)
                ),
                other => panic!("{stem}: unexpected rev {other:?}"),
            }
        }
    }

    /// Public single-read outputs equal the matching revision reader's
    /// values (conversion arithmetic unchanged), with the one-read footprint
    /// asserted in the same pass.
    #[test]
    fn lower_public_outputs_match_the_revision_generated_reader() {
        for (stem, rev) in REV_FIXTURES {
            let bytes = fixture_lower(stem);
            let (m, log) = counted_module(bytes);
            log.borrow_mut().clear(); // drop the constructor's detection read
            match rev {
                Rev::CMIS_3_0 => assert_public_matches_reader!(
                    m,
                    crate::generated::v3_0::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                Rev::CMIS_4_0 => assert_public_matches_reader!(
                    m,
                    crate::generated::v4_0::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                Rev::CMIS_5_0 => assert_public_matches_reader!(
                    m,
                    crate::generated::v5_0::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                Rev::CMIS_5_1 => assert_public_matches_reader!(
                    m,
                    crate::generated::v5_1::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                Rev::CMIS_5_2 => assert_public_matches_reader!(
                    m,
                    crate::generated::v5_2::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                Rev::CMIS_5_3 => assert_public_matches_reader!(
                    m,
                    crate::generated::v5_3::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                Rev::CMIS_5_4 => assert_public_matches_reader!(
                    m,
                    crate::generated::v5_4::CmisLowerMemory::decode(&bytes),
                    stem
                ),
                other => panic!("{stem}: unexpected rev {other:?}"),
            }
            assert_eq!(
                *log.borrow(),
                vec![("read", 0x00, 0, 128); 4],
                "{stem}: exactly one 128-byte Lower read per public call"
            );
        }
    }

    /// Acquisition footprint preserved: each Lower-reading public method
    /// performs exactly one 128-byte Lower read (page 0x00, offset 0) and no
    /// writes, on every admitted revision.
    #[test]
    fn lower_reading_public_methods_keep_one_128_byte_lower_read_per_call() {
        for (stem, _rev) in REV_FIXTURES {
            let (m, log) = counted_module(fixture_lower(stem));
            log.borrow_mut().clear(); // drop the constructor's detection read
            let _ = m.cmis_revision();
            assert_eq!(
                *log.borrow(),
                vec![("read", 0x00, 0, 128)],
                "{stem}: cmis_revision"
            );
            log.borrow_mut().clear();
            let _ = m.identifier();
            assert_eq!(
                *log.borrow(),
                vec![("read", 0x00, 0, 128)],
                "{stem}: identifier"
            );
            log.borrow_mut().clear();
            let _ = m.temperature_celsius();
            assert_eq!(
                *log.borrow(),
                vec![("read", 0x00, 0, 128)],
                "{stem}: temperature_celsius"
            );
            log.borrow_mut().clear();
            let _ = m.supply_voltage_v();
            assert_eq!(
                *log.borrow(),
                vec![("read", 0x00, 0, 128)],
                "{stem}: supply_voltage_v"
            );
        }
    }

    /// CMIS 4.0: byte 2 bit 6 is reserved in 4.0 (`stepped_config_only`
    /// exists only since 5.0), so toggling it must preserve every public
    /// value. PRESERVATION coverage — expected to pass on the pre-dispatch
    /// baseline too; it is not a newly fixed public-output bug.
    #[test]
    fn cmis_4_0_reserved_byte2_bit6_toggling_preserves_public_values() {
        let base = fixture_lower("fboss-cmis-4-0-qsfpdd-400g-dr4");
        let mut toggled = base;
        toggled[2] ^= 0x40;

        let (m0, _l0) = counted_module(base);
        let (m1, _l1) = counted_module(toggled);
        assert_eq!(
            m0.identifier().unwrap(),
            m1.identifier().unwrap(),
            "identifier"
        );
        assert_eq!(
            m0.cmis_revision().unwrap(),
            m1.cmis_revision().unwrap(),
            "cmis_revision"
        );
        assert_eq!(
            m0.temperature_celsius().unwrap(),
            m1.temperature_celsius().unwrap(),
            "temperature_celsius"
        );
        assert_eq!(
            m0.supply_voltage_v().unwrap(),
            m1.supply_voltage_v().unwrap(),
            "supply_voltage_v"
        );
        assert_eq!(
            m0.lane_count().unwrap(),
            m1.lane_count().unwrap(),
            "lane_count"
        );
        // Both transports zero the upper pages, so per-lane flags agree
        // trivially; assert the lower-derived module flags explicitly.
        let f0 = m0.flags().unwrap();
        let f1 = m1.flags().unwrap();
        assert_eq!(f0.data_not_ready, f1.data_not_ready);
        assert_eq!(f0.interrupt_asserted, f1.interrupt_asserted);
        assert_eq!(f0.temp_high_alarm, f1.temp_high_alarm);
        assert_eq!(f0.temp_low_alarm, f1.temp_low_alarm);
        assert_eq!(f0.supply_high_alarm, f1.supply_high_alarm);
        assert_eq!(f0.supply_low_alarm, f1.supply_low_alarm);
    }

    /// CMIS 3.0 Table 66 reserves raw DP-state nibbles 0h and 5h..Fh; only
    /// 1h Deactivated, 2h Init, 3h Deinit, 4h Activated are named. On a
    /// module whose frozen detected rev is CMIS 3.0, the public per-lane
    /// `dp_state` label must therefore render raw 5/6/7 with the
    /// version-neutral `Unknown<n>` fallback — the same reserved-state
    /// interpretation the revision-qualified public adapter
    /// `DpState::from_nibble_for_rev` yields for pre-4.0 modules (it
    /// returns semantic `DpState` variants, NOT strings) — NOT with the
    /// CMIS 4.0+ names (DPTxTurnOn/DPTxTurnOff/DPInitialized) that the
    /// unversioned generated Page 11h enum ascribes to those nibbles.
    ///
    /// Matrix: every raw nibble 0..=15 × all 8 lanes against a literal
    /// want-table (Table 66 names for 1..4, the established `Reserved0`
    /// display for 0, and the `Unknown<n>` fallback for 5..15). Catches:
    /// `lanes()` rendering a 3.0 module's reserved nibbles 5h/6h/7h with
    /// latest-revision generated names instead of the `Unknown<n>`
    /// fallback (or `lanes()`/detect-once no longer succeeding at all).
    ///
    /// Input provenance: Lower 128 bytes are the synthetic CMIS 3.0 fixture
    /// (header `18 30 00 00`: byte 1 = 0x30, byte 2 bit 7 = 0 → paged
    /// memory), served through the real `BufTransport` constructor path; the
    /// first four Page 11h bytes carry each raw state duplicated across
    /// both nibbles (`raw | raw << 4`), so all 8 lanes hold that state
    /// without assuming a nibble order. Page 01h and all other bytes are
    /// zeros — a deterministic test input, not a claim about a physical
    /// module. Expected values are hand-derived literals (Table 66 + the
    /// existing `Unknown<n>` public fallback), never a helper under test.
    #[test]
    fn cmis_3_0_lanes_render_reserved_dp_states_as_unknown_fallback() {
        // want[raw] = expected public dp_state label for that raw nibble on
        // CMIS 3.0: 0h reserved display, 1h-4h Table 66 names, 5h-Fh
        // reserved → Unknown<n> fallback. Hand-derived literals.
        let want = [
            "Reserved0",
            "DpDeactivated",
            "DpInit",
            "DpDeinit",
            "DpActivated",
            "Unknown5",
            "Unknown6",
            "Unknown7",
            "Unknown8",
            "Unknown9",
            "Unknown10",
            "Unknown11",
            "Unknown12",
            "Unknown13",
            "Unknown14",
            "Unknown15",
        ];
        // Collect across all 16 raw states x 8 lanes so the single final
        // assertion observes EVERY wrong rendering, not just the first
        // failing lane.
        let mut mismatches = Vec::new();
        for raw in 0u8..=15 {
            // Both nibbles of the four Page 11h state bytes carry `raw`
            // (no nibble-order assumption).
            let state_bytes = raw | (raw << 4);
            // Lower (128) + Upper pages 00h..11h in BufTransport's flat
            // layout: page 0xNN lives at 128 + NN*128, so 2432 bytes cover
            // the Lower read, Page 11h (dp-state bytes) and Page 01h (bias
            // multiplier — zeros → ×1) that `lanes()` reads.
            let mut image = vec![0u8; 128 + 0x12 * 128];
            image[..128].copy_from_slice(&fixture_lower("synthetic-cmis-3-0-qsfp-dd"));
            let page_11h = 128 + 0x11 * 128;
            image[page_11h..page_11h + 4].fill(state_bytes);

            let m = CmisModule::from_bytes(&image).expect("CMIS 3.0 detect");
            assert_eq!(
                m.detected_rev(),
                Rev::CMIS_3_0,
                "constructor must freeze the detect-once rev from Lower:1 = 0x30"
            );
            let lanes = m.lanes().unwrap_or_else(|e| panic!("lanes(): {e:?}"));
            assert_eq!(lanes.len(), 8, "Page 11h reports a full 8-lane block");
            let expected = want[raw as usize];
            for (i, lane) in lanes.iter().enumerate() {
                let actual = lane.dp_state.as_deref();
                if actual != Some(expected) {
                    mismatches.push(format!(
                        "raw {raw:#04x} lane {}: actual {actual:?}, expected {expected:?} \
                         (per the CMIS 3.0 Table 66 want-matrix for this raw nibble)",
                        i + 1,
                    ));
                }
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} dp_state mismatch(es) across raw 0..=15 x 8 lanes:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
    }

    /// Preservation matrix for CMIS 4.0+ modules: `from_nibble_for_rev`
    /// selects the CMIS 4.0 Table 8-58 interpretation for every frozen rev
    /// ≥ 4.0, so the public per-lane `dp_state` labels must stay exactly
    /// the unversioned generated Page 11h enum's current names for ALL
    /// 16 raw nibbles × 8 lanes — literal `Reserved0` for 0h, the
    /// Table 8-58 names for 1h..7h (including DPTxTurnOn/DPTxTurnOff/
    /// DPInitialized for raw 5/6/7) and `Unknown<n>` for 8h..Fh. The
    /// pre-4.0 reserved-nibble fix must not disturb any of these labels.
    ///
    /// Uses the existing `REV_FIXTURES` rev case list (4.0/5.0/5.1/5.2/5.3
    /// lower fixtures, the 3.0 row is covered by the matrix test above)
    /// with the real `BufTransport`/`from_bytes` constructor path and the
    /// same both-nibble-duplicated Page 11h state bytes; Page 01h and the
    /// rest of the image are zeros. Those upper pages are synthetic
    /// compatibility inputs on top of the checked-in lower headers — not
    /// new protocol or hardware qualification. Expectations are
    /// hand-derived literals, never computed via `Dpsm` or generated name
    /// helpers under test.
    #[test]
    fn cmis_4_0_on_lanes_preserve_current_dp_state_names() {
        // want[raw] = current public dp_state label for that raw nibble on
        // any module frozen at rev >= 4.0 (generated Page 11h enum names).
        let want = [
            "Reserved0",
            "DpDeactivated",
            "DpInit",
            "DpDeinit",
            "DpActivated",
            "DpTxTurnOn",
            "DpTxTurnOff",
            "DpInitialized",
            "Unknown8",
            "Unknown9",
            "Unknown10",
            "Unknown11",
            "Unknown12",
            "Unknown13",
            "Unknown14",
            "Unknown15",
        ];
        let mut mismatches = Vec::new();
        for (stem, rev) in REV_FIXTURES {
            if rev < Rev::CMIS_4_0 {
                continue; // pre-4.0 want-matrix covered by the test above
            }
            for raw in 0u8..=15 {
                // Both nibbles of the four Page 11h state bytes carry `raw`
                // (no nibble-order assumption); flat layout as above.
                let state_bytes = raw | (raw << 4);
                let mut image = vec![0u8; 128 + 0x12 * 128];
                image[..128].copy_from_slice(&fixture_lower(stem));
                let page_11h = 128 + 0x11 * 128;
                image[page_11h..page_11h + 4].fill(state_bytes);

                let m = CmisModule::from_bytes(&image)
                    .unwrap_or_else(|e| panic!("{stem}: CMIS detect: {e:?}"));
                assert_eq!(
                    m.detected_rev(),
                    rev,
                    "{stem}: constructor must freeze the detect-once rev from Lower:1"
                );
                let lanes = m
                    .lanes()
                    .unwrap_or_else(|e| panic!("{stem}: lanes(): {e:?}"));
                assert_eq!(
                    lanes.len(),
                    8,
                    "{stem}: Page 11h reports a full 8-lane block"
                );
                let expected = want[raw as usize];
                for (i, lane) in lanes.iter().enumerate() {
                    let actual = lane.dp_state.as_deref();
                    if actual != Some(expected) {
                        mismatches.push(format!(
                            "{stem} (rev {rev:?}) raw {raw:#04x} lane {}: actual \
                             {actual:?}, expected {expected:?} (current rev >= 4.0 label \
                             must be preserved)",
                            i + 1,
                        ));
                    }
                }
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} dp_state mismatch(es) across revs 4.0/5.0/5.1/5.2/5.3 x raw 0..=15 \
             x 8 lanes:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
    }

    /// Dispatch oracle: the type recorded at the real `read_page_00h` boundary
    /// on the public `vendor_info()` path must be the frozen revision's
    /// generated reader type across all 6 admitted revisions, compiler-derived
    /// on BOTH sides (`type_name_of_val` of the actually decoded page vs
    /// `type_name` of the concrete generated type).
    #[test]
    fn vendor_info_dispatches_the_revision_specialised_generated_decoder() {
        for (stem, rev) in REV_FIXTURES {
            let m = module_from_fixture(stem);
            assert_eq!(m.detected_rev(), rev, "{stem}: frozen rev from Lower:1");
            let _ = take_decoded_page00_types(); // isolate this read's record
            m.vendor_info()
                .unwrap_or_else(|e| panic!("{stem}: vendor_info: {e:?}"));
            let recorded = take_decoded_page00_types();
            assert_eq!(
                recorded.len(),
                1,
                "{stem}: exactly one Page 00h decode per vendor_info, got {recorded:?}"
            );
            let expected = match rev {
                Rev::CMIS_3_0 => {
                    std::any::type_name::<crate::generated::v3_0::CmisPage00hAdministrative>()
                }
                Rev::CMIS_4_0 => {
                    std::any::type_name::<crate::generated::v4_0::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_0 => {
                    std::any::type_name::<crate::generated::v5_0::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_1 => {
                    std::any::type_name::<crate::generated::v5_1::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_2 => {
                    std::any::type_name::<crate::generated::v5_2::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_3 => {
                    std::any::type_name::<crate::generated::v5_3::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_4 => {
                    std::any::type_name::<crate::generated::v5_4::CmisPage00hAdministrative>()
                }
                other => panic!("{stem}: unexpected rev {other:?}"),
            };
            assert_eq!(
                recorded[0], expected,
                "{stem}: read_page_00h selected the wrong generated decoder"
            );
        }
    }

    /// Defensive inner boundary: an unadmitted frozen context (CMIS 5.4, 3.1,
    /// 4.1, or non-CMIS family) is refused BEFORE any Transport read and before
    /// any decode — zero Transport operations and zero observer recordings.
    #[test]
    fn vendor_info_refuses_an_unadmitted_frozen_rev_before_the_data_read() {
        let foreign_cases = [
            Rev::parse_cmis("5.5").expect("parse 5.5"),
            Rev::parse_cmis("3.1").expect("parse 3.1"),
            Rev::parse_cmis("4.1").expect("parse 4.1"),
            Rev {
                family: openxcvr_core::rev::Family::Sff8472,
                major: 1,
                minor: 0,
            },
        ];
        for rev in foreign_cases {
            let (t, op_log, spy_log, _shared_img) = Page00SpyTransport::new([0u8; 128]);
            let m = CmisModule {
                transport: RefCell::new(t),
                ctx: Ctx { detected_rev: rev },
            };
            let _ = take_decoded_page00_types();
            match m.vendor_info() {
                Err(ModuleError::Unsupported) => {}
                other => panic!("{rev:?}: expected Err(ModuleError::Unsupported), got {other:?}"),
            }
            assert!(
                spy_log.borrow().is_empty(),
                "{rev:?}: decoder refusal must precede any Transport I/O; spy saw {:?}",
                spy_log.borrow()
            );
            assert!(
                op_log.borrow().is_empty(),
                "{rev:?}: decoder refusal must precede any Transport I/O; op_log saw {:?}",
                op_log.borrow()
            );
            assert!(
                take_decoded_page00_types().is_empty(),
                "{rev:?}: refused revision must not decode"
            );
        }
    }

    /// Preservation control: independent literal non-zero 9-field identity
    /// payload decoded across all 6 admitted revisions preserves exact
    /// VendorInfo fields: name, oui, part_number, rev, serial_number,
    /// 4-piece concatenated date_code, identifier = None, and ASCII trimming
    /// of spaces and NULs.
    #[test]
    fn vendor_info_preserves_non_zero_identity_fields_and_trimming() {
        let page00 = synthetic_page00_identity_bytes();
        for (stem, rev) in REV_FIXTURES {
            let mut image = [0u8; 256];
            image[..128].copy_from_slice(&fixture_lower(stem));
            image[128..256].copy_from_slice(&page00);

            let (t, _oplog, _spylog, _shared_img) = Page00SpyTransport::from_256(image);
            let m = CmisModule::new(t).expect("CMIS detect");
            assert_eq!(m.detected_rev(), rev);

            let info = m.vendor_info().expect("vendor_info");
            assert_eq!(info.name, "PETABIT-SCALE", "{stem}: name");
            assert_eq!(info.oui, [0x00, 0x1A, 0x2B], "{stem}: oui");
            assert_eq!(info.part_number, "PS-400G-DR4", "{stem}: part_number");
            assert_eq!(info.rev, "A1", "{stem}: rev");
            assert_eq!(
                info.serial_number, "SN1234567890ABCD",
                "{stem}: serial_number"
            );
            assert_eq!(info.date_code, "260415L1", "{stem}: date_code");
            assert_eq!(info.identifier, None, "{stem}: identifier must be None");
        }
    }

    /// Post-construction mutation of Lower:1 in fake transport verifies:
    /// 1. `vendor_info()` dispatches on frozen `ctx.detected_rev` (matches
    ///    original type, not mutated byte).
    /// 2. Zero Lower re-reads or re-detections.
    /// 3. Dedicated full-tuple spy verifies exact (slave_addr 0x50, page 0x00,
    ///    bank 0, offset 128, len 128), zero writes, zero probes.
    #[test]
    fn vendor_info_frozen_rev_survives_lower_mutation_with_exact_full_tuple_read() {
        for (stem, rev) in REV_FIXTURES {
            let mut image = [0u8; 256];
            image[..128].copy_from_slice(&fixture_lower(stem));
            image[128..256].copy_from_slice(&synthetic_page00_identity_bytes());

            let (t, op_log, spy_log, shared_img) = Page00SpyTransport::from_256(image);
            // Construction performs the detect-once Lower read
            let m = CmisModule::new(t).expect("CMIS detect");
            assert_eq!(m.detected_rev(), rev);

            // Mutate Lower byte 1 to 0x54 (a different rev than any fixture)
            // in the underlying transport memory
            shared_img.borrow_mut()[1] = 0x54;

            op_log.borrow_mut().clear();
            spy_log.borrow_mut().clear();
            let _ = take_decoded_page00_types();

            let info = m.vendor_info().expect("vendor_info");
            assert_eq!(info.name, "PETABIT-SCALE");

            // Type recorded matches the original frozen rev, never the mutated rev
            let recorded = take_decoded_page00_types();
            assert_eq!(recorded.len(), 1);
            let expected = match rev {
                Rev::CMIS_3_0 => {
                    std::any::type_name::<crate::generated::v3_0::CmisPage00hAdministrative>()
                }
                Rev::CMIS_4_0 => {
                    std::any::type_name::<crate::generated::v4_0::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_0 => {
                    std::any::type_name::<crate::generated::v5_0::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_1 => {
                    std::any::type_name::<crate::generated::v5_1::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_2 => {
                    std::any::type_name::<crate::generated::v5_2::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_3 => {
                    std::any::type_name::<crate::generated::v5_3::CmisPage00hAdministrative>()
                }
                Rev::CMIS_5_4 => {
                    std::any::type_name::<crate::generated::v5_4::CmisPage00hAdministrative>()
                }
                other => panic!("{stem}: unexpected rev {other:?}"),
            };
            assert_eq!(recorded[0], expected);

            // Full-tuple spy assertions:
            // Exactly one read: slave 0x50, page 0x00, bank 0, offset 128, len 128
            assert_eq!(
                *spy_log.borrow(),
                vec![SpyOp::Read {
                    slave_addr: I2C_ADDR_MODULE,
                    page: 0x00,
                    bank: 0,
                    offset: 128,
                    len: 128,
                }],
                "{stem}: exact full tuple on Page 00 read; no writes or probes"
            );
            // OpLog assertion:
            assert_eq!(
                *op_log.borrow(),
                vec![("read", 0x00, 128, 128)],
                "{stem}: OpLog exactly matches one Page 00 read"
            );
        }
    }

    /// Fault injection test: Transport failure during Page 00h read propagates
    /// unchanged, performs exactly one read attempt, and does not invoke
    /// the decoder or record to the observer.
    #[test]
    fn vendor_info_propagates_page00_transport_error_without_decoding() {
        let (mut t, _oplog, spy_log, _shared_img) =
            Page00SpyTransport::new(fixture_lower("synthetic-cmis-3-0-qsfp-dd"));
        t.fault_on_page00 = true;
        let m = CmisModule::new(t).expect("CMIS detect");

        spy_log.borrow_mut().clear();
        let _ = take_decoded_page00_types();

        match m.vendor_info() {
            Err(ModuleError::Io(io)) => assert!(io.message().contains("page 00h failure"), "{io}"),
            other => panic!("expected propagated Io error, got {other:?}"),
        }

        assert_eq!(
            *spy_log.borrow(),
            vec![SpyOp::Read {
                slave_addr: I2C_ADDR_MODULE,
                page: 0x00,
                bank: 0,
                offset: 128,
                len: 128,
            }],
            "exactly one attempt on Page 00h before propagating error"
        );
        assert!(
            take_decoded_page00_types().is_empty(),
            "must not invoke decoder when Transport read fails"
        );
    }

    /// Dispatch oracle: the types recorded at the real `read_page_11h` and
    /// `tx_bias_multiplier` boundaries on the public `lanes()` and `flags()`
    /// paths must be the frozen revision's generated reader types across
    /// all 6 admitted revisions, compiler-derived on BOTH sides
    /// (`type_name_of_val` of the actually decoded page vs `type_name` of
    /// the concrete generated type). `lanes()` dispatches BOTH the Page 11h
    /// and the Page 01h (bias) readers; `flags()` dispatches the Page 11h
    /// reader. Also pins the one reserved-nibble semantic the
    /// revision-specialised decoders own on the same pass: a raw 0x7
    /// DP-state nibble on lane 1 renders `Unknown7` at 3.0 (Table 66
    /// reserves 5h-Fh) and `DpInitialized` at 4.0..5.3 (Table 8-58).
    #[test]
    fn lanes_and_flags_dispatch_the_revision_specialised_generated_decoders() {
        for (stem, rev) in REV_FIXTURES {
            // Lower + Upper pages 00h..11h in BufTransport's flat layout;
            // Page 11h byte 128 carries raw nibble 0x7 on lane 1 (low
            // nibble) and lane 2 (high nibble); Page 01h and the rest are
            // zeros (bias multiplier ×1). Synthetic compatibility input on
            // top of the checked-in lower headers, not hardware
            // qualification.
            let mut image = vec![0u8; 128 + 0x12 * 128];
            image[..128].copy_from_slice(&fixture_lower(stem));
            let page_11h = 128 + 0x11 * 128;
            image[page_11h] = 0x77;
            let m = CmisModule::from_bytes(&image)
                .unwrap_or_else(|e| panic!("{stem}: CMIS detect: {e:?}"));
            assert_eq!(m.detected_rev(), rev, "{stem}: frozen rev from Lower:1");

            let _ = take_decoded_page11_types(); // isolate this call's records
            let _ = take_decoded_page01_types();
            let lanes = m
                .lanes()
                .unwrap_or_else(|e| panic!("{stem}: lanes(): {e:?}"));

            let (expected_p11, expected_p01) = match rev {
                Rev::CMIS_3_0 => (
                    std::any::type_name::<crate::generated::v3_0::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v3_0::CmisPage01hAdvertising>(),
                ),
                Rev::CMIS_4_0 => (
                    std::any::type_name::<crate::generated::v4_0::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v4_0::CmisPage01hAdvertising>(),
                ),
                Rev::CMIS_5_0 => (
                    std::any::type_name::<crate::generated::v5_0::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v5_0::CmisPage01hAdvertising>(),
                ),
                Rev::CMIS_5_1 => (
                    std::any::type_name::<crate::generated::v5_1::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v5_1::CmisPage01hAdvertising>(),
                ),
                Rev::CMIS_5_2 => (
                    std::any::type_name::<crate::generated::v5_2::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v5_2::CmisPage01hAdvertising>(),
                ),
                Rev::CMIS_5_3 => (
                    std::any::type_name::<crate::generated::v5_3::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v5_3::CmisPage01hAdvertising>(),
                ),
                Rev::CMIS_5_4 => (
                    std::any::type_name::<crate::generated::v5_4::CmisPage11hLaneStatus>(),
                    std::any::type_name::<crate::generated::v5_4::CmisPage01hAdvertising>(),
                ),
                other => panic!("{stem}: unexpected rev {other:?}"),
            };
            assert_eq!(
                take_decoded_page11_types(),
                vec![expected_p11],
                "{stem}: read_page_11h selected the wrong generated decoder"
            );
            assert_eq!(
                take_decoded_page01_types(),
                vec![expected_p01],
                "{stem}: tx_bias_multiplier selected the wrong generated decoder"
            );
            let expected_dp_state = if rev == Rev::CMIS_3_0 {
                "Unknown7"
            } else {
                "DpInitialized"
            };
            assert_eq!(
                lanes[0].dp_state.as_deref(),
                Some(expected_dp_state),
                "{stem}: lane 1 dp_state for raw 0x7 must follow the frozen rev's table"
            );

            // flags() dispatches the same revision-specialised Page 11h
            // reader on its own path.
            let _ = take_decoded_page11_types();
            m.flags()
                .unwrap_or_else(|e| panic!("{stem}: flags(): {e:?}"));
            assert_eq!(
                take_decoded_page11_types(),
                vec![expected_p11],
                "{stem}: flags()'s read_page_11h selected the wrong generated decoder"
            );
        }
    }

    /// Per-lane signal flags surface through `lanes()` from the same
    /// generated Page 11h flag decoders `flags()` reads: latched Tx LOS
    /// (byte 136.0-7), Tx CDR LOL (137), Tx Adaptive Input EQ Fail (138),
    /// host lanes, and Rx CDR LOL (148), media lanes (§ 8.10.3 Tables
    /// 8-87/8-88). One bit per flag byte picks a distinct lane so a
    /// cross-wired projection cannot pass.
    #[test]
    fn lanes_surface_the_page11h_signal_flags_per_lane() {
        let mut image = vec![0u8; 128 + 0x12 * 128];
        image[..128].copy_from_slice(&fixture_lower("synthetic-cmis-3-0-qsfp-dd"));
        let p11 = 128 + 0x11 * 128;
        image[p11 + 8] = 0b0000_0001; // Tx LOS, host lane 1
        image[p11 + 9] = 0b0000_0010; // Tx CDR LOL, host lane 2
        image[p11 + 10] = 0b0000_0100; // Tx adaptive EQ fail, host lane 3
        image[p11 + 20] = 0b0000_1000; // Rx CDR LOL, media lane 4
        let m = CmisModule::from_bytes(&image).expect("CMIS 3.0 detect");
        let lanes = m.lanes().expect("lanes");
        assert_eq!(lanes.len(), 8);
        assert_eq!(lanes[0].tx_los, Some(true));
        assert_eq!(lanes[1].tx_cdr_lol, Some(true));
        assert_eq!(lanes[2].tx_adaptive_eq_fault, Some(true));
        assert_eq!(lanes[3].rx_cdr_lol, Some(true));
        // Every other lane decodes the clear bit, not `None` — the flags
        // exist at every admitted revision, so absence would be a dropped
        // projection, not an unimplemented monitor.
        for (i, lane) in lanes.iter().enumerate() {
            assert_eq!(lane.tx_los, Some(i == 0), "lane {} tx_los", i + 1);
            assert_eq!(lane.tx_cdr_lol, Some(i == 1), "lane {} tx_cdr_lol", i + 1);
            assert_eq!(
                lane.tx_adaptive_eq_fault,
                Some(i == 2),
                "lane {} tx_adaptive_eq_fault",
                i + 1
            );
            assert_eq!(lane.rx_cdr_lol, Some(i == 3), "lane {} rx_cdr_lol", i + 1);
        }
    }

    /// Defensive inner boundary: an unadmitted frozen context (CMIS 5.4,
    /// 3.1, 4.1, or non-CMIS family) is refused BEFORE any Transport read
    /// and before any decode — on the private `read_page_11h` and
    /// `tx_bias_multiplier` boundaries directly, and on the public
    /// `lanes()`/`flags()` paths (which refuse at their first
    /// revision-dispatched read) — zero Transport operations and zero
    /// observer recordings.
    #[test]
    fn lanes_and_flags_refuse_an_unadmitted_frozen_rev_before_the_data_read() {
        let foreign_cases = [
            Rev::parse_cmis("5.5").expect("parse 5.5"),
            Rev::parse_cmis("3.1").expect("parse 3.1"),
            Rev::parse_cmis("4.1").expect("parse 4.1"),
            Rev {
                family: openxcvr_core::rev::Family::Sff8472,
                major: 1,
                minor: 0,
            },
        ];
        for rev in foreign_cases {
            let (t, op_log, spy_log, _shared_img) = Page00SpyTransport::new([0u8; 128]);
            let m = CmisModule {
                transport: RefCell::new(t),
                ctx: Ctx { detected_rev: rev },
            };
            let _ = take_decoded_page11_types();
            let _ = take_decoded_page01_types();
            // Private boundaries refuse directly.
            for (label, call) in [
                ("read_page_11h", m.read_page_11h().map(|_: Page11Common| ())),
                (
                    "tx_bias_multiplier",
                    m.tx_bias_multiplier().map(|_: Option<f32>| ()),
                ),
                ("lanes", m.lanes().map(|_: Vec<_>| ())),
                ("flags", m.flags().map(|_: ModuleFlags| ())),
            ] {
                match call {
                    Err(ModuleError::Unsupported) => {}
                    other => panic!(
                        "{rev:?}: {label}: expected Err(ModuleError::Unsupported), got {other:?}"
                    ),
                }
            }
            assert!(
                spy_log.borrow().is_empty(),
                "{rev:?}: decoder refusal must precede any Transport I/O; spy saw {:?}",
                spy_log.borrow()
            );
            assert!(
                op_log.borrow().is_empty(),
                "{rev:?}: decoder refusal must precede any Transport I/O; op_log saw {:?}",
                op_log.borrow()
            );
            assert!(
                take_decoded_page11_types().is_empty(),
                "{rev:?}: refused revision must not decode Page 11h"
            );
            assert!(
                take_decoded_page01_types().is_empty(),
                "{rev:?}: refused revision must not decode Page 01h"
            );
        }
    }
}

#[cfg(test)]
mod fault_injection_tests {
    use super::tests::fixture_lower;
    use super::*;

    /// Transport wrapper that injects a failure into every Page 11h read,
    /// passing everything else through to the deterministic image below.
    struct Page11hFault<T> {
        inner: T,
    }

    impl<T: Transport> Transport for Page11hFault<T> {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            if page == 0x11 {
                return Err(ModuleError::Io("injected page 11h failure".into()));
            }
            self.inner.read(slave_addr, page, bank, offset, buf)
        }
        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            self.inner.write(slave_addr, page, bank, offset, data)
        }
        fn probe(&mut self) -> Result<(), ModuleError> {
            self.inner.probe()
        }
    }
    impl<T: CmisTransport> CmisTransport for Page11hFault<T> {}

    fn image_with_page11h(state_bytes: u8) -> Vec<u8> {
        // Lower (128) + Upper pages 00h..11h in BufTransport's flat layout.
        let mut image = vec![0u8; 128 + 0x12 * 128];
        image[..128].copy_from_slice(&fixture_lower("synthetic-cmis-3-0-qsfp-dd"));
        let page_11h = 128 + 0x11 * 128;
        image[page_11h..page_11h + 4].fill(state_bytes);
        image
    }

    /// Fault-injected Page 11h transport error: `lanes()` propagates the
    /// error and manufactures no lanes (no zero-filled or fallback block).
    #[test]
    fn lanes_propagate_a_page11h_transport_error_without_manufacturing_lanes() {
        let image = image_with_page11h(0x43);
        let transport = Page11hFault {
            inner: openxcvr_core::transport::BufTransport::from_slice(&image),
        };
        let m = CmisModule::new(transport).expect("CMIS 3.0 detect");
        match m.lanes() {
            Err(ModuleError::Io(io)) => assert!(io.message().contains("page 11h"), "{io}"),
            other => panic!("expected propagated Page 11h Io error, got {other:?}"),
        }
        // flags() reads Page 11h too: same propagated refusal, never a
        // manufactured lane-flag tree.
        match m.flags() {
            Err(ModuleError::Io(io)) => assert!(io.message().contains("page 11h"), "{io}"),
            other => panic!("expected propagated Page 11h Io error, got {other:?}"),
        }
    }

    /// Truncated capture (short read): the module constructs (Lower is
    /// present), but the Page 11h read runs past the end of the image and
    /// `lanes()` propagates the short-read error — no zero-filled lanes.
    #[test]
    fn lanes_propagate_a_page11h_short_read_without_manufacturing_lanes() {
        // Lower + Page 01h only: page 0x11 lives beyond the image.
        let mut image = vec![0u8; 128 + 0x02 * 128];
        image[..128].copy_from_slice(&fixture_lower("synthetic-cmis-3-0-qsfp-dd"));
        let m = CmisModule::from_bytes(&image).expect("CMIS 3.0 detect");
        match m.lanes() {
            Err(ModuleError::Io(io)) => assert!(io.message().contains("short read"), "{io}"),
            other => panic!("expected propagated short-read error, got {other:?}"),
        }
    }

    /// Flat-memory module: `lanes()` reports Unsupported even when bank-0
    /// Page 11h bytes are present in the capture — a flat module has no
    /// banked upper pages, so no lanes are ever manufactured from them.
    #[test]
    fn flat_memory_lanes_report_unsupported_even_with_page11h_bytes_present() {
        let mut image = image_with_page11h(0x43);
        image[2] |= 0x80; // Flat_mem (Lower:2 bit 7)
        let m = CmisModule::from_bytes(&image).expect("CMIS 3.0 detect");
        assert!(matches!(m.lanes(), Err(ModuleError::Unsupported)));
        assert!(matches!(m.flags(), Err(ModuleError::Unsupported)));
        assert!(matches!(m.lane_count(), Err(ModuleError::Unsupported)));
    }
}
