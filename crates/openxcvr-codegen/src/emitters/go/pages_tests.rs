use super::*;
use crate::ir::pages::{PageField, PageSchema};

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

fn upper_schema() -> PageSchema {
    PageSchema {
        spec: None,
        provenance: None,
        schema: "openxcvr.page.v1".to_string(),
        kind: "page".to_string(),
        id: "CmisPage00hAdministrative".to_string(),
        title: "Test".to_string(),
        page_address: "00h".to_string(),
        size_bytes: Some(128),
        field_includes: vec![],
        fields: vec![
            u8_field("connector_type", 128), // flat 128 → file offset 0
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
fn test_lower_page_offsets() {
    let code = emit_page(&lower_schema());
    assert!(code.contains("package generated"));
    assert!(code.contains("type CmisLowerMemory struct"));
    assert!(code.contains("func DecodeCmisLowerMemory(buf []byte)"));
    // U8 at flat 0, lower base=0, file_off=0
    assert!(
        code.contains("Sff8024Identifier: int(buf[0])"),
        "wrong U8 decode\n---\n{code}"
    );
    // I16Be at flat 14, lower base=0, file_off=14
    assert!(
        code.contains("TempMonValue:      int(int16(binary.BigEndian.Uint16(buf[14:16])))"),
        "wrong I16Be decode\n---\n{code}"
    );
    assert!(
        code.contains("\"encoding/binary\""),
        "must import encoding/binary for I16Be"
    );
}

#[test]
fn test_upper_page_offsets() {
    let code = emit_page(&upper_schema());
    // connector_type at flat 128, upper base=128, file_off=0
    assert!(
        code.contains("ConnectorType: int(buf[0])"),
        "upper page offset must subtract 128\n---\n{code}"
    );
    assert!(
        !code.contains("\"encoding/binary\""),
        "no binary needed for U8"
    );
}

#[test]
fn test_instanced_field_evaluates_offset_and_bit_offset_exprs() {
    let code = emit_page(&instanced_expr_schema());
    assert!(
            code.contains(
                "AdaptiveInputEqStoreTx: []int{int(buf[7] & 0x3), int((buf[7] >> 2) & 0x3), int((buf[7] >> 4) & 0x3), int((buf[7] >> 6) & 0x3), int(buf[8] & 0x3)}"
            ),
            "instanced offset/bit expressions decoded incorrectly\n---\n{code}"
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
    let code = emit_page(&schema);
    assert!(
        code.contains("FormatId: int(binary.BigEndian.Uint16(buf[0:2]))"),
        "2-byte enum must decode as big-endian u16\n---\n{code}"
    );
    assert!(
        !code.contains("FormatId: int(buf[0])"),
        "2-byte enum must not truncate to the first byte\n---\n{code}"
    );
    assert!(
        code.contains("\"encoding/binary\""),
        "2-byte enum decode requires the encoding/binary import\n---\n{code}"
    );
}
