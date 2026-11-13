//! Portable supplied-input management contracts. No type here owns a device or clock.
//!
//! Addresses are seven-bit I2C addresses. Times are nanoseconds from a single
//! caller-chosen monotonic origin; mixing origins is a caller error.
use crate::rev::{Family, Rev};
use crate::types::VendorInfo;

pub mod cache;
pub mod lifecycle;
pub mod operations;
pub mod scheduling;
pub mod thermal;

/// A contiguous byte span inside one module memory window: one seven-bit
/// I2C address, bank and page selector, starting `offset` with length
/// `len`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    /// Seven-bit I2C address of the window.
    pub address: u8,
    /// Bank selector (0 when the family has no banks).
    pub bank: u8,
    /// Page selector (0 for lower/base memory).
    pub page: u8,
    /// Starting byte offset inside the 256-byte window.
    pub offset: u8,
    /// Length in bytes.
    pub len: u16,
}

impl Range {
    /// Whether the range is well-formed: a seven-bit address, nonzero
    /// length, and an extent inside the 256-byte window.
    pub fn validate(self) -> Result<Self, InputError> {
        if self.address > 0x7f || self.len == 0 || self.end() > 256 {
            return Err(InputError::InvalidRange(self));
        }
        Ok(self)
    }

    /// Exclusive end offset (`offset + len`).
    pub fn end(self) -> u32 {
        u32::from(self.offset) + u32::from(self.len)
    }

    /// Whether `other` lies entirely inside this range (same window and
    /// byte containment).
    pub fn contains(self, other: Self) -> bool {
        self.address == other.address
            && self.bank == other.bank
            && self.page == other.page
            && self.offset <= other.offset
            && self.end() >= other.end()
    }
}

/// Operation-end invalidations for the daemon to apply, not library effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalidation {
    /// Invalidate cached bytes covered by a planned write, even if unconfirmed.
    Covered(Range),
    /// Retire the password pin generation; no unlock survives the operation.
    Password,
}

/// The current bank/page selector state of one module: what the module's
/// paging window currently maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Selection {
    /// Seven-bit I2C address the selector governs.
    pub address: u8,
    /// Currently selected bank.
    pub bank: u8,
    /// Currently selected page.
    pub page: u8,
}

macro_rules! counter {
    ($($name:ident),+ $(,)?) => {$(
        /// Opaque monotonically increasing counter; see the named type's
        /// declaration site for its role.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(
            /// The counter value.
            pub u64,
        );
    )+};
}
counter!(
    ModuleGeneration,
    CacheEpoch,
    EventSequence,
    OperationId,
    PowerGeneration
);
counter!(MonoTime, MonoDuration);

/// Whether a captured (generation, epoch) stamp still governs a publication.
///
/// The pure `generation_valid` predicate the resident owner's instance-validity
/// seam routes through: a captured stamp governs exactly when both its
/// generation and its epoch equal the current ones. There is no other rule,
/// no table and no tolerance: a stamp that moved in either component is
/// retired, never partially valid.
pub fn generation_valid(
    captured_generation: ModuleGeneration,
    captured_epoch: CacheEpoch,
    current_generation: ModuleGeneration,
    current_epoch: CacheEpoch,
) -> bool {
    captured_generation == current_generation && captured_epoch == current_epoch
}

impl MonoTime {
    /// Add a duration, refusing overflow rather than wrapping.
    pub fn checked_add(self, duration: MonoDuration) -> Result<Self, InputError> {
        self.0
            .checked_add(duration.0)
            .map(Self)
            .ok_or(InputError::TimeOverflow)
    }

    /// The duration from `earlier` to this time, refusing a reversed
    /// interval rather than wrapping.
    pub fn checked_duration_since(self, earlier: Self) -> Result<MonoDuration, InputError> {
        self.0
            .checked_sub(earlier.0)
            .map(MonoDuration)
            .ok_or(InputError::ReversedTime)
    }
}

impl MonoDuration {
    /// Add two durations, refusing overflow rather than wrapping.
    pub fn checked_add(self, other: Self) -> Result<Self, InputError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(InputError::TimeOverflow)
    }

    /// Subtract `other`, refusing a negative result rather than wrapping.
    pub fn checked_sub(self, other: Self) -> Result<Self, InputError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(InputError::ReversedTime)
    }
}

/// What reading a byte does to the module, per the schema's read-effect
/// metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadEffect {
    /// Reading has no effect on module state.
    None,
    /// Reading clears a latched flag.
    ClearOnRead,
    /// Reading completes a startup-interrupt acknowledgment.
    AcknowledgeStartupInterrupt,
    /// A field claims the byte but its read behaviour is not asserted safe;
    /// fail closed rather than guess.
    Unknown,
    /// The byte is write-only and not readable through the normal read path.
    WriteOnly,
    /// No field metadata covers the byte at all.
    Uncovered,
}

/// Whether any field covering a byte may change its value on its own —
/// the input to cache freshness policy. This is a value statement only: it
/// says nothing about read effects (see [`ReadEffect`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeSource {
    /// No covering field changes value on its own (e.g. capability
    /// advertisement): a cached value stays valid until the module is
    /// re-initialized, so no freshness bound is owed.
    Static,
    /// At least one covering field may change value on its own (e.g.
    /// monitors, status, and latched flags): a cached value can be stale,
    /// so observation planning owes it a freshness bound.
    Dynamic,
}

/// Effects are a set, not a priority list: acknowledgment cannot erase Unknown.
///
/// Per-bit coverage of one byte: which bits are readable, which are covered
/// but of unknown read behaviour, which no field metadata covers at all,
/// which are write-only, and whether any covering field may change value
/// dynamically. Masks are bit-mapped (`0x01` = bit 0); `is_qualified_none`
/// and `passive_refusal` are the verdict combinators over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteClassification {
    /// The set of distinct read effects the byte's covering fields report
    /// (each with a nonzero constituent mask). A set, not a priority list:
    /// a `None` alongside a restrictive effect is a conflict, not a
    /// cancellation — an acknowledgment never erases an `Unknown`.
    /// May be empty; a byte with no known effect is never
    /// [`is_qualified_none`](Self::is_qualified_none).
    pub known_effects: Vec<ReadEffect>,
    /// Bits covered by readable fields (bit set = covered and readable).
    pub readable_mask: u8,
    /// Bits covered by fields whose read effect is not known to be safe:
    /// fail-closed coverage — the bit is claimed by metadata, but that
    /// metadata does not assert reading has no effect, so the passive test
    /// refuses the byte rather than guessing.
    pub unknown_mask: u8,
    /// Bits no field metadata covers at all (reserved ranges and
    /// out-of-window bytes): reading them is refused on absence of
    /// metadata, never silently treated as safe.
    pub uncovered_mask: u8,
    /// Bits covered by write-only fields: present in the map but not
    /// readable through the normal read path.
    pub write_only_mask: u8,
    /// Whether any covering field may change value on its own — the cache
    /// policy input, independent of the read-effect masks above.
    pub change_source: ChangeSource,
}

impl ByteClassification {
    /// Build one byte's classification from the compact mask form the code
    /// generator emits (and tests build synthetic footprints with).
    ///
    /// `known_effects` carries each effect with a nonzero constituent mask;
    /// it may be empty — a byte with no known effect is never
    /// [`is_qualified_none`](Self::is_qualified_none).
    pub fn from_masks(
        readable_mask: u8,
        unknown_mask: u8,
        uncovered_mask: u8,
        write_only_mask: u8,
        known_effects: &[ReadEffect],
        change_source: ChangeSource,
    ) -> Self {
        Self {
            known_effects: known_effects.to_vec(),
            readable_mask,
            unknown_mask,
            uncovered_mask,
            write_only_mask,
            change_source,
        }
    }

    /// Whether every bit of the byte is covered, readable, write-permissive
    /// and explicitly asserted to have no read effect — the passive test.
    pub fn is_qualified_none(&self) -> bool {
        self.readable_mask == 0xff
            && self.unknown_mask == 0
            && self.uncovered_mask == 0
            && self.write_only_mask == 0
            && !self.known_effects.is_empty()
            && self.known_effects.iter().all(|e| *e == ReadEffect::None)
    }
}

/// Who produced the bytes an observation carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationSource {
    /// Supplied by the caller (fixtures, prior observations).
    Supplied,
    /// Observed by the daemon through a live read.
    DaemonObserved,
}
/// Whether an observation is current or retained only as history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Current: valid for publication in the governing interval.
    Fresh,
    /// Retained for history; never re-published as current.
    Historical,
}

/// Provenance of one observation: the instance validity components and
/// the acquisition interval it ran in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationStamp {
    /// Module generation the observation was taken under.
    pub generation: ModuleGeneration,
    /// Cache epoch the observation was taken under.
    pub epoch: CacheEpoch,
    /// Monotonic time the acquisition started.
    pub started_at: MonoTime,
    /// Monotonic time the acquisition completed.
    pub completed_at: MonoTime,
    /// Who produced the bytes.
    pub source: ObservationSource,
    /// Whether the observation is current or historical.
    pub freshness: Freshness,
    /// Read effects the acquisition knowingly consumed (empty for a
    /// passive read).
    pub consumed_effects: Vec<ReadEffect>,
}

impl ObservationStamp {
    /// Validate the stamp's interval and that it still governs the named
    /// generation and epoch.
    pub fn validate(
        &self,
        generation: ModuleGeneration,
        epoch: CacheEpoch,
    ) -> Result<(), InputError> {
        self.completed_at.checked_duration_since(self.started_at)?;
        if self.generation != generation || self.epoch != epoch {
            return Err(InputError::StaleObservation);
        }
        Ok(())
    }
}

/// What an observation of one range found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeOutcome {
    /// The bytes the range read.
    Bytes(Vec<u8>),
    /// No bytes: the range was not supplied by any transfer.
    Absent,
    /// The read failed with the named reason.
    Failed {
        /// Failure reason as reported by the backend.
        reason: String,
    },
    /// The read's outcome cannot be decided with the named reason
    /// (for example a fence interrupted it).
    Uncertain {
        /// Why the outcome is undecided.
        reason: String,
    },
}

/// One observed range with its provenance stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeObservation {
    /// The observed range.
    pub range: Range,
    /// What the observation found.
    pub outcome: RangeOutcome,
    /// Provenance of the observation.
    pub stamp: ObservationStamp,
    /// Who asked for the read.
    pub requester: ReadRequester,
}

impl RangeObservation {
    /// The observation's bytes when the outcome carries exactly the range's
    /// byte count; `None` otherwise.
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.outcome {
            RangeOutcome::Bytes(bytes) if bytes.len() == usize::from(self.range.len) => Some(bytes),
            _ => None,
        }
    }
}

/// Sequential observations in one validity interval, NOT simultaneous sampling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The (start, end) interval the snapshot is valid in.
    pub validity_interval: (MonoTime, MonoTime),
    /// Module generation the snapshot was taken under.
    pub generation: ModuleGeneration,
    /// Cache epoch the snapshot was taken under.
    pub epoch: CacheEpoch,
    /// The snapshot's range observations (non-overlapping).
    pub ranges: Vec<RangeObservation>,
}

impl Snapshot {
    /// Validate the interval, every observation's range/stamp/length and
    /// pairwise non-overlap.
    pub fn validate(&self) -> Result<(), InputError> {
        self.validity_interval
            .1
            .checked_duration_since(self.validity_interval.0)?;
        for (index, observation) in self.ranges.iter().enumerate() {
            observation.range.validate()?;
            observation.stamp.validate(self.generation, self.epoch)?;
            if observation.stamp.started_at < self.validity_interval.0
                || observation.stamp.completed_at > self.validity_interval.1
                || observation.stamp.freshness != Freshness::Fresh
            {
                return Err(InputError::StaleObservation);
            }
            if let RangeOutcome::Bytes(bytes) = &observation.outcome {
                if bytes.len() != usize::from(observation.range.len) {
                    return Err(InputError::InvalidLength(observation.range));
                }
            }
            for previous in &self.ranges[..index] {
                let a = previous.range;
                let b = observation.range;
                if a.address == b.address
                    && a.bank == b.bank
                    && a.page == b.page
                    && u32::from(a.offset) < b.end()
                    && u32::from(b.offset) < a.end()
                {
                    return Err(InputError::OverlappingRanges);
                }
            }
        }
        Ok(())
    }
}

/// Retains OLD provenance even when publication of a consumed read was fenced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The retained observation.
    pub observation: RangeObservation,
    /// Event-sequence number the entry was recorded at.
    pub sequence: EventSequence,
}

/// Module capabilities the caller has established from the map, each
/// `None` until supplied (never defaulted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Whether the module reports flat (unpaged) memory.
    pub flat_memory: Option<bool>,
    /// Whether the module implements paged diagnostics.
    pub paged_diagnostics: Option<bool>,
    /// Whether diagnostics are implemented at all.
    pub diagnostics_implemented: Option<bool>,
    /// Number of lanes the module carries.
    pub lane_count: Option<u8>,
}

/// The caller's current access facts: the instance validity components
/// plus what the caller knows about the module's revision, capabilities and
/// selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessFacts {
    /// Module generation the facts were established under.
    pub generation: ModuleGeneration,
    /// Cache epoch the facts were established under.
    pub epoch: CacheEpoch,
    /// Module revision, when already established.
    pub revision: Option<Rev>,
    /// Module capabilities, as supplied.
    pub capabilities: Capabilities,
    /// Current selector state, when the caller holds one.
    pub selection: Option<Selection>,
    /// Whether the module is excluded from mutation (for example a
    /// read-only module).
    pub mutation_excluded: bool,
}

/// The outcome of an actuation (write) attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActuationOutcome {
    /// The actuation completed and its effect was confirmed.
    Confirmed,
    /// The actuation failed with the named reason.
    Failed {
        /// Failure reason as reported.
        reason: String,
    },
    /// The actuation's outcome is undecided with the named reason.
    Uncertain {
        /// Why the outcome is undecided.
        reason: String,
    },
}

/// Who a read serves; drives which read effects may be consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadRequester {
    /// Passive inspection: no side-effecting read may be consumed.
    PassiveInspection,
    /// Initialization: side-effecting reads needed to initialize are
    /// permitted.
    Initialization,
    /// Periodic monitoring.
    Monitoring,
    /// An explicit client request.
    ExplicitClient,
}

/// What a reader intends to acquire: the requester, the exact ranges, and
/// which read effects it consents to consume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadIntent {
    /// Who the read serves.
    pub requester: ReadRequester,
    /// The exact ranges to acquire.
    pub exact_ranges: Vec<Range>,
    /// Read effects the requester allows to be consumed.
    pub allowed_effects: Vec<ReadEffect>,
}

/// One physical transfer of a read plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadStep {
    /// The physical range this transfer acquires.
    pub physical_range: Range,
    /// The requester the step is executed for.
    pub requester: ReadRequester,
    /// Per-byte classification of the step's bytes.
    pub classifications: Vec<ByteClassification>,
    /// Read effects the step declares it will consume.
    pub declared_effects: Vec<ReadEffect>,
}

/// Location of a requested result within a physical step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultMapping {
    /// The requested range this mapping serves.
    pub requested_range: Range,
    /// Index into the plan's steps that carries the bytes.
    pub step_index: usize,
    /// Byte offset within that step where the requested range starts.
    pub offset_in_step: u16,
}

/// A bounded, passive read plan: the transfers to execute and how their
/// bytes map back onto the requested ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlan {
    /// The physical transfers, in execution order.
    pub steps: Vec<ReadStep>,
    /// How each requested range is served from the steps.
    pub result_mapping: Vec<ResultMapping>,
    /// The generation the plan is stamped with.
    pub required_generation: ModuleGeneration,
    /// The cache epoch the plan is stamped with.
    pub required_epoch: CacheEpoch,
}

/// A supplied input failed validation; no bytes were manufactured to
/// repair it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InputError {
    /// The range is malformed (address, empty length, or window overrun).
    #[error("invalid range: {0:?}")]
    InvalidRange(Range),
    /// A byte length does not match the range it was supplied for.
    #[error("invalid byte length for {0:?}")]
    InvalidLength(Range),
    /// The named range was not supplied by any observation.
    #[error("missing supplied bytes: {range:?}")]
    InsufficientInput {
        /// The range that is missing bytes.
        range: Range,
    },
    /// The revision has no schema surface to decode with.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// A monotonic-time computation would overflow.
    #[error("monotonic time overflow")]
    TimeOverflow,
    /// A monotonic interval was given with its ends reversed.
    #[error("reversed monotonic times")]
    ReversedTime,
    /// The observation's generation, epoch or validity interval no longer
    /// governs publication.
    #[error("stale generation, epoch or validity interval")]
    StaleObservation,
    /// Two observations cover overlapping bytes.
    #[error("overlapping observations")]
    OverlappingRanges,
}

/// An observation is borrowed, not manufactured from missing or failed input.
pub trait ByteView {
    /// The observation that covers `range`, when the view holds one.
    fn get(&self, range: Range) -> Option<&RangeObservation>;

    /// Validate view-wide provenance before extracting any field. Custom views
    /// may override this to reject crossed mutation/validity boundaries.
    fn validate_view(&self) -> Result<(), InputError> {
        Ok(())
    }
}

impl ByteView for Snapshot {
    fn validate_view(&self) -> Result<(), InputError> {
        self.validate()
    }

    fn get(&self, range: Range) -> Option<&RangeObservation> {
        if range.validate().is_err() || self.validate().is_err() {
            return None;
        }
        self.ranges.iter().find(|o| o.range.contains(range))
    }
}

/// Extract supplied bytes, checking validity even for third-party ByteViews.
/// A field may span multiple disjoint supplied observations.
pub fn supplied_bytes(
    view: &impl ByteView,
    range: Range,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<Vec<u8>, InputError> {
    range.validate()?;
    view.validate_view()?;
    let mut bytes = Vec::with_capacity(usize::from(range.len));
    for offset in u32::from(range.offset)..range.end() {
        let byte_range = Range {
            offset: offset as u8,
            len: 1,
            ..range
        };
        let observation = view
            .get(byte_range)
            .ok_or(InputError::InsufficientInput { range: byte_range })?;
        observation.range.validate()?;
        observation.stamp.validate(generation, epoch)?;
        if observation.stamp.freshness != Freshness::Fresh {
            return Err(InputError::StaleObservation);
        }
        if !observation.range.contains(byte_range) {
            return Err(InputError::InvalidRange(observation.range));
        }
        let data = observation
            .bytes()
            .ok_or(InputError::InsufficientInput { range: byte_range })?;
        bytes.push(data[usize::from(byte_range.offset - observation.range.offset)]);
    }
    Ok(bytes)
}

/// Schema-derived identity locations supplied by the family layer. Core does
/// not contain a second family memory map or depend on generated family crates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityLayout {
    /// The revision whose layout this is.
    pub revision: Rev,
    /// Vendor-name string range.
    pub vendor_name: Range,
    /// Vendor part-number string range.
    pub vendor_pn: Range,
    /// Vendor revision-code string range.
    pub vendor_rev: Range,
    /// Vendor serial-number string range.
    pub vendor_sn: Range,
    /// Date-code string range.
    pub date_code: Range,
    /// OUI (three bytes).
    pub oui: Range,
    /// The SFF-8024 identifier byte (byte 0 of the lower / A0h page),
    /// when the family layout carries it. Additive: `decode_identity`
    /// itself does not decode the identifier (the meaning lives in the
    /// generated family enums); the family layer reads this range and
    /// fills [`crate::types::VendorInfo::identifier`]. `None` keeps the
    /// portable extraction seam exactly as it was.
    pub identifier: Option<Range>,
}

/// Pure identity extraction using supplied family/revision layout and bytes.
/// This is not permission to acquire the ranges on a device. CMIS 3.0, 4.0, 5.0,
/// 5.1, 5.2, and 5.3 share the identity field encodings; the family layer
/// supplies the schema-derived footprint, not a second memory map here.
pub fn decode_identity(
    layout: &IdentityLayout,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<VendorInfo, InputError> {
    let rev = layout.revision;
    let supported = match rev.family {
        Family::Cmis => matches!(
            rev,
            Rev::CMIS_3_0
                | Rev::CMIS_4_0
                | Rev::CMIS_5_0
                | Rev::CMIS_5_1
                | Rev::CMIS_5_2
                | Rev::CMIS_5_3
                | Rev::CMIS_5_4
        ),
        Family::Sff8472 => rev.major == 12 && rev.minor == 5,
        Family::Sff8636 => rev.major == 2 && rev.minor == 12,
        _ => false,
    };
    if !supported {
        return Err(InputError::UnsupportedRevision(rev));
    }
    if layout.oui.len != 3 {
        return Err(InputError::InvalidLength(layout.oui));
    }
    let text = |range| -> Result<String, InputError> {
        let bytes = supplied_bytes(view, range, generation, epoch)?;
        Ok(String::from_utf8_lossy(&bytes)
            .trim_matches([' ', '\0'])
            .to_owned())
    };
    Ok(VendorInfo {
        name: text(layout.vendor_name)?,
        part_number: text(layout.vendor_pn)?,
        rev: text(layout.vendor_rev)?,
        serial_number: text(layout.vendor_sn)?,
        date_code: text(layout.date_code)?,
        oui: supplied_bytes(view, layout.oui, generation, epoch)?
            .try_into()
            .expect("validated OUI length"),
        identifier: None,
        // No detect-once revision context on the supplied-identity path
        // (W1 cascade); callers who have one set it explicitly.
        spec_rev: None,
    })
}

// ---------------------------------------------------------------------------
// Bounded R1 read planning (classify, plan, assemble)
// ---------------------------------------------------------------------------

/// Typed reason one byte of a requested range is not passively readable.
///
/// A byte is passively readable only when its metadata explicitly says
/// reading it has no effect ([`ByteClassification::is_qualified_none`]).
/// Read-only access alone proves nothing, so every way the metadata fails to
/// say that is named here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonPassiveReason {
    /// Reading the byte clears a latched flag.
    ClearOnRead,
    /// Reading the byte completes an initialization acknowledgment.
    AcknowledgeStartupInterrupt,
    /// The access value is unknown, or no read-effect metadata asserts reading
    /// the byte has no effect.
    ReadEffectUnknown,
    /// No field metadata covers the byte at all.
    MetadataAbsent,
    /// The byte is write-only.
    WriteOnly,
    /// The covering fields carry conflicting read-effect metadata.
    Conflict,
}

impl NonPassiveReason {
    /// Stable identifier for events, refusals and tests.
    pub const fn id(self) -> &'static str {
        match self {
            NonPassiveReason::ClearOnRead => "clear-on-read",
            NonPassiveReason::AcknowledgeStartupInterrupt => "acknowledge-startup-interrupt",
            NonPassiveReason::ReadEffectUnknown => "read-effect-unknown",
            NonPassiveReason::MetadataAbsent => "metadata-absent",
            NonPassiveReason::WriteOnly => "write-only",
            NonPassiveReason::Conflict => "conflict",
        }
    }
}

/// The reason one byte's classification fails the passive test, in the fixed
/// order the refusal is reported in.
fn non_passive_reason(classification: &ByteClassification) -> NonPassiveReason {
    if classification.uncovered_mask != 0 {
        return NonPassiveReason::MetadataAbsent;
    }
    if classification.unknown_mask != 0 {
        return NonPassiveReason::ReadEffectUnknown;
    }
    if classification.write_only_mask != 0 {
        return NonPassiveReason::WriteOnly;
    }
    let mut named = None;
    let mut conflicting = false;
    for effect in &classification.known_effects {
        match effect {
            ReadEffect::None => {}
            other => {
                if named.is_some() {
                    conflicting = true;
                } else {
                    named = Some(*other);
                }
            }
        }
    }
    if named.is_some() && classification.known_effects.contains(&ReadEffect::None) {
        conflicting = true;
    }
    if conflicting {
        return NonPassiveReason::Conflict;
    }
    match named {
        Some(ReadEffect::ClearOnRead) => NonPassiveReason::ClearOnRead,
        Some(ReadEffect::AcknowledgeStartupInterrupt) => {
            NonPassiveReason::AcknowledgeStartupInterrupt
        }
        // No restrictive effect, yet the byte still fails `is_qualified_none`:
        // some constituent has no qualified-none assertion at all.
        _ => NonPassiveReason::ReadEffectUnknown,
    }
}

impl ByteClassification {
    /// Why this byte is not positively passive, using the shared refusal order.
    /// `None` means every physical bit is explicitly qualified as no-effect.
    pub fn passive_refusal(&self) -> Option<NonPassiveReason> {
        (!self.is_qualified_none()).then(|| non_passive_reason(self))
    }
}

/// What classifying one requested range answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeClassified {
    /// Every byte of the range is passively readable: the metadata explicitly
    /// says reading it has no effect.
    Passive {
        /// Whether any constituent may change dynamically (for cache policy).
        change_source: ChangeSource,
    },
    /// The named byte is not passively readable, for the typed reason.
    NonPassive {
        /// The byte's absolute offset in the EEPROM window.
        byte: u8,
        /// Why reading it is not passive.
        reason: NonPassiveReason,
    },
    /// The footprint names no metadata covering the range's addressing.
    Uncovered,
}

/// The per-byte classification footprint of exactly one revision, supplied by
/// the family layer from the generated schema metadata.
///
/// This is data, not a decision: it covers CMIS module memory at one device
/// address — lower memory `0x00`–`0x7F` and upper page `00h` (bank 0, page 0)
/// `0x80`–`0xFF` — and nothing else. A range addressing anything beyond that is
/// [`RangeClassified::Uncovered`], never a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFootprint {
    /// The only revision this footprint's metadata was generated for.
    pub revision: Rev,
    /// The seven-bit device address the footprint covers.
    pub address: u8,
    /// Lower memory, offsets `0x00`–`0x7F` (bank 0, page 0).
    pub lower: [ByteClassification; 128],
    /// Upper page `00h` (bank 0, page 0), offsets `0x80`–`0xFF`.
    pub upper_page00: [ByteClassification; 128],
}

impl ReadFootprint {
    /// One byte's classification, when the footprint covers that addressing.
    ///
    /// The returned classification is borrowed, never synthesized: an
    /// addressing the footprint does not cover answers `None`.
    pub fn byte(&self, address: u8, bank: u8, page: u8, offset: u8) -> Option<&ByteClassification> {
        if address != self.address || bank != 0 || page != 0 {
            return None;
        }
        match offset {
            0..=127 => Some(&self.lower[usize::from(offset)]),
            _ => Some(&self.upper_page00[usize::from(offset - 128)]),
        }
    }

    /// Whether the footprint's addressing covers the (already valid) range.
    fn covers(&self, range: Range) -> bool {
        range.address == 0x50 && self.address == 0x50 && range.bank == 0 && range.page == 0
    }

    /// Classify one requested range against the footprint's metadata.
    ///
    /// The answer names the first byte that fails the passive test, with the
    /// typed reason; a range whose addressing the footprint does not cover is
    /// [`RangeClassified::Uncovered`]. A malformed range is a typed input
    /// error, never a classification.
    pub fn classify_range(&self, range: Range) -> Result<RangeClassified, InputError> {
        range.validate()?;
        if !self.covers(range) {
            return Ok(RangeClassified::Uncovered);
        }
        let mut change_source = ChangeSource::Static;
        for offset in u32::from(range.offset)..range.end() {
            // `covers` admitted the addressing and `validate` the bounds, so
            // every byte of the range is inside one of the two tables.
            let classification = self
                .byte(range.address, range.bank, range.page, offset as u8)
                .expect("covered addressing and validated bounds");
            if classification.change_source == ChangeSource::Dynamic {
                change_source = ChangeSource::Dynamic;
            }
            if let Some(reason) = classification.passive_refusal() {
                return Ok(RangeClassified::NonPassive {
                    byte: offset as u8,
                    reason,
                });
            }
        }
        Ok(RangeClassified::Passive { change_source })
    }
}

/// The supplied planning limits one `plan_observation` call carries.
///
/// Bounds are literal, including zero. A zero range/byte allowance admits
/// nothing; a zero transfer limit is invalid. No omitted/unlimited grant is
/// inferred from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    /// How many ranges one observation may request.
    pub max_ranges: usize,
    /// How many bytes one observation may request in total.
    pub max_bytes: u32,
    /// The largest transfer one plan step may name, in bytes.
    pub max_transfer_len: u16,
}

/// The values one `plan_observation` call receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanObservationInput {
    /// The revision the module advertises and the request is planned under.
    pub revision: Rev,
    /// Who asked for the observation.
    pub requester: ReadRequester,
    /// The exact ranges to read, in the order the requester named them.
    pub ranges: Vec<Range>,
    /// The supplied planning limits.
    pub limits: ReadLimits,
    /// The caller's statement of the current selector, when it holds one.
    /// Planning an upper-page range requires it to name bank 0 / page 0.
    pub selector: Option<Selection>,
    /// Whether the module reports flat memory, when the caller knows.
    /// A flat-memory module needs no selector for upper page `00h`.
    pub flat_memory: Option<bool>,
    /// The generation the plan is stamped with.
    pub generation: ModuleGeneration,
    /// The cache epoch the plan is stamped with.
    pub epoch: CacheEpoch,
}

/// Why `plan_observation` refused the observation.
///
/// Every refusal is typed and attributable; plans are never widened,
/// trimmed or partially accepted to avoid one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanRefusal {
    /// A transfer cannot contain zero bytes.
    #[error("transfer limit must be nonzero")]
    InvalidTransferLimit,
    /// The revision the module advertises has no delivered footprint.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// The request names no range.
    #[error("no ranges requested")]
    NoRanges,
    /// A range is malformed (empty, or outside the EEPROM window).
    #[error("malformed range: {0:?}")]
    MalformedRange(Range),
    /// Two requested ranges overlap.
    #[error("overlapping requested ranges")]
    OverlappingRanges,
    /// The request exceeds its supplied range limit.
    #[error("too many requested ranges: {requested} > {limit}")]
    TooManyRanges {
        /// How many ranges the request named.
        requested: usize,
        /// The limit the input carried.
        limit: usize,
    },
    /// The request exceeds its supplied byte limit.
    #[error("too many requested bytes: {requested} > {limit}")]
    TooManyBytes {
        /// How many bytes the request named.
        requested: u64,
        /// The limit the input carried.
        limit: u32,
    },
    /// The named byte is not passively readable, for the typed reason.
    #[error("byte {byte:#04x} is not passively readable: {}", reason.id())]
    NonPassive {
        /// The byte's absolute offset in the EEPROM window.
        byte: u8,
        /// Why reading it is not passive.
        reason: NonPassiveReason,
    },
    /// The footprint names no metadata covering the range's addressing.
    #[error("no footprint metadata covers {range:?}")]
    Uncovered {
        /// The range no footprint covers.
        range: Range,
    },
    /// An upper-page range needs a selector the caller did not state, and a
    /// selector change is a write this library does not deliver.
    #[error("selector change to bank {bank} page {page} required, not delivered")]
    SelectorChangeRequired {
        /// The bank the selector would need.
        bank: u8,
        /// The page the selector would need.
        page: u8,
    },
}

/// Plan one observation: classify every requested byte, refuse what is not
/// passively readable, and split what remains into bounded transfers.
///
/// The plan is read-only: no step ever contains a selector (bank/page) write.
/// A range in upper page `00h` is plannable only when the caller states the
/// current selector is bank 0 / page 0, or that the module reports flat
/// memory; anything else is [`PlanRefusal::SelectorChangeRequired`].
/// Transfers never span the 128-byte half boundary between lower memory and
/// upper page `00h` (the transport contract reads one half per transfer) and
/// respect the input's transfer limit. The returned plan is stamped with the
/// input's generation and epoch; nothing wider, shorter or partial is ever
/// returned.
pub fn plan_observation(
    footprint: &ReadFootprint,
    input: &PlanObservationInput,
) -> Result<ReadPlan, PlanRefusal> {
    if !matches!(input.revision, Rev::CMIS_4_0 | Rev::CMIS_5_3)
        || input.revision != footprint.revision
    {
        return Err(PlanRefusal::UnsupportedRevision(input.revision));
    }
    if input.ranges.is_empty() {
        return Err(PlanRefusal::NoRanges);
    }
    let mut requested_bytes = 0u64;
    for range in &input.ranges {
        range
            .validate()
            .map_err(|_| PlanRefusal::MalformedRange(*range))?;
        requested_bytes += u64::from(range.len);
    }
    for (index, a) in input.ranges.iter().enumerate() {
        for b in &input.ranges[index + 1..] {
            if a.address == b.address
                && a.bank == b.bank
                && a.page == b.page
                && u32::from(a.offset) < b.end()
                && u32::from(b.offset) < a.end()
            {
                return Err(PlanRefusal::OverlappingRanges);
            }
        }
    }
    if input.ranges.len() > input.limits.max_ranges {
        return Err(PlanRefusal::TooManyRanges {
            requested: input.ranges.len(),
            limit: input.limits.max_ranges,
        });
    }
    if requested_bytes > u64::from(input.limits.max_bytes) {
        return Err(PlanRefusal::TooManyBytes {
            requested: requested_bytes,
            limit: input.limits.max_bytes,
        });
    }
    if input.limits.max_transfer_len == 0 {
        return Err(PlanRefusal::InvalidTransferLimit);
    }
    let mut upper_requested = false;
    for range in &input.ranges {
        if range.end() > 128 {
            upper_requested = true;
        }
        match footprint.classify_range(*range) {
            Ok(RangeClassified::Passive { .. }) => {}
            Ok(RangeClassified::NonPassive { byte, reason }) => {
                return Err(PlanRefusal::NonPassive { byte, reason })
            }
            Ok(RangeClassified::Uncovered) => return Err(PlanRefusal::Uncovered { range: *range }),
            Err(InputError::InvalidRange(range)) => return Err(PlanRefusal::MalformedRange(range)),
            Err(other) => unreachable!("classify_range refuses only invalid ranges: {other:?}"),
        }
    }
    if upper_requested {
        let admitted = input.flat_memory == Some(true)
            || input
                .selector
                .is_some_and(|s| s.address == footprint.address && s.bank == 0 && s.page == 0);
        if !admitted {
            return Err(PlanRefusal::SelectorChangeRequired { bank: 0, page: 0 });
        }
    }
    let mut steps: Vec<ReadStep> = Vec::new();
    let mut result_mapping: Vec<ResultMapping> = Vec::new();
    for range in &input.ranges {
        let mut offset = u32::from(range.offset);
        let end = range.end();
        while offset < end {
            // One transfer never spans the half boundary: lower memory and the
            // upper page are separate transport lines. The caller's transfer
            // limit bounds it further.
            let half_cap = if offset < 128 {
                128 - offset
            } else {
                256 - offset
            };
            let len = half_cap
                .min(end - offset)
                .min(u32::from(input.limits.max_transfer_len));
            let classifications: Vec<ByteClassification> = (offset..offset + len)
                .map(|o| {
                    footprint
                        .byte(range.address, range.bank, range.page, o as u8)
                        .cloned()
                        .expect("classified range is inside the footprint")
                })
                .collect();
            result_mapping.push(ResultMapping {
                requested_range: *range,
                step_index: steps.len(),
                offset_in_step: 0,
            });
            steps.push(ReadStep {
                physical_range: Range {
                    offset: offset as u8,
                    len: len as u16,
                    ..*range
                },
                requester: input.requester,
                classifications,
                declared_effects: vec![ReadEffect::None],
            });
            offset += len;
        }
    }
    Ok(ReadPlan {
        steps,
        result_mapping,
        required_generation: input.generation,
        required_epoch: input.epoch,
    })
}

/// One finished plan step the backend reports, as the assembly's input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferResult {
    /// The accepted plan step this result belongs to.
    pub step_index: usize,
    /// The exact range this transfer reports (must equal its plan step).
    pub range: Range,
    /// The generation the transfer ran under.
    pub generation: ModuleGeneration,
    /// The cache epoch the transfer ran under.
    pub epoch: CacheEpoch,
    /// The bytes the transfer read.
    pub bytes: Vec<u8>,
}

/// The values one `assemble_snapshot` call receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembleInput<'a> {
    /// The accepted plan whose transfers these results are.
    pub plan: &'a ReadPlan,
    /// The finished steps the backend reported.
    pub results: &'a [TransferResult],
    /// The generation still governing publication.
    pub generation: ModuleGeneration,
    /// The epoch still governing publication.
    pub epoch: CacheEpoch,
    /// Supplied time the observation started.
    pub started_at: MonoTime,
    /// Supplied time the observation completed.
    pub completed_at: MonoTime,
}

/// Why `assemble_snapshot` refused to assemble.
///
/// The refusal is typed and names the offending transfer; assembly never
/// drops, pads or zero-fills a transfer to salvage a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssembleRefusal {
    /// The plan is retired in the caller's current validity interval.
    #[error("plan stamp does not match current generation or epoch")]
    PlanStampMismatch,
    /// A supplied public plan is malformed, effectful, widened or inconsistent.
    #[error("malformed or inconsistent read plan")]
    InvalidPlan,
    /// The supplied interval is reversed or otherwise invalid.
    #[error("invalid observation interval: {0}")]
    InvalidInterval(InputError),
    /// A result reports a range other than its plan's own transfer.
    #[error("transfer {index} range does not match its plan step")]
    RangeMismatch {
        /// The step index whose range differs.
        index: usize,
    },
    /// A result names no step of the plan.
    #[error("result names no step of the plan: index {index}")]
    UnknownStep {
        /// The step index the result named.
        index: usize,
    },
    /// Two results name the same plan step.
    #[error("two results name the same plan step: index {index}")]
    DuplicateStep {
        /// The step index named twice.
        index: usize,
    },
    /// A transfer's byte count does not match its step.
    #[error("transfer {index} supplied {supplied} bytes, step names {expected}")]
    TransferLength {
        /// The step index.
        index: usize,
        /// The step's byte count.
        expected: u16,
        /// The supplied byte count.
        supplied: usize,
    },
    /// A transfer ran under a different generation or epoch than the plan.
    #[error("transfer {index} stamp does not match the plan's generation or epoch")]
    StampMismatch {
        /// The step index.
        index: usize,
    },
}

// Task 24 plans have public fields. Do not trust a forged/stale mapping enough
// to panic or publish bytes; validate the bounded, passive, exact-cover shape.
fn validate_read_plan(plan: &ReadPlan) -> Result<Vec<Range>, AssembleRefusal> {
    let mut requested = Vec::<Range>::new();
    let mut used = vec![false; plan.steps.len()];
    for mapping in &plan.result_mapping {
        let range = mapping.requested_range;
        let step = plan
            .steps
            .get(mapping.step_index)
            .ok_or(AssembleRefusal::InvalidPlan)?;
        let physical = step.physical_range;
        if range.validate().is_err()
            || physical.validate().is_err()
            || !range.contains(physical)
            || mapping.offset_in_step != 0
            || physical.address != 0x50
            || physical.bank != 0
            || physical.page != 0
            || (physical.offset < 128 && physical.end() > 128)
            || step.classifications.len() != usize::from(physical.len)
            || !step
                .classifications
                .iter()
                .all(ByteClassification::is_qualified_none)
            || step.declared_effects != [ReadEffect::None]
            || used[mapping.step_index]
        {
            return Err(AssembleRefusal::InvalidPlan);
        }
        used[mapping.step_index] = true;
        if !requested.contains(&range) {
            requested.push(range);
        }
    }
    if requested.is_empty() || used.contains(&false) {
        return Err(AssembleRefusal::InvalidPlan);
    }
    for (index, range) in requested.iter().enumerate() {
        if requested[..index].iter().any(|previous| {
            previous.address == range.address
                && previous.bank == range.bank
                && previous.page == range.page
                && u32::from(previous.offset) < range.end()
                && u32::from(range.offset) < previous.end()
        }) {
            return Err(AssembleRefusal::InvalidPlan);
        }
        let mut requester = None;
        for offset in u32::from(range.offset)..range.end() {
            let mut count = 0;
            for mapping in plan
                .result_mapping
                .iter()
                .filter(|mapping| mapping.requested_range == *range)
            {
                let step = &plan.steps[mapping.step_index];
                if requester.is_some_and(|requester| requester != step.requester) {
                    return Err(AssembleRefusal::InvalidPlan);
                }
                requester = Some(step.requester);
                if u32::from(step.physical_range.offset) <= offset
                    && offset < step.physical_range.end()
                {
                    count += 1;
                }
            }
            if count != 1 {
                return Err(AssembleRefusal::InvalidPlan);
            }
        }
    }
    Ok(requested)
}

/// Assemble the snapshot of one observation from its plan's own transfers.
///
/// Only bytes the plan's own transfers supplied are served, under the plan's
/// own generation and epoch: a result outside the plan, a duplicated step, a
/// transfer whose length does not match its step, or one stamped with another
/// generation/epoch is a typed refusal. A byte no reported transfer supplied
/// stays missing ([`RangeOutcome::Absent`]), never zero-filled: a genuinely
/// supplied `0x00` is served, a missing byte is not.
pub fn assemble_snapshot(input: &AssembleInput<'_>) -> Result<Snapshot, AssembleRefusal> {
    let plan = input.plan;
    if !generation_valid(
        plan.required_generation,
        plan.required_epoch,
        input.generation,
        input.epoch,
    ) {
        return Err(AssembleRefusal::PlanStampMismatch);
    }
    input
        .completed_at
        .checked_duration_since(input.started_at)
        .map_err(AssembleRefusal::InvalidInterval)?;
    let requested_ranges = validate_read_plan(plan)?;
    let mut by_step: std::collections::HashMap<usize, &TransferResult> =
        std::collections::HashMap::with_capacity(input.results.len());
    for result in input.results {
        let step = plan
            .steps
            .get(result.step_index)
            .ok_or(AssembleRefusal::UnknownStep {
                index: result.step_index,
            })?;
        if result.bytes.len() != usize::from(step.physical_range.len) {
            return Err(AssembleRefusal::TransferLength {
                index: result.step_index,
                expected: step.physical_range.len,
                supplied: result.bytes.len(),
            });
        }
        if result.range != step.physical_range {
            return Err(AssembleRefusal::RangeMismatch {
                index: result.step_index,
            });
        }
        if result.generation != plan.required_generation || result.epoch != plan.required_epoch {
            return Err(AssembleRefusal::StampMismatch {
                index: result.step_index,
            });
        }
        if by_step.insert(result.step_index, result).is_some() {
            return Err(AssembleRefusal::DuplicateStep {
                index: result.step_index,
            });
        }
    }
    let mut ranges = Vec::new();
    for requested in requested_ranges {
        let mappings: Vec<_> = plan
            .result_mapping
            .iter()
            .filter(|mapping| mapping.requested_range == requested)
            .collect();
        let mut bytes = Vec::with_capacity(usize::from(requested.len));
        let mut complete = true;
        // Order by the requested address, not the backend's completion order
        // (nor a public plan's mapping order). Nothing initializes missing bytes.
        for offset in u32::from(requested.offset)..requested.end() {
            let mapping = mappings
                .iter()
                .find(|mapping| {
                    let physical = plan.steps[mapping.step_index].physical_range;
                    u32::from(physical.offset) <= offset && offset < physical.end()
                })
                .expect("validated plan covers each byte exactly once");
            let physical = plan.steps[mapping.step_index].physical_range;
            if let Some(result) = by_step.get(&mapping.step_index) {
                bytes.push(result.bytes[(offset - u32::from(physical.offset)) as usize]);
            } else {
                complete = false;
                break;
            }
        }
        ranges.push(RangeObservation {
            range: requested,
            outcome: if complete {
                RangeOutcome::Bytes(bytes)
            } else {
                RangeOutcome::Absent
            },
            stamp: ObservationStamp {
                generation: plan.required_generation,
                epoch: plan.required_epoch,
                started_at: input.started_at,
                completed_at: input.completed_at,
                source: ObservationSource::Supplied,
                freshness: Freshness::Fresh,
                consumed_effects: Vec::new(),
            },
            requester: plan.steps[mappings[0].step_index].requester,
        });
    }
    Ok(Snapshot {
        validity_interval: (input.started_at, input.completed_at),
        generation: plan.required_generation,
        epoch: plan.required_epoch,
        ranges,
    })
}

#[cfg(test)]
mod generation_valid_tests {
    use super::{generation_valid, CacheEpoch, ModuleGeneration};

    #[test]
    fn a_stamp_governs_only_when_both_components_match() {
        let generation = ModuleGeneration(4);
        let epoch = CacheEpoch(2);
        assert!(generation_valid(generation, epoch, generation, epoch));
        assert!(!generation_valid(
            generation,
            epoch,
            ModuleGeneration(5),
            epoch
        ));
        assert!(!generation_valid(
            generation,
            epoch,
            generation,
            CacheEpoch(3)
        ));
        assert!(!generation_valid(
            ModuleGeneration(0),
            CacheEpoch(0),
            generation,
            epoch
        ));
    }
}
