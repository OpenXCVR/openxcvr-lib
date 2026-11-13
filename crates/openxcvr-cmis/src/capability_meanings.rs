//! Pure, positively allowlisted CMIS **5.3** capability meanings.
//!
//! Authority: OIF-CMIS-05.3 printed/PDF pp.171,173, Tables 8-46 and 8-49.
//! This is selected software scope, not a measured live-device revision.
//! Names come exclusively from generated schema enums. The enums deliberately
//! have no field `enum_ref`: existing generated numeric page fields, setters
//! and raw observations remain unchanged at every revision.
use crate::generated::{CmisBanksSupported, CmisTxInputClockingCapabilities};
use openxcvr_core::{
    field_catalog::FieldEntry,
    observation::{FieldObservation, FieldValue, ObservationState},
    rev::Rev,
};

/// An optional interpretation borrowing the unchanged observation and retaining
/// its resolved field/family/revision/range. No acquisition or I/O authority.
#[derive(Debug, Clone, Copy)]
pub struct CapabilityMeaning<'a> {
    /// The resolved field entry the interpretation belongs to.
    pub entry: FieldEntry<'a>,
    /// The observation the interpretation borrows.
    pub source: &'a FieldObservation,
    /// Correlated, extracted two-bit code (not the enclosing register byte).
    pub raw: u8,
    /// Concise schema-generated label (1..=64 bytes).
    pub name: &'static str,
    /// Exact interpretation revision, not a family-wide qualification.
    pub revision: &'static str,
    /// Applicability only; not selector permission or a measured capability.
    pub note: &'static str,
}

const BANK_NOTE: &str = "Only lane-banked pages 10h-2Fh in 8-lane groups; not general/CDB banking. Reserved defines no bank count.";
const CLOCK_NOTE: &str = "Per 8-lane bank; across apps: default or, if meaningful, highest capabilities/strongest restrictions. App-standard tolerance.";

/// Supply a meaning only for the two exact CMIS 5.3 fields, a `Valid`
/// module-level observation and an agreeing raw two-bit code/value.
///
/// The caller supplies a catalog-resolved entry and its observation. Identity,
/// family, exact revision, lane and raw/value correlation are checked here;
/// this is not proof of provenance for manually constructed inputs. Missing,
/// non-valid, scaled, mismatched and out-of-width values return `None`, never
/// a reserved/unknown label. Bank code 3 is reserved; clock code 3 is defined.
///
/// Pure and allocation-free: no module, transport, ByteView, memory offsets,
/// acquisition, clock, selector, firmware or password operation is involved.
/// The source value, range, state, revision and note are never changed.
pub fn cmis53_capability_meaning<'a>(
    entry: &FieldEntry<'a>,
    source: &'a FieldObservation,
    raw: Option<u64>,
) -> Option<CapabilityMeaning<'a>> {
    if entry.rev != Rev::CMIS_5_3
        || source.state != ObservationState::Valid
        || source.field != entry.name()
        || source.lane.is_some()
        || entry.lane.is_some()
        || entry.width_bits() != 2
    {
        return None;
    }
    let raw = raw?;
    if raw > 3 || source.value != Some(FieldValue::Raw(raw)) {
        return None;
    }
    let raw = raw as u8;
    let revision = "5.3";
    let (name, note) = match entry.name() {
        "banks_supported" => (CmisBanksSupported::for_revision(0x53)?(raw)?, BANK_NOTE),
        "tx_input_clocking_capabilities" => (
            CmisTxInputClockingCapabilities::for_revision(0x53)?(raw)?,
            CLOCK_NOTE,
        ),
        _ => return None,
    };
    Some(CapabilityMeaning {
        entry: *entry,
        source,
        raw,
        name,
        revision,
        note,
    })
}
