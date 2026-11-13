use crate::expr::compute;
use crate::ir::pages::{page_base, FieldType, InstancesDesc, PageField, PageSchema};

// ─── type helpers ─────────────────────────────────────────────────────────────

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

pub(super) fn py_type(f: &PageField) -> Option<&'static str> {
    match &f.ty {
        FieldType::Bool => Some("bool"),
        FieldType::U2 | FieldType::U3 | FieldType::U4
        | FieldType::U5 | FieldType::U6 | FieldType::U7
        | FieldType::U8 | FieldType::I8
        | FieldType::EnumRef  // stored as raw int; hand-written layer does IntEnum
        | FieldType::U16Be | FieldType::I16Be | FieldType::U16Le
        | FieldType::U32Be | FieldType::I32Be | FieldType::U64Le | FieldType::U64Be
        | FieldType::F16Be
        | FieldType::X16Be | FieldType::Bitmap => Some("int"),
        FieldType::Ascii => Some("str"),
        FieldType::Bytes | FieldType::Composite => Some("bytes"),
    }
}

pub(super) fn field_needs_struct(f: &PageField) -> bool {
    matches!(
        f.ty,
        FieldType::I8
            | FieldType::U16Be
            | FieldType::I16Be
            | FieldType::U16Le
            | FieldType::U32Be
            | FieldType::I32Be
            | FieldType::U64Le
            | FieldType::U64Be
            | FieldType::F16Be
            | FieldType::X16Be
    ) || (f.ty == FieldType::EnumRef && f.size_bytes == Some(2))
}

pub(super) fn uses_bit_offset(f: &PageField) -> bool {
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

// ─── decode expression builders ───────────────────────────────────────────────

pub(super) fn decode_scalar(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bit_offset = f.bit_offset.unwrap_or(0);
    let bit_width = bit_width(f);

    match &f.ty {
        FieldType::Bool => {
            format!("bool((buf[{o}] >> {bit_offset}) & 0x1)")
        }
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bit_width) - 1;
            if bit_offset == 0 {
                format!("buf[{o}] & 0x{mask:X}")
            } else {
                format!("(buf[{o}] >> {bit_offset}) & 0x{mask:X}")
            }
        }
        FieldType::U8 => format!("buf[{o}]"),
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 and SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; read the full width (mirrors the Rust
        // emitter's decode_at enum branch).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("struct.unpack_from('>H', buf, {o})[0]")
        }
        FieldType::EnumRef => format!("buf[{o}]"),
        FieldType::I8 => format!("struct.unpack_from('b', buf, {o})[0]"),
        FieldType::U16Be => format!("struct.unpack_from('>H', buf, {o})[0]"),
        FieldType::I16Be => format!("struct.unpack_from('>h', buf, {o})[0]"),
        FieldType::U16Le => format!("struct.unpack_from('<H', buf, {o})[0]"),
        FieldType::U32Be => format!("struct.unpack_from('>I', buf, {o})[0]"),
        FieldType::I32Be => format!("struct.unpack_from('>i', buf, {o})[0]"),
        FieldType::U64Le => format!("struct.unpack_from('<Q', buf, {o})[0]"),
        FieldType::U64Be => format!("struct.unpack_from('>Q', buf, {o})[0]"),
        FieldType::F16Be | FieldType::X16Be => format!("struct.unpack_from('>H', buf, {o})[0]"),
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bit_width) - 1;
                if bit_offset == 0 {
                    format!("buf[{o}] & 0x{mask:X}")
                } else {
                    format!("(buf[{o}] >> {bit_offset}) & 0x{mask:X}")
                }
            } else {
                let size = f.size_bytes.unwrap_or(1) as usize;
                if size == 1 {
                    format!("buf[{o}]")
                } else {
                    let end = o + size;
                    format!("int.from_bytes(buf[{o}:{end}], 'big')")
                }
            }
        }
        FieldType::Ascii => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            let end = o + size;
            format!("buf[{o}:{end}].decode('ascii', errors='replace').rstrip()")
        }
        FieldType::Bytes | FieldType::Composite => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            let end = o + size;
            format!("bytes(buf[{o}:{end}])")
        }
    }
}

// ─── simple scalar field ──────────────────────────────────────────────────────

struct FieldEmit {
    decl: String,
    unpack: Option<String>, // pre-decode line (struct.unpack_from)
    init: String,           // value expression in cls(...)
}

fn emit_scalar_field(f: &PageField, base: i64) -> Option<FieldEmit> {
    if f.offset.is_none() || f.offset_expr.is_some() || f.instances.is_some() {
        return None;
    }
    // Raw byte-like and text fields need a known extent.
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let py = py_type(f)?;
    let flat = f.offset.unwrap() as i64;
    let file_off = flat - base;
    if file_off < 0 {
        return None;
    }
    let decode_expr = decode_scalar(f, file_off);

    Some(FieldEmit {
        decl: format!("{}: {py}", f.name),
        unpack: None,
        init: decode_expr,
    })
}

pub(super) fn resolve_offset(f: &PageField, inst: &InstancesDesc, i: i64) -> Option<i64> {
    if let Some(expr) = &f.offset_expr {
        if expr.is_empty() {
            return f.offset.map(|o| o as i64);
        }
        compute(expr, &inst.index, i).ok()
    } else {
        f.offset.map(|o| o as i64)
    }
}

pub(super) fn resolve_bit_offset(f: &PageField, inst: &InstancesDesc, i: i64) -> Option<i64> {
    if let Some(expr) = &f.bit_offset_expr {
        if expr.is_empty() {
            return Some(f.bit_offset.unwrap_or(0) as i64);
        }
        compute(expr, &inst.index, i).ok()
    } else {
        Some(f.bit_offset.unwrap_or(0) as i64)
    }
}

// ─── instanced array field ────────────────────────────────────────────────────

fn emit_instanced_field(f: &PageField, inst: &InstancesDesc, base: i64) -> Option<FieldEmit> {
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let py = py_type(f)?;
    let count = (inst.end - inst.start + 1) as usize;

    // Evaluate each element's decode expression
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
        elems.push(decode_scalar(&element, file_off));
    }

    let list_literal = format!("[{}]", elems.join(", "));
    let var = format!("{}_vals", f.name);

    Some(FieldEmit {
        decl: format!("{}: list[{py}]", f.name),
        unpack: Some(format!("{var} = {list_literal}")),
        init: var,
    })
}

// ─── encode (per-field setters; spec D1) ───────────────────────────────────────

fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

/// Encode statement(s) — inverse of `decode_scalar` — writing `value` into the
/// mutable `buf` (a bytearray). Sub-byte fields use read-modify-write.
fn encode_scalar(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    let bw = bit_width(f);
    match &f.ty {
        FieldType::Bool => {
            format!("buf[{o}] = (buf[{o}] & ~(1 << {bo})) | ((int(value) & 1) << {bo})")
        }
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bw) - 1;
            format!(
                "buf[{o}] = (buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo})"
            )
        }
        FieldType::U8 => format!("buf[{o}] = value & 0xFF"),
        // Two-byte enum_ref values encode big-endian at the field width,
        // the exact inverse of the decode branch (schema: size_bytes 2).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("struct.pack_into('>H', buf, {o}, value)")
        }
        FieldType::EnumRef => format!("buf[{o}] = value & 0xFF"),
        FieldType::I8 => format!("struct.pack_into('b', buf, {o}, value)"),
        FieldType::U16Be => format!("struct.pack_into('>H', buf, {o}, value)"),
        FieldType::I16Be => format!("struct.pack_into('>h', buf, {o}, value)"),
        FieldType::U16Le => format!("struct.pack_into('<H', buf, {o}, value)"),
        FieldType::U32Be => format!("struct.pack_into('>I', buf, {o}, value)"),
        FieldType::I32Be => format!("struct.pack_into('>i', buf, {o}, value)"),
        FieldType::U64Le => format!("struct.pack_into('<Q', buf, {o}, value)"),
        FieldType::U64Be => format!("struct.pack_into('>Q', buf, {o}, value)"),
        FieldType::F16Be | FieldType::X16Be => format!("struct.pack_into('>H', buf, {o}, value)"),
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bw) - 1;
                format!("buf[{o}] = (buf[{o}] & ~(0x{mask:X} << {bo})) | ((value & 0x{mask:X}) << {bo})")
            } else {
                let size = f.size_bytes.unwrap_or(1) as usize;
                if size == 1 {
                    format!("buf[{o}] = value & 0xFF")
                } else {
                    format!("buf[{o}:{}] = value.to_bytes({size}, 'big')", o + size)
                }
            }
        }
        FieldType::Ascii => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!(
                "buf[{o}:{}] = value.encode('ascii')[:{n}].ljust({n}, b' ')",
                o + n
            )
        }
        FieldType::Bytes | FieldType::Composite => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!(
                "buf[{o}:{}] = bytes(value)[:{n}].ljust({n}, b'\\x00')",
                o + n
            )
        }
    }
}

/// Emit a `set_<field>` @staticmethod for a writable, statically-placed field.
fn emit_setter(f: &PageField, base: i64) -> Option<String> {
    if !is_writable(f) {
        return None;
    }
    let py = py_type(f)?;

    // Scalar.
    if f.offset_expr.is_none() && f.instances.is_none() {
        if matches!(
            f.ty,
            FieldType::Ascii | FieldType::Bytes | FieldType::Composite
        ) && f.size_bytes.is_none()
        {
            return None;
        }
        let off = f.offset? as i64 - base;
        if off < 0 {
            return None;
        }
        let body = encode_scalar(f, off);
        return Some(format!(
            "\n    @staticmethod\n    def set_{}(buf: bytearray, value: {py}) -> None:\n        {body}\n",
            f.name
        ));
    }

    // Instanced.
    if let Some(inst) = &f.instances {
        let mut arms = String::new();
        for (n, i) in (inst.start..=inst.end).enumerate() {
            let off = resolve_offset(f, inst, i)? - base;
            let bit_offset = resolve_bit_offset(f, inst, i)?;
            if off < 0 || bit_offset < 0 {
                return None;
            }
            let mut element = f.clone();
            element.bit_offset = if uses_bit_offset(f) {
                Some(bit_offset as u32)
            } else {
                None
            };
            element.bit_offset_expr = None;
            let kw = if n == 0 { "if" } else { "elif" };
            arms.push_str(&format!(
                "        {kw} index == {n}:\n            {}\n",
                encode_scalar(&element, off)
            ));
        }
        return Some(format!(
            "\n    @staticmethod\n    def set_{}(buf: bytearray, index: int, value: {py}) -> None:\n{arms}",
            f.name
        ));
    }
    None
}

// ─── main emitter ─────────────────────────────────────────────────────────────

pub fn emit_page(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let class_name = &schema.id;
    let min_size = schema.size_bytes.unwrap_or(128);

    let mut decls = String::new();
    let mut unpacks = String::new();
    let mut inits = String::new();
    let mut needs_struct = false;

    for f in schema.value_fields() {
        let emit_opt = if let Some(instances) = &f.instances {
            emit_instanced_field(f, instances, base)
        } else {
            emit_scalar_field(f, base)
        };

        if let Some(emit) = emit_opt {
            decls.push_str(&format!("    {}\n", emit.decl));
            if let Some(u) = emit.unpack {
                unpacks.push_str(&format!("        {u}\n"));
            }
            inits.push_str(&format!("            {} = {},\n", f.name, emit.init));
            if field_needs_struct(f) {
                needs_struct = true;
            }
        } else {
            panic!("{}: cannot emit field {}", schema.id, f.name);
        }
    }

    if decls.is_empty() {
        decls.push_str("    pass\n");
    }

    // encode: per-field setters for writable fields (RMW for bit-fields).
    let mut setters = String::new();
    for f in schema.value_fields() {
        if let Some(s) = emit_setter(f, base) {
            setters.push_str(&s);
        }
    }

    // checksum recompute: emit recompute_checksums() for pages with ≥1 sum8 field.
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
        setters.push_str(
            "\n    @staticmethod\n    def recompute_checksums(buf: bytearray) -> None:\n",
        );
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
            let end_excl = cov_end + 1;
            setters.push_str(&format!(
                "        buf[{chk_idx}] = sum(buf[{cov_start}:{end_excl}]) & 0xFF\n"
            ));
        }
    }

    let mut imports =
        "from __future__ import annotations\nfrom dataclasses import dataclass\n".to_string();
    if needs_struct {
        imports.push_str("import struct\n");
    }

    format!(
        concat!(
            "# Generated by openxcvr-codegen. DO NOT EDIT.\n",
            "{imports}\n\n",
            "@dataclass\n",
            "class {class_name}:\n",
            "{decls}\n",
            "    @classmethod\n",
            "    def decode(cls, buf: bytes) -> {class_name}:\n",
            "        if len(buf) < {min_size}:\n",
            "            raise ValueError(f\"expected >={min_size} bytes, got {{len(buf)}}\")\n",
            "{unpacks}",
            "        return cls(\n",
            "{inits}",
            "        )\n",
            "{setters}",
        ),
        imports = imports,
        class_name = class_name,
        decls = decls,
        min_size = min_size,
        unpacks = unpacks,
        inits = inits,
        setters = setters,
    )
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod pages_tests;
