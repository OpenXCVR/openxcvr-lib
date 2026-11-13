use crate::expr::compute;
use crate::ir::naming::{GoNaming, NamingPolicy};
use crate::ir::pages::{page_base, FieldType, InstancesDesc, PageField, PageSchema};

#[path = "page_decode.rs"]
mod page_decode;
use page_decode::{decode_scalar_go, go_type, needs_binary, uses_bit_offset};

struct FieldEmit {
    field_name: String,
    field_type: String,
    decode_expr: String,
    needs_binary: bool,
    needs_strings: bool,
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
    let go = go_type(f)?;
    let flat = f.offset.unwrap() as i64;
    let file_off = flat - base;
    if file_off < 0 {
        return None;
    }

    let needs_binary = needs_binary(f);
    let needs_strings = matches!(f.ty, FieldType::Ascii);
    let camel = GoNaming.field_name(&f.name);
    let decode_expr = decode_scalar_go(f, file_off);

    Some(FieldEmit {
        field_name: camel,
        field_type: go.to_string(),
        decode_expr,
        needs_binary,
        needs_strings,
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

fn emit_instanced_field(f: &PageField, inst: &InstancesDesc, base: i64) -> Option<FieldEmit> {
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let go_elem = go_type(f)?;
    let count = (inst.end - inst.start + 1) as usize;

    let mut elems: Vec<String> = Vec::with_capacity(count);
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
        elems.push(decode_scalar_go(&element, file_off));
    }

    let list_literal = format!("[]{go_elem}{{{}}}", elems.join(", "));
    let camel = GoNaming.field_name(&f.name);

    Some(FieldEmit {
        field_name: camel,
        field_type: format!("[]{go_elem}"),
        decode_expr: list_literal,
        needs_binary: needs_binary(f),
        needs_strings: matches!(f.ty, FieldType::Ascii),
    })
}

// ─── encode (per-field setters; spec D1) ─────────────────────────────────────

fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

fn enc_bit_width(f: &PageField) -> u32 {
    f.bit_width.unwrap_or(match f.ty {
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        _ => 8,
    })
}

/// Encode body (tab-indented) — inverse of `decode_scalar_go` — writing `value`
/// into `buf`. Sub-byte fields use read-modify-write (Go `&^` = AND-NOT).
fn encode_scalar_go(f: &PageField, offset: i64) -> String {
    let o = offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    match &f.ty {
        FieldType::Bool => format!(
            "\tif value {{\n\t\tbuf[{o}] |= 1 << {bo}\n\t}} else {{\n\t\tbuf[{o}] &^= 1 << {bo}\n\t}}"
        ),
        FieldType::U2 | FieldType::U3 | FieldType::U4
        | FieldType::U5 | FieldType::U6 | FieldType::U7 => {
            let mask = (1u32 << enc_bit_width(f)) - 1;
            format!(
                "\tbuf[{o}] = (buf[{o}] &^ byte(({mask:#x}<<{bo})&0xFF)) | byte((value&{mask:#x})<<{bo})"
            )
        }
        FieldType::U8 | FieldType::I8 => format!("\tbuf[{o}] = byte(value)"),
        // Two-byte enum_ref values encode big-endian at the field width —
        // the exact inverse of decode_scalar_go's enum branch.
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("\tbinary.BigEndian.PutUint16(buf[{o}:], uint16(value))")
        }
        FieldType::EnumRef => format!("\tbuf[{o}] = byte(value)"),
        FieldType::U16Be => format!("\tbinary.BigEndian.PutUint16(buf[{o}:], uint16(value))"),
        FieldType::I16Be => {
            format!("\tbinary.BigEndian.PutUint16(buf[{o}:], uint16(int16(value)))")
        }
        FieldType::U16Le => format!("\tbinary.LittleEndian.PutUint16(buf[{o}:], uint16(value))"),
        FieldType::U32Be => format!("\tbinary.BigEndian.PutUint32(buf[{o}:], uint32(value))"),
        FieldType::I32Be => {
            format!("\tbinary.BigEndian.PutUint32(buf[{o}:], uint32(int32(value)))")
        }
        FieldType::U64Le => format!("\tbinary.LittleEndian.PutUint64(buf[{o}:], value)"),
        FieldType::U64Be => format!("\tbinary.BigEndian.PutUint64(buf[{o}:], value)"),
        FieldType::F16Be | FieldType::X16Be => {
            format!("\tbinary.BigEndian.PutUint16(buf[{o}:], uint16(value))")
        }
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << enc_bit_width(f)) - 1;
                format!(
                    "\tbuf[{o}] = (buf[{o}] &^ byte(({mask:#x}<<{bo})&0xFF)) | byte((value&{mask:#x})<<{bo})"
                )
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("\tbuf[{o}] = byte(value)"),
                    2 => format!("\tbinary.BigEndian.PutUint16(buf[{o}:], uint16(value))"),
                    4 => format!("\tbinary.BigEndian.PutUint32(buf[{o}:], uint32(value))"),
                    _ => format!("\tbinary.BigEndian.PutUint64(buf[{o}:], uint64(value))"),
                }
            }
        }
        FieldType::Ascii => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!(
                "\ts := []byte(value)\n\tfor i := 0; i < {n}; i++ {{\n\t\tif i < len(s) {{\n\t\t\tbuf[{o}+i] = s[i]\n\t\t}} else {{\n\t\t\tbuf[{o}+i] = ' '\n\t\t}}\n\t}}"
            )
        }
        FieldType::Bytes | FieldType::Composite => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("\tcopy(buf[{o}:{}], value)", o + n)
        }
    }
}

/// Emit a type-prefixed `Set<Type><Field>` for a writable scalar field. The Go
/// package is flat, so setters carry the type-name prefix like the decoders.
/// Instanced Go setters are deferred (Wave 2 follow-up).
fn emit_setter_go(f: &PageField, base: i64, type_name: &str) -> Option<String> {
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
    let gotype = go_type(f)?;
    let field = GoNaming.field_name(&f.name);

    // Scalar.
    if f.offset_expr.is_none() && f.instances.is_none() {
        let offset = f.offset? as i64 - base;
        if offset < 0 {
            return None;
        }
        let body = encode_scalar_go(f, offset);
        return Some(format!(
            "\nfunc Set{type_name}{field}(buf []byte, value {gotype}) {{\n{body}\n}}\n"
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
            let body: String = encode_scalar_go(&element, offset)
                .lines()
                .map(|l| format!("\t{l}"))
                .collect::<Vec<_>>()
                .join("\n");
            arms.push_str(&format!("\tcase {n}:\n{body}\n"));
        }
        return Some(format!(
            "\nfunc Set{type_name}{field}(buf []byte, index int, value {gotype}) {{\n\tswitch index {{\n{arms}\t}}\n}}\n"
        ));
    }
    None
}

pub fn emit_page(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let type_name = &schema.id;
    let func_name = format!("Decode{type_name}");
    let min_size = schema.size_bytes.unwrap_or(128);

    let mut fields = Vec::new();
    let mut needs_binary = false;
    let mut needs_strings = false;

    for f in schema.value_fields() {
        let emit_opt = if let Some(instances) = &f.instances {
            emit_instanced_field(f, instances, base)
        } else {
            emit_scalar_field(f, base)
        };

        if let Some(emit) = emit_opt {
            needs_binary |= emit.needs_binary;
            needs_strings |= emit.needs_strings;
            fields.push(emit);
        } else {
            panic!("{}: cannot emit field {}", schema.id, f.name);
        }
    }

    let decls = if fields.is_empty() {
        "\t_ struct{}\n".to_string()
    } else {
        let max_field_len = fields.iter().map(|f| f.field_name.len()).max().unwrap_or(0);
        let mut d = String::new();
        for f in &fields {
            let pad = " ".repeat(max_field_len - f.field_name.len() + 1);
            d.push_str(&format!("\t{}{pad}{}\n", f.field_name, f.field_type));
        }
        d
    };

    let inits = if fields.is_empty() {
        String::new()
    } else {
        let max_key_len = fields
            .iter()
            .map(|f| f.field_name.len() + 1)
            .max()
            .unwrap_or(0);
        let mut ini = String::new();
        for f in &fields {
            let pad = " ".repeat(max_key_len - (f.field_name.len() + 1) + 1);
            ini.push_str(&format!("\t\t{}:{pad}{},\n", f.field_name, f.decode_expr));
        }
        ini
    };

    let mut imports = vec!["\"fmt\"".to_string()];
    if needs_binary {
        imports.push("\"encoding/binary\"".to_string());
    }
    if needs_strings {
        imports.push("\"strings\"".to_string());
    }
    imports.sort();
    let import_block = format!("import (\n\t{}\n)\n", imports.join("\n\t"));

    let mut out = format!(
        concat!(
            "// Generated by openxcvr-codegen. DO NOT EDIT.\n",
            "package generated\n\n",
            "{import_block}\n",
            "type {type_name} struct {{\n",
            "{decls}",
            "}}\n\n",
            "func {func_name}(buf []byte) (*{type_name}, error) {{\n",
            "\tif len(buf) < {min_size} {{\n",
            "\t\treturn nil, fmt.Errorf(\"expected >={min_size} bytes, got %d\", len(buf))\n",
            "\t}}\n",
            "\treturn &{type_name}{{\n",
            "{inits}",
            "\t}}, nil\n",
            "}}\n",
        ),
        import_block = import_block,
        type_name = type_name,
        func_name = func_name,
        decls = decls,
        min_size = min_size,
        inits = inits,
    );
    for f in schema.value_fields() {
        if let Some(s) = emit_setter_go(f, base, type_name) {
            out.push_str(&s);
        }
    }

    // checksum recompute: emit RecomputeXxxChecksums() for every page that has
    // ≥1 sum8 checksum field.
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
            "\nfunc Recompute{type_name}Checksums(buf []byte) {{\n"
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
            // Go slice upper bound is exclusive: buf[cov_start:cov_end+1] == Rust buf[cov_start..=cov_end]
            let cov_end_excl = cov_end + 1;
            out.push_str(&format!(
                "\t{{\n\t\tvar c byte\n\t\tfor _, b := range buf[{cov_start}:{cov_end_excl}] {{\n\t\t\tc += b\n\t\t}}\n\t\tbuf[{chk_idx}] = c\n\t}}\n"
            ));
        }
        out.push_str("}\n");
    }

    super::format_go(&out)
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod pages_tests;
