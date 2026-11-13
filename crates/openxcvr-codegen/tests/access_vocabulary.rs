use openxcvr_codegen::emitters::rust::per_rev::emit_page_per_rev;
use openxcvr_codegen::ir::pages::FieldType;
use openxcvr_codegen::ir::version::{resolve_field_for_rev, Rev};
use openxcvr_codegen::parser::pages::parse_page_schema;

fn page(fields: &str) -> String {
    format!("schema: openxcvr.page.v2\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n{fields}")
}

#[test]
fn chained_partial_overrides_inherit_earlier_type() {
    let schema = parse_page_schema(&page(
        r#"  - name: control
    offset: 0
    type: u8
    changed_in:
      - { rev: '4.0', type: u16be }
      - { rev: '5.0', note: 'permission unchanged' }
"#,
    ))
    .unwrap();
    let resolved = resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
    assert_eq!(
        resolved.ty,
        FieldType::U16Be,
        "partial override lost earlier applicable type"
    );
}

#[test]
fn restrictive_revision_cannot_use_common_ever_writable_flag() {
    let schema = parse_page_schema(&page(
        r#"  - name: control
    offset: 0
    type: u8
    access: rw
    flags: { is_ever_writable: true }
    changed_in:
      - { rev: '4.0', access: ro }
      - { rev: '5.0', note: 'still restricted' }
"#,
    ))
    .unwrap();
    assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
    assert!(
        !emit_page_per_rev(&schema, Rev::V5_0).contains("set_control"),
        "resolved RO restriction must suppress the setter"
    );
}

#[test]
fn explicit_none_conflicting_with_clear_on_read_is_rejected() {
    let result = parse_page_schema(&page(
        "  - { name: flag, offset: 0, type: u8, access: ro_cor, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n",
    ));
    assert!(
        result.is_err(),
        "explicit None cannot erase restrictive access"
    );
}

use openxcvr_codegen::ir::pages::{
    Access, ChangeSourceMetadata, ReadEffectMetadata, ReadEffectScope,
};
use openxcvr_codegen::ir::version::applicable_revs;
use openxcvr_codegen::parser::pages::load_page_schema;

#[test]
fn canonical_and_transitional_access_are_typed_without_new_wo_setters() {
    for (spelling, access) in [
        ("ro", Access::Ro),
        ("rw", Access::Rw),
        ("rww", Access::Rww),
        ("wo", Access::Wo),
        ("wo_sc", Access::WoSc),
        ("ro_cor", Access::RoCor),
        ("unknown", Access::Unknown),
        ("cor", Access::RoCor),
        ("rw_nv", Access::Rw),
        ("rw_triggered", Access::Rw),
        ("rw_v", Access::Rw),
        ("rw_with_state_guard", Access::Rw),
        ("implementation_defined", Access::Unknown),
    ] {
        let schema = parse_page_schema(&page(&format!(
            "  - {{ name: f, offset: 0, type: u8, access: {spelling} }}\n"
        )))
        .unwrap();
        assert_eq!(schema.fields[0].access, Some(access));
        assert!(schema.fields[0].explicit_access.is_some());
        // This legacy label was not read by any emitter; don't enable its setter
        // merely because it now maps to canonical RW.
        let expected = matches!(spelling, "rw" | "rww" | "rw_nv" | "rw_triggered" | "rw_v");
        assert_eq!(
            emit_page_per_rev(&schema, Rev::V3_0).contains("set_f"),
            expected,
            "{spelling}"
        );
    }
}

#[test]
fn legacy_latching_is_preserved_as_access_and_effect() {
    for (latching, access, effect) in [
        (
            "clear_on_read",
            Access::RoCor,
            ReadEffectMetadata::ClearOnRead,
        ),
        (
            "clear_on_read_when_condition_clears",
            Access::RoCor,
            ReadEffectMetadata::ClearOnRead,
        ),
        (
            "implementation_defined",
            Access::Unknown,
            ReadEffectMetadata::Unknown,
        ),
    ] {
        let schema = parse_page_schema(&page(&format!("  - {{ name: f, offset: 0, type: u8, access: ro, behavior: {{ latching: {latching} }} }}\n"))).unwrap();
        assert_eq!(schema.fields[0].access, Some(access));
        assert_eq!(schema.fields[0].read_effects(), vec![effect]);
    }
}

#[test]
fn absent_is_not_false_or_explicit_none() {
    let schema = parse_page_schema(&page(r#"  - { name: absent, offset: 0, type: u8, flags: {} }
  - { name: explicit, offset: 1, type: u8, access: ro, flags: { is_dynamic: false }, read_effect: none, read_effect_provenance: 'synthetic test authority' }
"#)).unwrap();
    let absent = &schema.fields[0];
    let explicit = &schema.fields[1];
    assert_eq!(absent.access, None);
    assert_eq!(absent.flags.as_ref().unwrap().is_dynamic, None);
    assert_eq!(absent.metadata.read_effect, None);
    assert_eq!(absent.change_source(), None);
    assert_eq!(absent.read_effect_masks().unwrap()[0].unknown, 0xff);
    assert_eq!(explicit.flags.as_ref().unwrap().is_dynamic, Some(false));
    assert_eq!(explicit.change_source(), Some(ChangeSourceMetadata::Static));
    assert_eq!(
        explicit.metadata.read_effect,
        Some(ReadEffectMetadata::None)
    );
    assert_eq!(explicit.read_effect_masks().unwrap()[0].unknown, 0);
}

#[test]
fn full_byte_readable_none_with_byte_scope_remains_valid() {
    let schema = parse_page_schema(&page(
        "  - { name: safe, offset: 2, type: u8, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority', read_effect_scope: byte }\n",
    ))
    .unwrap();
    let field = &schema.fields[0];
    assert_eq!(field.readable(), Some(true));
    assert_eq!(
        field.metadata.read_effect_scope,
        Some(ReadEffectScope::Byte)
    );
    let masks = field.read_effect_masks().unwrap();
    assert_eq!(masks.len(), 1);
    let mask = &masks[0];
    assert_eq!(mask.offset, 2);
    assert_eq!(mask.covered, 0xff);
    assert_eq!(mask.known, vec![(ReadEffectMetadata::None, 0xff)]);
    assert_eq!((mask.unknown, mask.uncovered, mask.write_only), (0, 0, 0));
}

#[test]
fn partial_none_byte_scope_keeps_neighbor_restrictions() {
    for (neighbor, unknown, write_only) in [
        ("", 0, 0),
        (
            "  - { name: neighbor, offset: 2, bit_offset: 1, type: bool }\n",
            2,
            0,
        ),
        (
            "  - { name: neighbor, offset: 2, bit_offset: 1, type: bool, access: unknown }\n",
            2,
            0,
        ),
        (
            "  - { name: neighbor, offset: 2, bit_offset: 1, type: bool, access: wo }\n",
            0,
            2,
        ),
    ] {
        let schema = parse_page_schema(&page(&format!(
            "  - {{ name: safe, offset: 2, bit_offset: 0, type: bool, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority', read_effect_scope: byte }}\n{neighbor}"
        )))
        .unwrap();
        let masks = schema.fields[0].read_effect_masks().unwrap();
        assert_eq!(masks.len(), 1);
        let mask = &masks[0];
        assert_eq!(mask.offset, 2);
        assert_eq!(mask.covered, 1);
        assert_eq!(
            mask.known,
            vec![(ReadEffectMetadata::None, 1)],
            "partial None must not assert side-effect-free neighboring bits"
        );
        assert_eq!(
            (mask.unknown, mask.uncovered, mask.write_only),
            (0, 0xfe, 0)
        );
        if !neighbor.is_empty() {
            let masks = schema.fields[1].read_effect_masks().unwrap();
            let mask = &masks[0];
            assert_eq!(mask.covered, 2);
            assert!(mask.known.is_empty());
            assert_eq!(
                (mask.unknown, mask.uncovered, mask.write_only),
                (unknown, 0xfd, write_only)
            );
        }
    }
}

#[test]
fn side_effectful_partial_fields_keep_byte_physical_footprints() {
    for (spelling, effect) in [
        ("clear_on_read", ReadEffectMetadata::ClearOnRead),
        (
            "acknowledge_startup_interrupt",
            ReadEffectMetadata::AcknowledgeStartupInterrupt,
        ),
    ] {
        for (scope, physical) in [("field", 4), ("byte", 0xff)] {
            let schema = parse_page_schema(&page(&format!(
                "  - {{ name: flag, offset: 2, bit_offset: 2, type: bool, access: ro, read_effect: {spelling}, read_effect_scope: {scope} }}\n"
            )))
            .unwrap();
            let masks = schema.fields[0].read_effect_masks().unwrap();
            let mask = &masks[0];
            assert_eq!(mask.covered, 4);
            assert_eq!(mask.known, vec![(effect, physical)]);
            assert_eq!(
                (mask.unknown, mask.uncovered, mask.write_only),
                (0, 0xfb, 0)
            );
        }
        let conflict = parse_page_schema(&page(&format!(
            "  - {{ name: flag, offset: 2, bit_offset: 2, type: bool, access: ro, read_effect: {spelling}, read_effect_scope: byte }}\n  - {{ name: safe, offset: 2, bit_offset: 0, type: bool, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority', read_effect_scope: byte }}\n"
        )));
        assert!(conflict
            .unwrap_err()
            .to_string()
            .contains("contradictory read-effect"));
    }
}

#[test]
fn byte_acknowledgment_does_not_cover_unknown_or_uncovered_constituents() {
    let schema = parse_page_schema(&page(
        r#"  - name: ready
    offset: 2
    bit_offset: 0
    type: bool
    access: ro
    read_effect: acknowledge_startup_interrupt
    read_effect_scope: byte
    read_effect_provenance: { source: primary, clause: startup }
  - { name: reserved, offset: 2, bit_offset: 3, bit_width: 5, type: u5, access: unknown }
  - { name: write_only, offset: 2, bit_offset: 1, type: bool, access: wo }
  - { name: control, offset: 2, bit_offset: 2, type: bool, access: rw }
"#,
    ))
    .unwrap();
    let ack = &schema.fields[0];
    assert_eq!(ack.metadata.read_effect_scope, Some(ReadEffectScope::Byte));
    assert_eq!(
        ack.metadata.read_effect_provenance.as_ref().unwrap()["clause"].as_str(),
        Some("startup")
    );
    let mask = &ack.read_effect_masks().unwrap()[0];
    assert_eq!(mask.covered, 1);
    assert_eq!(mask.uncovered, 0xfe);
    assert_eq!(
        mask.known,
        vec![(ReadEffectMetadata::AcknowledgeStartupInterrupt, 0xff)]
    );
    assert_eq!(
        schema.fields[1].read_effect_masks().unwrap()[0].unknown,
        0xf8
    );
    assert_eq!(
        schema.fields[2].read_effect_masks().unwrap()[0].write_only,
        2
    );
    assert_eq!(schema.fields[2].readable(), Some(false));
    assert_eq!(schema.fields[3].writable(), Some(true));
    // Deleting the reserved marking leaves ack's uncovered mask restrictive.
    assert_ne!(mask.uncovered, 0);
}

#[test]
fn byte_effect_and_explicit_none_on_other_bits_conflict() {
    let result = parse_page_schema(&page(
        r#"  - { name: ack, offset: 2, type: bool, access: ro, read_effect: acknowledge_startup_interrupt, read_effect_scope: byte }
  - { name: reserved, offset: 2, bit_offset: 3, type: u5, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority' }
"#,
    ));
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("contradictory read-effect"));
    for access in ["unknown", "wo", "wo_sc", "cor"] {
        assert!(parse_page_schema(&page(&format!(
            "  - {{ name: f, offset: 0, type: u8, access: {access}, read_effect: none, read_effect_provenance: 'synthetic test authority' }}\n"
        )))
        .is_err());
    }
}

#[test]
fn chained_revision_metadata_and_authority_do_not_leak_backwards() {
    let schema = parse_page_schema(&page(
        r#"  - name: control
    offset: 0
    type: u8
    access: rw
    since: '3.0'
    until: '5.3'
    flags: { is_ever_writable: true, is_dynamic: false }
    changed_in:
      - rev: '5.2'
        access: rw
        flags: { is_dynamic: true, is_writable: true }
        read_effect: none
        read_effect_provenance: 'synthetic test authority'
        change_source: dynamic
        authority: { revision: '5.2', source: newer }
      - rev: '4.0'
        access: ro
        read_effect: clear_on_read
        read_effect_scope: field
        read_effect_provenance: { source: primary, revision: '4.0', clause: restricted }
        provenance: { table: restricted-table }
        authority: { revision: '4.0', source: older }
      - rev: '5.0'
        note: partial
      - rev: '5.1'
        authority: null
"#,
    ))
    .unwrap();
    let base = &schema.fields[0];
    assert!(!applicable_revs(base, Rev::CMIS_ALL).contains(&Rev::V5_3));
    let early = resolve_field_for_rev(base, Rev::V3_0);
    assert_eq!(early.access, Some(Access::Rw));
    assert_eq!(early.metadata.authority, None);
    assert_eq!(early.metadata.read_effect, None);
    let restricted = resolve_field_for_rev(base, Rev::V5_0);
    assert_eq!(restricted.access, Some(Access::Ro));
    assert_eq!(restricted.writable(), Some(false));
    assert_eq!(
        restricted.metadata.read_effect,
        Some(ReadEffectMetadata::ClearOnRead)
    );
    assert_eq!(
        restricted.metadata.read_effect_scope,
        Some(ReadEffectScope::Field)
    );
    assert_eq!(
        restricted.metadata.authority.as_ref().unwrap()["revision"].as_str(),
        Some("4.0")
    );
    assert_eq!(
        restricted.metadata.read_effect_provenance.as_ref().unwrap()["clause"].as_str(),
        Some("restricted")
    );
    assert_eq!(
        restricted.provenance.as_ref().unwrap()["table"].as_str(),
        Some("restricted-table")
    );
    let missing_authority = resolve_field_for_rev(base, Rev::V5_1);
    assert!(missing_authority
        .metadata
        .authority
        .as_ref()
        .unwrap()
        .is_null());
    assert!(!emit_page_per_rev(&schema, Rev::V5_1).contains("set_control"));
    let later = resolve_field_for_rev(base, Rev::V5_2);
    assert_eq!(later.change_source(), Some(ChangeSourceMetadata::Dynamic));
    assert_eq!(
        later.metadata.authority.as_ref().unwrap()["revision"].as_str(),
        Some("5.2")
    );
    assert!(emit_page_per_rev(&schema, Rev::V5_2).contains("set_control"));
}

#[test]
fn revision_flag_false_inherits_across_partial_overrides() {
    let schema = parse_page_schema(&page(
        r#"  - name: control
    offset: 0
    type: u8
    access: rw
    flags: { is_ever_writable: true, is_dynamic: true }
    changed_in:
      - { rev: '4.0', flags: { is_writable: false, is_dynamic: false } }
      - { rev: '5.0', read_effect: unknown }
"#,
    ))
    .unwrap();
    let field = resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
    assert_eq!(field.writable(), Some(false));
    assert_eq!(field.change_source(), Some(ChangeSourceMetadata::Static));
    assert!(!field.emits_setter());
    assert!(!emit_page_per_rev(&schema, Rev::V5_0).contains("set_control"));
}

#[test]
fn include_template_metadata_inherits_but_output_policy_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("_index.yaml"),
        r#"templates:
  flag:
    access: ro
    flags: { is_dynamic: true, is_readable: true, is_ever_writable: true }
    behavior: { latching: clear_on_read }
    read_effect_scope: byte
    read_effect_provenance: { source: standard, clause: latch }
    authority: { revision: '4.0' }
    since: '4.0'
    until: '5.3'
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("part.yaml"),
        r#"fields:
  - name: flag
    offset: 2
    type: u8
    template: flag
    flags: { is_dynamic: false }
    changed_in:
      - { rev: '5.0', note: partial }
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("page.yaml"),
        page("  []\n") + "field_includes: [part.yaml]\n",
    )
    .unwrap();
    let schema = load_page_schema(&tmp.path().join("page.yaml")).unwrap();
    let field = resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
    assert_eq!(field.access, Some(Access::RoCor));
    assert_eq!(field.explicit_access, None);
    assert!(!applicable_revs(&field, Rev::CMIS_ALL).contains(&Rev::V3_0));
    assert!(!applicable_revs(&field, Rev::CMIS_ALL).contains(&Rev::V5_3));
    assert_eq!(field.flags.as_ref().unwrap().is_dynamic, Some(false));
    assert_eq!(
        field.metadata.read_effect_provenance.as_ref().unwrap()["clause"].as_str(),
        Some("latch")
    );
    assert_eq!(field.read_effects(), vec![ReadEffectMetadata::ClearOnRead]);
    assert!(
        !field.emits_setter(),
        "previously ignored template must not add a product setter"
    );
}

#[test]
fn collapsed_instances_and_byte_ranges_retain_complete_bit_masks() {
    let schema = parse_page_schema(&page(
        r#"  - name: flags
    type: bool
    offset_expr: '10 + (lane - 1) / 8'
    bit_offset_expr: '(lane - 1) % 8'
    instances: { index: lane, start: 1, end: 16 }
    access: ro_cor
  - { name: reserved, offset: 20, type: bytes, size_bytes: 3, access: unknown }
"#,
    ))
    .unwrap();
    assert_eq!(
        schema.fields[0].bit_coverage().unwrap(),
        [(10, 0xff), (11, 0xff)].into()
    );
    assert_eq!(
        schema.fields[1].bit_coverage().unwrap(),
        [(20, 0xff), (21, 0xff), (22, 0xff)].into()
    );
}

#[test]
fn revision_effect_conflicts_are_checked_after_partial_inheritance() {
    assert!(parse_page_schema(&page(
        r#"  - name: flag
    offset: 0
    type: u8
    access: ro
    changed_in:
      - { rev: '4.0', access: ro_cor }
      - { rev: '5.0', read_effect: none, read_effect_provenance: 'synthetic test authority' }
"#
    ))
    .is_err());
    // Disjoint applicability doesn't create a fictitious byte conflict.
    assert!(parse_page_schema(&page(r#"  - { name: ack, offset: 0, type: u8, access: ro, until: '4.0', read_effect: acknowledge_startup_interrupt, read_effect_scope: byte }
  - { name: safe, offset: 0, type: u8, access: ro, since: '4.0', read_effect: none, read_effect_provenance: 'synthetic test authority' }
"#)).is_ok());
}

#[test]
fn unknown_and_write_only_are_retained_alongside_known_effects() {
    for (access, unknown, write_only) in [("unknown", 0xff, 0), ("wo", 0, 0xff)] {
        let schema = parse_page_schema(&page(&format!("  - {{ name: mixed, offset: 0, type: u8, access: {access}, read_effect: acknowledge_startup_interrupt, read_effect_scope: byte }}\n"))).unwrap();
        let masks = schema.fields[0].read_effect_masks().unwrap();
        assert_eq!(
            masks[0].known,
            vec![(ReadEffectMetadata::AcknowledgeStartupInterrupt, 0xff)]
        );
        assert_eq!(masks[0].unknown, unknown);
        assert_eq!(masks[0].write_only, write_only);
    }
}

#[test]
fn template_revision_overrides_use_the_existing_resolver() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("_index.yaml"),
        r#"templates:
  control:
    access: rw
    flags: { is_ever_writable: true }
    changed_in:
      - { rev: '4.0', access: ro, read_effect: unknown, authority: { revision: '4.0' } }
      - { rev: '5.0', note: partial }
"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("page.yaml"), page("  - { name: control, offset: 0, type: u8, template: control, flags: { is_ever_writable: true } }\n")).unwrap();
    let schema = load_page_schema(&tmp.path().join("page.yaml")).unwrap();
    assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
    let restricted = resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
    assert_eq!(restricted.access, Some(Access::Ro));
    assert_eq!(
        restricted.metadata.read_effect,
        Some(ReadEffectMetadata::Unknown)
    );
    assert_eq!(
        restricted.metadata.authority.as_ref().unwrap()["revision"].as_str(),
        Some("4.0")
    );
    assert!(!emit_page_per_rev(&schema, Rev::V5_0).contains("set_control"));
}

#[test]
fn later_explicit_access_overrides_legacy_latching_without_leaking_back() {
    let schema = parse_page_schema(&page(
        r#"  - name: flag
    offset: 0
    type: u8
    access: ro
    behavior: { latching: clear_on_read }
    changed_in:
      - { rev: '4.0', access: rw, read_effect: none, read_effect_provenance: 'synthetic test authority' }
      - { rev: '5.0', note: partial }
"#,
    ))
    .unwrap();
    assert_eq!(
        resolve_field_for_rev(&schema.fields[0], Rev::V3_0).access,
        Some(Access::RoCor)
    );
    let later = resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
    assert_eq!(later.access, Some(Access::Rw));
    assert_eq!(later.read_effects(), vec![ReadEffectMetadata::None]);
    assert!(emit_page_per_rev(&schema, Rev::V5_0).contains("set_flag"));
}
