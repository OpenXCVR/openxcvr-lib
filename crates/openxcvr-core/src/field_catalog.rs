//! Schema-derived field catalog: resolution by exact name, schema alias,
//! glob and lane, revision-aware, over generated per-family tables.
//!
//! The rows are generated from the page schemas by `openxcvr-codegen`
//! (`emitters::rust::catalog`); nothing in this module encodes family memory
//! maps. A row describes one revision span of one field: `since`/`until`
//! bound the revisions in which the row applies (`since` inclusive, `until`
//! exclusive) and `name` is the schema-effective field name inside that span,
//! so renames and aliases resolve as plain name lookups.
use crate::rev::{Family, Rev};
use crate::types::management::{ChangeSource, Range, ReadEffect};

/// Field permission vocabulary, mirroring the schema `access:` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    /// Read-only: the module implements reads, and writes are refused.
    Ro,
    /// Read-write: plain writes are permitted.
    Rw,
    /// Read-write with write sequences: writes require the schema's
    /// write-sequence protocol rather than a plain store.
    Rww,
    /// Write-only: reads are refused.
    Wo,
    /// Write-only, self-clearing: the bit clears itself once acted on.
    WoSc,
    /// Read-only, cleared on read: a read has a side effect on the value.
    RoCor,
    /// The schema declared no recognised access value; fail closed on
    /// admission decisions rather than guessing.
    Unknown,
}

/// Wire type of a field, mirroring the schema `type:` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// One bit, `0` = false, non-zero = true.
    Bool,
    /// Unsigned integer, 2 bits wide, right-aligned in its byte.
    U2,
    /// Unsigned integer, 3 bits wide, right-aligned in its byte.
    U3,
    /// Unsigned integer, 4 bits wide (a nibble).
    U4,
    /// Unsigned integer, 5 bits wide, right-aligned in its byte.
    U5,
    /// Unsigned integer, 6 bits wide, right-aligned in its byte.
    U6,
    /// Unsigned integer, 7 bits wide, right-aligned in its byte.
    U7,
    /// Unsigned integer, one byte wide.
    U8,
    /// Signed two's-complement integer, one byte wide.
    I8,
    /// Unsigned 16-bit integer, big endian.
    U16Be,
    /// Signed 16-bit integer, big endian.
    I16Be,
    /// Unsigned 16-bit integer, little endian.
    U16Le,
    /// Unsigned 32-bit integer, big endian.
    U32Be,
    /// Signed 32-bit integer, big endian.
    I32Be,
    /// Unsigned 64-bit integer, little endian.
    U64Le,
    /// Unsigned 64-bit integer, big endian.
    U64Be,
    /// CMIS F16 half-precision float, big endian.
    F16Be,
    /// Raw 16-bit code, big endian, decoded as an opaque unsigned value.
    X16Be,
    /// ASCII string (NUL-padded).
    Ascii,
    /// Opaque byte blob.
    Bytes,
    /// Bit set: named bits within one or more bytes.
    Bitmap,
    /// Composite of named sub-byte members.
    Composite,
    /// Enumerated code, resolved through the referenced enum table.
    EnumRef,
}

/// Physical unit of a scaled value. SI prefixes are folded into the
/// `ScaledLsb` numerator/denominator, so `100 uV` is `(1, 10_000)` Volt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Temperature in degrees Celsius.
    Celsius,
    /// Electrical potential in Volt.
    Volt,
    /// Optical power in Watt.
    Watt,
    /// Electrical current in Ampere.
    Ampere,
    /// Relative level in decibel.
    Decibel,
    /// Absolute optical power in dBm.
    DecibelMilliwatt,
    /// Frequency in Hertz.
    Hertz,
    /// Length in Meter.
    Meter,
    /// Duration in Second.
    Second,
    /// Symbol rate in Baud.
    Baud,
    /// A pure count, such as a sequence number.
    Count,
    /// A dimensionless ratio, such as a bit error ratio.
    Ratio,
}

impl Unit {
    /// The unit's short display name, as shown in observation output.
    pub const fn name(self) -> &'static str {
        match self {
            Unit::Celsius => "degC",
            Unit::Volt => "V",
            Unit::Watt => "W",
            Unit::Ampere => "A",
            Unit::Decibel => "dB",
            Unit::DecibelMilliwatt => "dBm",
            Unit::Hertz => "Hz",
            Unit::Meter => "m",
            Unit::Second => "s",
            Unit::Baud => "Bd",
            Unit::Count => "count",
            Unit::Ratio => "ratio",
        }
    }
}

/// A scaling LSB parsed from the schema's `lsb_units` declaration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledLsb {
    /// Whether the raw code is interpreted as signed two's complement.
    pub signed: bool,
    /// Numerator of the LSB fraction.
    pub num: u32,
    /// Denominator of the LSB fraction.
    pub den: u32,
    /// Physical unit of the scaled value.
    pub unit: Unit,
    /// Retained schema caveat that qualifies the scaling (for example
    /// `"times TxBiasCurrentScalingFactor (Page 01h:160)"`).
    pub note: Option<&'static str>,
}

impl ScaledLsb {
    /// The LSB's magnitude in the row's unit.
    pub fn lsb(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

/// Byte location of one value inside a page window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldLoc {
    /// Seven-bit I2C address of the window.
    pub address: u8,
    /// Page selector of the window (0 for lower/base pages).
    pub page: u8,
    /// Flat byte offset inside the 256-byte window.
    pub offset: u16,
    /// Byte length of the value.
    pub len: u16,
}

impl FieldLoc {
    /// This location as a read/write [`Range`], at `bank` (ignored when
    /// the location's page is not banked by the caller's contract).
    pub fn range(self, bank: u8) -> Range {
        Range {
            address: self.address,
            bank,
            page: self.page,
            offset: self.offset as u8,
            len: self.len,
        }
    }
}

/// SFF-8472 external linear calibration constants (slope 8.8 fixed point,
/// offset signed, both in the parameter's internal LSB units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalLinear {
    /// Location of the slope constant.
    pub slope: FieldLoc,
    /// Location of the offset constant.
    pub offset: FieldLoc,
}

/// SFF-8472 external Rx-power degree-4 polynomial coefficients
/// (IEEE 754 float32, big endian). The polynomial result is in the
/// parameter's internal LSB units, like the linear constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalPoly {
    /// Locations of the degree-4 polynomial coefficients, index 0 = highest
    /// degree, ascending in the schema's declared order.
    pub coefficients: [FieldLoc; 5],
}

/// How a field's raw bytes become a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Scaling {
    /// No declared encoding: the raw code is the value.
    None,
    /// Fixed scaling: `value = raw * num/den` in `unit`.
    Scaled(ScaledLsb),
    /// Telemetry with an internal-calibration LSB and an optional
    /// external-calibration variant selected by a discriminator bit.
    Linear {
        /// The internal-calibration scaling.
        internal: ScaledLsb,
        /// The external linear-calibration constants, when the field
        /// declares them.
        external: Option<ExternalLinear>,
    },
    /// Rx-power style telemetry: internal LSB or degree-4 polynomial.
    Polynomial {
        /// The internal-calibration scaling.
        internal: ScaledLsb,
        /// The external polynomial coefficients, when the field declares
        /// them.
        external: Option<ExternalPoly>,
    },
    /// A declared encoding this surface does not decode (checksum, ieee754,
    /// q16_16, bcd, the calibration constants themselves, ...). The kind
    /// string is the schema's `encoding.kind`.
    Other {
        /// The schema's `encoding.kind` string for the undecoded encoding.
        kind: &'static str,
    },
    /// A CMIS F16 value in `unit`; only on F16Be rows. The value is the
    /// binary64 interpretation of the two-byte code
    /// ([`crate::cmis_f16::CmisF16`]), and the unit is schema-owned
    /// (`unit:` on the field).
    F16 {
        /// The schema-declared unit of the F16 value.
        unit: Unit,
    },
}

/// variant discriminator: a bit elsewhere in the map that selects
/// the external-calibration encoding of a telemetry field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDiscriminator {
    /// Schema field path as written (for example
    /// `page_a0h_base_memory.externally_calibrated`).
    pub field: &'static str,
    /// Location of the discriminator bit's byte.
    pub loc: FieldLoc,
    /// Bit position within that byte.
    pub bit: u8,
}

/// A field's instance dimension (collapsed array,): `lane` for
/// per-lane fields, `app`, `severity`, `term`, ... for others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDimension {
    /// Name of the dimension (`lane`, `app`, `severity`, ...).
    pub index: &'static str,
    /// First instance number (1-based, like schema lane numbering).
    pub first: u16,
    /// Last instance number.
    pub last: u16,
    /// Flat byte offset of each instance (`last - first + 1` entries).
    pub offsets: &'static [u16],
    /// Bit offset of each instance within its byte.
    pub bit_offsets: &'static [u8],
}

impl InstanceDimension {
    /// Number of instances (`last - first + 1`).
    pub fn count(&self) -> u16 {
        self.last - self.first + 1
    }

    /// Whether this is the per-lane (`lane`) dimension.
    pub fn is_lane(&self) -> bool {
        self.index == "lane"
    }
}

/// A declared rename or alias, as recorded in the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenameRecord {
    /// Revision (as written in the schema) from which the recorded name
    /// applies.
    pub rev: &'static str,
    /// The recorded field name.
    pub name: &'static str,
}

/// How the module signals that a content switch has settled, per the
/// revision the decode runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlingKind {
    /// CMIS 4.0 § 8.11.1 (printed p. 179): the selected data "shall be
    /// updated with the selected diagnostic data within the advertised
    /// tNACK time after byte 128 has been written".
    TnackAdvertised,
    /// CMIS 5.0+ § 8.13.1 (printed p. 244) and Table 10-5 (printed p. 353):
    /// a pure timing specification — there is no completion confirmation,
    /// and the host waits a tDDCS guard period (at most 10 ms from WRITE
    /// STOP to READ START).
    Tddcs,
}

/// One revision-qualified settling bound of a content switch: from `since`
/// (inclusive; `None` means from the earliest family revision) the switch
/// settles per `settling`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchBound {
    /// First revision the bound applies to (`None` = from the earliest
    /// family revision).
    pub since: Option<Rev>,
    /// The settling semantics in force from `since`.
    pub settling: SettlingKind,
}

/// Schema-declared content-switch metadata on a selector field: the
/// selector multiplexes the content of a data window in the same page
/// selection, and the module updates that window within the settling bound
/// of the decode's revision.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentSwitch {
    /// The multiplexed data window (window offsets, `len` bytes).
    pub data: FieldLoc,
    /// Settling bounds, ascending by `since`.
    pub bound: &'static [SwitchBound],
}

impl ContentSwitch {
    /// The settling bound in force at `rev` (the latest bound whose `since`
    /// is at or below it), or `None` below the first bound.
    pub fn settling_at(&self, rev: Rev) -> Option<SettlingKind> {
        self.bound
            .iter()
            .rev()
            .find(|bound| bound.since.is_none_or(|since| rev >= since))
            .map(|bound| bound.settling)
    }
}

/// An equality presence prerequisite, resolved during generation: the
/// field is present only when the referenced field's raw code equals
/// `expected`. The referenced location, bank policy and scalar extraction
/// (kind, bit offset/width) travel with the condition so the decoder
/// extracts the prerequisite exactly as it would decode the field itself.
#[derive(Debug, Clone, PartialEq)]
pub struct PrerequisiteEquals {
    /// Schema field path as written (for example
    /// `page_00h_upper_administrative.option_control_or_flag_support[rx_squelch_disable]`).
    pub field: &'static str,
    /// Location of the referenced field (or its selected instance).
    pub loc: FieldLoc,
    /// The referenced field's page is banked: the prerequisite is read at
    /// the dependent entry's bank.
    pub banked: bool,
    /// Scalar kind of the referenced field, for raw-code extraction.
    pub kind: FieldKind,
    /// Bit offset of the referenced field within its byte.
    pub bit_offset: u8,
    /// Total bit width of the referenced field.
    pub bit_width: u16,
    /// The raw code the prerequisite must carry (true → 1, false → 0).
    pub expected: u64,
    /// Content-switch metadata declared by the referenced field: this
    /// prerequisite selects the content of a multiplexed data window, so a
    /// decode additionally requires acquisition evidence.
    pub content_switch: Option<ContentSwitch>,
}

/// Field-level presence, derived from the schema's `present_when`.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldPresence {
    /// No declared condition: the field's bytes, when supplied, are the
    /// field.
    Unconditional,
    /// Present only when the referenced field's raw code equals the
    /// expected value. Prerequisite bytes absent → `NotSupplied`;
    /// mismatch → `Invalid` (no value either way).
    Equals(PrerequisiteEquals),
    /// A well-formed condition this surface does not evaluate (other
    /// operators, compound expressions). Observation yields
    /// `UnsupportedField` with the expression as the note — visibly
    /// unsupported, never silently unconditional.
    Unsupported {
        /// The presence expression this surface does not evaluate.
        expr: &'static str,
    },
}

/// Where a family's devices report their own revision, schema-derived.
/// The core decoder cannot depend on generated family crates, so the
/// generated family catalog carries the probe and resolved entries expose
/// it. CMIS: lower memory byte 1, BCD major/minor in the nibbles — decoded
/// with [`crate::rev::Rev::from_major_byte_cmis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionProbe {
    /// Catalog row name of the revision field (for example `cmis_revision`).
    pub field: &'static str,
    /// Location of the revision byte.
    pub loc: FieldLoc,
}

/// One revision span of one schema field. All data is generated; the
/// effective name for the span is `name`, and `renamed_from`/`aliased_to`
/// retain the schema's declaration for revision-window metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogRow {
    /// Schema-effective field name for this revision span.
    pub name: &'static str,
    /// Human-facing name from the schema, when declared.
    pub display_name: &'static str,
    /// Page schema id (for example `CmisPage11hLaneStatus`).
    pub page_id: &'static str,
    /// Schema `page_address` label (for example `11h`, `lower`, `A2h`).
    pub page_label: &'static str,
    /// Seven-bit I2C address of the page window.
    pub address: u8,
    /// Page selector of the window (0 for lower/base pages).
    pub page: u8,
    /// Whether the page is banked (the bank is caller-selected).
    pub banked: bool,
    /// Flat byte offset of the base instance within the 256-byte window.
    pub offset: u16,
    /// Byte length of one instance.
    pub len: u16,
    /// Bit offset of the base instance within its byte.
    pub bit_offset: u8,
    /// Total field width in bits (byte blobs carry `len * 8`).
    pub bit_width: u16,
    /// Wire type of the field.
    pub kind: FieldKind,
    /// Referenced enum table name for `EnumRef` fields.
    pub enum_ref: Option<&'static str>,
    /// How the raw bytes become a value.
    pub scaling: Scaling,
    /// Declared permission for the field.
    pub access: AccessKind,
    /// Side effects a read of this field has, per the schema.
    pub read_effects: &'static [ReadEffect],
    /// `None` also covers the schema's `unknown` change source.
    pub change_source: Option<ChangeSource>,
    /// Instance dimension for collapsed-array fields, when the field is
    /// instanced.
    pub instances: Option<InstanceDimension>,
    /// Applicable for `since <= rev < until`; `None` is unbounded.
    pub since: Option<Rev>,
    /// Exclusive upper revision bound; `None` is unbounded.
    pub until: Option<Rev>,
    /// Previous schema name this field was renamed from, if any.
    pub renamed_from: Option<RenameRecord>,
    /// Additional names this field is aliased to.
    pub aliased_to: &'static [RenameRecord],
    /// Schema tags carried by the field (for example `telemetry`).
    pub tags: &'static [&'static str],
    /// Schema `field_type` category (for example `Telemetry`).
    pub category: Option<&'static str>,
    /// Schema `unit` string, when declared separately from the encoding.
    pub unit: Option<&'static str>,
    /// The variant discriminator selecting this field's scaling, when the
    /// field is externally calibrated telemetry.
    pub discriminator: Option<VariantDiscriminator>,
    /// Field-level presence from the schema's `present_when`.
    pub presence: FieldPresence,
}

/// A generated per-family table.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyCatalog {
    /// The family this catalog describes.
    pub family: Family,
    /// Revisions the schemas describe; resolution outside this set refuses.
    pub supported_revs: &'static [Rev],
    /// Where the family's devices report their own revision (CMIS: lower
    /// memory byte 1), when the schemas declare one. Drives the core
    /// decoder's reported-revision admission.
    pub revision_probe: Option<RevisionProbe>,
    /// All revision-span rows of the family.
    pub rows: &'static [CatalogRow],
}

/// How a query selects instances of a lane (or other) dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneSelection {
    /// The field as a whole: one entry at the base instance, no lane.
    Unspecified,
    /// Every instance.
    All,
    /// One instance index (1-based, like the schema's lane numbering).
    One(u16),
    /// At most the first `n` instances; `0` yields no entries.
    UpTo(u16),
}

/// A field-resolution query.
#[derive(Debug, Clone, Copy)]
pub struct FieldQuery<'a> {
    /// Exact name, schema alias, or glob with `*` and `?`.
    pub pattern: &'a str,
    /// How the query selects instances of a lane (or other) dimension.
    pub lane: LaneSelection,
    /// Optional page filter: page schema id or page label.
    pub page: Option<&'a str>,
    /// Selected bank for banked pages (ignored otherwise).
    pub bank: u8,
}

/// A field-resolution failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    /// The family/revision is not described by any generated catalog.
    #[error("unsupported family/revision {rev:?} for catalog resolution")]
    UnsupportedRevision {
        /// The revision that was asked for.
        rev: Rev,
    },
    /// No applicable row matches the query's pattern (and page filter).
    #[error("no field matches {pattern:?} at revision {rev:?}")]
    NoMatch {
        /// The pattern that matched nothing.
        pattern: String,
        /// The revision the resolution ran at.
        rev: Rev,
    },
    /// An exact name matched several rows; qualify with the page filter.
    #[error(
        "ambiguous field {pattern:?} at revision {rev:?}: matches {candidates:?}; qualify with the page filter"
    )]
    Ambiguous {
        /// The pattern that matched several rows.
        pattern: String,
        /// The revision the resolution ran at.
        rev: Rev,
        /// `page_id::name` of every matching row.
        candidates: Vec<String>,
    },
    /// The requested lane index is outside the field's instance range.
    #[error("lane {lane} out of range {first}..={last} for field {field}")]
    LaneOutOfRange {
        /// The requested lane index.
        lane: u16,
        /// First instance number of the field.
        first: u16,
        /// Last instance number of the field.
        last: u16,
        /// The field whose range was exceeded.
        field: &'static str,
    },
    /// A lane selection was made against a field with no instance
    /// dimension.
    #[error("field {field} has no instance dimension")]
    NotInstanced {
        /// The field that is not instanced.
        field: &'static str,
    },
}

/// A resolved, revision- and instance-specific field.
#[derive(Debug, Clone, Copy)]
pub struct FieldEntry<'a> {
    /// The resolved catalog row.
    pub row: &'a CatalogRow,
    /// The revision the resolution used.
    pub rev: Rev,
    /// Instance index when a lane/instance was selected.
    pub lane: Option<u16>,
    /// Bank for banked pages (always 0 for unbanked pages).
    pub bank: u8,
    /// Concrete flat byte offset of the selected instance.
    pub offset: u16,
    /// Concrete bit offset of the selected instance.
    pub bit_offset: u8,
    /// The family's reported-revision probe (from the family catalog),
    /// when the family declares one. `None` for hand-built entries with
    /// no generated catalog behind them.
    pub revision_probe: Option<&'a RevisionProbe>,
}

impl<'a> FieldEntry<'a> {
    /// The entry's schema-effective field name.
    pub fn name(&self) -> &'static str {
        self.row.name
    }

    /// Concrete location of the selected instance.
    pub fn range(&self) -> Range {
        Range {
            address: self.row.address,
            bank: if self.row.banked { self.bank } else { 0 },
            page: self.row.page,
            offset: self.offset.min(u16::from(u8::MAX)) as u8,
            len: self.row.len,
        }
    }

    /// Total bit width of the selected instance.
    pub fn width_bits(&self) -> u16 {
        self.row.bit_width
    }
}

fn glob_match(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            glob_match(&pattern[1..], name) || (!name.is_empty() && glob_match(pattern, &name[1..]))
        }
        (Some(b'?'), Some(_)) => glob_match(&pattern[1..], &name[1..]),
        (Some(p), Some(n)) => p == n && glob_match(&pattern[1..], &name[1..]),
        _ => false,
    }
}

/// Whether `row`'s revision window includes `rev`.
///
/// `since` is inclusive and `until` is exclusive. `None` on either side is
/// unbounded. [`crate::field_catalog::resolve`] and
/// [`crate::revision_report::field_availability`] both use this rule, so a
/// row outside the window is revision-gated rather than missing.
pub fn row_in_window(row: &CatalogRow, rev: Rev) -> bool {
    row.since.is_none_or(|since| rev >= since) && row.until.is_none_or(|until| rev < until)
}

fn row_applicable(row: &CatalogRow, rev: Rev) -> bool {
    row_in_window(row, rev)
}

fn expand<'a>(
    catalog: &'a FamilyCatalog,
    row: &'a CatalogRow,
    rev: Rev,
    bank: u8,
    lane: LaneSelection,
) -> Result<Vec<FieldEntry<'a>>, CatalogError> {
    let mut entries = Vec::new();
    match &row.instances {
        None => {
            if lane != LaneSelection::Unspecified {
                return Err(CatalogError::NotInstanced { field: row.name });
            }
            entries.push(FieldEntry {
                row,
                rev,
                lane: None,
                bank,
                offset: row.offset,
                bit_offset: row.bit_offset,
                revision_probe: catalog.revision_probe.as_ref(),
            });
        }
        Some(dim) => {
            let indices: Vec<Option<u16>> = match lane {
                LaneSelection::Unspecified => vec![None],
                LaneSelection::All => (dim.first..=dim.last).map(Some).collect(),
                LaneSelection::One(index) => {
                    if index < dim.first || index > dim.last {
                        return Err(CatalogError::LaneOutOfRange {
                            lane: index,
                            first: dim.first,
                            last: dim.last,
                            field: row.name,
                        });
                    }
                    vec![Some(index)]
                }
                LaneSelection::UpTo(limit) => (dim.first..=dim.last.min(limit)).map(Some).collect(),
            };
            for index in indices {
                let instance = index.unwrap_or(dim.first);
                let slot = usize::from(instance - dim.first);
                if slot >= dim.offsets.len() || slot >= dim.bit_offsets.len() {
                    // A corrupt generated table is a generator defect; refuse
                    // rather than decode from a fabricated offset.
                    return Err(CatalogError::NotInstanced { field: row.name });
                }
                entries.push(FieldEntry {
                    row,
                    rev,
                    lane: index,
                    bank,
                    offset: dim.offsets[slot],
                    bit_offset: dim.bit_offsets[slot],
                    revision_probe: catalog.revision_probe.as_ref(),
                });
            }
        }
    }
    Ok(entries)
}

/// Resolve fields by exact name, schema alias or glob at one revision.
///
/// Exact names that match several rows are an error carrying the candidates
/// (qualify with the query's page filter); globs return every match. Zero
/// matches are an explicit error, except for [`LaneSelection::UpTo`] with a
/// zero limit, which trims every instance and yields an empty result.
pub fn resolve<'a>(
    catalog: &'a FamilyCatalog,
    rev: Rev,
    query: &FieldQuery<'_>,
) -> Result<Vec<FieldEntry<'a>>, CatalogError> {
    if rev.family != catalog.family || !catalog.supported_revs.contains(&rev) {
        return Err(CatalogError::UnsupportedRevision { rev });
    }
    let is_glob = query.pattern.contains('*') || query.pattern.contains('?');
    let mut matched: Vec<&CatalogRow> = catalog
        .rows
        .iter()
        .filter(|row| row_applicable(row, rev))
        .filter(|row| {
            query
                .page
                .is_none_or(|page| page == row.page_id || page == row.page_label)
        })
        .filter(|row| {
            if is_glob {
                glob_match(query.pattern.as_bytes(), row.name.as_bytes())
            } else {
                row.name == query.pattern
            }
        })
        .collect();
    if matched.is_empty() {
        return Err(CatalogError::NoMatch {
            pattern: query.pattern.to_string(),
            rev,
        });
    }
    if !is_glob && matched.len() > 1 {
        let mut candidates: Vec<String> = matched
            .iter()
            .map(|row| format!("{}::{}", row.page_id, row.name))
            .collect();
        candidates.sort();
        candidates.dedup();
        return Err(CatalogError::Ambiguous {
            pattern: query.pattern.to_string(),
            rev,
            candidates,
        });
    }
    matched.sort_by_key(|row| (row.page_id, row.offset));
    matched.dedup_by(|a, b| a.page_id == b.page_id && a.offset == b.offset && a.name == b.name);
    let mut entries = Vec::new();
    for row in matched {
        entries.extend(expand(catalog, row, rev, query.bank, query.lane)?);
    }
    Ok(entries)
}
