//! Caller-directed layout discovery and its pure supplied-input companion.
//! Neither path owns live management state. Supplied decoding is not permission
//! to read a device: in particular SFF-8636 byte 2 acknowledges startup.
use crate::{dispatch_identifier, SpecKind};
use openxcvr_core::{
    types::management::{
        supplied_bytes, ByteView, CacheEpoch, InputError, ModuleGeneration, Range,
    },
    Rev,
};
use openxcvr_transports::{
    bus::{Address7, I2cBus},
    paged::Layout,
    Error, ErrorKind, Result,
};
use std::time::Instant;

pub use openxcvr_core::types::ModuleIdentifier;

fn lower_range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

/// Decode only caller-supplied bytes, checking their generation/epoch first.
/// Failed, historical and absent observations never become zero-valued fields.
/// Generated field accessors consume only their supplied byte spans.
///
/// Any-version detection: the family routes from byte 0/1 and the revision
/// is detected from the family's own revision byte — CMIS Lower:1 (packed
/// BCD), SFF-8636 Lower:1 (Table 6-4 compliance code), SFF-8472 A0h byte 94
/// (Table 8-8 compliance code) — through the same fail-closed machinery as
/// [`crate::unified::detect`]. A truly unknown revision byte (unspecified,
/// legacy SFF-8436, unassigned code) is [`LayoutDiscoveryError::UnsupportedRevision`];
/// a caller-declared revision that disagrees with the detected one is
/// refused, never reconciled. Every admitted revision then decodes through
/// the family's rev-agnostic union view.
pub fn discover_layout_supplied(
    view: &impl ByteView,
    revision: Rev,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> std::result::Result<Layout, LayoutDiscoveryError> {
    let prefix = supplied_bytes(view, lower_range(0, 2), generation, epoch)?;
    let family = dispatch_identifier(prefix[0], prefix[1])
        .map_err(|_| LayoutDiscoveryError::UnknownIdentifier(prefix[0]))?;
    match family {
        SpecKind::Cmis => {
            let detected = Rev::from_major_byte_cmis(prefix[1])
                .ok_or(LayoutDiscoveryError::UnsupportedRevision(prefix[1]))?;
            if revision != detected {
                return Err(InputError::UnsupportedRevision(revision).into());
            }
            let capability = supplied_bytes(view, lower_range(2, 1), generation, epoch)?;
            Ok(Layout::Cmis {
                flat:
                    openxcvr_cmis::generated::union::CmisLowerMemory::decode_memory_model_supplied(
                        &capability,
                        2,
                    )?,
            })
        }
        SpecKind::Sff8636 => {
            // Table 6-4 compliance byte at Lower:1. Unspecified (0x00) and
            // legacy SFF-8436 codes fail closed; range codes 0x07/0x08 map to
            // their earliest provable baseline (`from_compliance_byte_sff8636`).
            let detected = Rev::from_compliance_byte_sff8636(prefix[1])
                .ok_or(LayoutDiscoveryError::UnsupportedRevision(prefix[1]))?;
            if revision != detected {
                return Err(InputError::UnsupportedRevision(revision).into());
            }
            let capability = supplied_bytes(view, lower_range(2, 1), generation, epoch)?;
            Ok(Layout::Sff8636 {
                flat: openxcvr_sff8636::generated::union::Sff8636LowerMemory::decode_flat_memory_supplied(
                    &capability,
                    2,
                )?,
            })
        }
        SpecKind::Sff8472 => {
            // Table 8-8 compliance code at A0h byte 94; unspecified (0x00)
            // fails closed rather than guessing a baseline.
            let compliance = supplied_bytes(view, lower_range(94, 1), generation, epoch)?;
            let detected = Rev::from_compliance_byte_sff8472(compliance[0])
                .ok_or(LayoutDiscoveryError::UnsupportedRevision(compliance[0]))?;
            if revision != detected {
                return Err(InputError::UnsupportedRevision(revision).into());
            }
            let options = supplied_bytes(view, lower_range(64, 1), generation, epoch)?;
            Ok(Layout::Sff8472 {
                paged_diagnostics:
                    openxcvr_sff8472::generated::union::PageA0hBaseMemory::decode_options_supplied(
                        &options, 64, 2,
                    )?,
            })
        }
    }
}

/// Why a supplied layout decode was refused.
#[derive(Debug)]
pub enum LayoutDiscoveryError {
    /// The observation view was stale, absent or from another
    /// generation/epoch.
    Input(InputError),
    /// A supplied byte span was too short or out of range for its field.
    Span(openxcvr_core::supplied::SpanError),
    /// Byte 0 named a family outside the SFF-8024 routing table.
    UnknownIdentifier(u8),
    /// The family's revision byte named a revision outside its known
    /// domain (unspecified, legacy or unassigned codes) — detected, never
    /// assumed.
    UnsupportedRevision(u8),
}

impl From<openxcvr_core::supplied::SpanError> for LayoutDiscoveryError {
    fn from(error: openxcvr_core::supplied::SpanError) -> Self {
        Self::Span(error)
    }
}

impl From<InputError> for LayoutDiscoveryError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

impl std::fmt::Display for LayoutDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => error.fmt(f),
            Self::Span(error) => error.fmt(f),
            Self::UnknownIdentifier(byte) => write!(f, "unknown module identifier: 0x{byte:02x}"),
            Self::UnsupportedRevision(byte) => {
                write!(f, "unsupported revision byte: 0x{byte:02x}")
            }
        }
    }
}

impl std::error::Error for LayoutDiscoveryError {
    /// Preserve the source chain: `Input` and `Span` delegates report their
    /// underlying error rather than flattening it into the display text.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Span(error) => Some(error),
            Self::UnknownIdentifier(_) | Self::UnsupportedRevision(_) => None,
        }
    }
}

/// Passive compatibility helper. Identity acquisition never covers byte 2.
/// SFF-8636 layout acquisition is refused before its startup acknowledgment.
/// CMIS 3.0 acquisition is conservatively refused until reserved-bit authority
/// is available. SFF-8472 reads only the first options byte (64), not reserved
/// portions of byte 65. Supplied decoding remains available without I/O.
pub fn discover_layout(bus: &mut impl I2cBus, deadline: Instant) -> Result<Layout> {
    let address = Address7::new(0x50)?;
    let mut prefix = [0; 2];
    bus.write_read(address, &[0], &mut prefix, deadline)?;
    let family = dispatch_identifier(prefix[0], prefix[1])
        .map_err(|e| Error::new(ErrorKind::Unsupported, e.to_string()))?;
    if family == SpecKind::Sff8472 {
        let mut options = [0];
        bus.write_read(address, &[64], &mut options, deadline)?;

        return Ok(Layout::Sff8472 {
            paged_diagnostics:
                openxcvr_sff8472::generated::union::PageA0hBaseMemory::decode_options_supplied(
                    &options, 64, 2,
                )
                .map_err(|e| Error::new(ErrorKind::Unsupported, e.to_string()))?,
        });
    }
    if family == SpecKind::Sff8636 {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "passive layout footprint is not fully qualified",
        ));
    }
    let rev = Rev::from_major_byte_cmis(prefix[1])
        .ok_or_else(|| Error::new(ErrorKind::Unsupported, "unsupported CMIS revision"))?;
    if rev == Rev::CMIS_3_0 {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "CMIS 3.0 reserved-bit authority is unresolved",
        ));
    }
    let mut capability = [0];
    bus.write_read(address, &[2], &mut capability, deadline)?;
    Ok(Layout::Cmis {
        flat: openxcvr_cmis::generated::union::CmisLowerMemory::decode_memory_model_supplied(
            &capability,
            2,
        )
        .map_err(|e| Error::new(ErrorKind::Unsupported, e.to_string()))?,
    })
}

/// Schema-derived CMIS 3.0 / 4.0 / 5.0 / 5.1 / 5.2 / 5.3 identity footprint, without I/O permission.
///
/// Resolves the identity field spans through each revision's matching generated
/// Page00 type authority (`openxcvr_cmis::generated::v3_0..v5_3::CmisPage00hAdministrative`),
/// rather than an unversioned flat union schema. Each admitted revision defines the identical
/// six source-proved identity ranges for static RO identification at 129–189, present in
/// both flat and paged modules: vendor name (129–144), OUI (145–147), vendor PN (148–163),
/// vendor rev (164–165), vendor SN (166–181), and date code (182–189). ASCII padding,
/// raw three-byte OUI and the four date subspans have the same meaning. Only these
/// fields are reused, not the rest of the administrative page.
///
/// Revision is a caller-supplied fact: this API does not read or verify lower memory byte 1
/// or grant permission for hardware I/O. See `crates/openxcvr/docs/cmis4-runtime-support.md`
/// for primary specification table citations across all six revisions. Other revisions
/// (e.g. 3.1, 4.1, 5.4) and other families remain refused rather than guessed compatible.
///
/// The layout also carries the SFF-8024 identifier byte (lower memory byte 0,
/// from the `sff8024_identifier` catalog row) so the decoded identity can
/// include it; see [`decode_cmis_identity_supplied`].
pub fn cmis_identity_layout(
    revision: Rev,
) -> std::result::Result<openxcvr_core::types::management::IdentityLayout, InputError> {
    use openxcvr_core::types::management::IdentityLayout;

    macro_rules! layout_for {
        ($page:ty) => {{
            let range = |(offset, len): (usize, usize)| lower_range(offset as u8, len as u16);
            IdentityLayout {
                revision,
                vendor_name: range(<$page>::VENDOR_NAME_RANGE),
                vendor_pn: range(<$page>::VENDOR_PN_RANGE),
                vendor_rev: range(<$page>::VENDOR_REV_RANGE),
                vendor_sn: range(<$page>::VENDOR_SN_RANGE),
                date_code: range((
                    <$page>::DATE_CODE_YEAR_RANGE.0,
                    <$page>::DATE_CODE_YEAR_RANGE.1
                        + <$page>::DATE_CODE_MONTH_RANGE.1
                        + <$page>::DATE_CODE_DAY_OF_MONTH_RANGE.1
                        + <$page>::DATE_CODE_LOT_CODE_RANGE.1,
                )),
                oui: range(<$page>::VENDOR_OUI_RANGE),
                identifier: match identifier_range(revision) {
                    Ok(identifier) => Some(identifier),
                    // The revision was admitted above, so a failed resolution is
                    // a generated-table defect — refused, never guessed.
                    Err(_) => return Err(InputError::UnsupportedRevision(revision)),
                },
            }
        }};
    }

    let layout = match revision {
        Rev::CMIS_3_0 => layout_for!(openxcvr_cmis::generated::v3_0::CmisPage00hAdministrative),
        Rev::CMIS_4_0 => layout_for!(openxcvr_cmis::generated::v4_0::CmisPage00hAdministrative),
        Rev::CMIS_5_0 => layout_for!(openxcvr_cmis::generated::v5_0::CmisPage00hAdministrative),
        Rev::CMIS_5_1 => layout_for!(openxcvr_cmis::generated::v5_1::CmisPage00hAdministrative),
        Rev::CMIS_5_2 => layout_for!(openxcvr_cmis::generated::v5_2::CmisPage00hAdministrative),
        Rev::CMIS_5_3 => layout_for!(openxcvr_cmis::generated::v5_3::CmisPage00hAdministrative),
        _ => return Err(InputError::UnsupportedRevision(revision)),
    };
    Ok(layout)
}

/// Decode a CMIS identity from supplied observations, without I/O or padding.
///
/// When the snapshot supplies the identifier byte (lower memory byte 0), the
/// decoded identity carries it as [`VendorInfo::identifier`](openxcvr_core::types::VendorInfo::identifier) with the meaning
/// the generated `Sff8024Identifier` enum gives the code; a snapshot without
/// byte 0 decodes exactly as before, with the identifier simply `None` (the
/// daemon acquires the layout's ranges field by field, and existing callers
/// must not gain a new hard requirement). This is not permission to acquire
/// the ranges on a device.
pub fn decode_cmis_identity_supplied(
    view: &impl ByteView,
    revision: Rev,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> std::result::Result<openxcvr_core::types::VendorInfo, InputError> {
    let layout = cmis_identity_layout(revision)?;
    let mut info =
        openxcvr_core::types::management::decode_identity(&layout, view, generation, epoch)?;
    if let Some(identifier_range) = layout.identifier {
        info.identifier =
            decode_identifier_at(&identifier_range, view, revision, generation, epoch)?;
    }
    Ok(info)
}

// ─── SFF-8024 module identifier, every cataloged family ────────────────

/// Where each family's identifier byte lives, by catalog page and field
/// name (the catalog supplies the location; there is no second offset
/// table). Byte 0 of the A0h / lower page for every family.
fn identifier_field(family: openxcvr_core::rev::Family) -> Option<(&'static str, &'static str)> {
    match family {
        openxcvr_core::rev::Family::Cmis => Some(("CmisLowerMemory", "sff8024_identifier")),
        openxcvr_core::rev::Family::Sff8636 => Some(("Sff8636LowerMemory", "sff8024_identifier")),
        openxcvr_core::rev::Family::Sff8472 => Some(("page_a0h_base_memory", "sff8024_identifier")),
        _ => None,
    }
}

/// The identifier byte's range, resolved through the family's catalog at
/// the given revision. Fails closed when the family has no catalog or the
/// row is absent at that revision.
fn identifier_range(revision: Rev) -> std::result::Result<Range, IdentifierError> {
    let (page, name) = identifier_field(revision.family)
        .ok_or(IdentifierError::UnsupportedFamily(revision.family))?;
    let catalog = crate::catalog::family_catalog(revision.family)
        .ok_or(IdentifierError::UnsupportedFamily(revision.family))?;
    if !catalog.supported_revs.contains(&revision) {
        return Err(IdentifierError::UnsupportedRevision { rev: revision });
    }
    let query = openxcvr_core::field_catalog::FieldQuery {
        pattern: name,
        lane: openxcvr_core::field_catalog::LaneSelection::Unspecified,
        page: Some(page),
        bank: 0,
    };
    match openxcvr_core::field_catalog::resolve(catalog, revision, &query) {
        Ok(mut entries) if entries.len() == 1 => Ok(entries.remove(0).range()),
        _ => Err(IdentifierError::IdentifierRowAbsent {
            page,
            name,
            rev: revision,
        }),
    }
}

/// Map one identifier byte to the decoded identifier through the family's
/// own generated `Sff8024Identifier` enum: the code, the enum's rendering
/// of it, and `known` false for codes the schema leaves unassigned — never
/// a guess.
fn decode_identifier_at(
    identifier_range: &Range,
    view: &impl ByteView,
    revision: Rev,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> std::result::Result<Option<openxcvr_core::types::ModuleIdentifier>, InputError> {
    let code = match supplied_bytes(view, *identifier_range, generation, epoch) {
        Ok(bytes) => bytes[0],
        // A snapshot without the identifier byte is not an error: the
        // identifier is additive to the identity decode.
        Err(InputError::InsufficientInput { .. }) => return Ok(None),
        Err(other) => return Err(other),
    };
    let (name, known) = match revision.family {
        openxcvr_core::rev::Family::Cmis => {
            use openxcvr_cmis::generated::Sff8024Identifier;
            let decoded = Sff8024Identifier::from(code);
            (
                decoded.name(),
                !matches!(decoded, Sff8024Identifier::Unknown(_)),
            )
        }
        openxcvr_core::rev::Family::Sff8636 => {
            use openxcvr_sff8636::generated::Sff8024Identifier;
            let decoded = Sff8024Identifier::from(code);
            (
                decoded.name(),
                !matches!(decoded, Sff8024Identifier::Unknown(_)),
            )
        }
        openxcvr_core::rev::Family::Sff8472 => {
            use openxcvr_sff8472::generated::Sff8024Identifier;
            let decoded = Sff8024Identifier::from(code);
            (
                decoded.name(),
                !matches!(decoded, Sff8024Identifier::Unknown(_)),
            )
        }
        // identifier_range already refused every other family.
        _ => unreachable!("identifier decode reached an uncataloged family"),
    };
    Ok(Some(openxcvr_core::types::ModuleIdentifier {
        code,
        name,
        known,
    }))
}

/// Why an identifier decode was refused.
#[derive(Debug, thiserror::Error)]
pub enum IdentifierError {
    /// The family has no identifier decode.
    #[error("unsupported family {0:?} for the identifier decode")]
    UnsupportedFamily(openxcvr_core::rev::Family),
    /// The revision has no identifier decode.
    #[error("unsupported revision {rev:?} for the identifier decode")]
    UnsupportedRevision {
        /// The revision that was refused.
        rev: Rev,
    },
    /// The identifier field's catalog row is absent at the revision.
    #[error("catalog row for {page}::{name} absent at {rev:?}")]
    IdentifierRowAbsent {
        /// The row's page id.
        page: &'static str,
        /// The row's field name.
        name: &'static str,
        /// The revision the row is absent at.
        rev: Rev,
    },
    /// The supplied view failed validation.
    #[error("invalid supplied view: {0}")]
    Input(InputError),
}

impl From<InputError> for IdentifierError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

/// Decode the SFF-8024 module identifier from supplied observations, for
/// any cataloged family and revision: byte 0 of the A0h / lower page,
/// located through the family's catalog row, with the code, the generated
/// enum's meaning and `known` false for unassigned codes. The revision's
/// family selects both the catalog row and the generated enum. This is a
/// decode of supplied bytes, not permission to acquire them on a device.
pub fn decode_identifier_supplied(
    view: &impl ByteView,
    revision: Rev,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> std::result::Result<openxcvr_core::types::ModuleIdentifier, IdentifierError> {
    let range = identifier_range(revision)?;
    decode_identifier_at(&range, view, revision, generation, epoch)?.ok_or(IdentifierError::Input(
        InputError::InsufficientInput { range },
    ))
}

// ─── Task 32 facade: ReadIntent footprints + per-requester poll plans ───────
//
// Pure planning over supplied observations: the poll plan names the exact
// physical footprint a readiness poll may read, who it runs for, and which
// effects it may cause — validated against the revision's actual generated
// classification before it is returned. Nothing here reads, schedules or
// acknowledges anything; the daemon owns every poll and its timing (the
// family crate's `CmisReadiness` reducers own the walk decisions).

use openxcvr_core::types::management::{
    NonPassiveReason, RangeClassified as PollRangeClassified, ReadIntent, ReadRequester,
};

/// The exact lower-memory footprint every CMIS readiness poll names: byte
/// 3 alone (ModuleState bits 1..3 and the interrupt flag bit 0), never a
/// widened span around it.
pub const CMIS_MODULE_STATE_OFFSET: u8 = 3;

/// Why a readiness poll intent was refused. Fail closed: an undelivered
/// footprint table or an unqualified byte never yields a guessed plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollIntentRefusal {
    /// The revision has no delivered read-classification footprint
    /// (CMIS 3.0's reserved-bit authority is unresolved; 5.0–5.2 and
    /// 5.4/0x54 tables are not delivered on this path): no admissible
    /// managed poll exists for it, and none is guessed.
    UnsupportedRevision(Rev),
    /// The footprint's own generated classification refuses the passive
    /// read: the byte and the typed reason name exactly what failed
    /// (clear-on-read, unknown effect, uncovered metadata, write-only or
    /// a conflicting marking). Decoding an already supplied byte stays
    /// available; permission to read it does not follow.
    NotQualified {
        /// The byte whose classification refused the passive read.
        byte: u8,
        /// The typed reason it failed.
        reason: NonPassiveReason,
    },
}

/// The per-requester CMIS readiness poll plan (task 32): the exact
/// byte-3 [`ReadIntent`] for the supplied requester, validated against
/// the revision's actual generated classification.
///
/// Every requester gets the identical physical footprint — byte 3 alone,
/// effect-free — with its own tag, so the observation's attribution stays
/// auditable: a passive inspection, the daemon's monitoring poll, an
/// initialization walk and an explicit client query all read the same one
/// byte and none of them acknowledges anything (CMIS byte 3 carries no
/// startup-acknowledgment semantics; SFF-8636 byte 2 is a different
/// family's contract, owned there). A revision whose footprint is not
/// delivered, or whose byte 3 is not fully qualified as effect-free,
/// returns the typed refusal instead of a plan.
pub fn cmis_readiness_poll_intent(
    revision: Rev,
    requester: ReadRequester,
) -> std::result::Result<ReadIntent, PollIntentRefusal> {
    let planner = openxcvr_cmis::read_planning::CmisReadPlanner::for_revision(revision)
        .map_err(|_| PollIntentRefusal::UnsupportedRevision(revision))?;
    let range = lower_range(CMIS_MODULE_STATE_OFFSET, 1);
    match planner.classify_range(revision, range) {
        Ok(PollRangeClassified::Passive { .. }) => Ok(ReadIntent {
            requester,
            exact_ranges: vec![range],
            allowed_effects: vec![openxcvr_core::types::management::ReadEffect::None],
        }),
        Ok(PollRangeClassified::NonPassive { byte, reason }) => {
            Err(PollIntentRefusal::NotQualified { byte, reason })
        }
        // The planner's footprint covers exactly the lower window this
        // range addresses; an uncovered answer here is a footprint defect,
        // refused as such rather than guessed around.
        Ok(PollRangeClassified::Uncovered) => Err(PollIntentRefusal::NotQualified {
            byte: CMIS_MODULE_STATE_OFFSET,
            reason: NonPassiveReason::MetadataAbsent,
        }),
        Err(_) => Err(PollIntentRefusal::UnsupportedRevision(revision)),
    }
}

#[cfg(test)]
mod poll_intent_tests {
    use super::*;
    use openxcvr_core::types::management::{ByteClassification, ChangeSource, ReadEffect};

    fn requesters() -> [ReadRequester; 4] {
        [
            ReadRequester::PassiveInspection,
            ReadRequester::Initialization,
            ReadRequester::Monitoring,
            ReadRequester::ExplicitClient,
        ]
    }

    #[test]
    fn every_requester_gets_the_exact_byte3_footprint_with_its_own_tag() {
        for requester in requesters() {
            let intent =
                cmis_readiness_poll_intent(Rev::CMIS_5_3, requester).expect("5.3 delivers");
            assert_eq!(intent.requester, requester);
            assert_eq!(
                intent.exact_ranges,
                vec![Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 3,
                    len: 1
                }]
            );
            // Effect-free only: a readiness poll never acknowledges.
            assert_eq!(intent.allowed_effects, vec![ReadEffect::None]);
        }
        // 4.0's own footprint qualifies its byte 3 as well.
        assert!(cmis_readiness_poll_intent(Rev::CMIS_4_0, ReadRequester::Monitoring).is_ok());
    }

    #[test]
    fn revisions_without_a_delivered_footprint_fail_closed() {
        for revision in [
            Rev::CMIS_3_0,
            Rev::CMIS_5_0,
            Rev::CMIS_5_1,
            Rev::CMIS_5_2,
            Rev::CMIS_5_4,
        ] {
            assert_eq!(
                cmis_readiness_poll_intent(revision, ReadRequester::Monitoring),
                Err(PollIntentRefusal::UnsupportedRevision(revision)),
                "{revision:?}"
            );
        }
    }

    #[test]
    fn an_unqualified_footprint_refuses_with_the_typed_reason() {
        // The real delivered tables qualify byte 3 for 4.0/5.3, so the
        // refusal path is exercised the way the shared classifications
        // themselves are: by feeding the planner's classification shape
        // synthetic unqualified bytes through the shared
        // passive_refusal() combinator the intent check consumes.
        let cor = ByteClassification::from_masks(
            0x01,
            0,
            0,
            0,
            &[ReadEffect::ClearOnRead],
            ChangeSource::Dynamic,
        );
        assert_eq!(cor.passive_refusal(), Some(NonPassiveReason::ClearOnRead));
        let unknown = ByteClassification::from_masks(0x01, 0x02, 0, 0, &[], ChangeSource::Dynamic);
        assert_eq!(
            unknown.passive_refusal(),
            Some(NonPassiveReason::ReadEffectUnknown)
        );
        let uncovered =
            ByteClassification::from_masks(0x01, 0, 0xfe, 0, &[], ChangeSource::Dynamic);
        assert_eq!(
            uncovered.passive_refusal(),
            Some(NonPassiveReason::MetadataAbsent)
        );
        // And the full byte qualified is exactly the admission condition.
        let qualified = ByteClassification::from_masks(
            0xff,
            0,
            0,
            0,
            &[ReadEffect::None],
            ChangeSource::Dynamic,
        );
        assert_eq!(qualified.passive_refusal(), None);
    }
}
