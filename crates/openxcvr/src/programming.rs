//! Pure named preflight for the existing CMIS 4.0 vendor-password admission.
//!
//! This is NOT standard permission to write RO identity fields. The caller
//! supplies the reported revision, operation-scoped password and current page
//! bytes. No transport, selector, cache or generic write engine is owned here.
//! Catalog names (including any catalog-declared aliases) resolve exactly;
//! globs are not mutation targets. Encodings are deliberately limited to the
//! existing admitted setters. Text is exact-width printable ASCII, with no
//! implicit padding/truncation or interpretation of date-code strings.
//!
//! All admitted fields occupy whole bytes. Shared-bit fields on this page are
//! outside the existing vendor range and remain refused, not newly authorized.
use crate::catalog::{
    self, AccessKind, CatalogError, FieldEntry, FieldKind, FieldQuery, LaneSelection,
};
use openxcvr_cmis::{
    generated::v4_0::CmisPage00hAdministrative as Page,
    vendor_programming::{
        plan_vendor_programming, VendorProgrammingPlan, VendorProgrammingRefusal,
    },
};
use openxcvr_core::{types::management::Range, Rev};

/// Schema-level values, not a consumer-side hexadecimal/string parser.
#[derive(Debug, Clone, Copy)]
pub enum FieldValue<'a> {
    /// An ASCII/string field's value.
    Text(&'a str),
    /// A byte-blob field's value.
    Bytes(&'a [u8]),
}

/// Why a named vendor-programming proposal was refused during preflight.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProgrammingRefusal {
    /// Only the revision that owns the vendor password function is admitted.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// The current page image is not exactly 128 bytes.
    #[error("current page image must be exactly 128 bytes")]
    InvalidCurrentLength,
    /// The field name is a glob; only exact catalog names are targets.
    #[error("field name is not exact: {field}")]
    NonExactName {
        /// The non-exact (glob) name that was refused.
        field: String,
    },
    /// No catalog field matches the name.
    #[error("unknown field: {field}")]
    UnknownField {
        /// The name that matched nothing.
        field: String,
    },
    /// The name matches more than one catalog field.
    #[error("ambiguous field: {field}")]
    AmbiguousField {
        /// The ambiguous name.
        field: String,
    },
    /// The same field appears more than once in the proposal.
    #[error("duplicate field: {field}")]
    DuplicateField {
        /// The duplicated field name.
        field: String,
    },
    /// The field is read-only and outside the vendor admission.
    #[error("read-only field: {field}")]
    ReadOnlyField {
        /// The read-only field's name.
        field: String,
    },
    /// The field is not admitted by the existing vendor setters.
    #[error("field not admitted for vendor programming: {field}")]
    NotAdmitted {
        /// The unadmitted field's name.
        field: String,
    },
    /// The value length does not equal the field's physical width.
    #[error("value for {field} must be exactly {expected_len} bytes")]
    OutOfRange {
        /// The field whose value had the wrong length.
        field: String,
        /// The field's physical byte width.
        expected_len: u16,
    },
    /// The value does not fit the field's admitted encoding.
    #[error("malformed value for field: {field}")]
    MalformedValue {
        /// The field whose value did not fit the encoding.
        field: String,
    },
    /// The vendor planner refused the proposal; the original refusal is the
    /// source.
    #[error("vendor programming refused")]
    Vendor(#[source] VendorProgrammingRefusal),
}

/// Catalog-derived target metadata; access remains RO even under vendor admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedFieldTarget {
    /// The field's schema-effective name.
    pub name: &'static str,
    /// The field's physical byte range.
    pub range: Range,
    /// The field's catalog access kind (RO even under vendor admission).
    pub access: AccessKind,
}

/// One validated named-programming proposal: its targets, encoded bytes and
/// the underlying vendor plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedProgrammingPlan {
    /// The proposal's catalog-derived targets.
    pub fields: Vec<NamedFieldTarget>,
    /// Fully validated ranges, exactly as passed to the execution-plan authority.
    pub ranges: Vec<(Range, Vec<u8>)>,
    /// Local encoding only; never an instruction to acquire/write a whole page.
    pub encoded_page: [u8; 128],
    /// The underlying vendor-programming plan.
    pub plan: VendorProgrammingPlan,
}

// The generated mutator contract: page-fixed `&mut [u8; 128]`, matching the
// 128-byte upper-page-00h image the plan encodes into.
type Setter = fn(&mut [u8; 128], &[u8]);

// This dispatch chooses existing generated setters, never implements an encoding
// or memory layout. Physical ranges/widths come from the resolved catalog.
fn setter(entry: &FieldEntry<'_>) -> Option<Setter> {
    Some(match entry.name() {
        "vendor_name" => |buf, v| Page::set_vendor_name(buf, v.try_into().unwrap()),
        "vendor_oui" => |buf, v| Page::set_vendor_oui(buf, v.try_into().unwrap()),
        "vendor_pn" => |buf, v| Page::set_vendor_pn(buf, v.try_into().unwrap()),
        "vendor_rev" => |buf, v| Page::set_vendor_rev(buf, v.try_into().unwrap()),
        "vendor_sn" => |buf, v| Page::set_vendor_sn(buf, v.try_into().unwrap()),
        "date_code_year" => |buf, v| Page::set_date_code_year(buf, v.try_into().unwrap()),
        "date_code_month" => |buf, v| Page::set_date_code_month(buf, v.try_into().unwrap()),
        "date_code_day_of_month" => {
            |buf, v| Page::set_date_code_day_of_month(buf, v.try_into().unwrap())
        }
        "date_code_lot_code" => |buf, v| Page::set_date_code_lot_code(buf, v.try_into().unwrap()),
        _ => return None,
    })
}

/// All-or-nothing named preflight. Neither partial ranges nor a partial plan
/// escape on refusal. `current_page` is a supplied upper-page-00h image; it is
/// not freshness evidence and this API never infers a verified no-change.
pub fn plan_named_vendor_programming(
    revision: Rev,
    password: Option<&[u8; 4]>,
    current_page: &[u8],
    proposals: &[(&str, FieldValue<'_>)],
) -> Result<NamedProgrammingPlan, ProgrammingRefusal> {
    use ProgrammingRefusal as Refusal;
    if revision != Rev::CMIS_4_0 {
        return Err(Refusal::UnsupportedRevision(revision));
    }
    let mut encoded_page: [u8; 128] = current_page
        .try_into()
        .map_err(|_| Refusal::InvalidCurrentLength)?;
    let mut fields: Vec<NamedFieldTarget> = Vec::new();
    let mut encodings = Vec::new();
    let mut ranges = Vec::new();
    for (name, value) in proposals {
        let field = || (*name).to_string();
        if name.contains('*') || name.contains('?') {
            return Err(Refusal::NonExactName { field: field() });
        }
        let entries = catalog::resolve(
            revision,
            &FieldQuery {
                pattern: name,
                lane: LaneSelection::Unspecified,
                page: None,
                bank: 0,
            },
        )
        .map_err(|error| match error {
            CatalogError::NoMatch { .. } => Refusal::UnknownField { field: field() },
            CatalogError::UnsupportedRevision { .. } => Refusal::UnsupportedRevision(revision),
            _ => Refusal::AmbiguousField { field: field() },
        })?;
        if entries.len() != 1 {
            return Err(Refusal::AmbiguousField { field: field() });
        }
        let entry = entries[0];
        if fields.iter().any(|prior| prior.name == entry.name()) {
            return Err(Refusal::DuplicateField { field: field() });
        }
        let encode = setter(&entry).ok_or_else(|| {
            if matches!(entry.row.access, AccessKind::Ro | AccessKind::RoCor) {
                Refusal::ReadOnlyField { field: field() }
            } else {
                Refusal::NotAdmitted { field: field() }
            }
        })?;
        let bytes = match (entry.row.kind, value) {
            (FieldKind::Ascii, FieldValue::Text(text))
                if text.bytes().all(|b| (0x20..=0x7e).contains(&b)) =>
            {
                text.as_bytes()
            }
            (FieldKind::Bytes, FieldValue::Bytes(bytes)) => *bytes,
            _ => return Err(Refusal::MalformedValue { field: field() }),
        };
        if bytes.len() != usize::from(entry.row.len) {
            return Err(Refusal::OutOfRange {
                field: field(),
                expected_len: entry.row.len,
            });
        }
        fields.push(NamedFieldTarget {
            name: entry.name(),
            range: entry.range(),
            access: entry.row.access,
        });
        ranges.push((entry.range(), bytes.to_vec()));
        encodings.push((encode, bytes));
    }
    // The existing planner is the sole range/revision/password/overlap and
    // execution-order authority. Validate everything before invoking setters.
    plan_vendor_programming(revision, password, &ranges).map_err(Refusal::Vendor)?;
    for (encode, bytes) in encodings {
        encode(&mut encoded_page, bytes);
    }
    // Extract the encoded targets rather than creating a second raw encoder.
    for (range, bytes) in &mut ranges {
        let start = usize::from(range.offset) - 128;
        bytes.copy_from_slice(&encoded_page[start..start + usize::from(range.len)]);
    }
    let plan = plan_vendor_programming(revision, password, &ranges).map_err(Refusal::Vendor)?;
    Ok(NamedProgrammingPlan {
        fields,
        ranges,
        encoded_page,
        plan,
    })
}

pub use openxcvr_cmis::vendor_programming::{
    ChecksumPolicy, ProgrammingChecksumEvidence, ProgrammingDisposition,
    VendorProgrammingObservation, VendorProgrammingOptions, VendorProgrammingPolicyPlan,
    VendorProgrammingPolicyRefusal, VendorProgrammingScope,
};

impl NamedProgrammingPlan {
    /// Apply checksum policy/fresh comparison through the same vendor authority.
    /// Current encoding inputs alone never qualify as fresh equality evidence.
    /// The returned envelope must be consumed with its scope/finish-read contract.
    pub fn with_options(
        &self,
        password: Option<&[u8; 4]>,
        options: VendorProgrammingOptions,
        scope: VendorProgrammingScope,
        observed: Option<&VendorProgrammingObservation<'_>>,
    ) -> Result<VendorProgrammingPolicyPlan, VendorProgrammingPolicyRefusal> {
        openxcvr_cmis::vendor_programming::plan_vendor_programming_with_options(
            self.plan.revision,
            password,
            &self.ranges,
            options,
            scope,
            observed,
        )
    }
}

use openxcvr_core::types::management::{
    Freshness, InputError, ObservationSource, OperationId, RangeClassified, RangeObservation,
    RangeOutcome, ReadEffect, ReadRequester, ReadStep, Selection,
};

/// An atomically validated proposal, without a supplied current page image.
///
/// Encoding scratch is discarded: it cannot be used as comparison evidence.
/// No password is retained. This and the continuation/proof types deliberately
/// have no public constructors, mutable access, serde, or Clone implementation.
pub struct PreparedNamedProgramming {
    revision: Rev,
    fields: Vec<NamedFieldTarget>,
    ranges: Vec<(Range, Vec<u8>)>,
    options: VendorProgrammingOptions,
}

/// Ordered *conditional* reads for the same exclusive programming operation.
///
/// The owner must correlate each completion, call `validate_prefix`, and check
/// its own fence/deadline/cancellation before issuing the next read. Merely
/// inspecting `reads` never authorizes upper memory: the selector receipt must
/// first confirm the required selection. No selector mutation is delivered.
pub struct ProgrammingComparisonPrelude {
    proposal: PreparedNamedProgramming,
    scope: VendorProgrammingScope,
    reads: [ReadStep; 3],
}

/// A sealed canonical tail. The owner must consume all policy finish reads and
/// recheck the live scope before every transfer and evidence publication.
pub struct NamedProgrammingExecution {
    proposal: PreparedNamedProgramming,
    policy: VendorProgrammingPolicyPlan,
}

/// Fresh equality with zero changing transfers, not a claim of durable success.
/// Checksum validity is independent of requested-data equality.
///
/// Only continuation can construct this proof; a public legacy policy plan,
/// disposition, encoding image or receipt cannot be converted into one.
///
pub struct VerifiedNamedNoChange {
    proposal: PreparedNamedProgramming,
    checksum: ProgrammingChecksumEvidence,
    scope: VendorProgrammingScope,
}

/// Trusted owner-supplied completion facts, NOT client JSON or cryptographic
/// proof. The owner must derive `confirmed` from a correlated single-use
/// completion in this operation; stamping supplied/scratch bytes is forbidden.
pub struct ProgrammingReadReceipt {
    /// The owning operation's ID.
    pub operation: OperationId,
    /// The ordered read step the receipt completes.
    pub step: usize,
    /// Whether the owner correlated and confirmed this completion.
    pub confirmed: bool,
    /// The read's observation.
    pub observation: RangeObservation,
}

/// What a validated named-programming proposal continues into.
pub enum NamedProgrammingContinuation {
    /// Fresh comparison established all requested bytes already stored.
    VerifiedNoChange(VerifiedNamedNoChange),
    /// Execute the plan under the policy envelope.
    Execute(NamedProgrammingExecution),
}

/// Why a named programming continuation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProgrammingContinuationRefusal {
    /// The proposal itself was refused; the original refusal is the source.
    #[error("proposal refused")]
    Proposal(#[source] ProgrammingRefusal),
    /// The policy authority refused the request; the original refusal is the
    /// source.
    #[error("policy refused")]
    Policy(#[source] VendorProgrammingPolicyRefusal),
    /// A supplied input is invalid; the original error is the source.
    #[error("invalid input")]
    Input(#[source] InputError),
    /// A comparison read is not classified as a qualified passive read.
    #[error("read footprint not qualified as passive: {range:?}")]
    ReadFootprintNotQualified {
        /// The range whose footprint is not qualified as passive.
        range: Range,
    },
    /// A completion does not correspond to the expected ordered read.
    #[error("unexpected read completion at step {step}")]
    UnexpectedRead {
        /// The step whose completion arrived unexpectedly.
        step: usize,
    },
    /// A completion was not confirmed by the operation owner.
    #[error("unconfirmed read completion at step {step}")]
    UnconfirmedRead {
        /// The step whose completion was not confirmed.
        step: usize,
    },
    /// A completion was not exactly the requested length; exact length is
    /// mandatory and overlong completions are also refused.
    #[error("read at {range:?} returned {received} bytes, expected {expected}")]
    ShortRead {
        /// The range the read addressed.
        range: Range,
        /// The requested byte count.
        expected: u16,
        /// The byte count actually received.
        received: usize,
    },
    /// A completion's identity, freshness or ordering does not fit this
    /// operation.
    #[error("read context mismatch")]
    ContextMismatch,
    /// The module's revision byte does not match the proposal's revision.
    #[error("module reports revision byte 0x{reported:02x}, expected {expected:?}")]
    RevisionMismatch {
        /// The revision the proposal was validated for.
        expected: Rev,
        /// The raw revision byte the module reports.
        reported: u8,
    },
    /// The selector registers do not already hold the required selection.
    #[error("selector change required: actual {actual:?}, required {required:?}")]
    SelectorChangeRequired {
        /// The selection the selector actually holds.
        actual: Selection,
        /// The selection the comparison requires.
        required: Selection,
    },
    /// The supplied scope is not the live exclusive scope.
    #[error("exclusive scope lost")]
    ScopeLost,
}

/// Nonsecret intent description only, never execution or equality proof.
/// The daemon owns serialization, persistence, correlation and terminal outcome.
/// Ranges contain only admitted identity target bytes, never password bytes.
pub struct NamedProgrammingDescription<'a> {
    /// The revision the proposal was validated for.
    pub revision: Rev,
    /// The proposal's targets.
    pub fields: &'a [NamedFieldTarget],
    /// The proposal's validated ranges and bytes.
    pub ranges: &'a [(Range, Vec<u8>)],
    /// The options the proposal runs under.
    pub options: VendorProgrammingOptions,
}

/// Proof-backed process facts for the daemon's named-only evidence reducer.
/// This is not a durable audit record or permission to report audited success.
pub struct NamedProgrammingNoChangeFacts<'a> {
    proof: &'a VerifiedNamedNoChange,
}

/// Prepare every target without a page image or I/O. The owner supplies its
/// live detected revision for this generation; the prelude independently
/// confirms the module revision byte. Existing whole-byte
/// generated setters and the existing planner remain the sole encoding and
/// range/password/overlap authorities. The scratch image and raw plan never
/// escape this function and are never treated as an observation.
pub fn prepare_named_vendor_programming(
    revision: Rev,
    password: Option<&[u8; 4]>,
    proposals: &[(&str, FieldValue<'_>)],
    options: VendorProgrammingOptions,
) -> Result<PreparedNamedProgramming, ProgrammingContinuationRefusal> {
    let named = plan_named_vendor_programming(revision, password, &[0; 128], proposals)
        .map_err(ProgrammingContinuationRefusal::Proposal)?;
    Ok(PreparedNamedProgramming {
        revision,
        fields: named.fields,
        ranges: named.ranges,
        options,
    })
}

fn programming_range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

fn validate_programming_scope(
    captured: VendorProgrammingScope,
    current: VendorProgrammingScope,
) -> Result<(), ProgrammingContinuationRefusal> {
    if !captured.exclusive
        || captured.now < captured.started_at
        || !current.exclusive
        || current.operation != captured.operation
        || current.generation != captured.generation
        || current.epoch != captured.epoch
        || current.started_at != captured.started_at
        || current.now < captured.now
    {
        return Err(ProgrammingContinuationRefusal::ScopeLost);
    }
    Ok(())
}

impl PreparedNamedProgramming {
    /// The validated proposal's targets.
    pub fn fields(&self) -> &[NamedFieldTarget] {
        &self.fields
    }

    /// The nonsecret intent description for audit surfaces.
    pub fn description(&self) -> NamedProgrammingDescription<'_> {
        NamedProgrammingDescription {
            revision: self.revision,
            fields: &self.fields,
            ranges: &self.ranges,
            options: self.options,
        }
    }

    /// Begin the fresh-comparison prelude under the supplied live scope.
    pub fn comparison_prelude(
        self,
        scope: VendorProgrammingScope,
    ) -> Result<ProgrammingComparisonPrelude, ProgrammingContinuationRefusal> {
        use openxcvr_cmis::{generated::v4_0::CmisLowerMemory, read_planning::CmisReadPlanner};
        use ProgrammingContinuationRefusal as Refusal;
        validate_programming_scope(scope, scope)?;
        let planner = CmisReadPlanner::for_revision(self.revision).map_err(Refusal::Input)?;
        let lower = CmisLowerMemory::read_classifications();
        let upper = Page::read_classifications();
        let mut reads = Vec::with_capacity(3);
        for range in [
            programming_range(1, 1),
            programming_range(126, 2),
            programming_range(128, 95),
        ] {
            if !matches!(
                planner
                    .classify_range(self.revision, range)
                    .map_err(Refusal::Input)?,
                RangeClassified::Passive { .. }
            ) {
                return Err(Refusal::ReadFootprintNotQualified { range });
            }
            let classifications: Vec<_> = (u32::from(range.offset)..range.end())
                .map(|offset| {
                    if offset < 128 {
                        lower[offset as usize].clone()
                    } else {
                        upper[(offset - 128) as usize].clone()
                    }
                })
                .collect();
            // These are exactly the revision's generated rows, not the raw
            // vendor helper's conservative Unknown rows relabeled as None.
            if !classifications.iter().all(|byte| byte.is_qualified_none()) {
                return Err(Refusal::ReadFootprintNotQualified { range });
            }
            reads.push(ReadStep {
                physical_range: range,
                requester: ReadRequester::ExplicitClient,
                classifications,
                declared_effects: vec![ReadEffect::None],
            });
        }
        Ok(ProgrammingComparisonPrelude {
            proposal: self,
            scope,
            reads: reads.try_into().expect("exactly three fixed ranges"),
        })
    }
}

impl ProgrammingComparisonPrelude {
    /// The revision the prelude's proposal was validated for.
    pub fn revision(&self) -> Rev {
        self.proposal.revision
    }

    /// The prelude's three fixed ordered reads.
    pub fn reads(&self) -> &[ReadStep; 3] {
        &self.reads
    }

    /// The selection the comparison requires to already hold (page 00h).
    pub fn required_selection(&self) -> Selection {
        Selection {
            address: 0x50,
            bank: 0,
            page: 0,
        }
    }

    /// Validate the entire ordered prefix before a dependent read is issued.
    /// ObservationStamp alone does not prove confirmation or operation identity.
    /// The current scope is a live owner fact, never a new exclusion interval.
    pub fn validate_prefix(
        &self,
        receipts: &[ProgrammingReadReceipt],
        current: VendorProgrammingScope,
    ) -> Result<(), ProgrammingContinuationRefusal> {
        use ProgrammingContinuationRefusal as Refusal;
        validate_programming_scope(self.scope, current)?;
        let mut previous_completion = self.scope.started_at;
        for (index, receipt) in receipts.iter().enumerate() {
            let expected = self
                .reads
                .get(index)
                .ok_or(Refusal::UnexpectedRead { step: index })?;
            let observation = &receipt.observation;
            if receipt.step != index || observation.range != expected.physical_range {
                return Err(Refusal::UnexpectedRead { step: index });
            }
            if !receipt.confirmed {
                return Err(Refusal::UnconfirmedRead { step: index });
            }
            let bytes = match &observation.outcome {
                RangeOutcome::Bytes(bytes) => bytes,
                _ => return Err(Refusal::UnconfirmedRead { step: index }),
            };
            if bytes.len() != usize::from(expected.physical_range.len) {
                return Err(Refusal::ShortRead {
                    range: expected.physical_range,
                    expected: expected.physical_range.len,
                    received: bytes.len(),
                });
            }
            let stamp = &observation.stamp;
            stamp
                .validate(current.generation, current.epoch)
                .map_err(Refusal::Input)?;
            if receipt.operation != current.operation
                || observation.requester != expected.requester
                || stamp.freshness != Freshness::Fresh
                || stamp.source != ObservationSource::DaemonObserved
                || !stamp.consumed_effects.is_empty()
                || stamp.started_at < previous_completion
                || stamp.completed_at > current.now
            {
                return Err(Refusal::ContextMismatch);
            }
            previous_completion = stamp.completed_at;
            if index == 0 && Rev::from_major_byte_cmis(bytes[0]) != Some(self.proposal.revision) {
                return Err(Refusal::RevisionMismatch {
                    expected: self.proposal.revision,
                    reported: bytes[0],
                });
            }
            if index == 1 && bytes != &[0, 0] {
                return Err(Refusal::SelectorChangeRequired {
                    actual: Selection {
                        address: 0x50,
                        bank: bytes[0],
                        page: bytes[1],
                    },
                    required: self.required_selection(),
                });
            }
        }
        Ok(())
    }

    /// Consume the proposal exactly once. No mutation plan or equality proof
    /// escapes before all three exact, confirmed, current receipts are checked.
    pub fn continue_programming(
        self,
        password: Option<&[u8; 4]>,
        receipts: &[ProgrammingReadReceipt],
        current: VendorProgrammingScope,
    ) -> Result<NamedProgrammingContinuation, ProgrammingContinuationRefusal> {
        use ProgrammingContinuationRefusal as Refusal;
        self.validate_prefix(receipts, current)?;
        if receipts.len() != self.reads.len() {
            return Err(Refusal::UnexpectedRead {
                step: receipts.len(),
            });
        }
        let comparison = &receipts[2].observation;
        let observed = VendorProgrammingObservation {
            operation: receipts[2].operation,
            stamp: &comparison.stamp,
            selectors: [0, 0], // validated ordered selector receipt above
            page: comparison.bytes().expect("validated exact confirmed bytes"),
        };
        let policy = openxcvr_cmis::vendor_programming::plan_vendor_programming_with_options(
            self.proposal.revision,
            password,
            &self.proposal.ranges,
            self.proposal.options,
            current,
            Some(&observed),
        )
        .map_err(Refusal::Policy)?;
        match policy.disposition {
            ProgrammingDisposition::VerifiedNoChange => {
                // Preserve the policy authority's zero-changing-transfer
                // contract; never seal an envelope containing residual work.
                if !policy.plan.steps.is_empty()
                    || !policy.plan.invalidations.is_empty()
                    || !policy.plan.unconfirmed_step_retries.is_empty()
                    || !policy.finish_reads.is_empty()
                    || !policy.finish_read_retries.is_empty()
                {
                    return Err(Refusal::ContextMismatch);
                }
                let checksum = policy.initial_checksum.ok_or(Refusal::ContextMismatch)?;
                Ok(NamedProgrammingContinuation::VerifiedNoChange(
                    VerifiedNamedNoChange {
                        proposal: self.proposal,
                        checksum,
                        scope: current,
                    },
                ))
            }
            ProgrammingDisposition::RequiresExecution => Ok(NamedProgrammingContinuation::Execute(
                NamedProgrammingExecution {
                    proposal: self.proposal,
                    policy,
                },
            )),
        }
    }
}

impl NamedProgrammingExecution {
    /// The executed proposal's targets.
    pub fn fields(&self) -> &[NamedFieldTarget] {
        self.proposal.fields()
    }

    /// The policy-bearing plan to hand to the vendor executor.
    pub fn policy_plan(&self) -> &VendorProgrammingPolicyPlan {
        &self.policy
    }

    /// The nonsecret intent description for audit surfaces.
    pub fn description(&self) -> NamedProgrammingDescription<'_> {
        self.proposal.description()
    }
}

impl VerifiedNamedNoChange {
    /// The verified proposal's targets.
    pub fn fields(&self) -> &[NamedFieldTarget] {
        self.proposal.fields()
    }

    /// The fresh checksum observation the proof carries.
    pub fn checksum(&self) -> ProgrammingChecksumEvidence {
        self.checksum
    }

    /// The options the verified proposal ran under.
    pub fn options(&self) -> VendorProgrammingOptions {
        self.proposal.options
    }

    /// The nonsecret intent description for audit surfaces.
    pub fn description(&self) -> NamedProgrammingDescription<'_> {
        self.proposal.description()
    }

    /// Validate live owner facts again before sealing/publishing evidence. The
    /// proof keeps its admission epoch; no re-stamping or epoch successor here.
    pub fn validate_scope(
        &self,
        current: VendorProgrammingScope,
    ) -> Result<(), ProgrammingContinuationRefusal> {
        validate_programming_scope(self.scope, current)
    }

    /// Produce the proof-backed process facts, revalidating the live scope.
    pub fn outcome_facts(
        &self,
        current: VendorProgrammingScope,
    ) -> Result<NamedProgrammingNoChangeFacts<'_>, ProgrammingContinuationRefusal> {
        self.validate_scope(current)?;
        Ok(NamedProgrammingNoChangeFacts { proof: self })
    }
}

impl NamedProgrammingNoChangeFacts<'_> {
    /// The evidence label: stored bytes were verified equal.
    pub fn evidence(&self) -> &'static str {
        "stored_bytes_verified"
    }
    /// The write-target label: nothing was submitted.
    pub fn target_write(&self) -> &'static str {
        "not_submitted"
    }
    /// The readback label: the fresh read matched.
    pub fn target_readback(&self) -> &'static str {
        "matched"
    }
    /// Always true: a verified no-change made no modification.
    pub fn no_change(&self) -> bool {
        true
    }
    /// Always false: no write was submitted, so none could apply.
    pub fn possible_application(&self) -> bool {
        false
    }
    /// The pre-write checksum observation (equal to the final one).
    pub fn initial_checksum(&self) -> ProgrammingChecksumEvidence {
        self.proof.checksum
    }
    /// The final checksum observation (equal to the initial one).
    pub fn final_checksum(&self) -> ProgrammingChecksumEvidence {
        self.proof.checksum
    }
}

// Standalone entry is deliberately not an empty named-programming proposal.
pub use openxcvr_cmis::password_entry::{
    plan_password_entry, plan_password_entry_for_revision, PasswordEntryEvidence,
    PasswordEntryFact, PasswordEntryOperation, PasswordEntryOutcome, PasswordEntryPlan,
    PasswordEntryRefusal, PasswordEntrySecret,
};
