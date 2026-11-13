use std::path::Path;

use crate::expr::compute;
use crate::ir::naming::{NamingPolicy, RustNaming};
use crate::ir::pages::{page_base, FieldType, InstancesDesc, PageField, PageSchema};

// ─── helpers ──────────────────────────────────────────────────────────────────

fn bit_width(ty: &FieldType) -> u32 {
    match ty {
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        _ => 8,
    }
}

/// Rust type for a scalar field (Some) or None if special handling needed.
fn scalar_rust_type(f: &PageField) -> Option<&str> {
    if f.enum_ref.is_some() {
        return None; // returned separately via enum_ref
    }
    match &f.ty {
        FieldType::Bool => Some("bool"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7
        | FieldType::U8 => Some("u8"),
        FieldType::I8 => Some("i8"),
        FieldType::U16Be | FieldType::U16Le => Some("u16"),
        FieldType::I16Be => Some("i16"),
        FieldType::U32Be => Some("u32"),
        FieldType::I32Be => Some("i32"),
        FieldType::U64Le | FieldType::U64Be => Some("u64"),
        FieldType::F16Be | FieldType::X16Be => Some("u16"),
        FieldType::EnumRef => None,
        _ => None,
    }
}

/// Effective Rust type string for a field (enum name if present, else scalar,
/// or [u8; N] for ascii/bytes with a known size).
fn effective_type(f: &PageField) -> Option<String> {
    if let Some(name) = &f.enum_ref {
        return Some(name.clone());
    }
    match &f.ty {
        FieldType::Ascii | FieldType::Bytes => f.size_bytes.map(|n| format!("[u8; {n}]")),
        _ => scalar_rust_type(f).map(|s| s.to_string()),
    }
}

/// Decode expression for a single (offset, bit_offset) pair.
///
/// `emit_rev` selects the enum decode site: `None` is the flat union page
/// (the deliberately revision-unqualified `Enum::from(..)` view), while
/// `Some(rev)` is a per-revision page under `v{rev}/`, whose enum fields
/// decode through the revision-qualified `Enum::from_rev(rev_byte, ..)`
/// (`rev_byte` = `rev.family_rev_byte()`: packed-BCD CMIS major byte or SFF
/// compliance code) so a raw reserved at that
/// revision never borrows a later revision's named state.
fn decode_at(
    f: &PageField,
    offset: i64,
    bit_offset: i64,
    emit_rev: Option<crate::ir::version::Rev>,
) -> String {
    let o = offset as usize;

    if let Some(enum_name) = &f.enum_ref {
        // For multi-byte enum fields, read the raw value at the correct width.
        let (raw, cast) = match f.size_bytes {
            Some(2) => (
                format!("u16::from_be_bytes([buf[{o}], buf[{}]])", o + 1),
                "u16",
            ),
            _ => (raw_numeric_at(f, offset, bit_offset), "u8"),
        };
        // Total `From` (see emitters/rust/enums.rs): unmapped values fold into the
        // enum's `Unknown(repr)` catch-all instead of panicking — decode stays
        // total on untrusted EEPROM. The per-revision form layers the schema's
        // entry since/until windows on top of that totality.
        return match emit_rev {
            None => format!("{enum_name}::from({raw} as {cast})"),
            Some(rev) => format!(
                "{enum_name}::from_rev({:#04x}, {raw} as {cast})",
                rev.family_rev_byte()
            ),
        };
    }

    match &f.ty {
        FieldType::Bool => {
            format!("(buf[{o}] >> {bit_offset}) & 1 != 0")
        }
        FieldType::U2 | FieldType::U3 | FieldType::U4
        | FieldType::U5 | FieldType::U6 | FieldType::U7 => {
            let bits = bit_width(&f.ty);
            let mask = (1u32 << bits) - 1;
            if bit_offset == 0 {
                format!("buf[{o}] & {mask:#04x}")
            } else {
                format!("(buf[{o}] >> {bit_offset}) & {mask:#04x}")
            }
        }
        FieldType::U8 | FieldType::EnumRef => format!("buf[{o}]"),
        FieldType::I8   => format!("buf[{o}] as i8"),
        FieldType::U16Be => format!("u16::from_be_bytes([buf[{}], buf[{}]])", o, o+1),
        FieldType::I16Be => format!("i16::from_be_bytes([buf[{}], buf[{}]])", o, o+1),
        FieldType::U16Le => format!("u16::from_le_bytes([buf[{}], buf[{}]])", o, o+1),
        FieldType::U32Be => format!(
            "u32::from_be_bytes([buf[{}], buf[{}], buf[{}], buf[{}]])",
            o, o+1, o+2, o+3
        ),
        FieldType::I32Be => format!(
            "i32::from_be_bytes([buf[{}], buf[{}], buf[{}], buf[{}]])",
            o, o+1, o+2, o+3
        ),
        FieldType::U64Le => format!(
            "u64::from_le_bytes([buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}]])",
            o, o+1, o+2, o+3, o+4, o+5, o+6, o+7
        ),
        FieldType::U64Be => format!(
            "u64::from_be_bytes([buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}], buf[{}]])",
            o, o+1, o+2, o+3, o+4, o+5, o+6, o+7
        ),
        FieldType::F16Be | FieldType::X16Be => {
            format!("u16::from_be_bytes([buf[{}], buf[{}]])", o, o+1)
        }
        FieldType::Ascii | FieldType::Bytes => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("<[u8; {}]>::try_from(&buf[{}..{}]).unwrap()", n, o, o + n)
        }
        _ => format!("buf[{o}] /* unsupported type */"),
    }
}

fn raw_numeric_at(f: &PageField, offset: i64, bit_offset: i64) -> String {
    let o = offset as usize;
    match &f.ty {
        FieldType::Bool => format!("(buf[{o}] >> {bit_offset}) & 1"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let bits = bit_width(&f.ty);
            let mask = (1u32 << bits) - 1;
            if bit_offset == 0 {
                format!("buf[{o}] & {mask:#04x}")
            } else {
                format!("(buf[{o}] >> {bit_offset}) & {mask:#04x}")
            }
        }
        _ => format!("buf[{o}]"),
    }
}

// ─── encode (per-field setters; spec D1) ─────────────────────────────────────

/// A field gets a setter only if it is ever writable (`is_ever_writable`
/// flag or an `rw*` access mode).
fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

/// Encode statement — the exact inverse of `decode_at`. Writes `value` into
/// `buf` at `(offset, bit_offset)`. Sub-byte fields use read-modify-write so
/// neighbouring bits in the byte are preserved.
fn encode_at(f: &PageField, offset: i64, bit_offset: i64) -> String {
    let o = offset as usize;

    if f.enum_ref.is_some() {
        return match f.size_bytes {
            Some(2) => format!(
                "buf[{o}..{}].copy_from_slice(&(value.repr() as u16).to_be_bytes());",
                o + 2
            ),
            // Sub-byte enum fields (bool/u2..u7 backing) get the same masked
            // read-modify-write as the non-enum branch below, so neighbouring
            // bits survive and the value lands at the field's bit span — the
            // decode side already extracts through `raw_numeric_at`'s masks,
            // and encode is its exact inverse. `repr()` is masked to the field
            // width, matching non-enum raw-setter overflow semantics (an
            // out-of-range repr folds into the width instead of spilling into
            // neighbours). Full-byte enums (u8/enum_ref) and u16 enums keep
            // their whole-byte / big-endian behaviour unchanged.
            _ => match &f.ty {
                FieldType::Bool => format!(
                    "buf[{o}] = (buf[{o}] & !(1u8 << {bit_offset})) | (((value.repr() as u8) & 1u8) << {bit_offset});"
                ),
                FieldType::U2
                | FieldType::U3
                | FieldType::U4
                | FieldType::U5
                | FieldType::U6
                | FieldType::U7 => {
                    let bits = bit_width(&f.ty);
                    let mask = (1u32 << bits) - 1;
                    format!(
                        "buf[{o}] = (buf[{o}] & !({mask:#04x}u8 << {bit_offset})) | (((value.repr() as u8) & {mask:#04x}) << {bit_offset});"
                    )
                }
                _ => format!("buf[{o}] = value.repr() as u8;"),
            },
        };
    }

    match &f.ty {
        FieldType::Bool => format!(
            "buf[{o}] = (buf[{o}] & !(1u8 << {bit_offset})) | ((value as u8) << {bit_offset});"
        ),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let bits = bit_width(&f.ty);
            let mask = (1u32 << bits) - 1;
            format!(
                "buf[{o}] = (buf[{o}] & !({mask:#04x}u8 << {bit_offset})) | ((value & {mask:#04x}) << {bit_offset});"
            )
        }
        FieldType::U8 => format!("buf[{o}] = value;"),
        FieldType::I8 => format!("buf[{o}] = value as u8;"),
        FieldType::U16Be | FieldType::I16Be | FieldType::F16Be | FieldType::X16Be => {
            format!("buf[{o}..{}].copy_from_slice(&value.to_be_bytes());", o + 2)
        }
        FieldType::U16Le => {
            format!("buf[{o}..{}].copy_from_slice(&value.to_le_bytes());", o + 2)
        }
        FieldType::U32Be | FieldType::I32Be => {
            format!("buf[{o}..{}].copy_from_slice(&value.to_be_bytes());", o + 4)
        }
        FieldType::U64Le => {
            format!("buf[{o}..{}].copy_from_slice(&value.to_le_bytes());", o + 8)
        }
        FieldType::U64Be => {
            format!("buf[{o}..{}].copy_from_slice(&value.to_be_bytes());", o + 8)
        }
        FieldType::Ascii | FieldType::Bytes => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("buf[{o}..{}].copy_from_slice(&value);", o + n)
        }
        _ => "/* unsupported field type for encode */".to_string(),
    }
}

/// Emit `set_<field>` for a writable, statically-placed field. Instanced fields
/// get an `index` parameter dispatched through a match. Returns None for
/// non-writable / non-emittable (variants/composite/variable-length) fields.
///
/// Mutator contract (settled, hardening): the buffer parameter is the page's
/// fixed-size `&mut [u8; N]` (N = declared page size, mirroring the decode
/// side's `&[u8; N]`), never a caller-sized slice — a short buffer is a
/// compile error, not a runtime panic. Instanced setters validate the
/// index and return a typed error instead of silently ignoring it.
fn emit_setter(f: &PageField, base: i64, size: usize) -> Option<String> {
    if !is_writable(f) {
        return None;
    }

    // Scalar (no instances, no variants, static offset).
    if f.offset_expr.is_none() && f.instances.is_none() && f.variants.is_none() {
        if matches!(f.ty, FieldType::Ascii | FieldType::Bytes) && f.size_bytes.is_none() {
            return None;
        }
        let ty = effective_type(f)?;
        let fname = RustNaming.field_name(&f.name);
        let offset = f.offset? as i64 - base;
        if offset < 0 {
            return None;
        }
        let bit_offset = f.bit_offset.unwrap_or(0) as i64;
        let body = encode_at(f, offset, bit_offset);
        return Some(format!(
            "    pub fn set_{fname}(buf: &mut [u8; {size}], value: {ty}) {{\n        {body}\n    }}\n"
        ));
    }

    // Instanced: one match arm per index value.
    if let Some(inst) = &f.instances {
        if f.variants.is_some() {
            return None;
        }
        let elem_ty = effective_type(f)?;
        let fname = RustNaming.field_name(&f.name);
        let mut arms = String::new();
        for i in inst.start..=inst.end {
            let offset = resolve_offset(f, inst, i)? - base;
            let bit_offset = resolve_bit_offset(f, inst, i)?;
            if offset < 0 {
                return None;
            }
            let idx = (i - inst.start) as usize;
            let body = encode_at(f, offset, bit_offset);
            arms.push_str(&format!("            {idx} => {{ {body} Ok(()) }}\n"));
        }
        return Some(format!(
            "    pub fn set_{fname}(buf: &mut [u8; {size}], index: usize, value: {elem_ty}) -> Result<(), openxcvr_core::supplied::SpanError> {{\n        match index {{\n{arms}            _ => Err(openxcvr_core::supplied::SpanError::InvalidIndex(index as i64)),\n        }}\n    }}\n"
        ));
    }

    None
}

// ─── simple scalar (no instances) ────────────────────────────────────────────

fn simple_field_parts(
    f: &PageField,
    base: i64,
    emit_rev: Option<crate::ir::version::Rev>,
) -> Option<(String, String, String)> {
    if f.offset_expr.is_some() || f.instances.is_some() || f.variants.is_some() {
        return None;
    }
    let raw_offset = f.offset?;
    // ascii/bytes need size_bytes to be emitted
    if matches!(f.ty, FieldType::Ascii | FieldType::Bytes) && f.size_bytes.is_none() {
        return None;
    }
    let eff_type = effective_type(f)?;
    let fname = RustNaming.field_name(&f.name);
    let offset = raw_offset as i64 - base;
    let bit_offset = f.bit_offset.unwrap_or(0) as i64;
    let decode = decode_at(f, offset, bit_offset, emit_rev);
    Some((fname, eff_type, decode))
}

// ─── instanced field (instances: + offset_expr | fixed offset) ───────────────

/// Expands an instanced field to (field_name, type_str, array_init_expr).
/// Returns None if we can't handle it (e.g., variants or missing offset info).
fn instanced_field_parts(
    f: &PageField,
    base: i64,
    emit_rev: Option<crate::ir::version::Rev>,
) -> Option<(String, String, String)> {
    let inst = f.instances.as_ref()?;
    // Variants on instanced fields are not handled yet.
    if f.variants.is_some() {
        return None;
    }
    let eff_type = effective_type(f)?;
    let fname = RustNaming.field_name(&f.name);
    let count = (inst.end - inst.start + 1) as usize;

    // Compute (offset, bit_offset) for each index value; subtract page base
    // so buf indices are page-relative (0-based within the page slice).
    let mut elements: Vec<String> = Vec::with_capacity(count);
    for i in inst.start..=inst.end {
        let flat_offset = resolve_offset(f, inst, i)?;
        let bit_offset = resolve_bit_offset(f, inst, i)?;
        elements.push(decode_at(f, flat_offset - base, bit_offset, emit_rev));
    }

    let array_init = format!("[{}]", elements.join(", "));
    let array_type = format!("[{eff_type}; {count}]");
    Some((fname, array_type, array_init))
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

fn field_width(f: &PageField) -> usize {
    match f.ty {
        FieldType::Ascii | FieldType::Bytes => f.size_bytes.unwrap_or(1) as usize,
        FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::F16Be
        | FieldType::X16Be => 2,
        FieldType::U32Be | FieldType::I32Be => 4,
        FieldType::U64Le | FieldType::U64Be => 8,
        _ => f.size_bytes.unwrap_or(1) as usize,
    }
}

// ─── emitter ─────────────────────────────────────────────────────────────────
/// Emit the flat, rev-agnostic union page: every field the schema declares,
/// with enum fields decoding through the unqualified `Enum::from(..)`
/// (`from_rev`'s `emit_rev` is `None`).
pub fn emit_page(schema: &PageSchema) -> String {
    emit_page_inner(schema, Some(crate::ir::version::Rev::V5_3), None)
}

// Preserve the delivered 4.0 tables; pages 01h, 9Fh, and 11h also have
// source-qualified 5.3 specializations. Other specializations retain their
// fail-closed surface.
pub(super) fn emit_page_for_revision(schema: &PageSchema, rev: crate::ir::version::Rev) -> String {
    emit_page_inner(
        schema,
        (rev == crate::ir::version::Rev::V4_0
            || (matches!(
                schema.id.as_str(),
                "CmisPage01hAdvertising" | "CmisPage9FhCdbMessage" | "CmisPage11hLaneStatus"
            ) && rev == crate::ir::version::Rev::V5_3))
            .then_some(rev),
        // A per-revision page always decodes its enum fields through
        // `Enum::from_rev(rev_byte, ..)` — including the specializations
        // that do not emit read_classifications.
        Some(rev),
    )
}

fn emit_page_inner(
    schema: &PageSchema,
    read_revision: Option<crate::ir::version::Rev>,
    emit_rev: Option<crate::ir::version::Rev>,
) -> String {
    let mut out = String::new();
    out.push_str("// Generated by openxcvr-codegen. DO NOT EDIT.\n");
    out.push_str(&format!("// Source: {}\n", schema.id));
    out.push('\n');
    // Enum types referenced in this page live in sibling modules under the
    // same parent (typically `generated/`). Wildcard import pulls them all in.
    out.push_str("use super::*;\n");
    out.push('\n');

    let struct_name = RustNaming.type_name(&schema.id);
    // Subtract the normalized base from all flat field offsets so buffer indices
    // are page-relative (0-based within the per-page .bin slice).
    let page_base = page_base(schema);
    // Buffer size = the declared page size, not a derived max-flat-offset.
    let size = schema.size_bytes.unwrap_or(128) as usize;

    // Classify fields into: simple scalar, instanced, or skipped.
    #[derive(Debug)]
    enum FieldClass {
        Simple(String, String, String),    // name, type, decode
        Instanced(String, String, String), // name, type[], array_init
        Skipped,
    }

    let classified: Vec<(_, FieldClass)> = schema
        .value_fields()
        .map(|f| {
            let cls = if let Some(parts) = simple_field_parts(f, page_base, emit_rev) {
                FieldClass::Simple(parts.0, parts.1, parts.2)
            } else if let Some(parts) = instanced_field_parts(f, page_base, emit_rev) {
                FieldClass::Instanced(parts.0, parts.1, parts.2)
            } else {
                FieldClass::Skipped
            };
            (f, cls)
        })
        .collect();

    let emittable: Vec<_> = classified
        .iter()
        .filter(|(_, c)| !matches!(c, FieldClass::Skipped))
        .collect();
    let skipped: Vec<_> = classified
        .iter()
        .filter(|(f, c)| {
            matches!(c, FieldClass::Skipped) && {
                // Only report fields that had *something* to emit but were blocked
                f.offset.is_some() || f.offset_expr.is_some()
            }
        })
        .collect();

    // Struct
    // Shared schema-recorded caveats live once on the struct (not repeated
    // per field); they only make sense where field documentation follows.
    if !emittable.is_empty() {
        out.push_str(&super::field_docs::struct_doc_lines());
    }
    out.push_str("#[derive(Debug, Clone)]\n");
    out.push_str(&format!("pub struct {struct_name} {{\n"));
    for (f, cls) in &emittable {
        match cls {
            FieldClass::Simple(name, ty, _) | FieldClass::Instanced(name, ty, _) => {
                // Schema-recorded rustdoc, from the same resolved PageField
                // that produced the declaration — selected fields only, so
                // skipped/filtered fields never leave dangling docs.
                out.push_str(&super::field_docs::field_doc_lines(f));
                out.push_str(&format!("    pub {name}: {ty},\n"));
            }
            FieldClass::Skipped => {}
        }
    }
    out.push_str("}\n\n");

    // decode() constructor
    let buf_param = if emittable.is_empty() { "_buf" } else { "buf" };
    out.push_str(&format!("impl {struct_name} {{\n"));
    out.push_str(&format!(
        "    pub fn decode({buf_param}: &[u8; {size}]) -> Self {{\n"
    ));
    out.push_str("        Self {\n");
    for (_, cls) in &emittable {
        match cls {
            FieldClass::Simple(name, _, decode) => {
                out.push_str(&format!("            {name}: {decode},\n"));
            }
            FieldClass::Instanced(name, _, array_init) => {
                out.push_str(&format!("            {name}: {array_init},\n"));
            }
            FieldClass::Skipped => {}
        }
    }
    out.push_str("        }\n");
    out.push_str("    }\n");
    // Supplied-range accessors on the pages used by identity/layout discovery.
    // Offsets are absolute EEPROM-window offsets, never scratch-page indices.
    if matches!(
        schema.id.as_str(),
        "CmisPage00hAdministrative"
            | "CmisLowerMemory"
            | "Sff8636LowerMemory"
            | "Sff8636Page00hUpperAdministrative"
            | "page_a0h_base_memory"
    ) {
        for (f, cls) in &emittable {
            if matches!(
                schema.id.as_str(),
                "CmisPage00hAdministrative"
                    | "Sff8636Page00hUpperAdministrative"
                    | "page_a0h_base_memory"
            ) && !matches!(
                f.name.as_str(),
                "vendor_name"
                    | "vendor_pn"
                    | "vendor_rev"
                    | "vendor_sn"
                    | "vendor_oui"
                    | "date_code"
                    | "date_code_year"
                    | "date_code_month"
                    | "date_code_day_of_month"
                    | "date_code_lot_code"
                    | "sff8024_identifier_copy"
                    | "options"
            ) {
                continue;
            }
            if matches!(schema.id.as_str(), "CmisLowerMemory" | "Sff8636LowerMemory")
                && !matches!(f.name.as_str(), "memory_model" | "flat_memory")
                && !(schema.id == "CmisLowerMemory" && f.name == "module_state")
            {
                continue;
            }
            if schema.id == "page_a0h_base_memory" && f.instances.is_some() && f.name != "options" {
                continue;
            }
            match cls {
                FieldClass::Simple(name, ty, _) => {
                    let offset = f.offset.unwrap();
                    let len = field_width(f);
                    // Supplied-range accessors keep the flat union decode
                    // (`Enum::from(..)`, `emit_rev` = None): they are the
                    // identity/layout discovery surface, not a per-revision
                    // view, and stay byte-identical here.
                    let decode = decode_at(f, 0, f.bit_offset.unwrap_or(0) as i64, None);
                    out.push_str(&format!("\n    pub const {upper}_RANGE: (usize, usize) = ({offset}, {len});\n    pub fn decode_{name}_supplied(bytes: &[u8], offset: usize) -> Result<{ty}, openxcvr_core::supplied::SpanError> {{\n        let buf = openxcvr_core::supplied::field_bytes(bytes, offset, {offset}, {len})?;\n        Ok({decode})\n    }}\n", upper = name.to_uppercase()));
                }
                FieldClass::Instanced(name, _, _) => {
                    let inst = f.instances.as_ref().unwrap();
                    let ty = effective_type(f).unwrap();
                    let mut arms = String::new();
                    for index in inst.start..=inst.end {
                        if f.name == "options" && index != 2 {
                            continue;
                        }
                        let offset = resolve_offset(f, inst, index).unwrap();
                        let len = field_width(f);
                        // Flat union decode, as above: `emit_rev` = None.
                        let decode =
                            decode_at(f, 0, resolve_bit_offset(f, inst, index).unwrap(), None);
                        arms.push_str(&format!("            {index} => {{ let buf = openxcvr_core::supplied::field_bytes(bytes, offset, {offset}, {len})?; Ok({decode}) }},\n"));
                    }
                    out.push_str(&format!("\n    pub fn decode_{name}_supplied(bytes: &[u8], offset: usize, index: i64) -> Result<{ty}, openxcvr_core::supplied::SpanError> {{\n        match index {{\n{arms}            _ => Err(openxcvr_core::supplied::SpanError::InvalidIndex(index)),\n        }}\n    }}\n"));
                }
                FieldClass::Skipped => {}
            }
        }
    }
    if let Some(revision) = read_revision.filter(|revision| {
        matches!(
            schema.id.as_str(),
            "CmisLowerMemory"
                | "CmisPage00hAdministrative"
                | "CmisPage01hAdvertising"
                | "CmisPage11hLaneStatus"
        ) || (schema.id == "CmisPage9FhCdbMessage" && *revision == crate::ir::version::Rev::V5_3)
    }) {
        out.push_str(&emit_read_classifications(schema, revision));
    }
    // encode: per-field setters for writable fields (read-modify-write for
    // bit-fields), the exact inverse of decode (spec D1).
    for f in schema.value_fields() {
        if let Some(setter) = emit_setter(f, page_base, size) {
            out.push('\n');
            out.push_str(&setter);
        }
    }

    // checksum recompute: emit recompute_checksums() for every page that has
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
        out.push('\n');
        out.push_str(&format!(
            "    pub fn recompute_checksums(buf: &mut [u8; {size}]) {{\n"
        ));
        for (flat_offset, flat_start, flat_end) in &chksum_fields {
            // flat → page-relative. Validate at GENERATION time: a
            // covers_range dipping below page_base would emit syntactically
            // invalid Rust (negative index); one past `size` would panic at
            // runtime. Schema errors must fail here with a clear message.
            let chk_idx = *flat_offset as i64 - page_base;
            let cov_start = *flat_start as i64 - page_base;
            let cov_end = *flat_end as i64 - page_base;
            assert!(
                chk_idx >= 0
                    && chk_idx < size as i64
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < size as i64,
                "{}: checksum at flat {flat_offset} covering flat [{flat_start},{flat_end}] \
                 out of bounds for page_base {page_base}, size {size}",
                schema.id
            );
            out.push_str(&format!(
                "        buf[{chk_idx}] = buf[{cov_start}..={cov_end}].iter().fold(0u8, |a, b| a.wrapping_add(*b));\n"
            ));
        }
        out.push_str("    }\n");
    }

    out.push_str("}\n");

    if !skipped.is_empty() {
        out.push('\n');
        out.push_str(&format!(
            "// {} field(s) skipped (variants/composite/variable-length):\n",
            skipped.len()
        ));
        for (f, _) in &skipped {
            out.push_str(&format!("//   {}\n", f.name));
        }
    }

    out
}

/// Emit a bounded footprint at the requested revision, never from
/// the flat union's unresolved permission metadata. Aggregate *covered* masks
/// first; OR-ing each field's uncovered mask would incorrectly leave sibling
/// constituents uncovered. Restrictive effects/unknown assertions are unioned:
/// no field's None can erase another field's restriction.
fn emit_read_classifications(schema: &PageSchema, revision: crate::ir::version::Rev) -> String {
    use crate::ir::pages::{ChangeSourceMetadata, ReadEffectMetadata};
    use crate::ir::version::{applicable_revs, resolve_field_for_rev, Rev};

    #[derive(Clone, Default, PartialEq, Eq)]
    struct Byte {
        covered: u8,
        readable: u8,
        unknown: u8,
        write_only: u8,
        effects: Vec<&'static str>,
        dynamic: bool,
    }
    let base = page_base(schema);
    let size = schema.size_bytes.unwrap_or(128) as usize;
    let mut table = vec![Byte::default(); size];
    for field in &schema.fields {
        if !applicable_revs(field, Rev::CMIS_ALL).contains(&revision) {
            continue;
        }
        let resolved = resolve_field_for_rev(field, revision);
        // Invalid/unresolvable addressing is a schema generation error, not
        // permission to silently drop a restrictive field from the table.
        let masks = resolved.read_effect_masks().unwrap_or_else(|error| {
            panic!("{}: cannot classify {}: {error}", schema.id, field.name)
        });
        for masks in masks {
            let index = i64::from(masks.offset) - base;
            assert!(
                index >= 0 && index < size as i64,
                "{}: classification offset {} outside page",
                schema.id,
                masks.offset
            );
            let byte = &mut table[index as usize];
            byte.covered |= masks.covered;
            byte.unknown |= masks.unknown;
            // Legacy readable flags may contradict the canonical permission.
            // Preserve the disagreement, never upgrade it to qualified None.
            if resolved
                .access
                .is_some_and(|access| access.is_readable() == Some(true))
                && resolved
                    .flags
                    .as_ref()
                    .is_some_and(|flags| flags.is_readable == Some(false))
            {
                byte.unknown |= masks.covered;
            }
            byte.write_only |= masks.write_only;
            byte.dynamic |= resolved.change_source() != Some(ChangeSourceMetadata::Static);
            for (effect, mask) in masks.known {
                if mask == 0 {
                    continue;
                }
                // A byte-scoped restrictive effect is physical-byte evidence,
                // including reserved constituents (e.g. CMIS 5.3 Table 8-3 COR).
                // None masks remain field-scoped, and explicit sibling unknown
                // or write-only assertions above are never erased by this union.
                byte.covered |= mask;
                byte.readable |= mask;
                let effect = match effect {
                    ReadEffectMetadata::None => "None",
                    ReadEffectMetadata::ClearOnRead => "ClearOnRead",
                    ReadEffectMetadata::AcknowledgeStartupInterrupt => {
                        "AcknowledgeStartupInterrupt"
                    }
                    ReadEffectMetadata::Unknown | ReadEffectMetadata::WriteOnly => {
                        unreachable!("known effect mask")
                    }
                };
                if !byte.effects.contains(&effect) {
                    byte.effects.push(effect);
                }
            }
        }
    }
    for byte in &mut table {
        // Deterministic set order, independent of source field ordering.
        byte.effects.sort_by_key(|effect| match *effect {
            "None" => 0,
            "ClearOnRead" => 1,
            _ => 2,
        });
    }
    let mut output = format!(
        "\n    /// CMIS {} per-byte classification, generated from access and read-effect metadata.\n    /// Indices are page-relative; missing/unknown assertions never qualify as None.\n    /// Unknown change-source metadata is conservatively dynamic (not cache-static).\n    pub fn read_classifications() -> [openxcvr_core::types::management::ByteClassification; {size}] {{\n        use openxcvr_core::types::management::{{ByteClassification, ChangeSource, ReadEffect}};\n        std::array::from_fn(|index| match index {{\n", revision.as_str()
    );
    let mut start = 0;
    while start < size {
        let mut end = start + 1;
        while end < size && table[end] == table[start] {
            end += 1;
        }
        let byte = &table[start];
        let effects = byte
            .effects
            .iter()
            .map(|effect| format!("ReadEffect::{effect}"))
            .collect::<Vec<_>>()
            .join(", ");
        let constructor = format!("ByteClassification::from_masks({:#04x}, {:#04x}, {:#04x}, {:#04x}, &[{}], ChangeSource::{})",
            byte.readable, byte.unknown, !byte.covered, byte.write_only,
            effects, if byte.dynamic { "Dynamic" } else { "Static" });
        // Compress only runs of three or more; each index still has one table entry.
        if end - start >= 3 {
            output.push_str(&format!(
                "            {start}..={} => {constructor},\n",
                end - 1
            ));
        } else {
            for index in start..end {
                output.push_str(&format!("            {index} => {constructor},\n"));
            }
        }
        start = end;
    }
    output.push_str("            _ => unreachable!(\"array index\"),\n        })\n    }\n");
    output
}

/// True when any field in `page` carries version metadata (`since`, `until`,
/// `changed_in`, `renamed_from`, or `aliased_to`) and the page therefore gets
/// per-revision specialisations.
pub fn is_versioned(page: &PageSchema) -> bool {
    page.is_versioned()
}

/// Emit one page. Every page gets the flat, rev-agnostic file in `out_dir/`
/// (the union of all fields, for consumers that are not rev-threaded). A
/// versioned page additionally gets one filtered file per revision of the
/// family domain its `spec.id` names (CMIS for spec-less union runs), under
/// `out_dir/v{rev}/`, produced by `per_rev::emit_page_per_rev`.
pub fn emit_page_dispatched(page: &PageSchema, out_dir: &Path) -> std::io::Result<()> {
    let family_revs =
        crate::ir::version::family_revs_for_spec(page.spec.as_ref().map(|s| s.id.as_str()));
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(out_dir.join(format!("{}.rs", page.id)), emit_page(page))?;
    if !is_versioned(page) {
        return Ok(());
    }
    for rev in family_revs {
        let body = crate::emitters::rust::per_rev::emit_page_per_rev(page, *rev);
        let dir = out_dir.join(format!("v{}", rev.dirname()));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{}.rs", page.id)), body)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, offset: u32) -> PageField {
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

    fn schema(id: &str, page_address: &str, size_bytes: u32, fields: Vec<PageField>) -> PageSchema {
        PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: id.to_string(),
            title: id.to_string(),
            page_address: page_address.to_string(),
            size_bytes: Some(size_bytes),
            field_includes: vec![],
            fields,
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        }
    }

    #[test]
    fn cdb_read_qualification_is_since_53_and_comes_from_source() {
        use crate::ir::version::Rev;
        let source: PageSchema = serde_norway::from_str(include_str!(
            "../../../../../schemas/cmis/page-9fh-cdb-message.yaml"
        ))
        .unwrap();
        let qualified = emit_page_for_revision(&source, Rev::V5_3);
        assert!(qualified.contains("pub fn read_classifications()"));
        assert!(qualified.contains("0xff, 0x00, 0x00, 0x00, &[ReadEffect::None]"));
        // The 9Fh read-classification surface is a hand-qualified delivery:
        // the evidence is cited to the 05.3 PDF, so 5.4 stays outside the
        // emit gate (fail closed) even though the 5.3 `changed_in`
        // annotation resolves forward at the table level. Extending the
        // qualification to 5.4 requires re-verifying the read-effect
        // citations against the 05.4 PDF first.
        assert!(
            !emit_page_for_revision(&source, Rev::V5_4).contains("pub fn read_classifications()")
        );
        for revision in Rev::CMIS_ALL.iter().copied().filter(|r| *r < Rev::V5_3) {
            assert!(!emit_page_for_revision(&source, revision)
                .contains("pub fn read_classifications()"));
            // Even direct table generation must not infer earlier-revision
            // evidence from the 5.3-only changed_in annotation.
            assert!(!emit_read_classifications(&source, revision).contains("ReadEffect::None"));
        }
    }

    #[test]
    fn byte_scoped_cor_covers_reserved_bits_without_inventing_none() {
        let source: PageSchema = serde_norway::from_str("schema: openxcvr.page.v2\nkind: page\nid: CmisLowerMemory\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n  - { name: flag, offset: 8, type: bool, bit_offset: 6, access: ro_cor, read_effect_scope: byte }\n").unwrap();
        let output = emit_read_classifications(&source, crate::ir::version::Rev::V5_3);
        assert!(output.contains("8 => ByteClassification::from_masks(0xff, 0x00, 0x00, 0x00, &[ReadEffect::ClearOnRead], ChangeSource::Dynamic)"), "{output}");
        assert!(!output.contains("ReadEffect::None"));
    }

    #[test]
    fn lower_cdb_qualification_is_source_bound_and_changes_no_earlier_revision() {
        use crate::ir::{
            pages::{ReadEffectMetadata, ReadEffectScope},
            version::{resolve_field_for_rev, Rev},
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis");
        let source =
            crate::parser::pages::load_page_schema(&root.join("lower-memory.yaml")).unwrap();
        let mut previous = source.clone();
        for field in previous
            .fields
            .iter_mut()
            .filter(|f| matches!(f.offset, Some(8 | 37)))
        {
            field.changed_in = None;
        }
        for &revision in Rev::CMIS_ALL {
            if revision < Rev::V5_3 {
                // Compare the complete byte tables, not only target bytes.
                assert_eq!(
                    emit_read_classifications(&source, revision),
                    emit_read_classifications(&previous, revision),
                    "{revision:?}"
                );
            }
            // The two 5.3-qualified subjects of this test. Byte 8 also
            // carries the since-5.4 AbnormalFwIndicationFlag; its 5.4
            // changed_in annotation is out of scope here — it is qualified
            // by its own OIF-CMIS-05.4 citation and must resolve to no
            // effect before 5.4, which the else branch asserts.
            for field in source.fields.iter().filter(|f| {
                matches!(
                    f.name.as_str(),
                    "module_state_changed_flag" | "cdb_status_1"
                )
            }) {
                let resolved = resolve_field_for_rev(field, revision);
                // The 5.3 annotations propagate forward to 5.4 unchanged;
                // only revisions before 5.3 stay unqualified.
                if revision >= Rev::V5_3 {
                    let effect = if field.offset == Some(8) {
                        ReadEffectMetadata::ClearOnRead
                    } else {
                        ReadEffectMetadata::None
                    };
                    assert_eq!(
                        resolved.metadata.read_effect,
                        Some(effect),
                        "{}",
                        field.name
                    );
                    if field.offset == Some(8) {
                        assert_eq!(
                            resolved.metadata.read_effect_scope,
                            Some(ReadEffectScope::Byte)
                        );
                    }
                    let citation = format!("{:?}", resolved.metadata.read_effect_provenance);
                    for required in [
                        "OIF-CMIS-05.3",
                        "Table 8-3",
                        "138",
                        if field.offset == Some(8) {
                            "146"
                        } else {
                            "151"
                        },
                    ] {
                        assert!(citation.contains(required), "{}: {citation}", field.name);
                    }
                } else {
                    // Before 5.3 — and for any field not applicable at this
                    // revision — no read-effect annotation may resolve.
                    assert_eq!(
                        resolved.metadata.read_effect, None,
                        "{} at {revision:?}",
                        field.name
                    );
                    assert_eq!(resolved.metadata.read_effect_scope, None);
                }
            }
        }
        let cdb = crate::parser::pages::load_page_schema(&root.join("page-9fh-cdb-message.yaml"))
            .unwrap();
        for &revision in Rev::CMIS_ALL {
            if revision < Rev::V5_3 {
                assert!(!emit_page_for_revision(&cdb, revision)
                    .contains("pub fn read_classifications()"));
                assert!(!emit_read_classifications(&cdb, revision).contains("ReadEffect::None"));
            }
        }
    }

    #[test]
    fn full_256_byte_page_spans_both_halves() {
        let out = emit_page(&schema(
            "Sff8472A0hMemory",
            "A0h",
            256,
            vec![field("lower_half", 0), field("upper_half", 200)],
        ));
        assert!(
            out.contains("pub fn decode(buf: &[u8; 256]) -> Self"),
            "{out}"
        );
        assert!(out.contains("lower_half: buf[0],"), "{out}");
        assert!(out.contains("upper_half: buf[200],"), "{out}");
    }

    #[test]
    fn upper_128_byte_page_at_a0h_address_uses_page_relative_base() {
        // Regression: CMIS pages can live at the A0h *address* while remaining
        // 128-byte upper pages (flat offsets 128..=255). The page base must be
        // derived from size_bytes, not hardcoded per address — otherwise a
        // field at flat 128 decodes from buf[128] in a 128-byte buffer.
        let out = emit_page(&schema(
            "CmisPageA0hCdbEplSegment1",
            "A0h",
            128,
            vec![field("epl_segment_1", 128)],
        ));
        assert!(
            out.contains("pub fn decode(buf: &[u8; 128]) -> Self"),
            "{out}"
        );
        assert!(out.contains("epl_segment_1: buf[0],"), "{out}");
        assert!(!out.contains("buf[128]"), "{out}");
    }
}
