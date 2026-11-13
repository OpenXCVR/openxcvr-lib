//! Library-issued prerequisite-I/O receipts: original CMIS 5.3 identity and
//! the explicitly scoped internal-D4 CMIS 4.0 addition.
//!
//! The original functions and committed 5.3 draft keep their 5.3 answer.
//! Revision-aware functions never substitute 5.3 classification for 4.0.
//!
//! The daemon admits a `prerequisite-io-contract` (capability
//! `instance-prerequisite-io`) from its admissions document and reads identity
//! through the USB transport only under that contract's grants. This module
//! is how the library answers it: a small generator that emits the daemon's
//! § 12 JSON shape (`docs/dependencies/library-receipt.md` in the daemon
//! repository) for CMIS 5.3 at this revision, with every footprint
//! qualification **computed** from [`CmisReadPlanner::classify_range`] over
//! the library's own footprint registry — the daemon never hand-writes
//! `qualified` for any footprint, and neither does this crate's owner.
//!
//! Two rules keep the answer honest:
//!
//! - a step the library cannot support appears as an explicit **denial**
//!   (`{"step": ..., "denied": <reason>}`), never as a fabricated grant
//!   naming an unqualified footprint: the contract still names all five
//!   prerequisite steps exactly once, granted or denied, so no step is
//!   dropped silently ([`read_grant`] refuses a plan over non-passive bytes,
//!   and unsupported steps never reach it at all);
//! - each footprint carries the byte ranges its qualification is computed
//!   over, and the qualified footprint a granted plan names covers exactly
//!   the bytes that plan reads, so no qualification ever covers a byte the
//!   library did not classify — the daemon's gate refuses a plan outside its
//!   footprint's ranges, and this generator refuses first, at its own
//!   classification.
//!
//! Nothing here reads hardware, opens a device or issues a transfer: the
//! receipt is a reviewed record of the library's classification answer, and
//! a committed receipt is a **draft** (see [`receipt_json`]). The revision
//! pins (`library_revision`, `connector_revision`) are inputs the consumer
//! supplies, never guessed here; the committed receipt next to the crate tests
//! records the exact command that produced it.

use crate::read_planning::CmisReadPlanner;
use openxcvr_core::rev::Rev;
use openxcvr_core::types::management::{InputError, NonPassiveReason, Range, RangeClassified};
use serde::Serialize;

/// The daemon's identity-read footprint: exactly the bytes the library's
/// generated tables classify as passively readable at CMIS 5.3 — lower
/// 0–2 (Table 8-5 identity bytes), lower 126–127 (BankSelect/PageSelect, the
/// paged module's reads-only selector readback, § 8.2.15) and upper page-00h
/// 129–189 (Table 8-28 vendor identity bytes). The ranges are exactly the
/// bytes the granted plans read: the detection read 0..3, its 126..128
/// paged readback and the pre-identification read 129..190.
pub const IDENTITY_FOOTPRINT: &str = "cmis-5.3-lower-page00h-identity";

/// The readiness bootstrap's window: lower byte 3 (ModuleState), whose
/// per-bit readiness/detection marking the library has not authored.
const MODULE_STATE_FOOTPRINT: &str = "cmis-5.3-lower-module-state";

/// The duration acquisition's window: the page-01h advertised-duration
/// bytes, which the library's classification (bank 0, page 00h) does not
/// cover.
const DURATION_FOOTPRINT: &str = "cmis-page-01h-duration";

/// The unresolved per-bit readiness/detection marking the daemon names
/// `C2.1`. It is a marking identifier, not a byte window, so it carries no
/// ranges to classify: the library's qualification record keeps it
/// `unresolved` until the per-bit marking is authored, exactly as the
/// daemon's § 12 record requires.
pub const C2_1_FOOTPRINT: &str = "C2.1";

/// The contract kind, version, capability and identity the daemon's
/// prerequisite-member parser admits (its closed member set).
const KIND: &str = "prerequisite-io-contract";
const VERSION: &str = "1";
const CAPABILITY: &str = "instance-prerequisite-io";
const OWNER: &str = "petabit";
const REPOSITORY: &str = "openxcvr-lib";

/// Why the library denies `startup-power-hold`: the board-control plan a
/// power hold names needs the per-bit readiness/detection marking, which is
/// unresolved at this revision.
const STARTUP_POWER_HOLD_DENIED: &str = "the per-bit readiness/detection marking (C2.1) is \
    unresolved at this revision, so no board-control plan for a power hold is qualified";

/// CMIS 5.3 byte-3 read safety does not itself authorize a readiness policy.
/// Keep this contract's existing explicit denial; qualification and the
/// admission of an automatic prerequisite step are separate decisions.
const READINESS_BOOTSTRAP_DENIED: &str = "the module-state byte at lower page 00h offset 3 \
    (footprint cmis-5.3-lower-module-state) is passive, but this contract does not issue a CMIS 5.3 readiness-bootstrap grant";

/// Why the library denies `duration-acquisition`: the advertised-duration
/// bytes lie on page 01h, outside the classification coverage of this
/// revision's tables, so the paged selector plan the step needs is not
/// qualified.
const DURATION_ACQUISITION_DENIED: &str =
    "the advertised-duration bytes at bank 0 page 01h offset \
    166 lie outside the library's classification coverage (footprint \
    cmis-page-01h-duration is uncovered), so no paged selector plan is qualified";

/// One module-memory range of a footprint window.
const fn window(page: u8, offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page,
        offset,
        len,
    }
}

/// The library's footprint registry: every byte-window footprint the receipt
/// carries, with the ranges its qualification is computed over. A footprint
/// is `qualified` only when `classify_range` answers [`RangeClassified::Passive`]
/// for **every** range; any non-passive byte types the footprint's
/// non-qualified answer instead. The [`C2_1_FOOTPRINT`] marking identifier is
/// carried outside this registry with its constant `unresolved` answer.
const FOOTPRINT_RANGES: &[(&str, &[Range])] = &[
    (
        IDENTITY_FOOTPRINT,
        &[window(0, 0, 3), window(0, 126, 2), window(0, 129, 61)],
    ),
    (MODULE_STATE_FOOTPRINT, &[window(0, 3, 1)]),
    (DURATION_FOOTPRINT, &[window(1, 166, 2)]),
];

/// CMIS 4.0 source-qualified identity, including all eight bits of byte 2.
/// Its detection windows remain separate from the dynamic byte-3 bootstrap.
pub const CMIS4_IDENTITY_FOOTPRINT: &str = "cmis-4.0-lower-page00h-identity";
/// Source-qualified dynamic byte-3 footprint for scoped internal D4 only.
pub const CMIS4_MODULE_STATE_FOOTPRINT: &str = "cmis-4.0-lower-module-state";
const FOOTPRINT_RANGES_40: &[(&str, &[Range])] = &[
    (
        CMIS4_IDENTITY_FOOTPRINT,
        &[window(0, 0, 3), window(0, 126, 2), window(0, 129, 61)],
    ),
    ("cmis-4.0-detection", &[window(0, 0, 3)]),
    (CMIS4_MODULE_STATE_FOOTPRINT, &[window(0, 3, 1)]),
    (DURATION_FOOTPRINT, &[window(1, 166, 2)]),
];

fn footprint_ranges(
    revision: Rev,
) -> Result<&'static [(&'static str, &'static [Range])], InputError> {
    match revision {
        Rev::CMIS_4_0 => Ok(FOOTPRINT_RANGES_40),
        Rev::CMIS_5_3 => Ok(FOOTPRINT_RANGES),
        _ => Err(InputError::UnsupportedRevision(revision)),
    }
}

/// The typed non-qualified answer one classification reason maps to, in the
/// daemon's closed qualification vocabulary. Fixed severity order, so a
/// footprint with several restrictions keeps its most restrictive answer
/// deterministically.
fn typed_reason(classified: RangeClassified) -> Option<&'static str> {
    match classified {
        RangeClassified::Passive { .. } => None,
        RangeClassified::Uncovered => Some("uncovered"),
        RangeClassified::NonPassive { reason, .. } => Some(match reason {
            // A known read effect is the most restrictive answer.
            NonPassiveReason::ClearOnRead | NonPassiveReason::AcknowledgeStartupInterrupt => {
                "effectful"
            }
            // The effect is not resolved, or the marking conflicts.
            NonPassiveReason::ReadEffectUnknown | NonPassiveReason::Conflict => "unresolved",
            // The value a write-only byte delivers is unusable, but the read
            // itself has no effect: the answer is ambiguous, not effectful.
            NonPassiveReason::WriteOnly => "ambiguous",
            // No field metadata covers the byte at all.
            NonPassiveReason::MetadataAbsent => "uncovered",
        }),
    }
}

/// The severity of one non-qualified answer, for deterministic folding.
fn severity(qualification: &str) -> u8 {
    match qualification {
        "effectful" => 0,
        "ambiguous" => 1,
        "unresolved" => 2,
        _ => 3,
    }
}

/// Compute one footprint's qualification over its ranges: `qualified` only
/// when `classify_range` answers passive for every range, otherwise the most
/// restrictive typed non-qualified answer the ranges produced.
pub fn qualification_for_ranges(ranges: &[Range]) -> Result<&'static str, InputError> {
    qualification_for_ranges_for_revision(Rev::CMIS_5_3, ranges)
}

/// Compute qualification using only the actual revision's generated tables.
pub fn qualification_for_ranges_for_revision(
    revision: Rev,
    ranges: &[Range],
) -> Result<&'static str, InputError> {
    let planner = CmisReadPlanner::for_revision(revision)?;
    let mut refused: Option<&'static str> = None;
    for range in ranges {
        let classified = planner.classify_range(revision, *range)?;
        if let Some(reason) = typed_reason(classified) {
            refused = Some(match refused {
                Some(current) if severity(current) <= severity(reason) => current,
                _ => reason,
            });
        }
    }
    Ok(refused.unwrap_or("qualified"))
}

/// The computed qualification of a footprint the receipt carries: the
/// registry's answer over its ranges, or the marking identifier's constant
/// answer. A footprint the receipt does not carry is [`None`].
fn carried_qualification(
    revision: Rev,
    footprint: &str,
) -> Result<Option<&'static str>, InputError> {
    if footprint == C2_1_FOOTPRINT {
        return Ok(Some("unresolved"));
    }
    match footprint_ranges(revision)?
        .iter()
        .find(|(id, _)| *id == footprint)
    {
        Some((_, ranges)) => Ok(Some(qualification_for_ranges_for_revision(
            revision, ranges,
        )?)),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Grant emission: a granted plan may only read passive bytes of a qualified,
// carried footprint — everything else is a denial, never a fabricated grant.
// ---------------------------------------------------------------------------

/// One planned memory read of a read grant, naming the footprint its bytes
/// are qualified under.
#[derive(Debug, Clone, Copy)]
pub struct PlannedRead {
    /// The footprint whose qualification covers the read's bytes.
    pub footprint: &'static str,
    /// The byte window the read transfers.
    pub range: Range,
}

/// Why the generator refuses to emit a grant.
///
/// The library refuses at its own classification, before the daemon's
/// contract gate ever sees the receipt: a grant whose plan reads a byte the
/// library does not classify as passive is never emitted, and neither is one
/// naming a footprint the receipt does not carry or does not qualify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptRefusal {
    /// A grant needs at least one read and a nonzero finite transfer budget.
    InvalidReadGrant {
        /// The step whose grant was invalid.
        step: &'static str,
    },
    /// A passive read still must be inside the named carried footprint.
    OutsideFootprint {
        /// The step whose read fell outside the footprint.
        step: &'static str,
        /// The range that fell outside.
        range: Range,
        /// The footprint it fell outside of.
        footprint: &'static str,
    },
    /// The plan reads a byte the library does not classify as passive, for
    /// the typed reason (for the CMIS 5.3 identity window, the latched
    /// clear-on-read range between the passive bytes).
    NonPassivePlanRange {
        /// The step the refused grant named.
        step: &'static str,
        /// The byte window the plan would have read.
        range: Range,
        /// The first non-passive byte the plan reads.
        byte: u8,
        /// Why reading that byte is not passive.
        reason: NonPassiveReason,
    },
    /// The plan reads a byte window the library's tables do not cover at
    /// all, so its passivity is unproven.
    UncoveredPlanRange {
        /// The step the refused grant named.
        step: &'static str,
        /// The byte window the plan would have read.
        range: Range,
    },
    /// The plan names a footprint the receipt does not carry.
    UncarriedFootprint {
        /// The step the refused grant named.
        step: &'static str,
        /// The footprint the plan named.
        footprint: &'static str,
    },
    /// The plan names a footprint the library does not qualify.
    UnqualifiedFootprint {
        /// The step the refused grant named.
        step: &'static str,
        /// The footprint the plan named.
        footprint: &'static str,
        /// The footprint's computed qualification.
        qualification: &'static str,
    },
}

impl std::fmt::Display for ReceiptRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceiptRefusal::InvalidReadGrant { step } => write!(
                formatter,
                "step {step}: empty reads or zero transfer budget"
            ),
            ReceiptRefusal::OutsideFootprint {
                step,
                range,
                footprint,
            } => write!(formatter, "step {step}: {range:?} is outside {footprint}"),
            ReceiptRefusal::NonPassivePlanRange {
                step,
                range,
                byte,
                reason,
            } => write!(
                formatter,
                "step {step}: the planned read {range:?} covers byte {byte:#04x}, \
                 which the library does not classify as passive ({reason:?})"
            ),
            ReceiptRefusal::UncoveredPlanRange { step, range } => write!(
                formatter,
                "step {step}: the planned read {range:?} covers bytes the library's tables do \
                 not cover at this revision"
            ),
            ReceiptRefusal::UncarriedFootprint { step, footprint } => write!(
                formatter,
                "step {step}: the plan names footprint {footprint}, which the receipt \
                 does not carry"
            ),
            ReceiptRefusal::UnqualifiedFootprint {
                step,
                footprint,
                qualification,
            } => write!(
                formatter,
                "step {step}: the plan names footprint {footprint}, which the library \
                 does not qualify ({qualification})"
            ),
        }
    }
}

impl std::error::Error for ReceiptRefusal {}

/// Why the receipt as a whole cannot be emitted.
#[derive(Debug)]
pub enum ReceiptError {
    /// The library refused to classify a footprint range.
    Classification(InputError),
    /// A granted plan was refused ([`ReceiptRefusal`]).
    Grant(ReceiptRefusal),
    /// A delivered receipt was requested without a real review reference: the
    /// generator never emits `delivered: true` with a blank one, and a
    /// receipt without one is emitted as an explicitly undelivered draft
    /// ([`receipt_json`]).
    BlankReviewReference,
}

impl std::fmt::Display for ReceiptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceiptError::Classification(error) => {
                write!(
                    formatter,
                    "the library refused to classify a receipt range: {error:?}"
                )
            }
            ReceiptError::Grant(refusal) => {
                write!(formatter, "the library refused to emit a grant: {refusal}")
            }
            ReceiptError::BlankReviewReference => write!(
                formatter,
                "a delivered receipt requires a non-empty review reference; \
                 a receipt without one is emitted as an explicitly undelivered draft"
            ),
        }
    }
}

impl std::error::Error for ReceiptError {}

impl From<ReceiptRefusal> for ReceiptError {
    fn from(refusal: ReceiptRefusal) -> Self {
        ReceiptError::Grant(refusal)
    }
}

// ---------------------------------------------------------------------------
// The § 12 JSON shape: the daemon's closed member set, in its documented
// order, emitted deterministically.
// ---------------------------------------------------------------------------

/// One byte range of a footprint: the qualification covers exactly these
/// bytes, at this device address, bank, page, offset and length.
#[derive(Debug, Serialize)]
struct RangeRow {
    device_address: u8,
    bank: u8,
    page: u8,
    offset: u16,
    length: u16,
}

/// One row of the receipt's `footprints` member: the footprint's computed
/// qualification and the byte ranges that qualification covers (the
/// marking identifier [`C2_1_FOOTPRINT`] carries no ranges, so its `ranges`
/// member is omitted).
#[derive(Serialize)]
struct FootprintRow {
    footprint: &'static str,
    qualification: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ranges: Option<Vec<RangeRow>>,
}

/// One grant of the receipt's `grants` member: the accepted plan one
/// prerequisite step executes, with its finite transfer budget. Only
/// [`read_grant`] builds one, after validating every planned read.
#[derive(Debug, Serialize)]
pub struct Grant {
    /// The prerequisite step this grant covers.
    pub step: &'static str,
    /// The finite transfer budget of the step.
    pub transfers_per_generation: u32,
    /// The authority the accepted plan supplies.
    pub authority: &'static str,
    /// The accepted plan's steps, in order.
    pub steps: Vec<PlannedStep>,
}

/// One explicit denial in the receipt's `grants` member: the library's
/// recorded answer that a step is unsupported, with its non-empty reason and
/// no other member — a denial carries no footprint, no plan and no budget.
#[derive(Serialize)]
struct Denial {
    step: &'static str,
    denied: &'static str,
}

/// One entry of the `grants` member: a granted step or an explicit denial,
/// exactly as the daemon's parser admits either shape.
#[derive(Serialize)]
#[serde(untagged)]
enum GrantOrDenial {
    Grant(Grant),
    Denial(Denial),
}

/// One accepted plan step of a grant, exactly as the daemon's parser admits.
#[derive(Debug, Serialize)]
pub struct PlannedStep {
    /// The footprint whose qualification covers this step's bytes.
    pub footprint: &'static str,
    /// The effect the accepted plan supplies.
    pub effect: &'static str,
    /// The declared effect of the plan.
    pub declared: &'static str,
    /// The transfer this step issues.
    pub plan: Plan,
}

/// The transfer one accepted plan step names: exactly one of a read, a write
/// or a board control.
#[derive(Debug, Serialize)]
pub struct Plan {
    /// The module-memory read this step issues, when it issues one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<ReadPlan>,
    /// The module-memory write this step issues, when it issues one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write: Option<WritePlan>,
    /// The board control this step issues, when it issues one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<&'static str>,
}

/// A module-memory read at the address the accepted plan selected.
#[derive(Debug, Serialize)]
pub struct ReadPlan {
    /// The address the read starts from.
    pub address: PlanAddress,
    /// How many bytes the read transfers.
    pub length: u16,
}

/// A module-memory write at the address the accepted plan selected.
#[derive(Debug, Serialize)]
pub struct WritePlan {
    /// The address the write targets.
    pub address: PlanAddress,
    /// The payload the write delivers, as lower-case hex.
    pub payload_hex: &'static str,
}

/// The address one accepted plan selected; the daemon passes it through
/// unchanged.
#[derive(Debug, Serialize)]
pub struct PlanAddress {
    /// The module's management device address.
    pub device_address: u8,
    /// The bank the address names.
    pub bank: u8,
    /// The page the address names.
    pub page: u8,
    /// The first offset the address names.
    pub offset: u16,
}

const fn address(page: u8, offset: u16) -> PlanAddress {
    PlanAddress {
        device_address: 0x50,
        bank: 0,
        page,
        offset,
    }
}

/// Emit one read grant, refusing any plan the library does not qualify.
///
/// Every planned read must name a footprint the receipt carries and
/// qualifies, and its byte window must be entirely passive at this revision:
/// the generator never emits a grant whose plan reads a byte the library
/// does not classify as passive ([`ReceiptRefusal::NonPassivePlanRange`]) or
/// a window its tables do not cover ([`ReceiptRefusal::UncoveredPlanRange`]),
/// and never one naming an uncarried or unqualified footprint — a step the
/// library cannot support is denied explicitly instead, so this refusal is
/// the fail-closed answer, never a local reclassification.
pub fn read_grant(
    step: &'static str,
    transfers_per_generation: u32,
    reads: &[PlannedRead],
) -> Result<Grant, ReceiptError> {
    read_grant_for_revision(Rev::CMIS_5_3, step, transfers_per_generation, reads)
}

/// Emit a revision-bound read grant. This is only a memory-read plan, never a
/// full readiness/Power row. The caller supplies a finite nonzero transfer
/// budget; this does not establish a timing policy or admit board controls.
pub fn read_grant_for_revision(
    revision: Rev,
    step: &'static str,
    transfers_per_generation: u32,
    reads: &[PlannedRead],
) -> Result<Grant, ReceiptError> {
    let planner = CmisReadPlanner::for_revision(revision).map_err(ReceiptError::Classification)?;
    if transfers_per_generation == 0 || reads.is_empty() {
        return Err(ReceiptRefusal::InvalidReadGrant { step }.into());
    }
    let mut steps = Vec::with_capacity(reads.len());
    for planned in reads {
        let qualification = carried_qualification(revision, planned.footprint)
            .map_err(ReceiptError::Classification)?
            .ok_or(ReceiptError::Grant(ReceiptRefusal::UncarriedFootprint {
                step,
                footprint: planned.footprint,
            }))?;
        if qualification != "qualified" {
            return Err(ReceiptError::Grant(ReceiptRefusal::UnqualifiedFootprint {
                step,
                footprint: planned.footprint,
                qualification,
            }));
        }
        match planner
            .classify_range(revision, planned.range)
            .map_err(ReceiptError::Classification)?
        {
            RangeClassified::Passive { .. } => {}
            RangeClassified::Uncovered => {
                return Err(ReceiptError::Grant(ReceiptRefusal::UncoveredPlanRange {
                    step,
                    range: planned.range,
                }));
            }
            RangeClassified::NonPassive { byte, reason } => {
                return Err(ReceiptError::Grant(ReceiptRefusal::NonPassivePlanRange {
                    step,
                    range: planned.range,
                    byte,
                    reason,
                }));
            }
        }
        let ranges = footprint_ranges(revision).map_err(ReceiptError::Classification)?;
        let covered = ranges
            .iter()
            .find(|(id, _)| *id == planned.footprint)
            .is_some_and(|(_, ranges)| ranges.iter().any(|range| range.contains(planned.range)));
        if !covered {
            return Err(ReceiptRefusal::OutsideFootprint {
                step,
                range: planned.range,
                footprint: planned.footprint,
            }
            .into());
        }
        steps.push(PlannedStep {
            footprint: planned.footprint,
            effect: "memory",
            declared: "read",
            plan: Plan {
                read: Some(ReadPlan {
                    address: address(planned.range.page, u16::from(planned.range.offset)),
                    length: planned.range.len,
                }),
                write: None,
                control: None,
            },
        });
    }
    Ok(Grant {
        step,
        transfers_per_generation,
        authority: "privileged",
        steps,
    })
}

/// The whole § 12 member: the contract the daemon's admissions document
/// carries as its `prerequisite` member.
#[derive(Serialize)]
struct Receipt {
    kind: &'static str,
    version: &'static str,
    owner: &'static str,
    repository: &'static str,
    capability: &'static str,
    library_revision: String,
    connector_revision: String,
    /// The independent review reference, present only on a delivered receipt:
    /// a draft omits the member entirely instead of carrying a placeholder.
    #[serde(skip_serializing_if = "Option::is_none")]
    review_reference: Option<String>,
    /// True only when the receipt is delivered: a committed receipt is a
    /// draft (`false`) until it is regenerated from the landed revision
    /// with a real review reference.
    delivered: bool,
    footprints: Vec<FootprintRow>,
    grants: Vec<GrantOrDenial>,
}

/// Generate the prerequisite-I/O receipt for CMIS 5.3 at this revision, as
/// the pretty-printed § 12 JSON the daemon's admissions document carries as
/// its `prerequisite` member (with a trailing newline).
///
/// The revision pins are the consumer's compiled pins, supplied by the
/// caller and recorded verbatim. The qualifications are computed from the
/// library's own classification, never typed, and a granted plan may only
/// read bytes the library classifies as passive — classification of the
/// registry's ranges cannot fail at this revision, but a range the library
/// refuses to classify or a plan over non-passive bytes refuses the whole
/// receipt rather than emitting an unmarked answer.
///
/// The three steps the library does not support appear as explicit denials
/// carrying the library's reason, so the contract still names all five
/// prerequisite steps exactly once and the daemon's boundary refuses every
/// use of a denied step with its typed `StepDenied` refusal instead of
/// skipping it.
///
/// **A receipt without a review reference is a draft.** `review_reference:
/// None` emits `delivered: false` and omits the `review_reference` member
/// entirely — a committed receipt is such a draft, because a commit cannot
/// name its own OID: the delivered receipt is regenerated from the landed
/// revision (the HEAD of a clean checkout, checked by the
/// `cmis-prerequisite-receipt` bin), for the consumer's verified pins, with
/// the real review reference. A delivered receipt requires a non-empty
/// review reference: a blank one is refused
/// ([`ReceiptError::BlankReviewReference`]), never silently de-drafted.
pub fn receipt_json(
    library_revision: &str,
    connector_revision: &str,
    review_reference: Option<&str>,
) -> Result<String, ReceiptError> {
    receipt_json_for_revision(
        Rev::CMIS_5_3,
        library_revision,
        connector_revision,
        review_reference,
    )
}

/// Scoped prerequisite receipt for the actual CMIS 4.0 or 5.3 revision.
/// At 4.0, detection reads qualified bytes 0-2 and 126-127, identity
/// pre-read is separate, and readiness-bootstrap grants one byte-3 read per
/// generation. Further polling requires a separately bounded scoped grant,
/// not reuse beyond that budget. Hold/duration/full helper rows remain denied.
/// Draft/review and clean-HEAD obligations are identical to [`receipt_json`].
pub fn receipt_json_for_revision(
    revision: Rev,
    library_revision: &str,
    connector_revision: &str,
    review_reference: Option<&str>,
) -> Result<String, ReceiptError> {
    let ranges = footprint_ranges(revision).map_err(ReceiptError::Classification)?;
    let (review_reference, delivered) = match review_reference {
        Some(reference) if reference.trim().is_empty() => {
            return Err(ReceiptError::BlankReviewReference);
        }
        Some(reference) => (Some(reference.to_string()), true),
        None => (None, false),
    };
    // Compute every qualification before building the receipt, so a refused
    // range emits nothing at all.
    let footprints: Vec<FootprintRow> = ranges
        .iter()
        .map(|(id, ranges)| {
            Ok(FootprintRow {
                footprint: id,
                qualification: qualification_for_ranges_for_revision(revision, ranges)?,
                ranges: Some(
                    ranges
                        .iter()
                        .map(|range| RangeRow {
                            device_address: range.address,
                            bank: range.bank,
                            page: range.page,
                            offset: u16::from(range.offset),
                            length: range.len,
                        })
                        .collect(),
                ),
            })
        })
        .collect::<Result<_, InputError>>()
        .map_err(ReceiptError::Classification)?;
    let footprints = {
        let mut rows = footprints;
        rows.push(FootprintRow {
            footprint: C2_1_FOOTPRINT,
            qualification: "unresolved",
            ranges: None,
        });
        rows
    };
    let identity_footprint = if revision == Rev::CMIS_4_0 {
        CMIS4_IDENTITY_FOOTPRINT
    } else {
        IDENTITY_FOOTPRINT
    };
    let detection = GrantOrDenial::Grant(read_grant_for_revision(
        revision,
        "detection",
        4,
        &[
            PlannedRead {
                footprint: identity_footprint,
                range: window(0, 0, 3),
            },
            PlannedRead {
                footprint: identity_footprint,
                range: window(0, 126, 2),
            },
        ],
    )?);
    let pre_identification = read_grant_for_revision(
        revision,
        "pre-identification",
        4,
        &[PlannedRead {
            footprint: if revision == Rev::CMIS_4_0 {
                CMIS4_IDENTITY_FOOTPRINT
            } else {
                IDENTITY_FOOTPRINT
            },
            range: window(0, 129, 61),
        }],
    )?;
    let bootstrap = if revision == Rev::CMIS_4_0 {
        GrantOrDenial::Grant(read_grant_for_revision(
            revision,
            "readiness-bootstrap",
            1,
            &[PlannedRead {
                footprint: CMIS4_MODULE_STATE_FOOTPRINT,
                range: window(0, 3, 1),
            }],
        )?)
    } else {
        GrantOrDenial::Denial(Denial {
            step: "readiness-bootstrap",
            denied: READINESS_BOOTSTRAP_DENIED,
        })
    };
    let receipt = Receipt {
        kind: KIND,
        version: VERSION,
        owner: OWNER,
        repository: REPOSITORY,
        capability: CAPABILITY,
        library_revision: library_revision.to_string(),
        connector_revision: connector_revision.to_string(),
        review_reference,
        delivered,
        footprints,
        grants: vec![
            detection,
            GrantOrDenial::Grant(pre_identification),
            // Unsupported steps carry explicit denials with the library's
            // reason, never grants
            // naming unqualified footprints. The daemon admits each denial
            // and refuses every qualify and submit for the step with its
            // typed `StepDenied` refusal.
            GrantOrDenial::Denial(Denial {
                step: "startup-power-hold",
                denied: STARTUP_POWER_HOLD_DENIED,
            }),
            bootstrap,
            GrantOrDenial::Denial(Denial {
                step: "duration-acquisition",
                denied: DURATION_ACQUISITION_DENIED,
            }),
        ],
    };
    let mut json = serde_json::to_string_pretty(&receipt)
        .expect("the receipt's own types serialize; this is a programming error otherwise");
    json.push('\n');
    Ok(json)
}
