use openxcvr_codegen::emitters::rust::{pages::emit_page, per_rev::emit_page_per_rev};
use openxcvr_codegen::ir::version::Rev;
use openxcvr_codegen::parser::pages::parse_page_schema;

fn page(fields: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    parse_page_schema(&format!("schema: openxcvr.page.v2\nkind: page\nid: CmisLowerMemory\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n{fields}")).unwrap()
}

#[test]
fn explicit_none_qualifies_but_ro_alone_unknown_and_absent_do_not() {
    let output = emit_page(&page("  - { name: safe, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n  - { name: unmarked, offset: 1, type: u8, access: ro }\n  - { name: unknown, offset: 2, type: u8, access: unknown }\n  - { name: latched, offset: 3, type: u8, access: ro_cor }\n"));
    assert!(output.contains("pub fn read_classifications()"), "{output}");
    assert!(
        output.contains(
            "0 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
        ),
        "{output}"
    );
    assert!(
        output.contains("1 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[]"),
        "{output}"
    );
    assert!(
        output.contains("2 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[]"),
        "{output}"
    );
    assert!(output.contains("3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::ClearOnRead]"), "{output}");
    assert!(
        output.contains("4..=127 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[]"),
        "{output}"
    );
}

#[test]
fn constituent_coverage_is_unioned_not_intersected_and_effects_survive() {
    let output = emit_page(&page("  - { name: safe_low, offset: 0, type: u4, bit_offset: 0, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n  - { name: safe_high, offset: 0, type: u4, bit_offset: 4, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n  - { name: flag, offset: 1, type: bool, access: ro_cor, read_effect_scope: byte }\n  - { name: sibling, offset: 1, type: u7, bit_offset: 1, access: ro }\n  - { name: wo, offset: 2, type: u8, access: wo }\n"));
    assert!(
        output.contains(
            "0 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
        ),
        "{output}"
    );
    assert!(output.contains("1 => ByteClassification::from_masks(0xff, 0xfe, 0x00, 0x00, &[ReadEffect::ClearOnRead]"), "{output}");
    assert!(
        output.contains("2 => ByteClassification::from_masks(0x00, 0x00, 0x00, 0xff, &[]"),
        "{output}"
    );
}

#[test]
fn conflicting_field_assertions_never_qualify_a_byte() {
    let output = emit_page(&page("  - { name: safe, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n  - { name: flag, offset: 0, type: u8, access: ro_cor }\n"));
    assert!(
        output.contains("&[ReadEffect::None, ReadEffect::ClearOnRead]"),
        "{output}"
    );
    // Execute the actual emitted table with the consumer's predicate, rather
    // than merely checking that a restrictive effect appears somewhere in text.
    let start = output.find("    pub fn read_classifications()").unwrap();
    let end = start + output[start..].find("\n    }\n").unwrap() + "\n    }\n".len();
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("conflict.rs");
    std::fs::write(&source, format!("{}\nfn main() {{ let table = read_classifications(); assert!(!table[0].is_qualified_none()); }}", &output[start..end])).unwrap();
    trybuild::TestCases::new().pass(&source);
}

#[test]
fn the_table_resolves_53_overrides_and_is_not_emitted_for_earlier_revisions() {
    let schema = page("  - name: f\n    offset: 0\n    type: u8\n    access: ro\n    read_effect: none\n    read_effect_provenance: 'synthetic test authority'\n    changed_in:\n      - { rev: '5.3', access: unknown, read_effect: unknown }\n  - { name: gone, offset: 1, type: u8, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority', until: '5.3' }\n");
    let output = emit_page(&schema);
    assert!(
        output.contains("0 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[]"),
        "{output}"
    );
    assert!(
        output.contains("1..=127 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[]"),
        "{output}"
    );
    assert!(!emit_page_per_rev(&schema, Rev::V5_2).contains("pub fn read_classifications()"));
}

#[test]
fn upper_page_table_indices_are_page_relative() {
    let mut schema =
        page("  - { name: safe, offset: 128, type: u8, access: ro, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n");
    schema.id = "CmisPage00hAdministrative".into();
    schema.page_address = "00h".into();
    let output = emit_page(&schema);
    assert!(
        output.contains(
            "0 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
        ),
        "{output}"
    );
}

#[test]
fn a_conflicting_readable_flag_cannot_turn_into_passive_none() {
    let output = emit_page(&page("  - { name: conflict, offset: 0, type: u8, access: ro, flags: { is_readable: false }, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n"));
    assert!(
        output.contains(
            "0 => ByteClassification::from_masks(0xff, 0xff, 0x00, 0x00, &[ReadEffect::None]"
        ),
        "{output}"
    );
}

#[test]
fn uncited_none_is_rejected_in_base_and_revision_metadata() {
    for fields in [
        "  - { name: uncited, offset: 0, type: u8, access: ro, read_effect: none }\n",
        "  - { name: uncited, offset: 0, type: u8, access: ro, changed_in: [{ rev: '5.3', read_effect: none }] }\n",
        "  - { name: uncited, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: null }\n",
        "  - { name: uncited, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: {} }\n",
        "  - { name: uncited, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: '' }\n",
        "  - { name: uncited, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: [] }\n",
        "  - { name: cleared, offset: 0, type: u8, access: ro, read_effect: none, read_effect_provenance: 'synthetic authority', changed_in: [{ rev: '5.3', read_effect_provenance: null }] }\n",
    ] {
        let yaml = format!("schema: openxcvr.page.v2\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n{fields}");
        let error = parse_page_schema(&yaml).expect_err("uncited none must fail closed");
        assert!(error.to_string().contains("read_effect_provenance"), "{error}");
    }
}

#[test]
fn revision_40_classification_uses_40_metadata_not_a_53_override() {
    let schema = page("  - { name: changed, offset: 0, type: u8, access: ro, changed_in: [{ rev: '5.3', read_effect: none, read_effect_provenance: 'synthetic test authority' }] }\n");
    let output = emit_page_per_rev(&schema, Rev::V4_0);
    assert!(output.contains("CMIS 4.0 per-byte classification"));
    assert!(output.contains("0 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[]"));
    assert!(!output.contains("ReadEffect::None"));
    let output = emit_page(&schema);
    assert!(output.contains("ReadEffect::None"));
}

#[test]
fn module_state_supplied_accessor_is_emitted_for_real_cmis4_schema() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema =
        openxcvr_codegen::parser::pages::load_page_schema(&root.join("cmis/lower-memory.yaml"))
            .unwrap();
    let output = emit_page_per_rev(&schema, Rev::V4_0);
    assert!(output.contains("pub const MODULE_STATE_RANGE: (usize, usize) = (3, 1);"));
    assert!(output.contains("pub fn decode_module_state_supplied(bytes: &[u8], offset: usize) -> Result<CmisModuleState, openxcvr_core::supplied::SpanError>"));
    assert!(output.contains("field_bytes(bytes, offset, 3, 1)?"));
    assert!(output.contains("3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None], ChangeSource::Dynamic)"));
    let output53 = emit_page(&schema);
    // Independently qualified CMIS 5.3 status is now passive, while the
    // neighboring summary bytes keep their original unknown/uncovered masks.
    assert!(output53.contains("3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None], ChangeSource::Dynamic)"));
    assert!(
        output53.contains("4..=7 => ByteClassification::from_masks(0x00, 0x0f, 0xf0, 0x00, &[]")
    );
    assert!(!output53.contains("CMIS4p0"));
}

#[test]
fn cmis4_status_requires_each_source_qualified_constituent_and_resets_at_50() {
    use openxcvr_codegen::ir::{pages::ReadEffectMetadata, version::resolve_field_for_rev};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema =
        openxcvr_codegen::parser::pages::load_page_schema(&root.join("cmis/lower-memory.yaml"))
            .unwrap();
    for name in [
        "reserved_module_status_cmis4",
        "module_state",
        "interrupt_deasserted",
    ] {
        let field = schema.fields.iter().find(|f| f.name == name).unwrap();
        let qualified = resolve_field_for_rev(field, Rev::V4_0);
        assert_eq!(
            qualified.metadata.read_effect,
            Some(ReadEffectMetadata::None)
        );
        // The source name and section are recorded on every constituent,
        // not inferred from a decoder or a read-only access token.
        let provenance = format!("{:?}", qualified.metadata.read_effect_provenance);
        assert!(
            provenance.contains("CMIS4p0_Third_Party_Spec"),
            "{name}: {provenance}"
        );
        if name != "reserved_module_status_cmis4" {
            let reset = resolve_field_for_rev(field, Rev::V5_0);
            assert_eq!(
                reset.metadata.read_effect,
                Some(ReadEffectMetadata::Unknown)
            );
            assert_eq!(
                reset.metadata.read_effect_provenance,
                Some(serde_norway::Value::Null)
            );
        }
        let mut unknown = schema.clone();
        let uncertain = unknown.fields.iter_mut().find(|f| f.name == name).unwrap();
        uncertain.changed_in = None;
        uncertain.metadata.read_effect = Some(ReadEffectMetadata::Unknown);
        uncertain.metadata.read_effect_provenance = None;
        let output = emit_page_per_rev(&unknown, Rev::V4_0);
        assert!(
            !output.contains(
                "3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
            ),
            "unknown {name} must deny full-byte admission"
        );
        let mut missing = schema.clone();
        missing.fields.retain(|f| f.name != name);
        let output = emit_page_per_rev(&missing, Rev::V4_0);
        assert!(
            !output.contains(
                "3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
            ),
            "missing {name} must deny full-byte admission"
        );
    }
}

#[test]
fn page01_byte142_is_qualified_from_real_53_metadata_only() {
    use openxcvr_codegen::ir::{pages::ReadEffectMetadata, version::resolve_field_for_rev};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = openxcvr_codegen::parser::pages::load_page_schema(
        &root.join("cmis/page-01h-advertising.yaml"),
    )
    .unwrap();
    let fields: Vec<_> = schema
        .fields
        .iter()
        .filter(|f| f.offset == Some(142))
        .collect();
    assert_eq!(fields.len(), 7);
    for field in fields {
        for &revision in Rev::CMIS_ALL {
            let resolved = resolve_field_for_rev(field, revision);
            if revision >= Rev::V5_3 {
                assert_eq!(
                    resolved.metadata.read_effect,
                    Some(ReadEffectMetadata::None),
                    "{}",
                    field.name
                );
                let provenance = format!("{:?}", resolved.metadata.read_effect_provenance);
                for citation in ["OIF-CMIS-05.3", "8.4", "8-46", "168", "171"] {
                    assert!(provenance.contains(citation), "{provenance}");
                }
            } else {
                assert_ne!(
                    resolved.metadata.read_effect,
                    Some(ReadEffectMetadata::None),
                    "{} at {revision:?}",
                    field.name
                );
            }
        }
    }
    for output in [emit_page(&schema), emit_page_per_rev(&schema, Rev::V5_3)] {
        assert!(output.contains("CMIS 5.3 per-byte classification"));
        assert!(output.contains("14 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None], ChangeSource::Static)"));
    }
    let output = emit_page_per_rev(&schema, Rev::V4_0);
    assert!(output.contains("CMIS 4.0 per-byte classification"));
    assert!(!output.contains("ReadEffect::None"));
    for revision in [Rev::V3_0, Rev::V5_0, Rev::V5_1, Rev::V5_2] {
        assert!(!emit_page_per_rev(&schema, revision).contains("pub fn read_classifications()"));
    }
}

#[test]
fn page01_capability_and_firmware_effects_are_bound_to_53() {
    use openxcvr_codegen::ir::{pages::ReadEffectMetadata, version::resolve_field_for_rev};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis");
    for (page, offsets, section, table) in [
        (
            "page-01h-advertising.yaml",
            vec![
                128, 129, 145, 146, 147, 150, 151, 153, 154, 155, 156, 157, 158, 159, 160, 161,
                162, 163, 164, 165, 166,
            ],
            "8.4",
            "Table 8-",
        ),
        ("lower-memory.yaml", vec![39, 40], "8.2.9", "Table 8-15"),
    ] {
        let schema = openxcvr_codegen::parser::pages::load_page_schema(&root.join(page)).unwrap();
        for offset in offsets {
            let fields: Vec<_> = schema
                .fields
                .iter()
                .filter(|f| f.offset == Some(offset))
                .collect();
            assert!(!fields.is_empty(), "{page}: {offset}");
            for field in fields {
                for &revision in Rev::CMIS_ALL {
                    let resolved = resolve_field_for_rev(field, revision);
                    if revision >= Rev::V5_3 {
                        assert_eq!(
                            resolved.metadata.read_effect,
                            Some(ReadEffectMetadata::None),
                            "{}",
                            field.name
                        );
                        let provenance = format!("{:?}", resolved.metadata.read_effect_provenance);
                        for citation in ["OIF-CMIS-05.3", section, table, "row", "pdf_page"] {
                            assert!(
                                provenance.contains(citation),
                                "{}: {provenance}",
                                field.name
                            );
                        }
                    } else {
                        assert_ne!(
                            resolved.metadata.read_effect,
                            Some(ReadEffectMetadata::None),
                            "{} at {revision:?}",
                            field.name
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cmis53_status_qualification_is_revision_isolated_and_requires_all_constituents() {
    use openxcvr_codegen::ir::{
        pages::ReadEffectMetadata,
        version::{applicable_revs, resolve_field_for_rev},
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema =
        openxcvr_codegen::parser::pages::load_page_schema(&root.join("cmis/lower-memory.yaml"))
            .unwrap();
    let names = [
        "reserved_module_status_cmis53",
        "module_state",
        "interrupt_deasserted",
    ];
    for name in names {
        let field = schema.fields.iter().find(|f| f.name == name).unwrap();
        let qualified = resolve_field_for_rev(field, Rev::V5_3);
        assert_eq!(
            qualified.metadata.read_effect,
            Some(ReadEffectMetadata::None)
        );
        let provenance = format!("{:?}", qualified.metadata.read_effect_provenance);
        assert!(provenance.contains("OIF-CMIS-05.3"), "{name}: {provenance}");
        assert!(provenance.contains("Table 8-6"), "{name}: {provenance}");
        assert_eq!(
            qualified.flags.as_ref().unwrap().is_dynamic,
            Some(name != names[0])
        );
        assert_eq!(qualified.flags.as_ref().unwrap().is_writable, Some(false));
        assert_eq!(
            qualified.flags.as_ref().unwrap().is_ever_writable,
            Some(false)
        );
        if name == names[0] {
            assert_eq!(
                applicable_revs(field, Rev::CMIS_ALL),
                [Rev::V5_3, Rev::V5_4].into_iter().collect()
            );
        } else {
            assert_eq!(
                resolve_field_for_rev(field, Rev::V4_0).metadata.read_effect,
                Some(ReadEffectMetadata::None)
            );
            for rev in [Rev::V5_0, Rev::V5_1, Rev::V5_2] {
                let reset = resolve_field_for_rev(field, rev);
                assert_eq!(
                    reset.metadata.read_effect,
                    Some(ReadEffectMetadata::Unknown)
                );
                assert_eq!(
                    reset.metadata.read_effect_provenance,
                    Some(serde_norway::Value::Null)
                );
                // These decoder revisions have no serving planner table;
                // preserve that absence rather than inventing support.
                assert!(!emit_page_per_rev(&schema, rev).contains("pub fn read_classifications()"));
            }
        }
        // Low-bit None cannot fake absent high-bit coverage: deleting any
        // one constituent must deny the physical byte.
        let mut missing = schema.clone();
        missing.fields.retain(|f| f.name != name);
        assert!(!emit_page(&missing).contains(
            "3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
        ));
        let mut unknown = schema.clone();
        let uncertain = unknown.fields.iter_mut().find(|f| f.name == name).unwrap();
        uncertain.changed_in = None;
        uncertain.metadata.read_effect = Some(ReadEffectMetadata::Unknown);
        uncertain.metadata.read_effect_provenance = None;
        assert!(!emit_page(&unknown).contains(
            "3 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"
        ));
    }
    let output = emit_page(&schema);
    assert!(!output.contains("pub reserved_module_status_cmis53:"));
    assert!(!output.contains("pub fn set_reserved_module_status_cmis53"));
    assert!(!output.contains("pub fn decode_reserved_module_status_cmis53"));
}
