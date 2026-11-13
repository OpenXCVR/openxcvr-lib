//! SFF-8472 module adapter: the unified [`Module`](openxcvr_core::module::Module)
//! surface translated down to the A0h/A2h byte locations.

mod applications;
mod calibration;
mod checksums;
mod control;
mod flags;
mod identity;
mod lanes;
mod monitoring;
mod pages;
mod state;
mod thresholds;

use std::cell::RefCell;

use openxcvr_core::{
    error::ModuleError,
    module::Module,
    rev::Rev,
    transport::Transport,
    types::{Application, ChecksumStatus, LaneStatus, ModuleFlags, ModuleState, VendorInfo},
    DetectError,
};

pub use crate::ctx::Ctx;
pub use thresholds::Sff8472ThresholdEvaluations;

/// SFF-8472 module surface over any [`Transport`]: the unified
/// [`Module`] vocabulary (vendor identity, monitoring, lanes, flags,
/// checksums, applications, state) translated down to the A0h/A2h byte
/// locations. Construct with [`Self::new`] (rev-agnostic) or [`Self::open`]
/// (detect-once, revision-specialised decoding).
pub struct Sff8472Module<T: Transport> {
    transport: RefCell<T>,
    /// Frozen detect-once revision context (None on the undetected union
    /// path).
    ctx: Option<Ctx>,
}

impl<T: Transport> Sff8472Module<T> {
    /// Rev-agnostic construction: decode through the union view of every
    /// generated field. Callers that can read A0h:94 should prefer
    /// [`Self::open`], which decodes through the module's actual revision.
    pub fn new(transport: T) -> Self {
        Self {
            transport: RefCell::new(transport),
            ctx: None,
        }
    }

    /// Detect-once construction: read A0h byte 94 (Table 8-8 compliance
    /// code) and freeze the detected revision. Fails closed - never a
    /// latest-revision fallback - on transport errors and on
    /// unspecified/reserved compliance codes.
    pub fn open(mut transport: T) -> Result<Self, DetectError> {
        let ctx = pages::detect(&mut transport)?;
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

    // ── control surface (soft TX disable / rate select) ──────────────

    /// Set the Soft TX Disable Select bit (A2h byte 110 bit 6, Table
    /// 9-16): writing '1' disables the laser; the bit is OR'd with the
    /// hard TX_DISABLE pin value. Byte 110 bit 7 is the read-only pin-state
    /// mirror, not a write target — a module that does not implement soft
    /// TX disable ignores the bit. Read-modify-write: only bit 6 changes.
    pub fn set_tx_disable(&self, disable: bool) -> Result<(), ModuleError> {
        control::set_tx_disable(&mut *self.transport.borrow_mut(), disable)
    }

    /// Set the Soft Rate Select RS(0) bit (A2h byte 110 bit 3, Table
    /// 9-16): writing '1' selects full bandwidth operation; the bit is
    /// OR'd with the hard Rate_Select / AS(0) / RS(0) pin value.
    /// Read-modify-write: only bit 3 changes.
    pub fn set_rate_select_rs0(&self, full_bandwidth: bool) -> Result<(), ModuleError> {
        control::set_rate_select_rs0(&mut *self.transport.borrow_mut(), full_bandwidth)
    }

    /// Set the Soft RS(1) Select bit (A2h byte 118 bit 3, Table 10-1):
    /// writing '1' selects full speed Tx operation; the bit is OR'd with
    /// the hard RS(1) pin value. Read-modify-write: only bit 3 changes.
    pub fn set_rate_select_rs1(&self, full_speed: bool) -> Result<(), ModuleError> {
        control::set_rate_select_rs1(&mut *self.transport.borrow_mut(), full_speed)
    }

    /// Evaluate the five base monitors (temperature, supply voltage, TX
    /// bias, TX power, RX power) against their A2h alarm/warning
    /// threshold sets (§ 9.4, Table 9-5) and return the pure decisions —
    /// the library-side counterpart of the module's own Table 9-17 flag
    /// bits. Monitors or threshold sets the module does not implement
    /// stay within bounds instead of tripping on zero bounds. This is a
    /// decision, not permission to act on the module: executing on an
    /// alarm belongs to the daemon.
    pub fn evaluate_thresholds(&self) -> Result<Sff8472ThresholdEvaluations, ModuleError> {
        thresholds::evaluate_thresholds(&mut *self.transport.borrow_mut(), self.ctx)
    }
}

impl<T: Transport> Module for Sff8472Module<T> {
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
        identity::vendor_info(&mut *self.transport.borrow_mut(), self.ctx)
    }
    fn identifier(&self) -> Result<u8, ModuleError> {
        identity::identifier(&mut *self.transport.borrow_mut())
    }
    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
        monitoring::temperature_celsius(&mut *self.transport.borrow_mut())
    }
    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
        monitoring::supply_voltage_v(&mut *self.transport.borrow_mut())
    }
    fn lane_count(&self) -> Result<u8, ModuleError> {
        Ok(1)
    }
    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
        lanes::lanes(&mut *self.transport.borrow_mut(), self.ctx)
    }
    fn flags(&self) -> Result<ModuleFlags, ModuleError> {
        flags::module_flags(&mut *self.transport.borrow_mut(), self.ctx)
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
        state::module_state(&mut *self.transport.borrow_mut(), self.ctx)
    }
}

// cfg(test) observers for the dispatch tests: record the compiler-derived
// type name of the page value actually decoded on the real read paths.
#[cfg(test)]
thread_local! {
    static DECODED_A0H_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
    static DECODED_A2H_TYPES: RefCell<Vec<&'static str>> =
        const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(super) fn record_decoded_a0h_type(ty: &'static str) {
    DECODED_A0H_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
pub(super) fn take_decoded_a0h_types() -> Vec<&'static str> {
    DECODED_A0H_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[cfg(test)]
pub(super) fn record_decoded_a2h_type(ty: &'static str) {
    DECODED_A2H_TYPES.with(|log| log.borrow_mut().push(ty));
}

#[cfg(test)]
pub(super) fn take_decoded_a2h_types() -> Vec<&'static str> {
    DECODED_A2H_TYPES.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use openxcvr_core::transport::Transport;

    struct TestTransport {
        a0: [u8; 256],
        a2: [u8; 256],
    }

    impl Transport for TestTransport {
        fn read(
            &mut self,
            address: u8,
            _page: u8,
            _bank: u8,
            offset: u8,
            out: &mut [u8],
        ) -> Result<(), ModuleError> {
            let src = match address {
                0xA0 => &self.a0[offset as usize..],
                0xA2 => &self.a2[offset as usize..],
                _ => return Err(ModuleError::Io("bad address".into())),
            };
            out.copy_from_slice(&src[..out.len()]);
            Ok(())
        }
        fn write(&mut self, _a: u8, _p: u8, _b: u8, _o: u8, _d: &[u8]) -> Result<(), ModuleError> {
            Err(ModuleError::Io("read-only".into()))
        }
        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    fn transport_with(compliance: u8) -> TestTransport {
        let mut t = TestTransport {
            a0: [0; 256],
            a2: [0; 256],
        };
        t.a0[0] = 0x03;
        t.a0[94] = compliance;
        t
    }

    /// Every admitted revision decodes A0h/A2h through its OWN generated
    /// reader on the real read paths - never the union, never an assumed
    /// latest revision. The undetected path decodes the union view.
    #[test]
    fn per_revision_reads_dispatch_to_the_matching_compiled_reader() {
        for (byte, label) in [
            (0x01u8, "v9_3"),
            (0x02, "v9_5"),
            (0x03, "v10_2"),
            (0x04, "v10_4"),
            (0x05, "v11_0"),
            (0x06, "v11_3"),
            (0x07, "v11_4"),
            (0x08, "v12_3"),
            (0x09, "v12_4"),
            (0x0A, "v12_5"),
        ] {
            let module =
                Sff8472Module::open(transport_with(byte)).unwrap_or_else(|e| panic!("{e:?}"));
            module.vendor_info().expect("vendor_info");
            module.flags().expect("flags");
            let a0 = take_decoded_a0h_types();
            let a2 = take_decoded_a2h_types();
            assert!(
                a0.iter()
                    .any(|t| t.contains(&format!("generated::{label}::"))),
                "byte {byte:#04x} A0h must decode through generated::{label}, got {a0:?}"
            );
            assert!(
                a2.iter()
                    .any(|t| t.contains(&format!("generated::{label}::"))),
                "byte {byte:#04x} A2h must decode through generated::{label}, got {a2:?}"
            );
        }

        let module = Sff8472Module::new(transport_with(0x0A));
        module.vendor_info().expect("vendor_info");
        module.flags().expect("flags");
        let a0 = take_decoded_a0h_types();
        let a2 = take_decoded_a2h_types();
        assert!(
            a0.iter()
                .any(|t| t.contains("generated::page_a0h_base_memory::")),
            "new() decodes the union view, got {a0:?}"
        );
        assert!(
            a2.iter()
                .any(|t| t.contains("generated::page_a2h_diagnostics_base::")),
            "new() decodes the union view, got {a2:?}"
        );
    }

    /// The rev-aware path reports the detected revision in
    /// `vendor_info().spec_rev`; the undetected path never guesses.
    #[test]
    fn vendor_info_carries_detected_spec_rev() {
        let module = Sff8472Module::open(transport_with(0x07)).expect("11.4 opens");
        assert_eq!(module.detected_rev(), Some(Rev::SFF8472_11_4));
        assert_eq!(
            module.vendor_info().unwrap().spec_rev,
            Some(Rev::SFF8472_11_4)
        );

        let union = Sff8472Module::new(transport_with(0x07));
        assert_eq!(union.vendor_info().unwrap().spec_rev, None);
    }

    /// Unspecified and reserved compliance codes fail closed.
    #[test]
    fn open_fails_closed_on_unknown_codes() {
        for byte in [0x00u8, 0x0B, 0xFF] {
            match Sff8472Module::open(transport_with(byte)) {
                Err(DetectError::UnsupportedRev { detected_byte, .. }) => {
                    assert_eq!(detected_byte, byte)
                }
                Err(other) => panic!("byte {byte:#04x}: unexpected {other:?}"),
                Ok(_) => panic!("byte {byte:#04x} must fail closed"),
            }
        }
    }
}
