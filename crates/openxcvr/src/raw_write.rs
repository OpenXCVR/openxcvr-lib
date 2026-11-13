//! Pure raw-WRITE execution plan over a validated `Purpose::Write` description.
//!
//! This module plans, never acts: it returns ordered planned effects for the
//! daemon's write owner to execute; no transport, clock, cache, selector
//! transition, retry loop, checksum or password is owned or implied here.
//! It serves the raw `--write` parity target (`xcvr-tools --write-raw`):
//! write exactly the requested bytes, read the same range back, and classify
//! the readback as STORED / MISMATCH / READFAIL — never computing a checksum,
//! entering a password, or selecting pages itself.
//!
//! # Admission and scope
//!
//! The only input is a `RawDescription` built by [`crate::raw::preflight`]
//! (its fields are private, so no forged description reaches this planner).
//! A `Purpose::Write` description already guarantees the whole admitted scope:
//! CMIS 4.0, address `0x50`, bank 0, page `00h`, and only bytes `raw.rs`
//! classifies as ordinary, nonsecret and passively readable — so the same-range
//! readback it plans is possible. The planner re-checks only what the
//! description cannot carry: the write payloads, supplied again beside it
//! because descriptions deliberately never retain payload bytes.
//!
//! # Plan shape
//!
//! 1. One `ConfirmSelection` step: a fresh read of the bank/page selector
//!    bytes 126–127 requiring `[0, 0]`, exactly the confirmation the daemon's
//!    vendor-programming owner executes before any page-00h write. It is a
//!    read only: the selection is never changed and never restored, because
//!    it is never left.
//! 2. One `Write` step per requested target, exactly the requested bytes at
//!    the requested range. No widening, no unrequested byte (no byte 222, no
//!    122–126 password area), no chunking: `raw.rs` already confines every
//!    range to one 128-byte lower/upper window, and the vendor-programming
//!    path likewise preserves each supplied range exactly.
//! 3. One `VerifyRead` step per target over the same range, carrying the same
//!    borrowed bytes for comparison by [`classify_readback`].
//!
//! The plan names its one required selection and carries operation-end
//! invalidations (one `Invalidation::Covered` per written range, never a
//! password pin) and one terminal-uncertain decision per step — the existing
//! vendor-programming convention, so an unconfirmed step is never retried or
//! continued past. Payloads stay borrowed and opaque: no plan copy exists, and
//! `Debug` exposes counts only. Plan equality is deliberately not derived; the
//! verdict function is the only place bytes are compared.
//!
//! # Range contract
//!
//! Writes are confined to the requested ranges and verification reads cover
//! exactly those ranges. The single `ConfirmSelection` read of bytes 126–127 is
//! the one declared auxiliary step outside them; it is a read and never a write.
//! The generated classification of those two bytes as no-effect reads carries a
//! CMIS 5.3 citation (`schemas/cmis/fields/lower-memory-part-10.yaml`); the
//! delivered vendor-programming `ConfirmPage00` relies on the same row at 4.0.
//!
//! # CMIS 4.0 basis
//!
//! This plan admits only CMIS 4.0, so the 5.3 row is not its evidence. CMIS
//! Rev 4.0 (May 8, 2019) defines no read behaviour for bytes 126 and 127
//! (Table 8-1, p.80; §8.2.11 and §8.2.12, p.91). The read side effects it names
//! are clear-on-read latched flags (§3.2 defines COR, p.16; Table 8-5, p.83) and
//! interrupt flags cleared when the host reads them (e.g. p.50). It has no
//! access-type table, so this is the absence of a defined effect rather than
//! an explicit statement that the read has none. Control registers default to
//! 0 (§8.1.3.4, p.79), and a flat-memory module implements only page 00h
//! (Table 8-2, p.81), so expecting `[0, 0]` fails closed on any other value.
//! The plan names ranges and typed refusals. It does not perform the write.
//!
//! # Execution boundary
//!
//! A plan is not proof of safe execution. Reading 126–127 does not by itself
//! prevent a write to the wrong page: the executor must compare the result with
//! `expected` and hold exclusive, generation-fenced ownership of the module from
//! that read through the last verify read. Nothing retries or corrects the
//! selector, and an unconfirmed step follows the plan's terminal-uncertain
//! decision. Applied writes cannot be undone, so outcomes split three ways:
//!
//! - **Zero data writes, definite.** A confirmation that completes with a value
//!   other than `expected`, completes short or completes with a read failure
//!   ends the operation. So does a fence, ownership loss or cancellation seen
//!   before any step is dispatched, or after the confirmation completes and
//!   before the first data write is dispatched.
//! - **Zero data writes, uncertain.** A dispatched confirmation read that is
//!   unconfirmed (timeout, unknown transport outcome, or ownership lost or the
//!   operation cancelled while it is outstanding) is uncertain; the executor
//!   states that no data write was dispatched.
//! - **Uncertain after a data write.** From the first data write dispatch on,
//!   ownership loss, cancellation, timeout or an unknown transport outcome stops
//!   the remaining steps, keeps the known progress, reports uncertain and never
//!   replays.
//!
//! [`RawWriteVerdict::ReadFailure`] applies only when the verify read completed
//! with a definite failure ([`RawReadback::Failed`]); a verify read that timed
//! out or was lost is uncertain, not a read failure.
use crate::raw::{Purpose, RawDescription, WriteBytes};
use openxcvr_cmis::generated::v4_0::{CmisLowerMemory, CmisPage00hAdministrative};
use openxcvr_core::{
    rev::Rev,
    types::management::{
        operations::RetryDecision, ActuationOutcome, Invalidation, Range, ReadEffect,
        ReadRequester, ReadStep, Selection,
    },
};

/// One planned effect, in execution order. No selector-transition variant
/// exists: the selection is confirmed by a read and never written.
#[derive(Debug, Clone)]
pub enum RawWriteStep<'a> {
    /// Refuse before any write unless the fresh selector read equals `expected`.
    ConfirmSelection {
        /// The selector-confirmation read to execute.
        read: ReadStep,
        /// The selector bytes the read must equal.
        expected: [u8; 2],
    },
    /// Write exactly `bytes` to `range`; no widening, no chunking.
    Write {
        /// The range to write.
        range: Range,
        /// The exact bytes to write.
        bytes: WriteBytes<'a>,
    },
    /// Read `range` back; classify with [`classify_readback`] or
    /// [`RawWritePlan::verdict`].
    VerifyRead {
        /// The readback read to execute.
        read: ReadStep,
        /// The bytes the readback must return.
        expected: WriteBytes<'a>,
    },
}

/// Ordered planned effects for one exclusive daemon operation, plus the
/// operation-end rows its write owner already consumes for vendor programming.
#[derive(Debug, Clone)]
pub struct RawWritePlan<'a> {
    /// The revision the plan was validated for.
    pub revision: Rev,
    /// The only selection this plan ever requires: bank 0, page `00h`, at the
    /// admitted address. Confirmed by the first step; never changed or restored.
    pub required_selection: Selection,
    /// The ordered effects to execute.
    pub steps: Vec<RawWriteStep<'a>>,
    /// Cache invalidations for the daemon to apply at operation end, even on
    /// failure or uncertainty: one `Covered` per written range. No password
    /// pin is named because no password is written.
    pub invalidations: Vec<Invalidation>,
    /// One cadence-row decision per step, in step order: every decision is
    /// terminal Uncertain, so an unconfirmed step is never retried or continued.
    pub unconfirmed_step_retries: Vec<RetryDecision>,
}

/// What the executor reports for one verify read. A failed read carries no
/// byte count: the actual count stays unknown, never guessed or padded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawReadback<'a> {
    /// Confirmed readback bytes. A length other than the written count is a
    /// short (or otherwise inexact) read.
    Bytes(&'a [u8]),
    /// The read failed.
    Failed,
}

/// Pure verdict for one verify readback, matching the raw-write parity
/// reporting: STORED, MISMATCH (with differing offsets), or READFAIL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawWriteVerdict {
    /// The exact range came back with every requested byte stored.
    Stored,
    /// The exact range came back; the listed absolute offsets differ.
    Mismatch {
        /// The absolute offsets whose bytes differ.
        differing_offsets: Vec<u8>,
    },
    /// The readback length is not the written count; `actual` is what came back.
    ShortRead {
        /// The byte count that actually came back.
        actual: usize,
    },
    /// The read failed. Whether or how many bytes returned stays unknown.
    ReadFailure,
}

/// Why no plan was returned. Refusals carry no bytes and no partial plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RawWritePlanRefusal {
    /// The description is not a write description.
    #[error("raw-write plans require a Purpose::Write description")]
    PurposeNotWrite,
    /// The payload count differs from the description's target count.
    #[error("write payload count differs from the description's target count")]
    PayloadCount,
    /// A payload's length differs from its target range.
    #[error("write payload length differs from its target range")]
    PayloadLength,
    /// Two write targets overlap.
    #[error("overlapping write targets: verification cannot demand contradictory bytes")]
    OverlappingTargets,
}

/// Selector-confirmation read over lower-memory bytes 126–127 (bank select,
/// page select), requiring `[0, 0]`. Generated 4.0 metadata classifies both
/// as qualified no-effect reads; the step is a read only.
fn selector_read_step(address: u8) -> ReadStep {
    let lower = CmisLowerMemory::read_classifications();
    ReadStep {
        physical_range: Range {
            address,
            bank: 0,
            page: 0,
            offset: 126,
            len: 2,
        },
        requester: ReadRequester::ExplicitClient,
        classifications: lower[126..128].to_vec(),
        declared_effects: vec![ReadEffect::None],
    }
}

/// Same-range verify read, built from the generated 4.0 classifications the
/// preflight already admitted every byte against (`raw.rs` refuses any byte
/// that is not qualified no-effect, so the declared effect is [`ReadEffect::None`]).
fn verify_read_step(range: Range) -> ReadStep {
    let lower = CmisLowerMemory::read_classifications();
    let upper = CmisPage00hAdministrative::read_classifications();
    let classifications = (u32::from(range.offset)..range.end())
        .map(|offset| {
            if offset < 128 {
                lower[offset as usize].clone()
            } else {
                upper[(offset - 128) as usize].clone()
            }
        })
        .collect();
    ReadStep {
        physical_range: range,
        requester: ReadRequester::ExplicitClient,
        classifications,
        declared_effects: vec![ReadEffect::None],
    }
}

/// Plan a raw write from a validated `Purpose::Write` description plus the
/// payloads supplied again beside it (descriptions never retain payload bytes).
///
/// Refuses, before any step exists, a read description, a payload count that
/// differs from the description's target count, a payload length that differs
/// from its target range, and overlapping targets — two writes to the same
/// byte would demand contradictory verification bytes. Every refusal is
/// all-or-nothing: no partial plan or step list escapes.
pub fn plan_raw_write<'a>(
    description: &RawDescription,
    payloads: &[WriteBytes<'a>],
) -> Result<RawWritePlan<'a>, RawWritePlanRefusal> {
    use RawWritePlanRefusal as Refusal;
    if description.purpose() != Purpose::Write {
        return Err(Refusal::PurposeNotWrite);
    }
    let targets = description.targets();
    if payloads.len() != targets.len() {
        return Err(Refusal::PayloadCount);
    }
    for (target, payload) in targets.iter().zip(payloads) {
        if payload.len() != usize::from(target.target().range.len) {
            return Err(Refusal::PayloadLength);
        }
    }
    for (index, target) in targets.iter().enumerate() {
        let range = target.target().range;
        for prior in &targets[..index] {
            let prior = prior.target().range;
            if prior.address == range.address
                && prior.bank == range.bank
                && prior.page == range.page
                && u32::from(prior.offset) < range.end()
                && u32::from(range.offset) < prior.end()
            {
                return Err(Refusal::OverlappingTargets);
            }
        }
    }
    let required_selection = targets[0].target().selection();
    let mut steps = vec![RawWriteStep::ConfirmSelection {
        read: selector_read_step(required_selection.address),
        expected: [0, 0],
    }];
    for (target, payload) in targets.iter().zip(payloads) {
        steps.push(RawWriteStep::Write {
            range: target.target().range,
            bytes: *payload,
        });
    }
    for (target, payload) in targets.iter().zip(payloads) {
        steps.push(RawWriteStep::VerifyRead {
            read: verify_read_step(target.target().range),
            expected: *payload,
        });
    }
    let invalidations = targets
        .iter()
        .map(|target| Invalidation::Covered(target.target().range))
        .collect();
    let unconfirmed_step_retries = steps
        .iter()
        .map(|_| {
            RetryDecision::Terminal(ActuationOutcome::Uncertain {
                reason: "step-unconfirmed".into(),
            })
        })
        .collect();
    Ok(RawWritePlan {
        revision: description.revision(),
        required_selection,
        steps,
        invalidations,
        unconfirmed_step_retries,
    })
}

/// Classify one verify readback against the bytes that were written.
///
/// `expected` is the exact written byte sequence at `range`; plan steps carry
/// both, so a verdict is always computed over a consistent pair. Bytes are
/// compared here and nowhere else: a readback that fails reports no count
/// (unknown stays unknown), an inexact length carries its actual count, and
/// an exact-length readback names every differing absolute offset.
pub fn classify_readback(
    range: Range,
    expected: &[u8],
    readback: RawReadback<'_>,
) -> RawWriteVerdict {
    let actual = match readback {
        RawReadback::Failed => return RawWriteVerdict::ReadFailure,
        RawReadback::Bytes(actual) => actual,
    };
    if actual.len() != expected.len() {
        return RawWriteVerdict::ShortRead {
            actual: actual.len(),
        };
    }
    let differing_offsets: Vec<u8> = expected
        .iter()
        .zip(actual)
        .enumerate()
        .filter(|(_, (written, read))| written != read)
        .map(|(index, _)| range.offset + index as u8)
        .collect();
    if differing_offsets.is_empty() {
        RawWriteVerdict::Stored
    } else {
        RawWriteVerdict::Mismatch { differing_offsets }
    }
}

impl RawWritePlan<'_> {
    /// Classify the readback of the `target_index`-th verify read (targets in
    /// description order). A target index with no verify step answers `None`,
    /// never an inferred verdict.
    pub fn verdict(
        &self,
        target_index: usize,
        readback: RawReadback<'_>,
    ) -> Option<RawWriteVerdict> {
        self.steps
            .iter()
            .filter_map(|step| match step {
                RawWriteStep::VerifyRead { read, expected } => {
                    Some((read.physical_range, expected.expose()))
                }
                _ => None,
            })
            .nth(target_index)
            .map(|(range, expected)| classify_readback(range, expected, readback))
    }
}
