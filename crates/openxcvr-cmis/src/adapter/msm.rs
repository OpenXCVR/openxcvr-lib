use std::{error::Error, fmt};

use openxcvr_core::field_catalog::{self, FieldQuery, LaneSelection};
use openxcvr_core::fsm::{
    FsmEvent, FsmScalar, FsmSideEffect, FsmStateChangeOp, FsmTable, FsmTimerDuration,
    FsmTransition, GuardContext,
};
use openxcvr_core::types::management::Range;
use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE, Family, Rev};

use crate::generated::fsm::cmis_module_state_machine::{self, CmisModuleStateSpace};
use crate::generated::CmisModuleState;
use crate::module::CmisTransport;

const LOWER_PAGE: u8 = 0x00;
const LOWER_BANK: u8 = 0;
const MODULE_STATE_OFFSET: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The module state machine's state (lower-memory byte 3, bits 3:1).
pub enum ModuleState {
    /// The module is in low power.
    LowPwr,
    /// The module is powering up.
    PwrUp,
    /// The module is ready.
    Ready,
    /// The module is powering down.
    PwrDn,
    /// The module is in a fault state.
    Fault,
    /// A reserved code the revision does not name, carried raw.
    Unknown(u8),
}

impl ModuleState {
    /// Decodes CMIS lower-memory byte 3 as read; ModuleState is bits 3:1.
    pub fn from_register(raw: u8) -> Self {
        match (raw >> 1) & 0x07 {
            0x1 => Self::LowPwr,
            0x2 => Self::PwrUp,
            0x3 => Self::Ready,
            0x4 => Self::PwrDn,
            0x5 => Self::Fault,
            other => Self::Unknown(other),
        }
    }

    /// Decodes CMIS lower-memory byte 3 for a specific catalog revision
    /// context, through the generated `CmisModuleState` decode.
    ///
    /// Fail closed, mirroring [`DpState::from_nibble_for_rev`](super::DpState::from_nibble_for_rev):
    /// the context must be CMIS-family with both version components
    /// packable into a MajorRevision byte, and that packed byte must
    /// round-trip through `Rev::from_major_byte_cmis` back to exactly this
    /// `rev` — so 3.1/4.1/5.5 or any other non-catalog context never
    /// borrows a catalog decode and returns `Unknown(bits 3:1)`, the raw
    /// state field preserved losslessly.
    pub fn from_register_for_rev(raw: u8, rev: Rev) -> Self {
        let field = (raw >> 1) & 0x07;
        if rev.family == Family::Cmis && rev.major <= 15 && rev.minor <= 15 {
            let byte = (rev.major << 4) | rev.minor;
            if Rev::from_major_byte_cmis(byte) == Some(rev) {
                return CmisModuleState::from_rev(byte, field).into();
            }
        }
        Self::Unknown(field)
    }
}

// ── Generated-enum deduplication ──────────────────────────────────────
//
// `ModuleState` keeps its long-public variant names so every existing
// consumer keeps compiling; the lossless conversions below make the
// generated `CmisModuleState` and this enum one semantic identity. Raw
// values round-trip exactly: the schema-named `Reserved0` (raw 0) maps to
// `Unknown(0)` and back — value-preserving, never a named-state
// invention.

impl From<CmisModuleState> for ModuleState {
    fn from(state: CmisModuleState) -> Self {
        match state {
            CmisModuleState::Reserved0 => Self::Unknown(0),
            CmisModuleState::ModuleLowPwr => Self::LowPwr,
            CmisModuleState::ModulePwrUp => Self::PwrUp,
            CmisModuleState::ModuleReady => Self::Ready,
            CmisModuleState::ModulePwrDn => Self::PwrDn,
            CmisModuleState::ModuleFault => Self::Fault,
            CmisModuleState::Unknown(v) => Self::Unknown(v),
        }
    }
}

impl From<ModuleState> for CmisModuleState {
    fn from(state: ModuleState) -> Self {
        match state {
            ModuleState::LowPwr => Self::ModuleLowPwr,
            ModuleState::PwrUp => Self::ModulePwrUp,
            ModuleState::Ready => Self::ModuleReady,
            ModuleState::PwrDn => Self::ModulePwrDn,
            ModuleState::Fault => Self::ModuleFault,
            ModuleState::Unknown(0) => Self::Reserved0,
            ModuleState::Unknown(v) => Self::Unknown(v),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
/// Why an MSM operation failed.
pub enum MsmError {
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The module is not in one of the required states.
    WrongState {
        /// The states the operation requires.
        required: Vec<ModuleState>,
        /// The state actually read.
        actual: ModuleState,
    },
    /// The poll budget expired before the target state arrived.
    Timeout,
    /// The revision context is not one of the CMIS catalog revisions
    /// (3.0, 4.0, 5.0, 5.1, 5.2, 5.3, 5.4). Every revision-qualified MSM
    /// entry point fails closed with this before any transport I/O rather
    /// than silently borrowing a catalog state machine.
    UnsupportedRevision(Rev),
    /// The requested target state cannot be reached from the current state
    /// through the revision's generated behaviour table. Pure plan refusal;
    /// never a transport outcome.
    UnreachableTransition {
        /// The state the transition would leave.
        from: ModuleState,
        /// The state that cannot be reached.
        to: ModuleState,
    },
    /// A matched transition's recorded side effect cannot be resolved to a
    /// planned effect at this revision: a target field the schema-derived
    /// catalog does not carry, or a symbolic value the machine's states do
    /// not name. Fail closed: nothing is executed and the whole step is
    /// refused, never partially planned.
    UnresolvedEffect {
        /// Why the side effect could not be resolved.
        detail: String,
    },
}

impl fmt::Display for MsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "MSM transport error: {err}"),
            Self::WrongState { required, actual } => {
                write!(
                    f,
                    "MSM wrong state: required {required:?}, actual {actual:?}"
                )
            }
            Self::Timeout => write!(f, "MSM timed out"),
            Self::UnsupportedRevision(rev) => {
                write!(f, "unsupported MSM CMIS revision {rev:?}")
            }
            Self::UnreachableTransition { from, to } => {
                write!(f, "no MSM transition path from {from:?} to {to:?}")
            }
            Self::UnresolvedEffect { detail } => {
                write!(f, "MSM side effect cannot be resolved to a plan: {detail}")
            }
        }
    }
}

impl Error for MsmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ModuleError> for MsmError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

/// Module state-machine helper over any CMIS transport.
pub struct Msm;

impl Msm {
    /// The generated behaviour table for `rev`, or
    /// `None` for any non-catalog or non-CMIS revision context. Fail
    /// closed: callers must refuse to operate under an unsupported
    /// revision, never borrow the nearest table.
    ///
    /// The table runs over the generated machine-space enum
    /// [`CmisModuleStateSpace`]: the complete Module State Machine space
    /// (CMIS 5.3 § 6.3.2.5), reported states plus the unreported
    /// Resetting/Reset/MgmtInit states the state register cannot carry.
    /// [`CmisModuleStateSpace::reported`] exposes the register value of a
    /// reported state; unreported states have none.
    pub fn fsm_for_rev(rev: Rev) -> Option<&'static FsmTable<CmisModuleStateSpace>> {
        cmis_module_state_machine::for_revision(rev)
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
    /// edge path — including a raw/reserved register state, which is not a
    /// node of the generated graph. No transport I/O happens here.
    pub fn plan_transition_for_rev(
        rev: Rev,
        current: ModuleState,
        target: ModuleState,
    ) -> Result<Vec<FsmTransition<CmisModuleStateSpace>>, MsmError> {
        let table = Self::fsm_for_rev(rev).ok_or(MsmError::UnsupportedRevision(rev))?;
        let from = CmisModuleStateSpace::from_reported(CmisModuleState::from(current)).ok_or(
            MsmError::UnreachableTransition {
                from: current,
                to: target,
            },
        )?;
        let to = CmisModuleStateSpace::from_reported(CmisModuleState::from(target)).ok_or(
            MsmError::UnreachableTransition {
                from: current,
                to: target,
            },
        )?;
        openxcvr_core::fsm::plan_path(table, from, to)
            .map(|path| path.into_iter().copied().collect())
            .map_err(|_| MsmError::UnreachableTransition {
                from: current,
                to: target,
            })
    }

    /// Pure, revision-aware step of the behaviour table over one observed
    /// trigger: the matched edge's destination plus its side effects,
    /// resolved to a [`PlannedEffect`] plan the caller may execute through
    /// the transport it owns. Nothing here reads, writes, starts or cancels
    /// anything — the plan is data.
    ///
    /// The trigger and the guard facts are caller-supplied
    /// ([`FsmEvent`], [`GuardContext`]): guards depend on live module facts
    /// only the caller observes. Fail closed on every axis:
    /// [`MsmError::UnsupportedRevision`] for a non-catalog revision before
    /// any resolution; [`StepTransition::NoMatch`] when no edge matches
    /// (the machine stays put, nothing is planned);
    /// [`MsmError::UnresolvedEffect`] when a matched edge records an effect
    /// that cannot be resolved at this revision — the step is refused whole,
    /// never partially planned.
    pub fn step_transition_for_rev(
        rev: Rev,
        current: CmisModuleStateSpace,
        event: FsmEvent<'_>,
        ctx: &dyn GuardContext,
    ) -> Result<StepTransition, MsmError> {
        let table = Self::fsm_for_rev(rev).ok_or(MsmError::UnsupportedRevision(rev))?;
        match table.evaluate(current, event, ctx) {
            openxcvr_core::fsm::FsmStep::NoMatch => Ok(StepTransition::NoMatch),
            openxcvr_core::fsm::FsmStep::Advance { to, transition } => {
                let effects = transition
                    .side_effects
                    .iter()
                    .map(|effect| resolve_effect(rev, effect))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(StepTransition::Advance { to, effects })
            }
        }
    }

    /// [`current_state`](Self::current_state), gated on a catalog revision
    /// context: a non-catalog revision fails closed with
    /// [`MsmError::UnsupportedRevision`] before any transport I/O, and the
    /// decode runs through [`ModuleState::from_register_for_rev`] so a
    /// revision the schema does not name never borrows a catalog state.
    pub fn current_state_for_rev<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
    ) -> Result<ModuleState, MsmError> {
        validate_catalog_cmis_rev(rev)?;
        let mut buf = [0u8; 1];
        transport
            .read(
                I2C_ADDR_MODULE,
                LOWER_PAGE,
                LOWER_BANK,
                MODULE_STATE_OFFSET,
                &mut buf,
            )
            .map_err(MsmError::Transport)?;
        Ok(ModuleState::from_register_for_rev(buf[0], rev))
    }

    /// [`wait_for`](Self::wait_for), gated on a catalog revision context.
    /// The gate runs at entry — before the poll loop — so a non-catalog
    /// revision fails closed with [`MsmError::UnsupportedRevision`] with
    /// zero I/O even when `max_polls == 0`, never falling through to
    /// `Timeout`.
    pub fn wait_for_state_for_rev<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
        target: ModuleState,
        max_polls: u32,
    ) -> Result<(), MsmError> {
        validate_catalog_cmis_rev(rev)?;
        for _ in 0..max_polls {
            if Self::current_state_for_rev(transport, rev)? == target {
                return Ok(());
            }
        }
        Err(MsmError::Timeout)
    }

    /// Read the module's current state from lower-memory byte 3.
    pub fn current_state<T: CmisTransport>(transport: &mut T) -> Result<ModuleState, MsmError> {
        let mut buf = [0u8; 1];
        transport
            .read(
                I2C_ADDR_MODULE,
                LOWER_PAGE,
                LOWER_BANK,
                MODULE_STATE_OFFSET,
                &mut buf,
            )
            .map_err(MsmError::Transport)?;
        Ok(ModuleState::from_register(buf[0]))
    }

    /// Poll until the module reports `target`, or fail with a timeout.
    pub fn wait_for<T: CmisTransport>(
        transport: &mut T,
        target: ModuleState,
        max_polls: u32,
    ) -> Result<(), MsmError> {
        for _ in 0..max_polls {
            if Self::current_state(transport)? == target {
                return Ok(());
            }
        }
        Err(MsmError::Timeout)
    }

    /// Read the current state and require it to be one of the listed states.
    pub fn assert_in<T: CmisTransport>(
        transport: &mut T,
        required_states: &[ModuleState],
    ) -> Result<ModuleState, MsmError> {
        let state = Self::current_state(transport)?;
        if required_states.contains(&state) {
            Ok(state)
        } else {
            Err(MsmError::WrongState {
                required: required_states.to_vec(),
                actual: state,
            })
        }
    }
}

// ── Pure step_transition: trigger → planned effects as data ────────────

/// One planned register write, resolved from a matched transition's recorded
/// side effect against the schema-derived field catalog. A read-modify-write
/// of the `mask` bits at `offset` inside the page window: the caller applies
/// it through the transport it owns, preserving every bit outside `mask`.
/// Never executed by the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedRegisterWrite {
    /// Seven-bit I2C address of the page window (from the field's schema
    /// location, not a hand-pasted constant).
    pub address: u8,
    /// Page selector of the window (0 for the lower bank).
    pub page: u8,
    /// Bank selector of the window (0 for unbanked pages).
    pub bank: u8,
    /// Flat byte offset within the 256-byte window.
    pub offset: u8,
    /// Bits this write owns, positioned at their register offset.
    pub mask: u8,
    /// The value to write into the masked bits, positioned the same way.
    pub value: u8,
}

/// One planned effect of a matched transition, as data. Timers, log events
/// and machine spawn/terminate ride along exactly as the schema recorded
/// them; the daemon owns every timer, log sink and machine instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedEffect {
    /// A register write the caller may apply through its transport.
    RegisterWrite(PlannedRegisterWrite),
    /// A standard timer to start, with the schema-recorded duration source.
    StartTimer {
        /// The timer's schema name.
        timer: &'static str,
        /// Where the duration comes from (fixed, or a page 01h advertised
        /// field the caller reads).
        duration: FsmTimerDuration,
    },
    /// A timer to cancel.
    CancelTimer {
        /// The timer's schema name.
        timer: &'static str,
    },
    /// An event the schema asks to log.
    LogEvent {
        /// The schema-recorded event id.
        event_id: &'static str,
    },
    /// A machine the transition spawns (e.g. the Data Path machine at
    /// ModuleReady entry).
    SpawnMachine {
        /// The spawned machine's schema id.
        machine: &'static str,
    },
    /// A machine the transition terminates.
    TerminateMachine {
        /// The terminated machine's schema id.
        machine: &'static str,
    },
}

/// What evaluating one observed trigger against the revision's behaviour
/// table produced, with the matched edge's effects resolved to a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepTransition {
    /// No edge from `current` matches the trigger under the supplied guard
    /// facts. The machine stays where it was; nothing is planned.
    NoMatch,
    /// The matched edge's destination and its resolved effect plan. The
    /// caller decides whether to execute; executing is not the library's.
    Advance {
        /// The state the machine advances to (machine space: unreported
        /// reset phases included).
        to: CmisModuleStateSpace,
        /// The edge's effects, resolved against the revision's catalog.
        effects: Vec<PlannedEffect>,
    },
}

/// Resolve one schema-recorded side effect to a planned effect at `rev`.
/// Fail closed: an unresolvable effect refuses the whole step, never a
/// partial plan.
fn resolve_effect(rev: Rev, effect: &FsmSideEffect) -> Result<PlannedEffect, MsmError> {
    match effect {
        FsmSideEffect::StateChange {
            target,
            operation,
            value,
        } => {
            let write = resolve_state_change(rev, target, *operation, value.as_ref())?;
            Ok(PlannedEffect::RegisterWrite(write))
        }
        FsmSideEffect::LogEvent { event_id } => Ok(PlannedEffect::LogEvent { event_id }),
        FsmSideEffect::StartTimer { timer, duration } => Ok(PlannedEffect::StartTimer {
            timer,
            duration: *duration,
        }),
        FsmSideEffect::CancelTimer { timer } => Ok(PlannedEffect::CancelTimer { timer }),
        FsmSideEffect::SpawnMachine { machine } => Ok(PlannedEffect::SpawnMachine { machine }),
        FsmSideEffect::TerminateMachine { machine } => {
            Ok(PlannedEffect::TerminateMachine { machine })
        }
    }
}

/// Resolve a recorded `state_change` effect into a planned register write
/// through the schema-derived field catalog: the target's path
/// names a lower-memory field (the only register surface this machine's
/// effects address), and the catalog supplies the window, byte offset and
/// bit placement — nothing is hand-pasted.
fn resolve_state_change(
    rev: Rev,
    target: &str,
    operation: FsmStateChangeOp,
    value: Option<&FsmScalar>,
) -> Result<PlannedRegisterWrite, MsmError> {
    let unresolved = |detail: String| MsmError::UnresolvedEffect { detail };
    let Some(field) = target.strip_prefix("lower_memory.") else {
        return Err(unresolved(format!(
            "state_change target {target} is not a lower_memory field"
        )));
    };
    let query = FieldQuery {
        pattern: field,
        lane: LaneSelection::Unspecified,
        page: Some("lower"),
        bank: 0,
    };
    let entry = field_catalog::resolve(&crate::generated::catalog::CATALOG, rev, &query)
        .map_err(|err| unresolved(format!("catalog resolution of {target} at {rev:?}: {err}")))?
        .pop()
        .ok_or_else(|| unresolved(format!("catalog resolution of {target} returned no entry")))?;
    let width = entry.width_bits();
    if width == 0 || width > 8 {
        return Err(unresolved(format!(
            "field {target} is {width} bits wide; a single-byte planned write cannot carry it"
        )));
    }
    let bit_offset = u32::from(entry.bit_offset);
    if bit_offset + u32::from(width) > 8 {
        return Err(unresolved(format!(
            "field {target} spans bits {bit_offset}..{} of its byte; a planned write cannot cross a byte boundary",
            bit_offset + u32::from(width)
        )));
    }
    let range: Range = entry.range();
    let raw_bits = match (operation, value) {
        (FsmStateChangeOp::Assign, Some(scalar)) => match *scalar {
            FsmScalar::Bool(bit) => {
                if width != 1 {
                    return Err(unresolved(format!(
                        "bool value assigned to {width}-bit field {target}"
                    )));
                }
                u8::from(bit)
            }
            FsmScalar::Int(int) => {
                if int < 0 || u64::try_from(int).unwrap_or(u64::MAX) >> u64::from(width) != 0 {
                    return Err(unresolved(format!(
                        "integer {int} does not fit the {width}-bit field {target}"
                    )));
                }
                int as u8
            }
            FsmScalar::Symbol(name) => {
                let state = CmisModuleStateSpace::from_schema_name(name).ok_or_else(|| {
                    unresolved(format!(
                        "symbolic value {name} is not a state of this machine"
                    ))
                })?;
                let reported = state.reported().ok_or_else(|| {
                    unresolved(format!(
                        "symbolic value {name} names an unreported state; a state register write cannot carry it"
                    ))
                })?;
                let repr = reported.repr();
                if u64::from(repr) >> u64::from(width) != 0 {
                    return Err(unresolved(format!(
                        "state {name} encodes {repr}, which does not fit the {width}-bit field {target}"
                    )));
                }
                repr
            }
        },
        (FsmStateChangeOp::Assign, None) => {
            return Err(unresolved(format!(
                "assign to {target} without a recorded value"
            )))
        }
        (FsmStateChangeOp::Set, _) | (FsmStateChangeOp::Clear, _) => {
            if width != 1 {
                return Err(unresolved(format!(
                    "set/clear on the {width}-bit field {target}; only a 1-bit field can be set or cleared"
                )));
            }
            u8::from(operation == FsmStateChangeOp::Set)
        }
    };
    let mask = (((1u16 << width) - 1) as u8) << bit_offset;
    Ok(PlannedRegisterWrite {
        address: range.address,
        page: range.page,
        bank: range.bank,
        offset: range.offset,
        mask,
        value: raw_bits << bit_offset,
    })
}

/// The packed MajorRevision byte for a catalog CMIS revision, or
/// `UnsupportedRevision`. Shared gate for every revision-qualified MSM entry
/// point: the context must be CMIS-family and round-trip through
/// `Rev::from_major_byte_cmis` exactly, so a non-catalog context (3.1,
/// 5.5, a foreign family) fails closed before any I/O.
fn validate_catalog_cmis_rev(rev: Rev) -> Result<u8, MsmError> {
    if rev.family == Family::Cmis && rev.major <= 15 && rev.minor <= 15 {
        let byte = (rev.major << 4) | rev.minor;
        if Rev::from_major_byte_cmis(byte) == Some(rev) {
            return Ok(byte);
        }
    }
    Err(MsmError::UnsupportedRevision(rev))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{error::ModuleError, transport::Transport};

    struct MockTransport {
        lower: [u8; 128],
        poll_count: u32,
        advance_after: u32,
        initial_state: u8,
        target_state: u8,
    }

    impl MockTransport {
        fn new(initial_state: u8, advance_after: u32, target_state: u8) -> Self {
            let mut lower = [0u8; 128];
            lower[MODULE_STATE_OFFSET as usize] = initial_state;
            Self {
                lower,
                poll_count: 0,
                advance_after,
                initial_state,
                target_state,
            }
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
            assert_eq!(bank, LOWER_BANK);

            if page != LOWER_PAGE {
                return Err(ModuleError::Io(
                    format!("unexpected page 0x{page:02x}").into(),
                ));
            }

            if offset == MODULE_STATE_OFFSET {
                self.poll_count += 1;
                if self.poll_count > self.advance_after {
                    self.lower[MODULE_STATE_OFFSET as usize] = self.target_state;
                }
            }

            let start = offset as usize;
            buf.copy_from_slice(&self.lower[start..start + buf.len()]);
            Ok(())
        }

        fn write(
            &mut self,
            _slave_addr: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            _data: &[u8],
        ) -> Result<(), ModuleError> {
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_wait_for_module_ready_advances_on_poll() {
        // Byte 3 = PwrUp (2 << 1) then Ready (3 << 1) with interrupt-deasserted bit 0 set.
        let mut transport = MockTransport::new(0x04, 3, 0x07);

        let result = Msm::wait_for(&mut transport, ModuleState::Ready, 100);

        assert!(result.is_ok());
        assert_eq!(transport.poll_count, transport.advance_after + 1);
        assert_eq!(transport.initial_state, 0x04);
    }

    #[test]
    fn test_assert_in_wrong_state() {
        // Byte 3 = LowPwr (1 << 1) with interrupt-deasserted bit 0 set.
        let mut transport = MockTransport::new(0x03, u32::MAX, 0x03);

        let result = Msm::assert_in(&mut transport, &[ModuleState::Ready]);

        match result {
            Err(MsmError::WrongState { required, actual }) => {
                assert_eq!(required, &[ModuleState::Ready][..]);
                assert_eq!(actual, ModuleState::LowPwr);
            }
            other => panic!("expected WrongState, got {other:?}"),
        }
    }

    #[test]
    fn test_current_state_decodes_byte3_bits_3_to_1() {
        for state in 0u8..=7 {
            let expected = match state {
                1 => ModuleState::LowPwr,
                2 => ModuleState::PwrUp,
                3 => ModuleState::Ready,
                4 => ModuleState::PwrDn,
                5 => ModuleState::Fault,
                other => ModuleState::Unknown(other),
            };
            for int_deasserted in [0u8, 1] {
                for upper in [0x00u8, 0xF0] {
                    let raw = upper | (state << 1) | int_deasserted;
                    let mut transport = MockTransport::new(raw, u32::MAX, raw);

                    let actual = Msm::current_state(&mut transport);

                    assert_eq!(actual.unwrap(), expected, "byte 3 = 0x{raw:02x}");
                }
            }
        }
    }

    #[test]
    fn test_current_state_real_module_low_power_bytes() {
        for raw in [0x03u8, 0x02] {
            let mut transport = MockTransport::new(raw, u32::MAX, raw);

            let actual = Msm::current_state(&mut transport);

            assert_eq!(actual.unwrap(), ModuleState::LowPwr, "byte 3 = 0x{raw:02x}");
        }
    }

    // ── Generated-enum dedup + revision-aware planning ──

    #[test]
    fn module_state_conversions_are_lossless_on_raw_values() {
        for raw in 0u8..=255 {
            let adapter = ModuleState::Unknown(raw);
            assert_eq!(
                ModuleState::from(CmisModuleState::from(adapter)),
                adapter,
                "raw {raw} must round-trip losslessly"
            );
        }
        let pairs = [
            (ModuleState::LowPwr, CmisModuleState::ModuleLowPwr),
            (ModuleState::PwrUp, CmisModuleState::ModulePwrUp),
            (ModuleState::Ready, CmisModuleState::ModuleReady),
            (ModuleState::PwrDn, CmisModuleState::ModulePwrDn),
            (ModuleState::Fault, CmisModuleState::ModuleFault),
        ];
        for (adapter, generated) in pairs {
            assert_eq!(CmisModuleState::from(adapter), generated);
            assert_eq!(ModuleState::from(generated), adapter);
        }
        assert_eq!(
            ModuleState::from(CmisModuleState::Reserved0),
            ModuleState::Unknown(0)
        );
        assert_eq!(
            CmisModuleState::from(ModuleState::Unknown(0)),
            CmisModuleState::Reserved0
        );
    }

    #[test]
    fn from_register_for_rev_fails_closed_off_catalog() {
        // The five named encodings decode identically at every catalog rev.
        for field in [1u8, 2, 3, 4, 5] {
            let raw = field << 1;
            for rev in [Rev::CMIS_3_0, Rev::CMIS_4_0, Rev::CMIS_5_0, Rev::CMIS_5_4] {
                assert_eq!(
                    ModuleState::from_register_for_rev(raw, rev),
                    ModuleState::from_register(raw),
                    "field {field} at {rev:?}"
                );
            }
        }
        // Off-catalog contexts never borrow a catalog decode: the raw state
        // field (bits 3:1) is preserved losslessly in Unknown.
        let rev = Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        };
        assert_eq!(
            ModuleState::from_register_for_rev(0x07, rev),
            ModuleState::Unknown(3)
        );
        let sff = Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        };
        assert_eq!(
            ModuleState::from_register_for_rev(0x07, sff),
            ModuleState::Unknown(3)
        );
    }

    #[test]
    fn msm_plan_transition_follows_generated_edges() {
        // LowPwr -> Ready must route through PwrUp in the generated graph.
        let plan =
            Msm::plan_transition_for_rev(Rev::CMIS_5_3, ModuleState::LowPwr, ModuleState::Ready)
                .expect("a path must exist at 5.3");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].from, CmisModuleStateSpace::ModuleLowPwr);
        assert_eq!(plan[0].to, CmisModuleStateSpace::ModulePwrUp);
        assert_eq!(plan[1].from, CmisModuleStateSpace::ModulePwrUp);
        assert_eq!(plan[1].to, CmisModuleStateSpace::ModuleReady);
        // Guards and side effects ride along as data.
        assert!(!plan[0].guards.is_empty());
        assert!(plan[0]
            .side_effects
            .iter()
            .any(|e| matches!(e, openxcvr_core::fsm::FsmSideEffect::StartTimer { .. })));
        // The terminal exit of ModuleFault is recorded in the table.
        let table = Msm::fsm_for_rev(Rev::CMIS_5_3).expect("catalog rev");
        assert_eq!(
            table.terminal_exit(CmisModuleStateSpace::ModuleFault),
            Some(openxcvr_core::fsm::FsmTerminalExit::ManagementReset)
        );
        // The machine space models the unreported reset phases and the
        // reported register values both ways, fail closed for the rest.
        assert_eq!(
            CmisModuleStateSpace::Reset.reported(),
            None,
            "Reset is not reported in the state register"
        );
        assert_eq!(
            CmisModuleStateSpace::MgmtInit.reported(),
            None,
            "MgmtInit is not reported in the state register"
        );
        assert_eq!(
            CmisModuleStateSpace::ModuleLowPwr.reported(),
            Some(CmisModuleState::ModuleLowPwr)
        );
        assert_eq!(
            CmisModuleStateSpace::from_reported(CmisModuleState::ModuleReady),
            Some(CmisModuleStateSpace::ModuleReady)
        );
        assert_eq!(
            CmisModuleStateSpace::from_reported(CmisModuleState::Unknown(6)),
            None,
            "a reserved encoding never borrows a machine state"
        );
        // The reset path exists through the unreported states: Fault is
        // recovered through Resetting/Reset/MgmtInit back to ModuleLowPwr.
        let reset_plan =
            Msm::plan_transition_for_rev(Rev::CMIS_5_3, ModuleState::Fault, ModuleState::LowPwr)
                .expect("the reset path must exist at 5.3");
        assert_eq!(reset_plan.len(), 4);
        assert_eq!(reset_plan[0].to, CmisModuleStateSpace::Resetting);
        assert_eq!(reset_plan[1].to, CmisModuleStateSpace::Reset);
        assert_eq!(reset_plan[2].to, CmisModuleStateSpace::MgmtInit);
        assert_eq!(reset_plan[3].to, CmisModuleStateSpace::ModuleLowPwr);
        // The schema's host policy values are emitted beside the tables.
        assert_eq!(cmis_module_state_machine::HOST_POLICY_POLL_CADENCE_MS, 50);
        assert_eq!(
            cmis_module_state_machine::HOST_POLICY_READINESS_BACKSTOP_MS,
            10_000
        );
    }

    #[test]
    fn msm_plan_transition_fails_closed() {
        let rev = Rev {
            family: Family::Cmis,
            major: 4,
            minor: 1,
        };
        assert!(matches!(
            Msm::plan_transition_for_rev(rev, ModuleState::LowPwr, ModuleState::Ready),
            Err(MsmError::UnsupportedRevision(_))
        ));
        // A raw reserved state is not a node of the generated graph.
        assert!(matches!(
            Msm::plan_transition_for_rev(
                Rev::CMIS_5_3,
                ModuleState::Unknown(6),
                ModuleState::Ready
            ),
            Err(MsmError::UnreachableTransition { .. })
        ));
    }

    // ── step_transition: trigger → planned effects as data ──────────────

    /// Guard facts over the lower-memory booleans the MSM edges test.
    struct StepCtx {
        allow_hw: Option<bool>,
        request_sw: Option<bool>,
    }

    impl openxcvr_core::fsm::GuardContext for StepCtx {
        fn field(&self, path: &str) -> Option<openxcvr_core::fsm::FsmValue<'_>> {
            match path {
                "lower_memory.low_pwr_allow_request_hw" => {
                    self.allow_hw.map(openxcvr_core::fsm::FsmValue::Bool)
                }
                "lower_memory.low_pwr_request_sw" => {
                    self.request_sw.map(openxcvr_core::fsm::FsmValue::Bool)
                }
                _ => None,
            }
        }
    }

    fn step(
        current: CmisModuleStateSpace,
        event: openxcvr_core::fsm::FsmEvent<'_>,
        ctx: &StepCtx,
    ) -> Result<StepTransition, MsmError> {
        Msm::step_transition_for_rev(Rev::CMIS_5_3, current, event, ctx)
    }

    #[test]
    fn step_transition_plans_the_matched_edge_as_data() {
        // Host clears LowPwrRequestSW with the hardware allow bit cleared:
        // LowPwr -> PwrUp, planned as a byte-3 state-register write plus the
        // PwrUp timer and log event — never executed here.
        let ctx = StepCtx {
            allow_hw: Some(false),
            request_sw: None,
        };
        let outcome = step(
            CmisModuleStateSpace::ModuleLowPwr,
            FsmEvent::RegisterWrite {
                register: "lower_memory.low_pwr_request_sw",
            },
            &ctx,
        )
        .expect("the sw power-up edge must match");
        let StepTransition::Advance {
            to: CmisModuleStateSpace::ModulePwrUp,
            effects,
        } = outcome
        else {
            panic!("expected an advance to ModulePwrUp, got {outcome:?}");
        };
        // The state-register write is resolved from the schema catalog:
        // lower page, offset 3, bits 3:1 = 0b1110, value ModulePwrUp (2)
        // shifted into place. Read-modify-write data, never a whole byte.
        assert!(
            effects.contains(&PlannedEffect::RegisterWrite(PlannedRegisterWrite {
                address: I2C_ADDR_MODULE,
                page: 0,
                bank: 0,
                offset: MODULE_STATE_OFFSET,
                mask: 0x0E,
                value: 0x04,
            })),
            "effects: {effects:?}"
        );
        assert!(effects.contains(&PlannedEffect::StartTimer {
            timer: "ModulePwrUpMaxDuration",
            duration: openxcvr_core::fsm::FsmTimerDuration::AdvertisedField(
                "page_01h_advertising.max_duration_module_pwr_up"
            )
        }));
        assert!(effects.contains(&PlannedEffect::LogEvent {
            event_id: "msm_power_up_started"
        }));
        assert_eq!(effects.len(), 3);
    }

    #[test]
    fn step_transition_plans_the_mgmt_init_exit() {
        // The MgmtInit exit edge to ModuleLowPwr: its recorded state-change
        // effect resolves to the byte-3 write carrying ModuleLowPwr (1).
        // The edge is selected from the table directly because several
        // internal edges leave MgmtInit and `FsmEvent::Internal` alone
        // cannot discriminate their recorded conditions — the caller
        // disambiguates with the facts it observed, which is exactly the
        // fail-closed `evaluate` contract.
        let table = Msm::fsm_for_rev(Rev::CMIS_5_3).expect("catalog rev");
        let edge = table
            .transitions
            .iter()
            .find(|t| t.id == "mgmt_init_to_module_low_pwr")
            .expect("the mgmt-init exit edge is schema-recorded");
        let effects = edge
            .side_effects
            .iter()
            .map(|effect| resolve_effect(Rev::CMIS_5_3, effect))
            .collect::<Result<Vec<_>, _>>()
            .expect("the recorded effects resolve at 5.3");
        assert_eq!(edge.to, CmisModuleStateSpace::ModuleLowPwr);
        assert!(
            effects.contains(&PlannedEffect::RegisterWrite(PlannedRegisterWrite {
                address: I2C_ADDR_MODULE,
                page: 0,
                bank: 0,
                offset: MODULE_STATE_OFFSET,
                mask: 0x0E,
                value: 0x02,
            }))
        );
    }

    #[test]
    fn step_transition_no_match_stays_put() {
        // Unsatisfied guard: fail closed, nothing planned.
        let ctx = StepCtx {
            allow_hw: Some(true),
            request_sw: None,
        };
        assert_eq!(
            step(
                CmisModuleStateSpace::ModuleLowPwr,
                FsmEvent::RegisterWrite {
                    register: "lower_memory.low_pwr_request_sw"
                },
                &ctx,
            )
            .unwrap(),
            StepTransition::NoMatch
        );
        // Unsupplied guard fact: fail closed the same way.
        assert_eq!(
            step(
                CmisModuleStateSpace::ModuleLowPwr,
                FsmEvent::RegisterWrite {
                    register: "lower_memory.low_pwr_request_sw"
                },
                &StepCtx {
                    allow_hw: None,
                    request_sw: None,
                },
            )
            .unwrap(),
            StepTransition::NoMatch
        );
        // A different register never fires the edge.
        assert_eq!(
            step(
                CmisModuleStateSpace::ModuleLowPwr,
                FsmEvent::RegisterWrite {
                    register: "lower_memory.other"
                },
                &StepCtx {
                    allow_hw: Some(false),
                    request_sw: None,
                },
            )
            .unwrap(),
            StepTransition::NoMatch
        );
    }

    #[test]
    fn step_transition_fails_closed_off_catalog() {
        let rev = Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        };
        let err = Msm::step_transition_for_rev(
            rev,
            CmisModuleStateSpace::ModuleLowPwr,
            FsmEvent::Internal,
            &StepCtx {
                allow_hw: None,
                request_sw: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, MsmError::UnsupportedRevision(r) if r == rev),
            "{err}"
        );
    }

    #[test]
    fn unresolvable_effects_refuse_the_whole_step() {
        // Synthetic recorded effects the resolver must refuse, each for a
        // typed reason — never a partial plan.
        let unresolved = |effect: &FsmSideEffect| {
            let err = resolve_effect(Rev::CMIS_5_3, effect).unwrap_err();
            assert!(matches!(err, MsmError::UnresolvedEffect { .. }), "{err}");
        };
        // A target outside lower memory.
        unresolved(&FsmSideEffect::StateChange {
            target: "page_10h_lane_control.dp_applied_lane",
            operation: FsmStateChangeOp::Assign,
            value: Some(FsmScalar::Bool(true)),
        });
        // A lower-memory field the catalog does not carry.
        unresolved(&FsmSideEffect::StateChange {
            target: "lower_memory.no_such_field",
            operation: FsmStateChangeOp::Assign,
            value: Some(FsmScalar::Bool(true)),
        });
        // A symbolic value that is not a state of this machine.
        unresolved(&FsmSideEffect::StateChange {
            target: "lower_memory.module_state",
            operation: FsmStateChangeOp::Assign,
            value: Some(FsmScalar::Symbol("NotAState")),
        });
        // An unreported state cannot be written into the state register.
        unresolved(&FsmSideEffect::StateChange {
            target: "lower_memory.module_state",
            operation: FsmStateChangeOp::Assign,
            value: Some(FsmScalar::Symbol("MgmtInit")),
        });
        // Assign without a value.
        unresolved(&FsmSideEffect::StateChange {
            target: "lower_memory.module_state",
            operation: FsmStateChangeOp::Assign,
            value: None,
        });
        // An integer wider than the 3-bit field.
        unresolved(&FsmSideEffect::StateChange {
            target: "lower_memory.module_state",
            operation: FsmStateChangeOp::Assign,
            value: Some(FsmScalar::Int(9)),
        });
        // Set/clear on the multi-bit state field.
        unresolved(&FsmSideEffect::StateChange {
            target: "lower_memory.module_state",
            operation: FsmStateChangeOp::Set,
            value: None,
        });
        // A resolvable bool set on the byte-26 field plans a masked write.
        let planned = resolve_effect(
            Rev::CMIS_5_3,
            &FsmSideEffect::StateChange {
                target: "lower_memory.low_pwr_request_sw",
                operation: FsmStateChangeOp::Set,
                value: None,
            },
        )
        .expect("byte 26 bit 4 is a 1-bit lower-memory field");
        assert_eq!(
            planned,
            PlannedEffect::RegisterWrite(PlannedRegisterWrite {
                address: I2C_ADDR_MODULE,
                page: 0,
                bank: 0,
                offset: 26,
                mask: 0x10,
                value: 0x10,
            })
        );
    }

    #[test]
    fn msm_current_state_for_rev_fails_closed_before_io() {
        let rev = Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        };
        // A transport that fails the test if touched: the rev gate must
        // refuse before any read.
        struct Untouched;
        impl Transport for Untouched {
            fn read(
                &mut self,
                _a: u8,
                _p: u8,
                _b: u8,
                _o: u8,
                _buf: &mut [u8],
            ) -> Result<(), ModuleError> {
                panic!("unsupported revision must be refused before any I/O");
            }
            fn write(
                &mut self,
                _a: u8,
                _p: u8,
                _b: u8,
                _o: u8,
                _d: &[u8],
            ) -> Result<(), ModuleError> {
                panic!("no write may happen");
            }
            fn probe(&mut self) -> Result<(), ModuleError> {
                Ok(())
            }
        }
        impl CmisTransport for Untouched {}
        assert!(matches!(
            Msm::current_state_for_rev(&mut Untouched, rev),
            Err(MsmError::UnsupportedRevision(_))
        ));
    }

    #[test]
    fn test_timeout() {
        // Byte 3 = PwrUp (2 << 1).
        let mut transport = MockTransport::new(0x04, u32::MAX, 0x04);

        let result = Msm::wait_for(&mut transport, ModuleState::Ready, 5);

        assert!(matches!(result, Err(MsmError::Timeout)));
        assert_eq!(transport.poll_count, 5);
    }
}
