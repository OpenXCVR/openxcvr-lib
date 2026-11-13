use crate::expr::compute;
use crate::ir::naming::{NamingPolicy, TypeScriptNaming};
use crate::ir::pages::{page_base, FieldType, InstancesDesc, PageField, PageSchema};

fn bit_width(f: &PageField) -> u32 {
    f.bit_width.unwrap_or(match f.ty {
        FieldType::Bool => 1,
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        _ => 8,
    })
}

fn ts_type(f: &PageField) -> Option<&'static str> {
    match &f.ty {
        FieldType::Bool => Some("boolean"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7
        | FieldType::U8
        | FieldType::I8
        | FieldType::EnumRef
        | FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::U32Be
        | FieldType::I32Be
        | FieldType::F16Be
        | FieldType::X16Be
        | FieldType::Bitmap => Some("number"),
        FieldType::U64Le | FieldType::U64Be => Some("bigint"),
        FieldType::Ascii => Some("string"),
        FieldType::Bytes | FieldType::Composite => Some("Uint8Array"),
    }
}

/// Whether decoding this field needs the `DataView` (multi-byte primitive).
fn needs_view(f: &PageField) -> bool {
    matches!(
        f.ty,
        FieldType::U16Be
            | FieldType::I16Be
            | FieldType::U16Le
            | FieldType::U32Be
            | FieldType::I32Be
            | FieldType::U64Le
            | FieldType::U64Be
            | FieldType::F16Be
            | FieldType::X16Be
    ) || (matches!(f.ty, FieldType::Bitmap)
        && f.bit_offset.is_none()
        && f.bit_width.is_none()
        && f.size_bytes.unwrap_or(1) > 1)
        // Two-byte enum_ref fields decode through the DataView (16-bit BE
        // enum value; see decode_scalar_ts).
        || (f.ty == FieldType::EnumRef && f.size_bytes == Some(2))
}

fn be_int_expr_ts(offset: usize, size: usize) -> String {
    let terms: Vec<String> = (0..size)
        .map(|i| {
            let shift = (size - i - 1) * 8;
            if shift == 0 {
                format!("buf[{}]", offset + i)
            } else {
                format!("(buf[{}] << {})", offset + i, shift)
            }
        })
        .collect();
    terms.join(" | ")
}

fn decode_scalar_ts(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    let bw = bit_width(f);

    match &f.ty {
        FieldType::Bool => format!("((buf[{o}] >> {bo}) & 0x1) !== 0"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bw) - 1;
            if bo == 0 {
                format!("(buf[{o}] & 0x{mask:X})")
            } else {
                format!("((buf[{o}] >> {bo}) & 0x{mask:X})")
            }
        }
        FieldType::U8 => format!("buf[{o}]"),
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 / SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; read the full width (mirrors the
        // Rust emitter's decode_at enum branch).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("dv.getUint16({o}, false)")
        }
        FieldType::EnumRef => format!("buf[{o}]"),
        FieldType::I8 => format!("((buf[{o}] << 24) >> 24)"),
        FieldType::U16Be => format!("dv.getUint16({o}, false)"),
        FieldType::I16Be => format!("dv.getInt16({o}, false)"),
        FieldType::U16Le => format!("dv.getUint16({o}, true)"),
        FieldType::U32Be => format!("dv.getUint32({o}, false)"),
        FieldType::I32Be => format!("dv.getInt32({o}, false)"),
        FieldType::U64Le => format!("dv.getBigUint64({o}, true)"),
        FieldType::U64Be => format!("dv.getBigUint64({o}, false)"),
        FieldType::F16Be | FieldType::X16Be => format!("dv.getUint16({o}, false)"),
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bw) - 1;
                if bo == 0 {
                    format!("(buf[{o}] & 0x{mask:X})")
                } else {
                    format!("((buf[{o}] >> {bo}) & 0x{mask:X})")
                }
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("buf[{o}]"),
                    2 => format!("dv.getUint16({o}, false)"),
                    4 => format!("dv.getUint32({o}, false)"),
                    8 => format!("Number(dv.getBigUint64({o}, false))"),
                    n => be_int_expr_ts(o, n as usize),
                }
            }
        }
        FieldType::Ascii => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            let end = o + size;
            format!("trimTrailingSpaces(new TextDecoder().decode(buf.subarray({o}, {end})))")
        }
        FieldType::Bytes | FieldType::Composite => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            let end = o + size;
            format!("buf.slice({o}, {end})")
        }
    }
}

struct FieldEmit {
    decl: String,
    init: String,
    needs_view: bool,
    needs_trim: bool,
}

fn emit_scalar_field(f: &PageField, base: i64) -> Option<FieldEmit> {
    if f.offset.is_none() || f.offset_expr.is_some() || f.instances.is_some() {
        return None;
    }
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let ts = ts_type(f)?;
    let file_off = f.offset.unwrap() as i64 - base;
    if file_off < 0 {
        return None;
    }
    let name = TypeScriptNaming.field_name(&f.name);
    Some(FieldEmit {
        decl: format!("  {name}: {ts};\n"),
        init: format!("    {name}: {},\n", decode_scalar_ts(f, file_off)),
        needs_view: needs_view(f),
        needs_trim: matches!(f.ty, FieldType::Ascii),
    })
}

fn resolve_offset(f: &PageField, inst: &InstancesDesc, i: i64) -> Option<i64> {
    if let Some(expr) = &f.offset_expr {
        if expr.is_empty() {
            return f.offset.map(|o| o as i64);
        }
        compute(expr, &inst.index, i).ok()
    } else {
        f.offset.map(|o| o as i64)
    }
}

fn resolve_bit_offset(f: &PageField, inst: &InstancesDesc, i: i64) -> Option<i64> {
    if let Some(expr) = &f.bit_offset_expr {
        if expr.is_empty() {
            return Some(f.bit_offset.unwrap_or(0) as i64);
        }
        compute(expr, &inst.index, i).ok()
    } else {
        Some(f.bit_offset.unwrap_or(0) as i64)
    }
}

fn uses_bit_offset(f: &PageField) -> bool {
    matches!(
        f.ty,
        FieldType::Bool
            | FieldType::U2
            | FieldType::U3
            | FieldType::U4
            | FieldType::U5
            | FieldType::U6
            | FieldType::U7
    ) || f.bit_offset.is_some()
        || f.bit_offset_expr.is_some()
        || f.bit_width.is_some()
}

fn emit_instanced_field(f: &PageField, inst: &InstancesDesc, base: i64) -> Option<FieldEmit> {
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let ts_elem = ts_type(f)?;
    let mut elems: Vec<String> = Vec::new();
    for i in inst.start..=inst.end {
        let flat = resolve_offset(f, inst, i)?;
        let bit_offset = resolve_bit_offset(f, inst, i)?;
        if bit_offset < 0 {
            return None;
        }
        let file_off = flat - base;
        if file_off < 0 {
            return None;
        }
        let mut element = f.clone();
        element.bit_offset = if uses_bit_offset(f) {
            Some(bit_offset as u32)
        } else {
            None
        };
        element.bit_offset_expr = None;
        elems.push(decode_scalar_ts(&element, file_off));
    }
    let name = TypeScriptNaming.field_name(&f.name);
    Some(FieldEmit {
        decl: format!("  {name}: {ts_elem}[];\n"),
        init: format!("    {name}: [{}],\n", elems.join(", ")),
        needs_view: needs_view(f),
        needs_trim: matches!(f.ty, FieldType::Ascii),
    })
}

fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

/// Encode statement — inverse of `decode_scalar_ts`. Uses `dv` for multi-byte
/// primitives; sub-byte fields use read-modify-write.
fn encode_scalar_ts(f: &PageField, offset: i64) -> String {
    let o = offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    let bw = bit_width(f);
    match &f.ty {
        FieldType::Bool => {
            format!("buf[{o}] = (buf[{o}] & ~(1 << {bo})) | ((value ? 1 : 0) << {bo});")
        }
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bw) - 1;
            format!(
                "buf[{o}] = (buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo});"
            )
        }
        FieldType::U8 | FieldType::I8 => format!("buf[{o}] = value & 0xFF;"),
        // Two-byte enum_ref values encode big-endian at the field width —
        // the exact inverse of the decode branch.
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("dv.setUint16({o}, value, false);")
        }
        FieldType::EnumRef => format!("buf[{o}] = value & 0xFF;"),
        FieldType::U16Be => format!("dv.setUint16({o}, value, false);"),
        FieldType::I16Be => format!("dv.setInt16({o}, value, false);"),
        FieldType::U16Le => format!("dv.setUint16({o}, value, true);"),
        FieldType::U32Be => format!("dv.setUint32({o}, value, false);"),
        FieldType::I32Be => format!("dv.setInt32({o}, value, false);"),
        FieldType::U64Le => format!("dv.setBigUint64({o}, value, true);"),
        FieldType::U64Be => format!("dv.setBigUint64({o}, value, false);"),
        FieldType::F16Be | FieldType::X16Be => format!("dv.setUint16({o}, value, false);"),
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bw) - 1;
                format!("buf[{o}] = (buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo});")
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("buf[{o}] = value & 0xFF;"),
                    2 => format!("dv.setUint16({o}, value, false);"),
                    4 => format!("dv.setUint32({o}, value, false);"),
                    _ => format!("dv.setBigUint64({o}, BigInt(value), false);"),
                }
            }
        }
        FieldType::Ascii => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("for (let i = 0; i < {n}; i++) buf[{o} + i] = i < value.length ? value.charCodeAt(i) : 0x20;")
        }
        FieldType::Bytes | FieldType::Composite => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("buf.set(value.subarray(0, {n}), {o});")
        }
    }
}

/// Emit a page-prefixed `set<Type><Field>` for a writable field (barrel
/// re-exports flatten names, so the page prefix avoids collisions).
fn emit_setter_ts(f: &PageField, base: i64, type_name: &str) -> Option<String> {
    if !is_writable(f) {
        return None;
    }
    let ts = ts_type(f)?;
    let dv = if needs_view(f) {
        "  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);\n"
    } else {
        ""
    };

    if f.offset_expr.is_none() && f.instances.is_none() {
        if matches!(
            f.ty,
            FieldType::Ascii | FieldType::Bytes | FieldType::Composite
        ) && f.size_bytes.is_none()
        {
            return None;
        }
        let o = f.offset? as i64 - base;
        if o < 0 {
            return None;
        }
        let suffix = TypeScriptNaming.enum_variant(&f.name);
        let body = encode_scalar_ts(f, o);
        return Some(format!(
            "\nexport function set{type_name}{suffix}(buf: Uint8Array, value: {ts}): void {{\n{dv}  {body}\n}}\n"
        ));
    }

    if let Some(inst) = &f.instances {
        let suffix = TypeScriptNaming.enum_variant(&f.name);
        let mut arms = String::new();
        for (n, i) in (inst.start..=inst.end).enumerate() {
            let o = resolve_offset(f, inst, i)? - base;
            let bit_offset = resolve_bit_offset(f, inst, i)?;
            if o < 0 || bit_offset < 0 {
                return None;
            }
            let mut element = f.clone();
            element.bit_offset = if uses_bit_offset(f) {
                Some(bit_offset as u32)
            } else {
                None
            };
            element.bit_offset_expr = None;
            arms.push_str(&format!(
                "    case {n}: {} break;\n",
                encode_scalar_ts(&element, o)
            ));
        }
        return Some(format!(
            "\nexport function set{type_name}{suffix}(buf: Uint8Array, index: number, value: {ts}): void {{\n{dv}  switch (index) {{\n{arms}  }}\n}}\n"
        ));
    }
    None
}

pub fn emit_page(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let type_name = TypeScriptNaming.type_name(&schema.id);
    let min_size = schema.size_bytes.unwrap_or(128);

    let mut decls = String::new();
    let mut inits = String::new();
    let mut needs_view = false;
    let mut needs_trim = false;

    for f in schema.value_fields() {
        let emit_opt = if let Some(instances) = &f.instances {
            emit_instanced_field(f, instances, base)
        } else {
            emit_scalar_field(f, base)
        };
        match emit_opt {
            Some(emit) => {
                needs_view |= emit.needs_view;
                needs_trim |= emit.needs_trim;
                decls.push_str(&emit.decl);
                inits.push_str(&emit.init);
            }
            None => panic!("{}: cannot emit field {}", schema.id, f.name),
        }
    }

    let view_decl = if needs_view {
        "  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);\n"
    } else {
        ""
    };

    let helper = if needs_trim {
        "function trimTrailingSpaces(s: string): string {\n  let end = s.length;\n  while (end > 0 && s.charCodeAt(end - 1) === 0x20) end--;\n  return s.slice(0, end);\n}\n\n"
    } else {
        ""
    };

    let mut out = format!(
        "// Generated by openxcvr-codegen. DO NOT EDIT.\n\n\
         {helper}\
         export interface {type_name} {{\n{decls}}}\n\n\
         export function decode{type_name}(buf: Uint8Array): {type_name} {{\n\
         \x20 if (buf.length < {min_size}) {{\n\
         \x20\x20\x20 throw new Error(`expected >={min_size} bytes, got ${{buf.length}}`);\n\
         \x20 }}\n{view_decl}  return {{\n{inits}  }};\n}}\n"
    );
    for f in schema.value_fields() {
        if let Some(s) = emit_setter_ts(f, base, &type_name) {
            out.push_str(&s);
        }
    }

    // Emit recomputeChecksums if the schema has any sum8 checksum fields.
    let chksum_fields: Vec<_> = schema
        .value_fields()
        .filter_map(|f| {
            let offset = f.offset?;
            let (flat_start, flat_end) = f.checksum_region()?;
            Some((offset, flat_start, flat_end))
        })
        .collect();
    if !chksum_fields.is_empty() {
        let size = schema.size_bytes.unwrap_or(128) as i64;
        out.push_str(&format!(
            "\nexport function recomputeChecksums{type_name}(buf: Uint8Array): void {{\n"
        ));
        for (flat_offset, flat_start, flat_end) in &chksum_fields {
            let chk_idx = *flat_offset as i64 - base;
            let cov_start = *flat_start as i64 - base;
            let cov_end = *flat_end as i64 - base;
            assert!(
                chk_idx >= 0
                    && chk_idx < size
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < size,
                "{}: checksum at flat {flat_offset} covering flat [{flat_start},{flat_end}] \
                 out of bounds for page_base {base}, size {size}",
                schema.id
            );
            out.push_str(&format!(
                "  let sum{chk_idx} = 0;\n  for (let i = {cov_start}; i <= {cov_end}; i++) sum{chk_idx} = (sum{chk_idx} + buf[i]) & 0xFF;\n  buf[{chk_idx}] = sum{chk_idx};\n"
            ));
        }
        out.push_str("}\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::pages::{PageField, PageSchema};

    fn field(name: &str, offset: u32, ty: FieldType, size: Option<u32>) -> PageField {
        PageField {
            name: name.to_string(),
            display_name: None,
            offset: Some(offset),
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: size,
            ty,
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

    fn page(fields: Vec<PageField>) -> PageSchema {
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
            fields,
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        }
    }

    #[test]
    fn emits_interface_and_decoder() {
        let out = emit_page(&page(vec![
            field("cmis_revision", 1, FieldType::U8, Some(1)),
            field("temp_mon_value", 14, FieldType::I16Be, Some(2)),
            field("vendor_name", 129, FieldType::Ascii, Some(16)),
        ]));
        assert!(out.contains("export interface CmisLowerMemory {"), "{out}");
        assert!(out.contains("cmisRevision: number;"), "{out}");
        assert!(out.contains("tempMonValue: number;"), "{out}");
        assert!(
            out.contains("export function decodeCmisLowerMemory(buf: Uint8Array)"),
            "{out}"
        );
        assert!(out.contains("const dv = new DataView("), "{out}");
        assert!(out.contains("dv.getInt16(14, false)"), "{out}");
        assert!(out.contains("cmisRevision: buf[1],"), "{out}");
    }

    #[test]
    fn omits_dataview_when_only_byte_access() {
        let out = emit_page(&page(vec![field("a", 0, FieldType::U8, Some(1))]));
        assert!(
            !out.contains("new DataView("),
            "dv should be omitted\n{out}"
        );
    }

    #[test]
    fn test_ascii_trim_trailing_spaces() {
        let page_with_ascii = page(vec![
            field("field1", 0, FieldType::Ascii, Some(16)),
            field("field2", 16, FieldType::Ascii, Some(16)),
        ]);
        let page_no_ascii = page(vec![field("a", 0, FieldType::U8, Some(1))]);

        let out_ascii = emit_page(&page_with_ascii);
        assert!(
            !out_ascii.contains(".replace(/"),
            "should not emit .replace(/"
        );
        assert!(
            out_ascii.contains("function trimTrailingSpaces("),
            "should contain trimTrailingSpaces helper"
        );
        assert!(
            !out_ascii.contains("export function trimTrailingSpaces("),
            "trimTrailingSpaces helper must not be exported"
        );
        let helper_count = out_ascii.matches("function trimTrailingSpaces(").count();
        assert_eq!(helper_count, 1, "helper should be emitted exactly once");
        let call_count = out_ascii.matches("trimTrailingSpaces(").count();
        // 1 definition + 2 calls = 3 occurrences
        assert_eq!(
            call_count, 3,
            "helper should be defined once and called twice"
        );

        let out_no_ascii = emit_page(&page_no_ascii);
        assert!(
            !out_no_ascii.contains("trimTrailingSpaces"),
            "page without ascii should not contain trimTrailingSpaces"
        );
    }

    #[test]
    fn test_instanced_ascii_trim_helper() {
        let mut ascii_field = field("instanced_ascii", 0, FieldType::Ascii, Some(4));
        ascii_field.instances = Some(InstancesDesc {
            index: "lane".into(),
            start: 1,
            end: 3,
        });
        ascii_field.offset_expr = Some("0 + (lane - 1) * 4".into());

        let u8_field = field("status", 12, FieldType::U8, Some(1));
        let page_schema = page(vec![ascii_field, u8_field]);

        let output = emit_page(&page_schema);
        let trim_helper_def = "function trimTrailingSpaces(";
        assert_eq!(output.matches(trim_helper_def).count(), 1);
        assert!(!output.contains("export function trimTrailingSpaces("));

        let trim_helper_call = "trimTrailingSpaces(new TextDecoder()";
        assert_eq!(output.matches(trim_helper_call).count(), 3);
    }

    #[test]
    fn two_byte_enum_ref_decodes_full_width() {
        // SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13: "Format ID (Bytes
        // 128-129)", 16-bit big-endian value (CA1Bh / 100Bh). A one-byte read
        // truncates it.
        let f = field("format_id", 0, FieldType::EnumRef, Some(2));
        let out = emit_page(&page(vec![f]));
        assert!(
            out.contains("formatId: dv.getUint16(0, false)"),
            "2-byte enum must decode as big-endian u16\n---\n{out}"
        );
        assert!(
            out.contains("const dv = new DataView("),
            "2-byte enum decode requires the DataView\n---\n{out}"
        );
    }
}
