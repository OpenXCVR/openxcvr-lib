use openxcvr_codegen::emitters::rust::per_rev::emit_page_per_rev;
use openxcvr_codegen::ir::pages::{Access, PageSchema, ReadEffectMetadata};
use openxcvr_codegen::ir::version::{resolve_field_for_rev, Rev};
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};

fn page(body: &str) -> String {
    format!("schema: openxcvr.page.v2\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n  - name: control\n    offset: 0\n    type: u8\n{body}")
}

fn templated(template: &str, body: &str) -> PageSchema {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("_index.yaml"),
        format!("templates:\n  control:\n{template}"),
    )
    .unwrap();
    std::fs::write(tmp.path().join("page.yaml"), page(body)).unwrap();
    load_page_schema(&tmp.path().join("page.yaml")).unwrap()
}

// Oracle: Task10 retains raw-spelling emission compatibility, but a resolved
// RO/COR, Unknown, or explicit writable=false restriction cannot be waived by
// ever-writable. WO/guarded RW need ever-writable to enable their legacy setter.
#[test]
fn finding_4217155307_revision_ever_writable_does_not_get_masked() {
    for (access, ordinary, writable) in [
        ("rw", true, Some(true)),
        ("rww", true, Some(true)),
        ("rw_nv", true, Some(true)),
        ("rw_v", true, Some(true)),
        ("rw_triggered", true, Some(true)),
        ("rw_with_state_guard", false, Some(true)),
        ("wo", false, Some(true)),
        ("wo_sc", false, Some(true)),
        ("ro", false, Some(false)),
        ("ro_cor", false, Some(false)),
        ("cor", false, Some(false)),
        ("unknown", false, None),
        ("implementation_defined", false, None),
    ] {
        for (ever, enables) in [
            ("", false),
            ("is_ever_writable: false,", false),
            ("is_ever_writable: true,", true),
        ] {
            for (flag, denies) in [
                ("", false),
                ("is_writable: false", true),
                ("is_writable: true", false),
            ] {
                let schema = parse_page_schema(&page(&format!("    access: rw\n    changed_in:\n      - {{ rev: '4.0', access: {access}, flags: {{ {ever} {flag} }} }}\n      - {{ rev: '5.0', note: omitted-permission }}\n"))).unwrap();
                assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
                for rev in [Rev::V4_0, Rev::V5_0, Rev::V5_3] {
                    let field = resolve_field_for_rev(&schema.fields[0], rev);
                    let expected = writable == Some(true) && !denies && (ordinary || enables);
                    assert_eq!(
                        field.emits_setter(),
                        expected,
                        "{access}/{ever}/{flag}/{rev:?}"
                    );
                    assert_eq!(
                        emit_page_per_rev(&schema, rev).contains("set_control"),
                        expected
                    );
                }
                // The unrestricted top-level spelling has the same legacy
                // eligibility as the revision spelling, independently checked.
                if writable == Some(true) && !denies {
                    let top = parse_page_schema(&page(&format!(
                        "    access: {access}\n    flags: {{ {ever} {flag} }}\n"
                    )))
                    .unwrap();
                    assert_eq!(top.fields[0].emits_setter(), ordinary || enables);
                }
            }
        }
    }
}

#[test]
fn revision_latching_restrictions_outrank_ever_writable() {
    for access in ["rw", "wo", "rw_with_state_guard"] {
        for (behavior, expected_access, expected_setter) in [
            ("{}", Access::Rw, true),
            ("{ latching: clear_on_read }", Access::RoCor, false),
            (
                "{ latching: clear_on_read_when_condition_clears }",
                Access::RoCor,
                false,
            ),
            (
                "{ latching: implementation_defined }",
                Access::Unknown,
                false,
            ),
        ] {
            let schema = parse_page_schema(&page(&format!("    access: rw\n    changed_in:\n      - {{ rev: '4.0', access: {access}, flags: {{ is_ever_writable: true }}, behavior: {behavior} }}\n      - {{ rev: '5.0', note: partial }}\n"))).unwrap();
            for rev in [Rev::V4_0, Rev::V5_0] {
                let field = resolve_field_for_rev(&schema.fields[0], rev);
                let expected_access = if behavior == "{}" && access == "wo" {
                    Access::Wo
                } else {
                    expected_access
                };
                assert_eq!(field.access, Some(expected_access));
                assert_eq!(
                    emit_page_per_rev(&schema, rev).contains("set_control"),
                    expected_setter
                );
            }
        }
    }
}

#[test]
fn template_and_field_revision_flags_merge_at_equal_or_later_revision() {
    for field_rev in ["4.0", "5.0"] {
        for (flag, expected) in [("true", true), ("false", false)] {
            let schema = templated("    changed_in:\n      - { rev: '4.0', access: wo, flags: { is_ever_writable: false } }\n", &format!("    access: rw\n    template: control\n    changed_in:\n      - {{ rev: '{field_rev}', flags: {{ is_ever_writable: {flag} }} }}\n      - {{ rev: '5.1', note: partial }}\n"));
            assert!(emit_page_per_rev(&schema, Rev::V3_0).contains("set_control"));
            if field_rev == "5.0" {
                assert!(!emit_page_per_rev(&schema, Rev::V4_0).contains("set_control"));
            }
            for rev in [Rev::V5_0, Rev::V5_1] {
                assert_eq!(
                    emit_page_per_rev(&schema, rev).contains("set_control"),
                    expected,
                    "{field_rev}/{flag}/{rev:?}"
                );
            }
        }
    }
}

// Task10's omitted-key inheritance applies inside behavior maps, just as the
// existing equal-revision template/field merge does. Explicit keys override.
#[test]
fn finding_4217155313_partial_behavior_accumulates_across_revisions() {
    let schema = parse_page_schema(&page("    access: rw\n    changed_in:\n      - { rev: '5.0', behavior: { write_trigger: self_clearing } }\n      - { rev: '4.0', behavior: { latching: clear_on_read, reset_domain: module } }\n      - { rev: '5.1', behavior: { reset_domain: lane } }\n")).unwrap();
    assert_eq!(
        resolve_field_for_rev(&schema.fields[0], Rev::V3_0).behavior,
        None
    );
    let v4 = resolve_field_for_rev(&schema.fields[0], Rev::V4_0);
    assert_eq!(
        v4.behavior.as_ref().unwrap()["write_trigger"],
        serde_norway::Value::Null
    );
    for (rev, reset) in [
        (Rev::V4_0, "module"),
        (Rev::V5_0, "module"),
        (Rev::V5_1, "lane"),
    ] {
        let field = resolve_field_for_rev(&schema.fields[0], rev);
        let behavior = field.behavior.as_ref().unwrap();
        assert_eq!(behavior["latching"].as_str(), Some("clear_on_read"));
        assert_eq!(behavior["reset_domain"].as_str(), Some(reset));
        if rev >= Rev::V5_0 {
            assert_eq!(behavior["write_trigger"].as_str(), Some("self_clearing"));
        }
        assert_eq!(field.read_effects(), vec![ReadEffectMetadata::ClearOnRead]);
        assert!(!emit_page_per_rev(&schema, rev).contains("set_control"));
    }
}

#[test]
fn behavior_equal_and_different_revision_overrides_and_nulls() {
    for field_rev in ["4.0", "5.0"] {
        let schema = templated("    changed_in:\n      - { rev: '4.0', behavior: { latching: clear_on_read, reset_domain: module } }\n", &format!("    access: rw\n    template: control\n    changed_in:\n      - {{ rev: '{field_rev}', behavior: {{ reset_domain: null, write_trigger: self_clearing }} }}\n      - {{ rev: '5.1', behavior: null }}\n      - {{ rev: '5.2', behavior: {{ reset_domain: lane }} }}\n"));
        // Existing Option<behavior> contract: a whole null is absent/inherits;
        // a present mapping key with null overrides only that key. Neither
        // removes omitted latching or licenses a previously restricted setter.
        for rev in [Rev::V5_0, Rev::V5_1] {
            let field = resolve_field_for_rev(&schema.fields[0], rev);
            let behavior = field.behavior.as_ref().unwrap();
            assert!(behavior["reset_domain"].is_null());
            assert_eq!(behavior["latching"].as_str(), Some("clear_on_read"));
            assert_eq!(behavior["write_trigger"].as_str(), Some("self_clearing"));
            assert!(!emit_page_per_rev(&schema, rev).contains("set_control"));
        }
        let later = resolve_field_for_rev(&schema.fields[0], Rev::V5_2);
        assert_eq!(
            later.behavior.as_ref().unwrap()["reset_domain"].as_str(),
            Some("lane")
        );
        assert_eq!(
            later.behavior.as_ref().unwrap()["latching"].as_str(),
            Some("clear_on_read")
        );
    }
}

#[test]
fn finding_4217155320_actual_sff8472_flag_is_unknown_not_readable() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/sff-8472");
    let schema = load_page_schema(&root.join("page-a2h-diagnostics-base.yaml")).unwrap();
    let field = schema
        .fields
        .iter()
        .find(|f| f.name == "alarm_warning_flag")
        .unwrap();
    assert_eq!(field.template.as_deref(), Some("flag"));
    assert_eq!(field.flags.as_ref().unwrap().is_readable, Some(true));
    assert_eq!(field.access, Some(Access::Unknown));
    assert_eq!(field.read_effects(), vec![ReadEffectMetadata::Unknown]);
    assert_eq!(
        field.readable(),
        None,
        "template true must not qualify unknown access"
    );
    assert_eq!(field.writable(), None);
    assert!(!field.emits_setter());
}

// Unknown is unresolved (None), not a known prohibition (Some(false)). RO/COR
// remains readable unless explicitly denied; WO is never readable. An explicit
// None read-effect assertion cannot turn restrictive access into safe access.
#[test]
fn revision_unknown_stays_unresolved_despite_readable_flag_overrides() {
    for flag in ["true", "false"] {
        let schema = parse_page_schema(&page(&format!("    access: rw\n    flags: {{ is_readable: true }}\n    changed_in:\n      - {{ rev: '4.0', access: unknown, flags: {{ is_readable: {flag} }} }}\n      - {{ rev: '5.0', flags: {{ is_readable: true }} }}\n      - {{ rev: '5.1', access: ro, flags: {{ is_readable: false }} }}\n"))).unwrap();
        assert_eq!(
            resolve_field_for_rev(&schema.fields[0], Rev::V3_0).readable(),
            Some(true)
        );
        for rev in [Rev::V4_0, Rev::V5_0] {
            let field = resolve_field_for_rev(&schema.fields[0], rev);
            assert_eq!(field.readable(), None);
            assert_eq!(field.writable(), None);
            assert!(!emit_page_per_rev(&schema, rev).contains("set_control"));
        }
        // A later known permission permits a known denial, not Unknown=false.
        assert_eq!(
            resolve_field_for_rev(&schema.fields[0], Rev::V5_1).readable(),
            Some(false)
        );
    }
}

#[test]
fn unknown_readable_precedence_matrix_and_conflicting_none() {
    for (access, latching, default) in [
        ("ro", "null", Some(true)),
        ("rw", "null", Some(true)),
        ("wo", "null", Some(false)),
        ("wo_sc", "null", Some(false)),
        ("ro_cor", "null", Some(true)),
        ("cor", "null", Some(true)),
        ("unknown", "null", None),
        ("implementation_defined", "null", None),
        ("rw", "implementation_defined", None),
        ("rw", "clear_on_read", Some(true)),
        ("rw", "clear_on_read_when_condition_clears", Some(true)),
    ] {
        for (flag, value) in [
            ("", None),
            ("is_readable: true", Some(true)),
            ("is_readable: false", Some(false)),
        ] {
            for from_template in [false, true] {
                let schema = if from_template {
                    templated(&format!("    access: {access}\n    behavior: {{ latching: {latching} }}\n    flags: {{ is_readable: true }}\n"), &format!("    template: control\n    flags: {{ {flag} }}\n"))
                } else {
                    parse_page_schema(&page(&format!("    access: {access}\n    behavior: {{ latching: {latching} }}\n    flags: {{ {flag} }}\n"))).unwrap()
                };
                let expected = match default {
                    None => None,
                    Some(false) => Some(false),
                    Some(true) => value.or(if from_template { Some(true) } else { default }),
                };
                for rev in [Rev::V3_0, Rev::V5_0] {
                    assert_eq!(
                        resolve_field_for_rev(&schema.fields[0], rev).readable(),
                        expected,
                        "{access}/{latching}/{flag}/template={from_template}"
                    );
                }
            }
        }
        let none = parse_page_schema(&page(&format!("    access: {access}\n    behavior: {{ latching: {latching} }}\n    read_effect: none\n    read_effect_provenance: 'synthetic test authority'\n")));
        let restrictive =
            default != Some(true) || access == "ro_cor" || access == "cor" || latching != "null";
        assert_eq!(
            none.is_err(),
            restrictive,
            "None conflicts with {access}/{latching}"
        );
    }
}
