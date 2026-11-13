//! Pure operation validation, access, lease, transfer/retry and
//! power/reset/protection plans (task 64). No type here owns a queue,
//! timer, clock, callback, lock or handle, and no family bit table lives
//! here: every family/revision mask, permission or legal encoding is
//! supplied by the caller from the generated schema metadata.
//!
//! Delivered: zero-retry and bounded-retry decisions
//! ([`retry_decide`], [`retry_decide_bounded`]); per-byte and per-bit
//! write validation ([`validate_write`], [`validate_write_bits`]); lease
//! gating ([`lease_gate`]); transfer limits and choice
//! ([`validate_range`], [`transfer_choose`]); RMW prerequisites
//! ([`rmw_prerequisites`]); the power/reset/protection plan reducers
//! ([`reconcile_power`], [`power_request_step`], [`power_result_step`],
//! [`plan_reset`], [`plan_protection`]); and the operation driver over
//! the plan steps (the donor sysfs request/commit/rollback sequence,
//! [`operation_step`]).
//!
//! Still owed by task 64's full contract: SFF family plan constructors
//! in the family layer (the core reducers here consume supplied
//! descriptors; the SFF-8636/SFF-8472 constructors that resolve them from
//! the generated catalogs belong with the family crates).

use super::{
    generation_valid, ActuationOutcome, CacheEpoch, ModuleGeneration, MonoDuration, MonoTime,
    OperationId, PowerGeneration, Range,
};
use crate::field_catalog::AccessKind;

/// The retry policy this bounded subset delivers: zero retries.
///
/// The delivered policy grants no second attempt for any outcome, so it
/// carries no budget, backoff or deadline of its own; those task-64 policy
/// knobs are owed. The parameter stays so the accepted signature does not
/// move.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetryPolicy;

/// What the routine decided for one finished attempt.
///
/// The bounded zero-retry policy has exactly one decision: the operation ends
/// with its outcome, unchanged. A bounded retry deadline variant is owed by
/// task 64; nothing this subset returns ever replays an effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    /// The operation is terminal with this outcome, preserved exactly: a
    /// confirmed outcome stays success, a failed outcome stays failure, and an
    /// uncertain outcome keeps its possibly-applied marking — never collapsed
    /// into success or failure, and never retried.
    Terminal(ActuationOutcome),
}

/// Decide what happens to the operation after one finished attempt.
///
/// Under the bounded zero-retry policy every outcome is terminal: the attempt's
/// outcome is returned unchanged for every attempt count and supplied time, so
/// an uncertain effect is never replayed as if it had failed before applying.
/// The attempt count, time and policy are accepted because the owning task's
/// frozen signature names them; no zero-retry rule consumes them.
pub fn retry_decide(
    result: ActuationOutcome,
    _attempt: u32,
    _now: MonoTime,
    _policy: RetryPolicy,
) -> RetryDecision {
    RetryDecision::Terminal(result)
}

/// A bounded retry policy for idempotent operations (task 64 slice).
///
/// `max_retries` is the number of *additional* attempts after the first
/// (0 preserves the zero-retry behaviour of [`RetryPolicy`]). `deadline`
/// is the supplied time after which no further attempt may start. `None`
/// bounds nothing on the time axis; callers that owe a deadline must supply
/// one. `idempotent` is the caller's declaration that replaying the whole
/// operation is safe (a read, or an idempotent transfer): non-idempotent
/// operations are never retried here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedRetryPolicy {
    /// Additional attempts allowed after the first (0 = zero-retry).
    pub max_retries: u32,
    /// Supplied time after which no further attempt may start; `None`
    /// bounds nothing on the time axis.
    pub deadline: Option<MonoTime>,
    /// The caller's declaration that replaying the whole operation is
    /// safe; non-idempotent operations are never retried.
    pub idempotent: bool,
}

/// What the bounded policy decided for one finished attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedRetryDecision {
    /// The operation ends with this outcome, preserved exactly: a
    /// confirmed outcome stays success, a failed outcome stays failure, and
    /// an uncertain outcome keeps its possibly-applied marking.
    Terminal(ActuationOutcome),
    /// One more attempt may run. `next_attempt` is the attempt index the
    /// caller should stamp it with (the finished attempt's index plus
    /// one); the deadline is echoed back so the caller re-checks it before
    /// the replay. Backoff pacing stays the caller's: no clock is read
    /// here.
    Retry {
        /// The attempt index to stamp the next attempt with.
        next_attempt: u32,
        /// The echoed deadline for the caller to re-check before the
        /// replay.
        deadline: Option<MonoTime>,
    },
}

/// Decide what happens to an idempotent operation after one finished
/// attempt, against a bounded retry policy.
///
/// Fail closed, exactly as task 64 specifies: only a **failed** outcome of
/// an **idempotent** operation can be retried, and only while the attempt
/// budget (`attempt` counts finished attempts, so the first is `0`) and the
/// supplied deadline both allow it. A **confirmed** outcome is terminal
/// success; an **uncertain** outcome is terminal and stays uncertain — a
/// possibly-applied side effect is never replayed as if it had failed
/// before applying. No I/O, no clock, no queue: the daemon owns execution.
pub fn retry_decide_bounded(
    result: ActuationOutcome,
    attempt: u32,
    now: MonoTime,
    policy: BoundedRetryPolicy,
) -> BoundedRetryDecision {
    match result {
        ActuationOutcome::Confirmed => BoundedRetryDecision::Terminal(result),
        ActuationOutcome::Uncertain { .. } => BoundedRetryDecision::Terminal(result),
        ActuationOutcome::Failed { .. } => {
            let budget_left = policy.idempotent && attempt < policy.max_retries;
            let deadline_allows = policy.deadline.is_none_or(|deadline| now < deadline);
            if budget_left && deadline_allows {
                BoundedRetryDecision::Retry {
                    next_attempt: attempt + 1,
                    deadline: policy.deadline,
                }
            } else {
                BoundedRetryDecision::Terminal(result)
            }
        }
    }
}

// ── Write validation (task 64 slice) ───────────────────────────────────────
//
// Family/revision write permissions are generated schema inputs, not
// handwritten core family tables (task 64's rule): the caller supplies one
// access entry per addressed byte, from the schema-derived catalog it
// already holds, and the reducer refuses everything it cannot prove
// writable. Delivered here: per-byte validation, per-bit write masks,
// RMW prerequisites, lease gating, transfer limits/choice, and the
// power/reset/protection plans.

/// One planned write the caller asks the library to validate: the exact
/// byte range it would address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteRequest {
    /// The byte range the write addresses, window-qualified.
    pub range: Range,
}

/// The caller's per-byte access descriptor for the bytes a write
/// addresses: one entry per byte of [`WriteRequest::range`], from the
/// generated schema metadata (the field catalog's access kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteAccess {
    /// The byte's schema access kind.
    pub access: AccessKind,
}

/// Why a write was refused. Typed and offset-carrying: the caller learns
/// the first offending byte, never just a boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteRefusal {
    /// The request addresses no bytes.
    EmptyRange,
    /// The descriptor does not match the request's length: a validation
    /// over a mismatched descriptor proves nothing.
    DescriptorMismatch {
        /// The request's byte count.
        expected: usize,
        /// The descriptor's entry count.
        got: usize,
    },
    /// The first offending byte is read-only (`ro`/`ro_cor`).
    ReadOnlyByte {
        /// The read-only byte's offset.
        offset: u8,
    },
    /// The first offending byte's access is unknown: fail closed, never a
    /// guessed permission.
    UnknownAccessByte {
        /// The unknown-access byte's offset.
        offset: u8,
    },
}

/// The validation decision for one write request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    /// Every addressed byte is writable under the supplied descriptor
    /// (`rw`, `rww`, `wo` or `wo_sc`).
    Allowed,
    /// The write is refused with the typed reason; nothing was written
    /// and the caller must not execute the request as planned.
    Refused(WriteRefusal),
}

/// Validate one planned write against the caller's per-byte access
/// descriptor (task 64 slice). Pure: the request is data, nothing is
/// written. Fail closed: an empty range, a descriptor whose length does
/// not match the request, a read-only byte and an unknown-access byte are
/// each a typed refusal — an unreadable or unprovable byte is never a
/// guessed-writable one.
pub fn validate_write(request: &WriteRequest, descriptor: &[ByteAccess]) -> AccessDecision {
    if request.range.len == 0 {
        return AccessDecision::Refused(WriteRefusal::EmptyRange);
    }
    let len = usize::from(request.range.len);
    if descriptor.len() != len {
        return AccessDecision::Refused(WriteRefusal::DescriptorMismatch {
            expected: len,
            got: descriptor.len(),
        });
    }
    for (index, byte) in descriptor.iter().enumerate() {
        let offset = request.range.offset + index as u8;
        match byte.access {
            AccessKind::Rw | AccessKind::Rww | AccessKind::Wo | AccessKind::WoSc => continue,
            AccessKind::Ro | AccessKind::RoCor => {
                return AccessDecision::Refused(WriteRefusal::ReadOnlyByte { offset })
            }
            AccessKind::Unknown => {
                return AccessDecision::Refused(WriteRefusal::UnknownAccessByte { offset })
            }
        }
    }
    AccessDecision::Allowed
}

// ── Lease gating (task 64) ─────────────────────────────────────────────────
//
// Access/lease predicates consume ALREADY VERIFIED authorization/expiry/
// entitlement facts: the library never verifies a signature, enrollment or
// credential, never stores password material, and never implements a bus
// lease. Absent facts, expired facts and unknown rights are refused — a
// grant is never guessed from missing evidence.

/// The caller-verified entitlement facts one lease decision consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseFacts {
    /// Whether the caller positively verified the requester's entitlement
    /// to the operation. `false` (absent or unverified) refuses.
    pub entitled: bool,
    /// Whether the caller positively verified the requester's rights for
    /// THIS operation class. Unknown rights refuse; there is no default
    /// allow.
    pub rights_known: bool,
}

/// The supplied-authorization lease state (task 64: "LeaseState (supplied
/// authorization validity only)"). Plain validity data — nothing here
/// re-verifies it, and no credential or password material is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseState {
    /// The module generation the lease was verified under.
    pub generation: ModuleGeneration,
    /// The cache epoch the lease was verified under.
    pub epoch: CacheEpoch,
    /// The time after which the verification no longer holds, when the
    /// caller's verification carried one.
    pub expires_at: Option<MonoTime>,
}

/// Why a lease-gated operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseRefusal {
    /// The caller supplied no lease at all: an operation that needs one is
    /// refused outright, never run unverified.
    NoLease,
    /// The caller could not verify entitlement.
    NotEntitled,
    /// The rights for this operation class are unknown: fail closed, no
    /// default allow.
    UnknownRights,
    /// The lease expired: a lease is valid strictly while `now <
    /// expires_at` — at exactly its expiry it is already expired (a grant
    /// fails closed at the boundary, unlike a request deadline, which only
    /// retires strictly past it).
    Expired {
        /// The supplied time the expiry was observed at.
        at: MonoTime,
        /// The lease's expiry time.
        expires_at: MonoTime,
    },
    /// The lease was verified under a stamp that no longer governs the
    /// module (the shared `generation_valid` predicate failed): the
    /// verification does not transfer to a replacement instance.
    StaleStamp {
        /// The generation the retired lease was verified under.
        lease_generation: ModuleGeneration,
        /// The epoch the retired lease was verified under.
        lease_epoch: CacheEpoch,
    },
}

/// The lease decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseDecision {
    /// The supplied lease is valid for this operation: the caller may
    /// proceed with the authorized steps it names.
    Granted,
    /// Refused with the typed reason; nothing runs.
    Refused(LeaseRefusal),
}

/// Gate one operation on its supplied authorization validity (task 64).
/// Pure: the facts arrive verified; this reducer only refuses absent,
/// expired, unentitled, unknown-right or stale-stamped ones. Check order:
/// no lease, then entitlement, then rights, then the stamp (a retired
/// stamp fences publication), then expiry — the boundary is fail-closed
/// for grants.
pub fn lease_gate(
    lease: Option<&LeaseState>,
    facts: LeaseFacts,
    now: MonoTime,
    current_generation: ModuleGeneration,
    current_epoch: CacheEpoch,
) -> LeaseDecision {
    let Some(lease) = lease else {
        return LeaseDecision::Refused(LeaseRefusal::NoLease);
    };
    if !facts.entitled {
        return LeaseDecision::Refused(LeaseRefusal::NotEntitled);
    }
    if !facts.rights_known {
        return LeaseDecision::Refused(LeaseRefusal::UnknownRights);
    }
    if !generation_valid(
        lease.generation,
        lease.epoch,
        current_generation,
        current_epoch,
    ) {
        return LeaseDecision::Refused(LeaseRefusal::StaleStamp {
            lease_generation: lease.generation,
            lease_epoch: lease.epoch,
        });
    }
    if lease.expires_at.is_some_and(|expires_at| now >= expires_at) {
        return LeaseDecision::Refused(LeaseRefusal::Expired {
            at: now,
            expires_at: lease.expires_at.expect("checked Some"),
        });
    }
    LeaseDecision::Granted
}

// ── Transfer limits and choice (task 64; donor core/io.c submit rules) ─────

/// The transfer limits one plan must respect, supplied by the caller from
/// the device's own advertised limits (CMIS: the module's maximum transfer
/// length; the transport's own window rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferLimits {
    /// The largest transfer one step may name, in bytes. Zero is invalid
    /// (nothing could be transferred).
    pub max_transfer_len: u16,
    /// Transfers never span the 128-byte half boundary of a page: lower
    /// memory and the upper half are separate transport windows (the
    /// shared `plan_observation` contract).
    pub split_at_half: bool,
}

impl TransferLimits {
    /// The transfer boundary after `offset` within one 256-byte page
    /// window: the bytes remaining to the half boundary when splitting is
    /// on, else the bytes remaining to the window end.
    fn half_cap(&self, offset: u32) -> u32 {
        if self.split_at_half && offset < 128 {
            128 - offset
        } else {
            256 - offset
        }
    }
}

/// Why a range cannot be transferred as planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferRefusal {
    /// The range is malformed (empty, or beyond the 256-byte page window).
    InvalidRange(Range),
    /// The transfer limit is zero: no plan can be built from it.
    InvalidLimit,
}

/// Validate one range against the transfer limits (task 64's
/// `validate_range`): the range must be a legal window range and the
/// limits usable. Pure; nothing is widened or trimmed to fit.
pub fn validate_range(range: Range, limits: TransferLimits) -> Result<(), TransferRefusal> {
    if range.validate().is_err() {
        return Err(TransferRefusal::InvalidRange(range));
    }
    if limits.max_transfer_len == 0 {
        return Err(TransferRefusal::InvalidLimit);
    }
    Ok(())
}

/// One chosen transfer of a split (task 64's `TransferChoice`): the exact
/// physical span this step addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferChoice {
    /// The physical span this transfer addresses.
    pub range: Range,
}

/// Split a requested range into bounded transfers (task 64's
/// `transfer_choose`). Pure planning: each returned transfer is bounded by
/// the supplied `max_transfer_len`, never spans the 128-byte half boundary
/// when the limits say the transport reads one half per transfer, and
/// covers the requested bytes exactly — nothing is widened through an
/// effect barrier (barrier-respecting splits are the caller's per-byte
/// classifications feeding the request range; this reducer only splits
/// what it was given).
pub fn transfer_choose(
    request: Range,
    limits: TransferLimits,
) -> Result<Vec<TransferChoice>, TransferRefusal> {
    validate_range(request, limits)?;
    let mut choices = Vec::new();
    let mut offset = u32::from(request.offset);
    let end = request.end();
    while offset < end {
        let len = limits
            .half_cap(offset)
            .min(end - offset)
            .min(u32::from(limits.max_transfer_len));
        choices.push(TransferChoice {
            range: Range {
                offset: offset as u8,
                len: len as u16,
                ..request
            },
        });
        offset += len;
    }
    Ok(choices)
}

// ── Per-bit write masks (task 64) ──────────────────────────────────────────

/// The per-byte bit-level write permission, from the generated schema
/// metadata: which bits of the byte a write may touch. Bit `n` is
/// `1 << n`. A byte whose schema access is writable may still carry bits
/// this mask refuses (reserved bits inside a writable byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitAccess {
    /// Bits the byte's generated metadata marks writable.
    pub writable_mask: u8,
}

/// Why a bit-level write was refused. Typed with byte offset and bit
/// index: the caller learns the exact offending bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitRefusal {
    /// The request addresses no bytes.
    EmptyRange,
    /// The mask vector does not match the request's length.
    MaskMismatch {
        /// The request's byte count.
        expected: usize,
        /// The mismatching vector's length.
        got: usize,
    },
    /// The intended write touches a bit the byte's metadata does not mark
    /// writable: the first offending byte and bit.
    BitNotWritable {
        /// The offending byte's offset.
        offset: u8,
        /// The offending bit index within that byte.
        bit: u8,
    },
}

/// The bit-level validation decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitDecision {
    /// Every intended bit is writable under the supplied masks.
    Allowed,
    /// Refused with the typed reason; nothing was written.
    Refused(BitRefusal),
}

/// Validate a planned write bit-by-bit (task 64's per-bit write masks).
/// `masks` carries one [`BitAccess`] per byte of the request, and the
/// write's intent is expressed as one mask per byte (a read-modify-write
/// declares the bits it changes). Any intended bit outside the writable
/// mask refuses the whole write — fail closed, the first offending byte
/// and bit named. Pure: nothing is written, no value is needed (bit
/// permissions do not depend on values).
pub fn validate_write_bits(
    request: &WriteRequest,
    intent_masks: &[u8],
    masks: &[BitAccess],
) -> BitDecision {
    if request.range.len == 0 {
        return BitDecision::Refused(BitRefusal::EmptyRange);
    }
    let len = usize::from(request.range.len);
    if intent_masks.len() != len || masks.len() != len {
        return BitDecision::Refused(BitRefusal::MaskMismatch {
            expected: len,
            got: if intent_masks.len() != len {
                intent_masks.len()
            } else {
                masks.len()
            },
        });
    }
    for (index, (intent, permission)) in intent_masks.iter().zip(masks).enumerate() {
        if intent & !permission.writable_mask != 0 {
            let offending = intent & !permission.writable_mask;
            let bit = offending.trailing_zeros() as u8;
            return BitDecision::Refused(BitRefusal::BitNotWritable {
                offset: request.range.offset + index as u8,
                bit,
            });
        }
    }
    BitDecision::Allowed
}

// ── RMW prerequisites (task 64) ────────────────────────────────────────────

/// The exclusion contract covering one read-modify-write: the evidence the
/// executor holds that no other writer interleaves across the
/// read/write/confirmation. Two transport calls or a process mutex are NOT
/// proof of atomicity — only an actual exclusion contract is (task 63's
/// exclusive token, or a positively mutation-excluded observation window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionContract {
    /// An exclusive-session token held across the whole RMW (task 63's
    /// `ExclusiveToken`): the token must still be live at every step.
    ExclusiveToken {
        /// The exclusive-session token value.
        token: u64,
    },
    /// A positively mutation-excluded observation window over the same
    /// generation/epoch, supplied by the executor's own fencing.
    MutationExcluded {
        /// The generation the exclusion window covers.
        generation: ModuleGeneration,
        /// The epoch the exclusion window covers.
        epoch: CacheEpoch,
    },
}

/// The freshness facts one RMW consumes: where the read data came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmwReadFacts {
    /// The generation the read observation was stamped with.
    pub generation: ModuleGeneration,
    /// The epoch the read observation was stamped with.
    pub epoch: CacheEpoch,
}

/// Why RMW prerequisites were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmwRefusal {
    /// The caller supplied no read facts at all: a read-modify-write over
    /// missing data is refused — never a guessed zero or stale value.
    MissingRead,
    /// The read data is from another generation/epoch than the one the
    /// write targets: stale data must not be merged.
    StaleRead {
        /// The generation the read observation was stamped with.
        read_generation: ModuleGeneration,
        /// The epoch the read observation was stamped with.
        read_epoch: CacheEpoch,
    },
    /// The caller supplied no exclusion contract: without one, another
    /// writer may interleave between the read and the write and the RMW
    /// would silently lose its update.
    NoExclusion,
}

/// The RMW prerequisite decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmwDecision {
    /// Fresh same-generation/epoch data under a real exclusion contract:
    /// the RMW may be planned.
    Admitted,
    /// Refused with the typed reason; the write is never planned over it.
    Refused(RmwRefusal),
}

/// Gate one read-modify-write on its prerequisites (task 64). Pure: the
/// facts arrive supplied; the reducer refuses missing reads, stale-stamped
/// reads (against the write's own target stamp) and absent exclusion
/// contracts. The exclusion contract's own liveness at execution time is
/// the executor's check — this is the planning gate.
pub fn rmw_prerequisites(
    read: Option<RmwReadFacts>,
    exclusion: Option<ExclusionContract>,
    write_generation: ModuleGeneration,
    write_epoch: CacheEpoch,
) -> RmwDecision {
    let Some(read) = read else {
        return RmwDecision::Refused(RmwRefusal::MissingRead);
    };
    if !generation_valid(read.generation, read.epoch, write_generation, write_epoch) {
        return RmwDecision::Refused(RmwRefusal::StaleRead {
            read_generation: read.generation,
            read_epoch: read.epoch,
        });
    }
    match exclusion {
        Some(ExclusionContract::MutationExcluded { generation, epoch }) => {
            if generation_valid(generation, epoch, write_generation, write_epoch) {
                RmwDecision::Admitted
            } else {
                RmwDecision::Refused(RmwRefusal::NoExclusion)
            }
        }
        // A token contract's liveness is checked against the live session
        // at execution; its presence here admits the plan.
        Some(ExclusionContract::ExclusiveToken { .. }) => RmwDecision::Admitted,
        None => RmwDecision::Refused(RmwRefusal::NoExclusion),
    }
}

// ── Power / reset / protection plans (task 64) ─────────────────────────────
//
// Donor behavior, preserved with provenance: the byte-26 read-modify-write
// (plugins/cmis/power.c:81-117 `cmis_apply_power_state` — SoftwareReset is
// ALWAYS cleared on this path, squelch-method and bank-broadcast are
// preserved by the RMW, LowPwrRequestSW is set to force and cleared with
// LowPwrAllowHW on release) and the soft-reset sequence (power.c:239-309
// `cmis_reset` — RMW-set SoftwareReset, cache-epoch advance, then poll the
// bit's clear under the module poll cadence and readiness backstop). The
// bit masks themselves are SUPPLIED per call from the generated family
// descriptors (CMIS 5.3 byte 26: LowPwrRequestSW bit 4, LowPwrAllowHW bit
// 6, SoftwareReset bit 3, SquelchSel bit 5, BankBroadcast bit 7) — core
// holds no family table.

/// The control-register bit masks one power/reset plan consumes, supplied
/// by the family layer from the generated schema descriptors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerControlBits {
    /// The software low-power request bit (CMIS 5.3 byte 26 bit 4).
    pub low_pwr_request_sw: u8,
    /// The low-power-allow-hardware bit (CMIS 5.3 byte 26 bit 6).
    pub low_pwr_allow_hw: u8,
    /// The software-reset bit (CMIS 5.3 byte 26 bit 3).
    pub software_reset: u8,
}

impl PowerControlBits {
    /// All bits the power/reset RMW owns: exactly the three control bits.
    /// Everything else in the register is preserved by the RMW.
    pub fn owned_mask(self) -> u8 {
        self.low_pwr_request_sw | self.low_pwr_allow_hw | self.software_reset
    }
}

/// A requested power state (task 64: "requested versus committed power").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerRequest {
    /// Force low power: set the software low-power request (and clear the
    /// reset bit, as the donor always does on this path).
    ForceLowPower,
    /// Release the force: clear the software request AND the
    /// hardware-allow bit, so the host, not the unmanaged pin, stays
    /// authoritative (the donor's deliberate release semantics).
    Release,
}

/// One explicit step of a returned plan, as data for the daemon to
/// execute. Plans never execute: every step is a named authorization,
/// read, field-preserving write, confirmation or invalidation the caller
/// applies and reports back through the result reducers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationStep {
    /// An exact prerequisite read: never widened, requester-tagged, its
    /// allowed effects explicit.
    PrerequisiteRead {
        /// The read intent to execute.
        intent: super::ReadIntent,
    },
    /// A field-preserving write: the exact byte range, the fully computed
    /// new bytes, and the mask of bits the operation owns — every other
    /// bit carries the supplied current value unchanged.
    FieldPreservingWrite {
        /// The byte range the write addresses.
        range: Range,
        /// The fully computed new bytes.
        value: Vec<u8>,
        /// The bits this operation owns; all others are preserved.
        owned_mask: u8,
    },
    /// An exact confirmation read after the write.
    ConfirmationRead {
        /// The read intent to execute.
        intent: super::ReadIntent,
    },
    /// Advance the cache epoch: pre-write snapshots must never serve the
    /// post-write state (the donor bumps `cache_epoch` after every
    /// successful byte-26 write and after the reset write).
    AdvanceCacheEpoch,
}

/// One planned operation: ordered steps plus the stamp it must run under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPlan {
    /// The ordered steps, in execution order.
    pub steps: Vec<OperationStep>,
    /// The caller-minted operation identity the plan is stamped with; a
    /// result naming another operation never advances or commits it, even
    /// under the same module generation (fresh, strictly increasing
    /// `OperationId`s per module instance, as every consumer of the
    /// driver's counter convention mints them).
    pub identity: OperationId,
    /// The generation the plan is stamped with; a result from another
    /// generation never commits it.
    pub generation: ModuleGeneration,
    /// The epoch the plan is stamped with.
    pub epoch: CacheEpoch,
}

/// The pure RMW byte computation for a power request (the donor's exact
/// register arithmetic, over supplied bits): the reset bit is always
/// cleared on the power path, force sets the software request, release
/// clears the software request and the hardware-allow bit. Every bit
/// outside the owned mask is preserved from `current`.
pub fn power_byte(current: u8, request: PowerRequest, bits: PowerControlBits) -> (u8, u8) {
    let mut next = current & !bits.software_reset;
    match request {
        PowerRequest::ForceLowPower => next |= bits.low_pwr_request_sw,
        PowerRequest::Release => {
            next &= !(bits.low_pwr_request_sw | bits.low_pwr_allow_hw);
        }
    }
    (next, bits.owned_mask())
}

/// The supplied facts one power/reset/protection plan consumes (the plan's
/// own `facts`/`inputs` parameter, bundled): the fresh control byte, the
/// RMW freshness and exclusion facts, the family-supplied control bits,
/// and the stamp the plan must run under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerFacts {
    /// The freshly read control-register byte, when the caller holds one.
    pub current_byte: Option<u8>,
    /// The read observation's freshness facts (RMW prerequisite).
    pub rmw_read: Option<RmwReadFacts>,
    /// The exclusion contract across the RMW.
    pub exclusion: Option<ExclusionContract>,
    /// The family-supplied control-register bit masks.
    pub bits: PowerControlBits,
    /// The caller-minted identity stamped onto the resulting plan; every
    /// result the plan's driver accepts must name it back.
    pub identity: OperationId,
    /// The generation the plan is stamped with.
    pub generation: ModuleGeneration,
    /// The epoch the plan is stamped with.
    pub epoch: CacheEpoch,
}

/// The power operation state the caller stores between steps (task 64's
/// requested-versus-committed distinction). Plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// No power operation in flight.
    Idle,
    /// A plan was issued with this request under this power generation;
    /// its result has not arrived.
    Pending {
        /// The requested state.
        request: PowerRequest,
        /// The power generation the plan was issued under.
        power_generation: PowerGeneration,
    },
    /// A confirmed commit: the requested state is the committed state.
    Committed {
        /// The committed request.
        request: PowerRequest,
        /// The power generation the commit was confirmed under.
        power_generation: PowerGeneration,
    },
}

/// Why a power plan was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerRefusal {
    /// The RMW prerequisites failed (typed): missing/stale read data or no
    /// exclusion contract.
    Rmw(RmwRefusal),
    /// The control bits supplied are unusable for the request (a zero
    /// owned mask proves no descriptor resolved).
    MissingControlBits,
}

/// What [`power_request_step`] returned: the plan to execute, or the typed
/// refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerPlanDecision {
    /// Execute this plan and feed the outcome to [`power_result_step`].
    Plan(OperationPlan),
    /// The new request is refused; the previous state stands.
    Refused(PowerRefusal),
    /// A new request arrived while a release is pending: rechecked before
    /// completion, the force wins — the caller receives the new plan and
    /// the pending release's result can no longer commit (a stale release
    /// result never clears a newer request).
    SupersedesPending {
        /// The superseding plan to execute.
        plan: OperationPlan,
        /// The power generation of the release being superseded.
        superseded_generation: PowerGeneration,
    },
}

/// Plan one power request (task 64). Pure over the supplied current byte,
/// control bits, RMW facts and the previous state. The plan is the donor's
/// byte-26 RMW: one exact prerequisite read is implied by the caller's
/// supplied current byte (the daemon supplies the fresh read it holds —
/// the RMW gate refused stale data), the write is field-preserving, and a
/// successful commit advances the cache epoch. A request arriving while a
/// release is pending is rechecked before completion and supersedes it.
pub fn power_request_step(
    previous: PowerState,
    request: PowerRequest,
    facts: PowerFacts,
) -> PowerPlanDecision {
    if facts.bits.owned_mask() == 0 {
        return PowerPlanDecision::Refused(PowerRefusal::MissingControlBits);
    }
    let Some(current) = facts.current_byte else {
        return PowerPlanDecision::Refused(PowerRefusal::Rmw(RmwRefusal::MissingRead));
    };
    if let RmwDecision::Refused(reason) = rmw_prerequisites(
        facts.rmw_read,
        facts.exclusion,
        facts.generation,
        facts.epoch,
    ) {
        return PowerPlanDecision::Refused(PowerRefusal::Rmw(reason));
    }
    let control_range = Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 26,
        len: 1,
    };
    let (value, owned_mask) = power_byte(current, request, facts.bits);
    let plan = OperationPlan {
        steps: vec![
            OperationStep::PrerequisiteRead {
                intent: super::ReadIntent {
                    requester: super::ReadRequester::ExplicitClient,
                    exact_ranges: vec![control_range],
                    allowed_effects: vec![super::ReadEffect::None],
                },
            },
            OperationStep::FieldPreservingWrite {
                range: control_range,
                value: vec![value],
                owned_mask,
            },
            OperationStep::AdvanceCacheEpoch,
        ],
        identity: facts.identity,
        generation: facts.generation,
        epoch: facts.epoch,
    };
    match previous {
        PowerState::Pending {
            request: PowerRequest::Release,
            power_generation,
        } if request == PowerRequest::ForceLowPower => PowerPlanDecision::SupersedesPending {
            plan,
            superseded_generation: power_generation,
        },
        _ => PowerPlanDecision::Plan(plan),
    }
}

/// What [`power_result_step`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerResultDecision {
    /// A confirmed outcome under the matching power generation commits:
    /// requested becomes committed.
    Committed,
    /// A failed outcome invalidates the relevant knowledge and stays
    /// retryable: a new plan may be issued.
    FailedRetryable,
    /// An uncertain outcome (the write may or may not have applied) stays
    /// uncertain: it is never replayed as if it had failed before
    /// applying, and the caller must confirm before any new plan.
    UncertainPending,
    /// The result's power generation does not match the pending operation:
    /// a stale result never commits or clears a newer request.
    StaleResult,
}

/// Reduce one power-operation result (task 64). Pure: exact
/// same-generation matching — only a confirmed outcome of the matching
/// `PowerGeneration` commits; failure is retryable; uncertainty stays
/// uncertain; a stale generation is discarded without touching the state.
pub fn power_result_step(
    previous: PowerState,
    power_generation: PowerGeneration,
    outcome: &ActuationOutcome,
) -> (PowerState, PowerResultDecision) {
    match previous {
        PowerState::Pending {
            request,
            power_generation: pending,
        } => {
            if power_generation != pending {
                return (previous, PowerResultDecision::StaleResult);
            }
            match outcome {
                ActuationOutcome::Confirmed => (
                    PowerState::Committed {
                        request,
                        power_generation,
                    },
                    PowerResultDecision::Committed,
                ),
                ActuationOutcome::Failed { .. } => {
                    (PowerState::Idle, PowerResultDecision::FailedRetryable)
                }
                ActuationOutcome::Uncertain { .. } => {
                    (previous, PowerResultDecision::UncertainPending)
                }
            }
        }
        _ => (previous, PowerResultDecision::StaleResult),
    }
}

/// The module state a power reconciliation consumes — the SUPPLIED
/// DECODED state from the generated revision decode (standard-correct:
/// code 4 is ModulePwrDn and code 5 is ModuleFault). The donor's own
/// `CMIS_MOD_STATE_*` mapping (reconcile.h:28-33) calls code 4 FAULT; the
/// library consumes the generated decode instead, never the donor's
/// erroneous one (the module-state-4-5 parity difference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileModuleState {
    /// Standard code 1: the module is in low power.
    LowPwr,
    /// Standard code 2: the module is powering up.
    PwrUp,
    /// Standard code 3: the module is ready.
    Ready,
    /// Standard code 4: a transient return toward low power. The donor
    /// treats this code as a fault and clears the force; the
    /// standard-correct reconcile reports the module still transitioning
    /// and KEEPS the force intent (the module is complying) — a
    /// documented divergence from the donor, owned by the
    /// module-state-4-5 parity difference.
    PwrDn,
    /// Standard code 5: a real module fault; the forced claim is cleared.
    Fault,
}

/// What one reconciliation decided, as data: what the caller publishes and
/// whether the forced intent is cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileDecision {
    /// Publish this status; the force intent keeps or clears as named.
    Publish {
        /// The low-power status to publish.
        low_power: bool,
        /// Whether the forced intent is cleared.
        clear_force: bool,
    },
    /// A guard tripped: the caller leaves the port alone (no
    /// publication). The donor's guards are preserved exactly: no force
    /// intent pending, or a real actuated force owns the state (committed
    /// generation equals the request generation).
    Guarded,
}

/// The supplied forced-default bind reconcile facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileFacts {
    /// Whether a forced low-power intent is pending at all (donor
    /// `force_lowpwr`).
    pub force_lowpwr: bool,
    /// The forced intent's generation (donor `force_lowpwr_gen`).
    pub force_generation: PowerGeneration,
    /// The generation of the last confirmed actuation (donor
    /// `force_lowpwr_committed_gen`).
    pub committed_generation: PowerGeneration,
}

/// Pure read-only bind reconciliation over the supplied decoded state
/// (task 64; donor `cmis_lpwr_reconcile_decide`, plugins/cmis/reconcile.h:44-90).
/// The donor's guards are preserved verbatim: no pending force publishes
/// nothing, and a committed actuation of the current request generation
/// owns the state. LowPwr keeps the intent (the module is low through the
/// pin or default, not through an actuation, so nothing is committed);
/// PwrUp/Ready/Fault clear it (the module demonstrably escaped the forced
/// claim). PwrDn is the standard-correct divergence described on
/// [`ReconcileModuleState::PwrDn`]: the module is complying on its way
/// down, so the intent is kept while the caller reports the transition.
pub fn reconcile_power(state: ReconcileModuleState, facts: ReconcileFacts) -> ReconcileDecision {
    if !facts.force_lowpwr {
        return ReconcileDecision::Guarded;
    }
    if facts.committed_generation == facts.force_generation
        && facts.force_generation != PowerGeneration(0)
    {
        return ReconcileDecision::Guarded;
    }
    match state {
        ReconcileModuleState::LowPwr => ReconcileDecision::Publish {
            low_power: true,
            clear_force: false,
        },
        ReconcileModuleState::PwrDn => ReconcileDecision::Publish {
            low_power: true,
            clear_force: false,
        },
        ReconcileModuleState::PwrUp => ReconcileDecision::Publish {
            low_power: false,
            clear_force: true,
        },
        ReconcileModuleState::Ready => ReconcileDecision::Publish {
            low_power: false,
            clear_force: true,
        },
        ReconcileModuleState::Fault => ReconcileDecision::Publish {
            low_power: false,
            clear_force: true,
        },
    }
}

/// The reset request (task 64): a soft reset is explicit authorized work
/// with a supplied finite deadline; a hard reset is a typed unsupported —
/// the donor returns `-EOPNOTSUPP` (power.c:244-246: "Hard reset via GPIO
/// not yet supported"), and an unqualified capability stays refused here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetRequest {
    /// A soft (register) reset, planned by [`plan_reset`].
    Soft,
    /// A hard (GPIO) reset: typed unsupported by this path.
    Hard,
}

/// The reset plan's supplied policy: the poll cadence and backstop the
/// caller resolved from the schema's host-policy constants (CMIS:
/// HOST_POLICY_POLL_CADENCE_MS and HOST_POLICY_READINESS_BACKSTOP_MS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetPolicy {
    /// Poll cadence while waiting for the reset bit to clear.
    pub poll_interval: MonoDuration,
    /// The finite backstop anchored at the reset write: expiry is a typed
    /// timeout, never an unbounded wait.
    pub backstop: MonoDuration,
}

/// Why a reset plan was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetRefusal {
    /// Hard reset is not supported by this path (typed, the donor's own
    /// refusal) — a board GPIO route is never invented.
    HardResetUnsupported,
    /// The RMW prerequisites failed (typed).
    Rmw(RmwRefusal),
    /// The control bits supplied are unusable.
    MissingControlBits,
    /// The policy is unusable: a zero poll interval or backstop, or an
    /// interval exceeding the backstop.
    InvalidPolicy,
}

/// The supplied inputs of one reset plan (the plan's own `inputs`
/// parameter, bundled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetInputs {
    /// Which reset was requested.
    pub request: ResetRequest,
    /// The supplied power/control facts.
    pub facts: PowerFacts,
    /// The reset poll/backstop policy resolved from the schema's
    /// host-policy constants.
    pub policy: ResetPolicy,
}

/// What [`plan_reset`] returned: the plan (whose confirmation step is the
/// poll of the reset bit's clear under the supplied cadence and backstop),
/// or the typed refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetPlanDecision {
    /// The soft-reset plan to execute.
    Plan(OperationPlan),
    /// The reset is refused with the typed reason.
    Refused(ResetRefusal),
}

/// Plan one soft reset (task 64; donor `cmis_reset`, power.c:239-309).
/// Pure: the plan RMW-sets the software-reset bit (preserving every other
/// bit), advances the cache epoch after the write, and names the exact
/// confirmation read (poll the bit's clear) bounded by the supplied
/// backstop; a hard reset is refused with the typed reason.
pub fn plan_reset(inputs: ResetInputs) -> ResetPlanDecision {
    match inputs.request {
        ResetRequest::Hard => {
            return ResetPlanDecision::Refused(ResetRefusal::HardResetUnsupported)
        }
        ResetRequest::Soft => {}
    }
    if inputs.facts.bits.software_reset == 0 {
        return ResetPlanDecision::Refused(ResetRefusal::MissingControlBits);
    }
    if inputs.policy.poll_interval.0 == 0
        || inputs.policy.backstop.0 == 0
        || inputs.policy.poll_interval > inputs.policy.backstop
    {
        return ResetPlanDecision::Refused(ResetRefusal::InvalidPolicy);
    }
    let Some(current) = inputs.facts.current_byte else {
        return ResetPlanDecision::Refused(ResetRefusal::Rmw(RmwRefusal::MissingRead));
    };
    if let RmwDecision::Refused(reason) = rmw_prerequisites(
        inputs.facts.rmw_read,
        inputs.facts.exclusion,
        inputs.facts.generation,
        inputs.facts.epoch,
    ) {
        return ResetPlanDecision::Refused(ResetRefusal::Rmw(reason));
    }
    let control_range = Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 26,
        len: 1,
    };
    let value = current | inputs.facts.bits.software_reset;
    let plan = OperationPlan {
        steps: vec![
            OperationStep::FieldPreservingWrite {
                range: control_range,
                value: vec![value],
                owned_mask: inputs.facts.bits.software_reset,
            },
            OperationStep::AdvanceCacheEpoch,
            OperationStep::ConfirmationRead {
                intent: super::ReadIntent {
                    requester: super::ReadRequester::ExplicitClient,
                    exact_ranges: vec![control_range],
                    allowed_effects: vec![super::ReadEffect::None],
                },
            },
        ],
        identity: inputs.facts.identity,
        generation: inputs.facts.generation,
        epoch: inputs.facts.epoch,
    };
    ResetPlanDecision::Plan(plan)
}

/// The thermal-protection request (task 45/64): an explicitly decided
/// verdict/trigger with its authorization, capabilities and enforcement
/// route facts, all supplied. Nothing here re-decides the thermal verdict
/// (task 29 owns that) or reinterprets the donor-comparison field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectionRequest {
    /// Whether the decided trigger is a high-alarm verdict (true) or a
    /// threshold-decided alarm the donor field would not act on (false) —
    /// both may yield a qualified plan; the distinction is preserved for
    /// the caller's records.
    pub high_alarm: bool,
}

/// The enforcement route facts the caller resolved: whether the family's
/// generated capability descriptors qualify a protection route at all.
/// Qualified backend-route facts only — an invented board GPIO mapping is
/// never a fact (task 64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionRoute {
    /// The module supports a software force-low-power control (CMIS byte
    /// 26 at the supplied bits): the qualified plan is the byte-26 force.
    SoftwareLowPower,
    /// The caller positively established that no qualified route exists
    /// for this module/family (including SFF families without a
    /// protection control): typed unsupported, never an invented one.
    NoQualifiedRoute,
}

/// The typed unsupported marker (task 64's `ProtectionUnsupported`): the
/// decided trigger yields no executable protection effect because no
/// qualified route exists. Distinct from a refusal to decide: the verdict
/// stands, only the enforcement is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectionUnsupported {
    /// The decided trigger this marker preserves for the caller's record.
    pub request: ProtectionRequest,
}

/// The supplied inputs of one protection plan (the plan's own `inputs`
/// parameter, bundled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectionInputs {
    /// The explicitly decided trigger this plan consumes.
    pub request: ProtectionRequest,
    /// The caller-resolved enforcement route.
    pub route: ProtectionRoute,
    /// The supplied power/control facts for the software route.
    pub facts: PowerFacts,
}

/// What [`plan_protection`] returned. No effect is executed; the plan is
/// data, and the outcome flows back through task 29's latch reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectionDecision {
    /// Execute this plan (the qualified family-specific low-power route).
    Plan(OperationPlan),
    /// No qualified route exists: the decided trigger is preserved, no
    /// effect is invented.
    Unsupported(ProtectionUnsupported),
}

/// Plan one thermal-protection actuation (task 64). Pure over the
/// explicitly supplied decided trigger, control bits, RMW facts and route.
/// An alarm — including a threshold-decided alarm the donor's
/// `kmod_action` field leaves empty — may yield the qualified
/// family-specific low-power plan or [`ProtectionUnsupported`]; nothing is
/// executed here and the thermal latch/commit/release reductions stay
/// solely with task 29's owner.
pub fn plan_protection(inputs: ProtectionInputs) -> ProtectionDecision {
    match inputs.route {
        ProtectionRoute::NoQualifiedRoute => {
            ProtectionDecision::Unsupported(ProtectionUnsupported {
                request: inputs.request,
            })
        }
        ProtectionRoute::SoftwareLowPower => {
            match power_request_step(PowerState::Idle, PowerRequest::ForceLowPower, inputs.facts) {
                PowerPlanDecision::Plan(plan) => ProtectionDecision::Plan(plan),
                PowerPlanDecision::SupersedesPending { plan, .. } => ProtectionDecision::Plan(plan),
                // The protection path inherits the power plan's typed
                // refusals: with no plan there is nothing to execute, and the
                // caller records the trigger as unenforced rather than
                // inventing an effect.
                PowerPlanDecision::Refused(_) => {
                    ProtectionDecision::Unsupported(ProtectionUnsupported {
                        request: inputs.request,
                    })
                }
            }
        }
    }
}

// ── Operation driver over the plan steps (task 64) ─────────────────────────
//
// The donor's sysfs request/commit/rollback sequence (core/sysfs.c:711-927
// `xcvr_force_lowpwr_set`): a changed request stamps a generation and sets
// its claim optimistically before any hardware effect; every result is
// then re-checked against the caller's LIVE generation — a failure rolls
// the claim back only while the operation still owns the generation, a
// success publishes its committed generation only under the same
// equality, and a newer request supersedes both paths and owns the state
// alone. This driver is that sequence over an [`OperationPlan`]: the
// caller executes steps and reports outcomes with its live generation,
// the driver returns the next step, the compensating write, or the
// terminal decision. No queue, timer, clock or generation counter is
// owned here — the caller's counter is a supplied input.

/// The caller-stored driver state for one in-flight [`OperationPlan`]
/// (task 64's `OperationState`). Plain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationState {
    /// No plan in flight.
    Idle,
    /// A plan was requested under its stamp and is being executed from
    /// `cursor`. The donor sets its knob and bumps its generation here,
    /// before any hardware effect (the optimistic request).
    Requested {
        /// The requested plan; its steps execute in order.
        plan: OperationPlan,
        /// The index of the next unconfirmed step.
        cursor: usize,
    },
    /// Every step confirmed while the caller's live generation still
    /// matched the plan's stamp: the donor publishes its committed
    /// generation under the same equality test and nothing else.
    Committed {
        /// The generation the committed plan was stamped with.
        generation: ModuleGeneration,
    },
    /// A step failed while the operation still owned the generation and
    /// the compensating write was issued. The donor's failure rollback
    /// restores its own flag and status; over a plan the caller's claim is
    /// retracted and, for a failed field-preserving write, the supplied
    /// pre-write bytes are restored instead. The caller re-stamps its own
    /// request generation (the donor's `gen - 1` restore): the driver owns
    /// no counter.
    RolledBack {
        /// The generation the rolled-back plan was stamped with.
        generation: ModuleGeneration,
    },
    /// The operation was superseded by a newer request: neither rolled
    /// back nor committed — the donor's rollback and commit gates both
    /// test generation equality and do nothing when it moved; the newer
    /// request owns the caller's state alone.
    Superseded {
        /// The superseded plan's stamp, kept for the caller's records.
        generation: ModuleGeneration,
    },
}

/// One reported step result, as the donor reports its actuator's return
/// with the live generation and cache epoch read under its state lock,
/// naming the operation and the step the result belongs to. Supplied by
/// the caller on every step; the driver holds no clock or counter of its
/// own. A result that names another operation, another step, or a cache
/// epoch other than the one in effect for the current step is refused
/// without any state change (see [`operation_step`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationEvent<'a> {
    /// The step at the cursor confirmed under this live stamp.
    Confirmed {
        /// The caller's live generation at the result.
        live_generation: ModuleGeneration,
        /// The operation the reported step belongs to (the plan's
        /// `identity`).
        operation: OperationId,
        /// The index of the step whose result this is.
        step: usize,
        /// The caller's live cache epoch at the result.
        epoch: CacheEpoch,
    },
    /// The step at the cursor failed under this live stamp. The pre-write
    /// bytes of the failed write's range are supplied when the caller
    /// holds them: the compensating write restores exactly those, and the
    /// driver never re-reads or invents bytes.
    Failed {
        /// The caller's live generation at the result.
        live_generation: ModuleGeneration,
        /// The operation the reported step belongs to (the plan's
        /// `identity`).
        operation: OperationId,
        /// The index of the step whose result this is.
        step: usize,
        /// The caller's live cache epoch at the result.
        epoch: CacheEpoch,
        /// The pre-write bytes of the failed write step's range, when the
        /// caller holds them.
        original: Option<&'a [u8]>,
    },
    /// The step's outcome is uncertain (the effect may or may not have
    /// applied): never replayed as if it had failed before applying, and
    /// never rolled back on the strength of it.
    Uncertain {
        /// The caller's live generation at the result.
        live_generation: ModuleGeneration,
        /// The operation the reported step belongs to (the plan's
        /// `identity`).
        operation: OperationId,
        /// The index of the step whose result this is.
        step: usize,
        /// The caller's live cache epoch at the result.
        epoch: CacheEpoch,
    },
}

/// What [`operation_step`] decided, as data for the caller: the next step
/// to execute, the compensating write, or the terminal outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationDecision {
    /// Execute this next step of the plan, in order.
    Continue(OperationStep),
    /// The plan completed: every step confirmed under the matching
    /// generation (the donor's committed-generation publish).
    Committed,
    /// A step failed while the operation still owned the generation.
    /// Issue the returned compensating write when there is one — a failed
    /// field-preserving write restores the caller's supplied pre-write
    /// bytes over the same range and owned mask; a failure with no
    /// supplied bytes, a length mismatch, or a non-write step retracts
    /// only the caller's request and never invents register bytes.
    Rollback {
        /// The compensating write to issue, when the caller supplied the
        /// failed write's pre-write bytes; `None` retracts only the
        /// caller's request.
        restore: Option<OperationStep>,
    },
    /// A newer request superseded this operation: no rollback, no
    /// commit; the newer request owns the state.
    Superseded,
    /// The outcome is uncertain: the operation stays pending at this
    /// step. Nothing is replayed or rolled back until the caller confirms.
    UncertainPending,
    /// A result arrived with no plan in flight, after a terminal
    /// decision, or past the plan's last step: it never commits or
    /// clears anything.
    StaleResult,
    /// A result that names another operation, another step, or a cache
    /// epoch other than the one in effect for the current step: refused
    /// with the typed mismatch and no state change — the plan stays
    /// pending at its cursor and nothing is advanced, committed, rolled
    /// back or cleared. A late or replayed result from a different
    /// operation never advances the current plan, even under the same
    /// module generation; the caller re-drives the current step under a
    /// fresh result or supersedes the plan itself.
    RefusedResult(OperationEventRefusal),
}

/// Why a reported step result was refused (typed, fail closed): the
/// event's identity, step or epoch did not match the in-flight plan's
/// current step. Each variant names what arrived and what the current
/// plan's step expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationEventRefusal {
    /// The result names an operation other than the in-flight plan.
    Operation {
        /// The operation the result named.
        event: OperationId,
        /// The in-flight plan's stamped identity.
        plan: OperationId,
    },
    /// The result names a step other than the cursor's step (a late,
    /// duplicated or out-of-order step result).
    Step {
        /// The step index the result named.
        event: usize,
        /// The index of the next unconfirmed step.
        cursor: usize,
    },
    /// The result was observed under a cache epoch other than the one in
    /// effect for the current step.
    Epoch {
        /// The epoch the result was observed under.
        event: CacheEpoch,
        /// The epoch in effect for the current step: the plan's stamp,
        /// advanced by every `AdvanceCacheEpoch` step up to and including
        /// the cursor's (an epoch-advance step is confirmed under the
        /// epoch it advanced to).
        expected: CacheEpoch,
    },
}

impl OperationEvent<'_> {
    /// The caller's live generation this event was reported under.
    pub fn live_generation(&self) -> ModuleGeneration {
        match *self {
            OperationEvent::Confirmed {
                live_generation, ..
            }
            | OperationEvent::Failed {
                live_generation, ..
            }
            | OperationEvent::Uncertain {
                live_generation, ..
            } => live_generation,
        }
    }

    /// The operation, step index and live cache epoch this event was
    /// reported under.
    fn event_stamp(&self) -> (OperationId, usize, CacheEpoch) {
        match *self {
            OperationEvent::Confirmed {
                operation,
                step,
                epoch,
                ..
            }
            | OperationEvent::Failed {
                operation,
                step,
                epoch,
                ..
            }
            | OperationEvent::Uncertain {
                operation,
                step,
                epoch,
                ..
            } => (operation, step, epoch),
        }
    }
}

/// The cache epoch in effect for the plan's step at `cursor`: the plan's
/// stamp advanced by every `AdvanceCacheEpoch` step at or before the
/// cursor — an epoch-advance step is confirmed under the epoch it
/// advanced to, and the steps after it run under that same advanced
/// epoch. Pure over the plan and cursor; the caller's live epoch must
/// equal this for a result to count for the step.
fn step_epoch(plan: &OperationPlan, cursor: usize) -> CacheEpoch {
    CacheEpoch(
        plan.epoch.0
            + plan
                .steps
                .iter()
                .take(cursor + 1)
                .filter(|step| matches!(step, OperationStep::AdvanceCacheEpoch))
                .count() as u64,
    )
}

/// Drive one plan step (task 64). Pure over the caller-stored state and
/// the supplied event, exactly the donor's sysfs sequence: a live
/// generation that moved off the plan's stamp retires the operation as
/// superseded for every outcome (the newer request owns the state); under
/// the matching generation a result is accepted only when it names the
/// in-flight plan's own operation identity, the cursor's step, and the
/// cache epoch in effect for that step — anything else is refused with a
/// typed mismatch and no state change (a late or replayed result from
/// another operation never advances or commits the current plan, even
/// under the same module generation, and a result observed under a moved
/// epoch never counts for a step of the old one). An accepted confirmed
/// step advances the cursor and a fully-confirmed plan commits, an
/// accepted failed step rolls the operation back (with the compensating
/// write whenever the caller supplied the pre-write bytes), and an
/// accepted uncertain step leaves the operation pending — never replayed,
/// never rolled back. Results arriving with nothing in flight, after a
/// terminal decision, or past the last step are stale and change nothing.
pub fn operation_step(
    previous: OperationState,
    event: OperationEvent<'_>,
) -> (OperationState, OperationDecision) {
    let OperationState::Requested { plan, cursor } = previous else {
        return (previous, OperationDecision::StaleResult);
    };
    let Some(step) = plan.steps.get(cursor) else {
        return (
            OperationState::Requested { plan, cursor },
            OperationDecision::StaleResult,
        );
    };
    if event.live_generation() != plan.generation {
        return (
            OperationState::Superseded {
                generation: plan.generation,
            },
            OperationDecision::Superseded,
        );
    }
    // The identity gate, fail closed: a result that names another
    // operation, another step, or a cache epoch other than the one in
    // effect for the cursor's step is refused with the typed mismatch and
    // the state is returned unchanged — nothing advances, commits, rolls
    // back or clears on the strength of a foreign or stale result.
    let (event_operation, event_step, event_epoch) = event.event_stamp();
    if event_operation != plan.identity {
        let refusal = OperationEventRefusal::Operation {
            event: event_operation,
            plan: plan.identity,
        };
        return (
            OperationState::Requested { plan, cursor },
            OperationDecision::RefusedResult(refusal),
        );
    }
    if event_step != cursor {
        return (
            OperationState::Requested { plan, cursor },
            OperationDecision::RefusedResult(OperationEventRefusal::Step {
                event: event_step,
                cursor,
            }),
        );
    }
    let expected_epoch = step_epoch(&plan, cursor);
    if event_epoch != expected_epoch {
        return (
            OperationState::Requested { plan, cursor },
            OperationDecision::RefusedResult(OperationEventRefusal::Epoch {
                event: event_epoch,
                expected: expected_epoch,
            }),
        );
    }
    match event {
        OperationEvent::Confirmed { .. } => {
            let next_step = plan.steps.get(cursor + 1).cloned();
            if let Some(next_step) = next_step {
                (
                    OperationState::Requested {
                        plan,
                        cursor: cursor + 1,
                    },
                    OperationDecision::Continue(next_step),
                )
            } else {
                (
                    OperationState::Committed {
                        generation: plan.generation,
                    },
                    OperationDecision::Committed,
                )
            }
        }
        OperationEvent::Uncertain { .. } => (
            OperationState::Requested { plan, cursor },
            OperationDecision::UncertainPending,
        ),
        OperationEvent::Failed { original, .. } => {
            let restore = match (step, original) {
                (
                    OperationStep::FieldPreservingWrite {
                        range, owned_mask, ..
                    },
                    Some(original),
                ) if original.len() == usize::from(range.len) => {
                    Some(OperationStep::FieldPreservingWrite {
                        range: *range,
                        value: original.to_vec(),
                        owned_mask: *owned_mask,
                    })
                }
                _ => None,
            };
            (
                OperationState::RolledBack {
                    generation: plan.generation,
                },
                OperationDecision::Rollback { restore },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: MonoTime = MonoTime(1_000);

    fn failed() -> ActuationOutcome {
        ActuationOutcome::Failed {
            reason: "nack".to_owned(),
        }
    }

    fn uncertain() -> ActuationOutcome {
        ActuationOutcome::Uncertain {
            reason: "timeout after write".to_owned(),
        }
    }

    #[test]
    fn a_zero_retry_budget_is_terminal_for_everything() {
        // max_retries 0 preserves the delivered zero-retry behaviour.
        let policy = BoundedRetryPolicy {
            max_retries: 0,
            deadline: None,
            idempotent: true,
        };
        for (outcome, attempt) in [
            (ActuationOutcome::Confirmed, 0u32),
            (failed(), 0),
            (uncertain(), 0),
        ] {
            assert_eq!(
                retry_decide_bounded(outcome.clone(), attempt, NOW, policy),
                BoundedRetryDecision::Terminal(outcome),
            );
        }
    }

    #[test]
    fn an_idempotent_failure_retries_within_budget() {
        let policy = BoundedRetryPolicy {
            max_retries: 2,
            deadline: None,
            idempotent: true,
        };
        assert_eq!(
            retry_decide_bounded(failed(), 0, NOW, policy),
            BoundedRetryDecision::Retry {
                next_attempt: 1,
                deadline: None
            }
        );
        assert_eq!(
            retry_decide_bounded(failed(), 1, NOW, policy),
            BoundedRetryDecision::Retry {
                next_attempt: 2,
                deadline: None
            }
        );
        // Budget exhausted: attempt == max_retries is terminal.
        assert_eq!(
            retry_decide_bounded(failed(), 2, NOW, policy),
            BoundedRetryDecision::Terminal(failed())
        );
    }

    #[test]
    fn a_non_idempotent_failure_is_never_retried() {
        let policy = BoundedRetryPolicy {
            max_retries: 5,
            deadline: None,
            idempotent: false,
        };
        assert_eq!(
            retry_decide_bounded(failed(), 0, NOW, policy),
            BoundedRetryDecision::Terminal(failed())
        );
    }

    #[test]
    fn an_uncertain_outcome_is_never_replayed() {
        let policy = BoundedRetryPolicy {
            max_retries: 5,
            deadline: None,
            idempotent: true,
        };
        assert_eq!(
            retry_decide_bounded(uncertain(), 0, NOW, policy),
            BoundedRetryDecision::Terminal(uncertain())
        );
    }

    #[test]
    fn a_passed_deadline_stops_the_retries() {
        let policy = BoundedRetryPolicy {
            max_retries: 5,
            deadline: Some(MonoTime(2_000)),
            idempotent: true,
        };
        // Before the deadline: retry.
        assert!(matches!(
            retry_decide_bounded(failed(), 0, MonoTime(1_999), policy),
            BoundedRetryDecision::Retry { .. }
        ));
        // At the deadline a retry may no longer *start*; one tick before
        // it could. At/after it the outcome is terminal.
        assert_eq!(
            retry_decide_bounded(failed(), 0, MonoTime(2_000), policy),
            BoundedRetryDecision::Terminal(failed())
        );
        assert_eq!(
            retry_decide_bounded(failed(), 0, MonoTime(2_001), policy),
            BoundedRetryDecision::Terminal(failed())
        );
    }

    #[test]
    fn a_confirmed_outcome_is_terminal_even_for_retriable_policies() {
        let policy = BoundedRetryPolicy {
            max_retries: 5,
            deadline: None,
            idempotent: true,
        };
        assert_eq!(
            retry_decide_bounded(ActuationOutcome::Confirmed, 0, NOW, policy),
            BoundedRetryDecision::Terminal(ActuationOutcome::Confirmed)
        );
    }
}

#[cfg(test)]
mod validate_write_tests {
    use super::*;
    use crate::field_catalog::AccessKind;

    fn request(offset: u8, len: u16) -> WriteRequest {
        WriteRequest {
            range: Range {
                address: 0x50,
                bank: 0,
                page: 0x00,
                offset,
                len,
            },
        }
    }

    fn byte(access: AccessKind) -> ByteAccess {
        ByteAccess { access }
    }

    #[test]
    fn writable_bytes_validate() {
        for access in [
            AccessKind::Rw,
            AccessKind::Rww,
            AccessKind::Wo,
            AccessKind::WoSc,
        ] {
            assert_eq!(
                validate_write(&request(26, 2), &[byte(access), byte(access)]),
                AccessDecision::Allowed,
                "{access:?}"
            );
        }
    }

    #[test]
    fn read_only_and_unknown_bytes_refuse_with_the_first_offset() {
        assert_eq!(
            validate_write(
                &request(26, 3),
                &[
                    byte(AccessKind::Rw),
                    byte(AccessKind::Ro),
                    byte(AccessKind::Wo)
                ]
            ),
            AccessDecision::Refused(WriteRefusal::ReadOnlyByte { offset: 27 })
        );
        assert_eq!(
            validate_write(
                &request(26, 3),
                &[
                    byte(AccessKind::Rw),
                    byte(AccessKind::Rw),
                    byte(AccessKind::Unknown)
                ]
            ),
            AccessDecision::Refused(WriteRefusal::UnknownAccessByte { offset: 28 })
        );
        // ro_cor is read-only too.
        assert_eq!(
            validate_write(&request(3, 1), &[byte(AccessKind::RoCor)]),
            AccessDecision::Refused(WriteRefusal::ReadOnlyByte { offset: 3 })
        );
    }

    #[test]
    fn empty_ranges_and_mismatched_descriptors_refuse() {
        assert_eq!(
            validate_write(&request(26, 0), &[]),
            AccessDecision::Refused(WriteRefusal::EmptyRange)
        );
        assert_eq!(
            validate_write(&request(26, 2), &[byte(AccessKind::Rw)]),
            AccessDecision::Refused(WriteRefusal::DescriptorMismatch {
                expected: 2,
                got: 1
            })
        );
    }
}
