//! C field-catalog emission: one generated header/source pair per spec under
//! `bindings/c/generated/catalog/`. The shared row types are emitted once per
//! header behind a common include guard so any subset of the three spec
//! headers can be included together.
use std::collections::BTreeMap;

use crate::emitters::rust::catalog::{
    derive_rows, spec_family, CatalogPageInfo, EmittedLoc, EmittedScaling,
};
use crate::ir::pages::PageSchema;
use crate::presence::{EmittedPresence, Settling};

fn file_stem(spec: &str) -> String {
    spec.replace('-', "")
}

/// The shared type block, identical in every generated catalog header.
fn type_block() -> String {
    let mut out = String::new();
    out.push_str("#ifndef OPENXCVR_CATALOG_TYPES\n#define OPENXCVR_CATALOG_TYPES\n\n");
    out.push_str(concat!(
        "typedef enum openxcvr_access_kind {\n",
        "    OPENXCVR_ACCESS_RO = 0,\n",
        "    OPENXCVR_ACCESS_RW,\n",
        "    OPENXCVR_ACCESS_RWW,\n",
        "    OPENXCVR_ACCESS_WO,\n",
        "    OPENXCVR_ACCESS_WO_SC,\n",
        "    OPENXCVR_ACCESS_RO_COR,\n",
        "    OPENXCVR_ACCESS_UNKNOWN\n",
        "} openxcvr_access_kind_t;\n\n",
        "typedef enum openxcvr_field_kind {\n",
        "    OPENXCVR_FIELD_BOOL = 0,\n",
        "    OPENXCVR_FIELD_U2,\n",
        "    OPENXCVR_FIELD_U3,\n",
        "    OPENXCVR_FIELD_U4,\n",
        "    OPENXCVR_FIELD_U5,\n",
        "    OPENXCVR_FIELD_U6,\n",
        "    OPENXCVR_FIELD_U7,\n",
        "    OPENXCVR_FIELD_U8,\n",
        "    OPENXCVR_FIELD_I8,\n",
        "    OPENXCVR_FIELD_U16BE,\n",
        "    OPENXCVR_FIELD_I16BE,\n",
        "    OPENXCVR_FIELD_U16LE,\n",
        "    OPENXCVR_FIELD_U32BE,\n",
        "    OPENXCVR_FIELD_I32BE,\n",
        "    OPENXCVR_FIELD_U64LE,\n",
        "    OPENXCVR_FIELD_U64BE,\n",
        "    OPENXCVR_FIELD_F16BE,\n",
        "    OPENXCVR_FIELD_X16BE,\n",
        "    OPENXCVR_FIELD_ASCII,\n",
        "    OPENXCVR_FIELD_BYTES,\n",
        "    OPENXCVR_FIELD_BITMAP,\n",
        "    OPENXCVR_FIELD_COMPOSITE,\n",
        "    OPENXCVR_FIELD_ENUM_REF\n",
        "} openxcvr_field_kind_t;\n\n",
        "typedef enum openxcvr_unit {\n",
        "    OPENXCVR_UNIT_CELSIUS = 0,\n",
        "    OPENXCVR_UNIT_VOLT,\n",
        "    OPENXCVR_UNIT_WATT,\n",
        "    OPENXCVR_UNIT_AMPERE,\n",
        "    OPENXCVR_UNIT_DECIBEL,\n",
        "    OPENXCVR_UNIT_DECIBEL_MILLIWATT,\n",
        "    OPENXCVR_UNIT_HERTZ,\n",
        "    OPENXCVR_UNIT_METER,\n",
        "    OPENXCVR_UNIT_SECOND,\n",
        "    OPENXCVR_UNIT_BAUD,\n",
        "    OPENXCVR_UNIT_COUNT,\n",
        "    OPENXCVR_UNIT_RATIO\n",
        "} openxcvr_unit_t;\n\n",
        "typedef enum openxcvr_change_source {\n",
        "    OPENXCVR_CHANGE_STATIC = 0,\n",
        "    OPENXCVR_CHANGE_DYNAMIC\n",
        "} openxcvr_change_source_t;\n\n",
        "/* Read-effect bitmask over one byte's worth of declared effects. */\n",
        "enum {\n",
        "    OPENXCVR_READ_EFFECT_NONE = 1,\n",
        "    OPENXCVR_READ_EFFECT_CLEAR_ON_READ = 2,\n",
        "    OPENXCVR_READ_EFFECT_ACK_STARTUP = 4,\n",
        "    OPENXCVR_READ_EFFECT_UNKNOWN = 8,\n",
        "    OPENXCVR_READ_EFFECT_WRITE_ONLY = 16\n",
    ));
    out.push_str(concat!(
        "};\n\n",
        "typedef struct openxcvr_field_loc {\n",
        "    uint8_t address;\n",
        "    uint8_t page;\n",
        "    uint16_t offset;\n",
        "    uint16_t len;\n",
        "} openxcvr_field_loc_t;\n\n",
        "typedef struct openxcvr_scaled_lsb {\n",
        "    int is_signed;\n",
        "    uint32_t num;\n",
        "    uint32_t den;\n",
        "    openxcvr_unit_t unit;\n",
        "    const char *note;\n",
        "} openxcvr_scaled_lsb_t;\n\n",
        "typedef struct openxcvr_external_linear {\n",
        "    openxcvr_field_loc_t slope;\n",
        "    openxcvr_field_loc_t offset;\n",
        "} openxcvr_external_linear_t;\n\n",
        "typedef struct openxcvr_external_poly {\n",
        "    openxcvr_field_loc_t coefficients[5];\n",
        "} openxcvr_external_poly_t;\n\n",
        "typedef enum openxcvr_scaling_kind {\n",
        "    OPENXCVR_SCALING_NONE = 0,\n",
        "    OPENXCVR_SCALING_SCALED,\n",
        "    OPENXCVR_SCALING_LINEAR,\n",
        "    OPENXCVR_SCALING_POLYNOMIAL,\n",
        "    OPENXCVR_SCALING_OTHER,\n",
        "    /* F16 scaling uses only the `scaled.unit` member (num/den 0). */\n",
        "    OPENXCVR_SCALING_F16\n",
        "} openxcvr_scaling_kind_t;\n\n",
        "typedef struct openxcvr_scaling {\n",
        "    openxcvr_scaling_kind_t kind;\n",
        "    /* SCALED, and the internal part of LINEAR/POLYNOMIAL. */\n",
        "    openxcvr_scaled_lsb_t scaled;\n",
        "    openxcvr_external_linear_t linear;\n",
        "    openxcvr_external_poly_t poly;\n",
        "    const char *other_kind;\n",
        "} openxcvr_scaling_t;\n\n",
        "typedef struct openxcvr_variant_discriminator {\n",
        "    const char *field;\n",
        "    openxcvr_field_loc_t loc;\n",
        "    uint8_t bit;\n",
        "} openxcvr_variant_discriminator_t;\n\n",
        "typedef struct openxcvr_instance_dimension {\n",
        "    const char *index;\n",
        "    uint16_t first;\n",
        "    uint16_t last;\n",
        "    const uint16_t *offsets;\n",
        "    const uint8_t *bit_offsets;\n",
        "    uint16_t count;\n",
        "} openxcvr_instance_dimension_t;\n\n",
        "typedef struct openxcvr_rename_record {\n",
        "    const char *rev;\n",
        "    const char *name;\n",
        "} openxcvr_rename_record_t;\n\n",
        "/* How a content switch settles, per the revision the decode runs at\n",
        " * (Stage 1 B2; integer-only metadata - no floating point here). */\n",
        "typedef enum openxcvr_settling_kind {\n",
        "    OPENXCVR_SETTLING_TNACK_ADVERTISED = 0,\n",
        "    OPENXCVR_SETTLING_TDDCS\n",
        "} openxcvr_settling_kind_t;\n\n",
        "typedef struct openxcvr_switch_bound {\n",
        "    int has_since;\n",
        "    uint8_t since_major;\n",
        "    uint8_t since_minor;\n",
        "    openxcvr_settling_kind_t settling;\n",
        "} openxcvr_switch_bound_t;\n\n",
        "typedef struct openxcvr_content_switch {\n",
        "    /* The multiplexed data window (window offsets). */\n",
        "    openxcvr_field_loc_t data;\n",
        "    const openxcvr_switch_bound_t *bounds;\n",
        "    size_t bounds_len;\n",
        "} openxcvr_content_switch_t;\n\n",
        "typedef struct openxcvr_prerequisite_equals {\n",
        "    const char *field;\n",
        "    openxcvr_field_loc_t loc;\n",
        "    int banked;\n",
        "    openxcvr_field_kind_t kind;\n",
        "    uint8_t bit_offset;\n",
        "    uint16_t bit_width;\n",
        "    uint64_t expected;\n",
        "    int has_content_switch;\n",
        "    openxcvr_content_switch_t content_switch;\n",
        "} openxcvr_prerequisite_equals_t;\n\n",
        "typedef enum openxcvr_presence_kind {\n",
        "    OPENXCVR_PRESENCE_UNCONDITIONAL = 0,\n",
        "    OPENXCVR_PRESENCE_EQUALS,\n",
        "    OPENXCVR_PRESENCE_UNSUPPORTED\n",
        "} openxcvr_presence_kind_t;\n\n",
        "typedef struct openxcvr_catalog_row {\n",
        "    const char *name;\n",
        "    const char *display_name;\n",
        "    const char *page_id;\n",
        "    const char *page_label;\n",
        "    uint8_t address;\n",
        "    uint8_t page;\n",
        "    int banked;\n",
        "    uint16_t offset;\n",
        "    uint16_t len;\n",
        "    uint8_t bit_offset;\n",
        "    uint16_t bit_width;\n",
        "    openxcvr_field_kind_t kind;\n",
        "    const char *enum_ref;\n",
        "    openxcvr_scaling_t scaling;\n",
        "    int has_discriminator;\n",
        "    openxcvr_variant_discriminator_t discriminator;\n",
        "    openxcvr_access_kind_t access;\n",
        "    uint8_t read_effect_mask;\n",
        "    int has_change_source;\n",
        "    openxcvr_change_source_t change_source;\n",
        "    int has_instances;\n",
        "    openxcvr_instance_dimension_t instances;\n",
        "    uint8_t since_major;\n",
        "    uint8_t since_minor;\n",
        "    int has_since;\n",
        "    uint8_t until_major;\n",
        "    uint8_t until_minor;\n",
        "    int has_until;\n",
        "    int has_renamed_from;\n",
        "    openxcvr_rename_record_t renamed_from;\n",
        "    const openxcvr_rename_record_t *aliased_to;\n",
        "    size_t aliased_to_len;\n",
        "    const char *const *tags;\n",
        "    size_t tags_len;\n",
        "    const char *category;\n",
        "    const char *unit;\n",
        "    /* Stage 1 B1: field-level presence from present_when. Appended\n",
        "     * members - a coordinated ABI stride change: rebuild headers,\n",
        "     * catalogs, runtime and consumers together. */\n",
        "    openxcvr_presence_kind_t presence_kind;\n",
        "    /* Valid when presence_kind is OPENXCVR_PRESENCE_EQUALS. */\n",
        "    openxcvr_prerequisite_equals_t presence_equals;\n",
        "    /* Valid when presence_kind is OPENXCVR_PRESENCE_UNSUPPORTED. */\n",
        "    const char *presence_expr;\n",
        "} openxcvr_catalog_row_t;\n\n",
        "typedef struct openxcvr_rev {\n",
        "    uint8_t major;\n",
        "    uint8_t minor;\n",
        "} openxcvr_rev_t;\n\n",
        "/* `family` holds an openxcvr_spec_kind_t value (1 SFF-8472, 2 SFF-8636,\n",
        " * 3 CMIS); the generated headers cannot include openxcvr.h, which\n",
        " * includes this umbrella in turn. */\n",
        "typedef struct openxcvr_family_catalog {\n",
        "    int family;\n",
        "    const openxcvr_rev_t *supported_revs;\n",
        "    size_t supported_rev_count;\n",
        "    const openxcvr_catalog_row_t *rows;\n",
        "    size_t row_count;\n",
        "} openxcvr_family_catalog_t;\n\n",
        "#endif /* OPENXCVR_CATALOG_TYPES */\n\n",
    ));
    out
}

fn c_unit(unit: &str) -> &'static str {
    match unit {
        "Celsius" => "OPENXCVR_UNIT_CELSIUS",
        "Volt" => "OPENXCVR_UNIT_VOLT",
        "Watt" => "OPENXCVR_UNIT_WATT",
        "Ampere" => "OPENXCVR_UNIT_AMPERE",
        "Decibel" => "OPENXCVR_UNIT_DECIBEL",
        "DecibelMilliwatt" => "OPENXCVR_UNIT_DECIBEL_MILLIWATT",
        "Hertz" => "OPENXCVR_UNIT_HERTZ",
        "Meter" => "OPENXCVR_UNIT_METER",
        "Second" => "OPENXCVR_UNIT_SECOND",
        "Baud" => "OPENXCVR_UNIT_BAUD",
        "Count" => "OPENXCVR_UNIT_COUNT",
        "Ratio" => "OPENXCVR_UNIT_RATIO",
        other => unreachable!("unknown unit {other}"),
    }
}

fn c_field_kind(kind: &str) -> &'static str {
    match kind {
        "Bool" => "OPENXCVR_FIELD_BOOL",
        "U2" => "OPENXCVR_FIELD_U2",
        "U3" => "OPENXCVR_FIELD_U3",
        "U4" => "OPENXCVR_FIELD_U4",
        "U5" => "OPENXCVR_FIELD_U5",
        "U6" => "OPENXCVR_FIELD_U6",
        "U7" => "OPENXCVR_FIELD_U7",
        "U8" => "OPENXCVR_FIELD_U8",
        "I8" => "OPENXCVR_FIELD_I8",
        "U16Be" => "OPENXCVR_FIELD_U16BE",
        "I16Be" => "OPENXCVR_FIELD_I16BE",
        "U16Le" => "OPENXCVR_FIELD_U16LE",
        "U32Be" => "OPENXCVR_FIELD_U32BE",
        "I32Be" => "OPENXCVR_FIELD_I32BE",
        "U64Le" => "OPENXCVR_FIELD_U64LE",
        "U64Be" => "OPENXCVR_FIELD_U64BE",
        "F16Be" => "OPENXCVR_FIELD_F16BE",
        "X16Be" => "OPENXCVR_FIELD_X16BE",
        "Ascii" => "OPENXCVR_FIELD_ASCII",
        "Bytes" => "OPENXCVR_FIELD_BYTES",
        "Bitmap" => "OPENXCVR_FIELD_BITMAP",
        "Composite" => "OPENXCVR_FIELD_COMPOSITE",
        "EnumRef" => "OPENXCVR_FIELD_ENUM_REF",
        other => unreachable!("unknown field kind {other}"),
    }
}

fn c_access(access: &str) -> &'static str {
    match access {
        "Ro" => "OPENXCVR_ACCESS_RO",
        "Rw" => "OPENXCVR_ACCESS_RW",
        "Rww" => "OPENXCVR_ACCESS_RWW",
        "Wo" => "OPENXCVR_ACCESS_WO",
        "WoSc" => "OPENXCVR_ACCESS_WO_SC",
        "RoCor" => "OPENXCVR_ACCESS_RO_COR",
        "Unknown" => "OPENXCVR_ACCESS_UNKNOWN",
        other => unreachable!("unknown access {other}"),
    }
}

fn c_change_source(source: &str) -> &'static str {
    match source {
        "Static" => "OPENXCVR_CHANGE_STATIC",
        "Dynamic" => "OPENXCVR_CHANGE_DYNAMIC",
        other => unreachable!("unknown change source {other}"),
    }
}

fn c_read_effect_mask(effects: &[&'static str]) -> u8 {
    let mut mask = 0u8;
    for effect in effects {
        mask |= match *effect {
            "None" => 1,
            "ClearOnRead" => 2,
            "AcknowledgeStartupInterrupt" => 4,
            "Unknown" => 8,
            "WriteOnly" => 16,
            other => unreachable!("unknown read effect {other}"),
        };
    }
    mask
}

fn c_loc_initializer(loc: &EmittedLoc) -> String {
    format!(
        "{{ {o}, {p}, {off}, {len} }}",
        o = loc.address,
        p = loc.page,
        off = loc.offset,
        len = loc.len
    )
}

/// Emit the generated catalog header for one spec.
pub fn emit_catalog_c_header(
    spec: &str,
    schemas: &[PageSchema],
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<String, String> {
    let family =
        spec_family(spec).ok_or_else(|| format!("catalog emission does not know spec {spec:?}"))?;
    let rows = derive_rows(schemas, spec, info)?;
    let stem = file_stem(spec);
    let guard = format!("OPENXCVR_GENERATED_CATALOG_{}_H", stem.to_uppercase());
    let mut out = String::new();
    out.push_str("/* Generated by openxcvr-codegen. DO NOT EDIT. */\n");
    out.push_str(&format!("#ifndef {guard}\n#define {guard}\n\n"));
    out.push_str("#include <stddef.h>\n#include <stdint.h>\n\n");
    out.push_str(&type_block());
    out.push_str(&format!(
        "extern const openxcvr_family_catalog_t openxcvr_{stem}_catalog;\n"
    ));
    out.push_str(&format!(
        "/* {rows} field revision span row(s) for {spec}. */\n",
        rows = rows.len()
    ));
    let _ = family;
    out.push_str(&format!("#endif /* {guard} */\n"));
    Ok(out)
}

fn scaling_initializer(scaling: &EmittedScaling) -> String {
    let scaled_init = |scaled: &crate::emitters::rust::catalog::EmittedScaled| {
        let note = scaled
            .note
            .as_ref()
            .map(|note| format!("{note:?}"))
            .unwrap_or_else(|| "NULL".to_string());
        format!(
            "{{ {signed}, {num}, {den}, {unit}, {note} }}",
            signed = if scaled.signed { 1 } else { 0 },
            num = scaled.num,
            den = scaled.den,
            unit = c_unit(scaled.unit),
            note = note,
        )
    };
    match scaling {
        EmittedScaling::None => "{ OPENXCVR_SCALING_NONE, { 0, 0, 0, OPENXCVR_UNIT_COUNT, NULL }, { { 0, 0, 0, 0 }, { 0, 0, 0, 0 } }, { { { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 }, { 0, 0, 0, 0 } } }, NULL }".to_string(),
        EmittedScaling::Scaled(scaled) => {
            let inner = scaled_init(scaled);
            format!("{{ OPENXCVR_SCALING_SCALED, {inner}, {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }}, {{ {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }} }}, NULL }}")
        }
        EmittedScaling::Linear { internal, slope, offset } => {
            let inner = scaled_init(internal);
            let linear = format!(
                "{{ .slope = {slope}, .offset = {offset} }}",
                slope = c_loc_initializer(slope),
                offset = c_loc_initializer(offset),
            );
            format!("{{ OPENXCVR_SCALING_LINEAR, {inner}, {linear}, {{ {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }} }}, NULL }}")
        }
        EmittedScaling::Polynomial { internal, coefficients } => {
            let inner = scaled_init(internal);
            let locs: Vec<String> = coefficients.iter().map(c_loc_initializer).collect();
            let poly = format!(
                "{{ {{ {locs} }} }}",
                locs = locs.join(", ")
            );
            format!("{{ OPENXCVR_SCALING_POLYNOMIAL, {inner}, {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }}, {poly}, NULL }}")
        }
        EmittedScaling::Other { kind } => {
            format!("{{ OPENXCVR_SCALING_OTHER, {{ 0, 0, 0, OPENXCVR_UNIT_COUNT, NULL }}, {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }}, {{ {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }} }}, {kind:?} }}")
        }
        EmittedScaling::F16 { unit } => {
            // Like the NONE initializer, with the F16 kind and the unit in
            // `scaled.unit` — the only member F16 rows use.
            format!(
                "{{ OPENXCVR_SCALING_F16, {{ 0, 0, 0, {}, NULL }}, {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }}, {{ {{ {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }}, {{ 0, 0, 0, 0 }} }} }}, NULL }}",
                c_unit(unit)
            )
        }
    }
}

/// Render one row's presence metadata as C initializers for the appended
/// `presence_*` members (integer-only, tagged).
fn c_presence_initializer(
    presence: &EmittedPresence,
    stem: &str,
    counter: &mut usize,
    statics: &mut String,
) -> String {
    match presence {
        EmittedPresence::Unconditional => "        .presence_kind = OPENXCVR_PRESENCE_UNCONDITIONAL,\n"
            .to_string(),
        EmittedPresence::Unsupported { expr } => format!(
            "        .presence_kind = OPENXCVR_PRESENCE_UNSUPPORTED,\n        .presence_expr = {expr:?},\n"
        ),
        EmittedPresence::Equals(equals) => {
            let (has_switch, switch_init) = match &equals.content_switch {
                None => ("0", String::new()),
                Some(switch) => {
                    let bounds_name = format!("openxcvr_{stem}_bounds_{counter}");
                    *counter += 1;
                    let bounds: Vec<String> = switch
                        .bound
                        .iter()
                        .map(|bound| {
                            let settling = match bound.settling {
                                Settling::TnackAdvertised => "OPENXCVR_SETTLING_TNACK_ADVERTISED",
                                Settling::Tddcs => "OPENXCVR_SETTLING_TDDCS",
                            };
                            match bound.since {
                                None => format!(
                                    "            {{ .has_since = 0, .since_major = 0, .since_minor = 0, .settling = {settling} }},\n"
                                ),
                                Some((major, minor)) => format!(
                            "            {{ .has_since = 1, .since_major = {major}, .since_minor = {minor}, .settling = {settling} }},\n"
                        ),
                            }
                        })
                        .collect();
                    statics.push_str(&format!(
                        "static const openxcvr_switch_bound_t {bounds_name}[{count}] = {{\n{bounds}}};\n",
                        count = switch.bound.len(),
                        bounds = bounds.join(""),
                    ));
                    (
                        "1",
                        format!(
                            "        .presence_equals.content_switch = {{ .data = {data}, .bounds = {bounds_name}, .bounds_len = {len} }},\n",
                            data = c_loc_initializer(&switch.data),
                            len = switch.bound.len(),
                        ),
                    )
                }
            };
            format!(
                "        .presence_kind = OPENXCVR_PRESENCE_EQUALS,\n        .presence_equals = {{ .field = {field:?}, .loc = {loc}, .banked = {banked}, .kind = {kind}, .bit_offset = {bit_offset}, .bit_width = {bit_width}, .expected = {expected}, .has_content_switch = {has_switch} }},\n{switch_init}\n",
                field = equals.field,
                loc = c_loc_initializer(&equals.loc),
                banked = if equals.banked { 1 } else { 0 },
                kind = c_field_kind(equals.kind),
                bit_offset = equals.bit_offset,
                bit_width = equals.bit_width,
                expected = equals.expected,
            )
        }
    }
}

/// Emit the generated catalog source for one spec.
pub fn emit_catalog_c_source(
    spec: &str,
    schemas: &[PageSchema],
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<String, String> {
    let family =
        spec_family(spec).ok_or_else(|| format!("catalog emission does not know spec {spec:?}"))?;
    let rows = derive_rows(schemas, spec, info)?;
    let stem = file_stem(spec);
    let family_value = match family.spec {
        "sff-8472" => 1,
        "sff-8636" => 2,
        "cmis" => 3,
        other => unreachable!("unknown spec {other}"),
    };
    let mut out = String::new();
    out.push_str("/* Generated by openxcvr-codegen. DO NOT EDIT. */\n");
    out.push_str(&format!("#include \"catalog/{stem}_catalog.h\"\n\n"));
    // Static arrays for instance dimensions, aliases and tags.
    let mut statics = String::new();
    let mut counter = 0usize;
    let mut row_inits = String::new();
    for row in &rows {
        let mut init = String::new();
        init.push_str("    {\n");
        init.push_str(&format!("        .name = {:?},\n", row.name));
        match &row.display_name {
            Some(display) => init.push_str(&format!("        .display_name = {display:?},\n")),
            None => init.push_str("        .display_name = NULL,\n"),
        }
        init.push_str(&format!("        .page_id = {:?},\n", row.page_id));
        init.push_str(&format!("        .page_label = {:?},\n", row.page_label));
        init.push_str(&format!(
            "        .address = {address}, .page = {page}, .banked = {banked},\n",
            address = row.address,
            page = row.page,
            banked = if row.banked { 1 } else { 0 },
        ));
        init.push_str(&format!(
            "        .offset = {offset}, .len = {len},\n",
            offset = row.offset,
            len = row.len,
        ));
        init.push_str(&format!(
            "        .bit_offset = {bit_offset}, .bit_width = {bit_width},\n",
            bit_offset = row.bit_offset,
            bit_width = row.bit_width,
        ));
        init.push_str(&format!("        .kind = {},\n", c_field_kind(row.kind)));
        match &row.enum_ref {
            Some(enum_ref) => init.push_str(&format!("        .enum_ref = {enum_ref:?},\n")),
            None => init.push_str("        .enum_ref = NULL,\n"),
        }
        let scaling_init = scaling_initializer(&row.scaling);
        init.push_str(&format!("        .scaling = {scaling_init},\n"));
        match &row.discriminator {
            Some((path, loc, bit)) => init.push_str(&format!(
                "        .has_discriminator = 1, .discriminator = {{ .field = {path:?}, .loc = {}, .bit = {bit} }},\n",
                c_loc_initializer(loc)
            )),
            None => init.push_str("        .has_discriminator = 0, .discriminator = { .field = NULL, .loc = { 0, 0, 0, 0 }, .bit = 0 },\n"),
        }
        init.push_str(&format!("        .access = {},\n", c_access(row.access)));
        init.push_str(&format!(
            "        .read_effect_mask = {mask},\n",
            mask = c_read_effect_mask(&row.read_effects)
        ));
        match row.change_source {
            Some(source) => init.push_str(&format!(
                "        .has_change_source = 1, .change_source = {},\n",
                c_change_source(source)
            )),
            None => init.push_str(
                "        .has_change_source = 0, .change_source = OPENXCVR_CHANGE_STATIC,\n",
            ),
        }
        match &row.instances {
            Some(inst) => {
                let offsets_name = format!("openxcvr_{stem}_offsets_{counter}");
                let bit_offsets_name = format!("openxcvr_{stem}_bit_offsets_{counter}");
                counter += 1;
                statics.push_str(&format!(
                    "static const uint16_t {offsets_name}[{count}] = {{ {offsets} }};\n",
                    count = inst.offsets.len(),
                    offsets = inst
                        .offsets
                        .iter()
                        .map(|o| o.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                statics.push_str(&format!(
                    "static const uint8_t {bit_offsets_name}[{count}] = {{ {bit_offsets} }};\n",
                    count = inst.bit_offsets.len(),
                    bit_offsets = inst
                        .bit_offsets
                        .iter()
                        .map(|o| o.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
                init.push_str(&format!(
                    "        .has_instances = 1, .instances = {{ .index = {:?}, .first = {}, .last = {}, .offsets = {offsets_name}, .bit_offsets = {bit_offsets_name}, .count = {count} }},\n",
                    inst.index,
                    inst.first,
                    inst.last,
                    count = inst.offsets.len(),
                ));
            }
            None => init.push_str("        .has_instances = 0, .instances = { .index = NULL, .first = 0, .last = 0, .offsets = NULL, .bit_offsets = NULL, .count = 0 },\n"),
        }
        match row.since {
            Some((major, minor)) => init.push_str(&format!(
                "        .since_major = {major}, .since_minor = {minor}, .has_since = 1,\n"
            )),
            None => init.push_str("        .since_major = 0, .since_minor = 0, .has_since = 0,\n"),
        }
        match row.until {
            Some((major, minor)) => init.push_str(&format!(
                "        .until_major = {major}, .until_minor = {minor}, .has_until = 1,\n"
            )),
            None => init.push_str("        .until_major = 0, .until_minor = 0, .has_until = 0,\n"),
        }
        match &row.renamed_from {
            Some((rev, old_name)) => init.push_str(&format!(
                "        .has_renamed_from = 1, .renamed_from = {{ .rev = {rev:?}, .name = {old_name:?} }},\n"
            )),
            None => init.push_str("        .has_renamed_from = 0, .renamed_from = { .rev = NULL, .name = NULL },\n"),
        }
        if row.aliased_to.is_empty() {
            init.push_str("        .aliased_to = NULL, .aliased_to_len = 0,\n");
        } else {
            let aliases_name = format!("openxcvr_{stem}_aliases_{counter}");
            counter += 1;
            let entries = row
                .aliased_to
                .iter()
                .map(|(rev, name)| format!("{{ .rev = {rev:?}, .name = {name:?} }}"))
                .collect::<Vec<_>>()
                .join(", ");
            statics.push_str(&format!(
                "static const openxcvr_rename_record_t {aliases_name}[{count}] = {{ {entries} }};\n",
                count = row.aliased_to.len(),
            ));
            init.push_str(&format!(
                "        .aliased_to = {aliases_name}, .aliased_to_len = {count},\n",
                count = row.aliased_to.len(),
            ));
        }
        if row.tags.is_empty() {
            init.push_str("        .tags = NULL, .tags_len = 0,\n");
        } else {
            let tags_name = format!("openxcvr_{stem}_tags_{counter}");
            counter += 1;
            let entries = row
                .tags
                .iter()
                .map(|tag| format!("{tag:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            statics.push_str(&format!(
                "static const char *const {tags_name}[{count}] = {{ {entries} }};\n",
                count = row.tags.len(),
            ));
            init.push_str(&format!(
                "        .tags = {tags_name}, .tags_len = {count},\n",
                count = row.tags.len(),
            ));
        }
        match &row.category {
            Some(category) => init.push_str(&format!("        .category = {category:?},\n")),
            None => init.push_str("        .category = NULL,\n"),
        }
        match &row.unit {
            Some(unit) => init.push_str(&format!("        .unit = {unit:?},\n")),
            None => init.push_str("        .unit = NULL,\n"),
        }
        init.push_str(&c_presence_initializer(
            &row.presence,
            &stem,
            &mut counter,
            &mut statics,
        ));
        init.push_str("    },\n");
        row_inits.push_str(&init);
    }
    let revs_name = format!("openxcvr_{stem}_supported_revs");
    let revs = family
        .revs
        .iter()
        .map(|(major, minor)| format!("{{ .major = {major}, .minor = {minor} }}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&statics);
    out.push('\n');
    out.push_str(&format!(
        "static const openxcvr_rev_t {revs_name}[{count}] = {{ {revs} }};\n\n",
        count = family.revs.len(),
    ));
    out.push_str(&format!(
        "const openxcvr_catalog_row_t openxcvr_{stem}_rows[{count}] = {{\n{row_inits}}};\n\n",
        count = rows.len(),
    ));
    out.push_str(&format!(
        "const openxcvr_family_catalog_t openxcvr_{stem}_catalog = {{\n    \\
/* openxcvr_spec_kind_t OPENXCVR_SPEC_{upper} */\n    .family = {family_value},\n    \\
.supported_revs = {revs_name},\n    .supported_rev_count = {rev_count},\n    \\
.rows = openxcvr_{stem}_rows,\n    .row_count = {row_count},\n}};\n",
        upper = family.spec.replace('-', "_").to_uppercase(),
        family_value = family_value,
        rev_count = family.revs.len(),
        row_count = rows.len(),
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn u64be_field_kind_constant() {
        assert_eq!(super::c_field_kind("U64Be"), "OPENXCVR_FIELD_U64BE");
        assert!(super::type_block().contains("OPENXCVR_FIELD_U64BE"));
    }
}
