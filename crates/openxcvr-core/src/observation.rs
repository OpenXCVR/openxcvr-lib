//! Basic observation decoding from supplied byte snapshots. No I/O, no
//! clocks, no device: every byte comes from a caller-supplied [`ByteView`].
//!
//! Values are typed (numeric plus unit), never pre-formatted strings. Every
//! result carries an explicit validity state: missing bytes are
//! [`ObservationState::NotSupplied`], never zero; a clear-on-read field is
//! refused unless the caller authorizes it; a write-only field is refused
//! outright, because a write-only read is undefined and a guess is never
//! reported as a fact; an externally calibrated module
//! whose constants are missing from the snapshot is
//! [`ObservationState::Uncalibrated`], never a raw value presented as
//! calibrated.
//!
//! Stage 1 admission (supplied-data): before any value is produced, the
//! decode proves the snapshot's own reported revision (families whose
//! catalog declares a [`crate::field_catalog::RevisionProbe`] — CMIS lower
//! memory byte 1), the field's presence prerequisites (schema `present_when`
//! equality conditions), and — for rows whose prerequisite multiplexes a
//! data window's content (Page 14h diagnostics) — a caller-asserted
//! [`SelectorAcquisition`]. Admission refusals are typed outcomes with no
//! value; the context-free entry points behave as with an empty context
//! and refuse content-multiplexed rows.
//!
//! External calibration (SFF-8472 §§ 9.3, 9.5): the linear slope/offset and
//! the Rx-power polynomial both produce results in the parameter's internal
//! LSB units, which are then scaled by the internal LSB — the convention the
//! schemas' `calibration.v1` document states for the linear constants.
use crate::cmis_f16::CmisF16;
use crate::field_catalog::{
    FieldEntry, FieldKind, FieldLoc, PrerequisiteEquals, ScaledLsb, Scaling, Unit,
    VariantDiscriminator,
};
use crate::rev::Rev;
use crate::types::management::{
    supplied_bytes, ByteView, CacheEpoch, InputError, ModuleGeneration, Range, ReadEffect,
};

/// A decoded value: either a scaled quantity or the raw code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldValue {
    /// A physical quantity already scaled into `unit`.
    Scaled {
        /// The decoded value in `unit`'s physical units.
        value: f64,
        /// The unit `value` is expressed in.
        unit: Unit,
    },
    /// The raw encoded code, for rows the schema does not scale.
    Raw(u64),
}

/// Why a field has (or has not) a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationState {
    /// Decoded from the supplied bytes.
    Valid,
    /// The field's bytes are absent from the snapshot (for example an
    /// optional page that was not supplied).
    NotSupplied,
    /// The module is externally calibrated and the calibration constants are
    /// missing from the snapshot.
    Uncalibrated,
    /// The field does not exist at the revision being decoded.
    UnsupportedRevision(Rev),
    /// The field is clear-on-read and the caller did not authorize reading
    /// it (see [`ObserveOptions::authorize_clear_on_read`]).
    ClearOnReadRefused,
    /// The field is write-only, or shares a byte with a write-only field:
    /// reading the location returns undefined or vendor-specific bytes, so
    /// no value is decoded — a guess is never reported as a fact. Set by
    /// the facade's observation layer, which classifies entries against
    /// the catalog exactly as its field read plan does.
    WriteOnlyRefused,
    /// Supplied bytes exist but cannot be decoded honestly (for example a
    /// reserved multiplier advertisement).
    Invalid,
    /// The field's kind or encoding has no numeric observation on this
    /// surface (ascii, checksums, composites, ...).
    UnsupportedField,
}

/// One field's observation.
/// One field's decode outcome: the field, its lane, an explicit validity
/// state, the decoded value when valid, and a static caveat when one applies.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldObservation {
    /// Effective schema name at the decoded revision.
    pub field: &'static str,
    /// Lane index for per-lane fields.
    pub lane: Option<u16>,
    /// Why the field has (or has not) a value; `None` is never zero.
    pub state: ObservationState,
    /// The decoded value, present only when `state` is valid.
    pub value: Option<FieldValue>,
    /// Static caveat qualifying the value, when one applies.
    pub note: Option<&'static str>,
}

impl FieldObservation {
    fn state(field: &'static str, state: ObservationState) -> Self {
        Self {
            field,
            lane: None,
            state,
            value: None,
            note: None,
        }
    }
}

/// Optical power presentation outcome, separate from the source measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpticalDbmValue {
    /// Finite power in dBm (reference: one milliwatt).
    Value(f64),
    /// No value because the result's observation state is not `Valid`.
    Unavailable,
    /// A `Valid` observation without a scaled Watt value (including `None`).
    NotConvertible,
    /// Exactly zero Watts (including negative zero); not a finite dBm value.
    NoPower,
}

/// A derived display quantity borrowing its unchanged source observation.
///
/// `state` passes through the source state, except that a negative or
/// non-finite Watt value in a `Valid` observation becomes `Invalid`.
/// Only [`OpticalDbmValue::Value`] contains a number, always finite.
#[derive(Debug, Clone, Copy, PartialEq)]
#[must_use]
pub struct OpticalDbm<'a> {
    /// Original field, lane, Watt value, state and caveat remain available.
    pub source: &'a FieldObservation,
    /// The presentation state (may differ from the source state; see the
    /// struct docs).
    pub state: ObservationState,
    /// The dBm value, present only in [`OpticalDbmValue::Value`].
    pub value: OpticalDbmValue,
}

/// Convert only a `Valid`, scaled [`Unit::Watt`] observation to dBm.
///
/// Pure, allocation-free, no I/O: `10 * log10(W * 1000)`, evaluated as
/// `10 * (log10(W) + 3)` to avoid intermediate overflow for large finite W.
/// No field-name or lane inference is performed; the caller selects the
/// optical observation. Already-calibrated W values are consumed as-is.
///
/// Non-`Valid` states take precedence over any value and pass through with
/// `Unavailable`. A `Valid` raw, missing or non-Watt value is `NotConvertible`.
/// Zero W (either sign) is `NoPower`, never negative infinity or a made-up
/// floor. Negative or non-finite W is `Invalid`/`Unavailable`. Positive W,
/// including subnormal values and values below 1 uW, is converted without
/// clamping. The derived result never contains a non-finite number; the
/// borrowed source is not sanitized or changed.
#[must_use = "the derived dBm value is a pure calculation over the supplied observation"]
pub fn optical_power_dbm(source: &FieldObservation) -> OpticalDbm<'_> {
    let mut result = OpticalDbm {
        source,
        state: source.state,
        value: OpticalDbmValue::Unavailable,
    };
    if source.state != ObservationState::Valid {
        return result;
    }
    let Some(FieldValue::Scaled {
        value: watts,
        unit: Unit::Watt,
    }) = source.value
    else {
        result.value = OpticalDbmValue::NotConvertible;
        return result;
    };
    if !watts.is_finite() || watts < 0.0 {
        result.state = ObservationState::Invalid;
    } else if watts == 0.0 {
        result.value = OpticalDbmValue::NoPower;
    } else {
        let dbm = 10.0 * (watts.log10() + 3.0);
        if dbm.is_finite() {
            result.value = OpticalDbmValue::Value(dbm);
        } else {
            result.state = ObservationState::Invalid;
        }
    }
    result
}

/// Caller options for observation decoding.
#[derive(Debug, Clone, Copy, Default)]
pub struct ObserveOptions {
    /// Authorize decoding clear-on-read fields. The default is to refuse:
    /// catalog resolution of such a field is fine, but decoding one from a
    /// snapshot silently destroys its latched state.
    pub authorize_clear_on_read: bool,
}

/// An acquisition owner's assertion that one selector-controlled data
/// window was read under a settled, selector-stable acquisition: for that
/// generation/epoch/bank, the selector value was in force, writes were
/// serialized with the read, and the `data_range` read began after the
/// revision's content-switch bound (CMIS 4.0: within the advertised tNACK
/// after the selector write; CMIS 5.0+: after a tDDCS guard period, at
/// most 10 ms — CMIS 5.3 Table 10-5).
///
/// Trusted caller evidence, not library-measured timing: the library
/// performs no write, wait or timing check; it only matches the assertion
/// against the supplied bytes and the row's expectation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectorAcquisition {
    /// The module generation the acquisition was taken under.
    pub generation: ModuleGeneration,
    /// The cache epoch the acquisition was taken in.
    pub epoch: CacheEpoch,
    /// Page-window address of the selector.
    pub address: u8,
    /// Bank the acquisition read (for banked pages).
    pub bank: u8,
    /// Page selector byte of the window.
    pub page: u8,
    /// Window offset of the selector byte (for example 128 for Page 14h).
    pub selector_offset: u16,
    /// The selector value the acquisition held in force.
    pub selector_value: u8,
    /// The data window the acquisition read (must cover the observed
    /// row's own bytes for the assertion to apply).
    pub data_range: Range,
}

/// Acquisition evidence supplied by the caller: zero or more selector
/// acquisitions. Decoding a content-multiplexed row with no matching
/// evidence yields [`ObservationState::NotSupplied`] — the existing
/// context-free entry points behave exactly as with an empty context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquisitionContext<'a> {
    /// The caller-supplied acquisition evidence, matched against the decoded
    /// row's expectations.
    pub acquisitions: &'a [SelectorAcquisition],
}

impl<'a> AcquisitionContext<'a> {
    /// The empty context: no acquisition evidence at all.
    pub const fn empty() -> Self {
        AcquisitionContext { acquisitions: &[] }
    }
}

impl Default for AcquisitionContext<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

fn loc_bytes(
    loc: &FieldLoc,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    bank: u8,
) -> Result<Vec<u8>, InputError> {
    supplied_bytes(view, loc.range(bank), generation, epoch)
}

fn field_bytes(
    entry: &FieldEntry<'_>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<Vec<u8>, InputError> {
    supplied_bytes(view, entry.range(), generation, epoch)
}

/// Extract the raw unsigned bit pattern of a field from its bytes.
fn raw_bits(bytes: &[u8], entry: &FieldEntry<'_>) -> Option<u64> {
    raw_bits_at(
        entry.row.kind,
        entry.bit_offset,
        entry.row.bit_width,
        entry.row.len,
        bytes,
    )
}

/// Extract a raw unsigned code at a concrete location shape: the same
/// extraction `raw_bits` applies to a field entry, shared with presence
/// prerequisites so a prerequisite is read exactly as the field it names
/// would be.
fn raw_bits_at(
    kind: FieldKind,
    bit_offset: u8,
    bit_width: u16,
    len: u16,
    bytes: &[u8],
) -> Option<u64> {
    let bit_offset = usize::from(bit_offset);
    match kind {
        FieldKind::Bool
        | FieldKind::U2
        | FieldKind::U3
        | FieldKind::U4
        | FieldKind::U5
        | FieldKind::U6
        | FieldKind::U7
        | FieldKind::U8
        | FieldKind::I8 => {
            let byte = *bytes.first()? as u64;
            let width = usize::from(bit_width.min(8)).clamp(1, 8);
            Some((byte >> bit_offset) & ((1u64 << width) - 1))
        }
        // Enum refs carry their raw code at the declared width.
        FieldKind::EnumRef => match len {
            2 => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(u64::from(u16::from_be_bytes([bytes[0], bytes[1]])))
            }
            _ => {
                let byte = *bytes.first()? as u64;
                let width = usize::from(bit_width.min(8)).clamp(1, 8);
                Some((byte >> bit_offset) & ((1u64 << width) - 1))
            }
        },
        FieldKind::U16Be | FieldKind::F16Be | FieldKind::X16Be => {
            if bytes.len() < 2 {
                return None;
            }
            Some(u64::from(u16::from_be_bytes([bytes[0], bytes[1]])))
        }
        FieldKind::U16Le => {
            if bytes.len() < 2 {
                return None;
            }
            Some(u64::from(u16::from_le_bytes([bytes[0], bytes[1]])))
        }
        FieldKind::I16Be => {
            if bytes.len() < 2 {
                return None;
            }
            Some(u64::from(u16::from_be_bytes([bytes[0], bytes[1]])))
        }
        FieldKind::U32Be | FieldKind::I32Be => {
            if bytes.len() < 4 {
                return None;
            }
            Some(u64::from(u32::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
            ])))
        }
        FieldKind::U64Le => {
            if bytes.len() < 8 {
                return None;
            }
            let mut le = [0u8; 8];
            le.copy_from_slice(&bytes[..8]);
            Some(u64::from_le_bytes(le))
        }
        FieldKind::U64Be => {
            if bytes.len() < 8 {
                return None;
            }
            let mut be = [0u8; 8];
            be.copy_from_slice(&bytes[..8]);
            Some(u64::from_be_bytes(be))
        }
        // Variable-length or structured kinds have no numeric observation.
        FieldKind::Ascii | FieldKind::Bytes | FieldKind::Bitmap | FieldKind::Composite => None,
    }
}

/// Extract a presence prerequisite's raw code from its supplied bytes:
/// the same extraction the decoder's presence admission applies when it
/// reads an `Equals` condition, exposed so planning surfaces read a
/// prerequisite exactly as the field it names would be read, instead of
/// re-deriving the bit logic. `None` when the prerequisite's kind has no
/// scalar code (variable-length or structured kinds).
pub fn prerequisite_raw_code(prereq: &PrerequisiteEquals, bytes: &[u8]) -> Option<u64> {
    raw_bits_at(
        prereq.kind,
        prereq.bit_offset,
        prereq.bit_width,
        prereq.loc.len,
        bytes,
    )
}

fn sign_extend(raw: u64, bits: u16) -> i64 {
    let bits = u32::from(bits).clamp(1, 64);
    if bits == 64 {
        return raw as i64;
    }
    let shift = 64 - bits;
    ((raw << shift) as i64) >> shift
}

fn scaled_from_raw(raw: u64, lsb: &ScaledLsb, entry: &FieldEntry<'_>) -> FieldValue {
    let magnitude = if lsb.signed {
        sign_extend(raw, entry.row.bit_width) as f64
    } else {
        raw as f64
    };
    FieldValue::Scaled {
        value: magnitude * lsb.lsb(),
        unit: lsb.unit,
    }
}

fn discriminator_is_external(
    discriminator: &VariantDiscriminator,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    bank: u8,
) -> Result<bool, InputError> {
    let bytes = loc_bytes(&discriminator.loc, view, generation, epoch, bank)?;
    let byte = *bytes.first().ok_or(InputError::InsufficientInput {
        range: discriminator.loc.range(bank),
    })?;
    Ok(byte & (1 << discriminator.bit) != 0)
}

/// Supplied bytes at a location, `None` when not supplied. Other input
/// errors (invalid view, stale stamps) propagate.
fn opt_loc_bytes(
    loc: &FieldLoc,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    bank: u8,
) -> Result<Option<Vec<u8>>, InputError> {
    match loc_bytes(loc, view, generation, epoch, bank) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(InputError::InsufficientInput { .. }) => Ok(None),
        Err(other) => Err(other),
    }
}

fn external_linear_bytes(
    external: &crate::field_catalog::ExternalLinear,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    bank: u8,
) -> Result<Option<(u16, i16)>, InputError> {
    let slope = opt_loc_bytes(&external.slope, view, generation, epoch, bank)?;
    let offset = opt_loc_bytes(&external.offset, view, generation, epoch, bank)?;
    match (slope, offset) {
        (Some(slope), Some(offset)) if slope.len() >= 2 && offset.len() >= 2 => Ok(Some((
            u16::from_be_bytes([slope[0], slope[1]]),
            i16::from_be_bytes([offset[0], offset[1]]),
        ))),
        (Some(_), Some(_)) => Err(InputError::InvalidLength(external.slope.range(bank))),
        _ => Ok(None),
    }
}

fn external_poly_bytes(
    external: &crate::field_catalog::ExternalPoly,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    bank: u8,
) -> Result<Option<[f64; 5]>, InputError> {
    let mut coefficients = [0f64; 5];
    for (slot, loc) in external.coefficients.iter().enumerate() {
        let Some(bytes) = opt_loc_bytes(loc, view, generation, epoch, bank)? else {
            return Ok(None);
        };
        if bytes.len() < 4 {
            return Err(InputError::InvalidLength(loc.range(bank)));
        }
        let mut be = [0u8; 4];
        be.copy_from_slice(&bytes[..4]);
        coefficients[slot] = f64::from(f32::from_be_bytes(be));
    }
    Ok(Some(coefficients))
}

fn decode_scaling(
    entry: &FieldEntry<'_>,
    raw: u64,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> FieldObservation {
    let field = entry.row.name;
    let mut observation = FieldObservation::state(field, ObservationState::Valid);
    observation.lane = entry.lane;
    let scaling = &entry.row.scaling;
    match scaling {
        Scaling::None => {
            observation.value = Some(FieldValue::Raw(raw));
            observation
        }
        Scaling::Scaled(lsb) => {
            observation.value = Some(scaled_from_raw(raw, lsb, entry));
            observation.note = lsb.note;
            observation
        }
        Scaling::Linear { internal, external } => {
            let Some(discriminator) = &entry.row.discriminator else {
                observation.value = Some(scaled_from_raw(raw, internal, entry));
                return observation;
            };
            let external_active =
                match discriminator_is_external(discriminator, view, generation, epoch, entry.bank)
                {
                    Ok(active) => active,
                    Err(InputError::InsufficientInput { .. }) => {
                        observation.state = ObservationState::NotSupplied;
                        return observation;
                    }
                    Err(_) => {
                        observation.state = ObservationState::Invalid;
                        return observation;
                    }
                };
            if !external_active {
                observation.value = Some(scaled_from_raw(raw, internal, entry));
                observation.note = internal.note;
                return observation;
            }
            let Some(external) = external else {
                observation.state = ObservationState::UnsupportedField;
                return observation;
            };
            match external_linear_bytes(external, view, generation, epoch, entry.bank) {
                Ok(Some((slope, offset))) => {
                    // Calibrated result stays in the parameter's internal
                    // LSB units; the internal LSB then scales it.
                    let raw_value = if internal.signed {
                        sign_extend(raw, entry.row.bit_width) as f64
                    } else {
                        raw as f64
                    };
                    let calibrated = (f64::from(slope) / 256.0) * raw_value + f64::from(offset);
                    observation.value = Some(FieldValue::Scaled {
                        value: calibrated * internal.lsb(),
                        unit: internal.unit,
                    });
                    observation
                }
                Ok(None) => {
                    observation.state = ObservationState::Uncalibrated;
                    observation
                }
                Err(_) => {
                    observation.state = ObservationState::Invalid;
                    observation
                }
            }
        }
        Scaling::Polynomial { internal, external } => {
            let Some(discriminator) = &entry.row.discriminator else {
                observation.value = Some(scaled_from_raw(raw, internal, entry));
                return observation;
            };
            let external_active =
                match discriminator_is_external(discriminator, view, generation, epoch, entry.bank)
                {
                    Ok(active) => active,
                    Err(InputError::InsufficientInput { .. }) => {
                        observation.state = ObservationState::NotSupplied;
                        return observation;
                    }
                    Err(_) => {
                        observation.state = ObservationState::Invalid;
                        return observation;
                    }
                };
            if !external_active {
                observation.value = Some(scaled_from_raw(raw, internal, entry));
                observation.note = internal.note;
                return observation;
            }
            let Some(external) = external else {
                observation.state = ObservationState::UnsupportedField;
                return observation;
            };
            match external_poly_bytes(external, view, generation, epoch, entry.bank) {
                Ok(Some(coefficients)) => {
                    // coefficients[i] is the degree-i term, matching the
                    // schema's reference order; the result is in internal
                    // LSB units.
                    let r = raw as f64;
                    let poly = coefficients[4] * r * r * r * r
                        + coefficients[3] * r * r * r
                        + coefficients[2] * r * r
                        + coefficients[1] * r
                        + coefficients[0];
                    observation.value = Some(FieldValue::Scaled {
                        value: poly * internal.lsb(),
                        unit: internal.unit,
                    });
                    observation
                }
                Ok(None) => {
                    observation.state = ObservationState::Uncalibrated;
                    observation
                }
                Err(_) => {
                    observation.state = ObservationState::Invalid;
                    observation
                }
            }
        }
        Scaling::Other { kind } => {
            observation.state = ObservationState::UnsupportedField;
            observation.note = Some(kind);
            observation
        }
        Scaling::F16 { unit } => {
            // F16 scaling is meaningful only on a two-byte F16Be row whose
            // raw code is the 16-bit F16 code — never a narrowed
            // reinterpretation of an arbitrary u64. Any other shape is a
            // row/catalog mismatch this surface states instead of decoding.
            if entry.row.kind != FieldKind::F16Be || entry.row.len != 2 {
                observation.state = ObservationState::UnsupportedField;
                observation.note = Some(NOTE_F16_SCALING_MISMATCH);
                return observation;
            }
            let Ok(code) = u16::try_from(raw) else {
                observation.state = ObservationState::UnsupportedField;
                observation.note = Some(NOTE_F16_SCALING_MISMATCH);
                return observation;
            };
            observation.value = Some(FieldValue::Scaled {
                value: CmisF16::from_raw(code).to_f64(),
                unit: *unit,
            });
            observation
        }
    }
}

/// Decode one resolved field from the supplied view.
///
/// Clear-on-read fields are refused unless `options` authorizes them.
/// Missing bytes are [`ObservationState::NotSupplied`]; view-level problems
/// (overlapping observations, stale stamps) are
/// [`ObservationState::Invalid`]; nothing is ever zero-filled.
///
/// Stage 1 admission, applied before any value is produced:
/// - reported revision (families whose catalog declares a probe, CMIS:
///   lower memory byte 1): the byte must be supplied and name exactly the
///   entry's revision — otherwise `NotSupplied` (byte absent) or
///   `UnsupportedRevision(reported)`;
/// - presence prerequisites: an `Equals` condition's referenced field must
///   be supplied and carry the expected raw code (`NotSupplied` /
///   `Invalid`); an `Unsupported` condition yields `UnsupportedField`;
/// - selector-content evidence: a prerequisite that carries content-switch
///   metadata additionally requires a matching [`SelectorAcquisition`]
///   (absent → `NotSupplied`; contradicting the snapshot or not selecting
///   the row's view → `Invalid`).
///
/// This entry point behaves exactly as [`AcquisitionContext::empty`] context:
/// content-multiplexed rows decode only through
/// [`observe_field_with`].
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

/// The static notes Stage 1 admission distinguishes. Notes are static
/// strings: the numeric classification (00h, reserved, custom, other view)
/// is asserted by tests; every mismatch is `Invalid` with no value.
const NOTE_SELECTOR_EVIDENCE_MISSING: &str = "selector acquisition evidence not supplied";
const NOTE_SELECTOR_EVIDENCE_CONTRADICTS: &str = "selector evidence contradicts snapshot";
const NOTE_VIEW_NOT_SELECTED: &str = "view not selected by selector";
const NOTE_PREREQUISITE_NOT_SATISFIED: &str = "presence prerequisite not satisfied";
const NOTE_REVISION_BYTE_MISSING: &str = "CMIS revision byte not supplied; revision undetermined";
const NOTE_REVISION_REPORTED_OTHER: &str =
    "revision byte reports a different revision than requested";
const NOTE_REVISION_UNKNOWN: &str = "revision byte reports an unknown revision";

/// The static note every F16-scaling row-shape mismatch carries (the C
/// runtime shares the wording).
const NOTE_F16_SCALING_MISMATCH: &str = "f16 scaling requires a two-byte f16be row";

/// Reported-revision admission (B3): the probe byte must be supplied and
/// name exactly the entry's revision. `Ok(())` admits; otherwise the
/// refusal is returned as a ready observation.
fn revision_admission<'a>(
    entry: &FieldEntry<'a>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
) -> Result<(), FieldObservation> {
    let Some(probe) = entry.revision_probe else {
        return Ok(());
    };
    let mut refusal = FieldObservation::state(entry.row.name, ObservationState::Valid);
    refusal.lane = entry.lane;
    // The probe's bank policy is its own page's — the modeled probes live
    // in unbanked lower memory, so bank 0 regardless of the entry's bank.
    let bytes = match supplied_bytes(view, probe.loc.range(0), generation, epoch) {
        Ok(bytes) => bytes,
        Err(InputError::InsufficientInput { .. }) => {
            refusal.state = ObservationState::NotSupplied;
            refusal.note = Some(NOTE_REVISION_BYTE_MISSING);
            return Err(refusal);
        }
        Err(_) => {
            refusal.state = ObservationState::Invalid;
            refusal.note = Some("revision byte could not be read");
            return Err(refusal);
        }
    };
    let byte = bytes[0];
    match Rev::from_major_byte_cmis(byte) {
        Some(detected) if detected == entry.rev => Ok(()),
        Some(detected) => {
            refusal.state = ObservationState::UnsupportedRevision(detected);
            refusal.note = Some(NOTE_REVISION_REPORTED_OTHER);
            Err(refusal)
        }
        None => {
            refusal.state = ObservationState::UnsupportedRevision(Rev {
                family: entry.rev.family,
                major: byte >> 4,
                minor: byte & 0x0f,
            });
            refusal.note = Some(NOTE_REVISION_UNKNOWN);
            Err(refusal)
        }
    }
}

/// Whether one acquisition assertion covers a row: it must name the same
/// generation/epoch, the selector's (address, bank, page, offset), and a
/// data range covering the row's own bytes.
fn evidence_covers(
    entry: &FieldEntry<'_>,
    prereq: &PrerequisiteEquals,
    evidence: &SelectorAcquisition,
) -> bool {
    let bank = if prereq.banked { entry.bank } else { 0 };
    let row = entry.range();
    evidence.address == prereq.loc.address
        && evidence.bank == bank
        && evidence.page == prereq.loc.page
        && evidence.selector_offset == prereq.loc.offset
        && evidence.data_range.address == row.address
        && evidence.data_range.bank == row.bank
        && evidence.data_range.page == row.page
        && u32::from(evidence.data_range.offset) <= u32::from(row.offset)
        && evidence.data_range.end() >= row.end()
}

/// Presence admission (B1/B2). `Ok(())` admits; otherwise the refusal is
/// returned as a ready observation.
fn presence_admission(
    entry: &FieldEntry<'_>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    context: &AcquisitionContext<'_>,
) -> Result<(), FieldObservation> {
    let mut refusal = FieldObservation::state(entry.row.name, ObservationState::Valid);
    refusal.lane = entry.lane;
    let prereq = match &entry.row.presence {
        crate::field_catalog::FieldPresence::Unconditional => return Ok(()),
        crate::field_catalog::FieldPresence::Unsupported { expr } => {
            refusal.state = ObservationState::UnsupportedField;
            refusal.note = Some(expr);
            return Err(refusal);
        }
        crate::field_catalog::FieldPresence::Equals(prereq) => prereq,
    };
    // Content-multiplexed rows: missing evidence comes first, before the
    // selector value is even compared (the all-rows-NotSupplied contract
    // for a context-free decode).
    if let Some(switch) = &prereq.content_switch {
        let matching: Vec<&SelectorAcquisition> = context
            .acquisitions
            .iter()
            .filter(|evidence| {
                evidence.generation == generation
                    && evidence.epoch == epoch
                    && evidence_covers(entry, prereq, evidence)
            })
            .collect();
        if matching.is_empty() {
            refusal.state = ObservationState::NotSupplied;
            refusal.note = Some(NOTE_SELECTOR_EVIDENCE_MISSING);
            return Err(refusal);
        }
        // Read the snapshot's selector byte to cross-check the assertion.
        let bank = if prereq.banked { entry.bank } else { 0 };
        let selector = match supplied_bytes(view, prereq.loc.range(bank), generation, epoch) {
            Ok(bytes) => bytes,
            Err(InputError::InsufficientInput { .. }) => {
                refusal.state = ObservationState::NotSupplied;
                return Err(refusal);
            }
            Err(_) => {
                refusal.state = ObservationState::Invalid;
                refusal.note = Some("selector byte could not be read");
                return Err(refusal);
            }
        };
        let snapshot_value = match raw_bits_at(
            prereq.kind,
            prereq.bit_offset,
            prereq.bit_width,
            prereq.loc.len,
            &selector,
        ) {
            Some(raw) => u8::try_from(raw).unwrap_or(u8::MAX),
            None => {
                refusal.state = ObservationState::Invalid;
                refusal.note = Some("selector byte could not be decoded");
                return Err(refusal);
            }
        };
        // A matching assertion that disagrees with the snapshot is a
        // contradiction, never ignored in favour of another record.
        if matching
            .iter()
            .any(|evidence| evidence.selector_value != snapshot_value)
        {
            refusal.state = ObservationState::Invalid;
            refusal.note = Some(NOTE_SELECTOR_EVIDENCE_CONTRADICTS);
            return Err(refusal);
        }
        // The snapshot selector byte (asserted stable) must select this
        // row's view: 00h, reserved, custom and other standard views are
        // all Invalid with no value — a zero-filled window never becomes a
        // zero BER.
        let expected = u8::try_from(prereq.expected).unwrap_or(u8::MAX);
        if snapshot_value != expected {
            refusal.state = ObservationState::Invalid;
            refusal.note = Some(NOTE_VIEW_NOT_SELECTED);
            return Err(refusal);
        }
        let _ = switch;
        return Ok(());
    }
    // Plain prerequisite (read-only advertisement/capability): no evidence
    // involved — the referenced bytes decide.
    let bank = if prereq.banked { entry.bank } else { 0 };
    let bytes = match supplied_bytes(view, prereq.loc.range(bank), generation, epoch) {
        Ok(bytes) => bytes,
        Err(InputError::InsufficientInput { .. }) => {
            refusal.state = ObservationState::NotSupplied;
            return Err(refusal);
        }
        Err(_) => {
            refusal.state = ObservationState::Invalid;
            refusal.note = Some("presence prerequisite could not be read");
            return Err(refusal);
        }
    };
    match raw_bits_at(
        prereq.kind,
        prereq.bit_offset,
        prereq.bit_width,
        prereq.loc.len,
        &bytes,
    ) {
        Some(raw) if raw == prereq.expected => Ok(()),
        Some(_) => {
            refusal.state = ObservationState::Invalid;
            refusal.note = Some(NOTE_PREREQUISITE_NOT_SATISFIED);
            Err(refusal)
        }
        None => {
            refusal.state = ObservationState::Invalid;
            refusal.note = Some("presence prerequisite could not be decoded");
            Err(refusal)
        }
    }
}

/// Decode one resolved field from the supplied view with caller-supplied
/// acquisition evidence. See [`observe_field`] for the shared contract;
/// [`SelectorAcquisition`] documents what the caller asserts and the
/// library checks.
pub fn observe_field_with(
    entry: &FieldEntry<'_>,
    view: &impl ByteView,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    options: &ObserveOptions,
    context: &AcquisitionContext<'_>,
) -> FieldObservation {
    let field = entry.row.name;
    let mut observation = FieldObservation::state(field, ObservationState::Valid);
    observation.lane = entry.lane;
    // Admission order: reported revision, then presence (evidence before
    // selector comparison), then the existing clear-on-read gate, then the
    // field's own bytes. A refused admission produces no value.
    if let Err(refused) = revision_admission(entry, view, generation, epoch) {
        return refused;
    }
    if let Err(refused) = presence_admission(entry, view, generation, epoch, context) {
        return refused;
    }
    if entry.row.read_effects.contains(&ReadEffect::ClearOnRead) && !options.authorize_clear_on_read
    {
        observation.state = ObservationState::ClearOnReadRefused;
        return observation;
    }
    let bytes = match field_bytes(entry, view, generation, epoch) {
        Ok(bytes) => bytes,
        Err(InputError::InsufficientInput { .. }) => {
            observation.state = ObservationState::NotSupplied;
            return observation;
        }
        Err(_) => {
            observation.state = ObservationState::Invalid;
            return observation;
        }
    };
    let Some(raw) = raw_bits(&bytes, entry) else {
        observation.state = ObservationState::UnsupportedField;
        return observation;
    };
    decode_scaling(entry, raw, view, generation, epoch)
}
