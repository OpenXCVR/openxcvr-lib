use openxcvr_codegen::emitters::rust::per_rev::emit_page_per_rev;
use openxcvr_codegen::ir::version::Rev;
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};

fn page(fields: &str) -> String {
    format!("schema: openxcvr.page.v2\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n{fields}")
}

fn templated(template: &str, fields: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("_index.yaml"),
        format!("templates:\n  control:\n{template}"),
    )
    .unwrap();
    std::fs::write(tmp.path().join("page.yaml"), page(fields)).unwrap();
    load_page_schema(&tmp.path().join("page.yaml")).unwrap()
}

#[test]
fn finding_4216491237_actual_required_identifier() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = load_page_schema(&root.join("sff-8636/lower-memory.yaml")).unwrap();
    let field = schema
        .fields
        .iter()
        .find(|f| f.name == "sff8024_identifier")
        .unwrap();
    assert!(
        field.flags.as_ref().unwrap().is_required == Some(true),
        "explicit required=true lost to informational template"
    );
}

#[test]
fn finding_4216491237_required_precedence() {
    for (template, explicit, expected) in [
        (false, "true", true),
        (true, "false", false),
        (true, "", true),
        (false, "", false),
    ] {
        let flags = if explicit.is_empty() {
            "{}".to_string()
        } else {
            format!("{{ is_required: {explicit} }}")
        };
        let schema = templated(
            &format!("    flags: {{ is_required: {template} }}\n"),
            &format!(
                "  - {{ name: control, offset: 0, type: u8, template: control, flags: {flags} }}\n"
            ),
        );
        assert_eq!(
            schema.fields[0].flags.as_ref().unwrap().is_required,
            Some(expected),
            "template={template} explicit={explicit}"
        );
    }
}

#[test]
fn finding_4216491245_template_history_and_equal_revision() {
    for (field_history, later_setter) in [
        ("      - { rev: '5.0', read_effect: none, read_effect_provenance: 'synthetic test authority' }\n", false),
        (
            "      - { rev: '4.0', access: rw, flags: { is_dynamic: false }, read_effect: none, read_effect_provenance: 'synthetic test authority' }\n",
            true,
        ),
    ] {
        let schema = templated("    changed_in:\n      - { rev: '4.0', access: ro, flags: { is_dynamic: true, is_readable: false }, authority: { clause: template } }\n", &format!("  - name: control\n    offset: 0\n    type: u8\n    access: rw\n    template: control\n    changed_in:\n{field_history}"));
        assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
        for rev in [Rev::V4_0, Rev::V5_0, Rev::V5_3] {
            assert_eq!(
                emit_page_per_rev(&schema, rev).contains("set_control"),
                later_setter,
                "{rev:?}: template restriction erased"
            );
        }
        let field =
            openxcvr_codegen::ir::version::resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
        assert_eq!(
            field.flags.as_ref().unwrap().is_readable,
            Some(false),
            "omitted equal-rev flag must inherit"
        );
        assert_eq!(
            field.flags.as_ref().unwrap().is_dynamic,
            Some(!later_setter),
            "explicit false must win"
        );
        assert_eq!(
            field.metadata.authority.as_ref().unwrap()["clause"].as_str(),
            Some("template")
        );
    }
}

#[test]
fn finding_4216491251_revision_alias_setter_policy() {
    for (access, expected) in [
        ("rw_with_state_guard", false),
        ("rw", true),
        ("rw_nv", true),
        ("wo", false),
    ] {
        let schema = parse_page_schema(&page(&format!("  - name: control\n    offset: 0\n    type: u8\n    access: rw\n    changed_in:\n      - {{ rev: '4.0', access: {access} }}\n      - {{ rev: '5.0', note: partial }}\n"))).unwrap();
        assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
        for rev in [Rev::V4_0, Rev::V5_0, Rev::V5_1, Rev::V5_2, Rev::V5_3] {
            assert_eq!(
                emit_page_per_rev(&schema, rev).contains("set_control"),
                expected,
                "{access} at {rev:?}"
            );
        }
    }
}

#[test]
fn finding_4216491255_unrelated_behavior_preserves_api() {
    let schema = parse_page_schema(&page("  - name: control\n    offset: 0\n    type: u8\n    access: ro\n    flags: { is_ever_writable: true }\n    changed_in:\n      - { rev: '4.0', behavior: { reset_domain: module } }\n      - { rev: '5.0', note: partial }\n")).unwrap();
    let before = emit_page_per_rev(&schema, Rev::V3_0);
    assert!(before.contains("set_control"));
    for rev in [Rev::V4_0, Rev::V5_0, Rev::V5_3] {
        assert_eq!(
            emit_page_per_rev(&schema, rev),
            before,
            "unrelated behavior removed legacy setter at {rev:?}"
        );
    }
}

#[test]
fn finding_4216491255_permission_controls_still_restrict() {
    for base_access in ["ro", "rw"] {
        for entry in [
            "behavior: { latching: clear_on_read }",
            "behavior: { latching: implementation_defined }",
            "access: ro",
        ] {
            let schema = parse_page_schema(&page(&format!("  - name: control\n    offset: 0\n    type: u8\n    access: {base_access}\n    flags: {{ is_ever_writable: true }}\n    changed_in:\n      - {{ rev: '4.0', {entry} }}\n      - {{ rev: '5.0', note: partial }}\n"))).unwrap();
            assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
            for rev in [Rev::V4_0, Rev::V5_0, Rev::V5_3] {
                assert!(
                    !emit_page_per_rev(&schema, rev).contains("set_control"),
                    "{entry} at {rev:?}"
                );
            }
        }
    }
}

#[test]
fn required_presence_remains_distinct_without_template() {
    let schema = parse_page_schema(&page("  - { name: absent, offset: 0, type: u8, flags: {} }\n  - { name: explicit, offset: 1, type: u8, flags: { is_required: false } }\n")).unwrap();
    assert_eq!(schema.fields[0].flags.as_ref().unwrap().is_required, None);
    assert_eq!(
        schema.fields[1].flags.as_ref().unwrap().is_required,
        Some(false)
    );
}

#[test]
fn template_revision_alias_and_include_policy_survive_partial_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("_index.yaml"), "templates:\n  control:\n    changed_in:\n      - { rev: '4.0', access: rw_with_state_guard, read_effect: unknown, provenance: { clause: template }, flags: { is_dynamic: true } }\n").unwrap();
    std::fs::write(tmp.path().join("part.yaml"), "fields:\n  - name: control\n    offset: 0\n    type: u8\n    access: rw\n    template: control\n    changed_in:\n      - { rev: '4.0', read_effect: none, read_effect_provenance: 'synthetic test authority', provenance: null, flags: { is_dynamic: false } }\n      - { rev: '5.0', note: partial }\n").unwrap();
    std::fs::write(
        tmp.path().join("page.yaml"),
        page("  []\n") + "field_includes: [part.yaml]\n",
    )
    .unwrap();
    let schema = load_page_schema(&tmp.path().join("page.yaml")).unwrap();
    assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
    for rev in [Rev::V4_0, Rev::V5_0, Rev::V5_3] {
        let field = openxcvr_codegen::ir::version::resolve_field_for_rev(&schema.fields[0], rev);
        assert!(!emit_page_per_rev(&schema, rev).contains("set_control"));
        assert_eq!(
            field.metadata.read_effect,
            Some(openxcvr_codegen::ir::pages::ReadEffectMetadata::None)
        );
        assert_eq!(field.provenance, Some(serde_norway::Value::Null));
        assert_eq!(field.flags.as_ref().unwrap().is_dynamic, Some(false));
    }
}
