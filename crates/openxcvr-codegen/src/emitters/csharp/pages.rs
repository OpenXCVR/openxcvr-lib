use crate::expr::compute;
use crate::ir::naming::{CSharpNaming, NamingPolicy};
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

/// C# scalar type. `long` for values exceeding signed 32-bit (u32be, u64le, u64be,
/// 4/8-byte bitmaps); `int` otherwise.
fn cs_type(f: &PageField) -> Option<&'static str> {
    match &f.ty {
        FieldType::Bool => Some("bool"),
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
        | FieldType::I32Be
        | FieldType::F16Be
        | FieldType::X16Be => Some("int"),
        FieldType::U32Be | FieldType::U64Le | FieldType::U64Be => Some("long"),
        FieldType::Bitmap => Some(match f.size_bytes.unwrap_or(1) {
            n if f.bit_offset.is_none() && f.bit_width.is_none() && n >= 4 => "long",
            _ => "int",
        }),
        FieldType::Ascii => Some("string"),
        FieldType::Bytes | FieldType::Composite => Some("byte[]"),
    }
}

fn bp(method: &str, o: usize, len: usize) -> String {
    format!("System.Buffers.Binary.BinaryPrimitives.{method}(buf.AsSpan({o}, {len}))")
}

fn decode_scalar_cs(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    let bw = bit_width(f);

    match &f.ty {
        FieldType::Bool => format!("((buf[{o}] >> {bo}) & 0x1) != 0"),
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
        FieldType::U8 => format!("(int)buf[{o}]"),
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 / SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; read the full width (mirrors the
        // Rust emitter's decode_at enum branch).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("(int){}", bp("ReadUInt16BigEndian", o, 2))
        }
        FieldType::EnumRef => format!("(int)buf[{o}]"),
        FieldType::I8 => format!("(int)(sbyte)buf[{o}]"),
        FieldType::U16Be => format!("(int){}", bp("ReadUInt16BigEndian", o, 2)),
        FieldType::I16Be => format!("(int){}", bp("ReadInt16BigEndian", o, 2)),
        FieldType::U16Le => format!("(int){}", bp("ReadUInt16LittleEndian", o, 2)),
        FieldType::U32Be => format!("(long){}", bp("ReadUInt32BigEndian", o, 4)),
        FieldType::I32Be => bp("ReadInt32BigEndian", o, 4),
        FieldType::U64Le => format!("(long){}", bp("ReadUInt64LittleEndian", o, 8)),
        FieldType::U64Be => format!("(long){}", bp("ReadUInt64BigEndian", o, 8)),
        FieldType::F16Be | FieldType::X16Be => format!("(int){}", bp("ReadUInt16BigEndian", o, 2)),
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
                    1 => format!("(int)buf[{o}]"),
                    2 => format!("(int){}", bp("ReadUInt16BigEndian", o, 2)),
                    4 => format!("(long){}", bp("ReadUInt32BigEndian", o, 4)),
                    _ => format!("(long){}", bp("ReadUInt64BigEndian", o, 8)),
                }
            }
        }
        FieldType::Ascii => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            format!("System.Text.Encoding.ASCII.GetString(buf, {o}, {size}).TrimEnd(' ')")
        }
        FieldType::Bytes | FieldType::Composite => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            format!("buf[{o}..{}]", o + size)
        }
    }
}

struct FieldEmit {
    ty: String,
    name: String,
    init: String,
}

fn array_type(elem: &str) -> String {
    if elem == "byte[]" {
        "byte[][]".to_string()
    } else {
        format!("{elem}[]")
    }
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
    let ty = cs_type(f)?.to_string();
    let file_off = f.offset.unwrap() as i64 - base;
    if file_off < 0 {
        return None;
    }
    Some(FieldEmit {
        ty,
        name: CSharpNaming.field_name(&f.name),
        init: decode_scalar_cs(f, file_off),
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
    let elem = cs_type(f)?;
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
        elems.push(decode_scalar_cs(&element, file_off));
    }
    Some(FieldEmit {
        ty: array_type(elem),
        name: CSharpNaming.field_name(&f.name),
        init: format!("new {} {{ {} }}", array_type(elem), elems.join(", ")),
    })
}

fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

/// C# encode body — inverse of `decode_scalar_cs` — into `byte[] buf`.
fn encode_scalar_cs(f: &PageField, offset: i64) -> String {
    let o = offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    let bp = "System.Buffers.Binary.BinaryPrimitives";
    match &f.ty {
        FieldType::Bool => {
            format!("buf[{o}] = (byte)((buf[{o}] & ~(1 << {bo})) | ((value ? 1 : 0) << {bo}));")
        }
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bit_width(f)) - 1;
            format!(
                "buf[{o}] = (byte)((buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo}));"
            )
        }
        FieldType::U8 | FieldType::I8 => format!("buf[{o}] = (byte)value;"),
        // Two-byte enum_ref values encode big-endian at the field width —
        // the exact inverse of the decode branch.
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("{bp}.WriteUInt16BigEndian(buf.AsSpan({o}, 2), (ushort)value);")
        }
        FieldType::EnumRef => format!("buf[{o}] = (byte)value;"),
        FieldType::U16Be => {
            format!("{bp}.WriteUInt16BigEndian(buf.AsSpan({o}, 2), (ushort)value);")
        }
        FieldType::I16Be => format!("{bp}.WriteInt16BigEndian(buf.AsSpan({o}, 2), (short)value);"),
        FieldType::U16Le => {
            format!("{bp}.WriteUInt16LittleEndian(buf.AsSpan({o}, 2), (ushort)value);")
        }
        FieldType::U32Be => format!("{bp}.WriteUInt32BigEndian(buf.AsSpan({o}, 4), (uint)value);"),
        FieldType::I32Be => format!("{bp}.WriteInt32BigEndian(buf.AsSpan({o}, 4), value);"),
        FieldType::U64Le => {
            format!("{bp}.WriteUInt64LittleEndian(buf.AsSpan({o}, 8), (ulong)value);")
        }
        FieldType::U64Be => {
            format!("{bp}.WriteUInt64BigEndian(buf.AsSpan({o}, 8), (ulong)value);")
        }
        FieldType::F16Be | FieldType::X16Be => {
            format!("{bp}.WriteUInt16BigEndian(buf.AsSpan({o}, 2), (ushort)value);")
        }
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bit_width(f)) - 1;
                format!(
                    "buf[{o}] = (byte)((buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo}));"
                )
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("buf[{o}] = (byte)value;"),
                    2 => format!("{bp}.WriteUInt16BigEndian(buf.AsSpan({o}, 2), (ushort)value);"),
                    4 => format!("{bp}.WriteUInt32BigEndian(buf.AsSpan({o}, 4), (uint)value);"),
                    _ => format!("{bp}.WriteUInt64BigEndian(buf.AsSpan({o}, 8), (ulong)value);"),
                }
            }
        }
        FieldType::Ascii => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!(
                "var s = System.Text.Encoding.ASCII.GetBytes(value);\n            for (int i = 0; i < {n}; i++) buf[{o} + i] = i < s.Length ? s[i] : (byte)' ';"
            )
        }
        FieldType::Bytes | FieldType::Composite => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("for (int i = 0; i < {n}; i++) buf[{o} + i] = i < value.Length ? value[i] : (byte)0;")
        }
    }
}

/// Emit a static `Set<Field>(byte[], value)` for a writable scalar field.
/// Instanced C# setters are deferred (follow-up).
fn emit_setter_cs(f: &PageField, base: i64) -> Option<String> {
    if !is_writable(f) {
        return None;
    }
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let ty = cs_type(f)?;
    let method = CSharpNaming.field_name(&f.name);

    // Scalar.
    if f.offset_expr.is_none() && f.instances.is_none() {
        let offset = f.offset? as i64 - base;
        if offset < 0 {
            return None;
        }
        let body = encode_scalar_cs(f, offset);
        return Some(format!(
            "\n        public static void Set{method}(byte[] buf, {ty} value)\n        {{\n            {body}\n        }}\n"
        ));
    }

    // Instanced: switch on index.
    if let Some(inst) = &f.instances {
        let mut arms = String::new();
        for (n, i) in (inst.start..=inst.end).enumerate() {
            let offset = resolve_offset(f, inst, i)? - base;
            let bit_offset = resolve_bit_offset(f, inst, i)?;
            if offset < 0 || bit_offset < 0 {
                return None;
            }
            let mut element = f.clone();
            element.bit_offset = if uses_bit_offset(f) {
                Some(bit_offset as u32)
            } else {
                None
            };
            element.bit_offset_expr = None;
            let body: String = encode_scalar_cs(&element, offset)
                .lines()
                .map(|l| format!("                    {}", l.trim_start()))
                .collect::<Vec<_>>()
                .join("\n");
            arms.push_str(&format!(
                "                case {n}:\n{body}\n                    break;\n"
            ));
        }
        return Some(format!(
            "\n        public static void Set{method}(byte[] buf, int index, {ty} value)\n        {{\n            switch (index)\n            {{\n{arms}            }}\n        }}\n"
        ));
    }
    None
}

/// Emit a static `RecomputeChecksums(byte[] buf)` for every page that has
/// ≥1 sum8 checksum field. Mirrors the Rust emitter's block.
fn emit_recompute_checksums_cs(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let size = schema.size_bytes.unwrap_or(128) as i64;

    // Collect (chk_idx, cov_start, cov_end) triples for every sum8 field.
    let chksum_fields: Vec<(i64, i64, i64)> = schema
        .value_fields()
        .filter_map(|f| {
            let flat_offset = f.offset? as i64;
            let (flat_start, flat_end) = f.checksum_region()?;
            let chk_idx = flat_offset - base;
            let cov_start = flat_start as i64 - base;
            let cov_end = flat_end as i64 - base;
            assert!(
                chk_idx >= 0
                    && chk_idx < size
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < size,
                "{}: checksum at flat {} covering flat [{},{}] \
                 out of bounds for page_base {}, size {}",
                schema.id,
                flat_offset,
                flat_start,
                flat_end,
                base,
                size
            );
            Some((chk_idx, cov_start, cov_end))
        })
        .collect();

    if chksum_fields.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("\n        public static void RecomputeChecksums(byte[] buf)\n        {\n");
    for (chk_idx, cov_start, cov_end) in &chksum_fields {
        out.push_str(&format!(
            "            {{ byte sum = 0; for (int i = {cov_start}; i <= {cov_end}; i++) sum += buf[i]; buf[{chk_idx}] = sum; }}\n"
        ));
    }
    out.push_str("        }\n");
    out
}

pub fn emit_page(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let type_name = CSharpNaming.type_name(&schema.id);
    let min_size = schema.size_bytes.unwrap_or(128);

    let mut emits: Vec<FieldEmit> = Vec::new();
    for f in schema.value_fields() {
        let emit_opt = if let Some(instances) = &f.instances {
            emit_instanced_field(f, instances, base)
        } else {
            emit_scalar_field(f, base)
        };
        match emit_opt {
            Some(e) => emits.push(e),
            None => panic!("{}: cannot emit field {}", schema.id, f.name),
        }
    }

    let params: String = emits
        .iter()
        .map(|e| format!("        {} {}", e.ty, e.name))
        .collect::<Vec<_>>()
        .join(",\n");
    let args: String = emits
        .iter()
        .map(|e| format!("                {}", e.init))
        .collect::<Vec<_>>()
        .join(",\n");

    let setters: String = schema
        .value_fields()
        .filter_map(|f| emit_setter_cs(f, base))
        .collect();

    let recompute = emit_recompute_checksums_cs(schema);

    format!(
        "// Generated by openxcvr-codegen. DO NOT EDIT.\n\
         using System;\n\n\
         namespace OpenXcvr.Generated\n{{\n\
         \x20\x20\x20\x20public sealed record {type_name}(\n{params}\n    )\n    {{\n\
         \x20\x20\x20\x20\x20\x20\x20\x20public static {type_name} Decode(byte[] buf)\n        {{\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if (buf.Length < {min_size})\n            {{\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20throw new ArgumentException($\"expected >={min_size} bytes, got {{buf.Length}}\");\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20}}\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return new {type_name}(\n{args}\n            );\n\
         \x20\x20\x20\x20\x20\x20\x20\x20}}\n{setters}{recompute}    }}\n}}\n"
    )
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
    fn emits_record_and_decoder() {
        let out = emit_page(&page(vec![
            field("cmis_revision", 1, FieldType::U8, Some(1)),
            field("temp_mon_value", 14, FieldType::I16Be, Some(2)),
        ]));
        assert!(
            out.contains("public sealed record CmisLowerMemory("),
            "{out}"
        );
        assert!(out.contains("int CmisRevision"), "{out}");
        assert!(
            out.contains("public static CmisLowerMemory Decode(byte[] buf)"),
            "{out}"
        );
        assert!(
            out.contains("ReadInt16BigEndian(buf.AsSpan(14, 2))"),
            "{out}"
        );
        assert!(out.contains("(int)buf[1]"), "{out}");
    }

    #[test]
    fn two_byte_enum_ref_decodes_full_width() {
        // SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13: "Format ID (Bytes
        // 128-129)", 16-bit big-endian value (CA1Bh / 100Bh). A one-byte read
        // truncates it.
        let f = field("format_id", 0, FieldType::EnumRef, Some(2));
        let out = emit_page(&page(vec![f]));
        assert!(
            out.contains(
                "(int)System.Buffers.Binary.BinaryPrimitives.ReadUInt16BigEndian(buf.AsSpan(0, 2))"
            ),
            "2-byte enum must decode as big-endian u16\n---\n{out}"
        );
    }
}
