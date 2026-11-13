//! Catalog emitter tests on synthetic schemas (no real schema files).
use openxcvr_codegen::emitters::c::catalog::{emit_catalog_c_header, emit_catalog_c_source};
use openxcvr_codegen::emitters::rust::catalog::{emit_catalog, CatalogPageInfo};
use openxcvr_codegen::parser::pages::load_page_schema;
use std::collections::BTreeMap;
use std::path::Path;

fn load(dir: &Path, name: &str, yaml: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    let path = dir.join(name);
    std::fs::write(&path, yaml).unwrap();
    load_page_schema(&path).expect("parse")
}

fn info(entries: &[(&str, bool, Option<u8>)]) -> BTreeMap<String, CatalogPageInfo> {
    entries
        .iter()
        .map(|&(id, banked, page_select)| {
            (
                id.to_string(),
                CatalogPageInfo {
                    banked,
                    page_select,
                    path: None,
                },
            )
        })
        .collect()
}

const LOWER: &str = r#"
schema: openxcvr.page.v1
kind: page
id: SyntheticLower
title: T
page_address: lower
fields:
  - name: temp_mon_value
    display_name: TempMonValue
    offset: 14
    size_bytes: 2
    type: i16be
    encoding: { kind: scaled_signed, lsb_units: "1/256 degC" }
    access: ro
    field_type: Telemetry
    tags: [Monitor, "Monitor::Temperature"]
  - name: temp_mon_high_alarm_flag
    offset: 8
    bit_offset: 7
    bit_width: 1
    type: bool
    access: ro_cor
  - name: mci_max_speed
    offset: 2
    bit_offset: 2
    bit_width: 4
    type: u4
    access: ro
    aliased_to:
      - { rev: "4.0", new_name: twi_max_speed_cmis4 }
      - { rev: "5.0", new_name: mci_max_speed }
  - name: aux_mon_value
    offset: 18
    size_bytes: 2
    type: i16be
    since: "5.0"
"#;

const PAGE_11H: &str = r#"
schema: openxcvr.page.v1
kind: page
id: SyntheticPage11h
title: T
page_address: "11h"
fields:
  - name: optical_power_tx
    offset_expr: "154 + 2 * (lane - 1)"
    size_bytes: 2
    type: u16be
    encoding: { kind: scaled_unsigned, lsb_units: "0.1 uW" }
    access: ro
    instances: { index: lane, start: 1, end: 8 }
  - name: laser_bias_tx
    offset_expr: "170 + 2 * (lane - 1)"
    size_bytes: 2
    type: u16be
    encoding: { kind: scaled_unsigned, lsb_units: "2 uA times TxBiasCurrentScalingFactor (Page 01h:160)" }
    access: ro
    instances: { index: lane, start: 1, end: 8 }
"#;

const A0H: &str = r#"
schema: openxcvr.page.v1
kind: page
id: page_a0h_base_memory
title: T
page_address: A0h
fields:
  - name: externally_calibrated
    offset: 92
    bit_offset: 4
    bit_width: 1
    type: bool
"#;

const A2H: &str = r#"
schema: openxcvr.page.v1
kind: page
id: page_a2h_diagnostics_base
title: T
page_address: A2h
fields:
  - name: external_calibration_linear_term
    instances: { index: term, start: 0, end: 7 }
    offset_expr: "76 + 2 * term"
    size_bytes: 2
    type: u16be
  - name: rx_power_coefficient
    instances: { index: degree, start: 0, end: 4 }
    offset_expr: "72 - 4 * degree"
    size_bytes: 4
    type: u32be
  - name: temperature
    offset: 96
    size_bytes: 2
    type: i16be
    variants:
      discriminator: { field: page_a0h_base_memory.externally_calibrated }
      cases:
        - when: { value: false }
          encoding: { kind: scaled_signed, lsb_units: "1/256 deg C" }
        - when: { value: true }
          encoding: { kind: external_calibration, constants_field: external_calibration_constants, slope_field: "external_calibration_linear_term[4]", offset_field: "external_calibration_linear_term[5]" }
  - name: rx_power
    offset: 104
    size_bytes: 2
    type: u16be
    variants:
      discriminator: { field: page_a0h_base_memory.externally_calibrated }
      cases:
        - when: { value: false }
          encoding: { kind: scaled_unsigned, lsb_units: "0.1 uW" }
        - when: { value: true }
          encoding: { kind: external_calibration, formula: poly4, constants_field: external_calibration_constants, coefficients: ["rx_power_coefficient[0]", "rx_power_coefficient[1]", "rx_power_coefficient[2]", "rx_power_coefficient[3]", "rx_power_coefficient[4]"] }
"#;

/// Minimal lower-memory stand-in carrying the `cmis_revision` row: the
/// CMIS family catalog derives its reported-revision probe from that row,
/// so synthetic CMIS spec sets must include it.
const CMIS_PROBE_PAGE: &str = r#"
schema: openxcvr.page.v1
kind: page
id: CmisLowerMemory
title: T
page_address: lower
fields:
  - name: cmis_revision
    offset: 1
    size_bytes: 1
    type: u8
"#;

#[test]
fn catalog_emits_rows_with_schema_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = vec![
        load(tmp.path(), "lower.yaml", LOWER),
        load(tmp.path(), "page11h.yaml", PAGE_11H),
        load(tmp.path(), "cmis_lower.yaml", CMIS_PROBE_PAGE),
    ];
    let map = info(&[
        ("SyntheticLower", false, None),
        ("SyntheticPage11h", true, None),
    ]);
    let out = emit_catalog("cmis", &schemas, &map).unwrap();
    assert!(out.contains("Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(out.contains("pub static CATALOG: FamilyCatalog"));
    // The reported-revision probe is schema-derived (lower memory byte 1).
    assert!(out.contains("revision_probe: Some(RevisionProbe {"));
    assert!(out.contains("field: \"cmis_revision\""));
    assert!(out.contains("family: Family::Cmis"));
    assert!(out.contains("Rev::CMIS_3_0"));
    // Simple scaled row with location, width, type, access, scaling.
    assert!(out.contains("name: \"temp_mon_value\""));
    assert!(out.contains("offset: 14, len: 2"));
    assert!(out.contains("bit_width: 16"));
    assert!(out.contains("kind: FieldKind::I16Be"));
    assert!(out.contains("access: AccessKind::Ro"));
    assert!(out.contains("Scaling::Scaled(ScaledLsb {"));
    assert!(out.contains("num: 1, den: 256"));
    assert!(out.contains("unit: Unit::Celsius"));
    // Clear-on-read permission is carried as metadata.
    assert!(out.contains("name: \"temp_mon_high_alarm_flag\""));
    assert!(out.contains("access: AccessKind::RoCor"));
    assert!(out.contains("ReadEffect::ClearOnRead"));
}

#[test]
fn catalog_emits_lane_offsets_and_bias_note() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = vec![
        load(tmp.path(), "lower.yaml", LOWER),
        load(tmp.path(), "page11h.yaml", PAGE_11H),
        load(tmp.path(), "cmis_lower.yaml", CMIS_PROBE_PAGE),
    ];
    let map = info(&[
        ("SyntheticLower", false, None),
        ("SyntheticPage11h", true, None),
    ]);
    let out = emit_catalog("cmis", &schemas, &map).unwrap();
    assert!(out.contains("name: \"optical_power_tx\""));
    assert!(out.contains("offsets: &[154, 156, 158, 160, 162, 164, 166, 168]"));
    assert!(out.contains("first: 1, last: 8"));
    assert!(out.contains("index: \"lane\""));
    // The schema's conditional-multiplier caveat is preserved, not dropped.
    assert!(out.contains("note: Some(\"times TxBiasCurrentScalingFactor (Page 01h:160)\")"));
    // Banked page flag from the page schema's banking declaration.
    assert!(out.contains("banked: true"));
}

#[test]
fn catalog_emits_revision_spans_for_renames_and_windows() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = vec![
        load(tmp.path(), "lower.yaml", LOWER),
        load(tmp.path(), "cmis_lower.yaml", CMIS_PROBE_PAGE),
    ];
    let out = emit_catalog("cmis", &schemas, &info(&[])).unwrap();
    // Three spans for the aliased field.
    assert!(out.contains("name: \"twi_max_speed_cmis4\""));
    assert!(out.contains("since: Some(Rev::CMIS_4_0),\nuntil: Some(Rev::CMIS_5_0)"));
    assert!(out.contains("since: Some(Rev::CMIS_5_0),\nuntil: None"));
    assert!(out.contains("since: None,\nuntil: Some(Rev::CMIS_4_0)"));
    // Alias metadata retained.
    assert!(
        out.contains("aliased_to: &[RenameRecord { rev: \"4.0\", name: \"twi_max_speed_cmis4\" }")
    );
    // since: 5.0 field.
    assert!(out.contains("name: \"aux_mon_value\""));
}

#[test]
fn catalog_emits_sff8472_calibration_variants() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = vec![
        load(tmp.path(), "a0h.yaml", A0H),
        load(tmp.path(), "a2h.yaml", A2H),
    ];
    let map = info(&[
        ("page_a0h_base_memory", false, None),
        ("page_a2h_diagnostics_base", false, None),
    ]);
    let out = emit_catalog("sff-8472", &schemas, &map).unwrap();
    // Linear variant with resolved constant locations.
    assert!(out.contains("name: \"temperature\""));
    assert!(out.contains("Scaling::Linear {"));
    assert!(out.contains("offset: 84, len: 2"));
    assert!(out.contains("offset: 86, len: 2"));
    // Polynomial variant with the five coefficient locations.
    assert!(out.contains("name: \"rx_power\""));
    assert!(out.contains("Scaling::Polynomial {"));
    assert!(out.contains("offset: 56, len: 4"));
    assert!(out.contains("offset: 72, len: 4"));
    // Discriminator bit location on A0h.
    assert!(out.contains("VariantDiscriminator {"));
    assert!(out.contains("offset: 92, len: 1"));
    assert!(out.contains("bit: 4"));
    // SFF family and its single schema revision.
    assert!(out.contains("family: Family::Sff8472"));
    assert!(out.contains("major: 12, minor: 5"));
}

#[test]
fn catalog_fails_loud_on_unknown_scaled_unit() {
    let tmp = tempfile::tempdir().unwrap();
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: BadUnits
title: T
page_address: lower
fields:
  - name: weird
    offset: 0
    size_bytes: 2
    type: u16be
    encoding: { kind: scaled_unsigned, lsb_units: "3 apples" }
"#;
    let schemas = vec![load(tmp.path(), "bad.yaml", yaml)];
    let err = emit_catalog("cmis", &schemas, &info(&[])).unwrap_err();
    assert!(err.to_string().contains("apples"), "error was: {err}");
}

#[test]
fn c_catalog_header_and_source_emit_the_table() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = vec![
        load(tmp.path(), "lower.yaml", LOWER),
        load(tmp.path(), "page11h.yaml", PAGE_11H),
    ];
    let map = info(&[
        ("SyntheticLower", false, None),
        ("SyntheticPage11h", true, None),
    ]);
    let header = emit_catalog_c_header("cmis", &schemas, &map).unwrap();
    assert!(header.contains("Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(header.contains("openxcvr_catalog_row_t"));
    assert!(header.contains("openxcvr_family_catalog_t"));
    assert!(header.contains("extern const openxcvr_family_catalog_t openxcvr_cmis_catalog;"));
    let source = emit_catalog_c_source("cmis", &schemas, &map).unwrap();
    assert!(source.contains("#include \"catalog/cmis_catalog.h\""));
    assert!(source.contains("const openxcvr_family_catalog_t openxcvr_cmis_catalog"));
    assert!(source.contains(".name = \"temp_mon_value\""));
    assert!(source.contains("1, 1, 256, OPENXCVR_UNIT_CELSIUS, NULL"));
    // Lane offsets array emitted as data.
    assert!(source.contains("154, 156, 158, 160, 162, 164, 166, 168"));
}

#[test]
fn catalog_projects_effective_metadata_and_coalesces_only_equivalent_rows() {
    use openxcvr_codegen::emitters::rust::catalog::derive_rows;
    let tmp = tempfile::tempdir().unwrap();
    let source = r#"
schema: openxcvr.page.v1
kind: page
id: SyntheticEffects
title: T
page_address: lower
fields:
  - name: changing
    offset: 3
    type: u8
    access: ro
    changed_in:
      - { rev: "4.0", type: u2, bit_width: 2, read_effect: none, read_effect_provenance: "test authority", change_source: dynamic }
      - { rev: "5.0", read_effect: unknown, read_effect_provenance: null }
      - { rev: "5.3", read_effect: none, read_effect_provenance: "test authority" }
  - name: latched
    offset: 8
    type: u8
    access: ro_cor
  - name: custom
    offset: 9
    type: u8
    access: ro
    read_effect: acknowledge_startup_interrupt
"#;
    let schemas = vec![
        load(tmp.path(), "effects.yaml", source),
        load(tmp.path(), "probe.yaml", CMIS_PROBE_PAGE),
    ];
    let rows = derive_rows(&schemas, "cmis", &info(&[])).unwrap();
    let changing: Vec<_> = rows.iter().filter(|r| r.name == "changing").collect();
    assert_eq!(changing.len(), 4, "one row per effective metadata span");
    for (row, since, until, effects, change) in [
        (changing[0], None, Some((4, 0)), vec![], None),
        (
            changing[1],
            Some((4, 0)),
            Some((5, 0)),
            vec!["None"],
            Some("Dynamic"),
        ),
        (
            changing[2],
            Some((5, 0)),
            Some((5, 3)),
            vec!["Unknown"],
            Some("Dynamic"),
        ),
        (
            changing[3],
            Some((5, 3)),
            None,
            vec!["None"],
            Some("Dynamic"),
        ),
    ] {
        assert_eq!((row.since, row.until), (since, until));
        assert_eq!(row.read_effects, effects);
        assert_eq!(row.change_source, change);
    }
    assert_eq!(
        rows.iter()
            .find(|r| r.name == "latched")
            .unwrap()
            .read_effects,
        ["ClearOnRead"]
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.name == "custom")
            .unwrap()
            .read_effects,
        ["AcknowledgeStartupInterrupt"]
    );
    assert_eq!((changing[0].kind, changing[0].bit_width), ("U8", 8));
    for row in &changing[1..] {
        assert_eq!(
            (row.kind, row.bit_width),
            ("U2", 2),
            "partial overrides must inherit type and width too"
        );
    }
    // Both renderers consume the same effective rows (no language-specific projection).
    let rust = emit_catalog("cmis", &schemas, &info(&[])).unwrap();
    let c = emit_catalog_c_source("cmis", &schemas, &info(&[])).unwrap();
    assert_eq!(rust.matches("\nname: \"changing\"").count(), 4);
    assert_eq!(c.matches(".name = \"changing\"").count(), 4);
}

#[test]
fn real_catalog_has_exactly_one_identity_per_applicable_schema_field_and_revision() {
    use openxcvr_codegen::emitters::rust::catalog::derive_rows;
    use openxcvr_codegen::ir::version::{applicable_revs, resolve_field_for_rev, Rev};
    use std::collections::BTreeSet;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis");
    let schemas: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .filter(|p| {
            let doc: serde_norway::Value =
                serde_norway::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
            doc.get("kind").and_then(|k| k.as_str()) == Some("page")
        })
        .map(|p| load_page_schema(&p).expect("real page schema must parse"))
        .collect();
    assert!(!schemas.is_empty());
    let rows = derive_rows(&schemas, "cmis", &info(&[])).unwrap();
    for rev in Rev::CMIS_ALL {
        let pair = {
            let (a, b) = rev.as_str().split_once('.').unwrap();
            (a.parse::<u8>().unwrap(), b.parse::<u8>().unwrap())
        };
        let applicable_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.since.is_none_or(|s| s <= pair) && r.until.is_none_or(|u| pair < u))
            .collect();
        let actual: BTreeSet<_> = applicable_rows
            .iter()
            .map(|r| (r.page_id.clone(), r.name.clone()))
            .collect();
        assert_eq!(
            actual.len(),
            applicable_rows.len(),
            "duplicate identities at {rev:?}"
        );
        let expected: BTreeSet<_> = schemas
            .iter()
            .flat_map(|p| {
                p.fields
                    .iter()
                    .filter(|f| applicable_revs(f, Rev::CMIS_ALL).contains(rev))
                    .map(|f| (p.id.clone(), resolve_field_for_rev(f, *rev).name))
            })
            .collect();
        assert_eq!(actual, expected, "catalog identities at {rev:?}");
    }
}
