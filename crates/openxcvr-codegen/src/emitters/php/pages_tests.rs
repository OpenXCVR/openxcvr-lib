use super::*;
use crate::ir::pages::{FieldType, PageField, PageSchema};

fn u8_field(name: &str, offset: u32) -> PageField {
    PageField {
        name: name.to_string(),
        display_name: None,
        offset: Some(offset),
        offset_expr: None,
        bit_offset: None,
        bit_offset_expr: None,
        bit_width: None,
        size_bytes: Some(1),
        ty: FieldType::U8,
        enum_ref: None,
        composite_ref: None,
        encoding: None,
        unit: None,
        access: None,
        metadata: Default::default(),
        setter_eligible: None,
        explicit_access: None,
        field_category: None,
        tags: vec![],
        flags: None,
        description: None,
        instances: None,
        variants: None,
        present_when: None,
        content_switch: None,
        enum_inline: None,
        provenance: None,
        scope: None,
        behavior: None,
        constraints: None,
        default_value: None,
        template: None,
        supplement: None,
        since: None,
        until: None,
        changed_in: None,
        renamed_from: None,
        aliased_to: None,
    }
}

fn i16be_field(name: &str, offset: u32) -> PageField {
    PageField {
        name: name.to_string(),
        display_name: None,
        offset: Some(offset),
        offset_expr: None,
        bit_offset: None,
        bit_offset_expr: None,
        bit_width: None,
        size_bytes: Some(2),
        ty: FieldType::I16Be,
        enum_ref: None,
        composite_ref: None,
        encoding: None,
        unit: None,
        access: None,
        metadata: Default::default(),
        setter_eligible: None,
        explicit_access: None,
        field_category: None,
        tags: vec![],
        flags: None,
        description: None,
        instances: None,
        variants: None,
        present_when: None,
        content_switch: None,
        enum_inline: None,
        provenance: None,
        scope: None,
        behavior: None,
        constraints: None,
        default_value: None,
        template: None,
        supplement: None,
        since: None,
        until: None,
        changed_in: None,
        renamed_from: None,
        aliased_to: None,
    }
}

fn lower_schema() -> PageSchema {
    PageSchema {
        spec: None,
        provenance: None,
        schema: "openxcvr.page.v1".to_string(),
        kind: "page".to_string(),
        id: "CmisLowerMemory".to_string(),
        title: "Test".to_string(),
        page_address: "lower".to_string(),
        size_bytes: Some(128),
        field_includes: vec![],
        fields: vec![
            u8_field("sff8024_identifier", 0),
            i16be_field("temp_mon_value", 14),
        ],
        present_when: None,
        banking: None,
        page_select: None,
        page_select_field: None,
    }
}

fn instanced_expr_schema() -> PageSchema {
    let mut field = u8_field("adaptive_input_eq_store_tx", 135);
    field.offset = None;
    field.offset_expr = Some("135 + (lane - 1) / 4".to_string());
    field.bit_offset_expr = Some("((lane - 1) % 4) * 2".to_string());
    field.bit_width = Some(2);
    field.ty = FieldType::U2;
    field.instances = Some(InstancesDesc {
        index: "lane".to_string(),
        start: 1,
        end: 5,
    });

    PageSchema {
        spec: None,
        provenance: None,
        schema: "openxcvr.page.v1".to_string(),
        kind: "page".to_string(),
        id: "CmisPage10hLaneControl".to_string(),
        title: "Test".to_string(),
        page_address: "10h".to_string(),
        size_bytes: Some(128),
        field_includes: vec![],
        fields: vec![field],
        present_when: None,
        banking: None,
        page_select: None,
        page_select_field: None,
    }
}

#[test]
fn test_emit_page_lower() {
    let schema = lower_schema();
    let out = emit_page(&schema);
    assert!(
        out.contains("readonly class CmisLowerMemory"),
        "missing readonly class\n---\n{out}"
    );
    assert!(
        out.contains("namespace Openxcvr\\Generated;"),
        "missing namespace\n---\n{out}"
    );
    assert!(
        out.contains("public static function decode(string $buf): self"),
        "missing decode method\n---\n{out}"
    );
    assert!(
        out.contains("public readonly int $sff8024Identifier,"),
        "missing u8 field\n---\n{out}"
    );
    assert!(
        out.contains("sff8024Identifier: ord($buf[0]),"),
        "missing u8 decode\n---\n{out}"
    );
    assert!(
        out.contains("$tmp14 = unpack('n', substr($buf, 14, 2))[1]"),
        "missing i16be decode\n---\n{out}"
    );
    assert!(
        out.contains("strlen($buf) < 128"),
        "missing length check\n---\n{out}"
    );
}

#[test]
fn test_instanced_field_evaluates_offset_and_bit_offset_exprs() {
    let out = emit_page(&instanced_expr_schema());
    assert!(
        out.contains("public readonly array $adaptiveInputEqStoreTx,"),
        "missing instanced array constructor field\n---\n{out}"
    );
    assert!(
            out.contains(
                "adaptiveInputEqStoreTx: [ord($buf[7]) & 0x3, (ord($buf[7]) >> 2) & 0x3, (ord($buf[7]) >> 4) & 0x3, (ord($buf[7]) >> 6) & 0x3, ord($buf[8]) & 0x3],"
            ),
            "instanced offset/bit expressions decoded incorrectly\n---\n{out}"
        );
}

#[test]
fn two_byte_enum_ref_decodes_full_width() {
    // SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13: "Format ID (Bytes 128-129)",
    // 16-bit big-endian value (CA1Bh / 100Bh). A one-byte read truncates it.
    let mut f = u8_field("format_id", 0);
    f.ty = FieldType::EnumRef;
    f.size_bytes = Some(2);
    f.enum_ref = Some("Sff8636Page22hFormatId".to_string());
    let mut schema = lower_schema();
    schema.fields = vec![f];
    let out = emit_page(&schema);
    assert!(
        out.contains("formatId: unpack('n', substr($buf, 0, 2))[1],"),
        "2-byte enum must decode as big-endian u16\n---\n{out}"
    );
    assert!(
        !out.contains("formatId: ord($buf[0])"),
        "2-byte enum must not truncate to the first byte\n---\n{out}"
    );
}

#[test]
fn u64_is_decimal_string_and_u32_stays_int() {
    let mut le = u8_field("counter_le", 0);
    le.ty = FieldType::U64Le;
    le.size_bytes = Some(8);
    le.access = Some(crate::ir::pages::Access::Rw);
    let mut be = u8_field("counter_be", 8);
    be.ty = FieldType::U64Be;
    be.size_bytes = Some(8);
    be.access = Some(crate::ir::pages::Access::Rw);
    let mut u32 = u8_field("module_power_up_time", 16);
    u32.ty = FieldType::U32Be;
    u32.size_bytes = Some(4);
    let mut schema = lower_schema();
    schema.fields = vec![le, be, u32];
    schema.size_bytes = Some(128);
    let out = emit_page(&schema);
    assert!(out.contains("public readonly string $counterLe,"), "{out}");
    assert!(out.contains("public readonly string $counterBe,"), "{out}");
    assert!(
        out.contains("public readonly int $modulePowerUpTime,"),
        "{out}"
    );
    assert!(
        out.contains("counterLe: sprintf('%u', unpack('P', substr($buf, 0, 8))[1]),"),
        "{out}"
    );
    assert!(
        out.contains("counterBe: sprintf('%u', unpack('J', substr($buf, 8, 8))[1]),"),
        "{out}"
    );
    assert!(
        out.contains("function setCounterLe(string &$buf, string $value)"),
        "{out}"
    );
    assert!(
        out.contains("function setCounterBe(string &$buf, string $value)"),
        "{out}"
    );
    assert!(out.contains("18446744073709551615"), "{out}");
    assert!(out.contains("preg_match('/^[0-9]+$/', $value)"), "{out}");
    assert!(
        out.contains("$raw = strrev($raw);"),
        "le writer reverses\n{out}"
    );
    assert!(
        !out.contains("pack('P', $") && !out.contains("pack('J', $"),
        "writer must not pack the decimal string\n{out}"
    );
    assert!(!out.contains("bcmath") && !out.contains("gmp"), "{out}");
}
