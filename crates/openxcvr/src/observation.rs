//! Basic observations decoded from a supplied byte snapshot: module
//! temperature, supply voltage and per-lane TX bias, TX power and RX power,
//! with typed values, units and explicit validity states. No I/O.
//!
//! Family dispatch selects the generated catalog table; the five monitor
//! fields are located through the catalog (never hard-coded offsets). The
//! revision actually used is proven against the snapshot itself: CMIS
//! against the lower-memory revision byte, and SFF-8636 / SFF-8472 against
//! the revision-compliance byte the module reports (SFF-8636 page 00h
//! byte 1; SFF-8472 A0h byte 94). A decode is admitted only where the
//! reported code establishes applicability to the schema revision —
//! SFF-8636 0Ah (Rev 2.12; SFF-8636 Rev 2.12 § 6.2.2 Table 6-4, PDF p. 33)
//! and SFF-8472 0Ah (Rev 12.5, which the 12.5a schema implements; SFF-8472
//! Rev 12.5a § 8.11 Table 8-8, PDF p. 35). Every other code — including 00h
//! "unspecified", earlier revisions, and reserved codes — yields an
//! explicit unsupported/undetermined-revision outcome with no value, on
//! this surface and on the single-field [`observe_field`] path alike.
//!
//! CMIS laser bias is scaled by the page-01h TxBiasCurrentScalingFactor
//! advertisement (00b x1, 01b x2, 10b x4, 11b reserved; every schema-covered
//! CMIS revision from 3.0 to 5.3 defines the same advertisement at page 01h
//! byte 160 bits 4:3). With the advertisement absent the scale is
//! undetermined — the value can be off by up to 4x — so no scaled value is
//! produced; a reserved code is explicitly invalid.
pub use openxcvr_core::observation::{
    optical_power_dbm, FieldObservation, FieldValue, ObservationState, ObserveOptions, OpticalDbm,
    OpticalDbmValue,
};
pub use openxcvr_core::observation::{AcquisitionContext, SelectorAcquisition};

use crate::catalog::{family_catalog, resolve, FieldEntry, FieldQuery, LaneSelection, Scaling};
use openxcvr_core::field_catalog::FieldPresence;
use openxcvr_core::observation::observe_field_with as observe_entry_with;
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::{
    supplied_bytes, ByteView, CacheEpoch, InputError, ModuleGeneration, Range, ReadEffect,
};

/// The revision compliance a snapshot reports: the raw code plus the
/// schema's meaning for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportedCompliance {
    /// The raw compliance code.
    pub code: u8,
    /// The schema's meaning for the code.
    pub meaning: &'static str,
}

/// One lane's monitor triple.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneObservation {
    /// The lane's 1-based index.
    pub lane: u16,
    /// The lane's transmit-bias monitor.
    pub tx_bias: FieldObservation,
    /// The lane's transmit-power monitor.
    pub tx_power: FieldObservation,
    /// The lane's receive-power monitor.
    pub rx_power: FieldObservation,
}

/// The basic observation set for one snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct BasicObservations {
    /// The decoder's schema revision (byte-proven for CMIS).
    pub revision: Rev,
    /// The revision compliance the snapshot reports (SFF-8636 page 00h
    /// byte 1; SFF-8472 A0h byte 94), with the schema's meaning for the
    /// code. `None` for CMIS (whose revision byte is already proven into
    /// `revision`) and when the compliance byte is not supplied.
    pub reported_compliance: Option<ReportedCompliance>,
    /// Case temperature.
    pub temperature: FieldObservation,
    /// Supply voltage.
    pub supply_voltage: FieldObservation,
    /// Per-lane monitor triples, in lane order.
    pub lanes: Vec<LaneObservation>,
}

/// Why a basic-observation decode failed.
#[derive(Debug, thiserror::Error)]
pub enum ObservationError {
    /// The family has no basic-observation decode.
    #[error("unsupported family {0:?} for basic observations")]
    UnsupportedFamily(Family),
    /// The revision has no basic-observation decode.
    #[error("unsupported revision {rev:?} for basic observations")]
    UnsupportedRevision {
        /// The revision that was refused.
        rev: Rev,
    },
    /// The snapshot's own revision byte disagrees with the requested one.
    #[error("snapshot revision byte 0x{byte:02x} disagrees with requested {requested:?}")]
    RevisionMismatch {
        /// The revision the decode requested.
        requested: Rev,
        /// The raw revision byte the snapshot supplied.
        byte: u8,
    },
    /// The snapshot does not supply the CMIS revision byte.
    #[error("the snapshot does not supply the CMIS revision byte (lower memory byte 1)")]
    MissingRevisionByte,
    /// A monitor field's catalog row is absent at the revision.
    #[error("catalog row for {page}::{name} absent at {rev:?}")]
    MonitorRowAbsent {
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

/// The five basic monitor fields per family, named by their schema page and
/// field names. The catalog supplies every location, width and scaling.
struct BasicMonitors {
    temperature: (&'static str, &'static str),
    supply_voltage: (&'static str, &'static str),
    tx_bias: (&'static str, &'static str),
    tx_power: (&'static str, &'static str),
    rx_power: (&'static str, &'static str),
}

const CMIS_MONITORS: BasicMonitors = BasicMonitors {
    temperature: ("CmisLowerMemory", "temp_mon_value"),
    supply_voltage: ("CmisLowerMemory", "vcc_mon_voltage"),
    tx_bias: ("CmisPage11hLaneStatus", "laser_bias_tx"),
    tx_power: ("CmisPage11hLaneStatus", "optical_power_tx"),
    rx_power: ("CmisPage11hLaneStatus", "optical_power_rx"),
};

const SFF8636_MONITORS: BasicMonitors = BasicMonitors {
    temperature: ("Sff8636LowerMemory", "temperature"),
    supply_voltage: ("Sff8636LowerMemory", "supply_voltage"),
    tx_bias: ("Sff8636LowerMemory", "tx_bias_lane"),
    tx_power: ("Sff8636LowerMemory", "tx_power_lane"),
    rx_power: ("Sff8636LowerMemory", "rx_power_lane"),
};

const SFF8472_MONITORS: BasicMonitors = BasicMonitors {
    temperature: ("page_a2h_diagnostics_base", "temperature"),
    supply_voltage: ("page_a2h_diagnostics_base", "supply_voltage"),
    tx_bias: ("page_a2h_diagnostics_base", "tx_bias"),
    tx_power: ("page_a2h_diagnostics_base", "tx_power"),
    rx_power: ("page_a2h_diagnostics_base", "rx_power"),
};

fn monitors(family: Family) -> Option<&'static BasicMonitors> {
    match family {
        Family::Cmis => Some(&CMIS_MONITORS),
        Family::Sff8636 => Some(&SFF8636_MONITORS),
        Family::Sff8472 => Some(&SFF8472_MONITORS),
        Family::CCmis | Family::Elsfp => None,
    }
}

fn monitor_entry(
    revision: Rev,
    page: &'static str,
    name: &'static str,
) -> Result<openxcvr_core::field_catalog::FieldEntry<'static>, ObservationError> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::Unspecified,
        page: Some(page),
        bank: 0,
    };
    match resolve(revision, &query) {
        Ok(mut entries) if entries.len() == 1 => Ok(entries.remove(0)),
        _ => Err(ObservationError::MonitorRowAbsent {
            page,
            name,
            rev: revision,
        }),
    }
}

// ─── Revision compliance reported by SFF snapshots ──────────────────────
//
// The SFF families do not carry a plain revision number: the module
// reports a compliance code whose meaning is "includes functionality
// described in revision X of the standard". The tables below mirror the
// schema enums `Sff8636RevisionCompliance` and `Sff8472ComplianceCode`
// (meanings verbatim), which cite SFF-8636 Rev 2.12 § 6.2.2 Table 6-4
// (PDF p. 33) and SFF-8472 Rev 12.5a § 8.11 Table 8-8 (PDF p. 35). They
// must move with those enums. `reports` is the single family revision the
// code names; codes naming several revisions, SFF-8436 codes, unspecified
// and reserved codes name none. A decode is admitted only where the
// reported code names exactly the schema revision the catalog decodes.

struct ComplianceCode {
    code: u8,
    meaning: &'static str,
    reports: Option<Rev>,
}

const SFF8636_COMPLIANCE: &[ComplianceCode] = &[
    ComplianceCode {
        code: 0x00,
        meaning: "Revision not specified; not used for SFF-8636 rev 2.5 or higher",
        reports: None,
    },
    ComplianceCode {
        code: 0x01,
        meaning: "SFF-8436 Rev 4.8 or earlier",
        reports: None,
    },
    ComplianceCode {
        code: 0x02,
        meaning: "SFF-8436 Rev 4.8 or earlier except this byte and bytes 186-189 follow SFF-8636",
        reports: None,
    },
    ComplianceCode {
        code: 0x03,
        meaning: "SFF-8636 Rev 1.3 or earlier",
        reports: Some(Rev {
            family: Family::Sff8636,
            major: 1,
            minor: 3,
        }),
    },
    ComplianceCode {
        code: 0x04,
        meaning: "SFF-8636 Rev 1.4",
        reports: Some(Rev {
            family: Family::Sff8636,
            major: 1,
            minor: 4,
        }),
    },
    ComplianceCode {
        code: 0x05,
        meaning: "SFF-8636 Rev 1.5",
        reports: Some(Rev {
            family: Family::Sff8636,
            major: 1,
            minor: 5,
        }),
    },
    ComplianceCode {
        code: 0x06,
        meaning: "SFF-8636 Rev 2.0",
        reports: Some(Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 0,
        }),
    },
    // 07h and 08h each name several revisions (2.5-2.7, 2.8-2.10): none of
    // them is the single revision the schema decodes.
    ComplianceCode {
        code: 0x07,
        meaning: "SFF-8636 Rev 2.5, 2.6, and 2.7",
        reports: None,
    },
    ComplianceCode {
        code: 0x08,
        meaning: "SFF-8636 Rev 2.8, 2.9, and 2.10",
        reports: None,
    },
    // 09h names Rev 2.11: a named code in the schema enum, but not a
    // revision this schema decodes.
    ComplianceCode {
        code: 0x09,
        meaning: "SFF-8636 Rev 2.11",
        reports: Some(Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 11,
        }),
    },
    // 0Ah names Rev 2.12, the revision this schema decodes.
    ComplianceCode {
        code: 0x0a,
        meaning: "SFF-8636 Rev 2.12",
        reports: Some(Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        }),
    },
];

const SFF8472_COMPLIANCE: &[ComplianceCode] = &[
    ComplianceCode {
        code: 0x00,
        meaning: "SFF-8472 compliance not specified.",
        reports: None,
    },
    ComplianceCode {
        code: 0x01,
        meaning: "Complies with SFF-8472 Rev 9.3.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 9,
            minor: 3,
        }),
    },
    ComplianceCode {
        code: 0x02,
        meaning: "Complies with SFF-8472 Rev 9.5.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 9,
            minor: 5,
        }),
    },
    ComplianceCode {
        code: 0x03,
        meaning: "Complies with SFF-8472 Rev 10.2.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 10,
            minor: 2,
        }),
    },
    ComplianceCode {
        code: 0x04,
        meaning: "Complies with SFF-8472 Rev 10.4.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 10,
            minor: 4,
        }),
    },
    ComplianceCode {
        code: 0x05,
        meaning: "Complies with SFF-8472 Rev 11.0.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 11,
            minor: 0,
        }),
    },
    ComplianceCode {
        code: 0x06,
        meaning: "Complies with SFF-8472 Rev 11.3.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 11,
            minor: 3,
        }),
    },
    ComplianceCode {
        code: 0x07,
        meaning: "Complies with SFF-8472 Rev 11.4.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 11,
            minor: 4,
        }),
    },
    ComplianceCode {
        code: 0x08,
        meaning: "Complies with SFF-8472 Rev 12.3.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 3,
        }),
    },
    ComplianceCode {
        code: 0x09,
        meaning: "Complies with SFF-8472 Rev 12.4.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 4,
        }),
    },
    ComplianceCode {
        code: 0x0a,
        meaning: "Complies with SFF-8472 Rev 12.5.",
        reports: Some(Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        }),
    },
];

/// Codes above the last named entry are the enums' reserved range.
const RESERVED_MEANING: &str = "Reserved";

/// The compliance code a snapshot reports, decoded against the family's
/// table. Codes outside the table fall into the reserved range.
fn reported_compliance_for(family: Family, code: u8) -> ReportedCompliance {
    let table = match family {
        Family::Sff8636 => SFF8636_COMPLIANCE,
        Family::Sff8472 => SFF8472_COMPLIANCE,
        _ => unreachable!("no compliance table for {family:?}"),
    };
    match table.iter().find(|entry| entry.code == code) {
        Some(entry) => ReportedCompliance {
            code,
            meaning: entry.meaning,
        },
        None => ReportedCompliance {
            code,
            meaning: RESERVED_MEANING,
        },
    }
}

/// Whether a reported compliance code establishes applicability to the
/// given (schema) revision: the code must name exactly that revision.
fn compliance_admits(family: Family, code: u8, revision: Rev) -> bool {
    let table = match family {
        Family::Sff8636 => SFF8636_COMPLIANCE,
        Family::Sff8472 => SFF8472_COMPLIANCE,
        _ => return false,
    };
    table
        .iter()
        .find(|entry| entry.code == code)
        .is_some_and(|entry| entry.reports == Some(revision))
}

/// Where each SFF family's revision-compliance byte lives, by catalog page
/// and field name (the catalog supplies the location).
fn compliance_field(family: Family) -> Option<(&'static str, &'static str)> {
    match family {
        Family::Sff8636 => Some(("Sff8636LowerMemory", "revision_compliance")),
        Family::Sff8472 => Some(("page_a0h_base_memory", "sff8472_compliance")),
        _ => None,
    }
}

const COMPLIANCE_BYTE_NOT_SUPPLIED: &str =
    "revision compliance byte not supplied; module revision undetermined";

/// The revision compliance an SFF snapshot reports, or `None` when the
/// compliance byte is not supplied. Other read errors propagate.
fn read_reported_compliance(
    revision: Rev,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<Option<ReportedCompliance>, ObservationError> {
    let Some((page, name)) = compliance_field(revision.family) else {
        return Ok(None);
    };
    let entry = monitor_entry(revision, page, name)?;
    match supplied_bytes(view, entry.range(), generation, epoch) {
        Ok(bytes) => Ok(Some(reported_compliance_for(revision.family, bytes[0]))),
        Err(InputError::InsufficientInput { .. }) => Ok(None),
        Err(other) => Err(ObservationError::Input(other)),
    }
}

/// The SFF revision gate for one resolved field: `Some(refusal)` when the
/// snapshot's compliance byte does not admit a decode at the entry's
/// revision, `None` when it does (or the family has no gate).
fn sff_compliance_refusal(
    entry: &FieldEntry<'_>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Option<FieldObservation> {
    compliance_field(entry.rev.family)?;
    let mut observation = FieldObservation {
        field: entry.row.name,
        lane: entry.lane,
        state: ObservationState::UnsupportedRevision(entry.rev),
        value: None,
        note: None,
    };
    match read_reported_compliance(entry.rev, view, generation, epoch) {
        Ok(Some(reported)) => {
            if compliance_admits(entry.rev.family, reported.code, entry.rev) {
                return None;
            }
            observation.note = Some(reported.meaning);
            Some(observation)
        }
        Ok(None) => {
            observation.note = Some(COMPLIANCE_BYTE_NOT_SUPPLIED);
            Some(observation)
        }
        // A view that cannot be read at the compliance location is a view
        // problem, not a compliance verdict: fail closed like the core
        // decoder's own read errors.
        Err(_) => {
            observation.state = ObservationState::Invalid;
            observation.note = Some("revision compliance byte could not be read");
            Some(observation)
        }
    }
}

// ─── CMIS laser-bias scale advertisement ────────────────────────────────
//
// Every CMIS revision the schemas cover (3.0 through 5.3) defines the same
// advertisement (page 01h byte 160 bits 4:3: 00b x1, 01b x2, 10b x4, 11b
// reserved — CMIS 3.0 Table 45, CMIS 4.0 Table 8-42, CMIS 5.0/5.1
// Table 8-55, CMIS 5.2 Table 8-56, CMIS 5.3 Table 8-63) and scales the
// LaserBiasTx monitor by it ("unsigned integer in 2 uA increments, times
// the multiplier"). No covered revision defines a fixed bias scale without
// this advertisement.

const BIAS_ADVERTISEMENT_NOTE: &str =
    "TxBiasCurrentScalingFactor advertisement not supplied; laser bias scale undetermined";
const BIAS_RESERVED_NOTE: &str = "reserved TxBiasCurrentScalingFactor code";

/// Whether a row's scaling is qualified by the TxBiasCurrentScalingFactor
/// advertisement (the schema's `lsb_units` caveat, generated onto the row).
fn scaling_needs_bias_advertisement(entry: &FieldEntry<'_>) -> bool {
    matches!(
        &entry.row.scaling,
        Scaling::Scaled(lsb)
            if lsb.note.is_some_and(|note| note.contains("TxBiasCurrentScalingFactor"))
    )
}

/// The advertised multiplier for a CMIS laser-bias field: `Ok(None)` when
/// the advertisement is not supplied, `Ok(Some(factor))` for x1/x2/x4, and
/// an invalid marker for the reserved code.
fn bias_advertisement(
    revision: Rev,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<Option<f64>, ()> {
    let entry = match monitor_entry(
        revision,
        "CmisPage01hAdvertising",
        "tx_bias_current_scaling_factor",
    ) {
        Ok(entry) => entry,
        // No advertisement row: treat like an absent advertisement —
        // the scale is undetermined, never guessed.
        Err(_) => return Ok(None),
    };
    match supplied_bytes(view, entry.range(), generation, epoch) {
        Ok(bytes) => match (bytes[0] >> 3) & 0x03 {
            0 => Ok(Some(1.0)),
            1 => Ok(Some(2.0)),
            2 => Ok(Some(4.0)),
            _ => Err(()),
        },
        Err(InputError::InsufficientInput { .. }) => Ok(None),
        Err(_) => Err(()),
    }
}

/// Apply the laser-bias scale advertisement to one observation. Only a
/// value-producing observation is adjusted: reserved → invalid with no
/// value; advertisement absent → no scaled value (the not-supplied
/// representation); x1/x2/x4 → the correctly scaled value.
fn apply_bias_advertisement(
    entry: &FieldEntry<'_>,
    observation: &mut FieldObservation,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) {
    if !scaling_needs_bias_advertisement(entry) || observation.state != ObservationState::Valid {
        return;
    }
    match bias_advertisement(entry.rev, view, generation, epoch) {
        Ok(Some(factor)) => {
            if let Some(FieldValue::Scaled { value, .. }) = observation.value.as_mut() {
                *value *= factor;
            }
            observation.note = None;
        }
        Ok(None) => {
            observation.state = ObservationState::NotSupplied;
            observation.value = None;
            observation.note = Some(BIAS_ADVERTISEMENT_NOTE);
        }
        Err(()) => {
            observation.state = ObservationState::Invalid;
            observation.value = None;
            observation.note = Some(BIAS_RESERVED_NOTE);
        }
    }
}

// ─── CMIS revision proof ────────────────────────────────────────────────

/// CMIS: prove the requested revision against the snapshot's own revision
/// byte (lower memory byte 1). The decode never assumes a revision the
/// snapshot contradicts.
fn verify_cmis_revision(
    revision: Rev,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<(), ObservationError> {
    let entry = monitor_entry(revision, "CmisLowerMemory", "cmis_revision")?;
    match supplied_bytes(view, entry.range(), generation, epoch) {
        Ok(bytes) => {
            let byte = bytes[0];
            match Rev::from_major_byte_cmis(byte) {
                None => Err(ObservationError::UnsupportedRevision {
                    rev: Rev {
                        family: Family::Cmis,
                        major: byte >> 4,
                        minor: byte & 0x0f,
                    },
                }),
                Some(detected) if detected != revision => Err(ObservationError::RevisionMismatch {
                    requested: revision,
                    byte,
                }),
                Some(_) => Ok(()),
            }
        }
        Err(InputError::InsufficientInput { .. }) => Err(ObservationError::MissingRevisionByte),
        Err(other) => Err(ObservationError::Input(other)),
    }
}

// ─── Public decode entry points ─────────────────────────────────────────

/// The write-only refusal for one resolved entry: the entry's bytes are
/// classified against every catalog row overlapping them at the entry's
/// revision — exactly the classification the field read plan applies —
/// and a write-only byte (the entry's own row or a byte-sharing
/// neighbour's) refuses the decode: a read of a write-only location
/// returns undefined or vendor-specific bytes, so a decoded value would
/// report a guess as a fact. `None` when no write-only byte is involved
/// or the family has no catalog.
fn write_only_refusal(entry: &FieldEntry<'_>) -> Option<FieldObservation> {
    let catalog = family_catalog(entry.rev.family)?;
    (restrictive_effect(catalog, entry.rev, entry.range())
        == Some(FieldPlanRefusalReason::WriteOnly))
    .then_some(FieldObservation {
        field: entry.row.name,
        lane: entry.lane,
        state: ObservationState::WriteOnlyRefused,
        value: None,
        note: None,
    })
}

/// Decode one resolved field from the supplied view with caller-supplied
/// acquisition evidence. The same family rules as [`observe_field`] apply;
/// [`SelectorAcquisition`] documents what the caller asserts (settled,
/// selector-stable acquisition) and what the library checks (a matching
/// assertion covering the row's bytes, agreeing with the snapshot's
/// selector byte and selecting the row's view). Content-multiplexed rows
/// without matching evidence are [`ObservationState::NotSupplied`].
pub fn observe_field_with(
    entry: &FieldEntry<'_>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    options: &ObserveOptions,
    context: &AcquisitionContext<'_>,
) -> FieldObservation {
    if let Some(refused) = sff_compliance_refusal(entry, view, generation, epoch) {
        return refused;
    }
    if let Some(refused) = write_only_refusal(entry) {
        return refused;
    }
    let mut observation = observe_entry_with(entry, view, generation, epoch, options, context);
    apply_bias_advertisement(entry, &mut observation, view, generation, epoch);
    observation
}

/// Decode one resolved field from the supplied view.
///
/// The same family rules as [`observe_basic`] apply, so the generic field
/// path cannot bypass them: SFF-8636 / SFF-8472 decodes are admitted only
/// where the snapshot's revision-compliance byte establishes applicability
/// to the schema revision (otherwise [`ObservationState::UnsupportedRevision`]
/// with no value), and CMIS laser-bias fields apply the
/// TxBiasCurrentScalingFactor advertisement (an absent advertisement yields
/// no scaled value; a reserved code is invalid).
///
/// Stage 1 admission applies before any value: the snapshot's reported CMIS
/// revision (lower memory byte 1), presence prerequisites, and — for
/// selector-multiplexed rows such as Page 14h diagnostics — acquisition
/// evidence. This entry point behaves exactly as
/// [`observe_field_with`] with an empty context: content-multiplexed rows
/// decode only through the context-taking variants.
///
/// Write-only fields — and fields sharing a byte with a write-only row —
/// are [`ObservationState::WriteOnlyRefused`] with no value: a write-only
/// read returns undefined bytes, never a fact. Clear-on-read fields are
/// refused unless `options` authorizes them.
/// Missing bytes are [`ObservationState::NotSupplied`]; view-level problems
/// (overlapping observations, stale stamps) are
/// [`ObservationState::Invalid`]; nothing is ever zero-filled.
pub fn observe_field(
    entry: &FieldEntry<'_>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    options: &ObserveOptions,
) -> FieldObservation {
    observe_field_with(
        entry,
        view,
        generation,
        epoch,
        options,
        &AcquisitionContext::empty(),
    )
}

/// Decode the basic observation set from a supplied snapshot.
///
/// Per-field outcomes are explicit: an absent optional page leaves the lane
/// monitors [`ObservationState::NotSupplied`]; missing external-calibration
/// constants are [`ObservationState::Uncalibrated`]; no decode error is
/// ever converted to zero. A snapshot whose reported compliance does not
/// establish the schema revision decodes nothing: every field observation
/// is [`ObservationState::UnsupportedRevision`] with no value, and the
/// result still reports what the module said (see
/// [`BasicObservations::reported_compliance`]).
pub fn observe_basic(
    family: Family,
    revision: Rev,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    options: ObserveOptions,
) -> Result<BasicObservations, ObservationError> {
    let catalog = family_catalog(family).ok_or(ObservationError::UnsupportedFamily(family))?;
    if !catalog.supported_revs.contains(&revision) {
        return Err(ObservationError::UnsupportedRevision { rev: revision });
    }
    if family == Family::Cmis {
        verify_cmis_revision(revision, view, generation, epoch)?;
    }
    let monitors = monitors(family).ok_or(ObservationError::UnsupportedFamily(family))?;

    // What the module reports, for the result — read failures here leave
    // nothing reported; the per-field gate decides the decode outcomes.
    let reported_compliance =
        read_reported_compliance(revision, view, generation, epoch).unwrap_or_default();

    let temperature_entry =
        monitor_entry(revision, monitors.temperature.0, monitors.temperature.1)?;
    let supply_entry = monitor_entry(
        revision,
        monitors.supply_voltage.0,
        monitors.supply_voltage.1,
    )?;
    let tx_bias_entry = monitor_entry(revision, monitors.tx_bias.0, monitors.tx_bias.1)?;
    let tx_power_entry = monitor_entry(revision, monitors.tx_power.0, monitors.tx_power.1)?;
    let rx_power_entry = monitor_entry(revision, monitors.rx_power.0, monitors.rx_power.1)?;

    let temperature = observe_field(&temperature_entry, view, generation, epoch, &options);
    let supply_voltage = observe_field(&supply_entry, view, generation, epoch, &options);

    // Module-scope monitors stay module-scope; the lane triple enumerates
    // the row's own lane dimension (SFF-8472 has a single module-scope
    // monitor serving its one lane).
    let lane_numbers: Vec<Option<u16>> = match &tx_bias_entry.row.instances {
        Some(dimension) => (dimension.first..=dimension.last).map(Some).collect(),
        None => vec![None],
    };
    let mut lanes = Vec::with_capacity(lane_numbers.len());
    for lane in lane_numbers {
        let select = |entry: &openxcvr_core::field_catalog::FieldEntry<'static>| {
            let query = FieldQuery {
                pattern: entry.row.name,
                lane: lane
                    .map(LaneSelection::One)
                    .unwrap_or(LaneSelection::Unspecified),
                page: Some(entry.row.page_id),
                bank: 0,
            };
            resolve(revision, &query)
        };
        let (bias, tx, rx) = (
            select(&tx_bias_entry),
            select(&tx_power_entry),
            select(&rx_power_entry),
        );
        let (bias_entry, tx_entry, rx_entry) = match (bias, tx, rx) {
            (Ok(mut b), Ok(mut t), Ok(mut r)) if b.len() == 1 && t.len() == 1 && r.len() == 1 => {
                (b.remove(0), t.remove(0), r.remove(0))
            }
            _ => {
                return Err(ObservationError::MonitorRowAbsent {
                    page: tx_bias_entry.row.page_id,
                    name: tx_bias_entry.row.name,
                    rev: revision,
                })
            }
        };
        let bias = observe_field(&bias_entry, view, generation, epoch, &options);
        let tx = observe_field(&tx_entry, view, generation, epoch, &options);
        let rx = observe_field(&rx_entry, view, generation, epoch, &options);
        lanes.push(LaneObservation {
            lane: lane.unwrap_or(1),
            tx_bias: bias,
            tx_power: tx,
            rx_power: rx,
        });
    }
    Ok(BasicObservations {
        revision,
        reported_compliance,
        temperature,
        supply_voltage,
        lanes,
    })
}

// ─── Basic observation read plan ──────────────────────────────────────
//
// `observe_basic` decodes from a supplied view; nothing tells a caller
// which bytes to acquire. This section is that answer, as a library-owned
// plan: the exact ranges the decode reads, derived from the same catalog
// rows and monitor entries `observe_basic` uses — never from a second
// hand-written offset table. Two stages, because what the decode reads
// depends on bytes that are only known after reading:
//
//   1. [`basic_read_prelude`]: the fixed per-family gate ranges (CMIS
//      revision byte; SFF compliance byte; the SFF-8472 external-
//      calibration discriminator byte), all from catalog rows.
//   2. [`basic_read_remaining`]: takes the prelude bytes through any
//      [`ByteView`] and returns the monitor ranges the decode would read
//      — the five monitors' instance spans, the CMIS bias advertisement,
//      and the SFF-8472 external-calibration constants only when the
//      discriminator says external. It refuses when the prelude is
//      missing, unsupported, reserved, or names a revision other than the
//      one requested, mirroring the gates `observe_basic` applies.
//
// A caller that acquires the prelude, then the returned ranges, and feeds
// everything to `observe_basic` gets exactly the result a full snapshot
// would give (the tests prove this by recording every byte the decoders
// touch). Flat-memory modules make upper pages optional, not failures: a
// module reporting flat memory implements only page 00h, so the plan marks
// every upper-page range optional — the decode yields `NotSupplied`, the
// same outcome `observe_basic` produces for an absent optional page.
//
// Clear-on-read bytes are never planned: every range comes from a monitor,
// gate or constant row (or a discriminator location), and the tests prove
// for every family and supported revision that no planned range covers a
// row carrying [`openxcvr_core::types::management::ReadEffect::ClearOnRead`]
// (the CMIS lower-page flag bytes 8..=11 among them).

/// One planned acquisition: the exact byte range, whether absence is an
/// expected decode outcome rather than an acquisition failure, and the
/// catalog field these bytes serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedRead {
    /// The exact byte range to acquire.
    pub range: Range,
    /// A missing optional range is an expected `NotSupplied` outcome in
    /// the decode, not an acquisition failure. Optional ranges are exactly
    /// the upper-page ones (page != 0): a flat-memory module implements
    /// only page 00h — CMIS `memory_model` (lower memory byte 2 bit 7,
    /// 1b = "Flat memory (Page 00h supported only)") and SFF-8636
    /// `flat_memory` (byte 2 bit 2, 1b = "Flat memory; lower memory and
    /// upper page 00h only"). The SFF families' basic-set bytes all live
    /// in always-present windows (SFF-8636 lower page; SFF-8472 A0h/A2h).
    pub optional: bool,
    /// The field these bytes serve: the catalog row's effective name for
    /// monitor, gate and constant ranges, and the discriminator's schema
    /// field path for discriminator ranges.
    pub purpose: &'static str,
}

impl PlannedRead {
    /// A range in an upper page (page != 0) is optional: flat-memory
    /// modules do not implement it (see the field documentation).
    fn new(range: Range, purpose: &'static str) -> Self {
        Self {
            range,
            optional: range.page != 0,
            purpose,
        }
    }
}

/// Why a basic-observation read plan was refused.
/// Why a basic-observation read plan was refused.
#[derive(Debug, thiserror::Error)]
pub enum BasicPlanError {
    /// The family has no basic-observation plan.
    #[error("unsupported family {0:?} for the basic observation read plan")]
    UnsupportedFamily(Family),
    /// The revision has no basic-observation plan.
    #[error("unsupported revision {rev:?} for the basic observation read plan")]
    UnsupportedRevision {
        /// The revision that was refused.
        rev: Rev,
    },
    /// The snapshot's own revision byte disagrees with the requested one.
    #[error("snapshot revision byte 0x{byte:02x} disagrees with requested {requested:?}")]
    RevisionMismatch {
        /// The revision the plan requested.
        requested: Rev,
        /// The raw revision byte the snapshot supplied.
        byte: u8,
    },
    /// A prelude byte the plan needs is absent from the view.
    #[error("the prelude byte at {range:?} is not supplied")]
    PreludeMissing {
        /// The missing prelude range.
        range: Range,
    },
    /// The reported compliance code does not admit a decode at the schema
    /// revision.
    #[error("reported compliance code 0x{code:02x} ({meaning}) does not admit a decode at the schema revision")]
    ComplianceRefused {
        /// The family whose compliance byte refused the decode.
        family: Family,
        /// The raw compliance code.
        code: u8,
        /// The schema's meaning for the code.
        meaning: &'static str,
    },
    /// A monitor field's catalog row is absent at the revision.
    #[error("catalog row for {page}::{name} absent at {rev:?}")]
    MonitorRowAbsent {
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

/// The five monitor names of one family, in decode order.
fn monitor_names(family: Family) -> Option<[(&'static str, &'static str); 5]> {
    monitors(family).map(|m| {
        [
            m.temperature,
            m.supply_voltage,
            m.tx_bias,
            m.tx_power,
            m.rx_power,
        ]
    })
}

/// The enclosing span of one field's instance dimension: the exact bytes
/// the field's every instance occupies (a plain field is its own span).
fn entry_span(entry: &FieldEntry<'_>) -> Range {
    let base = entry.range();
    match &entry.row.instances {
        Some(dimension) if !dimension.offsets.is_empty() => {
            let first = u32::from(dimension.offsets[0]);
            let last = u32::from(dimension.offsets[dimension.offsets.len() - 1])
                + u32::from(entry.row.len);
            Range {
                offset: first as u8,
                len: (last - first) as u16,
                ..base
            }
        }
        _ => base,
    }
}

/// monitor_entry with the plan's error type. monitor_entry only fails
/// with MonitorRowAbsent (its body collapses every resolution failure into
/// that one refusal), so no decode-state error can leak through here.
fn plan_monitor_entry(
    revision: Rev,
    page: &'static str,
    name: &'static str,
) -> Result<openxcvr_core::field_catalog::FieldEntry<'static>, BasicPlanError> {
    monitor_entry(revision, page, name).map_err(|_| BasicPlanError::MonitorRowAbsent {
        page,
        name,
        rev: revision,
    })
}

/// The fixed per-family prelude: the ranges whose bytes decide everything
/// the rest of the plan needs to know — whether the module's reported
/// revision/compliance admits a decode at all (CMIS lower-memory revision
/// byte; SFF-8636 lower-page and SFF-8472 A0h compliance byte), and which
/// calibration variant the SFF-8472 monitor rows decode under (the rows'
/// external-calibration discriminator location). Every location is a
/// catalog row's; nothing here is a second offset table.
///
/// The SFF-8472 discriminator byte is part of the fixed prelude (not of
/// the remaining ranges) so that one acquire-decide-acquire round serves
/// every module: the constants it gates cannot be decided before its byte
/// exists. The consequence is deliberate and documented: when the
/// compliance byte refuses the decode, the prelude has named one byte
/// (the discriminator) that the refusal path does not read — a one-byte
/// over-read on a non-conforming module, traded for a two-round shape.
pub fn basic_read_prelude(
    family: Family,
    revision: Rev,
) -> Result<Vec<PlannedRead>, BasicPlanError> {
    let catalog = family_catalog(family).ok_or(BasicPlanError::UnsupportedFamily(family))?;
    if !catalog.supported_revs.contains(&revision) {
        return Err(BasicPlanError::UnsupportedRevision { rev: revision });
    }
    let names = monitor_names(family).ok_or(BasicPlanError::UnsupportedFamily(family))?;
    let mut reads = Vec::new();
    // The revision/compliance gate row.
    match family {
        Family::Cmis => {
            let entry = plan_monitor_entry(revision, "CmisLowerMemory", "cmis_revision")?;
            reads.push(PlannedRead::new(entry.range(), entry.row.name));
        }
        Family::Sff8636 | Family::Sff8472 => {
            let (page, name) =
                compliance_field(family).expect("every SFF family has a compliance field");
            let entry = plan_monitor_entry(revision, page, name)?;
            reads.push(PlannedRead::new(entry.range(), entry.row.name));
        }
        _ => return Err(BasicPlanError::UnsupportedFamily(family)),
    }
    // The monitor rows' external-calibration discriminators (unique, in
    // monitor order). SFF-8472's five rows share one location.
    for (page, name) in names {
        let Ok(entry) = monitor_entry(revision, page, name) else {
            continue;
        };
        if let Some(discriminator) = &entry.row.discriminator {
            let range = discriminator.loc.range(0);
            if !reads.iter().any(|read| read.range == range) {
                reads.push(PlannedRead::new(range, discriminator.field));
            }
        }
    }
    Ok(reads)
}

/// The bytes at one prelude-planned range, classified the way both plan
/// rounds classify prelude reads: `Ok(Some(bytes))` when the view
/// supplies the range, `Ok(None)` when it does not (the caller turns
/// that into its typed missing-byte outcome), and the view error
/// otherwise (stale generation/epoch, malformed view).
fn admission_bytes(
    range: Range,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<Option<Vec<u8>>, InputError> {
    match supplied_bytes(view, range, generation, epoch) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(InputError::InsufficientInput { .. }) => Ok(None),
        Err(other) => Err(other),
    }
}

/// One byte at a prelude range, or the typed refusal naming the range.
fn prelude_byte(
    range: Range,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<u8, BasicPlanError> {
    match admission_bytes(range, view, generation, epoch) {
        Ok(Some(bytes)) => Ok(bytes[0]),
        Ok(None) => Err(BasicPlanError::PreludeMissing { range }),
        Err(other) => Err(BasicPlanError::Input(other)),
    }
}

/// The remaining ranges the decode reads, given the prelude bytes: the
/// five monitors' exact instance spans, plus the CMIS
/// TxBiasCurrentScalingFactor advertisement when a monitor's scaling is
/// qualified by it, plus the SFF-8472 external-calibration constants only
/// when the discriminator byte says external calibration. Refuses when
/// the prelude bytes are missing, when the reported revision/compliance
/// does not admit a decode at the requested revision (the same gates
/// [`observe_basic`] applies), or when the view itself is invalid — in
/// which refusal cases no monitor byte is planned at all.
pub fn basic_read_remaining(
    family: Family,
    revision: Rev,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<Vec<PlannedRead>, BasicPlanError> {
    let catalog = family_catalog(family).ok_or(BasicPlanError::UnsupportedFamily(family))?;
    if !catalog.supported_revs.contains(&revision) {
        return Err(BasicPlanError::UnsupportedRevision { rev: revision });
    }
    let names = monitor_names(family).ok_or(BasicPlanError::UnsupportedFamily(family))?;
    // Gate the decode on the prelude bytes, mirroring observe_basic.
    match family {
        Family::Cmis => {
            let entry = plan_monitor_entry(revision, "CmisLowerMemory", "cmis_revision")?;
            let byte = prelude_byte(entry.range(), view, generation, epoch)?;
            match Rev::from_major_byte_cmis(byte) {
                None => {
                    return Err(BasicPlanError::UnsupportedRevision {
                        rev: Rev {
                            family: Family::Cmis,
                            major: byte >> 4,
                            minor: byte & 0x0f,
                        },
                    })
                }
                Some(detected) if detected != revision => {
                    return Err(BasicPlanError::RevisionMismatch {
                        requested: revision,
                        byte,
                    })
                }
                Some(_) => {}
            }
        }
        Family::Sff8636 | Family::Sff8472 => {
            let (page, name) =
                compliance_field(family).expect("every SFF family has a compliance field");
            let entry = plan_monitor_entry(revision, page, name)?;
            let code = prelude_byte(entry.range(), view, generation, epoch)?;
            if !compliance_admits(family, code, revision) {
                return Err(BasicPlanError::ComplianceRefused {
                    family,
                    code,
                    meaning: reported_compliance_for(family, code).meaning,
                });
            }
        }
        _ => return Err(BasicPlanError::UnsupportedFamily(family)),
    }
    let mut reads = Vec::new();
    for (page, name) in names {
        let entry = plan_monitor_entry(revision, page, name)?;
        reads.push(PlannedRead::new(entry_span(&entry), entry.row.name));
        // External-calibration constants: planned only when the row's own
        // discriminator byte says the external variant applies.
        if let Some(discriminator) = &entry.row.discriminator {
            let byte = prelude_byte(discriminator.loc.range(0), view, generation, epoch)?;
            if byte & (1 << discriminator.bit) == 0 {
                continue;
            }
            match &entry.row.scaling {
                Scaling::Linear {
                    external: Some(external),
                    ..
                } => {
                    reads.push(PlannedRead::new(external.slope.range(0), entry.row.name));
                    reads.push(PlannedRead::new(external.offset.range(0), entry.row.name));
                }
                Scaling::Polynomial {
                    external: Some(external),
                    ..
                } => {
                    for coefficient in &external.coefficients {
                        reads.push(PlannedRead::new(coefficient.range(0), entry.row.name));
                    }
                }
                _ => {}
            }
        }
    }
    // CMIS: the TxBiasCurrentScalingFactor advertisement, when a planned
    // monitor's scaling is qualified by it (the same test
    // apply_bias_advertisement uses).
    if family == Family::Cmis {
        let bias = plan_monitor_entry(revision, names[2].0, names[2].1)?;
        if scaling_needs_bias_advertisement(&bias) {
            let advertisement = plan_monitor_entry(
                revision,
                "CmisPage01hAdvertising",
                "tx_bias_current_scaling_factor",
            )?;
            reads.push(PlannedRead::new(
                advertisement.range(),
                advertisement.row.name,
            ));
        }
    }
    Ok(reads)
}

// ─── Two-round field read planning (prelude, then remaining) ───────────
//
// `field_read_plan` plans every entry's bytes and every decode
// dependency in one pure pass, because which bytes matter is decided by
// bytes nobody has read yet: a selector-multiplexed row's data is
// meaningless unless the selector names its view, and a gated row is
// absent unless the gate byte says otherwise. Two rounds make that
// decidable before the data transfer — the shape `basic_read_prelude` /
// `basic_read_remaining` already give the fixed basic set, given here to
// arbitrary catalog-resolved entries:
//
//   1. [`field_read_prelude`]: the family gate byte plus every entry's
//      presence-prerequisite byte — no entry data, no discriminator, no
//      calibration constant, no bias advertisement.
//   2. [`field_read_remaining`]: given those bytes through any
//      [`ByteView`], admit or refuse each entry (first matching rule
//      wins) and plan exactly the admitted entries' own ranges and
//      remaining decode dependencies.
//
// R3-A2, stated exactly: the prelude may itself read prerequisite bytes
// (possibly on an upper page) before revision proof; the guarantee is
// only that remaining plans no dependent range after a refusal.

/// Why the remaining round admits no data bytes for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldAdmissionReason {
    /// A prelude byte this entry's admission needs is absent from the
    /// view (the family gate byte, or this entry's own prerequisite).
    PreludeMissing {
        /// The missing prelude range.
        range: Range,
    },
    /// CMIS lower-memory byte 1 decodes to another (or no known)
    /// revision.
    UnsupportedRevision {
        /// The revision the module reported.
        reported: Rev,
    },
    /// SFF compliance byte does not admit a decode at the schema
    /// revision.
    ComplianceRefused {
        /// The raw compliance code.
        code: u8,
    },
    /// Plain Equals prerequisite present but not equal.
    PrerequisiteNotSatisfied {
        /// The prerequisite's schema field path.
        prerequisite: &'static str,
        /// The code the prerequisite must carry.
        expected: u64,
        /// The code the prerequisite carried.
        actual: u64,
    },
    /// Content-multiplexed entry: the selector names another view.
    ViewNotSelected {
        /// The selector's schema field path.
        selector: &'static str,
        /// The code that would select this view.
        expected: u64,
        /// The code the selector carried.
        actual: u64,
    },
    /// FieldPresence::Unsupported.
    UnsupportedPresence {
        /// The presence expression the surface does not evaluate.
        expr: &'static str,
    },
    /// Existing read-effect refusal (entry bytes or a remaining
    /// dependency).
    Effect(FieldPlanRefusalReason),
    /// The prerequisite's bytes are supplied but carry no scalar code
    /// (a non-scalar kind, or a shape too short for its kind), so no
    /// actual value exists to compare against the expected one. Reachable
    /// only through a hand-built entry — generated prerequisite kinds are
    /// scalar by construction — and never admitted: failed extraction is
    /// never a fabricated actual code.
    PrerequisiteUndecodable {
        /// The prerequisite whose scalar could not be extracted.
        prerequisite: &'static str,
    },
}

/// One entry the remaining round refused, and why no data bytes are
/// planned for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAdmissionRefusal {
    /// The catalog row's effective name at the requested revision.
    pub field: &'static str,
    /// Instance index for per-lane (or other) instanced fields.
    pub lane: Option<u16>,
    /// Why no data bytes are planned for the entry.
    pub reason: FieldAdmissionReason,
}

/// An admitted content-multiplexed entry: its decode needs a
/// [`SelectorAcquisition`] with these coordinates (a requirement, not
/// an authorization). `selector_range`/`selector_value` name the
/// prerequisite byte and raw code the view selected; `data_range` is
/// the content-switch data window the decode's evidence must cover.
///
/// `data_range` is an evidence coordinate, not a read authorization:
/// it is the one range a plan publishes that never passes the effect
/// classification every planned range passes (it is wider than the
/// entry's own range and is itself never planned), so an acquisition
/// owner must classify it against the catalog's read effects before
/// acquiring it, exactly as this module classifies every range it
/// plans. The generation and epoch the evidence must carry are not
/// repeated here: the decode accepts evidence stamped with exactly the
/// generation and epoch of the decode — and of the planning call — so
/// they are the call's own arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct AcquisitionRequirement {
    /// The catalog row's effective name at the requested revision.
    pub field: &'static str,
    /// Instance index for per-lane (or other) instanced fields.
    pub lane: Option<u16>,
    /// The selector byte's range.
    pub selector_range: Range,
    /// The raw code the view selected.
    pub selector_value: u64,
    /// The content-switch data window the evidence must cover.
    pub data_range: Range,
}

/// The remaining round's plan: the data ranges admitted entries still
/// need, the per-entry admission refusals (in entry order, one per
/// entry — first matching rule wins), and the selector-acquisition
/// evidence admitted content-multiplexed entries require.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldRemainingPlan {
    /// The exact ordered acquisition ranges, sorted and coalesced exactly
    /// as [`FieldReadPlan::reads`] are. Gate and prerequisite bytes the
    /// prelude already acquired are never re-listed.
    pub reads: Vec<PlannedRead>,
    /// Entries the prelude bytes did not admit, in entry order, with
    /// typed reasons. Their bytes appear in no planned range; a byte
    /// also needed by an admitted entry stays planned through that entry.
    pub refusals: Vec<FieldAdmissionRefusal>,
    /// One requirement per admitted content-multiplexed entry, in entry
    /// order.
    pub acquisition_required: Vec<AcquisitionRequirement>,
}

/// The family gate byte's range and catalog name: the byte whose value
/// decides whether any entry of the family decodes at the requested
/// revision at all. CMIS: lower-memory byte 1, located through the
/// resolved entries' revision probe (the catalog row behind it when no
/// entry carries one — a hand-built entry has no generated catalog
/// behind it). SFF-8636/SFF-8472: the compliance byte's catalog row.
/// After `validate_field_entries` only the three cataloged families
/// reach here, and both SFF ones have a compliance field; a cataloged
/// family without one would be a typed
/// [`FieldPlanError::UnsupportedFamily`] refusal, never a panic on
/// public input.
fn family_gate(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
) -> Result<(Range, &'static str), FieldPlanError> {
    if family == Family::Cmis {
        // The modeled probes live in unbanked lower memory: bank 0,
        // the same location the core decoder's revision admission reads.
        if let Some(probe) = entries.iter().find_map(|entry| entry.revision_probe) {
            return Ok((probe.loc.range(0), probe.field));
        }
        let entry = monitor_entry(revision, "CmisLowerMemory", "cmis_revision").map_err(|_| {
            FieldPlanError::MonitorRowAbsent {
                page: "CmisLowerMemory",
                name: "cmis_revision",
                rev: revision,
            }
        })?;
        Ok((entry.range(), entry.row.name))
    } else {
        // Unreachable for the families cataloged today (both SFF ones
        // carry a compliance field, and `validate_field_entries` refuses
        // every non-cataloged family first) — but reachability is a
        // property of the generated tables, not of this function's
        // contract, so the refusal is typed.
        let Some((page, name)) = compliance_field(family) else {
            return Err(FieldPlanError::UnsupportedFamily(family));
        };
        let entry =
            monitor_entry(revision, page, name).map_err(|_| FieldPlanError::MonitorRowAbsent {
                page,
                name,
                rev: revision,
            })?;
        Ok((entry.range(), entry.row.name))
    }
}

/// Round 1: the bytes whose values decide admission, before any entry
/// data is acquired — the family gate byte (CMIS lower-memory byte 1
/// through the entries' revision probe; the SFF compliance byte), plus
/// every entry's presence-prerequisite range (bank per the
/// prerequisite's own bank policy), validated, effect-checked, sorted
/// and coalesced exactly as [`field_read_plan`] treats its ranges. No
/// entry data range, no discriminator, no calibration constant and no
/// bias advertisement: those belong to the remaining round, and only
/// for entries the prelude bytes admit.
///
/// Refusals use the existing [`FieldPlanRefusal`] type, with the same
/// consequences [`field_read_plan`] gives them: an entry whose presence
/// prerequisite carries a restrictive read effect is refused (its bytes
/// appear in no planned range), and a gate byte that is not passively
/// readable refuses every surviving entry. Unconditional and
/// [`FieldPresence::Unsupported`] entries contribute only the gate byte.
/// Refusals are published in input-entry order: the two processing
/// passes keep their fixed order (each entry's prerequisite effect
/// first, the shared gate effect second), but a gate refusal discovered
/// in the second pass lands at its entry's input position, never after
/// a prerequisite refusal of a later entry.
///
/// R3-A2, stated exactly: the prelude may itself read prerequisite bytes
/// (possibly on an upper page) before revision proof; the guarantee is
/// only that the remaining round plans no dependent range after a
/// refusal.
pub fn field_read_prelude(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
) -> Result<FieldReadPlan, FieldPlanError> {
    validate_field_entries(family, revision, entries, FIELD_PLAN_DEFAULT_LIMITS)?;
    let catalog = family_catalog(family).ok_or(FieldPlanError::UnsupportedFamily(family))?;
    // Structural validation before any gate logic (rule 8): a malformed
    // entry is a plan-level input error in this round too — without this
    // pass the prelude would plan the gate byte alone for an entry whose
    // own or dependency ranges no later rule of this round consults.
    for entry in entries {
        validate_entry_structure(entry)?;
    }
    // Per-entry refusal reason, when this round refuses the entry —
    // assembled into `refusals` in input order at the end (rule 18), so
    // a gate-effect refusal discovered in the second pass lands at its
    // entry's input position, not after a pass-1 refusal of a later
    // entry. The processing passes keep their fixed order; only the
    // publication order is the input's.
    let mut refusal: Vec<Option<FieldPlanRefusalReason>> = vec![None; entries.len()];
    // Each surviving entry contributes at most its presence prerequisite;
    // refused entries contribute nothing (a byte shared with another,
    // surviving entry stays planned through that entry).
    let mut per_entry: Vec<Vec<(Range, &'static str)>> = vec![Vec::new(); entries.len()];

    // Pass 1 — each entry's presence prerequisite. An effect-tainted
    // prerequisite refuses its entry, exactly as `field_read_plan`
    // refuses a tainted dependency; a clean one is planned. Unconditional
    // and Unsupported entries add nothing here.
    for (index, entry) in entries.iter().enumerate() {
        let Some((range, purpose)) = entry_prerequisite(entry) else {
            continue;
        };
        range.validate().map_err(FieldPlanError::Input)?;
        if let Some(reason) = restrictive_effect(catalog, revision, range) {
            refusal[index] = Some(reason);
        } else {
            per_entry[index].push((range, purpose));
        }
    }

    // Pass 2 — the family gate byte, planned exactly as `field_read_plan`
    // plans it: only while an entry survives, and refusing every survivor
    // when the gate byte itself is not passively readable.
    let mut shared: Vec<(Range, &'static str)> = Vec::new();
    if refusal.iter().any(|reason| reason.is_none()) {
        let (range, purpose) = family_gate(family, revision, entries)?;
        range.validate().map_err(FieldPlanError::Input)?;
        if let Some(reason) = restrictive_effect(catalog, revision, range) {
            for (index, slot) in refusal.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(reason);
                    per_entry[index].clear();
                }
            }
        } else {
            shared.push((range, purpose));
        }
    }

    let planned: Vec<(Range, &'static str)> =
        per_entry.into_iter().flatten().chain(shared).collect();
    let reads = coalesce_reads(planned, FIELD_PLAN_DEFAULT_LIMITS)?;
    // Input order (rule 18): one refusal per refused entry, each at its
    // entry's input position.
    let refusals: Vec<FieldPlanRefusal> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            refusal[index].map(|reason| FieldPlanRefusal {
                field: entry.row.name,
                lane: entry.lane,
                reason,
            })
        })
        .collect();
    Ok(FieldReadPlan { reads, refusals })
}

/// Round 2: which data bytes remain, given the prelude bytes — a
/// self-contained admission: callers may pass the original, unfiltered
/// entries; the round re-checks every effect it relies on itself. Every
/// entry is admitted or refused by the first matching rule, in entry
/// order:
///
/// 1. the family gate byte: a restrictive read effect on its range →
///    [`FieldAdmissionReason::Effect`] for every entry, before the gate
///    byte is read at all; absent from the view →
///    [`FieldAdmissionReason::PreludeMissing`]; a CMIS byte 1 decoding
///    to another or no known revision →
///    [`FieldAdmissionReason::UnsupportedRevision`]; an SFF compliance
///    code that does not admit a decode at the schema revision →
///    [`FieldAdmissionReason::ComplianceRefused`];
/// 2. [`FieldPresence::Unsupported`] →
///    [`FieldAdmissionReason::UnsupportedPresence`];
/// 3. an `Equals` prerequisite: a restrictive read effect on its range →
///    [`FieldAdmissionReason::Effect`], before the prerequisite bytes
///    are read; bytes absent →
///    [`FieldAdmissionReason::PreludeMissing`]; supplied bytes carrying
///    no scalar code →
///    [`FieldAdmissionReason::PrerequisiteUndecodable`]; a raw code
///    (extracted exactly as the core decoder extracts it) other than
///    the expected value → [`FieldAdmissionReason::ViewNotSelected`]
///    for a content-multiplexed row, otherwise
///    [`FieldAdmissionReason::PrerequisiteNotSatisfied`];
/// 4. admitted: the entry's own range plus its remaining decode
///    dependencies (variant discriminator, external-calibration
///    constants, and the CMIS TxBiasCurrentScalingFactor advertisement
///    where [`field_read_plan`] adds it), effect-checked — a
///    restrictive effect refuses the entry with
///    [`FieldAdmissionReason::Effect`].
///
/// Exactly one refusal is published per refused entry, in input order —
/// including refusals discovered while processing an entry's remaining
/// decode dependencies or the shared bias advertisement. The gate and
/// prerequisite bytes the prelude acquired are never re-listed. Refused
/// entries contribute no range of their own; a byte also needed by an
/// admitted entry stays planned through that entry. Every admitted
/// content-multiplexed entry adds one [`AcquisitionRequirement`] — the
/// coordinates its decode's [`SelectorAcquisition`] evidence must carry
/// (a requirement, not an authorization; see [`AcquisitionRequirement`]
/// for what the data window is and where the evidence's generation and
/// epoch come from).
///
/// View errors other than a missing byte (stale generation/epoch,
/// malformed view) are [`FieldPlanError::Input`], never a per-entry
/// refusal. R3-A2, stated exactly: the prelude may itself read
/// prerequisite bytes (possibly on an upper page) before revision
/// proof; the guarantee is only that this round plans no dependent range
/// after a refusal.
pub fn field_read_remaining(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<FieldRemainingPlan, FieldPlanError> {
    validate_field_entries(family, revision, entries, FIELD_PLAN_DEFAULT_LIMITS)?;
    let catalog = family_catalog(family).ok_or(FieldPlanError::UnsupportedFamily(family))?;
    // Structural validation before the gate and the admission walk
    // (rules 8 and 12: the first-match precedence below applies only
    // after structural validation): every range an entry can contribute
    // — own, `Equals` prerequisite, remaining dependencies — is checked
    // up front, so no earlier outcome (a view lacking the gate byte, a
    // refusing gate, an `Unsupported` presence) can dispose of a
    // malformed entry as a refusal instead of a plan-level input error.
    // The lazy validation sites inside the walk stay as the reference
    // shape and test the same ranges.
    for entry in entries {
        validate_entry_structure(entry)?;
    }
    // Per-entry outcome: the refusal reason, when this round refuses the
    // entry — assembled into `refusals` in input order at the end, so a
    // refusal discovered during shared-dependency processing still lands
    // at its entry's input position, one per entry at most.
    let mut refusal: Vec<Option<FieldAdmissionReason>> = vec![None; entries.len()];
    // Per admitted entry: its own range plus its remaining decode
    // dependencies; per entry in rule-3 order: the acquisition evidence
    // coordinates its decode requires.
    let mut per_entry: Vec<Vec<(Range, &'static str)>> = vec![Vec::new(); entries.len()];
    let mut requirements: Vec<Option<AcquisitionRequirement>> = vec![None; entries.len()];
    let mut admitted = vec![false; entries.len()];

    // Rule 1 — the family gate byte, read once for every entry. An
    // effect-tainted gate refuses every entry before the gate byte is
    // read at all; a gate byte that does not admit the family at this
    // revision refuses every entry before any prerequisite is read or
    // any selector-dependent range is planned (R3-A2); a view that
    // cannot be read at the gate range at all is a plan-level input
    // error.
    if !entries.is_empty() {
        let (gate_range, _) = family_gate(family, revision, entries)?;
        gate_range.validate().map_err(FieldPlanError::Input)?;
        if let Some(reason) = restrictive_effect(catalog, revision, gate_range) {
            return Ok(FieldRemainingPlan {
                reads: Vec::new(),
                refusals: entries
                    .iter()
                    .map(|entry| FieldAdmissionRefusal {
                        field: entry.row.name,
                        lane: entry.lane,
                        reason: FieldAdmissionReason::Effect(reason),
                    })
                    .collect(),
                acquisition_required: Vec::new(),
            });
        }
        let gate_reason = match admission_bytes(gate_range, view, generation, epoch) {
            Ok(None) => Some(FieldAdmissionReason::PreludeMissing { range: gate_range }),
            Err(other) => return Err(FieldPlanError::Input(other)),
            Ok(Some(bytes)) => {
                let gate_byte = bytes[0];
                match family {
                    Family::Cmis => match Rev::from_major_byte_cmis(gate_byte) {
                        Some(detected) if detected == revision => None,
                        Some(detected) => {
                            Some(FieldAdmissionReason::UnsupportedRevision { reported: detected })
                        }
                        // An unknown revision still names what the byte
                        // reports, decoded the same way
                        // `basic_read_remaining` decodes it.
                        None => Some(FieldAdmissionReason::UnsupportedRevision {
                            reported: Rev {
                                family: Family::Cmis,
                                major: gate_byte >> 4,
                                minor: gate_byte & 0x0f,
                            },
                        }),
                    },
                    // validate_field_entries admits only cataloged
                    // families: CMIS plus the two SFF compliance families.
                    _ => {
                        if compliance_admits(family, gate_byte, revision) {
                            None
                        } else {
                            Some(FieldAdmissionReason::ComplianceRefused { code: gate_byte })
                        }
                    }
                }
            }
        };
        if let Some(reason) = gate_reason {
            return Ok(FieldRemainingPlan {
                reads: Vec::new(),
                refusals: entries
                    .iter()
                    .map(|entry| FieldAdmissionRefusal {
                        field: entry.row.name,
                        lane: entry.lane,
                        reason: reason.clone(),
                    })
                    .collect(),
                acquisition_required: Vec::new(),
            });
        }
    }

    // Rules 2-4, per entry in order; the first matching rule wins.
    for (index, entry) in entries.iter().enumerate() {
        // Rule 2 — a presence condition this surface does not evaluate.
        if let FieldPresence::Unsupported { expr } = &entry.row.presence {
            refusal[index] = Some(FieldAdmissionReason::UnsupportedPresence { expr });
            continue;
        }
        // Rule 3 — the presence prerequisite the prelude acquired: its
        // range is effect-checked before its bytes are read (an effect
        // outranks a missing or mismatching prerequisite), and its raw
        // code is extracted exactly as the Stage 1 core decoder extracts
        // it (openxcvr-core's `prerequisite_raw_code`).
        if let FieldPresence::Equals(prereq) = &entry.row.presence {
            let bank = if prereq.banked { entry.bank } else { 0 };
            let range = prereq.loc.range(bank);
            range.validate().map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, range) {
                refusal[index] = Some(FieldAdmissionReason::Effect(reason));
                continue;
            }
            let raw = match admission_bytes(range, view, generation, epoch) {
                Ok(Some(bytes)) => {
                    openxcvr_core::observation::prerequisite_raw_code(prereq, &bytes)
                }
                Ok(None) => {
                    refusal[index] = Some(FieldAdmissionReason::PreludeMissing { range });
                    continue;
                }
                Err(other) => return Err(FieldPlanError::Input(other)),
            };
            match raw {
                Some(raw) if raw == prereq.expected => {}
                // A successfully extracted unequal code: which variant
                // names it depends only on whether the prerequisite
                // multiplexes content.
                Some(raw) => {
                    refusal[index] = Some(if prereq.content_switch.is_some() {
                        FieldAdmissionReason::ViewNotSelected {
                            selector: prereq.field,
                            expected: prereq.expected,
                            actual: raw,
                        }
                    } else {
                        FieldAdmissionReason::PrerequisiteNotSatisfied {
                            prerequisite: prereq.field,
                            expected: prereq.expected,
                            actual: raw,
                        }
                    });
                    continue;
                }
                // No scalar code in the prerequisite's supplied bytes:
                // unreachable for generated rows (prerequisite kinds are
                // scalar by construction), reachable only through a
                // hand-built entry. No actual value exists to compare or
                // fabricate — the entry is refused on the typed
                // undecodable reason, never admitted on a guess.
                None => {
                    refusal[index] = Some(FieldAdmissionReason::PrerequisiteUndecodable {
                        prerequisite: prereq.field,
                    });
                    continue;
                }
            }
        }
        // Rule 4 — an admitted entry: its own range plus its remaining
        // decode dependencies, effect-checked exactly as `field_read_plan`
        // checks them. The gate and prerequisite bytes are not re-listed.
        let range = entry.range();
        range.validate().map_err(FieldPlanError::Input)?;
        if let Some(reason) = restrictive_effect(catalog, revision, range) {
            refusal[index] = Some(FieldAdmissionReason::Effect(reason));
            continue;
        }
        per_entry[index].push((range, entry.row.name));
        let mut effect_refused = false;
        for (need, purpose) in entry_remaining_dependencies(entry) {
            need.validate().map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, need) {
                refusal[index] = Some(FieldAdmissionReason::Effect(reason));
                effect_refused = true;
                break;
            }
            per_entry[index].push((need, purpose));
        }
        if effect_refused {
            per_entry[index].clear();
            continue;
        }
        admitted[index] = true;
        // A content-multiplexed entry's decode additionally requires
        // selector-acquisition evidence with these coordinates: the
        // prerequisite byte and expected code, and the content-switch
        // data window that evidence must cover.
        if let FieldPresence::Equals(prereq) = &entry.row.presence {
            if let Some(switch) = &prereq.content_switch {
                let bank = if prereq.banked { entry.bank } else { 0 };
                requirements[index] = Some(AcquisitionRequirement {
                    field: entry.row.name,
                    lane: entry.lane,
                    selector_range: prereq.loc.range(bank),
                    selector_value: prereq.expected,
                    data_range: switch.data.range(bank),
                });
            }
        }
    }

    // The CMIS TxBiasCurrentScalingFactor advertisement, planned exactly
    // where `field_read_plan` plans it: a remaining decode dependency of
    // every admitted laser-bias-scaled entry, acquired once, with an
    // effect-tainted advertisement refusing exactly those entries (and
    // dropping the evidence they would have required — a refused decode
    // needs none).
    let mut shared: Vec<(Range, &'static str)> = Vec::new();
    if family == Family::Cmis {
        let bias_dependents: Vec<usize> = (0..entries.len())
            .filter(|index| admitted[*index])
            .filter(|index| scaling_needs_bias_advertisement(&entries[*index]))
            .collect();
        if !bias_dependents.is_empty() {
            let advertisement = monitor_entry(
                revision,
                "CmisPage01hAdvertising",
                "tx_bias_current_scaling_factor",
            )
            .map_err(|_| FieldPlanError::MonitorRowAbsent {
                page: "CmisPage01hAdvertising",
                name: "tx_bias_current_scaling_factor",
                rev: revision,
            })?;
            let advertisement_range = advertisement.range();
            advertisement_range
                .validate()
                .map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, advertisement_range) {
                for index in bias_dependents {
                    refusal[index] = Some(FieldAdmissionReason::Effect(reason));
                    admitted[index] = false;
                    per_entry[index].clear();
                    requirements[index] = None;
                }
            } else {
                shared.push((advertisement_range, advertisement.row.name));
            }
        }
    }

    // Assemble from the admitted entries only, then sort and coalesce
    // exactly as `field_read_plan` does; refusals are published in input
    // order, one per entry.
    let planned: Vec<(Range, &'static str)> =
        per_entry.into_iter().flatten().chain(shared).collect();
    let reads = coalesce_reads(planned, FIELD_PLAN_DEFAULT_LIMITS)?;
    let acquisition_required = requirements.into_iter().flatten().collect();
    let refusals: Vec<FieldAdmissionRefusal> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            refusal[index].take().map(|reason| FieldAdmissionRefusal {
                field: entry.row.name,
                lane: entry.lane,
                reason,
            })
        })
        .collect();
    Ok(FieldRemainingPlan {
        reads,
        refusals,
        acquisition_required,
    })
}

// ─── Field read plan for arbitrary catalog-resolved entries ─────────────
//
// `observe_basic` has its own fixed plan (above); arbitrary catalog-
// resolved fields get this one. The consumer resolves field patterns with
// `crate::catalog::resolve` (pure, no module bytes), hands the resolved
// entries to [`field_read_plan`], acquires exactly the returned ranges,
// and decodes with [`observe_fields`] — no offset arithmetic ever lives in
// the consumer. The plan is pure in the same sense: it needs no module
// bytes beyond the family and revision the caller already established,
// and a flat/paged fact is not needed at all — upper-page ranges are
// marked optional unconditionally, exactly as `basic_read_remaining`
// does, so a flat-memory module's absent page is an expected
// `NotSupplied` decode outcome, not a failure.
//
// Alongside each entry's own bytes the plan carries the decode's
// dependencies, derived from the same rows the decoders read: the SFF
// revision-compliance gate byte (every SFF `observe_field` decode reads
// it), the variant-discriminator byte of rows that carry one
// (`decode_scaling` reads it before it can know which variant applies),
// the SFF-8472 external-calibration constants (a pure plan cannot know
// the discriminator byte's value, so it always acquires the declared
// constants — passive static bytes the internal-calibration decode
// simply never reads), and the CMIS TxBiasCurrentScalingFactor
// advertisement for laser-bias-scaled rows.
//
// Clear-on-read, acknowledge-on-read and write-only bytes are refused,
// never planned: an entry whose byte range (classified against every
// catalog row overlapping it at the requested revision) carries such an
// effect is excluded with a typed [`FieldPlanRefusal`] naming the field
// and lane, so the consumer can refuse before any transfer. Write-only
// bytes (CMIS `software_reset` and the password change/entry areas
// among them) return undefined or vendor-specific bytes on read: no
// honest value exists, so none is ever planned or decoded. Coalescing
// never reads a byte nobody asked for: ranges merge only when they
// touch or overlap inside one (address, bank, page) selection, and
// refused bytes are excluded before coalescing runs.

/// Output bounds for one field read plan. Bounds are literal, including
/// zero: a zero allowance admits nothing, and no omitted/unlimited grant
/// is inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldPlanLimits {
    /// How many resolved entries one plan accepts.
    pub max_entries: usize,
    /// How many coalesced ranges one plan may return.
    pub max_ranges: usize,
}

/// The default bounds: a show-style query resolves tens of entries; 256
/// leaves generous headroom while bounding the plan's wire payload.
pub const FIELD_PLAN_DEFAULT_LIMITS: FieldPlanLimits = FieldPlanLimits {
    max_entries: 256,
    max_ranges: 256,
};

/// Why a resolved entry was excluded from a field read plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPlanRefusalReason {
    /// Reading the entry's bytes (or a byte merged with them, or a byte
    /// the decode of this entry needs) clears a latched flag.
    ClearOnRead,
    /// Reading the entry's bytes completes a startup-interrupt
    /// acknowledgment.
    AcknowledgeOnRead,
    /// The entry's bytes (or a byte merged with them, or a byte the
    /// decode of this entry needs) are write-only: a read of a
    /// write-only location returns undefined or vendor-specific bytes,
    /// so no honest value exists and none is ever planned or decoded.
    WriteOnly,
}

impl FieldPlanRefusalReason {
    /// Stable identifier for events, refusals and tests.
    pub const fn id(self) -> &'static str {
        match self {
            FieldPlanRefusalReason::ClearOnRead => "clear-on-read",
            FieldPlanRefusalReason::AcknowledgeOnRead => "acknowledge-on-read",
            FieldPlanRefusalReason::WriteOnly => "write-only",
        }
    }
}

/// One refused entry: which field instance, and why its bytes appear in
/// no planned range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPlanRefusal {
    /// The catalog row's effective name at the requested revision.
    pub field: &'static str,
    /// Instance index for per-lane (or other) instanced fields.
    pub lane: Option<u16>,
    /// Why the entry was refused.
    pub reason: FieldPlanRefusalReason,
}

/// The acquisition plan for a set of resolved entries.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldReadPlan {
    /// The exact ordered acquisition ranges: sorted by
    /// (address, page, bank, offset), coalesced where adjacent ranges
    /// touch or overlap within one selection. A coalesced range's
    /// `purpose` names its lowest-offset contributor.
    pub reads: Vec<PlannedRead>,
    /// Entries refused before planning, in entry order, with typed
    /// reasons. Their bytes appear in no planned range, so a consumer
    /// that acquires the plan never touches them.
    pub refusals: Vec<FieldPlanRefusal>,
}

/// Why a field read plan was refused.
/// Why a field read plan was refused.
#[derive(Debug, thiserror::Error)]
pub enum FieldPlanError {
    /// The family has no field-read planning.
    #[error("unsupported family {0:?} for the field read plan")]
    UnsupportedFamily(Family),
    /// The revision has no field-read planning.
    #[error("unsupported revision {rev:?} for the field read plan")]
    UnsupportedRevision {
        /// The revision that was refused.
        rev: Rev,
    },
    /// An entry resolved at a different revision than the planned one.
    #[error(
        "entry {field:?} (lane {lane:?}) resolved at {entry_rev:?}, not the planned revision {rev:?}"
    )]
    EntryRevisionMismatch {
        /// The entry's field name.
        field: &'static str,
        /// The entry's instance index, when instanced.
        lane: Option<u16>,
        /// The revision the entry resolved at.
        entry_rev: Rev,
        /// The revision the plan runs at.
        rev: Rev,
    },
    /// More entries than the plan limit.
    #[error("too many entries: {count} > {limit}")]
    TooManyEntries {
        /// The supplied entry count.
        count: usize,
        /// The plan's entry limit.
        limit: usize,
    },
    /// More planned ranges than the plan limit.
    #[error("too many planned ranges: {count} > {limit}")]
    TooManyRanges {
        /// The resulting range count.
        count: usize,
        /// The plan's range limit.
        limit: usize,
    },
    /// A monitor field's catalog row is absent at the revision.
    #[error("catalog row for {page}::{name} absent at {rev:?}")]
    MonitorRowAbsent {
        /// The row's page id.
        page: &'static str,
        /// The row's field name.
        name: &'static str,
        /// The revision the row is absent at.
        rev: Rev,
    },
    /// The supplied view failed validation.
    #[error("invalid planned range: {0}")]
    Input(InputError),
}

/// The family/revision gates and bounds both plan entry points share:
/// the family's catalog must exist, the revision must be one the family's
/// schemas describe, the entry count must fit the limits, and every entry
/// must have been resolved at exactly the planned revision (an entry from
/// another revision or family is a caller error, never decoded
/// cross-revision).
fn validate_field_entries(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
    limits: FieldPlanLimits,
) -> Result<(), FieldPlanError> {
    let catalog = family_catalog(family).ok_or(FieldPlanError::UnsupportedFamily(family))?;
    if !catalog.supported_revs.contains(&revision) {
        return Err(FieldPlanError::UnsupportedRevision { rev: revision });
    }
    if entries.len() > limits.max_entries {
        return Err(FieldPlanError::TooManyEntries {
            count: entries.len(),
            limit: limits.max_entries,
        });
    }
    for entry in entries {
        if entry.rev != revision {
            return Err(FieldPlanError::EntryRevisionMismatch {
                field: entry.row.name,
                lane: entry.lane,
                entry_rev: entry.rev,
                rev: revision,
            });
        }
    }
    Ok(())
}

/// Structural per-entry validation, shared by both planning rounds and
/// run before any family-gate logic or admission walk: the entry's row
/// must apply at the entry's revision (`since`/`until`, the same test
/// `field_catalog::resolve` applies — `resolve` never returns rows
/// outside their window, so only a hand-built entry can carry one),
/// and every range an entry can contribute — its own range, its
/// `Equals` prerequisite's range with the prerequisite's content-switch
/// data window (bank per the prerequisite's own policy) and its
/// remaining decode-dependency ranges (variant discriminator,
/// external-calibration constants) — must be a valid `Range`, mapped to
/// [`FieldPlanError::Input`] exactly as the lazy validation sites inside
/// the walks map it. A structurally malformed entry is a plan-level
/// error (rule 8), never an admission refusal: without this pass a
/// malformed entry would escape as a cheaper outcome whenever an
/// earlier rule disposes of it — a view lacking the gate byte, a
/// refusing gate, an `Unsupported` presence. An inapplicable row is
/// named [`FieldPlanError::MonitorRowAbsent`] (row absent at the
/// revision), not the revision-mismatch variant: by the time this runs
/// `validate_field_entries` has already established that the entry's
/// revision equals the planned revision. This mirrors C's
/// `plan_row_valid`, which applies `row_applicable` and validates the
/// content-switch location alongside every other prerequisite
/// location.
fn validate_entry_structure(entry: &FieldEntry<'_>) -> Result<(), FieldPlanError> {
    if !row_applies_at(entry.row, entry.rev) {
        return Err(FieldPlanError::MonitorRowAbsent {
            page: entry.row.page_id,
            name: entry.row.name,
            rev: entry.rev,
        });
    }
    entry.range().validate().map_err(FieldPlanError::Input)?;
    if let FieldPresence::Equals(prerequisite) = &entry.row.presence {
        let bank = if prerequisite.banked { entry.bank } else { 0 };
        prerequisite
            .loc
            .range(bank)
            .validate()
            .map_err(FieldPlanError::Input)?;
        if let Some(switch) = &prerequisite.content_switch {
            switch
                .data
                .range(bank)
                .validate()
                .map_err(FieldPlanError::Input)?;
        }
    }
    for (need, _) in entry_remaining_dependencies(entry) {
        need.validate().map_err(FieldPlanError::Input)?;
    }
    Ok(())
}

/// Whether a catalog row applies at a revision (`since` inclusive,
/// `until` exclusive; `None` is unbounded) — the same test
/// `field_catalog::resolve` applies.
fn row_applies_at(row: &crate::catalog::CatalogRow, revision: Rev) -> bool {
    row.since.is_none_or(|since| revision >= since)
        && row.until.is_none_or(|until| revision < until)
}

/// The restrictive read effect any catalog row whose effect footprint
/// overlaps the range carries at the revision, if one does. Write-only
/// takes precedence over clear-on-read, which takes precedence over
/// acknowledge-on-read — no generated row carries more than one effect,
/// and a write-only byte is not readable at all (not even under a
/// clear-on-read authorization), so it is the reason worth naming;
/// core's own `NonPassiveReason` ordering reports write-only ahead of
/// the named effects too. Rows are matched by (address, page) and byte
/// overlap with their effect footprint — the union of the row's own
/// span and every instance offset span, see [`effect_footprint_overlaps`]
/// — so an instanced row's per-lane effects apply at every lane's byte,
/// not only at the base offset the row's own span names. The bank
/// dimension is deliberately not matched — a banked page's effect rows
/// apply to the same offsets on every bank, so this check is
/// conservative (it can refuse an entry whose other bank is clean,
/// never the reverse).
fn restrictive_effect(
    catalog: &crate::catalog::FamilyCatalog,
    revision: Rev,
    range: Range,
) -> Option<FieldPlanRefusalReason> {
    catalog
        .rows
        .iter()
        .filter(|row| row_applies_at(row, revision))
        .filter(|row| {
            row.address == range.address
                && row.page == range.page
                && effect_footprint_overlaps(row, range)
        })
        .find_map(|row| {
            if row.read_effects.contains(&ReadEffect::WriteOnly) {
                Some(FieldPlanRefusalReason::WriteOnly)
            } else if row.read_effects.contains(&ReadEffect::ClearOnRead) {
                Some(FieldPlanRefusalReason::ClearOnRead)
            } else if row
                .read_effects
                .contains(&ReadEffect::AcknowledgeStartupInterrupt)
            {
                Some(FieldPlanRefusalReason::AcknowledgeOnRead)
            } else {
                None
            }
        })
}

/// Whether a row's effect footprint overlaps the range. The footprint
/// is the union of the row's own span (`offset .. offset + len`) and,
/// for an instanced row, every instance offset's span of the row's
/// length: a per-lane latched flag whose `InstanceDimension` offsets
/// run past the base instance (CMIS 5.1 Page 12h `tuning_complete_flag_tx`
/// among them — lane byte offsets 231..238 against a one-byte row span)
/// applies its read effect at every lane's byte, so a read of any
/// lane's byte is classified against it. Bit-packed instances (every
/// offset equal to the row's own) add nothing to the footprint.
fn effect_footprint_overlaps(row: &crate::catalog::CatalogRow, range: Range) -> bool {
    let span_overlaps = |offset: u16| {
        u32::from(offset) < range.end()
            && u32::from(range.offset) < u32::from(offset) + u32::from(row.len)
    };
    span_overlaps(row.offset)
        || row.instances.as_ref().is_some_and(|dimension| {
            dimension
                .offsets
                .iter()
                .any(|&offset| span_overlaps(offset))
        })
}

/// The variant-discriminator range, when the row carries one.
fn entry_discriminator(entry: &FieldEntry<'_>) -> Option<(Range, &'static str)> {
    entry
        .row
        .discriminator
        .as_ref()
        .map(|discriminator| (discriminator.loc.range(entry.bank), discriminator.field))
}

/// The declared external-calibration constant ranges, when the row's
/// scaling declares them.
fn entry_constants(entry: &FieldEntry<'_>) -> Vec<(Range, &'static str)> {
    match &entry.row.scaling {
        Scaling::Linear {
            external: Some(external),
            ..
        } => vec![
            (external.slope.range(entry.bank), entry.row.name),
            (external.offset.range(entry.bank), entry.row.name),
        ],
        Scaling::Polynomial {
            external: Some(external),
            ..
        } => external
            .coefficients
            .iter()
            .map(|loc| (loc.range(entry.bank), entry.row.name))
            .collect(),
        _ => Vec::new(),
    }
}

/// The presence-prerequisite range, when the row carries an `Equals`
/// condition (banked per the prerequisite's own bank policy — the
/// prelude round's per-entry contribution).
fn entry_prerequisite(entry: &FieldEntry<'_>) -> Option<(Range, &'static str)> {
    match &entry.row.presence {
        FieldPresence::Equals(prerequisite) => {
            let bank = if prerequisite.banked { entry.bank } else { 0 };
            Some((prerequisite.loc.range(bank), prerequisite.field))
        }
        FieldPresence::Unconditional | FieldPresence::Unsupported { .. } => None,
    }
}

/// One resolved entry's decode dependencies, as byte ranges with purposes.
/// The variant discriminator (`decode_scaling` reads it before it can know
/// which calibration variant applies), the declared external-calibration
/// constants, and the presence prerequisite byte (a row gated on another
/// field's value reads that field's bytes first); purposes follow the
/// basic plan's convention (constants carry the row's name, the
/// discriminator its schema field path).
fn entry_dependencies(entry: &FieldEntry<'_>) -> Vec<(Range, &'static str)> {
    let mut needs = Vec::new();
    if let Some(discriminator) = entry_discriminator(entry) {
        needs.push(discriminator);
    }
    if let Some(prerequisite) = entry_prerequisite(entry) {
        needs.push(prerequisite);
    }
    needs.extend(entry_constants(entry));
    needs
}

/// The non-prerequisite decode dependencies — the variant discriminator
/// and the declared external-calibration constants — which the remaining
/// round plans for an admitted entry: the prerequisite itself belongs to
/// the prelude round and is never re-listed.
fn entry_remaining_dependencies(entry: &FieldEntry<'_>) -> Vec<(Range, &'static str)> {
    let mut needs = Vec::new();
    if let Some(discriminator) = entry_discriminator(entry) {
        needs.push(discriminator);
    }
    needs.extend(entry_constants(entry));
    needs
}

/// The pure field read plan for already-resolved entries, with the
/// default output bounds ([`FIELD_PLAN_DEFAULT_LIMITS`]).
///
/// Every range is derived from the entries' own catalog rows or the
/// decode-dependency rows named in the module documentation; nothing here
/// is a second offset table, and no module byte is read.
pub fn field_read_plan(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
) -> Result<FieldReadPlan, FieldPlanError> {
    field_read_plan_with_limits(family, revision, entries, FIELD_PLAN_DEFAULT_LIMITS)
}

/// The pure field read plan with caller-supplied output bounds. See
/// [`field_read_plan`] for the contract; oversize queries are refused
/// typed, never trimmed.
pub fn field_read_plan_with_limits(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
    limits: FieldPlanLimits,
) -> Result<FieldReadPlan, FieldPlanError> {
    validate_field_entries(family, revision, entries, limits)?;
    let catalog = family_catalog(family).ok_or(FieldPlanError::UnsupportedFamily(family))?;
    let mut refusals: Vec<FieldPlanRefusal> = Vec::new();
    let mut refused = vec![false; entries.len()];
    // Each surviving entry contributes its own range plus its decode
    // dependencies; refused entries contribute nothing (a byte shared
    // with another, surviving entry stays planned through that entry).
    let mut per_entry: Vec<Vec<(Range, &'static str)>> = vec![Vec::new(); entries.len()];
    let mut shared: Vec<(Range, &'static str)> = Vec::new();

    // Pass 1 — each entry's own bytes, classified against every catalog
    // row overlapping them at the revision.
    for (index, entry) in entries.iter().enumerate() {
        let range = entry.range();
        range.validate().map_err(FieldPlanError::Input)?;
        if let Some(reason) = restrictive_effect(catalog, revision, range) {
            refusals.push(FieldPlanRefusal {
                field: entry.row.name,
                lane: entry.lane,
                reason,
            });
            refused[index] = true;
            continue;
        }
        per_entry[index].push((range, entry.row.name));
    }

    // Pass 2 — per-entry decode dependencies. A tainted dependency
    // refuses its dependent entry: decoding it would read the effectful
    // byte.
    for (index, entry) in entries.iter().enumerate() {
        if refused[index] {
            continue;
        }
        for (need, purpose) in entry_dependencies(entry) {
            need.validate().map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, need) {
                refusals.push(FieldPlanRefusal {
                    field: entry.row.name,
                    lane: entry.lane,
                    reason,
                });
                refused[index] = true;
                per_entry[index].clear();
                break;
            }
            per_entry[index].push((need, purpose));
        }
    }

    // Pass 3 — shared decode dependencies. The SFF revision-compliance
    // gate byte (every SFF decode reads it), the CMIS revision byte
    // (every CMIS decode proves the snapshot's reported revision against
    // it), and the CMIS TxBiasCurrentScalingFactor advertisement for
    // laser-bias-scaled entries (the same test apply_bias_advertisement
    // uses). Flat-superset planning only: these additions say nothing
    // about acquisition ordering (Stage 1b owns pre-I/O admission).
    if refused.iter().any(|refused| !*refused) {
        if let Some((page, name)) = compliance_field(family) {
            let gate_entry = monitor_entry(revision, page, name).map_err(|_| {
                FieldPlanError::MonitorRowAbsent {
                    page,
                    name,
                    rev: revision,
                }
            })?;
            let gate = gate_entry.range();
            gate.validate().map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, gate) {
                // The gate byte every SFF decode reads is itself not
                // passively readable: no SFF entry can be observed.
                for (index, entry) in entries.iter().enumerate() {
                    if !refused[index] {
                        refusals.push(FieldPlanRefusal {
                            field: entry.row.name,
                            lane: entry.lane,
                            reason,
                        });
                        refused[index] = true;
                        per_entry[index].clear();
                    }
                }
            } else {
                shared.push((gate, gate_entry.row.name));
            }
        } else if family == Family::Cmis {
            let revision_entry = monitor_entry(revision, "CmisLowerMemory", "cmis_revision")
                .map_err(|_| FieldPlanError::MonitorRowAbsent {
                    page: "CmisLowerMemory",
                    name: "cmis_revision",
                    rev: revision,
                })?;
            let revision_range = revision_entry.range();
            revision_range.validate().map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, revision_range) {
                for (index, entry) in entries.iter().enumerate() {
                    if !refused[index] {
                        refusals.push(FieldPlanRefusal {
                            field: entry.row.name,
                            lane: entry.lane,
                            reason,
                        });
                        refused[index] = true;
                        per_entry[index].clear();
                    }
                }
            } else {
                shared.push((revision_range, revision_entry.row.name));
            }
        }
        let bias_dependents: Vec<usize> = (0..entries.len())
            .filter(|index| !refused[*index])
            .filter(|index| scaling_needs_bias_advertisement(&entries[*index]))
            .collect();
        if !bias_dependents.is_empty() {
            let advertisement = monitor_entry(
                revision,
                "CmisPage01hAdvertising",
                "tx_bias_current_scaling_factor",
            )
            .map_err(|_| FieldPlanError::MonitorRowAbsent {
                page: "CmisPage01hAdvertising",
                name: "tx_bias_current_scaling_factor",
                rev: revision,
            })?;
            let advertisement_range = advertisement.range();
            advertisement_range
                .validate()
                .map_err(FieldPlanError::Input)?;
            if let Some(reason) = restrictive_effect(catalog, revision, advertisement_range) {
                // The advertisement decoding a scaled laser bias needs is
                // itself effectful: those entries cannot be observed.
                for index in bias_dependents {
                    refusals.push(FieldPlanRefusal {
                        field: entries[index].row.name,
                        lane: entries[index].lane,
                        reason,
                    });
                    refused[index] = true;
                    per_entry[index].clear();
                }
            } else {
                shared.push((advertisement_range, advertisement.row.name));
            }
        }
    }

    // Assemble from the survivors only, then sort and coalesce.
    let planned: Vec<(Range, &'static str)> =
        per_entry.into_iter().flatten().chain(shared).collect();
    let reads = coalesce_reads(planned, limits)?;
    Ok(FieldReadPlan { reads, refusals })
}

/// Sort planned (range, purpose) pairs deterministically by
/// (address, page, bank, offset, len) and coalesce touching or
/// overlapping ranges inside one (address, bank, page) selection; the
/// merged purpose is the lowest-offset contributor's. Refused bytes are
/// excluded by construction, so no merged range absorbs an effectful
/// byte. The plan's range bound applies to the coalesced result.
fn coalesce_reads(
    mut planned: Vec<(Range, &'static str)>,
    limits: FieldPlanLimits,
) -> Result<Vec<PlannedRead>, FieldPlanError> {
    planned.sort_by_key(|(range, _)| {
        (
            range.address,
            range.page,
            range.bank,
            range.offset,
            range.len,
        )
    });
    let mut reads: Vec<PlannedRead> = Vec::with_capacity(planned.len());
    for (range, purpose) in planned {
        match reads.last_mut() {
            Some(last)
                if last.range.address == range.address
                    && last.range.bank == range.bank
                    && last.range.page == range.page
                    && u32::from(range.offset) <= last.range.end() =>
            {
                let merged_end = last.range.end().max(range.end());
                last.range.len = (merged_end - u32::from(last.range.offset)) as u16;
            }
            _ => reads.push(PlannedRead::new(range, purpose)),
        }
    }
    if reads.len() > limits.max_ranges {
        return Err(FieldPlanError::TooManyRanges {
            count: reads.len(),
            limit: limits.max_ranges,
        });
    }
    Ok(reads)
}

/// Decode resolved entries from a supplied view with caller-supplied
/// acquisition evidence, one [`FieldObservation`] per entry in entry order.
/// See [`observe_fields`] and [`observe_field_with`].
/// The context parameter extends the established `observe_fields` shape
/// (seven parameters) by exactly the Stage 1 evidence argument.
#[allow(clippy::too_many_arguments)]
pub fn observe_fields_with(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    options: &ObserveOptions,
    context: &AcquisitionContext<'_>,
) -> Result<Vec<FieldObservation>, FieldPlanError> {
    validate_field_entries(family, revision, entries, FIELD_PLAN_DEFAULT_LIMITS)?;
    Ok(entries
        .iter()
        .map(|entry| observe_field_with(entry, view, generation, epoch, options, context))
        .collect())
}

/// Decode resolved entries from a supplied view, one
/// [`FieldObservation`] per entry in entry order (name, lane, state,
/// value with unit, note). The same family rules as [`observe_field`]
/// apply per entry — SFF compliance admission, the write-only refusal
/// and the CMIS bias advertisement — so this bulk path cannot bypass
/// them, and an entry the field read plan refused for write-only bytes
/// surfaces here as [`ObservationState::WriteOnlyRefused`] with no
/// value. View-level problems are per-entry states, never zero-filled.
/// Stage 1 admission (reported revision, presence prerequisites,
/// selector-content evidence) applies per entry; this entry point
/// behaves exactly as with an empty acquisition context.
pub fn observe_fields(
    family: Family,
    revision: Rev,
    entries: &[FieldEntry<'_>],
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    options: &ObserveOptions,
) -> Result<Vec<FieldObservation>, FieldPlanError> {
    observe_fields_with(
        family,
        revision,
        entries,
        view,
        generation,
        epoch,
        options,
        &AcquisitionContext::empty(),
    )
}
