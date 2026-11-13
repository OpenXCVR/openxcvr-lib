use crate::expr::compute;
use crate::ir::naming::{CNaming, NamingPolicy};
use crate::ir::pages::{page_base, FieldType, InstancesDesc, PageField, PageSchema};

pub(super) fn c_type(f: &PageField) -> Option<&'static str> {
    match &f.ty {
        FieldType::Bool => Some("bool"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7
        | FieldType::U8 => Some("uint8_t"),
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 / SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; the member must hold the full width
        // (mirrors the Rust emitter's u16 enum backing).
        FieldType::EnumRef if f.size_bytes == Some(2) => Some("uint16_t"),
        FieldType::EnumRef => Some("uint8_t"),
        FieldType::I8 => Some("int8_t"),
        FieldType::U16Be => Some("uint16_t"),
        FieldType::I16Be => Some("int16_t"),
        FieldType::U64Le | FieldType::U64Be => Some("uint64_t"),
        FieldType::Ascii => Some("char"),
        _ => None,
    }
}

/// Eight wire bytes assembled into a `uint64_t`. Every byte is cast before
/// it is shifted, matching `extract_raw` in `bindings/c/src/observation.c`:
/// little-endian is `bytes[i] << (8 * i)`, big-endian is MSB at the lowest
/// address.
fn assemble_u64(file_offset: usize, little_endian: bool) -> String {
    let terms: Vec<String> = (0..8)
        .map(|i| {
            let shift = if little_endian { i * 8 } else { (7 - i) * 8 };
            format!("((uint64_t)buf[{}] << {shift})", file_offset + i)
        })
        .collect();
    format!("({})", terms.join(" | "))
}

fn decode_expr(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bit_offset = f.bit_offset.unwrap_or(0);
    // Single width defaulting rule (shared with bit_coverage and the docs
    // emitter): recorded bit_width when present, else the wire-type implied
    // width. A byte-wide default leaked high bits for implicit-width U2..U7
    // fields such as CMIS Page 01h media_lane_count_app (raw 0xA5 -> 165).
    let bit_width = f.effective_bit_width();
    match &f.ty {
        FieldType::Bool => format!("(((buf[{o}] >> {bit_offset}) & 0x1u) != 0)"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bit_width) - 1;
            format!("(uint8_t)((buf[{o}] >> {bit_offset}) & 0x{mask:X}u)")
        }
        FieldType::U8 => format!("buf[{o}]"),
        // Two-byte enum_ref fields read the full 16-bit big-endian enum
        // value (SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13 "128-129 | 2 |
        // Format ID | CA1Bh"); the struct member is uint16_t (see c_type).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!(
                "(uint16_t)(((uint16_t)buf[{o}] << 8) | (uint16_t)buf[{}])",
                o + 1
            )
        }
        FieldType::EnumRef => format!("buf[{o}]"),
        FieldType::I8 => format!("(int8_t)buf[{o}]"),
        FieldType::U16Be => {
            format!(
                "(uint16_t)(((uint16_t)buf[{o}] << 8) | (uint16_t)buf[{}])",
                o + 1
            )
        }
        FieldType::I16Be => {
            format!(
                "(int16_t)(((uint16_t)buf[{o}] << 8) | (uint16_t)buf[{}])",
                o + 1
            )
        }
        FieldType::U64Le => assemble_u64(o, true),
        FieldType::U64Be => assemble_u64(o, false),
        _ => "0".to_string(),
    }
}

pub(super) struct FieldEmit {
    pub decl: String,
    pub assign: String,
    pub needs_ascii_helper: bool,
}

fn emit_scalar_field(f: &PageField, base: i64, destination: &str) -> Option<FieldEmit> {
    if f.offset.is_none()
        || f.offset_expr.is_some()
        || f.instances.is_some()
        || f.variants.is_some()
    {
        return None;
    }
    if matches!(
        f.ty,
        FieldType::Bitmap
            | FieldType::Composite
            | FieldType::F16Be
            | FieldType::X16Be
            | FieldType::Bytes
            | FieldType::U16Le
            | FieldType::U32Be
            | FieldType::I32Be
    ) {
        return None;
    }
    if matches!(f.ty, FieldType::Ascii) && f.size_bytes.is_none() {
        return None;
    }
    let ty = c_type(f)?;
    let flat = f.offset.unwrap() as i64;
    let file_off = flat - base;
    if file_off < 0 {
        return None;
    }

    let name = &f.name;
    if matches!(f.ty, FieldType::Ascii) {
        let size = f.size_bytes.unwrap() as usize;
        return Some(FieldEmit {
            decl: format!("    {ty} {name}[{}];\n", size + 1),
            assign: format!(
                "    openxcvr_copy_ascii({destination}{name}, sizeof({destination}{name}), &buf[{file_off}], {size});\n"
            ),
            needs_ascii_helper: true,
        });
    }

    Some(FieldEmit {
        decl: format!("    {ty} {name};\n"),
        assign: format!("    {destination}{name} = {};\n", decode_expr(f, file_off)),
        needs_ascii_helper: false,
    })
}

/// Resolve a field's per-instance bit offset, mirroring the Go/Rust/Python
/// emitters (`go/pages.rs::resolve_bit_offset`). An empty `bit_offset_expr`
/// falls back to the static `bit_offset`; a present expr is evaluated against
/// the instance index; absence of both yields the static `bit_offset` (0).
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

fn emit_instanced_field(
    f: &PageField,
    inst: &InstancesDesc,
    base: i64,
    destination: &str,
) -> Option<FieldEmit> {
    if f.variants.is_some()
        || matches!(
            f.ty,
            FieldType::Ascii
                | FieldType::Bitmap
                | FieldType::Composite
                | FieldType::F16Be
                | FieldType::X16Be
                | FieldType::Bytes
                | FieldType::U16Le
                | FieldType::U32Be
                | FieldType::I32Be
        )
    {
        return None;
    }
    let ty = c_type(f)?;
    let count = (inst.end - inst.start + 1) as usize;
    let mut assign = String::new();
    for (slot, idx) in (inst.start..=inst.end).enumerate() {
        let flat = if let Some(expr) = &f.offset_expr {
            compute(expr, &inst.index, idx).ok()?
        } else {
            f.offset? as i64
        };
        let file_off = flat - base;
        if file_off < 0 {
            return None;
        }
        let bit_offset = resolve_bit_offset(f, inst, idx)?;
        if bit_offset < 0 {
            return None;
        }
        let mut element = f.clone();
        element.bit_offset = Some(bit_offset as u32);
        element.bit_offset_expr = None;
        assign.push_str(&format!(
            "    {destination}{}[{slot}] = {};\n",
            f.name,
            decode_expr(&element, file_off)
        ));
    }
    Some(FieldEmit {
        decl: format!("    {ty} {}[{count}];\n", f.name),
        assign,
        needs_ascii_helper: false,
    })
}

/// Shared raw/checked extraction; the field name is the stable public member
/// identity, while all other metadata may be revision-resolved.
pub(super) fn emit_field_to(f: &PageField, base: i64, destination: &str) -> Option<FieldEmit> {
    if let Some(inst) = &f.instances {
        emit_instanced_field(f, inst, base, destination)
    } else {
        emit_scalar_field(f, base, destination)
    }
}

const SUPPLIED_WIRE_READER_COMMENT: &str =
    "/* wire-value reader: no variant interpretation, no calibration */";

fn supplied_fields(schema: &PageSchema, header: bool) -> String {
    if !matches!(
        schema.id.as_str(),
        "CmisPage00hAdministrative"
            | "Sff8636Page00hUpperAdministrative"
            | "Sff8636LowerMemory"
            | "page_a0h_base_memory"
            | "page_a2h_diagnostics_base"
    ) {
        return String::new();
    }
    let suffix = CNaming.type_name(&schema.id);
    let mut out = String::new();
    for f in schema.value_fields() {
        if let Some(inst) = &f.instances {
            out.push_str(&supplied_instanced_reader(f, inst, &suffix, header));
            continue;
        }
        out.push_str(&supplied_fixed_reader(f, &suffix, header));
    }
    out
}

/// Fixed-offset supplied readers. Ascii/Bytes keep the identity-API shape
/// byte-identical; U16Be/I16Be (variants included — variants are
/// interpretation, not location) and whole-byte Bitmap are additive wire-value
/// readers for the SFF aggregate JSON decoders.
fn supplied_fixed_reader(f: &PageField, suffix: &str, header: bool) -> String {
    if f.offset.is_none() || f.offset_expr.is_some() {
        return String::new();
    }
    let offset = f.offset.unwrap();
    let name = &f.name;

    enum Out {
        Ascii(usize),
        Bytes(usize),
        U16,
        I16,
        Bitmap1,
    }
    let kind = match &f.ty {
        FieldType::Ascii if f.variants.is_none() => match f.size_bytes {
            Some(size) => Out::Ascii(size as usize),
            None => return String::new(),
        },
        FieldType::Bytes if f.variants.is_none() => match f.size_bytes {
            Some(size) => Out::Bytes(size as usize),
            None => return String::new(),
        },
        FieldType::U16Be if f.size_bytes == Some(2) && f.bit_offset.is_none() => Out::U16,
        FieldType::I16Be if f.size_bytes == Some(2) && f.bit_offset.is_none() => Out::I16,
        FieldType::Bitmap if f.size_bytes == Some(1) && f.bit_offset.is_none() => Out::Bitmap1,
        _ => return String::new(),
    };

    let (ty, size, decode) = match kind {
        Out::Ascii(size) => (
            "char",
            size,
            format!(
                "openxcvr_copy_ascii(out, {}, buf + start, {size});",
                size + 1
            ),
        ),
        Out::Bytes(size) => (
            "uint8_t",
            size,
            format!("for (size_t i = 0; i < {size}; ++i) out[i] = buf[start + i];"),
        ),
        Out::U16 => (
            "uint16_t",
            2,
            "*out = (uint16_t)(((uint16_t)buf[start] << 8) | (uint16_t)buf[start + 1]);"
                .to_string(),
        ),
        Out::I16 => (
            "int16_t",
            2,
            "*out = (int16_t)(((uint16_t)buf[start] << 8) | (uint16_t)buf[start + 1]);".to_string(),
        ),
        Out::Bitmap1 => ("uint8_t", 1, "*out = buf[start];".to_string()),
    };

    let function = format!("openxcvr_decode_{suffix}_{name}_supplied");
    let signature = format!(
        "openxcvr_status {function}(const uint8_t *buf, size_t offset, size_t len, {ty} *out)"
    );
    let wire_comment = if matches!(kind, Out::U16 | Out::I16 | Out::Bitmap1) {
        format!("{SUPPLIED_WIRE_READER_COMMENT}\n")
    } else {
        String::new()
    };
    if header {
        let macro_name = format!("OPENXCVR_{suffix}_{name}").to_uppercase();
        format!(
            "#define {macro_name}_OFFSET {offset}\n#define {macro_name}_LEN {size}\n{wire_comment}{signature};\n"
        )
    } else {
        format!(
            "\n{wire_comment}{signature} {{\n    if (buf == NULL || out == NULL || offset > 256 || len > 256 - offset) return OPENXCVR_ERR_INVALID_ARGUMENT;\n    if (offset > {offset}) return OPENXCVR_ERR_INSUFFICIENT_INPUT;\n    size_t start = {offset} - offset;\n    if (start > len || {size} > len - start) return OPENXCVR_ERR_INSUFFICIENT_INPUT;\n    {decode}\n    return OPENXCVR_OK;\n}}\n"
        )
    }
}

/// Instanced sub-byte Bitmap supplied reader (SFF-8636 lower-memory lane
/// alarm/warning nibbles): one function writing all N instance wire values,
/// in instance order, into out[0..N). `_OFFSET` is the lowest byte of the
/// field's footprint, `_LEN` its byte span, `_COUNT` the instance count.
fn supplied_instanced_reader(
    f: &PageField,
    inst: &InstancesDesc,
    suffix: &str,
    header: bool,
) -> String {
    if !matches!(&f.ty, FieldType::Bitmap)
        || f.bit_width.is_none()
        || f.bit_width.unwrap() >= 8
        || f.bit_width.unwrap() == 0
        || f.variants.is_some()
    {
        return String::new();
    }
    let width = f.bit_width.unwrap();
    let mask = (1u32 << width) - 1;

    let mut flats = Vec::new();
    let mut shifts = Vec::new();
    for idx in inst.start..=inst.end {
        let flat = match &f.offset_expr {
            Some(expr) => match compute(expr, &inst.index, idx) {
                Ok(v) => v,
                Err(_) => return String::new(),
            },
            None => return String::new(),
        };
        let bit = match resolve_bit_offset(f, inst, idx) {
            Some(v) => v,
            None => return String::new(),
        };
        if flat < 0 || bit < 0 {
            return String::new();
        }
        flats.push(flat as u32);
        shifts.push(bit as u32);
    }
    let lowest = *flats.iter().min().unwrap();
    let highest = *flats.iter().max().unwrap();
    let span = highest - lowest + 1;
    let count = flats.len();

    let name = &f.name;
    let function = format!("openxcvr_decode_{suffix}_{name}_supplied");
    let signature = format!(
        "openxcvr_status {function}(const uint8_t *buf, size_t offset, size_t len, uint8_t *out)"
    );
    if header {
        let macro_name = format!("OPENXCVR_{suffix}_{name}").to_uppercase();
        return format!(
            "#define {macro_name}_OFFSET {lowest}\n#define {macro_name}_LEN {span}\n#define {macro_name}_COUNT {count}\n{SUPPLIED_WIRE_READER_COMMENT}\n{signature};\n"
        );
    }

    let mut decode = String::new();
    for (slot, (flat, shift)) in flats.iter().zip(&shifts).enumerate() {
        let byte = if *flat == lowest {
            "start".to_string()
        } else {
            format!("start + {}", flat - lowest)
        };
        decode.push_str(&format!(
            "    out[{slot}] = (uint8_t)((buf[{byte}] >> {shift}) & 0x{mask:X}u);\n"
        ));
    }
    format!(
        "\n{SUPPLIED_WIRE_READER_COMMENT}\n{signature} {{\n    if (buf == NULL || out == NULL || offset > 256 || len > 256 - offset) return OPENXCVR_ERR_INVALID_ARGUMENT;\n    if (offset > {lowest}) return OPENXCVR_ERR_INSUFFICIENT_INPUT;\n    size_t start = {lowest} - offset;\n    if (start > len || {span} > len - start) return OPENXCVR_ERR_INSUFFICIENT_INPUT;\n{decode}    return OPENXCVR_OK;\n}}\n"
    )
}

pub(super) const ASCII_HELPER: &str = concat!(
    "static void openxcvr_copy_ascii(char *dst, size_t dst_len, const uint8_t *src, size_t src_len) {\n",
    "    size_t n = src_len;\n",
    "    while (n > 0 && src[n - 1] == ' ') {\n",
    "        n--;\n",
    "    }\n",
    "    if (n >= dst_len) {\n",
    "        n = dst_len - 1;\n",
    "    }\n",
    "    for (size_t i = 0; i < n; i++) {\n",
    "        dst[i] = (char)src[i];\n",
    "    }\n",
    "    dst[n] = '\\0';\n",
    "}\n\n",
);

pub fn emit_page_header(schema: &PageSchema) -> String {
    let type_suffix = CNaming.type_name(&schema.id);
    let type_name = format!("openxcvr_{type_suffix}");
    let func_name = format!("openxcvr_decode_{type_suffix}");
    let base = page_base(schema);

    let mut decls = String::new();
    for f in schema.value_fields() {
        let emit = emit_field_to(f, base, "out->");
        if let Some(field) = emit {
            decls.push_str(&field.decl);
        }
    }
    if decls.is_empty() {
        decls.push_str("    uint8_t _unused;\n");
    }

    // checksum recompute declaration: emit for pages with ≥1 sum8 field.
    let recompute_base = page_base(schema);
    let chksum_fields: Vec<_> = schema
        .value_fields()
        .filter_map(|f| {
            let offset = f.offset?;
            let (flat_start, flat_end) = f.checksum_region()?;
            Some((offset, flat_start, flat_end))
        })
        .collect();
    let recompute_decl = if chksum_fields.is_empty() {
        String::new()
    } else {
        let size = schema.size_bytes.unwrap_or(128) as i64;
        for (flat_offset, flat_start, flat_end) in &chksum_fields {
            let chk_idx = *flat_offset as i64 - recompute_base;
            let cov_start = *flat_start as i64 - recompute_base;
            let cov_end = *flat_end as i64 - recompute_base;
            assert!(
                chk_idx >= 0
                    && chk_idx < size
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < size,
                "{}: checksum at flat {} covering flat [{},{}] \
                 out of bounds for recompute_base {recompute_base}, size {size}",
                schema.id,
                flat_offset,
                flat_start,
                flat_end,
            );
        }
        format!("void openxcvr_{type_suffix}_recompute_checksums(uint8_t *buf);\n")
    };

    format!(
        concat!(
            "/* Generated by openxcvr-codegen. DO NOT EDIT. */\n",
            "#pragma once\n\n",
            "#include <stdbool.h>\n",
            "#include <stddef.h>\n",
            "#include <stdint.h>\n\n",
            "#include \"openxcvr/status.h\"\n\n",
            "typedef struct {type_name} {{\n",
            "{decls}",
            "}} {type_name};\n\n",
            "openxcvr_status {func_name}(const uint8_t *buf, size_t len, {type_name} *out);\n",
            "{recompute_decl}",
            "{supplied_decl}",
        ),
        type_name = type_name,
        func_name = func_name,
        decls = decls,
        recompute_decl = recompute_decl,
        supplied_decl = supplied_fields(schema, true),
    )
}

pub fn emit_page_source(schema: &PageSchema, header_path: &str) -> String {
    let type_suffix = CNaming.type_name(&schema.id);
    let type_name = format!("openxcvr_{type_suffix}");
    let func_name = format!("openxcvr_decode_{type_suffix}");
    let min_size = schema.size_bytes.unwrap_or(128);
    let base = page_base(schema);

    let mut assigns = String::new();
    let mut needs_ascii_helper = false;
    for f in schema.value_fields() {
        let emit = emit_field_to(f, base, "out->");
        if let Some(field) = emit {
            needs_ascii_helper |= field.needs_ascii_helper;
            assigns.push_str(&field.assign);
        }
    }

    let ascii_helper = if needs_ascii_helper { ASCII_HELPER } else { "" };

    // checksum recompute definition: emit for pages with ≥1 sum8 field.
    let recompute_base = page_base(schema);
    let chksum_fields: Vec<_> = schema
        .value_fields()
        .filter_map(|f| {
            let offset = f.offset?;
            let (flat_start, flat_end) = f.checksum_region()?;
            Some((offset, flat_start, flat_end))
        })
        .collect();
    let recompute_def = if chksum_fields.is_empty() {
        String::new()
    } else {
        let mut body = String::new();
        let size = schema.size_bytes.unwrap_or(128) as i64;
        for (flat_offset, flat_start, flat_end) in &chksum_fields {
            let chk_idx = *flat_offset as i64 - recompute_base;
            let cov_start = *flat_start as i64 - recompute_base;
            let cov_end = *flat_end as i64 - recompute_base;
            // bounds already asserted at declaration time in emit_page_header;
            // assert again defensively (both fns share the same logic).
            assert!(
                chk_idx >= 0
                    && chk_idx < size
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < size,
                "{}: checksum at flat {} covering flat [{},{}] \
                 out of bounds for recompute_base {recompute_base}, size {size}",
                schema.id,
                flat_offset,
                flat_start,
                flat_end,
            );
            let cov_end_excl = cov_end + 1;
            body.push_str(&format!(
                "    {{ uint8_t c = 0; for (size_t i = {cov_start}; i < {cov_end_excl}; i++) c += buf[i]; buf[{chk_idx}] = c; }}\n"
            ));
        }
        format!("\nvoid openxcvr_{type_suffix}_recompute_checksums(uint8_t *buf) {{\n{body}}}\n")
    };

    format!(
        concat!(
            "/* Generated by openxcvr-codegen. DO NOT EDIT. */\n",
            "#include \"{header_path}\"\n\n",
            "{ascii_helper}",
            "openxcvr_status {func_name}(const uint8_t *buf, size_t len, {type_name} *out) {{\n",
            "    if (buf == NULL || out == NULL) {{\n",
            "        return OPENXCVR_ERR_INVALID_ARGUMENT;\n",
            "    }}\n",
            "    if (len < {min_size}) {{\n",
            "        return OPENXCVR_ERR_SHORT_BUFFER;\n",
            "    }}\n",
            "{assigns}",
            "    return OPENXCVR_OK;\n",
            "}}\n",
            "{recompute_def}",
            "{supplied_def}",
        ),
        header_path = header_path,
        ascii_helper = ascii_helper,
        func_name = func_name,
        type_name = type_name,
        min_size = min_size,
        assigns = assigns,
        recompute_def = recompute_def,
        supplied_def = supplied_fields(schema, false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::pages::{FieldType, InstancesDesc, PageField, PageSchema};

    fn field(name: &str, offset: u32, ty: FieldType) -> PageField {
        PageField {
            name: name.to_string(),
            display_name: None,
            offset: Some(offset),
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: Some(1),
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

    #[test]
    fn emits_page_header_and_source() {
        let mut ascii = field("vendor_name", 129, FieldType::Ascii);
        ascii.size_bytes = Some(16);
        let mut temp = field("temperature_raw", 14, FieldType::I16Be);
        temp.size_bytes = Some(2);
        let mut flag = field("memory_model_paged", 2, FieldType::Bool);
        flag.bit_offset = Some(7);
        flag.bit_width = Some(1);

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisLowerMemory".to_string(),
            title: "Lower".to_string(),
            page_address: "lower".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![field("identifier", 0, FieldType::U8), flag, temp, ascii],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        assert!(
            h.contains("typedef struct openxcvr_cmis_lower_memory"),
            "{h}"
        );
        assert!(h.contains("uint8_t identifier;"), "{h}");
        assert!(h.contains("bool memory_model_paged;"), "{h}");
        assert!(h.contains("int16_t temperature_raw;"), "{h}");
        assert!(h.contains("char vendor_name[17];"), "{h}");

        let c = emit_page_source(&schema, "pages/cmis_lower_memory.h");
        assert!(c.contains("len < 128"), "{c}");
        assert!(c.contains("out->identifier = buf[0];"), "{c}");
        assert!(c.contains("buf[2] >> 7"), "{c}");
        assert!(c.contains("openxcvr_copy_ascii"), "{c}");
        assert!(c.contains("(int16_t)(((uint16_t)buf[14] << 8)"), "{c}");
    }

    #[test]
    fn emits_both_halves_of_256_byte_page_at_page_relative_offsets() {
        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "Sff8472A0hMemory".to_string(),
            title: "SFF-8472 A0h Memory".to_string(),
            page_address: "A0h".to_string(),
            size_bytes: Some(256),
            field_includes: vec![],
            fields: vec![
                field("lower_half", 0, FieldType::U8),
                field("upper_half", 200, FieldType::U8),
            ],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        assert!(h.contains("uint8_t lower_half;"), "{h}");
        assert!(h.contains("uint8_t upper_half;"), "{h}");

        let c = emit_page_source(&schema, "pages/sff8472_a0h_memory.h");
        assert!(c.contains("len < 256"), "{c}");
        assert!(c.contains("out->lower_half = buf[0];"), "{c}");
        assert!(c.contains("out->upper_half = buf[200];"), "{c}");
    }

    #[test]
    fn emits_instanced_array() {
        let mut lane = field("lane_status", 128, FieldType::U8);
        lane.offset = None;
        lane.offset_expr = Some("128 + lane".to_string());
        lane.instances = Some(InstancesDesc {
            index: "lane".to_string(),
            start: 0,
            end: 3,
        });
        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisPage11hLaneStatus".to_string(),
            title: "Lane Status".to_string(),
            page_address: "0x11".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![lane],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };
        let h = emit_page_header(&schema);
        let c = emit_page_source(&schema, "pages/cmis_page11h_lane_status.h");
        assert!(h.contains("uint8_t lane_status[4];"), "{h}");
        assert!(c.contains("out->lane_status[0] = buf[0];"), "{c}");
        assert!(c.contains("out->lane_status[3] = buf[3];"), "{c}");
    }

    /// Regression for issue #195: instanced fields with a per-instance
    /// `bit_offset_expr` must thread the resolved bit offset into each element,
    /// not collapse every lane to the same static shift. Models the real CMIS
    /// Page 11h indexing: `instances {index: lane, start: 1, end: 8}`.
    #[test]
    fn emits_instanced_array_with_bit_offset() {
        // Nibble field: even lanes -> low nibble (>>0), odd lanes -> high nibble
        // (>>4); two lanes per byte. offset_expr "128 + (lane - 1) / 2",
        // bit_offset_expr "((lane - 1) % 2) * 4", base 128.
        let mut nibble = field("dp_state_host_lane", 0, FieldType::U4);
        nibble.offset = None;
        nibble.offset_expr = Some("128 + (lane - 1) / 2".to_string());
        nibble.bit_offset_expr = Some("((lane - 1) % 2) * 4".to_string());
        nibble.bit_width = Some(4);
        nibble.instances = Some(InstancesDesc {
            index: "lane".to_string(),
            start: 1,
            end: 8,
        });

        // Bool flag: all 8 lanes in one byte, bit = lane - 1.
        let mut flag = field("dp_state_changed_flag", 0, FieldType::Bool);
        flag.offset = None;
        flag.offset_expr = Some("135".to_string());
        flag.bit_offset_expr = Some("lane - 1".to_string());
        flag.bit_width = Some(1);
        flag.instances = Some(InstancesDesc {
            index: "lane".to_string(),
            start: 1,
            end: 8,
        });

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisPage11hLaneStatus".to_string(),
            title: "Lane Status".to_string(),
            page_address: "0x11".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![nibble, flag],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        let c = emit_page_source(&schema, "pages/cmis_page11h_lane_status.h");

        assert!(h.contains("uint8_t dp_state_host_lane[8];"), "{h}");
        assert!(h.contains("bool dp_state_changed_flag[8];"), "{h}");

        // Exact alternating nibble shifts across the 8 lanes (the #195 bug
        // produced "buf[0] >> 0" for both lane[0] and lane[1]).
        let nibble_expected = [
            ("dp_state_host_lane[0]", 0, 0),
            ("dp_state_host_lane[1]", 0, 4),
            ("dp_state_host_lane[2]", 1, 0),
            ("dp_state_host_lane[3]", 1, 4),
            ("dp_state_host_lane[4]", 2, 0),
            ("dp_state_host_lane[5]", 2, 4),
            ("dp_state_host_lane[6]", 3, 0),
            ("dp_state_host_lane[7]", 3, 4),
        ];
        for (slot, byte, shift) in nibble_expected {
            let line = format!("out->{slot} = (uint8_t)((buf[{byte}] >> {shift}) & 0xFu);");
            assert!(c.contains(&line), "missing `{line}` in:\n{c}");
        }

        // Bool flag bit spread 0..7 within the same byte (file offset 7).
        for (slot, shift) in (0..8).map(|i| (i, i)) {
            let line = format!(
                "out->dp_state_changed_flag[{slot}] = (((buf[7] >> {shift}) & 0x1u) != 0);"
            );
            assert!(c.contains(&line), "missing `{line}` in:\n{c}");
        }
    }

    /// Implicit sub-byte widths: U2..U7 fields without a recorded `bit_width`
    /// must mask to their wire-type width (the `effective_bit_width()` rule),
    /// not the legacy byte-wide `unwrap_or(8)` default. Models the real CMIS
    /// Page 01h `media_lane_count_app` (u4, implicit width, 7 app slots) whose
    /// decode previously emitted `& 0xFFu`, so a raw 0xA5 decoded as 165.
    #[test]
    fn emits_implicit_subbyte_width_masks_from_wire_type() {
        let mut u2 = field("implicit_u2", 128, FieldType::U2);
        u2.bit_width = None;
        let mut u3 = field("implicit_u3", 129, FieldType::U3);
        u3.bit_width = None;
        let mut u4 = field("implicit_u4", 130, FieldType::U4);
        u4.bit_width = None;
        let mut u5 = field("implicit_u5", 131, FieldType::U5);
        u5.bit_width = None;
        let mut u6 = field("implicit_u6", 132, FieldType::U6);
        u6.bit_width = None;
        let mut u7 = field("implicit_u7", 133, FieldType::U7);
        u7.bit_width = None;

        // Explicit width still overrides the wire-type implied width.
        let mut explicit = field("explicit_u4_width2", 134, FieldType::U4);
        explicit.bit_width = Some(2);

        // Real defect shape: implicit-width instanced u4 (CMIS Page 01h
        // media_lane_count_app, 7 application slots, flat 225 + 4*(app-1)).
        let mut instanced = field("media_lane_count_app", 0, FieldType::U4);
        instanced.offset = None;
        instanced.offset_expr = Some("225 + 4 * (app - 1)".to_string());
        instanced.bit_width = None;
        instanced.instances = Some(InstancesDesc {
            index: "app".to_string(),
            start: 1,
            end: 7,
        });

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisPage01hAdvertising".to_string(),
            title: "Page 01h Advertising".to_string(),
            page_address: "0x01".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![u2, u3, u4, u5, u6, u7, explicit, instanced],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let c = emit_page_source(&schema, "pages/cmis_page01h_advertising.h");

        assert!(
            c.contains("out->implicit_u2 = (uint8_t)((buf[0] >> 0) & 0x3u);"),
            "{c}"
        );
        assert!(
            c.contains("out->implicit_u3 = (uint8_t)((buf[1] >> 0) & 0x7u);"),
            "{c}"
        );
        assert!(
            c.contains("out->implicit_u4 = (uint8_t)((buf[2] >> 0) & 0xFu);"),
            "{c}"
        );
        assert!(
            c.contains("out->implicit_u5 = (uint8_t)((buf[3] >> 0) & 0x1Fu);"),
            "{c}"
        );
        assert!(
            c.contains("out->implicit_u6 = (uint8_t)((buf[4] >> 0) & 0x3Fu);"),
            "{c}"
        );
        assert!(
            c.contains("out->implicit_u7 = (uint8_t)((buf[5] >> 0) & 0x7Fu);"),
            "{c}"
        );
        assert!(
            c.contains("out->explicit_u4_width2 = (uint8_t)((buf[6] >> 0) & 0x3u);"),
            "{c}"
        );

        // All seven instanced slots mask to the u4 nibble width; the
        // pre-fix emitter produced 0xFFu here (raw 0xA5 -> 165, not 5).
        for (slot, byte) in [
            (0, 97),
            (1, 101),
            (2, 105),
            (3, 109),
            (4, 113),
            (5, 117),
            (6, 121),
        ] {
            let line = format!(
                "out->media_lane_count_app[{slot}] = (uint8_t)((buf[{byte}] >> 0) & 0xFu);"
            );
            assert!(c.contains(&line), "missing `{line}` in:\n{c}");
        }
    }

    /// Checksum recompute emission: a page with a sum8 checksum field
    /// must emit both the declaration in the header and the free-function definition
    /// in the source with the correct buf-relative indices and loop bounds.
    ///
    /// Uses a synthetic upper-page schema (page_address "0x00", size_bytes 128).
    /// The covers_range [128, 190] and checksum at flat 191 maps to buf indices
    /// [0, 62] and 63 after subtracting recompute_base=128.
    /// Additive supplied-reader arms (SFF C aggregate decoders, design R2/R3
    /// C-a): the `page_a2h_diagnostics_base` page admits fixed-offset scalar
    /// readers for U16Be/I16Be (variants included — wire value, not
    /// interpretation) and whole-byte Bitmap fields.
    #[test]
    fn supplied_scalar_readers_for_u16be_i16be_and_bitmap1() {
        let mut temperature = field("temperature", 96, FieldType::I16Be);
        temperature.size_bytes = Some(2);
        temperature.variants = Some(serde_norway::Value::String(
            "dispatch on supply type".to_string(),
        ));
        let mut supply_voltage = field("supply_voltage", 98, FieldType::U16Be);
        supply_voltage.size_bytes = Some(2);
        supply_voltage.variants = Some(serde_norway::Value::String(
            "dispatch on supply type".to_string(),
        ));
        let mut status_control = field("status_control", 110, FieldType::Bitmap);
        status_control.size_bytes = Some(1);

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "page_a2h_diagnostics_base".to_string(),
            title: "SFF-8472 A2h Diagnostics".to_string(),
            page_address: "A2h".to_string(),
            size_bytes: Some(256),
            field_includes: vec![],
            fields: vec![temperature, supply_voltage, status_control],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        let c = emit_page_source(&schema, "pages/page_a2h_diagnostics_base.h");

        assert!(
            h.contains("#define OPENXCVR_PAGE_A2H_DIAGNOSTICS_BASE_TEMPERATURE_OFFSET 96\n#define OPENXCVR_PAGE_A2H_DIAGNOSTICS_BASE_TEMPERATURE_LEN 2\n/* wire-value reader: no variant interpretation, no calibration */\nopenxcvr_status openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(const uint8_t *buf, size_t offset, size_t len, int16_t *out);\n"),
            "temperature reader missing/wrong in header:\n{h}"
        );
        assert!(
            h.contains("openxcvr_decode_page_a2h_diagnostics_base_supply_voltage_supplied(const uint8_t *buf, size_t offset, size_t len, uint16_t *out);"),
            "supply_voltage u16be reader missing in header:\n{h}"
        );
        assert!(
            h.contains("openxcvr_decode_page_a2h_diagnostics_base_status_control_supplied(const uint8_t *buf, size_t offset, size_t len, uint8_t *out);"),
            "status_control bitmap reader missing in header:\n{h}"
        );

        // Big-endian assembly with the sign-preserving int16_t cast, the
        // whole-byte bitmap read, and the shared bounds checks.
        assert!(
            c.contains("*out = (int16_t)(((uint16_t)buf[start] << 8) | (uint16_t)buf[start + 1]);"),
            "i16be assembly missing in source:\n{c}"
        );
        assert!(
            c.contains(
                "*out = (uint16_t)(((uint16_t)buf[start] << 8) | (uint16_t)buf[start + 1]);"
            ),
            "u16be assembly missing in source:\n{c}"
        );
        assert!(
            c.contains("*out = buf[start];"),
            "bitmap-1 read missing in source:\n{c}"
        );
        assert!(
            c.contains("if (buf == NULL || out == NULL || offset > 256 || len > 256 - offset) return OPENXCVR_ERR_INVALID_ARGUMENT;"),
            "NULL/overflow guard missing in source:\n{c}"
        );
        assert!(
            c.matches("if (offset > 96) return OPENXCVR_ERR_INSUFFICIENT_INPUT;")
                .count()
                >= 1,
            "per-field insufficient-input guard missing in source:\n{c}"
        );
    }

    /// Instanced sub-byte Bitmap fields (SFF-8636 lower-memory lane
    /// alarm/warning nibbles, bytes 9-14) get one reader writing all N
    /// instance values in instance order, with _OFFSET/_LEN/_COUNT macros
    /// where _OFFSET is the lowest byte and _LEN the byte span.
    #[test]
    fn supplied_instanced_bitmap_nibble_reader() {
        let mut nibble = field("rx_power_alarm_warning_flag", 0, FieldType::Bitmap);
        nibble.offset = None;
        nibble.offset_expr = Some("9 + (lane - 1) / 2".to_string());
        nibble.bit_offset_expr = Some("4 * (1 - ((lane - 1) % 2))".to_string());
        nibble.bit_width = Some(4);
        nibble.instances = Some(InstancesDesc {
            index: "lane".to_string(),
            start: 1,
            end: 4,
        });

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "Sff8636LowerMemory".to_string(),
            title: "SFF-8636 Lower Memory".to_string(),
            page_address: "lower".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![nibble],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        let c = emit_page_source(&schema, "pages/sff8636_lower_memory.h");

        assert!(
            h.contains("#define OPENXCVR_SFF8636_LOWER_MEMORY_RX_POWER_ALARM_WARNING_FLAG_OFFSET 9\n#define OPENXCVR_SFF8636_LOWER_MEMORY_RX_POWER_ALARM_WARNING_FLAG_LEN 2\n#define OPENXCVR_SFF8636_LOWER_MEMORY_RX_POWER_ALARM_WARNING_FLAG_COUNT 4\n/* wire-value reader: no variant interpretation, no calibration */\nopenxcvr_status openxcvr_decode_sff8636_lower_memory_rx_power_alarm_warning_flag_supplied(const uint8_t *buf, size_t offset, size_t len, uint8_t *out);\n"),
            "instanced nibble reader macros/signature wrong in header:\n{h}"
        );

        for (slot, delta, shift) in [(0, 0, 4), (1, 0, 0), (2, 1, 4), (3, 1, 0)] {
            let base_idx = if delta == 0 { "start" } else { "start + 1" };
            let line = format!("out[{slot}] = (uint8_t)((buf[{base_idx}] >> {shift}) & 0xFu);");
            assert!(c.contains(&line), "missing `{line}` in:\n{c}");
        }
        assert!(
            c.contains("if (offset > 9) return OPENXCVR_ERR_INSUFFICIENT_INPUT;"),
            "lowest-byte insufficient-input guard missing in source:\n{c}"
        );
        assert!(
            c.contains(
                "if (start > len || 2 > len - start) return OPENXCVR_ERR_INSUFFICIENT_INPUT;"
            ),
            "span guard missing in source:\n{c}"
        );
    }

    /// Pages outside the allowlist produce no supplied readers at all.
    #[test]
    fn supplied_readers_absent_for_non_allowlisted_page() {
        let mut rate = field("rx_power", 104, FieldType::U16Be);
        rate.size_bytes = Some(2);
        let mut name = field("vendor_name", 20, FieldType::Ascii);
        name.size_bytes = Some(16);

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "Sff8636Page02hUserEeprom".to_string(),
            title: "SFF-8636 Page 02h".to_string(),
            page_address: "0x02".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![rate, name],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        let c = emit_page_source(&schema, "pages/sff8636_page02h_user_eeprom.h");
        assert!(!h.contains("_supplied"), "unexpected supplied decl:\n{h}");
        assert!(!c.contains("_supplied"), "unexpected supplied def:\n{c}");
        assert!(!h.contains("wire-value reader"), "{h}");
    }

    #[test]
    fn emits_checksum_recompute() {
        use serde_norway::Value;

        // Build the encoding map for a sum8 checksum covering flat [128, 190].
        let encoding_yaml = serde_norway::from_str::<Value>(
            "kind: checksum\nalgorithm: sum8\ncovers_range: [128, 190]\n",
        )
        .expect("encoding yaml");

        let mut chk = field("cc_base", 191, FieldType::U8);
        chk.encoding = Some(encoding_yaml);

        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "Sff8636Page00hAdministrative".to_string(),
            title: "SFF-8636 Page 00h".to_string(),
            page_address: "0x00".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![field("identifier", 128, FieldType::U8), chk],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        let h = emit_page_header(&schema);
        let c = emit_page_source(&schema, "pages/sff8636_page00h_administrative.h");

        // Header must declare the recompute symbol.
        assert!(
            h.contains(
                "void openxcvr_sff8636_page00h_administrative_recompute_checksums(uint8_t *buf);"
            ),
            "missing recompute decl in header:\n{h}"
        );

        // Source must define the function with correct buf indices:
        // chk_idx = 191 - 128 = 63, cov_start = 128 - 128 = 0, cov_end+1 = 63.
        assert!(
            c.contains(
                "void openxcvr_sff8636_page00h_administrative_recompute_checksums(uint8_t *buf)"
            ),
            "missing recompute definition in source:\n{c}"
        );
        assert!(
            c.contains("for (size_t i = 0; i < 63; i++) c += buf[i]; buf[63] = c;"),
            "recompute loop body wrong:\n{c}"
        );
    }

    #[test]
    fn two_byte_enum_ref_decodes_full_width() {
        // SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13: "Format ID (Bytes
        // 128-129)", 16-bit big-endian value (CA1Bh / 100Bh). A one-byte
        // read truncates it.
        let mut f = field("format_id", 0, FieldType::EnumRef);
        f.size_bytes = Some(2);
        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisLowerMemory".to_string(),
            title: "Lower".to_string(),
            page_address: "lower".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![f],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };
        let h = emit_page_header(&schema);
        assert!(
            h.contains("uint16_t format_id;"),
            "2-byte enum member must be uint16_t\n---\n{h}"
        );
        let c = emit_page_source(&schema, "pages/cmis_lower_memory.h");
        assert!(
            c.contains("out->format_id = (uint16_t)(((uint16_t)buf[0] << 8) | (uint16_t)buf[1]);"),
            "2-byte enum must decode as big-endian u16\n---\n{c}"
        );
    }

    /// `u64le` and `u64be` are struct members. Each of the eight wire bytes
    /// is cast to `uint64_t` before it is shifted, LSB-first or MSB-first.
    #[test]
    fn emits_u64_both_byte_orders() {
        let mut le = field("counter_le", 0, FieldType::U64Le);
        le.size_bytes = Some(8);
        let mut be = field("counter_be", 8, FieldType::U64Be);
        be.size_bytes = Some(8);
        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisLowerMemory".to_string(),
            title: "Lower".to_string(),
            page_address: "lower".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![le, be],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };
        let h = emit_page_header(&schema);
        assert!(h.contains("uint64_t counter_le;"), "{h}");
        assert!(h.contains("uint64_t counter_be;"), "{h}");

        let c = emit_page_source(&schema, "pages/cmis_lower_memory.h");
        let le_terms: Vec<String> = (0..8)
            .map(|i| format!("((uint64_t)buf[{i}] << {})", i * 8))
            .collect();
        let le_expr = format!("out->counter_le = ({});", le_terms.join(" | "));
        assert!(
            c.contains(&le_expr),
            "little-endian u64 decode missing:\n{c}"
        );

        let be_terms: Vec<String> = (0..8)
            .map(|i| format!("((uint64_t)buf[{}] << {})", 8 + i, (7 - i) * 8))
            .collect();
        let be_expr = format!("out->counter_be = ({});", be_terms.join(" | "));
        assert!(c.contains(&be_expr), "big-endian u64 decode missing:\n{c}");
        assert_ne!(le_expr, be_expr);
    }
}
