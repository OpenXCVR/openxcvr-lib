//! Field-catalog emission: one generated `catalog.rs` per spec, holding a
//! `FamilyCatalog` table derived entirely from the page schemas. Row content
//! (names, revision spans, locations, scaling, permissions, read effects)
//! comes from the parsed schemas; nothing here encodes a family memory map.
//!
//! Revision spans: for each field and each family revision the effective
//! field is resolved with `ir::version::resolve_field_for_rev`, then
//! consecutive revisions with the same emitted metadata collapse into
//! one row bounded by `since`/`until` (`since` inclusive, `until`
//! exclusive). A rename therefore resolves as a plain name lookup.
use std::collections::BTreeMap;

use crate::expr::compute;
use crate::ir::pages::{
    Access, ChangeSourceMetadata, FieldType, PageField, PageSchema, ReadEffectMetadata,
};
use crate::ir::version::{applicable_revs, resolve_field_for_rev, Rev};
use crate::presence::EmittedPresence;

/// Raw page-schema facts the IR does not carry: the `banking:` block's
/// presence, the SFF-8472 `page_select:` key, and the root schema file's
/// path (for generation-error messages). Extracted from the page YAML by
/// the driver so the emitter stays schema-driven.
#[derive(Debug, Clone, Default)]
pub struct CatalogPageInfo {
    pub banked: bool,
    pub page_select: Option<u8>,
    /// Root schema file path, when known — presence generation errors name
    /// it alongside the field and reference.
    pub path: Option<String>,
}

/// Spec identity for catalog emission.
pub struct SpecFamily {
    pub spec: &'static str,
    /// Rust constant fragments for the family, e.g. `Family::Cmis`.
    pub family_const: &'static str,
    /// Core `Rev` constant names in family order (CMIS only; SFF families
    /// use struct literals).
    pub rev_consts: &'static [&'static str],
    /// (major, minor) of every supported revision, in order.
    pub revs: &'static [(u8, u8)],
}

pub const SPEC_FAMILIES: &[SpecFamily] = &[
    SpecFamily {
        spec: "cmis",
        family_const: "Family::Cmis",
        rev_consts: &[
            "Rev::CMIS_3_0",
            "Rev::CMIS_4_0",
            "Rev::CMIS_5_0",
            "Rev::CMIS_5_1",
            "Rev::CMIS_5_2",
            "Rev::CMIS_5_3",
            "Rev::CMIS_5_4",
        ],
        revs: &[(3, 0), (4, 0), (5, 0), (5, 1), (5, 2), (5, 3), (5, 4)],
    },
    SpecFamily {
        spec: "sff-8636",
        family_const: "Family::Sff8636",
        rev_consts: &[],
        revs: &[
            (1, 3),
            (1, 4),
            (1, 5),
            (2, 0),
            (2, 5),
            (2, 8),
            (2, 11),
            (2, 12),
        ],
    },
    SpecFamily {
        spec: "sff-8472",
        family_const: "Family::Sff8472",
        rev_consts: &[],
        revs: &[
            (9, 3),
            (9, 5),
            (10, 2),
            (10, 4),
            (11, 0),
            (11, 3),
            (11, 4),
            (12, 3),
            (12, 4),
            (12, 5),
        ],
    },
];

impl SpecFamily {
    /// The family's revision domain, parsed from [`SpecFamily::revs`] in
    /// family order. Every family table mirrors a `Rev::…_ALL` domain;
    /// this is the single derivation point for span-row machinery that
    /// predates the SFF families gaining full domains.
    pub fn rev_domain(&self) -> Vec<Rev> {
        self.revs
            .iter()
            .map(|(major, minor)| {
                Rev::parse(&format!("{major}.{minor}")).expect("family rev table")
            })
            .collect()
    }

    /// Single-revision families have no span model: every field emits one
    /// unbounded row (no since/until boundaries are observable).
    pub fn single_rev(&self) -> bool {
        self.revs.len() <= 1
    }
}

pub fn spec_family(spec: &str) -> Option<&'static SpecFamily> {
    SPEC_FAMILIES.iter().find(|f| f.spec == spec)
}

// ─── lsb_units parsing ───────────────────────────────────────────────────────

/// (numerator, denominator) of a decimal or `a/b` literal.
fn parse_number(token: &str) -> Option<(u64, u64)> {
    if let Some((a, b)) = token.split_once('/') {
        let a: u64 = a.parse().ok()?;
        let b: u64 = b.parse().ok()?;
        if b == 0 {
            return None;
        }
        return Some((a, b));
    }
    if let Some((int_part, frac)) = token.split_once('.') {
        let int: u64 = if int_part.is_empty() {
            0
        } else {
            int_part.parse().ok()?
        };
        let frac_digits = frac.len() as u32;
        let frac: u64 = if frac.is_empty() {
            0
        } else {
            frac.parse().ok()?
        };
        let den = 10u64.checked_pow(frac_digits)?;
        return Some((int * den + frac, den));
    }
    let value: u64 = token.parse().ok()?;
    Some((value, 1))
}

fn reduce(num: u64, den: u64) -> (u64, u64) {
    match gcd(num, den) {
        0 => (num, den),
        gcd => (num / gcd, den / gcd),
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// A unit token and its SI-prefix factor, matched as a prefix of the text
/// following the numeric part of `lsb_units`.
fn unit_prefix(rest: &str) -> Option<(&'static str, u64, u64, usize)> {
    const TABLE: &[(&str, &str, u64, u64)] = &[
        ("degC", "Celsius", 1, 1),
        ("deg C", "Celsius", 1, 1),
        ("dBm", "DecibelMilliwatt", 1, 1),
        ("dB", "Decibel", 1, 1),
        ("GHz", "Hertz", 1_000_000_000, 1),
        ("MHz", "Hertz", 1_000_000, 1),
        ("kHz", "Hertz", 1_000, 1),
        ("Hz", "Hertz", 1, 1),
        ("uV", "Volt", 1, 1_000_000),
        ("mV", "Volt", 1, 1_000),
        ("V", "Volt", 1, 1),
        ("uW", "Watt", 1, 1_000_000),
        ("mW", "Watt", 1, 1_000),
        ("W", "Watt", 1, 1),
        ("uA", "Ampere", 1, 1_000_000),
        ("mA", "Ampere", 1, 1_000),
        ("A", "Ampere", 1, 1),
        ("nm", "Meter", 1, 1_000_000_000),
        ("km", "Meter", 1_000, 1),
        ("mm", "Meter", 1, 1_000),
        ("ms", "Second", 1, 1_000),
        ("ns", "Second", 1, 1_000_000_000),
        ("m", "Meter", 1, 1),
        ("s", "Second", 1, 1),
        ("MBd", "Baud", 1_000_000, 1),
        ("kBd", "Baud", 1_000, 1),
        ("Bd", "Baud", 1, 1),
    ];
    TABLE
        .iter()
        .find(|entry| rest.starts_with(entry.0))
        .map(|entry| (entry.1, entry.2, entry.3, entry.0.len()))
}

/// Parse an `lsb_units` declaration into an exact rational LSB plus unit.
/// Everything after the unit token is retained as the caveat note.
fn parse_lsb(text: &str) -> Result<(u32, u32, &'static str, Option<String>), String> {
    let text = text.trim();
    let (number, rest) = match text.split_once(char::is_whitespace) {
        Some((head, tail)) => match parse_number(head) {
            Some(number) => (number, tail.trim_start()),
            // No numeric prefix: the value is a raw count; the whole text is
            // the caveat (for example "repetitions (0=stop, 7=continuous)").
            None => return Ok((1, 1, "Count", Some(text.to_string()))),
        },
        None => match parse_number(text) {
            Some(number) => (number, ""),
            None => return Ok((1, 1, "Count", Some(text.to_string()))),
        },
    };
    let Some((unit, prefix_num, prefix_den, token_len)) = unit_prefix(rest) else {
        return Err(format!("unknown unit in lsb_units {text:?}"));
    };
    let trailing = rest[token_len..].trim();
    let note_len = (!trailing.is_empty()).then(|| trailing.to_string());
    let (num, den) = reduce(number.0 * prefix_num, number.1 * prefix_den);
    let num = u32::try_from(num).map_err(|_| format!("lsb numerator overflow in {text:?}"))?;
    let den = u32::try_from(den).map_err(|_| format!("lsb denominator overflow in {text:?}"))?;
    Ok((num, den, unit, note_len))
}

// ─── scaling model shared by both emitters ──────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct EmittedScaled {
    pub signed: bool,
    pub num: u32,
    pub den: u32,
    pub unit: &'static str,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmittedLoc {
    pub address: u8,
    pub page: u8,
    pub offset: u16,
    pub len: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EmittedScaling {
    None,
    Scaled(EmittedScaled),
    Linear {
        internal: EmittedScaled,
        slope: EmittedLoc,
        offset: EmittedLoc,
    },
    Polynomial {
        internal: EmittedScaled,
        coefficients: [EmittedLoc; 5],
    },
    Other {
        kind: String,
    },
    /// An F16-scaled row (CMIS/SFF-8636 F16 decimal diagnostic values):
    /// the unit comes from the schema's `unit:` through the closed f16
    /// table, never from `encoding:`, field names or tags. The value is
    /// the `Unit` variant name.
    F16 {
        unit: &'static str,
    },
}

fn scaled_from_encoding(encoding: &serde_norway::Value) -> Result<EmittedScaled, String> {
    let map = encoding
        .as_mapping()
        .ok_or_else(|| format!("encoding must be a mapping: {encoding:?}"))?;
    let kind = map
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("encoding without kind: {encoding:?}"))?;
    if kind != "scaled_signed" && kind != "scaled_unsigned" {
        return Err(format!("expected scaled encoding, got {kind:?}"));
    }
    let lsb_units = map
        .get("lsb_units")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("scaled encoding without lsb_units: {encoding:?}"))?;
    let (num, den, unit, note) = parse_lsb(lsb_units)?;
    Ok(EmittedScaled {
        signed: kind == "scaled_signed",
        num,
        den,
        unit,
        note,
    })
}

/// Derive the scaling of an f16be field without `variants` (Stage 2 D2):
/// the unit is schema-owned — `unit:` from the closed table below. An
/// `encoding:` alongside f16be, a missing unit, or a unit outside the
/// table is a generation error naming the field and the accepted units;
/// nothing is inferred from field names, tags or the kind alone.
fn f16_scaling(field: &PageField) -> Result<EmittedScaling, String> {
    if field.encoding.is_some() {
        return Err(format!(
            "field {}: f16be takes its unit from `unit:`, not `encoding:`",
            field.name
        ));
    }
    let Some(unit) = field.unit.as_deref() else {
        return Err(format!(
            "field {}: f16be requires a `unit:` from the closed table (accepted: \"ratio\")",
            field.name
        ));
    };
    let unit = match unit {
        "ratio" => "Ratio",
        other => {
            return Err(format!(
                "field {}: f16be unit {other:?} is not in the closed table (accepted: \"ratio\")",
                field.name
            ))
        }
    };
    Ok(EmittedScaling::F16 { unit })
}

/// Resolve a field reference like `external_calibration_linear_term[4]`
/// within one page schema to a byte location.
fn resolve_field_ref(page: &PageLocation, reference: &str) -> Result<EmittedLoc, String> {
    let (name, instance) = match reference.strip_suffix(']') {
        Some(head) => {
            let (name, index) = head
                .rsplit_once('[')
                .ok_or_else(|| format!("malformed field reference {reference:?}"))?;
            let index: i64 = index
                .parse()
                .map_err(|_| format!("malformed instance in {reference:?}"))?;
            (name, Some(index))
        }
        None => (reference, None),
    };
    let field = page
        .schema
        .fields
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| format!("unknown field reference {reference:?}"))?;
    let offset = field_offset(field, instance)?;
    let len = u16::try_from(field.size_bytes.unwrap_or(1)).unwrap_or(u16::MAX);
    Ok(EmittedLoc {
        address: page.address,
        page: page.page,
        offset: u16::try_from(offset).map_err(|_| format!("offset overflow in {reference:?}"))?,
        len,
    })
}

pub(crate) fn field_offset(field: &PageField, instance: Option<i64>) -> Result<i64, String> {
    match (&field.offset_expr, field.offset, instance) {
        (Some(expr), _, Some(index)) => {
            let var = field
                .instances
                .as_ref()
                .map(|i| i.index.as_str())
                .unwrap_or("");
            compute(expr, var, index)
        }
        (_, Some(offset), _) => Ok(i64::from(offset)),
        (Some(expr), _, None) => Err(format!("field {field:?} needs an instance for {expr:?}")),
        (None, None, _) => Err(format!("field {} has no offset", field.name)),
    }
}

/// One page's resolved location facts.
pub(crate) struct PageLocation {
    pub schema: PageSchema,
    pub address: u8,
    pub page: u8,
    pub banked: bool,
}

fn page_location(
    schema: &PageSchema,
    spec: &str,
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<PageLocation, String> {
    let page_info = info.get(&schema.id).cloned().unwrap_or_default();
    let (address, page) = match (spec, schema.page_address.trim()) {
        ("sff-8472", "A0h") => (0x50u8, 0u8),
        ("sff-8472", "A2h") => (0x51u8, page_info.page_select.unwrap_or(0)),
        (_, "lower") => (0x50u8, 0u8),
        _ => {
            let hex = schema
                .page_address
                .trim()
                .strip_suffix('h')
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                .ok_or_else(|| {
                    format!(
                        "unmapped page_address {:?} for spec {spec}",
                        schema.page_address
                    )
                })?;
            (0x50u8, hex)
        }
    };
    Ok(PageLocation {
        schema: schema.clone(),
        address,
        page,
        banked: page_info.banked,
    })
}

/// Full variant resolution needs the discriminator's page location, which
/// `parse_variants` defers. This struct carries the unresolved path for the
/// driver-level lookup.
pub(crate) struct VariantPlan {
    pub discriminator_path: String,
    pub internal: Option<serde_norway::Value>,
    pub external: Option<serde_norway::Value>,
}

pub(crate) fn variant_plan(variants: &serde_norway::Value) -> Result<VariantPlan, String> {
    let map = variants
        .as_mapping()
        .ok_or_else(|| format!("variants must be a mapping: {variants:?}"))?;
    let discriminator_path = map
        .get("discriminator")
        .and_then(|d| d.as_mapping())
        .and_then(|d| d.get("field"))
        .and_then(|f| f.as_str())
        .ok_or_else(|| format!("variants without discriminator.field: {variants:?}"))?
        .to_string();
    let mut internal = None;
    let mut external = None;
    for case in map
        .get("cases")
        .and_then(|c| c.as_sequence())
        .ok_or_else(|| format!("variants without cases: {variants:?}"))?
    {
        let case_map = case
            .as_mapping()
            .ok_or_else(|| format!("variant case must be a mapping: {case:?}"))?;
        let when = case_map
            .get("when")
            .and_then(|w| w.as_mapping())
            .and_then(|w| w.get("value"))
            .cloned()
            .ok_or_else(|| format!("variant case without when.value: {case:?}"))?;
        let encoding = case_map.get("encoding").cloned();
        match when.as_bool() {
            Some(false) => internal = encoding,
            Some(true) => external = encoding,
            _ => return Err(format!("variant when.value must be boolean: {case:?}")),
        }
    }
    Ok(VariantPlan {
        discriminator_path,
        internal,
        external,
    })
}

// ─── row derivation ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedRow {
    pub name: String,
    pub display_name: Option<String>,
    pub page_id: String,
    pub page_label: String,
    pub address: u8,
    pub page: u8,
    pub banked: bool,
    pub offset: u16,
    pub len: u16,
    pub bit_offset: u8,
    pub bit_width: u16,
    pub kind: &'static str,
    pub enum_ref: Option<String>,
    pub scaling: EmittedScaling,
    pub access: &'static str,
    pub read_effects: Vec<&'static str>,
    pub change_source: Option<&'static str>,
    pub instances: Option<EmittedInstances>,
    pub since: Option<(u8, u8)>,
    pub until: Option<(u8, u8)>,
    pub renamed_from: Option<(String, String)>,
    pub aliased_to: Vec<(String, String)>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub unit: Option<String>,
    pub discriminator: Option<(String, EmittedLoc, u8)>,
    /// Field-level presence, classified from `present_when` (B1).
    pub presence: EmittedPresence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmittedInstances {
    pub index: String,
    pub first: i64,
    pub last: i64,
    pub offsets: Vec<i64>,
    pub bit_offsets: Vec<i64>,
}

pub(crate) fn field_kind(ty: &FieldType) -> &'static str {
    match ty {
        FieldType::Bool => "Bool",
        FieldType::U2 => "U2",
        FieldType::U3 => "U3",
        FieldType::U4 => "U4",
        FieldType::U5 => "U5",
        FieldType::U6 => "U6",
        FieldType::U7 => "U7",
        FieldType::U8 => "U8",
        FieldType::I8 => "I8",
        FieldType::U16Be => "U16Be",
        FieldType::I16Be => "I16Be",
        FieldType::U16Le => "U16Le",
        FieldType::U32Be => "U32Be",
        FieldType::I32Be => "I32Be",
        FieldType::U64Le => "U64Le",
        FieldType::U64Be => "U64Be",
        FieldType::F16Be => "F16Be",
        FieldType::X16Be => "X16Be",
        FieldType::Ascii => "Ascii",
        FieldType::Bytes => "Bytes",
        FieldType::Bitmap => "Bitmap",
        FieldType::Composite => "Composite",
        FieldType::EnumRef => "EnumRef",
    }
}

fn access_kind(access: Option<Access>) -> &'static str {
    match access {
        Some(Access::Ro) | None => "Ro",
        Some(Access::Rw) => "Rw",
        Some(Access::Rww) => "Rww",
        Some(Access::Wo) => "Wo",
        Some(Access::WoSc) => "WoSc",
        Some(Access::RoCor) => "RoCor",
        Some(Access::Unknown) => "Unknown",
    }
}

fn read_effect_kind(effect: ReadEffectMetadata) -> &'static str {
    match effect {
        ReadEffectMetadata::None => "None",
        ReadEffectMetadata::ClearOnRead => "ClearOnRead",
        ReadEffectMetadata::AcknowledgeStartupInterrupt => "AcknowledgeStartupInterrupt",
        ReadEffectMetadata::Unknown => "Unknown",
        ReadEffectMetadata::WriteOnly => "WriteOnly",
    }
}

pub(crate) fn bit_width_of(field: &PageField) -> u64 {
    u64::from(field.bit_width.unwrap_or(match field.ty {
        FieldType::Bool => 1,
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::F16Be
        | FieldType::X16Be => 16,
        FieldType::U32Be | FieldType::I32Be => 32,
        FieldType::U64Le | FieldType::U64Be => 64,
        _ => field.size_bytes.unwrap_or(1).saturating_mul(8),
    }))
}

/// Model the SFF-8472 calibration-variant shape of : a boolean
/// discriminator bit elsewhere in the map selecting between a scaled
/// internal encoding and an external-calibration encoding (linear
/// slope/offset or a degree-4 polynomial). Returns `Ok(None)` for any other
/// variant shape, which the caller catalogs as an undecodable union.
/// A modeled calibration variant: the scaling plus its discriminator bit
/// (field path, byte location, bit index).
pub(crate) type ModeledVariants = (EmittedScaling, Option<(String, EmittedLoc, u8)>);

fn calibration_variants(
    page: &PageLocation,
    field: &PageField,
    variants: &serde_norway::Value,
    pages: &[PageLocation],
) -> Result<Option<ModeledVariants>, String> {
    let plan = match variant_plan(variants) {
        Ok(plan) => plan,
        Err(_) => return Ok(None),
    };
    let (disc_page, disc_field) = match plan.discriminator_path.split_once('.') {
        Some(parts) => parts,
        None => return Ok(None),
    };
    let Some(internal_encoding) = plan.internal.as_ref() else {
        return Ok(None);
    };
    let internal = match scaled_from_encoding(internal_encoding) {
        Ok(internal) => internal,
        Err(_) => return Ok(None),
    };
    let Some(external_encoding) = plan.external.as_ref() else {
        return Ok(None);
    };
    let external_map = match external_encoding.as_mapping() {
        Some(map) => map,
        None => return Ok(None),
    };
    if external_map.get("kind").and_then(|k| k.as_str()) != Some("external_calibration") {
        return Ok(None);
    }
    // Resolve the discriminator bit: a single-bit field in another page.
    let disc_page = match pages.iter().find(|p| p.schema.id == disc_page) {
        Some(page) => page,
        None => return Ok(None),
    };
    let disc_field = match disc_page
        .schema
        .fields
        .iter()
        .find(|f| f.name == disc_field)
    {
        Some(field) => field,
        None => return Ok(None),
    };
    let Some(bit) = disc_field.bit_offset else {
        return Ok(None);
    };
    let Some(offset) = disc_field.offset else {
        return Ok(None);
    };
    let disc_loc = EmittedLoc {
        address: disc_page.address,
        page: disc_page.page,
        offset: match u16::try_from(offset) {
            Ok(offset) => offset,
            Err(_) => return Err(format!("discriminator offset overflow for {}", field.name)),
        },
        len: 1,
    };
    let scaling = if external_map.get("formula").and_then(|f| f.as_str()) == Some("poly4") {
        let coefficients = external_map
            .get("coefficients")
            .and_then(|c| c.as_sequence())
            .ok_or_else(|| format!("field {}: poly4 without coefficients", field.name))?;
        if coefficients.len() != 5 {
            return Err(format!(
                "field {}: poly4 needs exactly 5 coefficients",
                field.name
            ));
        }
        let mut locs = Vec::with_capacity(5);
        for coefficient in coefficients {
            let reference = coefficient.as_str().ok_or_else(|| {
                format!(
                    "field {}: coefficient must be a field reference",
                    field.name
                )
            })?;
            locs.push(
                resolve_field_ref(page, reference)
                    .map_err(|e| format!("field {}: {}", field.name, e))?,
            );
        }
        EmittedScaling::Polynomial {
            internal,
            coefficients: [locs[0], locs[1], locs[2], locs[3], locs[4]],
        }
    } else {
        let slope = external_map
            .get("slope_field")
            .and_then(|s| s.as_str())
            .ok_or_else(|| format!("field {}: external case without slope_field", field.name))?;
        let offset = external_map
            .get("offset_field")
            .and_then(|s| s.as_str())
            .ok_or_else(|| format!("field {}: external case without offset_field", field.name))?;
        EmittedScaling::Linear {
            internal,
            slope: resolve_field_ref(page, slope)
                .map_err(|e| format!("field {}: {}", field.name, e))?,
            offset: resolve_field_ref(page, offset)
                .map_err(|e| format!("field {}: {}", field.name, e))?,
        }
    };
    Ok(Some((
        scaling,
        Some((plan.discriminator_path.clone(), disc_loc, bit as u8)),
    )))
}

/// Derive every revision-span row of one field.
pub(crate) fn derive_field_rows(
    page: &PageLocation,
    field: &PageField,
    family: &SpecFamily,
    pages: &[PageLocation],
    classified: &BTreeMap<(String, String), EmittedPresence>,
) -> Result<Vec<DerivedRow>, String> {
    let presence = classified
        .get(&(page.schema.id.clone(), field.name.clone()))
        .cloned()
        .unwrap_or(EmittedPresence::Unconditional);
    if family.single_rev() {
        return Ok(vec![derive_row(page, field, pages, &presence)?]);
    }
    let revisions = family.rev_domain();
    let applicable = applicable_revs(field, &revisions);
    let mut rows: Vec<DerivedRow> = Vec::new();
    for (index, &rev) in revisions.iter().enumerate() {
        if !applicable.contains(&rev) {
            continue;
        }
        check_span_prerequisite(page, field, &presence, rev)?;
        let resolved = resolve_field_for_rev(field, rev);
        let mut row = derive_row(page, &resolved, pages, &presence)?;
        // Compare all emitted metadata before adding span boundaries. Equal
        // names alone cannot coalesce different permissions/effects/types.
        let since = (index != 0).then_some(family.revs[index]);
        let until = family.revs.get(index + 1).copied();
        if let Some(last) = rows.last_mut() {
            let mut unbounded = last.clone();
            unbounded.since = None;
            unbounded.until = None;
            if last.until == Some(family.revs[index]) && unbounded == row {
                last.until = until;
                continue;
            }
        }
        row.since = since;
        row.until = until;
        rows.push(row);
    }
    Ok(rows)
}

/// Derive one fully resolved field, shared by Rust and C catalogs.
fn derive_row(
    page: &PageLocation,
    field: &PageField,
    pages: &[PageLocation],
    presence: &EmittedPresence,
) -> Result<DerivedRow, String> {
    // Instances.
    let instances = field
        .instances
        .as_ref()
        .map(|inst| {
            let count = (inst.end - inst.start + 1) as usize;
            let mut offsets = Vec::with_capacity(count);
            let mut bit_offsets = Vec::with_capacity(count);
            for index in inst.start..=inst.end {
                offsets.push(field_offset(field, Some(index))?);
                let bit = match &field.bit_offset_expr {
                    Some(expr) => compute(expr, &inst.index, index)?,
                    None => i64::from(field.bit_offset.unwrap_or(0)),
                };
                bit_offsets.push(bit);
            }
            Ok::<_, String>(EmittedInstances {
                index: inst.index.clone(),
                first: inst.start,
                last: inst.end,
                offsets,
                bit_offsets,
            })
        })
        .transpose()?;

    // Scaling (with variants resolved against the whole schema set).
    let (scaling, discriminator) = match &field.variants {
        None => {
            let scaling = if field.ty == FieldType::F16Be {
                f16_scaling(field)?
            } else {
                match &field.encoding {
                    None => EmittedScaling::None,
                    Some(encoding) => match encoding.as_mapping() {
                        // Plain-string encodings (for example `encoding: bcd`)
                        // declare no numeric scaling.
                        None => match encoding.as_str() {
                            Some(kind) => EmittedScaling::Other {
                                kind: kind.to_string(),
                            },
                            None => {
                                return Err(format!(
                                    "field {}: unsupported encoding {encoding:?}",
                                    field.name
                                ))
                            }
                        },
                        Some(map) => {
                            match map.get("kind").and_then(|kind| kind.as_str()) {
                                // A kindless encoding mapping (for example
                                // `valid_range`) declares constraints, not
                                // scaling: the raw code is the value, like the
                                // page decoders.
                                None => EmittedScaling::None,
                                Some("scaled_signed" | "scaled_unsigned") => {
                                    EmittedScaling::Scaled(scaled_from_encoding(encoding)?)
                                }
                                Some(kind) => EmittedScaling::Other {
                                    kind: kind.to_string(),
                                },
                            }
                        }
                    },
                }
            };
            (scaling, None)
        }
        Some(variants) => {
            // Only the SFF-8472 calibration-variant shape (boolean
            // discriminator, scaled internal case, external_calibration
            // external case) is modeled; every other variant shape (numeric
            // polymorphic dispatch, bitmap unions) is cataloged as an
            // undecodable union instead of a fabricated scaling.
            match calibration_variants(page, field, variants, pages) {
                Ok(Some(resolved)) => resolved,
                Ok(None) => (
                    EmittedScaling::Other {
                        kind: "variants".to_string(),
                    },
                    None,
                ),
                Err(e) => return Err(e),
            }
        }
    };

    let read_effects: Vec<&'static str> = field
        .read_effects()
        .into_iter()
        .map(read_effect_kind)
        .collect();
    let change_source = field.change_source().map(|source| match source {
        ChangeSourceMetadata::Static => "Static",
        ChangeSourceMetadata::Dynamic => "Dynamic",
        ChangeSourceMetadata::Unknown => "Unknown",
    });
    let renamed_from = field
        .renamed_from
        .as_ref()
        .map(|r| (r.rev.clone(), r.old_name.clone()));
    let aliased_to: Vec<(String, String)> = field
        .aliased_to
        .as_ref()
        .map(|aliases| {
            aliases
                .iter()
                .map(|a| (a.rev.clone(), a.new_name.clone()))
                .collect()
        })
        .unwrap_or_default();
    let base_offset = field_offset(
        field,
        field.instances.as_ref().map(|instances| instances.start),
    )
    .map_err(|e| format!("field {}: {}", field.name, e))?;
    let base_bit_offset = match &field.bit_offset_expr {
        Some(expr) => {
            let var = field
                .instances
                .as_ref()
                .map(|i| i.index.as_str())
                .unwrap_or("");
            compute(
                expr,
                var,
                field.instances.as_ref().map(|i| i.start).unwrap_or(0),
            )
            .map_err(|e| format!("field {}: {}", field.name, e))?
        }
        None => i64::from(field.bit_offset.unwrap_or(0)),
    };
    let len = u16::try_from(field.size_bytes.unwrap_or(1)).unwrap_or(u16::MAX);

    let checked_offset =
        u16::try_from(base_offset).map_err(|_| format!("field {}: offset overflow", field.name))?;
    let checked_bit_offset = u8::try_from(base_bit_offset.max(0))
        .map_err(|_| format!("field {}: bit offset overflow", field.name))?;
    let checked_bit_width = u16::try_from(bit_width_of(field))
        .map_err(|_| format!("field {}: bit width overflow", field.name))?;

    Ok(DerivedRow {
        name: field.name.clone(),
        display_name: field.display_name.clone(),
        page_id: page.schema.id.clone(),
        page_label: page.schema.page_address.clone(),
        address: page.address,
        page: page.page,
        banked: page.banked,
        offset: checked_offset,
        len,
        bit_offset: checked_bit_offset,
        bit_width: checked_bit_width,
        kind: field_kind(&field.ty),
        enum_ref: field.enum_ref.clone(),
        scaling,
        access: access_kind(field.access),
        read_effects,
        change_source,
        instances,
        since: None,
        until: None,
        renamed_from,
        aliased_to,
        tags: field.tags.clone(),
        category: field.field_category.clone(),
        unit: field.unit.clone(),
        discriminator,
        presence: presence.clone(),
    })
}

/// One revision-span prerequisite check: at `rev`, an Equals prerequisite
/// must be applicable, and a content-multiplexed one must carry a settling
/// bound. Failing either is a generation error naming the field, reference
/// and revision. Single-revision families never call this (their span model
/// has one revision and classification already resolved the reference).
fn check_span_prerequisite(
    page: &PageLocation,
    field: &PageField,
    presence: &EmittedPresence,
    rev: Rev,
) -> Result<(), String> {
    let EmittedPresence::Equals(equals) = presence else {
        return Ok(());
    };
    if !equals.applicable.is_empty() && !equals.applicable.contains(&rev) {
        return Err(format!(
            "{}: field {}: reference {:?} does not apply at revision {} \
             (prerequisite applies at {:?})",
            page.schema.id,
            field.name,
            equals.field,
            rev.as_str(),
            equals
                .applicable
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
        ));
    }
    if let Some(switch) = &equals.content_switch {
        if switch.settling_at(rev).is_none() {
            return Err(format!(
                "{}: field {}: reference {:?} carries a content_switch whose settling bound \
                 does not cover revision {}",
                page.schema.id,
                field.name,
                equals.field,
                rev.as_str()
            ));
        }
    }
    Ok(())
}

/// Build the page locations for a spec's schemas.
pub(crate) fn page_locations(
    schemas: &[PageSchema],
    spec: &str,
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<Vec<PageLocation>, String> {
    schemas
        .iter()
        .map(|schema| page_location(schema, spec, info))
        .collect()
}

/// Derive all rows for a spec, pages sorted by schema id for determinism.
/// Presence is classified once for the whole spec (fail-loud on malformed
/// or unresolvable conditions) and threaded into every revision-span row.
pub fn derive_rows(
    schemas: &[PageSchema],
    spec: &str,
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<Vec<DerivedRow>, String> {
    let family =
        spec_family(spec).ok_or_else(|| format!("catalog emission does not know spec {spec:?}"))?;
    let mut sorted: Vec<&PageSchema> = schemas.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let pages = page_locations(&sorted.into_iter().cloned().collect::<Vec<_>>(), spec, info)?;
    let classified = crate::presence::classify_presence(&pages, family, info)?;
    let mut rows = Vec::new();
    for page in &pages {
        for field in &page.schema.fields {
            rows.extend(derive_field_rows(page, field, family, &pages, &classified)?);
        }
    }
    Ok(rows)
}

/// Extract [`CatalogPageInfo`] from a spec's page-schema file paths by
/// reading the raw page YAML: the `banking:` block's presence, the
/// SFF-8472 `page_select:` key, and the root file's path (for
/// generation-error messages). Shared by the driver and tests so both
/// derive from the same source.
pub fn page_catalog_info(
    paths: &[std::path::PathBuf],
) -> Result<BTreeMap<String, CatalogPageInfo>, String> {
    let mut map = BTreeMap::new();
    for path in paths {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let doc: serde_norway::Value =
            serde_norway::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let Some(id) = doc.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let banked = doc.get("banking").is_some();
        let page_select = doc
            .get("page_select")
            .and_then(|v| v.as_str())
            .and_then(|label| label.strip_suffix('h'))
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        map.insert(
            id.to_string(),
            CatalogPageInfo {
                banked,
                page_select,
                path: Some(path.display().to_string()),
            },
        );
    }
    Ok(map)
}

// ─── Rust emission ──────────────────────────────────────────────────────────

fn rust_rev(family: &SpecFamily, rev: Option<(u8, u8)>) -> String {
    match rev {
        None => "None".to_string(),
        Some((major, minor)) => {
            if !family.rev_consts.is_empty() {
                let index = family
                    .revs
                    .iter()
                    .position(|&(m, n)| m == major && n == minor)
                    .expect("rev in family table");
                format!("Some({})", family.rev_consts[index])
            } else {
                format!(
                    "Some(Rev {{ family: {}, major: {major}, minor: {minor} }})",
                    family.family_const
                )
            }
        }
    }
}

fn rust_scaled(scaled: &EmittedScaled) -> String {
    let note = match &scaled.note {
        Some(note) => format!("Some({note:?})"),
        None => "None".to_string(),
    };
    format!(
        "ScaledLsb {{ signed: {}, num: {}, den: {}, unit: Unit::{}, note: {} }}",
        scaled.signed, scaled.num, scaled.den, scaled.unit, note
    )
}

fn rust_loc(loc: &EmittedLoc) -> String {
    format!(
        "FieldLoc {{ address: {}, page: {}, offset: {}, len: {} }}",
        loc.address, loc.page, loc.offset, loc.len
    )
}

fn rust_scaling(scaling: &EmittedScaling) -> String {
    match scaling {
        EmittedScaling::None => "Scaling::None".to_string(),
        EmittedScaling::Scaled(scaled) => format!("Scaling::Scaled({})", rust_scaled(scaled)),
        EmittedScaling::Linear { internal, slope, offset } => format!(
            "Scaling::Linear {{ internal: {}, external: Some(ExternalLinear {{ slope: {}, offset: {} }}) }}",
            rust_scaled(internal),
            rust_loc(slope),
            rust_loc(offset)
        ),
        EmittedScaling::Polynomial { internal, coefficients } => format!(
            "Scaling::Polynomial {{ internal: {}, external: Some(ExternalPoly {{ \
             coefficients: [{}] }}) }}",
            rust_scaled(internal),
            coefficients
                .iter()
                .map(rust_loc)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EmittedScaling::Other { kind } => format!("Scaling::Other {{ kind: {kind:?} }}"),
        EmittedScaling::F16 { unit } => {
            format!("Scaling::F16 {{ unit: Unit::{unit} }}")
        }
    }
}

fn rust_row(row: &DerivedRow) -> String {
    let mut out = String::new();
    out.push_str("CatalogRow {\n");
    out.push_str(&format!("name: {:?},\n", row.name));
    // display_name falls back to the schema name when the schema has none.
    let display_name = row.display_name.clone().unwrap_or_else(|| row.name.clone());
    out.push_str(&format!("display_name: {display_name:?},\n"));
    out.push_str(&format!("page_id: {:?},\n", row.page_id));
    out.push_str(&format!("page_label: {:?},\n", row.page_label));
    out.push_str(&format!(
        "address: {}, page: {}, banked: {},\n",
        row.address, row.page, row.banked
    ));
    out.push_str(&format!("offset: {}, len: {},\n", row.offset, row.len));
    out.push_str(&format!(
        "bit_offset: {}, bit_width: {},\n",
        row.bit_offset, row.bit_width
    ));
    out.push_str(&format!("kind: FieldKind::{},\n", row.kind));
    let enum_ref = row
        .enum_ref
        .as_ref()
        .map(|name| format!("Some({name:?})"))
        .unwrap_or_else(|| "None".to_string());
    out.push_str(&format!("enum_ref: {enum_ref},\n"));
    out.push_str(&format!("scaling: {},\n", rust_scaling(&row.scaling)));
    let discriminator = match &row.discriminator {
        Some((path, loc, bit)) => format!(
            "Some(VariantDiscriminator {{ field: {path:?}, loc: {}, bit: {bit} }})",
            rust_loc(loc)
        ),
        None => "None".to_string(),
    };
    out.push_str(&format!("discriminator: {discriminator},\n"));
    out.push_str(&format!("access: AccessKind::{},\n", row.access));
    let effects = row
        .read_effects
        .iter()
        .map(|effect| format!("ReadEffect::{effect}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!("read_effects: &[{effects}],\n"));
    let change_source = row
        .change_source
        .map(|source| format!("Some(ChangeSource::{source})"))
        .unwrap_or_else(|| "None".to_string());
    out.push_str(&format!("change_source: {change_source},\n"));
    let instances = match &row.instances {
        Some(inst) => format!(
            "Some(InstanceDimension {{ index: {:?}, first: {}, last: {}, offsets: &[{}], bit_offsets: &[{}] }})",
            inst.index,
            inst.first,
            inst.last,
            inst.offsets.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(", "),
            inst.bit_offsets.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(", ")
        ),
        None => "None".to_string(),
    };
    out.push_str(&format!("instances: {instances},\n"));
    // since/until need the family for constant names; the caller patches
    // them via `rust_rev` and they are appended by `emit_catalog`.
    out.push_str("__SINCE__,\n__UNTIL__,\n");
    let renamed_from = match &row.renamed_from {
        Some((rev, old_name)) => {
            format!("Some(RenameRecord {{ rev: {rev:?}, name: {old_name:?} }})")
        }
        None => "None".to_string(),
    };
    out.push_str(&format!("renamed_from: {renamed_from},\n"));
    let aliases = row
        .aliased_to
        .iter()
        .map(|(rev, name)| format!("RenameRecord {{ rev: {rev:?}, name: {name:?} }}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!("aliased_to: &[{aliases}],\n"));
    let tags = row
        .tags
        .iter()
        .map(|tag| format!("{tag:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!("tags: &[{tags}],\n"));
    let category = row
        .category
        .as_ref()
        .map(|category| format!("Some({category:?})"))
        .unwrap_or_else(|| "None".to_string());
    out.push_str(&format!("category: {category},\n"));
    let unit = row
        .unit
        .as_ref()
        .map(|unit| format!("Some({unit:?})"))
        .unwrap_or_else(|| "None".to_string());
    out.push_str(&format!("unit: {unit},\n"));
    out.push_str(&format!("presence: {},\n", rust_presence(&row.presence)));
    out.push_str("},\n");
    out
}

/// Render one row's presence metadata as a core `FieldPresence` literal.
fn rust_presence(presence: &EmittedPresence) -> String {
    match presence {
        EmittedPresence::Unconditional => "FieldPresence::Unconditional".to_string(),
        EmittedPresence::Unsupported { expr } => {
            format!("FieldPresence::Unsupported {{ expr: {expr:?} }}")
        }
        EmittedPresence::Equals(equals) => {
            let content_switch = match &equals.content_switch {
                None => "None".to_string(),
                Some(switch) => format!(
                    "Some(ContentSwitch {{ data: {}, bound: &[{}] }})",
                    rust_loc(&switch.data),
                    switch
                        .bound
                        .iter()
                        .map(|bound| {
                            let settling = match bound.settling {
                                crate::presence::Settling::TnackAdvertised => {
                                    "SettlingKind::TnackAdvertised"
                                }
                                crate::presence::Settling::Tddcs => "SettlingKind::Tddcs",
                            };
                            format!(
                                "SwitchBound {{ since: {}, settling: {settling} }}",
                                rust_rev_bound(bound.since)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            format!(
                "FieldPresence::Equals(PrerequisiteEquals {{ \
                 field: {:?}, loc: {}, banked: {}, kind: FieldKind::{}, \
                 bit_offset: {}, bit_width: {}, expected: {}, \
                 content_switch: {content_switch} }})",
                equals.field,
                rust_loc(&equals.loc),
                equals.banked,
                equals.kind,
                equals.bit_offset,
                equals.bit_width,
                equals.expected
            )
        }
    }
}

/// Render a switch bound's `since` as a core `Rev` (or `None`).
fn rust_rev_bound(since: Option<(u8, u8)>) -> String {
    match since {
        None => "None".to_string(),
        Some((major, minor)) => {
            format!("Some(Rev {{ family: Family::Cmis, major: {major}, minor: {minor} }})")
        }
    }
}

/// Derive the family's reported-revision probe from the schemas: the CMIS
/// lower-memory `cmis_revision` row's location (byte 1), schema-derived so
/// the core decoder needs no family-crate dependency. Families without a
/// self-reported revision byte (the SFF families use compliance codes,
/// gated at the facade) emit `None`.
fn rust_revision_probe(schemas: &[PageSchema], spec: &str) -> Result<String, String> {
    match spec {
        "cmis" => {
            let schema = schemas
                .iter()
                .find(|schema| schema.id == "CmisLowerMemory")
                .ok_or_else(|| {
                    "catalog emission: CmisLowerMemory schema absent; \
                     cannot derive the revision probe"
                        .to_string()
                })?;
            let field = schema
                .fields
                .iter()
                .find(|field| field.name == "cmis_revision")
                .ok_or_else(|| {
                    "catalog emission: CmisLowerMemory has no cmis_revision field; \
                     cannot derive the revision probe"
                        .to_string()
                })?;
            let offset = u16::try_from(field.offset.unwrap_or(1))
                .map_err(|_| "cmis_revision offset overflow".to_string())?;
            Ok(format!(
                "Some(RevisionProbe {{ field: \"cmis_revision\", \
                 loc: FieldLoc {{ address: 0x50, page: 0, offset: {offset}, len: 1 }} }})"
            ))
        }
        _ => Ok("None".to_string()),
    }
}

/// Emit the Rust family catalog module for one spec.
pub fn emit_catalog(
    spec: &str,
    schemas: &[PageSchema],
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<String, String> {
    let family =
        spec_family(spec).ok_or_else(|| format!("catalog emission does not know spec {spec:?}"))?;
    let rows = derive_rows(schemas, spec, info)?;
    let mut out = String::new();
    out.push_str("// Generated by openxcvr-codegen. DO NOT EDIT.\n");
    out.push_str(&format!("// Regenerate with: tools/regen-rust.sh {spec}\n"));
    out.push_str(&format!(
        "//! Schema-derived field catalog for {} ({} rows, one per field revision span).\n",
        spec.to_uppercase(),
        rows.len()
    ));
    out.push_str("use openxcvr_core::field_catalog::{\n");
    out.push_str(
        "    AccessKind, CatalogRow, ContentSwitch, ExternalLinear, ExternalPoly, FamilyCatalog,\n",
    );
    out.push_str(
        "    FieldKind, FieldLoc, FieldPresence, InstanceDimension, PrerequisiteEquals,\n",
    );
    out.push_str(
        "    RenameRecord, RevisionProbe, ScaledLsb, Scaling, SettlingKind, SwitchBound, Unit,\n",
    );
    out.push_str("    VariantDiscriminator,\n};\n");
    out.push_str("use openxcvr_core::rev::{Family, Rev};\n");
    out.push_str("use openxcvr_core::types::management::{ChangeSource, ReadEffect};\n\n");
    out.push_str("pub static CATALOG: FamilyCatalog = FamilyCatalog {\n");
    out.push_str(&format!("family: {},\n", family.family_const));
    out.push_str(&format!(
        "revision_probe: {},\n",
        rust_revision_probe(schemas, spec)?
    ));
    if family.rev_consts.is_empty() {
        let revs = family
            .revs
            .iter()
            .map(|(major, minor)| {
                format!(
                    "Rev {{ family: {}, major: {major}, minor: {minor} }}",
                    family.family_const
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("supported_revs: &[{revs}],\n"));
    } else {
        let revs = family
            .rev_consts
            .iter()
            .map(|constant| constant.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("supported_revs: &[{revs}],\n"));
    }
    out.push_str("rows: &[\n");
    for row in &rows {
        let rendered = rust_row(row)
            .replace(
                "__SINCE__",
                &format!("since: {}", rust_rev(family, row.since)),
            )
            .replace(
                "__UNTIL__",
                &format!("until: {}", rust_rev(family, row.until)),
            );
        out.push_str(&rendered);
    }
    out.push_str("],\n};\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fraction_decimal_and_prefixed_units() {
        assert_eq!(parse_lsb("1/256 degC").unwrap(), (1, 256, "Celsius", None));
        assert_eq!(parse_lsb("1/256 deg C").unwrap(), (1, 256, "Celsius", None));
        assert_eq!(parse_lsb("100 uV").unwrap(), (1, 10_000, "Volt", None));
        assert_eq!(parse_lsb("0.1 uW").unwrap(), (1, 10_000_000, "Watt", None));
        assert_eq!(parse_lsb("2 uA").unwrap(), (1, 500_000, "Ampere", None));
        assert_eq!(parse_lsb("0.25 W").unwrap(), (1, 4, "Watt", None));
        assert_eq!(
            parse_lsb("0.001 GHz").unwrap(),
            (1_000_000, 1, "Hertz", None)
        );
        assert_eq!(
            parse_lsb("0.01 dBm").unwrap(),
            (1, 100, "DecibelMilliwatt", None)
        );
        assert_eq!(
            parse_lsb("1 ns").unwrap(),
            (1, 1_000_000_000, "Second", None)
        );
        assert_eq!(parse_lsb("10 m").unwrap(), (10, 1, "Meter", None));
        assert_eq!(parse_lsb("1 ms").unwrap(), (1, 1_000, "Second", None));
        assert_eq!(
            parse_lsb("250 MBd").unwrap(),
            (250_000_000, 1, "Baud", None)
        );
    }

    #[test]
    fn preserves_caveats_and_counts() {
        let (num, den, unit, note) =
            parse_lsb("2 uA times TxBiasCurrentScalingFactor (Page 01h:160)").unwrap();
        assert_eq!((num, den, unit), (1, 500_000, "Ampere"));
        assert_eq!(
            note.as_deref(),
            Some("times TxBiasCurrentScalingFactor (Page 01h:160)")
        );
        let (num, den, unit, note) = parse_lsb("repetitions (0=stop, 7=continuous)").unwrap();
        assert_eq!((num, den, unit), (1, 1, "Count"));
        assert_eq!(note.as_deref(), Some("repetitions (0=stop, 7=continuous)"));
    }

    #[test]
    fn unknown_unit_fails_loud() {
        assert!(parse_lsb("3 apples").is_err());
    }

    #[test]
    fn u64be_catalog_kind_is_big_endian() {
        use crate::ir::pages::FieldType;
        assert_eq!(field_kind(&FieldType::U64Be), "U64Be");
        assert_eq!(field_kind(&FieldType::U64Le), "U64Le");
    }
}
