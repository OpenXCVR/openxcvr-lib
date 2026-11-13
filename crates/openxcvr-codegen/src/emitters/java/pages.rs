use crate::expr::compute;
use crate::ir::naming::{JavaNaming, NamingPolicy};
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

/// Java scalar type. `java.math.BigInteger` for u64 (always non-negative); `long` for
/// values exceeding signed 32-bit (u32be, 4/8-byte bitmaps); `int` otherwise
/// (Java has no unsigned types).
fn java_type(f: &PageField) -> Option<&'static str> {
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
        | FieldType::I32Be
        | FieldType::F16Be
        | FieldType::X16Be => Some("int"),
        FieldType::U32Be => Some("long"),
        FieldType::U64Le | FieldType::U64Be => Some("java.math.BigInteger"),
        FieldType::Bitmap => Some(match f.size_bytes.unwrap_or(1) {
            n if f.bit_offset.is_none() && f.bit_width.is_none() && n >= 4 => "long",
            _ => "int",
        }),
        FieldType::Ascii => Some("String"),
        FieldType::Bytes | FieldType::Composite => Some("byte[]"),
    }
}

fn b(o: usize) -> String {
    format!("(buf[{o}] & 0xFF)")
}

fn be_uint(o: usize, size: usize, as_long: bool) -> String {
    let terms: Vec<String> = (0..size)
        .map(|i| {
            let shift = (size - i - 1) * 8;
            let byte = if as_long && i == 0 {
                format!("(long) {}", b(o + i))
            } else {
                b(o + i)
            };
            if shift == 0 {
                byte
            } else {
                format!("({byte} << {shift})")
            }
        })
        .collect();
    format!("({})", terms.join(" | "))
}

fn decode_scalar_java(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    let bw = bit_width(f);

    match &f.ty {
        FieldType::Bool => format!("(({} >> {bo}) & 0x1) != 0", b(o)),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bw) - 1;
            if bo == 0 {
                format!("({} & 0x{mask:X})", b(o))
            } else {
                format!("(({} >> {bo}) & 0x{mask:X})", b(o))
            }
        }
        FieldType::U8 => b(o),
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 / SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; read the full width (mirrors the
        // Rust emitter's decode_at enum branch).
        FieldType::EnumRef if f.size_bytes == Some(2) => be_uint(o, 2, false),
        FieldType::EnumRef => b(o),
        FieldType::I8 => format!("(int) buf[{o}]"),
        FieldType::U16Be => be_uint(o, 2, false),
        FieldType::I16Be => format!("(int) (short) {}", be_uint(o, 2, false)),
        FieldType::U16Le => format!("({} | ({} << 8))", b(o), b(o + 1)),
        FieldType::U32Be => be_uint(o, 4, true),
        FieldType::I32Be => format!(
            "(({} << 24) | ({} << 16) | ({} << 8) | {})",
            b(o),
            b(o + 1),
            b(o + 2),
            b(o + 3)
        ),
        // Signum 1 plus a big-endian magnitude: the value is always >= 0,
        // including magnitudes whose high bit is set (which a Java long would
        // treat as negative).
        // Raw wire bytes: BigInteger(signum, magnitude) reads the array as an
        // unsigned big-endian magnitude, so a high bit stays non-negative.
        // `(buf[i] & 0xFF)` is an int and is not a legal byte[] element.
        FieldType::U64Le => format!(
            "new java.math.BigInteger(1, new byte[] {{ buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}] }})",
            o + 7,
            o + 6,
            o + 5,
            o + 4,
            o + 3,
            o + 2,
            o + 1,
            o
        ),
        FieldType::U64Be => format!(
            "new java.math.BigInteger(1, new byte[] {{ buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}] }})",
            o,
            o + 1,
            o + 2,
            o + 3,
            o + 4,
            o + 5,
            o + 6,
            o + 7
        ),
        FieldType::F16Be | FieldType::X16Be => be_uint(o, 2, false),
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bw) - 1;
                if bo == 0 {
                    format!("({} & 0x{mask:X})", b(o))
                } else {
                    format!("(({} >> {bo}) & 0x{mask:X})", b(o))
                }
            } else {
                let n = f.size_bytes.unwrap_or(1) as usize;
                be_uint(o, n, n >= 4)
            }
        }
        FieldType::Ascii => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            format!(
                "new String(buf, {o}, {size}, java.nio.charset.StandardCharsets.US_ASCII).replaceAll(\"[ ]+$\", \"\")"
            )
        }
        FieldType::Bytes | FieldType::Composite => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            format!("java.util.Arrays.copyOfRange(buf, {o}, {})", o + size)
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
    let ty = java_type(f)?.to_string();
    let file_off = f.offset.unwrap() as i64 - base;
    if file_off < 0 {
        return None;
    }
    Some(FieldEmit {
        ty,
        name: JavaNaming.field_name(&f.name),
        init: decode_scalar_java(f, file_off),
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
    let elem = java_type(f)?;
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
        elems.push(decode_scalar_java(&element, file_off));
    }
    Some(FieldEmit {
        ty: array_type(elem),
        name: JavaNaming.field_name(&f.name),
        init: format!("new {} {{ {} }}", array_type(elem), elems.join(", ")),
    })
}

fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

fn put_java(o: usize, size: usize, le: bool) -> String {
    (0..size)
        .map(|i| {
            let shift = if le { i * 8 } else { (size - 1 - i) * 8 };
            if shift == 0 {
                format!("buf[{}] = (byte) value;", o + i)
            } else {
                format!("buf[{}] = (byte) (value >> {shift});", o + i)
            }
        })
        .collect::<Vec<_>>()
        .join("\n        ")
}

/// Write a BigInteger in [0, 2^64-1] as 8 bytes. Out of range throws
/// IllegalArgumentException. `toByteArray` is big-endian two's-complement and
/// may carry a leading zero when the high bit is set; the loop keeps the low 8.
fn encode_u64_java(o: usize, le: bool) -> String {
    let index = if le {
        format!("{o} + (7 - u64i)")
    } else {
        format!("{o} + u64i")
    };
    [
        "if (value == null || value.signum() < 0 || value.bitLength() > 64) {",
        "throw new IllegalArgumentException(\"u64 out of range: \" + value);",
        "}",
        "byte[] u64mag = value.toByteArray();",
        "for (int u64i = 0; u64i < 8; u64i++) {",
        "int u64mi = u64mag.length - 8 + u64i;",
        &format!("buf[{index}] = u64mi >= 0 ? u64mag[u64mi] : (byte) 0;"),
        "}",
    ]
    .join("\n        ")
}

/// Java encode body — inverse of `decode_scalar_java` — into `byte[] buf`.
fn encode_scalar_java(f: &PageField, offset: i64) -> String {
    let o = offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    match &f.ty {
        FieldType::Bool => {
            format!("buf[{o}] = (byte) ((buf[{o}] & ~(1 << {bo})) | ((value ? 1 : 0) << {bo}));")
        }
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bit_width(f)) - 1;
            format!(
                "buf[{o}] = (byte) ((buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo}));"
            )
        }
        FieldType::U8 | FieldType::I8 => format!("buf[{o}] = (byte) value;"),
        // Two-byte enum_ref values encode big-endian at the field width —
        // the exact inverse of the decode branch.
        FieldType::EnumRef if f.size_bytes == Some(2) => put_java(o, 2, false),
        FieldType::EnumRef => format!("buf[{o}] = (byte) value;"),
        FieldType::U16Be | FieldType::I16Be | FieldType::F16Be | FieldType::X16Be => {
            put_java(o, 2, false)
        }
        FieldType::U16Le => put_java(o, 2, true),
        FieldType::U32Be | FieldType::I32Be => put_java(o, 4, false),
        FieldType::U64Le => encode_u64_java(o, true),
        FieldType::U64Be => encode_u64_java(o, false),
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bit_width(f)) - 1;
                format!(
                    "buf[{o}] = (byte) ((buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo}));"
                )
            } else {
                let n = f.size_bytes.unwrap_or(1) as usize;
                put_java(o, n, false)
            }
        }
        FieldType::Ascii => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!(
                "byte[] s = value.getBytes(java.nio.charset.StandardCharsets.US_ASCII);\n        for (int i = 0; i < {n}; i++) buf[{o} + i] = i < s.length ? s[i] : (byte) ' ';"
            )
        }
        FieldType::Bytes | FieldType::Composite => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("for (int i = 0; i < {n}; i++) buf[{o} + i] = i < value.length ? value[i] : 0;")
        }
    }
}

/// Emit a static `set<Field>(byte[], value)` for a writable scalar field.
/// Instanced Java setters are deferred (follow-up).
fn emit_setter_java(f: &PageField, base: i64) -> Option<String> {
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
    let ty = java_type(f)?;
    let method = crate::ir::naming::pascal_segments(&f.name);

    // Scalar.
    if f.offset_expr.is_none() && f.instances.is_none() {
        let offset = f.offset? as i64 - base;
        if offset < 0 {
            return None;
        }
        let body = encode_scalar_java(f, offset);
        return Some(format!(
            "\n    public static void set{method}(byte[] buf, {ty} value) {{\n        {body}\n    }}\n"
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
            let body: String = encode_scalar_java(&element, offset)
                .lines()
                .map(|l| format!("                {}", l.trim_start()))
                .collect::<Vec<_>>()
                .join("\n");
            arms.push_str(&format!(
                "            case {n}:\n{body}\n                break;\n"
            ));
        }
        return Some(format!(
            "\n    public static void set{method}(byte[] buf, int index, {ty} value) {{\n        switch (index) {{\n{arms}        }}\n    }}\n"
        ));
    }
    None
}

pub fn emit_page(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let type_name = JavaNaming.type_name(&schema.id);
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

    let decls: String = emits
        .iter()
        .map(|e| format!("    public final {} {};\n", e.ty, e.name))
        .collect();
    let params: String = emits
        .iter()
        .map(|e| format!("{} {}", e.ty, e.name))
        .collect::<Vec<_>>()
        .join(", ");
    let assigns: String = emits
        .iter()
        .map(|e| format!("        this.{n} = {n};\n", n = e.name))
        .collect();
    let args: String = emits
        .iter()
        .map(|e| format!("            {}", e.init))
        .collect::<Vec<_>>()
        .join(",\n");

    let setters: String = schema
        .value_fields()
        .filter_map(|f| emit_setter_java(f, base))
        .collect();

    // checksum recompute: emit recomputeChecksums() for every page that has
    // ≥1 sum8 checksum field.
    let chksum_fields: Vec<_> = schema
        .value_fields()
        .filter_map(|f| {
            let offset = f.offset?;
            let (flat_start, flat_end) = f.checksum_region()?;
            Some((offset, flat_start, flat_end))
        })
        .collect();
    let checksum = if !chksum_fields.is_empty() {
        let mut lines = String::new();
        for (flat_offset, flat_start, flat_end) in &chksum_fields {
            let chk_idx = *flat_offset as i64 - base;
            let cov_start = *flat_start as i64 - base;
            let cov_end = *flat_end as i64 - base;
            assert!(
                chk_idx >= 0
                    && chk_idx < min_size as i64
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < min_size as i64,
                "{}: checksum at flat {flat_offset} covering flat [{flat_start},{flat_end}] \
                 out of bounds for base {base}, size {min_size}",
                schema.id
            );
            lines.push_str(&format!(
                "        {{ int s = 0; for (int i = {cov_start}; i <= {cov_end}; i++) s += (buf[i] & 0xFF); buf[{chk_idx}] = (byte) s; }}\n"
            ));
        }
        format!("\n    public static void recomputeChecksums(byte[] buf) {{\n{lines}    }}\n")
    } else {
        String::new()
    };

    format!(
        "// Generated by openxcvr-codegen. DO NOT EDIT.\n\
         package com.openxcvr.generated;\n\n\
         public final class {type_name} {{\n{decls}\n\
         \x20\x20\x20\x20public {type_name}({params}) {{\n{assigns}\x20\x20\x20\x20}}\n\n\
         \x20\x20\x20\x20public static {type_name} decode(byte[] buf) {{\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if (buf.length < {min_size}) {{\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20throw new IllegalArgumentException(\"expected >={min_size} bytes, got \" + buf.length);\n\
         \x20\x20\x20\x20\x20\x20\x20\x20}}\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return new {type_name}(\n{args}\n        );\n\
         \x20\x20\x20\x20}}\n{setters}{checksum}}}\n"
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
    fn emits_class_and_decoder() {
        let out = emit_page(&page(vec![
            field("cmis_revision", 1, FieldType::U8, Some(1)),
            field("temp_mon_value", 14, FieldType::I16Be, Some(2)),
            field("module_power_up_time", 144, FieldType::U32Be, Some(4)),
        ]));
        assert!(
            out.contains("public final class CmisLowerMemory {"),
            "{out}"
        );
        assert!(out.contains("public final int cmisRevision;"), "{out}");
        assert!(
            out.contains("public final long modulePowerUpTime;"),
            "{out}"
        );
        assert!(
            out.contains("public static CmisLowerMemory decode(byte[] buf)"),
            "{out}"
        );
        assert!(out.contains("(buf[1] & 0xFF)"), "{out}");
        assert!(out.contains("(int) (short)"), "i16be cast\n{out}");
    }

    #[test]
    fn two_byte_enum_ref_decodes_full_width() {
        // SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13: "Format ID (Bytes
        // 128-129)", 16-bit big-endian value (CA1Bh / 100Bh). A one-byte read
        // truncates it.
        let f = field("format_id", 0, FieldType::EnumRef, Some(2));
        let out = emit_page(&page(vec![f]));
        assert!(
            out.contains("(((buf[0] & 0xFF) << 8) | (buf[1] & 0xFF))"),
            "2-byte enum must decode as big-endian u16\n---\n{out}"
        );
        assert!(
            !out.contains("this.formatId = (buf[0] & 0xFF);"),
            "2-byte enum must not truncate to the first byte\n---\n{out}"
        );
    }

    fn writable(name: &str, offset: u32, ty: FieldType) -> PageField {
        let mut f = field(name, offset, ty, Some(8));
        f.access = Some(crate::ir::pages::Access::Rw);
        f
    }

    #[test]
    fn u64_is_non_negative_big_integer_and_u32_stays_long() {
        let out = emit_page(&page(vec![
            writable("counter_le", 0, FieldType::U64Le),
            writable("counter_be", 8, FieldType::U64Be),
            field("module_power_up_time", 16, FieldType::U32Be, Some(4)),
        ]));
        assert!(
            out.contains("public final java.math.BigInteger counterLe;"),
            "{out}"
        );
        assert!(
            out.contains("public final java.math.BigInteger counterBe;"),
            "{out}"
        );
        assert!(
            out.contains("public final long modulePowerUpTime;"),
            "{out}"
        );
        assert!(
            out.contains(
                "new java.math.BigInteger(1, new byte[] { buf[7], buf[6], buf[5], buf[4], buf[3], buf[2], buf[1], buf[0] })"
            ),
            "u64le magnitude must be little-endian bytes reversed into big-endian order\n{out}"
        );
        assert!(
            out.contains(
                "new java.math.BigInteger(1, new byte[] { buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15] })"
            ),
            "u64be magnitude must be wire order\n{out}"
        );
        assert!(
            out.contains("void setCounterLe(byte[] buf, java.math.BigInteger value)"),
            "{out}"
        );
        assert!(
            out.contains("void setCounterBe(byte[] buf, java.math.BigInteger value)"),
            "{out}"
        );
        assert!(out.contains("value.bitLength() > 64"), "{out}");
        assert!(out.contains("IllegalArgumentException"), "{out}");
        assert!(out.contains("buf[0 + (7 - u64i)]"), "le writer\n{out}");
        assert!(out.contains("buf[8 + u64i]"), "be writer\n{out}");
        assert!(
            !out.contains("(long) (buf[0] & 0xFF)"),
            "u64 must not decode as a signed long\n{out}"
        );
    }
}
