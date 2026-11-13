use std::path::PathBuf;

use openxcvr_codegen::emitters::{c, csharp, go, java, php, python, typescript};
use openxcvr_codegen::ir::enums::EnumSchema;
use openxcvr_codegen::ir::pages::PageSchema;
use openxcvr_codegen::media_interface::{plan_from_schemas, MediaInterfacePlan};
use openxcvr_codegen::parser::enums::load_enum_schema;
use openxcvr_codegen::parser::index::IndexLoader;
use openxcvr_codegen::parser::pages::load_page_schema;

fn schemas_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("schemas")
}

#[test]
fn test_real_schemas_cmis_plan_and_resolution() {
    let root = schemas_root();

    let page_paths =
        IndexLoader::collect_page_schemas(&root, Some("cmis")).expect("collect page schemas");
    let page_schemas: Vec<PageSchema> = page_paths
        .iter()
        .map(|p| load_page_schema(p).expect("load page schema"))
        .collect();

    let enum_paths =
        IndexLoader::collect_enum_schemas(&root, Some("cmis")).expect("collect enum schemas");
    let enum_schemas: Vec<EnumSchema> = enum_paths
        .iter()
        .map(|p| load_enum_schema(p).expect("load enum schema"))
        .collect();

    let plan = plan_from_schemas(&page_schemas, &enum_schemas)
        .expect("plan_from_schemas should succeed")
        .expect("expected Some(MediaInterfacePlan) for cmis");

    assert_eq!(plan.tables.len(), 5);

    // Expected resolutions from brief:
    // (0x01,0x01) base_10g_sw
    assert_eq!(plan.resolve(0x01, 0x01), Some("base_10g_sw"));
    // (0x02,0x01) base_10g_lw
    assert_eq!(plan.resolve(0x02, 0x01), Some("base_10g_lw"));
    // (0x02,0xC0) vendor_specific [SMF range 0xc0..0xef]
    assert_eq!(plan.resolve(0x02, 0xC0), Some("vendor_specific"));
    // (0x02,0xF7) vendor_specific_f7
    assert_eq!(plan.resolve(0x02, 0xF7), Some("vendor_specific_f7"));
    // (0x03,0xC0) linear_active_copper_loopback_module
    assert_eq!(
        plan.resolve(0x03, 0xC0),
        Some("linear_active_copper_loopback_module")
    );
    // (0x03,0xC1) vendor_specific
    assert_eq!(plan.resolve(0x03, 0xC1), Some("vendor_specific"));
    // (0x03,0x02) reserved_02_be [range]
    assert_eq!(plan.resolve(0x03, 0x02), Some("reserved_02_be"));
    // (0x04,0x01) active_cable_ber_1e_12
    assert_eq!(plan.resolve(0x04, 0x01), Some("active_cable_ber_1e_12"));
    // (0x04,0xF7) vendor_specific
    assert_eq!(plan.resolve(0x04, 0xF7), Some("vendor_specific"));
    // (0x05,0x01) base_1000_t
    assert_eq!(plan.resolve(0x05, 0x01), Some("base_1000_t"));

    // None cases: media types 0x00, 0x06, 0x40, 0x90, 0xFF -> none
    assert_eq!(plan.resolve(0x00, 0x01), None);
    assert_eq!(plan.resolve(0x06, 0x01), None);
    assert_eq!(plan.resolve(0x40, 0x01), None);
    assert_eq!(plan.resolve(0x90, 0x01), None);
    assert_eq!(plan.resolve(0xFF, 0x01), None);
}

#[test]
fn test_plan_none_when_no_media_interface_id_app_field() {
    let pages = vec![];
    let enums = vec![];
    let plan = plan_from_schemas(&pages, &enums).expect("should succeed");
    assert!(plan.is_none());
}

fn make_synthetic_page(id: &str, variants_yaml: &str) -> PageSchema {
    let yaml = format!(
        r#"
schema: openxcvr.page.v2
kind: page
id: {id}
title: {id}
spec:
  id: cmis
page_address: lower
size_bytes: 128
fields:
  - name: media_interface_id_app
    offset: 0
    type: u8
    variants:
{variants_yaml}
"#
    );
    serde_norway::from_str(&yaml).expect("valid synthetic page yaml")
}

fn make_synthetic_enum(id: &str, entries_yaml: &str) -> EnumSchema {
    let yaml = format!(
        r#"
schema: openxcvr.enum.v2
kind: enum
id: {id}
title: {id}
spec:
  id: cmis
entries:
{entries_yaml}
"#
    );
    serde_norway::from_str(&yaml).expect("valid synthetic enum yaml")
}

#[test]
fn test_error_on_disagreeing_declarations() {
    let variants1 = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let variants2 = r#"
      discriminator: { field: lower_memory.media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: SmfEnum
"#;

    let pages = vec![
        make_synthetic_page("page_a", variants1),
        make_synthetic_page("page_b", variants2),
    ];
    let enums = vec![
        make_synthetic_enum("MmfEnum", "  - value: 1\n    name: entry_1\n"),
        make_synthetic_enum("SmfEnum", "  - value: 1\n    name: entry_1\n"),
    ];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("disagree"),
        "error should mention disagreement: {msg}"
    );
    assert!(msg.contains("page_b") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_invalid_discriminator() {
    let variants = r#"
      discriminator: { field: unsupported_field }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported discriminator") || msg.contains("discriminator"),
        "error should mention discriminator issue: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_duplicate_case_value() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate case value"),
        "error should mention duplicate case value: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_missing_or_non_mapping_variants() {
    let variants = "      \"not_a_map\"\n";
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("variants must be a mapping"),
        "error should mention variants mapping: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_missing_or_non_mapping_discriminator() {
    let variants = r#"
      discriminator: "not_a_map"
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("discriminator must be a mapping"),
        "error should mention discriminator mapping: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_missing_or_non_mapping_cases() {
    let variants = r#"
      discriminator: { field: media_type }
      cases: "not_a_seq_or_map"
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("cases"),
        "error should mention cases mapping issue: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_case_value_out_of_range() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 256 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("0..255"),
        "error should mention case value range: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_referenced_enum_missing() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MissingEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("MissingEnum") && msg.contains("not found"),
        "error should mention missing enum: {msg}"
    );
    assert!(msg.contains("page_a") && msg.contains("media_interface_id_app"));
}

#[test]
fn test_error_on_entry_value_out_of_range() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 256\n    name: out_of_bound\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("0..255"),
        "error should mention entry value out of range: {msg}"
    );
}

#[test]
fn test_error_on_entry_range_lo_greater_than_hi() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 10..5\n    name: invalid_range\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("lo > hi"),
        "error should mention lo > hi: {msg}"
    );
}

#[test]
fn test_error_on_entry_range_bound_out_of_range() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 200..300\n    name: invalid_bound\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("255"),
        "error should mention range bound out of range: {msg}"
    );
}

#[test]
fn test_error_on_duplicate_code_in_table() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 5\n    name: first\n  - value: 5\n    name: second\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("overlapping entries") && msg.contains("covering the same code"),
        "error should mention duplicate/overlapping code: {msg}"
    );
}

#[test]
fn test_emitters_output() {
    let plan = MediaInterfacePlan {
        tables: vec![(
            0x02,
            "Sff8024SmfMediaInterface".to_string(),
            vec![
                (0x01, 0x01, "base_10g_lw".to_string()),
                (0xC0, 0xEF, "vendor_specific".to_string()),
            ],
        )],
    };

    let py = python::emit_media_interface(&plan);
    assert!(
        py.contains("def cmis_media_interface_name(media_type: int, code: int) -> Optional[str]:")
    );
    assert!(py.contains("Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(py.contains("0xc0 <= code <= 0xef"));

    let go_code = go::emit_media_interface(&plan);
    assert!(go_code.contains("func CmisMediaInterfaceName(mediaType, code byte) (string, bool)"));
    assert!(go_code.contains("Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(go_code.contains("package enums"));
    assert!(go_code.contains("code >= 0xc0 && code <= 0xef"));

    let php_code = php::emit_media_interface(&plan);
    assert!(php_code.contains("namespace Openxcvr\\Generated\\Enums;"));
    assert!(php_code.contains("final class CmisMediaInterface"));
    assert!(php_code.contains("public static function nameOf(int $mediaType, int $code): ?string"));
    assert!(php_code.contains("Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(php_code.contains("$code >= 0xC0 && $code <= 0xEF"));

    let c_code = c::emit_media_interface(&plan);
    assert!(c_code.contains("#pragma once"));
    assert!(c_code.contains("#include <stdint.h>"));
    assert!(c_code.contains("static inline const char *openxcvr_cmis_media_interface_name(uint8_t media_type, uint8_t code)"));
    assert!(c_code.contains("Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(c_code.contains("code >= 0xc0 && code <= 0xef"));

    let ts_code = typescript::emit_media_interface(&plan);
    assert!(ts_code.contains("// Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(ts_code.contains(
        "export function CmisMediaInterfaceName(mediaType: number, code: number): string | null {"
    ));
    assert!(ts_code
        .contains("if (!Number.isInteger(mediaType) || !Number.isInteger(code)) return null;"));
    assert!(ts_code
        .contains("if (mediaType < 0 || mediaType > 255 || code < 0 || code > 255) return null;"));
    assert!(ts_code.contains("code >= 0xc0 && code <= 0xef"));
    assert!(ts_code.contains("default: return null;"));

    let java_code = java::emit_media_interface(&plan);
    assert!(java_code.contains("// Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(java_code.contains("package com.openxcvr.generated;"));
    assert!(java_code.contains("public final class CmisMediaInterface"));
    assert!(
        java_code.contains("public static String name(int mediaType, int code)"),
        "{java_code}"
    );
    assert!(
        java_code.contains("if (mediaType < 0 || mediaType > 255 || code < 0 || code > 255) {"),
        "{java_code}"
    );
    assert!(
        java_code.contains("code >= 0xc0 && code <= 0xef"),
        "{java_code}"
    );
    assert!(
        java_code.contains("if (code == 0x01) return \"base_10g_lw\";"),
        "{java_code}"
    );
    assert!(java_code.contains("default: return null;"), "{java_code}");

    let csharp_code = csharp::emit_media_interface(&plan);
    assert!(csharp_code.contains("// Generated by openxcvr-codegen. DO NOT EDIT."));
    assert!(csharp_code.contains("namespace OpenXcvr.Generated"));
    assert!(csharp_code.contains("public static class CmisMediaInterface"));
    assert!(
        csharp_code.contains("public static string? Name(int mediaType, int code)"),
        "{csharp_code}"
    );
    assert!(
        csharp_code.contains(
            "if (mediaType < 0 || mediaType > 255 || code < 0 || code > 255) return null;"
        ),
        "{csharp_code}"
    );
    assert!(
        csharp_code.contains("code >= 0xc0 && code <= 0xef"),
        "{csharp_code}"
    );
    assert!(
        csharp_code.contains("if (code == 0x01) return \"base_10g_lw\";"),
        "{csharp_code}"
    );
    assert!(
        csharp_code.contains("default: return null;"),
        "{csharp_code}"
    );
}

#[test]
fn test_real_schemas_cmis_typescript_resolver() {
    let root = schemas_root();

    let page_paths =
        IndexLoader::collect_page_schemas(&root, Some("cmis")).expect("collect page schemas");
    let page_schemas: Vec<PageSchema> = page_paths
        .iter()
        .map(|p| load_page_schema(p).expect("load page schema"))
        .collect();

    let enum_paths =
        IndexLoader::collect_enum_schemas(&root, Some("cmis")).expect("collect enum schemas");
    let enum_schemas: Vec<EnumSchema> = enum_paths
        .iter()
        .map(|p| load_enum_schema(p).expect("load enum schema"))
        .collect();

    let plan = plan_from_schemas(&page_schemas, &enum_schemas)
        .expect("plan_from_schemas should succeed")
        .expect("expected Some(MediaInterfacePlan) for cmis");

    let ts_code = typescript::emit_media_interface(&plan);
    assert!(ts_code.contains("case 0x01:"));
    assert!(ts_code.contains("case 0x02:"));
    assert!(ts_code.contains("case 0x03:"));
    assert!(ts_code.contains("case 0x04:"));
    assert!(ts_code.contains("case 0x05:"));
}

#[test]
fn test_error_on_unexpected_key_in_case_item() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
          extra_key: "forbidden"
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("extra_key"),
        "error should mention unexpected key: {msg}"
    );
    assert!(
        msg.contains("page_a") && msg.contains("media_interface_id_app"),
        "error should name page and field: {msg}"
    );
}

#[test]
fn test_error_on_unexpected_key_in_case_when() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01, revision_gte: 5 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("revision_gte"),
        "error should mention unexpected key: {msg}"
    );
    assert!(
        msg.contains("page_a") && msg.contains("media_interface_id_app"),
        "error should name page and field: {msg}"
    );
}

#[test]
fn test_error_on_unexpected_key_in_variants() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
      default: "foo"
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("default"),
        "error should mention unexpected key: {msg}"
    );
    assert!(
        msg.contains("page_a") && msg.contains("media_interface_id_app"),
        "error should name page and field: {msg}"
    );
}

#[test]
fn test_error_on_unexpected_key_in_discriminator() {
    let variants = r#"
      discriminator: { field: media_type, extra_field: 123 }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("extra_field"),
        "error should mention unexpected key: {msg}"
    );
    assert!(
        msg.contains("page_a") && msg.contains("media_interface_id_app"),
        "error should name page and field: {msg}"
    );
}

#[test]
fn test_error_on_missing_variants_when_another_page_declares_it() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let page_with_variants = make_synthetic_page("page_a", variants);

    let yaml_without_variants = r#"
schema: openxcvr.page.v2
kind: page
id: page_b
title: page_b
spec:
  id: cmis
page_address: lower
size_bytes: 128
fields:
  - name: media_interface_id_app
    offset: 0
    type: u8
"#;
    let page_without_variants: PageSchema =
        serde_norway::from_str(yaml_without_variants).expect("valid yaml");

    let pages = vec![page_with_variants, page_without_variants];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("page_b"),
        "error should name the page lacking variants: {msg}"
    );
    assert!(
        msg.contains("media_interface_id_app"),
        "error should name the field: {msg}"
    );
}

#[test]
fn test_error_on_invalid_entry_name_charset() {
    let variants = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 0x01 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: \"invalid-name\"\n",
    )];

    let err = plan_from_schemas(&pages, &enums).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("MmfEnum") && msg.contains("invalid-name"),
        "error should name enum id and invalid entry name: {msg}"
    );
}

#[test]
fn test_real_schemas_non_cmis_specs_return_none() {
    let root = schemas_root();

    for spec in ["sff-8636", "sff-8472"] {
        let page_paths =
            IndexLoader::collect_page_schemas(&root, Some(spec)).expect("collect page schemas");
        let page_schemas: Vec<PageSchema> = page_paths
            .iter()
            .map(|p| load_page_schema(p).expect("load page schema"))
            .collect();

        let enum_paths =
            IndexLoader::collect_enum_schemas(&root, Some(spec)).expect("collect enum schemas");
        let enum_schemas: Vec<EnumSchema> = enum_paths
            .iter()
            .map(|p| load_enum_schema(p).expect("load enum schema"))
            .collect();

        let plan = plan_from_schemas(&page_schemas, &enum_schemas)
            .unwrap_or_else(|e| panic!("plan_from_schemas for spec {spec} should succeed: {e}"));

        assert!(
            plan.is_none(),
            "expected None plan for spec {spec}, got: {plan:?}"
        );
    }
}

#[test]
fn test_error_on_invalid_case_value_types() {
    let variants_float = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: 1.5 }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants_float)];
    let enums = vec![make_synthetic_enum(
        "MmfEnum",
        "  - value: 1\n    name: entry_1\n",
    )];
    assert!(plan_from_schemas(&pages, &enums).is_err());

    let variants_bool = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: true }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants_bool)];
    assert!(plan_from_schemas(&pages, &enums).is_err());

    let variants_overflow = r#"
      discriminator: { field: media_type }
      cases:
        - when: { value: "0x100" }
          enum_ref: MmfEnum
"#;
    let pages = vec![make_synthetic_page("page_a", variants_overflow)];
    assert!(plan_from_schemas(&pages, &enums).is_err());
}
