//! Additive checked decoders for the CMIS upper pages and the versioned
//! pages of the SFF families. Planning is pure: the CLI must collect every
//! plan successfully before writing checked files.
use std::collections::BTreeSet;

use super::pages::{c_type, emit_field_to, ASCII_HELPER};
use crate::expr::compute;
use crate::ir::checked::plan_domain;
use crate::ir::naming::{CNaming, NamingPolicy};
use crate::ir::pages::{page_base, FieldType, PageField, PageSchema};
use crate::ir::version::{applicable_revs, resolve_field_for_rev, Family, Rev};

/// The settled CMIS checked surface: a curated two-page set. SFF families
/// derive their target set instead (family-owned versioned pages), so this
/// list gates only the CMIS namespace.
pub const CHECKED_PAGE_IDS: &[&str] = &["CmisPage01hAdvertising", "CmisPage11hLaneStatus"];

pub fn is_cmis_generation(spec: Option<&str>) -> bool {
    spec == Some("cmis")
}

/// True when `schema` belongs to an SFF family (a checked target candidate
/// under the derived selection rule the Python emitter already uses).
pub fn is_sff_checked_page(schema: &PageSchema) -> bool {
    matches!(
        schema.spec.as_ref().map(|spec| spec.id.as_str()),
        Some("sff-8636" | "sff-8472")
    )
}

#[derive(Debug)]
pub struct CheckedPageOutput {
    pub header: String,
    pub source: String,
}

fn error(schema: &PageSchema, field: &PageField, rev: Rev, reason: &str) -> String {
    format!(
        "{}.{} revision {}: {reason}",
        schema.id,
        field.name,
        rev.as_str()
    )
}

/// Verify the representation AND the footprint actually supported by the
/// shared C extraction functions. They deliberately do not grow new field
/// classes here. E.g. a U8 extractor reads a whole byte, so a sub-byte width
/// on U8 cannot be silently treated as a mask it does not implement.
/// `page_size` is the page window's byte count (128-byte upper pages,
/// 128/256-byte full-address pages).
fn check_extraction(f: &PageField, base: i64, page_size: i64) -> Result<(), String> {
    let width = f.effective_bit_width();
    let (bytes, allowed_width, packed) = match f.ty {
        FieldType::Bool => (1, 1, true),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => (1, 8, true),
        FieldType::U8 | FieldType::I8 | FieldType::EnumRef => (1, 8, false),
        FieldType::U16Be | FieldType::I16Be => (2, 16, false),
        FieldType::Ascii => (i64::from(f.size_bytes.unwrap_or(0)), width, false),
        _ => return Err(format!("non-emittable type {:?}", f.ty)),
    };
    if bytes <= 0 || width == 0 || width > allowed_width || (!packed && width != allowed_width) {
        return Err(format!("non-emittable width {width} for {:?}", f.ty));
    }
    let (index, start, end) = f
        .instances
        .as_ref()
        .map(|i| (i.index.as_str(), i.start, i.end))
        .unwrap_or(("", 0, 0));
    if start > end {
        return Err("non-emittable empty array extent".into());
    }
    for i in start..=end {
        let offset = if let Some(expr) = &f.offset_expr {
            compute(expr, index, i)?
        } else {
            i64::from(f.offset.ok_or("missing offset")?)
        } - base;
        let bit = match &f.bit_offset_expr {
            Some(expr) if !expr.is_empty() => compute(expr, index, i)?,
            _ => i64::from(f.bit_offset.unwrap_or(0)),
        };
        if !(0..8).contains(&bit) || (packed && bit + i64::from(width) > 8) || (!packed && bit != 0)
        {
            return Err(format!("non-emittable bit extent {bit}+{width}"));
        }
        if offset < 0 || offset > page_size - bytes {
            return Err(format!(
                "extraction extent {offset}..{} exceeds the {page_size}-byte page",
                offset + bytes
            ));
        }
    }
    Ok(())
}

fn shape(f: &PageField) -> (Option<i64>, Option<u32>) {
    (
        f.instances.as_ref().map(|i| i.end - i.start + 1),
        matches!(f.ty, FieldType::Ascii).then_some(f.size_bytes.unwrap_or(0)),
    )
}

/// Preflight every revision-resolved mapping to the existing raw C members,
/// then render direct assignments (never a flat decode followed by masking).
/// `header_path` is the include path of the new checked header.
pub fn plan_checked_page(
    schema: &PageSchema,
    header_path: &str,
) -> Result<CheckedPageOutput, String> {
    let domain = plan_domain(schema);
    let family = domain
        .first()
        .map(|rev| rev.family())
        .unwrap_or(Family::Cmis);
    let cmis_page = family == Family::Cmis;
    let accepted = if cmis_page {
        CHECKED_PAGE_IDS.contains(&schema.id.as_str())
    } else {
        is_sff_checked_page(schema)
    };
    if !accepted {
        return Err(format!(
            "{}: not a checked page of a modeled family",
            schema.id
        ));
    }
    let base = page_base(schema);
    let page_size: i64 = schema
        .size_bytes
        .unwrap_or(if base == 0 { 256 } else { 128 })
        .into();
    if (base != 0 && base != 128) || !matches!(page_size, 128 | 256) {
        return Err(format!(
            "{}: checked decoder requires a 128- or 256-byte page",
            schema.id
        ));
    }
    let mut members = BTreeSet::new();
    let mut presence = String::new();
    let mut bodies: Vec<String> = domain.iter().map(|_| String::new()).collect();
    let mut needs_ascii = false;

    for original in &schema.fields {
        let applicable = applicable_revs(original, domain);
        // The raw helper also builds assignment expressions. Reject impossible
        // sub-byte masks before calling it, rather than allowing a Rust shift
        // overflow to bypass the contextual generation error.
        if matches!(
            original.ty,
            FieldType::U2
                | FieldType::U3
                | FieldType::U4
                | FieldType::U5
                | FieldType::U6
                | FieldType::U7
        ) && original.emits_value()
            && original.effective_bit_width() > 8
        {
            return Err(error(
                schema,
                original,
                applicable.iter().next().copied().unwrap_or(domain[0]),
                &format!("non-emittable raw width {}", original.effective_bit_width()),
            ));
        }
        let raw = if original.emits_value() {
            emit_field_to(original, base, "out->")
        } else {
            None
        };
        if raw.is_some() {
            if !members.insert(original.name.as_str()) {
                return Err(error(
                    schema,
                    original,
                    domain[0],
                    "stable member collision",
                ));
            }
            presence.push_str(&format!("    bool {};\n", original.name));
        }
        for (slot, &rev) in domain.iter().enumerate() {
            if !applicable.contains(&rev) {
                continue;
            }
            let mut resolved = resolve_field_for_rev(original, rev);
            // A resolver rename/alias never changes the public C identity.
            // Retain ALL other resolved metadata for representation/extraction.
            resolved.name.clone_from(&original.name);
            let Some(raw) = &raw else {
                // The raw emitter may omit unsupported classes, but a resolved
                // emittable value cannot acquire a missing destination member.
                if resolved.emits_value() && c_type(&resolved).is_some() {
                    check_extraction(&resolved, base, page_size)
                        .map_err(|reason| error(schema, original, rev, &reason))?;
                    if emit_field_to(&resolved, base, "decoded.values.").is_some() {
                        return Err(error(schema, original, rev, "no raw destination member"));
                    }
                }
                continue;
            };
            if !resolved.emits_value() {
                return Err(error(
                    schema,
                    original,
                    rev,
                    "applicable member is non-emittable (emit_value)",
                ));
            }
            check_extraction(&resolved, base, page_size)
                .map_err(|reason| error(schema, original, rev, &reason))?;
            if c_type(original) != c_type(&resolved) || shape(original) != shape(&resolved) {
                return Err(error(
                    schema,
                    original,
                    rev,
                    &format!(
                        "incompatible raw representation {:?} / {:?}, resolved {:?} / {:?}",
                        c_type(original),
                        shape(original),
                        c_type(&resolved),
                        shape(&resolved)
                    ),
                ));
            }
            let emitted = emit_field_to(&resolved, base, "decoded.values.").ok_or_else(|| {
                error(schema, original, rev, "applicable member is non-emittable")
            })?;
            if emitted.decl != raw.decl {
                return Err(error(
                    schema,
                    original,
                    rev,
                    "incompatible member declaration/array extent",
                ));
            }
            needs_ascii |= emitted.needs_ascii_helper;
            bodies[slot].push_str(&emitted.assign);
            bodies[slot].push_str(&format!("    decoded.present.{} = true;\n", original.name));
        }
    }
    if presence.is_empty() {
        presence.push_str("    bool _unused;\n");
    }

    let suffix = CNaming.type_name(&schema.id);
    let ty = format!("openxcvr_{suffix}");
    let function = format!("openxcvr_decode_{suffix}_for_revision");
    // The revision selector parameter/member: the settled `cmis_rev_byte`
    // name for CMIS, `rev_compliance_byte` for the SFF families (the
    // selector is a compliance code there, not a packed-BCD major byte).
    let param = if cmis_page {
        "cmis_rev_byte"
    } else {
        "rev_compliance_byte"
    };
    let family_label = match family {
        Family::Cmis => "CMIS",
        Family::Sff8636 => "SFF-8636",
        Family::Sff8472 => "SFF-8472",
    };
    let mut rev_names: Vec<String> = Vec::with_capacity(domain.len());
    for rev in domain {
        let byte = rev.family_rev_byte();
        rev_names.push(format!("0x{byte:02X} ({family_label} {})", rev.as_str()));
    }
    let supported_comment_lines = wrap_comment(
        &format!(
            "Supported revision bytes: exactly {}; every other revision \
             byte is rejected",
            rev_names.join(", ")
        ),
        77,
    );
    // The CMIS Page 11h lane-status note names CMIS-specific helpers; other
    // pages end the block after the generic presence semantics.
    let page_note = if cmis_page {
        concat!(
            "enum storage is raw.\n",
            " * Interpret dp_state_host_lane with the existing\n",
            " * openxcvr_cmis_data_path_state_for_revision and\n",
            " * openxcvr_cmis_dpsm_is_terminal_for_rev helpers (no nibble remap). */\n",
        )
    } else {
        "enum storage is raw. */\n"
    };
    let signature = format!("openxcvr_status {function}(uint8_t {param}, const uint8_t *buf, size_t len, {ty}_checked *out)");
    let header = format!(
        concat!(
            "/* Generated by openxcvr-codegen. DO NOT EDIT. */\n#pragma once\n\n",
            "#include \"pages/{suffix}.h\"\n\n",
            "/* Inspect present before using values: absent values are zero, but\n",
            " * present-zero is also valid. One presence flag covers an entire array.\n",
            " * Presence is applicability, NOT enum validity; ",
            "{page_note}",
            "typedef struct {ty}_presence {{\n{presence}}} {ty}_presence;\n\n",
            "typedef struct {ty}_checked {{\n",
            "    uint8_t {param};\n    {ty} values;\n    {ty}_presence present;\n",
            "}} {ty}_checked;\n\n",
            "#ifdef __cplusplus\nextern \"C\" {{\n#endif\n\n",
            "{supported_comment}\n",
            " * NULL arguments, unsupported revision, then len < {page_size} are checked\n",
            " * in that order. Errors preserve *out; longer inputs are accepted.\n",
            " * On success the supplied input may overlap *out. */\n",
            "{signature};\n\n#ifdef __cplusplus\n}}\n#endif\n"
        ),
        suffix = suffix,
        page_note = page_note,
        ty = ty,
        presence = presence,
        param = param,
        supported_comment = supported_comment_lines,
        page_size = page_size,
        signature = signature,
    );
    let mut admission = String::new();
    let mut dispatch = String::new();
    for (&rev, body) in domain.iter().zip(bodies) {
        let byte = rev.family_rev_byte();
        admission.push_str(&format!("    case 0x{byte:02X}:\n"));
        dispatch.push_str(&format!(
            "    case 0x{byte:02X}: {{\n{body}        break;\n    }}\n"
        ));
    }
    let helper = if needs_ascii { ASCII_HELPER } else { "" };
    let source = format!(
        concat!(
            "/* Generated by openxcvr-codegen. DO NOT EDIT. */\n",
            "#include \"{header_path}\"\n\n{helper}",
            "{signature} {{\n",
            "    {ty}_checked decoded = {{0}};\n",
            "    if (buf == NULL || out == NULL) return OPENXCVR_ERR_INVALID_ARGUMENT;\n",
            "    switch ({param}) {{\n{admission}        break;\n",
            "    default: return OPENXCVR_ERR_UNSUPPORTED_REVISION;\n    }}\n",
            "    if (len < {page_size}) return OPENXCVR_ERR_SHORT_BUFFER;\n",
            "    decoded.{param} = {param};\n",
            "    switch ({param}) {{\n{dispatch}    }}\n",
            "    *out = decoded;\n    return OPENXCVR_OK;\n}}\n"
        ),
        header_path = header_path,
        helper = helper,
        signature = signature,
        admission = admission,
        ty = ty,
        param = param,
        page_size = page_size,
        dispatch = dispatch,
    );
    Ok(CheckedPageOutput { header, source })
}

/// Wrap `text` into block-comment lines of at most `width` characters: the
/// first line opens with `/*`, continuation lines continue with ` *`. The
/// closing `*/` is left to the caller's following lines. A parenthesized
/// word never separates from the word it annotates ("0x50 (CMIS 5.0)").
fn wrap_comment(text: &str, width: usize) -> String {
    let mut units: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if word.starts_with('(') {
            if let Some(last) = units.last_mut() {
                last.push(' ');
                last.push_str(word);
                continue;
            }
        }
        units.push(word.to_string());
    }
    let mut out = String::new();
    let mut line = String::from("/* ");
    for unit in &units {
        if line.chars().count() + unit.chars().count() > width {
            out.push_str(line.trim_end());
            out.push('\n');
            line = String::from(" * ");
        }
        line.push_str(unit);
        line.push(' ');
    }
    out.push_str(line.trim_end());
    out
}
