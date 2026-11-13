//! Presence-metadata generation tests (Stage 1 B1/B2).
//!
//! - Inventory: derives the field-level `present_when` conditions from the
//!   parsed schemas of all three cataloged specs (raw YAML walk, independent
//!   of the classifier) and asserts set equality with the presence metadata
//!   the catalog emitter derives — identities and classifications compared,
//!   never counts.
//! - Known resolutions: the SFF-8636 enum_inline-indexed prerequisite
//!   (rx_squelch_disable → page 00h byte 194 bit 3), the SFF-8472
//!   two-byte `format_id` enum prerequisite, the qualified SFF-8472
//!   capability prerequisite, and the CMIS Page 14h selector with
//!   content-switch metadata.
//! - Negative: one synthetic-schema test per generation-error class
//!   (explicit null, malformed form, bad literal, missing, ambiguous,
//!   cyclic, revision-inapplicable) and the Unsupported classification for
//!   well-formed syntax outside the supported subset.
//!
//! Synthetic fixtures below are labelled synthetic in each test's comment.
use openxcvr_codegen::emitters::rust::catalog::{derive_rows, page_catalog_info, DerivedRow};
use openxcvr_codegen::parser::index::IndexLoader;
use openxcvr_codegen::parser::pages::load_page_schema;
use openxcvr_codegen::presence::{
    EmittedContentSwitch, EmittedEquals, EmittedLoc, EmittedPresence, EmittedSwitchBound, Settling,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn spec_rows(spec: &str) -> Vec<DerivedRow> {
    let schemas = repo_root().join("schemas");
    let paths = IndexLoader::collect_page_schemas(&schemas, Some(spec)).expect("collect pages");
    let loaded: Vec<_> = paths
        .iter()
        .map(|p| load_page_schema(p).expect("load page"))
        .collect();
    let info = page_catalog_info(&paths).expect("page info");
    derive_rows(&loaded, spec, &info).expect("derive rows")
}

/// Field-level `present_when` conditions straight from the raw schema YAML:
/// (page id, field name) → raw condition value. Independent of the crate's
/// PageField/classifier machinery.
fn field_level_conditions(spec: &str) -> BTreeMap<(String, String), serde_norway::Value> {
    let schemas = repo_root().join("schemas");
    let paths = IndexLoader::collect_page_schemas(&schemas, Some(spec)).expect("collect pages");
    let mut conditions = BTreeMap::new();
    for path in &paths {
        let text = std::fs::read_to_string(path).unwrap();
        let doc: serde_norway::Value = serde_norway::from_str(&text).unwrap();
        let page_id = doc.get("id").and_then(|v| v.as_str()).unwrap().to_string();
        let mut walk = |fields: Option<&Vec<serde_norway::Value>>| {
            for field in fields.into_iter().flatten() {
                let Some(name) = field.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                if let Some(condition) = field.get("present_when") {
                    conditions.insert((page_id.clone(), name.to_string()), condition.clone());
                }
            }
        };
        walk(doc.get("fields").and_then(|v| v.as_sequence()));
        for include in doc
            .get("field_includes")
            .and_then(|v| v.as_sequence())
            .into_iter()
            .flatten()
        {
            let include: String = serde_norway::from_value(include.clone()).unwrap();
            let frag = std::fs::read_to_string(path.parent().unwrap().join(&include)).unwrap();
            let frag: serde_norway::Value = serde_norway::from_str(&frag).unwrap();
            walk(frag.get("fields").and_then(|v| v.as_sequence()));
        }
    }
    conditions
}

/// How the raw condition value reads, coarsely, for set-equality purposes:
/// the supported equality form versus anything else.
#[derive(Debug)]
enum ConditionForm {
    Equals { field: String },
    Other,
}

fn condition_form(value: &serde_norway::Value) -> ConditionForm {
    match value {
        serde_norway::Value::Mapping(map) => {
            let field = map.get("field").and_then(|v| v.as_str());
            let equals = map.get("equals");
            match (field, equals) {
                (Some(field), Some(_)) => ConditionForm::Equals {
                    field: field.to_string(),
                },
                _ => ConditionForm::Other,
            }
        }
        serde_norway::Value::String(text) => match text.split_once("==") {
            Some((path, _literal)) => ConditionForm::Equals {
                field: path.trim().to_string(),
            },
            None => ConditionForm::Other,
        },
        _ => ConditionForm::Other,
    }
}

fn equals_of(presence: &EmittedPresence) -> &EmittedEquals {
    match presence {
        EmittedPresence::Equals(equals) => equals,
        other => panic!("expected Equals, got {other:?}"),
    }
}

// ─── Inventory: set equality between schema conditions and emitted metadata ──

#[test]
fn emitted_presence_is_set_equal_to_the_schema_conditions() {
    for spec in ["cmis", "sff-8636", "sff-8472"] {
        let conditions = field_level_conditions(spec);
        let rows = spec_rows(spec);
        // Every revision-span row of one field carries the same presence.
        let mut emitted: BTreeMap<(String, String), &EmittedPresence> = BTreeMap::new();
        for row in &rows {
            let key = (row.page_id.clone(), row.name.clone());
            match emitted.get(&key) {
                Some(previous) => assert_eq!(
                    *previous, &row.presence,
                    "{spec}: {key:?} has inconsistent presence across spans"
                ),
                None => {
                    emitted.insert(key, &row.presence);
                }
            }
        }
        // Set equality on identities: exactly the conditioned fields carry
        // non-Unconditional presence.
        let conditioned: Vec<&(String, String)> = conditions.keys().collect();
        let non_unconditional: Vec<&(String, String)> = emitted
            .iter()
            .filter(|(_, presence)| !matches!(presence, EmittedPresence::Unconditional))
            .map(|(key, _)| key)
            .collect();
        assert_eq!(
            conditioned, non_unconditional,
            "{spec}: conditions and non-unconditional rows differ"
        );
        // Classification equality per identity (map/string equality forms
        // both classify as Equals with the written field path).
        for ((page, field), raw) in &conditions {
            let presence = emitted[&(page.clone(), field.clone())];
            match (condition_form(raw), presence) {
                (ConditionForm::Equals { field: path, .. }, EmittedPresence::Equals(equals)) => {
                    assert_eq!(
                        equals.field, *path,
                        "{spec}: {page}::{field} prerequisite path"
                    );
                }
                (ConditionForm::Other, EmittedPresence::Unsupported { expr }) => {
                    // The raw condition is retained verbatim.
                    let raw_text = match raw {
                        serde_norway::Value::String(s) => s.clone(),
                        other => format!("{other:?}"),
                    };
                    assert_eq!(expr, &raw_text, "{spec}: {page}::{field}");
                }
                (form, presence) => {
                    panic!("{spec}: {page}::{field}: form {form:?} vs presence {presence:?}")
                }
            }
        }
    }
}

// ─── Known resolutions (real schemas, transcribed from the standards) ─────

#[test]
fn cmis_page14h_selector_prerequisite_resolves_with_content_switch() {
    let rows = spec_rows("cmis");
    let row = rows
        .iter()
        .find(|r| r.name == "media_side_ber" && r.page_id == "CmisPage14hDiagnosticsResults")
        .expect("media_side_ber row");
    let equals = equals_of(&row.presence);
    assert_eq!(equals.field, "diagnostics_selector");
    assert_eq!(
        equals.loc,
        EmittedLoc {
            address: 0x50,
            page: 0x14,
            offset: 128,
            len: 1
        }
    );
    assert!(equals.banked, "page 14h is banked");
    assert_eq!(equals.kind, "EnumRef");
    assert_eq!(equals.bit_offset, 0);
    assert_eq!(equals.bit_width, 8);
    assert_eq!(equals.expected, 0x01);
    let switch = equals.content_switch.as_ref().expect("content_switch");
    assert_eq!(
        switch.data,
        EmittedLoc {
            address: 0x50,
            page: 0x14,
            offset: 192,
            len: 64
        }
    );
    assert_eq!(
        switch.bound,
        vec![
            EmittedSwitchBound {
                since: Some((4, 0)),
                settling: Settling::TnackAdvertised
            },
            EmittedSwitchBound {
                since: Some((5, 0)),
                settling: Settling::Tddcs
            },
        ]
    );
    // The floor: every Page 14h field exists from 4.0.
    let page14h: Vec<&DerivedRow> = rows
        .iter()
        .filter(|r| r.page_id == "CmisPage14hDiagnosticsResults")
        .collect();
    assert!(!page14h.is_empty());
    for row in &page14h {
        assert_eq!(row.since, Some((4, 0)), "{} must be since 4.0", row.name);
    }
}

#[test]
fn sff8636_enum_inline_prerequisite_resolves_to_option4_byte194_bit3() {
    let rows = spec_rows("sff-8636");
    let row = rows
        .iter()
        .find(|r| r.name == "rx_squelch_disable_lane")
        .expect("rx_squelch_disable_lane row");
    let equals = equals_of(&row.presence);
    assert_eq!(
        equals.field,
        "page_00h_upper_administrative.option_control_or_flag_support[rx_squelch_disable]"
    );
    assert_eq!(
        equals.loc,
        EmittedLoc {
            address: 0x50,
            page: 0x00,
            offset: 194,
            len: 1
        }
    );
    assert!(!equals.banked);
    assert_eq!(equals.kind, "Bool");
    assert_eq!(equals.bit_offset, 3);
    assert_eq!(equals.bit_width, 1);
    assert_eq!(equals.expected, 1);
    assert!(equals.content_switch.is_none());
}

#[test]
fn sff8472_qualified_capability_prerequisite_resolves() {
    let rows = spec_rows("sff-8472");
    let row = rows
        .iter()
        .find(|r| r.name == "optional_laser_temperature")
        .expect("optional_laser_temperature row");
    let equals = equals_of(&row.presence);
    assert_eq!(equals.field, "page_a0h_base_memory.externally_calibrated");
    assert_eq!(
        equals.loc,
        EmittedLoc {
            address: 0x50,
            page: 0x00,
            offset: 92,
            len: 1
        }
    );
    assert_eq!(equals.kind, "Bool");
    assert_eq!(equals.bit_offset, 4);
    assert_eq!(equals.bit_width, 1);
    assert_eq!(equals.expected, 0);
    assert!(equals.content_switch.is_none());
}

#[test]
fn sff8472_format_id_prerequisite_resolves_as_two_byte_big_endian() {
    let rows = spec_rows("sff-8472");
    let row = rows
        .iter()
        .find(|r| r.name == "optical_number_of_lanes")
        .expect("optical_number_of_lanes row");
    let equals = equals_of(&row.presence);
    assert_eq!(equals.field, "format_id");
    assert_eq!(
        equals.loc,
        EmittedLoc {
            address: 0x51,
            page: 0x03,
            offset: 128,
            len: 2
        }
    );
    assert_eq!(equals.kind, "EnumRef");
    assert_eq!(equals.bit_offset, 0);
    assert_eq!(equals.bit_width, 16);
    assert_eq!(equals.expected, 0xca1b);
}

#[test]
fn cmis_same_page_string_form_resolves() {
    let rows = spec_rows("cmis");
    let row = rows
        .iter()
        .find(|r| r.name == "power_saving_mode")
        .expect("power_saving_mode row");
    let equals = equals_of(&row.presence);
    assert_eq!(equals.field, "power_saving_support");
    assert_eq!(
        equals.loc,
        EmittedLoc {
            address: 0x50,
            page: 0x2f,
            offset: 128,
            len: 1
        }
    );
    assert_eq!(equals.kind, "Bool");
    assert_eq!(equals.bit_offset, 2);
    assert_eq!(equals.bit_width, 1);
    assert_eq!(equals.expected, 1);
}

#[test]
fn unconditional_rows_stay_unconditional() {
    let rows = spec_rows("cmis");
    let row = rows
        .iter()
        .find(|r| r.name == "temp_mon_value")
        .expect("temp_mon_value row");
    assert_eq!(row.presence, EmittedPresence::Unconditional);
}

// ─── Negative: generation errors and Unsupported (synthetic schemas) ──────

/// Writes a synthetic CMIS spec dir and derives its rows. Synthetic
/// fixture: none of these schemas exist in the repo.
fn synthetic_rows(page_yaml: &str) -> Result<Vec<DerivedRow>, String> {
    synthetic_rows_multi(&[("page.yaml", page_yaml)])
}

/// Multi-page variant of [`synthetic_rows`]: writes one file per
/// (filename, page YAML) pair, so a test can make one page's schema id
/// collide with another page's snake-cased file stem. Synthetic fixture:
/// none of these schemas exist in the repo.
fn synthetic_rows_multi(pages: &[(&str, &str)]) -> Result<Vec<DerivedRow>, String> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("fields")).unwrap();
    let mut paths = Vec::new();
    for (name, page_yaml) in pages {
        let page_path = dir.path().join(name);
        std::fs::write(&page_path, page_yaml).unwrap();
        paths.push(page_path);
    }
    let loaded: Vec<_> = paths
        .iter()
        .map(|p| load_page_schema(p).map_err(|e| format!("{e:#}")))
        .collect::<Result<_, _>>()?;
    let info = page_catalog_info(&paths).unwrap();
    derive_rows(&loaded, "cmis", &info)
}

const SYNTH_PAGE_HEAD: &str = r#"
schema: openxcvr.page.v1
kind: page
id: SynthPage
title: T
page_address: "01h"
fields:
"#;

fn synth_field(name: &str, present_when: &str) -> String {
    format!(
        r#"  - name: {name}
    offset: 200
    size_bytes: 2
    type: u16be
    present_when: {present_when}
"#
    )
}

fn synth_selector_field() -> String {
    r#"  - name: selector
    offset: 128
    size_bytes: 1
    type: enum_ref
    enum_ref: SynthSelector
    access: rw
"#
    .to_string()
}

#[test]
fn explicit_null_present_when_is_a_generation_error() {
    // Synthetic: explicit YAML null is malformed, not "unconditional".
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "null")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("gated") && err.contains("null"),
        "error must name field and form: {err}"
    );
}

#[test]
fn non_map_non_string_present_when_is_a_generation_error() {
    // Synthetic: a bare number is neither form.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "42")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(err.contains("gated"), "error must name field: {err}");
}

#[test]
fn bad_literal_is_a_generation_error() {
    // Synthetic: `== banana` has no valid literal.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "\"selector == banana\"")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("gated") && err.contains("banana"),
        "error must name field and literal: {err}"
    );
}

#[test]
fn missing_reference_is_a_generation_error() {
    // Synthetic: the referenced field does not exist.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "\"no_such_field == 1\"")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("gated") && err.contains("no_such_field"),
        "error must name field and reference: {err}"
    );
    assert!(
        err.contains("SynthPage") || err.contains("page.yaml"),
        "error must name the schema: {err}"
    );
}

#[test]
fn ambiguous_enum_inline_reference_is_a_generation_error() {
    // Synthetic: two enum_inline entries share the referenced name.
    let page = format!(
        "{}  - name: options\n    offset: 130\n    instances: {{ index: option, start: 0, end: 3 }}\n    offset_expr: \"130 + option / 8\"\n    bit_offset_expr: \"7 - (option % 8)\"\n    bit_width: 1\n    type: bool\n    enum_inline:\n      - {{ value: 0, name: dup_name, description: a }}\n      - {{ value: 1, name: dup_name, description: b }}\n{}",
        SYNTH_PAGE_HEAD,
        synth_field("gated", "\"options[dup_name] == true\"")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("gated") && err.contains("dup_name"),
        "error must name field and reference: {err}"
    );
}

#[test]
fn ambiguous_page_qualifier_is_a_generation_error() {
    // Synthetic: the qualifier "page" is answered by two pages — the page
    // whose schema id IS "page", and the page whose file stem is page.yaml
    // (snake-cased "page"). The first-match pick must be a generation error.
    let dependent = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "\"page.selector == 1\"")
    );
    let named_by_id = format!(
        "{}{}",
        r#"schema: openxcvr.page.v1
kind: page
id: page
title: T
page_address: "02h"
fields:
"#,
        synth_selector_field()
    );
    let err = synthetic_rows_multi(&[("page.yaml", &dependent), ("other.yaml", &named_by_id)])
        .expect_err("must fail");
    assert!(
        err.contains("gated") && err.contains("ambiguous") && err.contains("qualifier \"page\""),
        "error must name field, qualifier and the ambiguity: {err}"
    );
    assert!(
        err.contains("SynthPage") && err.contains("matches pages"),
        "error must name the matching page ids: {err}"
    );
}

#[test]
fn cyclic_reference_is_a_generation_error() {
    // Synthetic: a's presence references b; b's presence references a.
    let page = format!(
        "{}{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("a", "\"b == 1\""),
        synth_field("b", "\"a == 1\"")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("cycle") || err.contains("cyclic"),
        "error must name the cycle: {err}"
    );
}

#[test]
fn revision_inapplicable_reference_is_a_generation_error() {
    // Synthetic (page.v2): the dependent applies from 4.0 on; the
    // prerequisite ends at 5.0 — the reference is inapplicable at 5.0+.
    let page = r#"
schema: openxcvr.page.v2
kind: page
id: SynthPage
title: T
page_address: "01h"
fields:
  - name: selector
    offset: 128
    size_bytes: 1
    type: enum_ref
    enum_ref: SynthSelector
    until: "5.0"
  - name: gated
    offset: 200
    size_bytes: 2
    type: u16be
    since: "4.0"
    present_when: "selector == 1"
"#;
    let err = synthetic_rows(page).expect_err("must fail");
    assert!(
        err.contains("gated") && err.contains("selector"),
        "error must name field and reference: {err}"
    );
    assert!(
        err.contains("5.0") || err.contains("revision"),
        "error must name the revision: {err}"
    );
}

#[test]
fn other_operators_classify_as_unsupported() {
    // Synthetic: `>=` is well-formed syntax outside the supported subset —
    // emitted as Unsupported, never Unconditional, never a generation error.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "\"selector >= 2\"")
    );
    let rows = synthetic_rows(&page).expect("must derive");
    let row = rows.iter().find(|r| r.name == "gated").unwrap();
    assert_eq!(
        row.presence,
        EmittedPresence::Unsupported {
            expr: "selector >= 2".to_string()
        }
    );
}

#[test]
fn map_with_operator_key_classifies_as_unsupported() {
    // Synthetic: a map form with an operator key beyond {field, equals}.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "{ field: selector, not_equals: 1 }")
    );
    let rows = synthetic_rows(&page).expect("must derive");
    let row = rows.iter().find(|r| r.name == "gated").unwrap();
    assert!(matches!(row.presence, EmittedPresence::Unsupported { .. }));
}

#[test]
fn numeric_instance_index_classifies_as_unsupported() {
    // Synthetic: `field[3]` is a guessed position, outside the supported
    // resolution subset (enum_inline names only).
    let page = format!(
        "{}  - name: options\n    offset: 130\n    instances: {{ index: option, start: 0, end: 3 }}\n    offset_expr: \"130 + option / 8\"\n    bit_offset_expr: \"7 - (option % 8)\"\n    bit_width: 1\n    type: bool\n{}",
        SYNTH_PAGE_HEAD,
        synth_field("gated", "\"options[3] == true\"")
    );
    let rows = synthetic_rows(&page).expect("must derive");
    let row = rows.iter().find(|r| r.name == "gated").unwrap();
    assert!(matches!(row.presence, EmittedPresence::Unsupported { .. }));
}

#[test]
fn map_without_field_key_is_a_generation_error() {
    // Synthetic: a mapping that is not a field condition at all.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "{ banana: 1 }")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(err.contains("gated"), "error must name field: {err}");
}

#[test]
fn expected_value_wider_than_the_prerequisite_is_a_generation_error() {
    // Synthetic: 0x1ff cannot fit an 8-bit selector.
    let page = format!(
        "{}{}{}",
        SYNTH_PAGE_HEAD,
        synth_selector_field(),
        synth_field("gated", "\"selector == 0x1ff\"")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(err.contains("gated"), "error must name field: {err}");
}

// ─── content_switch validation (synthetic) ────────────────────────────────

#[test]
fn content_switch_with_unknown_settling_is_a_generation_error() {
    // Synthetic: settling must be tnack_advertised or tddcs.
    let page = format!(
        "{}  - name: selector\n    offset: 128\n    size_bytes: 1\n    type: enum_ref\n    enum_ref: SynthSelector\n    content_switch:\n      data_range: [192, 255]\n      bound:\n        - {{ since: \"4.0\", settling: whatever }}\n{}",
        SYNTH_PAGE_HEAD,
        synth_field("gated", "{ field: selector, equals: 0x01 }")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("selector") || err.contains("settling"),
        "error: {err}"
    );
}

#[test]
fn content_switch_with_malformed_data_range_is_a_generation_error() {
    // Synthetic: data_range must be [start, end] with start <= end.
    let page = format!(
        "{}  - name: selector\n    offset: 128\n    size_bytes: 1\n    type: enum_ref\n    enum_ref: SynthSelector\n    content_switch:\n      data_range: [255, 192]\n      bound:\n        - {{ since: \"3.0\", settling: tddcs }}\n{}",
        SYNTH_PAGE_HEAD,
        synth_field("gated", "{ field: selector, equals: 0x01 }")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("data_range") || err.contains("selector"),
        "error: {err}"
    );
}

#[test]
fn content_switch_bound_must_cover_the_dependent_revisions() {
    // Synthetic: the dependent applies from 3.0 (v1 page), but the first
    // bound starts at 4.0 — the row needs a settling bound at every
    // revision it can be observed at.
    let page = format!(
        "{}  - name: selector\n    offset: 128\n    size_bytes: 1\n    type: enum_ref\n    enum_ref: SynthSelector\n    content_switch:\n      data_range: [192, 255]\n      bound:\n        - {{ since: \"4.0\", settling: tddcs }}\n{}",
        SYNTH_PAGE_HEAD,
        synth_field("gated", "{ field: selector, equals: 0x01 }")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("bound") || err.contains("selector"),
        "error: {err}"
    );
}

// The content-switch model is public for consumers of the emitted metadata.
#[test]
fn emitted_content_switch_is_public() {
    let _: Option<EmittedContentSwitch> = None;
}
