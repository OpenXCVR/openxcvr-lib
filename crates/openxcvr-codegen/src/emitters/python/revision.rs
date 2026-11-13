//! Additive checked Python revision decoders for the versioned pages of
//! every modeled family (CMIS, SFF-8636, SFF-8472).
//!
//! Planning is pure: the driver collects and renders every checked output
//! for the invocation before any file is written (a generation error changes
//! no existing output). Emission consumes the shared language-neutral
//! [`CheckedPagePlan`] — resolved over the page's own family revision domain
//! — and reuses the raw Python page extraction helpers (`decode_scalar` and
//! the offset resolvers) with DIRECT per-revision reads — never a flat
//! decode followed by masking, which cannot represent revision-shifted
//! footprints or relocated bytes.
//!
//! The stable public shape per page schema id `T` (snake stem `S`) is fixed
//! by the settled spec: module `S_revision.py` with frozen dataclasses
//! `TPresence` (one bool per stable union member; one flag for a whole
//! array) and `TChecked` (`cmis_rev_byte: int` for CMIS pages,
//! `rev_compliance_byte: int` for the SFF families — the byte that selects
//! the revision: the CMIS packed-BCD major byte, the SFF-8636/SFF-8472
//! compliance code) plus the `decode_for_revision` classmethod. `V` is the
//! existing raw `T` when the raw member set losslessly covers the stable
//! union and every revision keeps each member's Python representation; only
//! such a switch (Page 00h at this base) introduces the module-local
//! `TCheckedValues` union. No encode/setter contract is invented here.
use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::pages::{
    decode_scalar, field_needs_struct, py_type, resolve_bit_offset, resolve_offset, uses_bit_offset,
};
use crate::ir::checked::{plan_domain, CheckedPagePlan};
use crate::ir::naming::camel_to_snake;
use crate::ir::pages::{page_base, FieldType, PageField, PageSchema};
use crate::ir::version::Rev;

/// The public parameter/dataclass member carrying the revision selector
/// byte: `cmis_rev_byte` for the settled CMIS contract, `rev_compliance_byte`
/// for the SFF families (their selector is a compliance code, not a CMIS
/// major byte).
fn rev_param(schema: &PageSchema) -> &'static str {
    match plan_domain(schema).first().map(|rev| rev.family()) {
        Some(crate::ir::version::Family::Sff8636 | crate::ir::version::Family::Sff8472) => {
            "rev_compliance_byte"
        }
        _ => "cmis_rev_byte",
    }
}

/// Family label for error/doc text ("CMIS", "SFF-8636", "SFF-8472").
fn family_label(schema: &PageSchema) -> &'static str {
    match plan_domain(schema).first().map(|rev| rev.family()) {
        Some(crate::ir::version::Family::Sff8636) => "SFF-8636",
        Some(crate::ir::version::Family::Sff8472) => "SFF-8472",
        _ => "CMIS",
    }
}

/// Supported wire revision bytes, derived from the page's family revision
/// table via [`Rev::family_rev_byte`] (packed-BCD major byte for CMIS,
/// compliance code for the SFF families), never a hand-pasted list.
fn supported_bytes(domain: &[Rev]) -> Vec<u8> {
    domain.iter().map(|rev| rev.family_rev_byte()).collect()
}

/// The family's self-reported revision byte, when THIS page carries it and
/// it applies at every family revision: (member-free wire offset, byte).
/// CMIS lower memory `cmis_revision` at 1; SFF-8636 lower memory
/// `revision_compliance` at 1 (absent before Rev 1.4, so no cross-check is
/// emitted); SFF-8472 A0h `sff8472_compliance` at 94. Pages that do not
/// carry the family probe (upper pages, A2h) get no embedded check.
fn revision_probe_offset(schema: &PageSchema, domain: &[Rev]) -> Option<usize> {
    let name = match domain.first()?.family() {
        crate::ir::version::Family::Cmis => "cmis_revision",
        crate::ir::version::Family::Sff8636 => "revision_compliance",
        crate::ir::version::Family::Sff8472 => "sff8472_compliance",
    };
    let field = schema.fields.iter().find(|f| f.name == name)?;
    // Only when the probe applies at every family revision: a windowed probe
    // (SFF-8636 byte 1 before Rev 1.4 held something else) cannot be
    // cross-checked at the revisions it does not cover.
    let applicable = crate::ir::version::applicable_revs(field, domain);
    if !domain.iter().all(|rev| applicable.contains(rev)) {
        return None;
    }
    usize::try_from(field.offset?).ok()
}

fn error(schema: &PageSchema, member: &str, rev: Option<Rev>, reason: &str) -> String {
    match rev {
        Some(rev) => format!(
            "{}.{} revision {}: {reason}",
            schema.id,
            member,
            rev.as_str()
        ),
        None => format!("{}.{}: {reason}", schema.id, member),
    }
}

/// Wire bytes consumed by one instance of `f` (its fixed payload length).
fn wire_bytes(f: &PageField) -> Result<u32, String> {
    Ok(match f.ty {
        FieldType::Bool
        | FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7
        | FieldType::U8
        | FieldType::I8 => 1,
        // An enum_ref's footprint follows its DECLARED size_bytes, so the
        // page-bounds admission matches what decode_scalar actually reads:
        // a 2-byte enum_ref (SFF-8636 v2.12 § 7 / SFF-8472 v12.5a § 13
        // "128-129 | 2 | Format ID | CA1Bh") is a 16-bit big-endian
        // `unpack_from('>H')` and must be refused when those two bytes would
        // overrun the page, never admitted as one byte whose decoder then
        // raises struct.error at runtime. decode_scalar implements only the
        // 1- and 2-byte widths, so a wider declaration fails closed here.
        FieldType::EnumRef => match f.size_bytes {
            None | Some(1) => 1,
            Some(2) => 2,
            other => {
                return Err(format!(
                    "unsupported enum_ref size_bytes {other:?}: extraction reads 1 or 2 bytes"
                ))
            }
        },
        FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::F16Be
        | FieldType::X16Be => 2,
        FieldType::U32Be | FieldType::I32Be => 4,
        FieldType::U64Le | FieldType::U64Be => 8,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite => f
            .size_bytes
            .ok_or_else(|| "fixed-length payload requires size_bytes".to_string())?,
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                1
            } else {
                f.size_bytes.unwrap_or(1)
            }
        }
    })
}

/// Validate the per-instance footprint actually implemented by the shared
/// extraction and return the element's decode expression.
fn element_expr(
    schema: &PageSchema,
    member: &str,
    rev: Rev,
    element: &PageField,
    file_off: i64,
    page_size: u32,
) -> Result<String, String> {
    let bytes = wire_bytes(element).map_err(|reason| error(schema, member, Some(rev), &reason))?;
    let packed = matches!(
        element.ty,
        FieldType::Bool
            | FieldType::U2
            | FieldType::U3
            | FieldType::U4
            | FieldType::U5
            | FieldType::U6
            | FieldType::U7
    ) || (element.ty == FieldType::Bitmap
        && (element.bit_offset.is_some() || element.bit_width.is_some()));
    let bit = i64::from(element.bit_offset.unwrap_or(0));
    let width = i64::from(element.effective_bit_width());
    if packed {
        // decode_scalar masks inside a single byte; a footprint crossing the
        // byte boundary would silently wrap instead of reading the wire.
        if !(0..8).contains(&bit) || bit + width > 8 {
            return Err(error(
                schema,
                member,
                Some(rev),
                &format!("unsupported bit extent {bit}+{width} for {:?}", element.ty),
            ));
        }
    } else if bit != 0 {
        return Err(error(
            schema,
            member,
            Some(rev),
            &format!(
                "unsupported bit_offset {bit} on whole-value type {:?}",
                element.ty
            ),
        ));
    }
    if file_off < 0 {
        return Err(error(
            schema,
            member,
            Some(rev),
            &format!("unsupported extraction offset {file_off}: below the page base"),
        ));
    }
    if file_off + i64::from(bytes) > i64::from(page_size) {
        return Err(error(
            schema,
            member,
            Some(rev),
            &format!(
                "extraction extent {file_off}..{} exceeds the {page_size}-byte page",
                file_off + i64::from(bytes)
            ),
        ));
    }
    Ok(decode_scalar(element, file_off))
}

/// The read expression for a present member at `rev` (direct per-revision
/// read of the resolved field), or a default expression for an absent one.
/// Returns (expression, is_present, pre_line, needs_struct).
fn member_expr(
    schema: &PageSchema,
    member: &PageField,
    resolved: Option<&PageField>,
    rev: Rev,
    base: i64,
    page_size: u32,
) -> Result<(String, bool, Option<String>, bool), String> {
    let Some(resolved) = resolved else {
        return Ok((default_expr(member)?, false, None, false));
    };
    if !resolved.emits_value() {
        return Err(error(
            schema,
            &member.name,
            Some(rev),
            "applicable member is non-emittable (emit_value)",
        ));
    }
    // Stable public representation: the original annotation must survive
    // every revision's resolution. Lossless int-width changes keep the int
    // annotation; bool/int, text/binary and scalar/array flips are refused.
    let original_py = py_type(member).ok_or_else(|| "unsupported Python type".to_string())?;
    let resolved_py = py_type(resolved).ok_or_else(|| "unsupported Python type".to_string())?;
    if original_py != resolved_py
        || member.instances.is_some() != resolved.instances.is_some()
        || member.instances.as_ref().map(|i| (i.start, i.end))
            != resolved.instances.as_ref().map(|i| (i.start, i.end))
    {
        return Err(error(
            schema,
            &member.name,
            Some(rev),
            &format!(
                "incompatible Python representation: original {original_py:?}, \
                 resolved {resolved_py:?}"
            ),
        ));
    }

    if let Some(inst) = &resolved.instances {
        let mut elems = Vec::new();
        for i in inst.start..=inst.end {
            let flat = resolve_offset(resolved, inst, i).ok_or_else(|| {
                error(
                    schema,
                    &member.name,
                    Some(rev),
                    "unsupported extraction: unresolvable instance offset",
                )
            })?;
            let bit = resolve_bit_offset(resolved, inst, i).ok_or_else(|| {
                error(
                    schema,
                    &member.name,
                    Some(rev),
                    "unsupported extraction: unresolvable instance bit offset",
                )
            })?;
            let mut element = resolved.clone();
            element.bit_offset = if uses_bit_offset(resolved) {
                Some(bit as u32)
            } else {
                None
            };
            element.bit_offset_expr = None;
            elems.push(element_expr(
                schema,
                &member.name,
                rev,
                &element,
                flat - base,
                page_size,
            )?);
        }
        let var = format!("{}_vals", member.name);
        let pre = format!("{var} = [{}]", elems.join(", "));
        Ok((var, true, Some(pre), field_needs_struct(resolved)))
    } else {
        if resolved.offset.is_none() {
            return Err(error(
                schema,
                &member.name,
                Some(rev),
                "unsupported extraction: missing static offset",
            ));
        }
        if resolved.offset_expr.is_some() {
            return Err(error(
                schema,
                &member.name,
                Some(rev),
                "unsupported extraction: expression-addressed scalar",
            ));
        }
        if resolved.bit_offset_expr.is_some() {
            return Err(error(
                schema,
                &member.name,
                Some(rev),
                "unsupported extraction: expression bit offset on a scalar",
            ));
        }
        let file_off = i64::from(resolved.offset.unwrap()) - base;
        let expr = element_expr(schema, &member.name, rev, resolved, file_off, page_size)?;
        Ok((expr, true, None, field_needs_struct(resolved)))
    }
}

/// Default expression for an absent member: integer 0, bool False, ASCII
/// empty string, bytes/composite zero-filled to the fixed payload length,
/// arrays independently allocated at the declared extent.
fn default_expr(member: &PageField) -> Result<String, String> {
    let scalar = |member: &PageField| -> Result<String, String> {
        Ok(match member.ty {
            FieldType::Ascii => "''".to_string(),
            FieldType::Bytes | FieldType::Composite => format!(
                "bytes({})",
                member
                    .size_bytes
                    .ok_or("fixed-length payload requires size_bytes")?
            ),
            FieldType::Bool => "False".to_string(),
            _ => "0".to_string(),
        })
    };
    if let Some(inst) = &member.instances {
        // A fresh list literal per call: results never share a mutable array.
        Ok(format!(
            "[{}] * {}",
            scalar(member)?,
            inst.end - inst.start + 1
        ))
    } else {
        scalar(member)
    }
}

/// Emit one checked revision module for `schema`. `plan` must come from
/// [`crate::ir::checked::plan_checked_page`] over the same schema.
pub fn emit_checked_page(schema: &PageSchema, plan: &CheckedPagePlan) -> Result<String, String> {
    let base = page_base(schema);
    let page_size = schema
        .size_bytes
        .unwrap_or(if base == 0 { 256 } else { 128 });
    let stem = camel_to_snake(&schema.id);
    let id = &schema.id;

    // V decision: the raw values type is reusable only when its member set
    // equals the stable union and every member's Python representation is
    // revision-stable (checked while building the branches below).
    let raw_members: BTreeSet<&str> = schema
        .fields
        .iter()
        .filter(|f| f.emits_value())
        .map(|f| f.name.as_str())
        .collect();
    let union_members: BTreeSet<&str> = plan
        .members
        .iter()
        .map(|m| m.original_name.as_str())
        .collect();
    let uses_raw = raw_members == union_members;
    let values_type = if uses_raw {
        id.clone()
    } else {
        format!("{id}CheckedValues")
    };

    // Resolve every member at every family revision before rendering, so an
    // unrepresentable mapping aborts the whole module, never a silent hole.
    let domain = plan_domain(schema);
    let param = rev_param(schema);
    let mut branches = String::new();
    let mut needs_struct = false;
    for rev in domain {
        let view = plan
            .revisions
            .get(rev)
            .ok_or_else(|| format!("{id}: plan is missing revision {}", rev.as_str()))?;
        let mut pre_lines = String::new();
        let mut value_lines = String::new();
        let mut presence_lines = String::new();
        for (index, member) in plan.members.iter().enumerate() {
            let original = &member.original;
            let resolved = view.fields.get(index).ok_or_else(|| {
                format!(
                    "{id}.{}: plan member missing at {}",
                    original.name,
                    rev.as_str()
                )
            })?;
            let (expr, present, pre, uses_struct) =
                member_expr(schema, original, resolved.as_ref(), *rev, base, page_size)?;
            needs_struct |= uses_struct;
            if let Some(pre) = pre {
                let _ = writeln!(pre_lines, "            {pre}");
            }
            let _ = writeln!(
                value_lines,
                "                    {}={},",
                original.name, expr
            );
            let _ = writeln!(
                presence_lines,
                "                    {}={},",
                original.name,
                if present { "True" } else { "False" }
            );
        }
        let byte = rev.family_rev_byte();
        let _ = write!(
            branches,
            concat!(
                "        if {param} == 0x{byte:02X}:\n",
                "{pre_lines}",
                "            return cls(\n",
                "                {param}={param},\n",
                "                values={values_type}(\n",
                "{value_lines}",
                "                ),\n",
                "                present={id}Presence(\n",
                "{presence_lines}",
                "                ),\n",
                "            )\n",
            ),
            param = param,
            byte = byte,
            pre_lines = pre_lines,
            values_type = values_type,
            id = id,
            value_lines = value_lines,
            presence_lines = presence_lines,
        );
    }

    // Presence flags: one bool per stable member (one flag covers an array).
    let mut presence_fields = String::new();
    for member in &plan.members {
        let _ = writeln!(presence_fields, "    {}: bool", member.original_name);
    }
    if presence_fields.is_empty() {
        presence_fields.push_str("    pass\n");
    }

    let mut imports =
        String::from("from __future__ import annotations\nfrom dataclasses import dataclass\n");
    if needs_struct {
        imports.push_str("import struct\n");
    }
    if uses_raw {
        let _ = writeln!(imports, "from .{stem} import {id}");
    }

    // The stable CheckedValues union: the emitted per-revision union with
    // original-identity names — no invented encode/setter contract.
    let mut values_class = String::new();
    if !uses_raw {
        let mut value_fields = String::new();
        for member in &plan.members {
            let original = &member.original;
            let py = py_type(original).ok_or("unsupported Python type")?;
            if original.instances.is_some() {
                let _ = writeln!(value_fields, "    {}: list[{py}]", original.name, py = py);
            } else {
                let _ = writeln!(value_fields, "    {}: {py}", original.name);
            }
        }
        if value_fields.is_empty() {
            value_fields.push_str("    pass\n");
        }
        let _ = write!(
            values_class,
            concat!("@dataclass\nclass {id}CheckedValues:\n", "{value_fields}\n",),
            id = id,
            value_fields = value_fields,
        );
    }

    let supported = supported_bytes(domain);
    let supported_list = supported
        .iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    // A page that carries the family's self-reported revision byte at every
    // family revision cross-checks it against the requested revision, after
    // the shared length floor and before any field reads. The offset is
    // schema-derived (CMIS/SFF-8636 lower byte 1, SFF-8472 A0h byte 94).
    let embedded_check = match revision_probe_offset(schema, domain) {
        Some(offset) => format!(
            concat!(
                "        if buf[{offset}] != {param}:\n",
                "            raise ValueError(\n",
                "                f\"buf[{offset}] revision byte {{buf[{offset}]:#04x}} does not match \"\n",
                "                f\"requested revision {{{param}:#04x}}\"\n",
                "            )\n",
            ),
            offset = offset,
            param = param,
        ),
        None => String::new(),
    };

    let family_label = family_label(schema);
    Ok(format!(
        concat!(
            "# Generated by openxcvr-codegen. DO NOT EDIT.\n",
            "{imports}\n\n",
            "_SUPPORTED_REVISION_BYTES = ({supported_list})\n\n\n",
            "{values_class}",
            "@dataclass(frozen=True)\n",
            "class {id}Presence:\n",
            "{presence_fields}\n\n",
            "@dataclass(frozen=True)\n",
            "class {id}Checked:\n",
            "    {param}: int\n",
            "    values: {values_type}\n",
            "    present: {id}Presence\n\n",
            "    @classmethod\n",
            "    def decode_for_revision(cls, {param}: int, buf: bytes) -> {id}Checked:\n",
            "        if not isinstance(buf, bytes):\n",
            "            raise TypeError(f\"buf must be bytes, got {{type(buf).__name__}}\")\n",
            "        if type({param}) is not int:\n",
            "            raise TypeError(\n",
            "                f\"{param} must be int, got {{type({param}).__name__}}\"\n",
            "            )\n",
            "        if {param} not in _SUPPORTED_REVISION_BYTES:\n",
            "            raise ValueError(\n",
            "                f\"unsupported {family} revision byte {{{param}:#04x}}; \"\n",
            "                \"expected one of {supported_list}\"\n",
            "            )\n",
            "        if len(buf) < {page_size}:\n",
            "            raise ValueError(f\"expected >={page_size} bytes, got {{len(buf)}}\")\n",
            "{embedded_check}",
            "{branches}",
            "        raise AssertionError(\"unreachable: revision byte admitted above\")\n",
        ),
        imports = imports,
        supported_list = supported_list,
        values_class = values_class,
        id = id,
        param = param,
        family = family_label,
        page_size = page_size,
        presence_fields = presence_fields,
        values_type = values_type,
        embedded_check = embedded_check,
        branches = branches,
    ))
}
