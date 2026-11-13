//! Pure, caller-held selector planning and supplied-result reduction.
//!
//! Independently authored from CMIS 4.0 / 5.3 lower-memory bank select (126)
//! and page select (127), and SFF-8636 page select (127). Not donated code.
//! No I/O, timing, exclusion, cache or module instance is owned here.
//!
//! Obtain revision/layout facts from the module, supply positively qualified
//! read metadata, plan, then publish `selector_begin`'s state BEFORE issuing
//! any step. Execute only `pending()` in order and publish every reduction.
//! An initial read expands the ordered plan with only the necessary writes
//! and a final verification; it does not itself authorize data. A failed,
//! NACKed or uncertain write poisons knowledge and skips remaining writes:
//! only a successful matching verification can recover that attempt.
//!
//! Use fresh, strictly increasing `OperationId`s per module instance for both
//! attempts and invalidations (including plans not yet begun). Never reuse an
//! ID across generation/epoch changes. The caller must apply `at` on EVERY
//! boundary change and invalidate on external selector-affecting events.
//! Readback is not proof of exclusion from external masters. Data permission
//! is only selector confirmation under the caller's still-current boundary
//! and exclusion; it is not data-read admission, cache-fill or write authority.

use crate::{
    rev::Rev,
    types::management::{
        ActuationOutcome, CacheEpoch, ModuleGeneration, OperationId, Range, RangeOutcome,
        ReadEffect, ReadFootprint, ReadRequester, ReadStep, Selection,
    },
};

/// What the caller knows about the module's current bank/page selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorKnowledge {
    /// No positive knowledge: nothing is confirmed, nothing is poisoned.
    Unknown,
    /// A verified selection: the readback matched the target.
    Confirmed(Selection),
    /// A failed, NACKed or uncertain write poisoned the knowledge; only a
    /// fresh attempt's successful verification recovers it.
    Poisoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutKind {
    Cmis(Rev),
    Sff8636,
}

/// A supported layout with positively supplied selector-read qualification.
/// Unsupported/flat/nonpaged layouts have no constructor in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorLayout {
    kind: LayoutKind,
    read: ReadStep,
}

impl SelectorLayout {
    /// Only the reported CMIS revision's own metadata may qualify 126-127.
    /// This does not qualify any upper-page data footprint.
    pub fn cmis_banked(revision: Rev, footprint: &ReadFootprint) -> Result<Self, SelectorRefusal> {
        if !matches!(revision, Rev::CMIS_4_0 | Rev::CMIS_5_3) {
            return Err(SelectorRefusal::UnsupportedLayout);
        }
        if footprint.revision != revision || footprint.address != 0x50 {
            return Err(SelectorRefusal::UnqualifiedReadback);
        }
        Self::qualified(
            LayoutKind::Cmis(revision),
            ReadStep {
                physical_range: register_range(126, 2),
                requester: ReadRequester::PassiveInspection,
                classifications: footprint.lower[126..128].to_vec(),
                declared_effects: vec![ReadEffect::None],
            },
        )
    }

    /// The family layer supplies positive metadata for SFF-8636 byte 127.
    /// Byte 126 is never read or written by this layout.
    pub fn sff8636_page_only(read: ReadStep) -> Result<Self, SelectorRefusal> {
        Self::qualified(LayoutKind::Sff8636, read)
    }

    fn qualified(kind: LayoutKind, read: ReadStep) -> Result<Self, SelectorRefusal> {
        let range = match kind {
            LayoutKind::Cmis(_) => register_range(126, 2),
            LayoutKind::Sff8636 => register_range(127, 1),
        };
        if read.physical_range != range
            || read.classifications.len() != usize::from(range.len)
            || !read
                .classifications
                .iter()
                .all(|byte| byte.is_qualified_none())
            || read.declared_effects.is_empty()
            || !read
                .declared_effects
                .iter()
                .all(|effect| *effect == ReadEffect::None)
        {
            return Err(SelectorRefusal::UnqualifiedReadback);
        }
        Ok(Self { kind, read })
    }

    /// The CMIS revision this layout was qualified for; `None` for
    /// SFF-8636 (which has no revision dimension here).
    pub fn revision(&self) -> Option<Rev> {
        match self.kind {
            LayoutKind::Cmis(revision) => Some(revision),
            LayoutKind::Sff8636 => None,
        }
    }

    fn accepts(&self, selection: Selection) -> bool {
        selection.address == 0x50
            && match self.kind {
                LayoutKind::Cmis(_) => selection.page >= 0x10 || selection.bank == 0,
                LayoutKind::Sff8636 => selection.bank == 0,
            }
    }

    fn decode(&self, outcome: &RangeOutcome) -> Option<Selection> {
        match (self.kind, outcome) {
            (LayoutKind::Cmis(_), RangeOutcome::Bytes(bytes)) if bytes.len() == 2 => {
                Some(Selection {
                    address: 0x50,
                    bank: bytes[0],
                    page: bytes[1],
                })
            }
            (LayoutKind::Sff8636, RangeOutcome::Bytes(bytes)) if bytes.len() == 1 => {
                Some(Selection {
                    address: 0x50,
                    bank: 0,
                    page: bytes[0],
                })
            }
            _ => None,
        }
    }

    fn readback(&self, purpose: ReadbackPurpose) -> SelectorStep {
        SelectorStep::Readback {
            purpose,
            read: self.read.clone(),
        }
    }

    fn changes(&self, current: Selection, target: Selection) -> Vec<SelectorStep> {
        let mut steps = Vec::new();
        if matches!(self.kind, LayoutKind::Cmis(_)) && current.bank != target.bank {
            steps.push(SelectorStep::Write {
                range: register_range(126, 1),
                value: target.bank,
            });
        }
        if current.page != target.page {
            steps.push(SelectorStep::Write {
                range: register_range(127, 1),
                value: target.page,
            });
        }
        steps.push(self.readback(ReadbackPurpose::Verification));
        steps
    }
}

fn register_range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

/// Why a selector plan was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorRefusal {
    /// The reported revision/layout has no selector planning in this slice.
    UnsupportedLayout,
    /// The supplied selector-read metadata is not positively qualified
    /// (wrong range, unqualified bytes, or declared effects).
    UnqualifiedReadback,
    /// The target selection does not fit the layout (address, bank or
    /// page outside what the layout can program).
    InvalidSelection,
    /// The attempt ID is not strictly newer than every previously
    /// allocated one.
    RetiredAttempt,
    /// Another plan is active on this state.
    Busy,
}

/// Why a readback step exists in a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadbackPurpose {
    /// The plan opens with a read to learn the current selection before
    /// computing the necessary writes.
    Initial,
    /// The plan closes with a read that must match the target to confirm.
    Verification,
}

/// One executable step of a selector plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorStep {
    /// Read the selector registers.
    Readback {
        /// Why this read exists in the plan.
        purpose: ReadbackPurpose,
        /// The qualified read to execute.
        read: ReadStep,
    },
    /// A single selector-register write, not a general data write grant.
    Write {
        /// The selector-register byte to write.
        range: Range,
        /// The value to write.
        value: u8,
    },
}

/// Step identity wraps the shared operation ID, generation and epoch; the
/// shared supplied results do not carry a selector-attempt/step identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectorStamp {
    /// The generation the step belongs to.
    pub generation: ModuleGeneration,
    /// The epoch the step belongs to.
    pub epoch: CacheEpoch,
    /// The plan's operation ID.
    pub attempt: OperationId,
    /// The step's index in the plan.
    pub step: usize,
}

/// The currently pending step, with its identity stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorPending {
    /// The pending step's identity.
    pub stamp: SelectorStamp,
    /// The step to execute now.
    pub step: SelectorStep,
}

/// An ordered, possibly staged plan. When `steps()` contains an initial
/// readback, its supplied result determines the exact remaining writes;
/// inspect the reducer's returned state's `pending()` / `remaining_steps()`.
/// Private fields prevent fabricated plans from bypassing layout checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorPlan {
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    attempt: OperationId,
    basis: SelectorKnowledge,
    previous_attempt: Option<OperationId>,
    target: Selection,
    layout: SelectorLayout,
    steps: Vec<SelectorStep>,
}

impl SelectorPlan {
    /// The generation the plan is stamped with.
    pub fn generation(&self) -> ModuleGeneration {
        self.generation
    }
    /// The cache epoch the plan is stamped with.
    pub fn epoch(&self) -> CacheEpoch {
        self.epoch
    }
    /// The plan's operation ID.
    pub fn attempt(&self) -> OperationId {
        self.attempt
    }
    /// The selection the plan aims to confirm.
    pub fn target(&self) -> Selection {
        self.target
    }
    /// The plan's ordered steps (an initial readback's remainder is
    /// computed only after its result is supplied).
    pub fn steps(&self) -> &[SelectorStep] {
        &self.steps
    }
    /// Planning alone never authorizes data access.
    pub fn data_permission(&self) -> DataPermission {
        DataPermission::Denied
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Active {
    plan: SelectorPlan,
    cursor: usize,
}

/// Caller-held knowledge for ONE module instance, including retired attempt
/// fencing. Do not share a state across module instances or restore old copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorState {
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    knowledge: SelectorKnowledge,
    last_attempt: Option<OperationId>,
    active: Option<Active>,
}

impl SelectorState {
    /// A fresh state at the given generation/epoch, knowledge unknown,
    /// no attempts yet.
    pub fn new(generation: ModuleGeneration, epoch: CacheEpoch) -> Self {
        Self {
            generation,
            epoch,
            knowledge: SelectorKnowledge::Unknown,
            last_attempt: None,
            active: None,
        }
    }

    /// The generation this state holds.
    pub fn generation(&self) -> ModuleGeneration {
        self.generation
    }
    /// The cache epoch this state holds.
    pub fn epoch(&self) -> CacheEpoch {
        self.epoch
    }

    /// The selector knowledge, valid only at the named generation/epoch;
    /// any other stamp answers `Unknown`.
    pub fn knowledge(&self, generation: ModuleGeneration, epoch: CacheEpoch) -> SelectorKnowledge {
        if self.generation == generation && self.epoch == epoch {
            self.knowledge
        } else {
            SelectorKnowledge::Unknown
        }
    }

    /// Publish this returned state on a generation/epoch change. Any pending
    /// attempt is retired, while the attempt-ID high-water mark is retained.
    pub fn at(&self, generation: ModuleGeneration, epoch: CacheEpoch) -> Self {
        if self.generation == generation && self.epoch == epoch {
            return self.clone();
        }
        Self {
            generation,
            epoch,
            knowledge: SelectorKnowledge::Unknown,
            last_attempt: self.last_attempt,
            active: None,
        }
    }

    /// The pending step to execute now, when an attempt is active.
    pub fn pending(&self) -> Option<SelectorPending> {
        let active = self.active.as_ref()?;
        Some(SelectorPending {
            stamp: SelectorStamp {
                generation: self.generation,
                epoch: self.epoch,
                attempt: active.plan.attempt,
                step: active.cursor,
            },
            step: active.plan.steps.get(active.cursor)?.clone(),
        })
    }

    /// The steps still to execute in the active attempt (empty when
    /// none is active).
    pub fn remaining_steps(&self) -> &[SelectorStep] {
        self.active
            .as_ref()
            .map_or(&[], |active| &active.plan.steps[active.cursor..])
    }

    fn poison(&self) -> Self {
        Self {
            knowledge: SelectorKnowledge::Poisoned,
            active: None,
            ..self.clone()
        }
    }
}

/// Plans never grant data permission. Confirmed same-boundary matches skip
/// writes but still verify. Unknown AND poisoned states require initial reads.
pub fn selector_plan(
    state: &SelectorState,
    target: Selection,
    layout: &SelectorLayout,
    attempt: OperationId,
) -> Result<SelectorPlan, SelectorRefusal> {
    if state.active.is_some() {
        return Err(SelectorRefusal::Busy);
    }
    if state.last_attempt.is_some_and(|last| attempt <= last) {
        return Err(SelectorRefusal::RetiredAttempt);
    }
    if !layout.accepts(target) {
        return Err(SelectorRefusal::InvalidSelection);
    }
    let steps = match state.knowledge {
        SelectorKnowledge::Confirmed(current) if layout.accepts(current) => {
            layout.changes(current, target)
        }
        _ => vec![layout.readback(ReadbackPurpose::Initial)],
    };
    Ok(SelectorPlan {
        generation: state.generation,
        epoch: state.epoch,
        attempt,
        basis: state.knowledge,
        previous_attempt: state.last_attempt,
        target,
        layout: layout.clone(),
        steps,
    })
}

/// Publish this pessimistic Unknown state before executing ANY plan step.
/// A stale/replayed plan poisons knowledge and retires any active attempt.
pub fn selector_begin(state: &SelectorState, plan: &SelectorPlan) -> SelectorState {
    if state.active.is_some()
        || state.generation != plan.generation
        || state.epoch != plan.epoch
        || state.knowledge != plan.basis
        || state.last_attempt != plan.previous_attempt
        || state.last_attempt.is_some_and(|last| plan.attempt <= last)
    {
        return state.poison();
    }
    SelectorState {
        knowledge: SelectorKnowledge::Unknown,
        last_attempt: Some(plan.attempt),
        active: Some(Active {
            plan: plan.clone(),
            cursor: 0,
        }),
        ..state.clone()
    }
}

/// Reuse portable supplied outcomes. NACK is separate because neither shared
/// RangeOutcome nor ActuationOutcome distinguishes NACK from other failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorResult {
    /// The readback's supplied outcome.
    Read(RangeOutcome),
    /// The write's supplied outcome.
    Write(ActuationOutcome),
    /// The module NACKed the write (neither shared outcome type
    /// distinguishes NACK).
    Nack,
}

/// One supplied result for a plan step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorSuppliedResult {
    /// The identity stamp of the step the result belongs to.
    pub stamp: SelectorStamp,
    /// The step's result.
    pub result: SelectorResult,
}

/// Whether data reads may proceed under the caller's still-current boundary
/// and exclusion. Not data-read admission or write authority by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPermission {
    /// No data permission is granted.
    Denied,
    /// Selector confirmation exists for the named selection.
    Granted(Selection),
}

/// The state a selector reduction leaves behind, plus its data permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorOutcome {
    /// The state after the reduction; publish it before any further step.
    pub state: SelectorState,
    /// Valid only at this state's generation/epoch with caller-held exclusion.
    pub data_permission: DataPermission,
}

/// Reduce ONLY the current step's supplied result. Identity/stamp/order errors
/// poison and terminate the attempt; subsequent late results cannot recover it.
pub fn selector_complete(
    state: &SelectorState,
    supplied: &SelectorSuppliedResult,
) -> SelectorOutcome {
    let denied = |state| SelectorOutcome {
        state,
        data_permission: DataPermission::Denied,
    };
    let Some(pending) = state.pending() else {
        return denied(state.poison());
    };
    if supplied.stamp != pending.stamp {
        return denied(state.poison());
    }
    let mut next = state.clone();
    let active = next
        .active
        .as_mut()
        .expect("pending step has an active plan");
    match (&pending.step, &supplied.result) {
        (SelectorStep::Readback { purpose, .. }, SelectorResult::Read(outcome)) => {
            let Some(observed) = active.plan.layout.decode(outcome) else {
                return denied(state.poison());
            };
            match purpose {
                ReadbackPurpose::Initial => {
                    active
                        .plan
                        .steps
                        .extend(active.plan.layout.changes(observed, active.plan.target));
                    active.cursor += 1;
                    denied(next)
                }
                ReadbackPurpose::Verification if observed == active.plan.target => {
                    let target = active.plan.target;
                    next.knowledge = SelectorKnowledge::Confirmed(target);
                    next.active = None;
                    SelectorOutcome {
                        state: next,
                        data_permission: DataPermission::Granted(target),
                    }
                }
                ReadbackPurpose::Verification => denied(state.poison()),
            }
        }
        (SelectorStep::Write { .. }, SelectorResult::Write(ActuationOutcome::Confirmed)) => {
            active.cursor += 1;
            denied(next)
        }
        (SelectorStep::Write { .. }, SelectorResult::Nack | SelectorResult::Write(_)) => {
            // No more selection effects after an ambiguous/failed write. A
            // matching final readback is the only allowed same-attempt recovery.
            next.knowledge = SelectorKnowledge::Poisoned;
            active.cursor = active.plan.steps.len() - 1;
            denied(next)
        }
        _ => denied(state.poison()),
    }
}

/// Caller-reported selector-affecting events; no event is detected by the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorInvalidation {
    /// A data transfer on the module failed.
    DataTransferFailed,
    /// An external write to a selector register.
    SelectorWrite,
    /// A write to the CMIS control register (byte 26).
    Control26Write,
    /// A CDB command ran on the module.
    CdbCommand,
    /// The module was reset.
    Reset,
    /// A power transition began.
    PowerTransition,
    /// A power state change was observed but is unconfirmed.
    PowerUncertain,
    /// A profile change was observed but is unconfirmed.
    ProfileUncertain,
    /// The caller lost exclusive ownership of the module.
    OwnershipUncertain,
    /// An explicit caller-directed invalidation.
    Explicit,
}

/// Invalidate with an ID newer than ALL previously allocated plans, including
/// unbegun ones. On generation/epoch retirement use `SelectorState::at` as well.
pub fn selector_invalidate(
    state: &SelectorState,
    _reason: SelectorInvalidation,
    boundary: OperationId,
) -> SelectorState {
    SelectorState {
        last_attempt: Some(
            state
                .last_attempt
                .map_or(boundary, |last| last.max(boundary)),
        ),
        ..state.poison()
    }
}
