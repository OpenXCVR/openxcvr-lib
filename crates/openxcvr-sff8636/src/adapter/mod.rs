//! SFF-8636 rev-aware adapter.
//!
//! Two construction paths, one decode discipline:
//!
//! - [`Sff8636Module::open`] performs detect-once revision detection
//!   (Lower:1, Table 6-4 compliance code) and freezes a [`Ctx`]; every
//!   subsequent read decodes through the revision-specialised generated
//!   reader for that revision (`generated::v1_3` … `generated::v2_12`) —
//!   never the union, never an assumed latest revision. An unknown or
//!   legacy code fails closed with
//!   [`openxcvr_core::DetectError::UnsupportedRev`].
//! - [`Sff8636Module::new`] keeps the historical rev-agnostic union view
//!   for callers (fixture decode, captures with byte 1 = 0x00) that have
//!   not opted into detection.
//!
//! Write/control support (tx disable, tx/rx rate select) lives on the
//! module handle below and goes through the same transport — pure
//! read-modify-write of the schema-declared control bytes, no autonomous
//! state.

mod applications;
mod checksums;
mod control;
mod flags;
mod identity;
mod lanes;
mod lower;
mod monitoring;
mod state;
mod thresholds;

use std::cell::RefCell;

use openxcvr_core::{
    error::ModuleError,
    module::Module,
    transport::Transport,
    types::{Application, ChecksumStatus, LaneStatus, ModuleFlags, ModuleState, VendorInfo},
    DetectError,
};

pub use crate::ctx::Ctx;
use openxcvr_core::rev::Rev;
pub use thresholds::Sff8636ThresholdEvaluations;

/// SFF-8636 module surface over any [`Transport`]: the unified [`Module`]
/// vocabulary (vendor identity, monitoring, lanes, flags, checksums,
/// applications, state) translated down to the lower/upper paged byte
/// locations. Construct with [`Self::new`] (rev-agnostic) or [`Self::open`]
/// (detect-once, revision-specialised decoding).
pub struct Sff8636Module<T: Transport> {
    transport: RefCell<T>,
    /// Frozen detect-once revision context (None on the undetected union
    /// path — see the module docs).
    ctx: Option<Ctx>,
}

impl<T: Transport> Sff8636Module<T> {
    /// Rev-agnostic construction: decode through the union view of every
    /// generated field. Callers that can read Lower:1 should prefer
    /// [`Self::open`], which decodes through the module's actual revision.
    pub fn new(transport: T) -> Self {
        Self {
            transport: RefCell::new(transport),
            ctx: None,
        }
    }

    /// Detect-once construction: read Lower:1, map it through Table 6-4 and
    /// freeze the detected revision. Fails closed — never a latest-revision
    /// fallback — on transport errors and on unknown/legacy compliance
    /// codes.
    pub fn open(mut transport: T) -> Result<Self, DetectError> {
        let ctx = lower::detect(&mut transport)?;
        Ok(Self {
            transport: RefCell::new(transport),
            ctx: Some(ctx),
        })
    }

    /// The detected revision, when this handle was opened through
    /// [`Self::open`].
    pub fn detected_rev(&self) -> Option<Rev> {
        self.ctx.map(|c| c.detected_rev)
    }

    /// The frozen detect-once context.
    pub fn ctx(&self) -> Option<&Ctx> {
        self.ctx.as_ref()
    }

    fn read_lower(&self) -> Result<lower::LowerCommon, ModuleError> {
        lower::read_lower(&mut *self.transport.borrow_mut(), self.ctx)
    }

    // ── control surface (tx disable / rate select) ───────────────────────

    /// Set the per-lane Tx Disable bits (Lower byte 86, bits 3-0; Table
    /// 6-9). Read-modify-write: only the selected lane's bit changes.
    pub fn set_tx_disable(&self, lane: u8, disable: bool) -> Result<(), ModuleError> {
        control::set_tx_disable(&mut *self.transport.borrow_mut(), lane, disable)
    }

    /// Set Tx Disable for all four lanes at once.
    pub fn set_tx_disable_all(&self, disable: [bool; 4]) -> Result<(), ModuleError> {
        control::set_tx_disable_all(&mut *self.transport.borrow_mut(), disable)
    }

    /// Set a lane's Tx Rate Select (Lower byte 88; Table 6-9). The value is
    /// the raw 2-bit code — see the schema's `Sff8636RateSelect` for the
    /// named codes.
    pub fn set_tx_rate_select_raw(&self, lane: u8, value: u8) -> Result<(), ModuleError> {
        control::set_rate_select_raw(
            &mut *self.transport.borrow_mut(),
            control::RateSelectTarget::Tx,
            lane,
            value,
        )
    }

    /// Set a lane's Rx Rate Select (Lower byte 87; Table 6-9).
    pub fn set_rx_rate_select_raw(&self, lane: u8, value: u8) -> Result<(), ModuleError> {
        control::set_rate_select_raw(
            &mut *self.transport.borrow_mut(),
            control::RateSelectTarget::Rx,
            lane,
            value,
        )
    }

    /// Evaluate the Lower-memory monitors (case temperature, supply
    /// voltage, per-lane RX power / TX bias / TX power) against the Upper
    /// Page 03h threshold sets (§ 6.6.1, Table 6-28) and return the pure
    /// decisions.
    ///
    /// Supply-bytes: `page03h` is the 128 bytes of Upper Page 03h (the
    /// window the module serves at offset 128 after the caller's own page
    /// select). This crate never writes the page-select byte (Lower:127)
    /// to fetch it — the read footprint stays the page-00h/lower reads
    /// every monitor accessor already makes, so the page selection stays
    /// caller-directed. A module that does not implement Page 03h (or a
    /// monitor whose threshold set is all-zero) stays within bounds,
    /// never a spurious alarm.
    pub fn evaluate_thresholds(
        &self,
        page03h: &[u8; 128],
    ) -> Result<Sff8636ThresholdEvaluations, ModuleError> {
        let lower = self.read_lower()?;
        let thresholds = thresholds::decode_page03h(self.ctx, page03h)?;
        Ok(thresholds::evaluate(&lower, &thresholds))
    }
}

impl<T: Transport> Module for Sff8636Module<T> {
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
        identity::vendor_info(&mut *self.transport.borrow_mut(), self.ctx)
    }

    fn identifier(&self) -> Result<u8, ModuleError> {
        identity::identifier(&mut *self.transport.borrow_mut())
    }

    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
        Ok(monitoring::temperature_celsius(&self.read_lower()?))
    }

    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
        Ok(monitoring::supply_voltage_v(&self.read_lower()?))
    }

    fn lane_count(&self) -> Result<u8, ModuleError> {
        Ok(4)
    }

    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
        lanes::lanes(&self.read_lower()?)
    }

    fn flags(&self) -> Result<ModuleFlags, ModuleError> {
        Ok(flags::module_flags(&self.read_lower()?))
    }

    fn checksums(&self) -> Result<ChecksumStatus, ModuleError> {
        checksums::checksums(&mut *self.transport.borrow_mut())
    }

    fn applications(&self) -> Result<Vec<Application>, ModuleError> {
        Ok(vec![applications::applications(
            &mut *self.transport.borrow_mut(),
            self.ctx,
        )?])
    }

    fn module_state(&self) -> Result<ModuleState, ModuleError> {
        Ok(state::module_state(&self.read_lower()?))
    }
}

// cfg(test) observers for the dispatch tests: record the compiler-derived
// type name of the page value actually decoded on the real read paths, so
// tests compare compiler-derived types rather than hand-written labels.
// Compiled out of release builds entirely.
#[cfg(test)]
thread_local! {
    static DECODED_LOWER_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
    static DECODED_PAGE00_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(super) fn record_decoded_lower_type(ty: &'static str) {
    DECODED_LOWER_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
pub(super) fn take_decoded_lower_types() -> Vec<&'static str> {
    DECODED_LOWER_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[cfg(test)]
pub(super) fn record_decoded_page00_type(ty: &'static str) {
    DECODED_PAGE00_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
pub(super) fn take_decoded_page00_types() -> Vec<&'static str> {
    DECODED_PAGE00_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use openxcvr_core::transport::Transport;

    /// Flat 256-byte transport: bytes 0..128 Lower page, 128..256 Upper
    /// Page 00h — same layout as `openxcvr_core::transport::BufTransport`,
    /// kept local so the control-surface tests stay in one place.
    struct TestTransport {
        buf: [u8; 256],
    }

    impl TestTransport {
        fn with_lower(lower: [u8; 128]) -> Self {
            let mut buf = [0u8; 256];
            buf[..128].copy_from_slice(&lower);
            Self { buf }
        }
    }

    impl Transport for TestTransport {
        fn read(
            &mut self,
            _a: u8,
            page: u8,
            _b: u8,
            offset: u8,
            out: &mut [u8],
        ) -> Result<(), ModuleError> {
            let start = match (page, offset) {
                (0x00, 0..=255) => offset as usize,
                _ => return Err(ModuleError::Io("unsupported page".into())),
            };
            let end = start + out.len();
            if end > self.buf.len() {
                return Err(ModuleError::Io("short read".into()));
            }
            out.copy_from_slice(&self.buf[start..end]);
            Ok(())
        }
        fn write(
            &mut self,
            _a: u8,
            page: u8,
            _b: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            if page != 0x00 {
                return Err(ModuleError::Io("unsupported page".into()));
            }
            let start = offset as usize;
            let end = start + data.len();
            if end > 128 {
                return Err(ModuleError::Io("write outside lower region".into()));
            }
            self.buf[start..end].copy_from_slice(data);
            Ok(())
        }
        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    /// Every admitted revision decodes through its OWN generated reader on
    /// the real `read_lower` path: the compiler-derived type of the decoded
    /// page is `generated::v{rev}`, never the union and never a
    /// latest-revision fallback. The union path (`new`) decodes the union.
    #[test]
    fn per_revision_reads_dispatch_to_the_matching_compiled_reader() {
        for (byte, label) in [
            (0x03u8, "v1_3"),
            (0x04, "v1_4"),
            (0x05, "v1_5"),
            (0x06, "v2_0"),
            (0x07, "v2_5"),
            (0x08, "v2_8"),
            (0x09, "v2_11"),
            (0x0A, "v2_12"),
        ] {
            let mut lower = [0u8; 128];
            lower[1] = byte;
            let module = Sff8636Module::open(TestTransport::with_lower(lower))
                .unwrap_or_else(|e| panic!("byte {byte:#04x}: {e:?}"));
            module.lanes().expect("lanes");
            let types = take_decoded_lower_types();
            assert!(
                types
                    .iter()
                    .any(|t| t.contains(&format!("generated::{label}::"))),
                "byte {byte:#04x} must decode through generated::{label}, got {types:?}"
            );
        }

        // Undetected union path decodes the union view.
        let mut lower = [0u8; 128];
        lower[1] = 0x09;
        let module = Sff8636Module::new(TestTransport::with_lower(lower));
        module.lanes().expect("lanes");
        let types = take_decoded_lower_types();
        // The union re-export aliases the flat backing module, whose
        // compiler-derived name is the union view's own type.
        assert!(
            types
                .iter()
                .any(|t| t.contains("generated::sff8636_lower_memory::")),
            "new() decodes the union view, got {types:?}"
        );
    }

    /// Page 00h identity reads dispatch per detected revision too.
    #[test]
    fn page00_reads_dispatch_per_revision() {
        for (byte, label) in [(0x03u8, "v1_3"), (0x09, "v2_11"), (0x0A, "v2_12")] {
            let mut lower = [0u8; 128];
            lower[1] = byte;
            let module = Sff8636Module::open(TestTransport::with_lower(lower)).expect("opens");
            module.vendor_info().expect("vendor_info");
            let types = take_decoded_page00_types();
            assert!(
                types
                    .iter()
                    .any(|t| t.contains(&format!("generated::{label}::"))),
                "byte {byte:#04x} page00 must decode through generated::{label}, got {types:?}"
            );
        }
    }
}
