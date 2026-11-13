use openxcvr_core::fsm::{FsmTable, FsmTransition};
use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE, Family, Rev};

use crate::generated::fsm::cmis_network_path_state_machine;
use crate::generated::CmisNetworkPathState;
use crate::module::CmisTransport;
use std::{error::Error, fmt};
const NPSM_SUPPORT_PAGE: u8 = 0x01;
const NPSM_SUPPORT_BANK: u8 = 0;
// NetworkPathPagesSupported flag (Page 16h / NP parts of 17h) — CMIS 5.3
// § 8.4.4, Page 01h byte 142 bit 7.
const NPSM_SUPPORT_OFFSET: u8 = 142;
const NPSM_SUPPORT_BIT: u8 = 0x80;
const NPSM_STATE_PAGE: u8 = 0x16;
const NPSM_STATE_BANK: u8 = 0;
// NPStateHostLane<lane> nibbles start at Page 16h byte 200 (CMIS 5.3
// § 8.15.4); byte 128 is the SCS0 NPID config register, not state.
const NPSM_STATE_OFFSET: u8 = 200;
const NPSM_DEACTIVATE_OFFSET: u8 = 160;
const NPSM_ACTIVATE_OFFSET: u8 = 176;
const MAX_NP_ID: u8 = 4;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// One network path's state-machine state (nibble code).
pub enum NpState {
    /// The network path is deactivated.
    Deactivated,
    /// The network path is initializing.
    Init,
    /// The network path is de-initializing.
    Deinit,
    /// The network path is activated.
    Activated,
    /// A reserved code the revision does not name, carried raw.
    Unknown(u8),
}
impl NpState {
    /// Decode the raw 4-bit nibble.
    pub fn from_nibble(raw: u8) -> Self {
        match raw & 0x0F {
            0x1 => Self::Deactivated,
            0x2 => Self::Init,
            0x3 => Self::Deinit,
            0x4 => Self::Activated,
            other => Self::Unknown(other),
        }
    }

    /// Decodes a Network Path State nibble for a specific catalog revision
    /// context, through the generated `CmisNetworkPathState` decode.
    ///
    /// Fail closed, mirroring [`DpState::from_nibble_for_rev`](super::DpState::from_nibble_for_rev):
    /// the context must be CMIS-family with both version components
    /// packable into a MajorRevision byte, and that packed byte must
    /// round-trip through `Rev::from_major_byte_cmis` back to exactly this
    /// `rev` — a non-catalog context (3.1, 5.5, a foreign family) never
    /// borrows a catalog decode and returns `Unknown(nibble)`.
    pub fn from_nibble_for_rev(raw: u8, rev: Rev) -> Self {
        let nibble = raw & 0x0F;
        if rev.family == Family::Cmis && rev.major <= 15 && rev.minor <= 15 {
            let byte = (rev.major << 4) | rev.minor;
            if Rev::from_major_byte_cmis(byte) == Some(rev) {
                return CmisNetworkPathState::from_rev(byte, nibble).into();
            }
        }
        Self::Unknown(nibble)
    }
}

// ── Generated-enum deduplication ──────────────────────────────────────
//
// `NpState` keeps its long-public variant names so every existing consumer
// keeps compiling; the lossless conversions below make the generated
// `CmisNetworkPathState` and this enum one semantic identity on raw
// values. `NpState` predates the schema's NPTxTurnOn/NPTxTurnOff/
// NPInitialized states, so those generated variants narrow to
// `Unknown(5/6/7)` here and widen back exactly — value-preserving, never a
// named-state invention.

impl From<CmisNetworkPathState> for NpState {
    fn from(state: CmisNetworkPathState) -> Self {
        match state {
            CmisNetworkPathState::Reserved0 => Self::Unknown(0),
            CmisNetworkPathState::NpDeactivated => Self::Deactivated,
            CmisNetworkPathState::NpInit => Self::Init,
            CmisNetworkPathState::NpDeinit => Self::Deinit,
            CmisNetworkPathState::NpActivated => Self::Activated,
            CmisNetworkPathState::NpTxTurnOn => Self::Unknown(5),
            CmisNetworkPathState::NpTxTurnOff => Self::Unknown(6),
            CmisNetworkPathState::NpInitialized => Self::Unknown(7),
            CmisNetworkPathState::Unknown(v) => Self::Unknown(v),
        }
    }
}

impl From<NpState> for CmisNetworkPathState {
    fn from(state: NpState) -> Self {
        match state {
            NpState::Deactivated => Self::NpDeactivated,
            NpState::Init => Self::NpInit,
            NpState::Deinit => Self::NpDeinit,
            NpState::Activated => Self::NpActivated,
            NpState::Unknown(0) => Self::Reserved0,
            NpState::Unknown(v) => Self::Unknown(v),
        }
    }
}
#[derive(Debug)]
#[non_exhaustive]
/// Why an NPSM operation failed.
pub enum NpsmError {
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The module does not advertise NPSM support.
    Unsupported,
    /// The network path ID is outside 0..=3.
    InvalidNpId(u8),
    /// The network path is not in one of the required states.
    WrongState {
        /// The states the operation requires.
        required: Vec<NpState>,
        /// The state actually read.
        actual: NpState,
    },
    /// The poll budget expired before the target state arrived.
    Timeout,
    /// The revision context is not one of the CMIS catalog revisions
    /// (3.0, 4.0, 5.0, 5.1, 5.2, 5.3, 5.4). Every revision-qualified NPSM
    /// entry point fails closed with this before any transport I/O rather
    /// than silently borrowing a catalog state machine.
    UnsupportedRevision(Rev),
    /// The requested target state cannot be reached from the current state
    /// through the revision's generated behaviour table. Pure plan refusal;
    /// never a transport outcome.
    UnreachableTransition {
        /// The state the transition would leave.
        from: NpState,
        /// The state that cannot be reached.
        to: NpState,
    },
}
impl fmt::Display for NpsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "NPSM transport error: {err}"),
            Self::Unsupported => write!(f, "NPSM unsupported"),
            Self::InvalidNpId(np_id) => write!(f, "invalid NPSM network path id {np_id}"),
            Self::WrongState { required, actual } => {
                write!(
                    f,
                    "NPSM wrong state: required {required:?}, actual {actual:?}"
                )
            }
            Self::Timeout => write!(f, "NPSM timed out"),
            Self::UnsupportedRevision(rev) => {
                write!(f, "unsupported NPSM CMIS revision {rev:?}")
            }
            Self::UnreachableTransition { from, to } => {
                write!(f, "no NPSM transition path from {from:?} to {to:?}")
            }
        }
    }
}
impl Error for NpsmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}
impl From<ModuleError> for NpsmError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}
/// Network-path state-machine helper over any CMIS transport.
pub struct Npsm;
impl Npsm {
    /// The generated behaviour table for `rev`, or
    /// `None` for any non-catalog or non-CMIS revision context. Fail
    /// closed: callers must refuse to operate under an unsupported
    /// revision, never borrow the nearest table.
    pub fn fsm_for_rev(rev: Rev) -> Option<&'static FsmTable<CmisNetworkPathState>> {
        cmis_network_path_state_machine::for_revision(rev)
    }

    /// Pure, revision-aware plan of the intermediate transitions from
    /// `current` to `target` through the generated behaviour table — the
    /// executable per-edge steps between bulk waits, with guards and side
    /// effects still attached as data for the caller to execute through
    /// the transport it owns.
    ///
    /// Structural (BFS, shortest edge path, schema-order ties): guards are
    /// facts only the caller observes live, so each returned edge is
    /// re-validated with `FsmTable::evaluate` as the caller executes it.
    /// Fails closed: `UnsupportedRevision` for a non-catalog revision,
    /// `UnreachableTransition` when the revision's table has no connecting
    /// edge path. No transport I/O happens here.
    pub fn plan_transition_for_rev(
        rev: Rev,
        current: NpState,
        target: NpState,
    ) -> Result<Vec<FsmTransition<CmisNetworkPathState>>, NpsmError> {
        let table = Self::fsm_for_rev(rev).ok_or(NpsmError::UnsupportedRevision(rev))?;
        let from = CmisNetworkPathState::from(current);
        let to = CmisNetworkPathState::from(target);
        openxcvr_core::fsm::plan_path(table, from, to)
            .map(|path| path.into_iter().copied().collect())
            .map_err(|_| NpsmError::UnreachableTransition {
                from: current,
                to: target,
            })
    }

    /// [`current_state`](Self::current_state), gated on a catalog revision
    /// context: a non-catalog revision fails closed with
    /// [`NpsmError::UnsupportedRevision`] before any transport I/O, and the
    /// decode runs through [`NpState::from_nibble_for_rev`] so a revision
    /// the schema does not name never borrows a catalog state.
    pub fn current_state_for_rev<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
        np_id: u8,
    ) -> Result<NpState, NpsmError> {
        validate_catalog_cmis_rev(rev)?;
        if np_id >= MAX_NP_ID {
            return Err(NpsmError::InvalidNpId(np_id));
        }
        Self::check_supported(transport)?;
        let mut buf = [0u8; 2];
        transport.read(
            I2C_ADDR_MODULE,
            NPSM_STATE_PAGE,
            NPSM_STATE_BANK,
            NPSM_STATE_OFFSET,
            &mut buf,
        )?;
        let byte = buf[(np_id / 2) as usize];
        let nibble = if np_id % 2 == 0 {
            byte & 0x0F
        } else {
            (byte >> 4) & 0x0F
        };
        Ok(NpState::from_nibble_for_rev(nibble, rev))
    }

    /// [`wait_for`](Self::wait_for), gated on a catalog revision context.
    /// The gate runs at entry — before the poll loop — so a non-catalog
    /// revision fails closed with [`NpsmError::UnsupportedRevision`] with
    /// zero I/O even when `max_polls == 0`, never falling through to
    /// `Timeout`.
    pub fn wait_for_state_for_rev<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
        np_id: u8,
        target: NpState,
        max_polls: u32,
    ) -> Result<(), NpsmError> {
        validate_catalog_cmis_rev(rev)?;
        for _ in 0..max_polls {
            if Self::current_state_for_rev(transport, rev, np_id)? == target {
                return Ok(());
            }
        }
        Err(NpsmError::Timeout)
    }

    /// Verify the module advertises NPSM support.
    pub fn check_supported<T: CmisTransport>(transport: &mut T) -> Result<(), NpsmError> {
        let mut buf = [0u8; 1];
        transport.read(
            I2C_ADDR_MODULE,
            NPSM_SUPPORT_PAGE,
            NPSM_SUPPORT_BANK,
            NPSM_SUPPORT_OFFSET,
            &mut buf,
        )?;
        if buf[0] & NPSM_SUPPORT_BIT == 0 {
            return Err(NpsmError::Unsupported);
        }
        Ok(())
    }
    /// Read one network path's current state.
    pub fn current_state<T: CmisTransport>(
        transport: &mut T,
        np_id: u8,
    ) -> Result<NpState, NpsmError> {
        if np_id >= MAX_NP_ID {
            return Err(NpsmError::InvalidNpId(np_id));
        }
        Self::check_supported(transport)?;
        let mut buf = [0u8; 2];
        transport.read(
            I2C_ADDR_MODULE,
            NPSM_STATE_PAGE,
            NPSM_STATE_BANK,
            NPSM_STATE_OFFSET,
            &mut buf,
        )?;
        let byte = buf[(np_id / 2) as usize];
        let nibble = if np_id % 2 == 0 {
            byte & 0x0F
        } else {
            (byte >> 4) & 0x0F
        };
        Ok(NpState::from_nibble(nibble))
    }
    /// Poll until the network path reports `target`, or fail with a timeout.
    pub fn wait_for<T: CmisTransport>(
        transport: &mut T,
        np_id: u8,
        target: NpState,
        max_polls: u32,
    ) -> Result<(), NpsmError> {
        for _ in 0..max_polls {
            if Self::current_state(transport, np_id)? == target {
                return Ok(());
            }
        }
        Err(NpsmError::Timeout)
    }
    /// Activate one network path and wait for Activated.
    pub fn activate<T: CmisTransport>(
        transport: &mut T,
        np_id: u8,
        max_polls: u32,
    ) -> Result<(), NpsmError> {
        Self::check_supported(transport)?;
        if np_id >= MAX_NP_ID {
            return Err(NpsmError::InvalidNpId(np_id));
        }
        transport.write(
            I2C_ADDR_MODULE,
            NPSM_STATE_PAGE,
            NPSM_STATE_BANK,
            NPSM_ACTIVATE_OFFSET,
            &[1u8 << np_id],
        )?;
        Self::wait_for(transport, np_id, NpState::Activated, max_polls)
    }
    /// Deactivate one network path and wait for Deactivated.
    pub fn deactivate<T: CmisTransport>(
        transport: &mut T,
        np_id: u8,
        max_polls: u32,
    ) -> Result<(), NpsmError> {
        Self::check_supported(transport)?;
        if np_id >= MAX_NP_ID {
            return Err(NpsmError::InvalidNpId(np_id));
        }
        let mut deinit = [0u8; 1];
        transport.read(
            I2C_ADDR_MODULE,
            NPSM_STATE_PAGE,
            NPSM_STATE_BANK,
            NPSM_DEACTIVATE_OFFSET,
            &mut deinit,
        )?;
        deinit[0] |= 1u8 << np_id;
        transport.write(
            I2C_ADDR_MODULE,
            NPSM_STATE_PAGE,
            NPSM_STATE_BANK,
            NPSM_DEACTIVATE_OFFSET,
            &deinit,
        )?;
        Self::wait_for(transport, np_id, NpState::Deactivated, max_polls)
    }
}
/// The packed MajorRevision byte for a catalog CMIS revision, or
/// `UnsupportedRevision`. Shared gate for every revision-qualified NPSM
/// entry point: the context must be CMIS-family and round-trip through
/// `Rev::from_major_byte_cmis` exactly, so a non-catalog context (3.1,
/// 5.5, a foreign family) fails closed before any I/O.
fn validate_catalog_cmis_rev(rev: Rev) -> Result<u8, NpsmError> {
    if rev.family == Family::Cmis && rev.major <= 15 && rev.minor <= 15 {
        let byte = (rev.major << 4) | rev.minor;
        if Rev::from_major_byte_cmis(byte) == Some(rev) {
            return Ok(byte);
        }
    }
    Err(NpsmError::UnsupportedRevision(rev))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::transport::Transport;

    // ── Generated-enum dedup + revision-aware planning ──

    #[test]
    fn npstate_conversions_are_lossless_on_raw_values() {
        for raw in 0u8..=255 {
            let adapter = NpState::Unknown(raw);
            assert_eq!(
                NpState::from(CmisNetworkPathState::from(adapter)),
                adapter,
                "raw {raw} must round-trip losslessly"
            );
        }
        let pairs = [
            (NpState::Deactivated, CmisNetworkPathState::NpDeactivated),
            (NpState::Init, CmisNetworkPathState::NpInit),
            (NpState::Deinit, CmisNetworkPathState::NpDeinit),
            (NpState::Activated, CmisNetworkPathState::NpActivated),
        ];
        for (adapter, generated) in pairs {
            assert_eq!(CmisNetworkPathState::from(adapter), generated);
            assert_eq!(NpState::from(generated), adapter);
        }
        // The schema's since-5.x Tx/parking states narrow to Unknown
        // (value-preserving: the raw repr round-trips); an adapter Unknown
        // is never laundered back into a generated named variant.
        for (generated, raw) in [
            (CmisNetworkPathState::NpTxTurnOn, 5u8),
            (CmisNetworkPathState::NpTxTurnOff, 6),
            (CmisNetworkPathState::NpInitialized, 7),
        ] {
            assert_eq!(NpState::from(generated), NpState::Unknown(raw));
            let widened = CmisNetworkPathState::from(NpState::Unknown(raw));
            assert_eq!(widened, CmisNetworkPathState::Unknown(raw));
            assert_eq!(widened.repr(), generated.repr());
        }
        assert_eq!(
            NpState::from(CmisNetworkPathState::Reserved0),
            NpState::Unknown(0)
        );
        assert_eq!(
            CmisNetworkPathState::from(NpState::Unknown(0)),
            CmisNetworkPathState::Reserved0
        );
    }

    #[test]
    fn from_nibble_for_rev_fails_closed_off_catalog() {
        for nibble in 0u8..=15 {
            for rev in [Rev::CMIS_3_0, Rev::CMIS_5_3, Rev::CMIS_5_4] {
                assert_eq!(
                    NpState::from_nibble_for_rev(nibble, rev),
                    NpState::from_nibble(nibble),
                    "nibble {nibble} at {rev:?}"
                );
            }
        }
        let rev = Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        };
        assert_eq!(NpState::from_nibble_for_rev(0x23, rev), NpState::Unknown(3));
        let sff = Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        };
        assert_eq!(NpState::from_nibble_for_rev(0x23, sff), NpState::Unknown(3));
    }

    #[test]
    fn npsm_plan_transition_follows_generated_edges() {
        // Deactivated -> Activated routes through the generated graph:
        // Deactivated -> Init -> Initialized -> TxTurnOn -> Activated.
        let plan =
            Npsm::plan_transition_for_rev(Rev::CMIS_5_3, NpState::Deactivated, NpState::Activated)
                .expect("a path must exist at 5.3");
        assert!(!plan.is_empty());
        assert_eq!(plan[0].from, CmisNetworkPathState::NpDeactivated);
        assert_eq!(plan.last().unwrap().to, CmisNetworkPathState::NpActivated);
        for pair in plan.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
        // The adapter's four named states are nodes of the generated graph.
        let table = Npsm::fsm_for_rev(Rev::CMIS_5_3).expect("catalog rev");
        assert_eq!(table.initial, CmisNetworkPathState::NpDeactivated);
    }

    #[test]
    fn npsm_plan_transition_fails_closed() {
        let rev = Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        };
        assert!(matches!(
            Npsm::plan_transition_for_rev(rev, NpState::Deactivated, NpState::Activated),
            Err(NpsmError::UnsupportedRevision(_))
        ));
        assert!(matches!(
            Npsm::plan_transition_for_rev(Rev::CMIS_5_3, NpState::Unknown(9), NpState::Activated),
            Err(NpsmError::UnreachableTransition { .. })
        ));
    }

    struct MockTransport {
        page_01h: [u8; 256],
        page_16h: [u8; 256],
        poll_count: u32,
        advance_after: u32,
        target_np_state_byte: u8,
        writes: Vec<(u8, Vec<u8>)>,
    }
    impl MockTransport {
        fn new() -> Self {
            Self {
                page_01h: [0u8; 256],
                page_16h: [0u8; 256],
                poll_count: 0,
                advance_after: u32::MAX,
                target_np_state_byte: 0,
                writes: Vec::new(),
            }
        }
        fn supported() -> Self {
            let mut transport = Self::new();
            transport.page_01h[NPSM_SUPPORT_OFFSET as usize] = NPSM_SUPPORT_BIT;
            transport
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
            assert_eq!(bank, NPSM_STATE_BANK);
            match (page, offset) {
                (NPSM_SUPPORT_PAGE, NPSM_SUPPORT_OFFSET) => {
                    assert_eq!(buf.len(), 1);
                    buf.copy_from_slice(
                        &self.page_01h[offset as usize..offset as usize + buf.len()],
                    );
                }
                (NPSM_STATE_PAGE, NPSM_STATE_OFFSET) => {
                    assert_eq!(buf.len(), 2);
                    let state_byte = if self.poll_count >= self.advance_after {
                        self.target_np_state_byte
                    } else {
                        self.page_16h[offset as usize]
                    };
                    self.poll_count += 1;
                    buf.copy_from_slice(&[state_byte, self.page_16h[offset as usize + 1]]);
                }
                (NPSM_STATE_PAGE, NPSM_DEACTIVATE_OFFSET) => {
                    buf[0] = self.page_16h[offset as usize]
                }
                _ => panic!("unexpected read page=0x{page:02x} offset={offset}"),
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
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(page, NPSM_STATE_PAGE);
            assert_eq!(bank, NPSM_STATE_BANK);
            self.page_16h[offset as usize..offset as usize + data.len()].copy_from_slice(data);
            self.writes.push((offset, data.to_vec()));
            Ok(())
        }
        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }
    impl CmisTransport for MockTransport {}
    #[test]
    fn test_check_supported_ok() {
        let mut transport = MockTransport::supported();
        assert!(Npsm::check_supported(&mut transport).is_ok());
    }
    #[test]
    fn test_check_unsupported() {
        let mut transport = MockTransport::new();
        assert!(matches!(
            Npsm::check_supported(&mut transport),
            Err(NpsmError::Unsupported)
        ));
    }
    #[test]
    fn test_current_state_np0_np1() {
        let mut transport = MockTransport::supported();
        transport.page_16h[NPSM_STATE_OFFSET as usize] = 0x41;
        assert_eq!(
            Npsm::current_state(&mut transport, 0).unwrap(),
            NpState::Deactivated
        );
        assert_eq!(
            Npsm::current_state(&mut transport, 1).unwrap(),
            NpState::Activated
        );
    }
    #[test]
    fn test_wait_for_activated() {
        let mut transport = MockTransport::supported();
        transport.page_16h[NPSM_STATE_OFFSET as usize] = 0x02;
        transport.advance_after = 3;
        transport.target_np_state_byte = 0x04;
        assert!(Npsm::wait_for(&mut transport, 0, NpState::Activated, 4).is_ok());
    }
    #[test]
    fn test_activate_writes_apply_mask_at_byte_176() {
        let mut transport = MockTransport::supported();
        transport.page_16h[NPSM_STATE_OFFSET as usize] = 0x40;
        Npsm::activate(&mut transport, 1, 1).unwrap();
        assert_eq!(transport.writes, vec![(176, vec![0x02])]);
    }
    #[test]
    fn test_deactivate_preserves_other_lane_bits_at_byte_160() {
        let mut transport = MockTransport::supported();
        transport.page_16h[160] = 0x81;
        transport.page_16h[NPSM_STATE_OFFSET as usize] = 0x10;
        Npsm::deactivate(&mut transport, 1, 1).unwrap();
        assert_eq!(transport.writes, vec![(160, vec![0x83])]);
    }
    #[test]
    fn test_invalid_np_id() {
        let mut transport = MockTransport::supported();
        assert!(matches!(
            Npsm::current_state(&mut transport, 4),
            Err(NpsmError::InvalidNpId(4))
        ));
    }
}
