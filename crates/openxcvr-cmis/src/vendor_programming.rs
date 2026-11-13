//! Pure CMIS 4.0 vendor-password programming plans, not standard RO permission.
//!
//! Plans contain no password bytes. The executor supplies the original
//! operation-scoped password only at `PasswordEntry`, confirms that transfer
//! without readback, then discards it. Nothing here owns transport or unlock state.
use crate::generated::v4_0::{CmisLowerMemory, CmisPage00hAdministrative};
use openxcvr_core::{
    types::management::{
        operations::RetryDecision, ActuationOutcome, Capabilities, Invalidation, Range,
        ReadRequester, ReadStep,
    },
    Rev,
};

/// Admission, not a claim that these fields are standard-writable.
pub const ADMISSION: &str = "CMIS 4.0 identity fields are RO in Table 8-15; written only under the vendor password function §8.2.10. This is vendor programming, never a standard write.";

/// Bytes are either supplied identity data or the result of the prior Checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgrammingValue {
    /// Caller-supplied identity bytes to write verbatim.
    Supplied(Vec<u8>),
    /// The value is the result of the prior [`VendorProgrammingStep::Checksum`]
    /// computation.
    Checksum,
}

/// Ordered instructions for one exclusive, generation-fenced daemon operation.
/// Every read bypasses cache. No selector register is ever written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VendorProgrammingStep {
    /// Refuse before password entry unless the fresh result equals `expected`.
    ConfirmPage00 {
        /// The exact page-00h selector read to execute.
        read: ReadStep,
        /// The selector bytes the fresh read must equal.
        expected: [u8; 2],
    },
    /// Write the original password; transport-confirmed ONLY, never read back.
    PasswordEntry {
        /// The password-entry byte range (page 9Fh 128..=131).
        range: Range,
    },
    /// Write supplied bytes or the computed checksum.
    Write {
        /// The range the write addresses.
        range: Range,
        /// The value to write.
        value: ProgrammingValue,
    },
    /// A fresh uncached read of the checksum region.
    FreshRead(ReadStep),
    /// Compute from FreshRead using `VendorProgrammingPlan::checksum`.
    Checksum {
        /// The freshly read region the checksum covers.
        covers: Range,
        /// The range of the stored check byte.
        target: Range,
    },
    /// Compare with supplied identity data or the computed checksum, not cache.
    VerifyRead {
        /// The fresh read to execute.
        read: ReadStep,
        /// The value the fresh read must match.
        expected: ProgrammingValue,
    },
}

/// One validated vendor-programming plan: ordered steps, end invalidations
/// and per-step uncertainty decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorProgrammingPlan {
    /// The revision the plan was validated for.
    pub revision: Rev,
    /// The admission string the plan carries (see [`ADMISSION`]).
    pub admission: &'static str,
    /// The ordered steps to execute.
    pub steps: Vec<VendorProgrammingStep>,
    /// Apply at operation end, including failure/uncertainty; no selector effect.
    pub invalidations: Vec<Invalidation>,
    /// One cadence-row decision per step, in step order, if unconfirmed.
    /// Every decision is terminal Uncertain: stop, never retry or continue.
    pub unconfirmed_step_retries: Vec<RetryDecision>,
}

/// Why a vendor-programming request was refused before any plan was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VendorProgrammingRefusal {
    /// The write class is not the one admitted vendor class.
    #[error("write class not admitted for vendor programming")]
    UnsupportedWriteClass,
    /// Only the revision that owns the vendor password function is admitted.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// The vendor password was not supplied.
    #[error("vendor password required")]
    MissingPassword,
    /// No write ranges were supplied, or a supplied range is empty.
    #[error("no write ranges supplied")]
    EmptyRanges,
    /// A range falls outside the admitted identity-field window.
    #[error("range not admitted for vendor programming: {0:?}")]
    FieldNotAdmitted(Range),
    /// The value length does not match the physical range length.
    #[error("value length does not match range {0:?}")]
    InvalidLength(Range),
    /// Two supplied ranges cover the same byte.
    #[error("write ranges overlap")]
    OverlappingRanges,
    /// The checksum input is not the exact freshly read region.
    #[error("checksum input must be exactly the 94 freshly read bytes")]
    InvalidChecksumInput,
}

/// The validation row's one admitted class. Other write classes must be mapped
/// to `Unsupported`, never coerced into vendor programming. Deliberately no
/// Debug/Clone: the original password remains operation-scoped request data.
pub enum WriteRequest {
    /// A vendor-programming request: the operation-scoped password and the
    /// (range, bytes) writes to apply.
    VendorProgramming {
        /// The original vendor password bytes.
        password: [u8; 4],
        /// The ranges and byte values to write.
        ranges: Vec<(Range, Vec<u8>)>,
    },
    /// Any other write class: refused, never coerced into vendor programming.
    Unsupported,
}

/// Deliver the daemon receipt §4 `validate_write` row for vendor programming
/// only. Capabilities contain no vendor-unlock permission or relevant page-00h
/// restriction: none is inferred from them. Admission is still revision/range
/// validation, not peer authorization or hardware qualification.
pub fn validate_write(
    request: &WriteRequest,
    revision: Rev,
    _capabilities: &Capabilities,
) -> Result<VendorProgrammingPlan, VendorProgrammingRefusal> {
    match request {
        WriteRequest::VendorProgramming { password, ranges } => {
            plan_vendor_programming(revision, Some(password), ranges)
        }
        WriteRequest::Unsupported => Err(VendorProgrammingRefusal::UnsupportedWriteClass),
    }
}

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

// Shared embedded/standalone CMIS 4.0 entry contract. Keep the footprint and
// generated encoding here so the standalone path cannot drift into change-entry.
pub(crate) fn password_entry_range() -> Range {
    range(122, 4)
}

pub(crate) fn encode_password_entry(buffer: &mut [u8; 128], value: u32) {
    CmisLowerMemory::set_password_entry_area(buffer, value);
}

pub(crate) fn password_entry_invalidations() -> [Invalidation; 2] {
    [
        Invalidation::Covered(password_entry_range()),
        Invalidation::Password,
    ]
}

fn read(r: Range) -> ReadStep {
    let lower = CmisLowerMemory::read_classifications();
    let upper = CmisPage00hAdministrative::read_classifications();
    let classifications = (u32::from(r.offset)..r.end())
        .map(|offset| {
            if offset < 128 {
                lower[offset as usize].clone()
            } else {
                upper[(offset - 128) as usize].clone()
            }
        })
        .collect();
    ReadStep {
        physical_range: r,
        requester: ReadRequester::ExplicitClient,
        classifications,
        // Exact internal checksum/verify reads are not a passive observation
        // grant. Preserve generated Unknown metadata rather than claiming None.
        declared_effects: vec![],
    }
}

impl VendorProgrammingPlan {
    /// Compute byte 222 from exactly the 94 freshly read bytes 128..=221.
    /// Padding is local only: it is never a widened physical read/write.
    /// The sum8 encoding remains owned by the generated schema helper.
    pub fn checksum(&self, fresh: &[u8]) -> Result<u8, VendorProgrammingRefusal> {
        if fresh.len() != 94 {
            return Err(VendorProgrammingRefusal::InvalidChecksumInput);
        }
        let mut page = [0; 128];
        page[..94].copy_from_slice(fresh);
        CmisPage00hAdministrative::recompute_checksums(&mut page);
        Ok(page[94])
    }
}

/// Validate the entire request before returning any instruction. The password
/// is presence-checked, never copied into a plan, refusal, or debug record.
/// Range is the shared management range (seven-bit I2C address).
/// No generated descriptor imposes identity field-boundary splitting: writes
/// preserve each supplied range exactly. Overlaps are refused so verify cannot
/// demand contradictory values for the same byte.
pub fn plan_vendor_programming(
    revision: Rev,
    password: Option<&[u8; 4]>,
    ranges: &[(Range, Vec<u8>)],
) -> Result<VendorProgrammingPlan, VendorProgrammingRefusal> {
    use VendorProgrammingRefusal as Refusal;
    use VendorProgrammingStep as Step;
    if revision != Rev::CMIS_4_0 {
        return Err(Refusal::UnsupportedRevision(revision));
    }
    if password.is_none() {
        return Err(Refusal::MissingPassword);
    }
    if ranges.is_empty() {
        return Err(Refusal::EmptyRanges);
    }
    for (index, (r, bytes)) in ranges.iter().enumerate() {
        if r.len == 0 {
            return Err(Refusal::EmptyRanges);
        }
        if r.address != 0x50 || r.bank != 0 || r.page != 0 || r.offset < 129 || r.end() > 190 {
            return Err(Refusal::FieldNotAdmitted(*r));
        }
        if bytes.len() != usize::from(r.len) {
            return Err(Refusal::InvalidLength(*r));
        }
        if ranges[..index].iter().any(|(prior, _)| {
            u32::from(prior.offset) < r.end() && u32::from(r.offset) < prior.end()
        }) {
            return Err(Refusal::OverlappingRanges);
        }
    }
    let mut steps = vec![
        Step::ConfirmPage00 {
            read: read(range(126, 2)),
            expected: [0, 0],
        },
        Step::PasswordEntry {
            range: password_entry_range(),
        },
    ];
    for (r, bytes) in ranges {
        steps.push(Step::Write {
            range: *r,
            value: ProgrammingValue::Supplied(bytes.clone()),
        });
    }
    steps.extend([
        Step::FreshRead(read(range(128, 94))),
        Step::Checksum {
            covers: range(128, 94),
            target: range(222, 1),
        },
        Step::Write {
            range: range(222, 1),
            value: ProgrammingValue::Checksum,
        },
    ]);
    for (r, bytes) in ranges {
        steps.push(Step::VerifyRead {
            read: read(*r),
            expected: ProgrammingValue::Supplied(bytes.clone()),
        });
    }
    steps.push(Step::VerifyRead {
        read: read(range(222, 1)),
        expected: ProgrammingValue::Checksum,
    });
    let [entry_covered, entry_password] = password_entry_invalidations();
    let mut invalidations = vec![entry_covered];
    invalidations.extend(ranges.iter().map(|(r, _)| Invalidation::Covered(*r)));
    invalidations.extend([Invalidation::Covered(range(222, 1)), entry_password]);
    let unconfirmed_step_retries = steps
        .iter()
        .map(|_| {
            RetryDecision::Terminal(ActuationOutcome::Uncertain {
                reason: "step-unconfirmed".into(),
            })
        })
        .collect();
    Ok(VendorProgrammingPlan {
        revision,
        admission: ADMISSION,
        steps,
        invalidations,
        unconfirmed_step_retries,
    })
}

/// Named-programming checksum policy. Preserve is an explicit opt-out of
/// repair, never an opt-out of data verification or checksum observation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChecksumPolicy {
    /// Recompute and write the check byte after the writes.
    #[default]
    Maintain,
    /// Leave the stored check byte untouched (explicit repair opt-out; the
    /// checksum is still observed and reported).
    Preserve,
}

/// Caller options for a policy-bearing vendor-programming request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VendorProgrammingOptions {
    /// Whether to maintain or preserve the stored check byte.
    pub checksum_policy: ChecksumPolicy,
}

/// Supplied facts from the existing exclusive operation owner, not a new
/// lock/token/selector engine. `started_at` identifies the current uninterrupted
/// exclusion interval; `now` shares the observation's monotonic clock origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorProgrammingScope {
    /// The owning exclusive operation's ID.
    pub operation: openxcvr_core::types::management::OperationId,
    /// The module generation the exclusion holds under.
    pub generation: openxcvr_core::types::management::ModuleGeneration,
    /// The cache epoch the exclusion holds under.
    pub epoch: openxcvr_core::types::management::CacheEpoch,
    /// The exclusion is still live.
    pub exclusive: bool,
    /// The current uninterrupted exclusion interval's start.
    pub started_at: openxcvr_core::types::management::MonoTime,
    /// The current time on the observation's clock origin.
    pub now: openxcvr_core::types::management::MonoTime,
}

/// Fresh exact checksum region plus stored check byte (physical 128..=222).
/// These bytes and the selector confirmation must have been acquired by the
/// named operation under its same uninterrupted exclusion interval. This is a
/// caller-supplied fact, not evidence manufactured from an offline/cache image.
#[derive(Debug, Clone, Copy)]
pub struct VendorProgrammingObservation<'a> {
    /// The named operation that acquired the bytes.
    pub operation: openxcvr_core::types::management::OperationId,
    /// The acquisition's provenance stamp.
    pub stamp: &'a openxcvr_core::types::management::ObservationStamp,
    /// The confirmed page-00h selector bytes.
    pub selectors: [u8; 2],
    /// The freshly read page bytes (physical 128..=222 window).
    pub page: &'a [u8],
}

/// One checksum observation: stored check byte versus freshly computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgrammingChecksumEvidence {
    /// The check byte the module stores.
    pub stored: u8,
    /// The value computed from the freshly read region.
    pub computed: u8,
    /// Whether stored and computed agree.
    pub valid: bool,
}

/// What a policy plan says about the requested change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgrammingDisposition {
    /// Nothing has been applied/verified by merely producing this plan.
    RequiresExecution,
    /// Fresh supplied equality established all requested stored bytes. Checksum
    /// validity remains independent: Preserve may report an invalid checksum.
    VerifiedNoChange,
}

/// Why a policy-bearing vendor-programming request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VendorProgrammingPolicyRefusal {
    /// The underlying plan was refused; the original refusal is the source.
    #[error("vendor programming refused")]
    Vendor(#[source] VendorProgrammingRefusal),
    /// The supplied exclusive scope is not live, or was lost.
    #[error("exclusive scope lost")]
    ScopeLost,
    /// The supplied comparison observation is not qualified evidence.
    #[error("comparison observation is invalid")]
    InvalidObservation,
}

/// Additive policy-bearing envelope. Existing raw plans/signatures are unchanged.
///
/// Executors MUST fence every step/publication with `validate_scope`, stop on
/// any unconfirmed step without retry, execute `plan` followed by `finish_reads`,
/// and report `checksum_evidence` from that final fresh read. The latter is
/// mandatory even with Preserve: a bad check byte is evidence, not suppressed
/// or compared as if repair had occurred. No password is retained here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorProgrammingPolicyPlan {
    /// The underlying raw plan.
    pub plan: VendorProgrammingPlan,
    /// The checksum policy this plan runs under.
    pub checksum_policy: ChecksumPolicy,
    /// What the plan says about the requested change.
    pub disposition: ProgrammingDisposition,
    /// Pre-write evidence only; never presented as final validity after writes.
    pub initial_checksum: Option<ProgrammingChecksumEvidence>,
    /// The mandatory final fresh reads the executor must perform.
    pub finish_reads: Vec<ReadStep>,
    /// One terminal decision per finish read, in order (stop, never retry).
    pub finish_read_retries: Vec<RetryDecision>,
    /// The exclusive scope the plan was validated against.
    pub scope: VendorProgrammingScope,
}

impl VendorProgrammingPolicyPlan {
    /// Recheck supplied live owner facts before each transfer and publication.
    /// Exclusion loss is terminal; reacquiring with a new interval cannot revive
    /// this plan, even in the same generation/operation. No replay is authorized.
    pub fn validate_scope(
        &self,
        current: VendorProgrammingScope,
    ) -> Result<(), VendorProgrammingPolicyRefusal> {
        if !current.exclusive
            || current.operation != self.scope.operation
            || current.generation != self.scope.generation
            || current.epoch != self.scope.epoch
            || current.started_at != self.scope.started_at
            || current.now < self.scope.now
        {
            return Err(VendorProgrammingPolicyRefusal::ScopeLost);
        }
        Ok(())
    }

    /// Evaluate the exact final 95-byte read using the existing generated sum8
    /// helper. This reports checksum validity separately from data readback.
    /// It alone is not freshness/scope proof; the executor must fence publication.
    pub fn checksum_evidence(
        &self,
        observed: &[u8],
    ) -> Result<ProgrammingChecksumEvidence, VendorProgrammingPolicyRefusal> {
        checksum_evidence(&self.plan, observed)
    }
}

fn checksum_evidence(
    plan: &VendorProgrammingPlan,
    observed: &[u8],
) -> Result<ProgrammingChecksumEvidence, VendorProgrammingPolicyRefusal> {
    if observed.len() != 95 {
        return Err(VendorProgrammingPolicyRefusal::InvalidObservation);
    }
    let computed = plan
        .checksum(&observed[..94])
        .map_err(VendorProgrammingPolicyRefusal::Vendor)?;
    Ok(ProgrammingChecksumEvidence {
        stored: observed[94],
        computed,
        valid: observed[94] == computed,
    })
}

/// Extend the existing vendor plan, never rederive its write admission/ordering.
/// Missing comparison bytes produce a normal plan, not an inferred no-change.
/// Supplied but stale/short/unscoped bytes refuse the whole request.
///
/// Default repair also repairs an invalid checksum when all payloads are equal;
/// only Preserve may produce no-change with explicitly invalid checksum evidence.
/// No generic write/selector/cache/scheduling contract is delivered by this API.
pub fn plan_vendor_programming_with_options(
    revision: Rev,
    password: Option<&[u8; 4]>,
    ranges: &[(Range, Vec<u8>)],
    options: VendorProgrammingOptions,
    scope: VendorProgrammingScope,
    current: Option<&VendorProgrammingObservation<'_>>,
) -> Result<VendorProgrammingPolicyPlan, VendorProgrammingPolicyRefusal> {
    use openxcvr_core::types::management::{Freshness, ObservationSource};
    use VendorProgrammingPolicyRefusal as Refusal;
    use VendorProgrammingStep as Step;
    let mut plan = plan_vendor_programming(revision, password, ranges).map_err(Refusal::Vendor)?;
    if !scope.exclusive || scope.now < scope.started_at {
        return Err(Refusal::ScopeLost);
    }
    let mut initial_checksum = None;
    let mut no_change = false;
    if let Some(current) = current {
        if current.operation != scope.operation
            || current.selectors != [0, 0]
            || current.stamp.freshness != Freshness::Fresh
            || current.stamp.source != ObservationSource::DaemonObserved
            || current
                .stamp
                .validate(scope.generation, scope.epoch)
                .is_err()
            || current.stamp.started_at < scope.started_at
            || current.stamp.completed_at > scope.now
        {
            return Err(Refusal::InvalidObservation);
        }
        let checksum = checksum_evidence(&plan, current.page)?;
        let equal = ranges.iter().all(|(r, bytes)| {
            let start = usize::from(r.offset) - 128;
            current.page[start..start + bytes.len()] == *bytes
        });
        no_change =
            equal && (checksum.valid || options.checksum_policy == ChecksumPolicy::Preserve);
        initial_checksum = Some(checksum);
    }
    if no_change {
        plan.steps.clear();
        plan.invalidations.clear();
        plan.unconfirmed_step_retries.clear();
    } else if options.checksum_policy == ChecksumPolicy::Preserve {
        // Preserve the legacy plan's data writes/verification and selector/password
        // order. Its repair-only checksum instructions must not assert equality
        // to a checksum we deliberately did not store. Final validity is read below.
        let (steps, retries) = plan
            .steps
            .into_iter()
            .zip(plan.unconfirmed_step_retries)
            .filter(|(step, _)| {
                !matches!(
                    step,
                    Step::FreshRead(_)
                        | Step::Checksum { .. }
                        | Step::Write {
                            value: ProgrammingValue::Checksum,
                            ..
                        }
                        | Step::VerifyRead {
                            expected: ProgrammingValue::Checksum,
                            ..
                        }
                )
            })
            .unzip();
        plan.steps = steps;
        plan.unconfirmed_step_retries = retries;
        plan.invalidations
            .retain(|invalidation| *invalidation != Invalidation::Covered(range(222, 1)));
    }
    let finish_reads = if no_change {
        vec![]
    } else {
        vec![read(range(128, 95))]
    };
    let finish_read_retries = finish_reads
        .iter()
        .map(|_| {
            RetryDecision::Terminal(ActuationOutcome::Uncertain {
                reason: "step-unconfirmed".into(),
            })
        })
        .collect();
    Ok(VendorProgrammingPolicyPlan {
        plan,
        checksum_policy: options.checksum_policy,
        disposition: if no_change {
            ProgrammingDisposition::VerifiedNoChange
        } else {
            ProgrammingDisposition::RequiresExecution
        },
        initial_checksum,
        finish_reads,
        finish_read_retries,
        scope,
    })
}
