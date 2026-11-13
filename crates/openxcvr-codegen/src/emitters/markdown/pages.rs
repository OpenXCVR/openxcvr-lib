//! Per-page Markdown reference rendering.
//!
//! Everything is read off the parsed IR. Recorded metadata is rendered
//! verbatim; anything the schema does not record renders as `not recorded`.
//! Revision applicability goes through the same `applicable_revs` /
//! `resolve_field_for_rev` resolver the Rust per-revision emitter uses.

use crate::ir::naming::camel_to_snake;
use crate::ir::pages::{page_base, FieldType, PageField, PageSchema, ReadEffectScope};
use crate::ir::version::{applicable_revs, resolve_field_for_rev, Rev};

use super::{cite, md_code, md_text, scalar, GENERATED_BY};

const NOT_RECORDED: &str = "not recorded";

pub fn page_file_stem(schema: &PageSchema) -> String {
    camel_to_snake(&schema.id)
}

/// One page reference document.
pub fn emit_page_doc(source_rel: &str, schema: &PageSchema) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# {} — {}\n\n{}\n\nSource schema: `schemas/{}`\n\n",
        md_text(&schema.id),
        md_text(&schema.title),
        GENERATED_BY,
        md_code(source_rel)
    ));
    out.push_str(&format!(
        "- Schema: `{}` (kind `{}`), id `{}`\n",
        md_code(&schema.schema),
        md_code(&schema.kind),
        md_code(&schema.id)
    ));
    if page_base(schema) == 0 {
        out.push_str(&format!(
            "- Page address: `{}` — recorded offsets are page-buffer bytes from 0\n",
            md_code(&schema.page_address)
        ));
    } else if let Some(size) = schema.size_bytes {
        out.push_str(&format!(
            "- Page address: `{}` — upper-half page: recorded offsets {}–{} map to page-buffer bytes 0–{}\n",
            md_code(&schema.page_address),
            page_base(schema),
            page_base(schema) + i64::from(size) - 1,
            size - 1
        ));
    } else {
        // No recorded size: the upper-half base mapping is real resolver
        // semantics, but the span end would be fabricated from the
        // resolver's 128-byte default. Keep the base, omit the span.
        out.push_str(&format!(
            "- Page address: `{}` — upper-half page: recorded offsets are page-buffer \
             bytes plus 128 (upper-half base; page size not recorded, so the \
             offset span is not shown)\n",
            md_code(&schema.page_address)
        ));
    }
    out.push_str(&format!(
        "- Size: {}\n",
        match schema.size_bytes {
            Some(n) => format!("{n} bytes"),
            None => NOT_RECORDED.to_string(),
        }
    ));
    out.push_str(&format!("- Spec: {}\n", spec_line(schema)));
    out.push_str(&format!(
        "- Provenance: {}\n",
        cite(schema.provenance.as_ref())
    ));
    if schema.is_versioned() {
        out.push_str(
            "- Revision metadata: present — the union table below lists every \
             field with its revision applicability, and per-revision resolved \
             tables follow it\n",
        );
    } else {
        out.push_str(
            "- Revision metadata: none recorded — revision applicability is not recorded\n",
        );
    }

    out.push_str("\n## Fields\n\n");
    out.push_str(
        "Widths are in bits (effective IR footprint); addressing expressions and \
         instance ranges are recorded schema values. Volatility and read-effect \
         cells follow the shared resolver: a value implied by the recorded \
         `access` or `is_dynamic` flag rather than recorded `change_source:` / \
         `read_effect:` metadata is marked in place, e.g. `dynamic` (implied by \
         `is_dynamic: true`).\n\n",
    );
    let spec_id = schema.spec.as_ref().map(|s| s.id.as_str());
    out.push_str(&fields_table(
        &schema.fields,
        schema.is_versioned(),
        spec_id,
    ));
    out.push_str(&inline_enum_section("###", &schema.fields));
    out.push_str(&variants_section("###", &schema.fields));

    if schema.is_versioned() {
        let spec_id = schema.spec.as_ref().map(|s| s.id.as_str());
        let family_revs = crate::ir::version::family_revs_for_spec(spec_id);
        let family_name = match spec_id {
            Some("sff-8636") => "SFF-8636",
            Some("sff-8472") => "SFF-8472",
            _ => "CMIS",
        };
        out.push_str("\n## Revision-resolved fields\n\n");
        out.push_str(
            "Each table below is the field set resolved for that revision by the \
             same resolver the generated Rust per-revision modules use: fields \
             absent at a revision are omitted, and `changed_in` / `renamed_from` / \
             `aliased_to` overrides are applied — including revision-specific \
             widths, types and inline enum values. These are IR projections, not \
             independent verification against each historical specification. \
             Unannotated fields default to all known family revisions in the resolver; \
             missing historical semantics remain unsupported. Citations are preserved \
             as recorded and may name a later revision. No introduction dates are inferred.\n\n",
        );
        for rev in family_revs {
            let resolved: Vec<PageField> = schema
                .fields
                .iter()
                .filter(|f| applicable_revs(f, family_revs).contains(rev))
                .map(|f| resolve_field_for_rev(f, *rev))
                .collect();
            out.push_str(&format!("### {} {}\n\n", family_name, rev.as_str()));
            if resolved.is_empty() {
                out.push_str("No fields of this page are present at this revision.\n\n");
            } else {
                out.push_str(&fields_table(&resolved, false, spec_id));
                out.push_str(&inline_enum_section("####", &resolved));
                out.push_str(&variants_section("####", &resolved));
            }
        }
    }

    super::finish_doc(out)
}

/// The `spec:` identity line, or `not recorded`.
fn spec_line(schema: &PageSchema) -> String {
    let Some(spec) = &schema.spec else {
        return NOT_RECORDED.to_string();
    };
    let mut s = format!("`{}`", md_code(&spec.id));
    if let Some(rev) = &spec.revision {
        s.push_str(&format!(" revision `{}`", md_code(rev)));
    }
    if !spec.errata.is_empty() {
        let errata = spec
            .errata
            .iter()
            .map(|e| {
                if let Some(map) = e.as_mapping() {
                    // { revision, source } form
                    let rev = map.get("revision").map(scalar).unwrap_or_default();
                    let source = map.get("source").map(scalar).unwrap_or_default();
                    format!("`{}` (`{}`)", md_code(&rev), md_code(&source))
                } else {
                    format!("`{}`", md_code(&scalar(e)))
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("; errata: {errata}"));
    }
    s
}

const FIELD_COLUMNS: &[&str] = &[
    "Field",
    "Offset",
    "Bit",
    "Width",
    "Type",
    "Units",
    "Access",
    "Category",
    "Volatility",
    "Read effect",
    "Revisions",
    "Provenance",
    "Description",
];
const REVISIONS_COLUMN: usize = 10;

fn fields_table(fields: &[PageField], with_revisions: bool, spec_id: Option<&str>) -> String {
    let mut header: Vec<&str> = FIELD_COLUMNS.to_vec();
    if !with_revisions {
        header.remove(REVISIONS_COLUMN);
    }
    let mut out = String::new();
    out.push_str(&format!("| {} |\n", header.join(" | ")));
    out.push_str(&format!("|{}|\n", vec!["---"; header.len()].join("|")));
    for field in fields {
        let mut cells = field_row(field, spec_id);
        if !with_revisions {
            cells.remove(REVISIONS_COLUMN);
        }
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out.push('\n');
    out
}

fn field_row(field: &PageField, spec_id: Option<&str>) -> Vec<String> {
    let cells = vec![
        field_cell(field),
        offset_cell(field),
        bit_cell(field),
        field.effective_bit_width().to_string(),
        type_cell(field),
        units_cell(field),
        access_cell(field),
        category_cell(field),
        volatility_cell(field),
        read_effect_cell(field),
        revision_cell(field, spec_id),
        cite(field.provenance.as_ref()),
        description_cell(field),
    ];
    debug_assert_eq!(cells.len(), FIELD_COLUMNS.len());
    cells
}

fn field_cell(field: &PageField) -> String {
    let mut s = format!("`{}`", md_code(&field.name));
    if let Some(inst) = &field.instances {
        s.push_str(&format!(
            " ×{} (`{}` {}–{})",
            inst.end - inst.start + 1,
            md_code(&inst.index),
            inst.start,
            inst.end
        ));
    }
    s
}

fn offset_cell(field: &PageField) -> String {
    if let Some(expr) = &field.offset_expr {
        format!("`{}`", md_code(expr))
    } else if let Some(offset) = field.offset {
        offset.to_string()
    } else {
        NOT_RECORDED.to_string()
    }
}

fn bit_cell(field: &PageField) -> String {
    if let Some(expr) = &field.bit_offset_expr {
        format!("`{}`", md_code(expr))
    } else if let Some(bit) = field.bit_offset {
        bit.to_string()
    } else {
        "—".to_string()
    }
}

fn type_cell(field: &PageField) -> String {
    let mut s = match (&field.ty, &field.enum_ref, &field.composite_ref) {
        (ty, Some(r), _) => format!("`{}` → `{}`", ty.wire_name(), md_code(r)),
        (FieldType::Composite, _, Some(r)) => format!("`composite` → `{}`", md_code(r)),
        (t, _, _) => {
            let name = t.wire_name();
            if matches!(t, FieldType::Ascii | FieldType::Bytes) {
                match field.size_bytes {
                    Some(n) => format!("`{name}[{n}]`"),
                    None => format!("`{name}`"),
                }
            } else {
                format!("`{name}`")
            }
        }
    };
    if let Some((start, end)) = field.checksum_region() {
        s.push_str(&format!(" · sum8 checksum over bytes {start}–{end}"));
    }
    if let Some(enc) = &field.encoding {
        if let Some(kind) = encoding_kind(enc) {
            s.push_str(&format!(" · `{}`", md_code(&kind)));
        }
        if let Some(severity) = encoding_str(enc, "severity_order") {
            s.push_str(&format!(" · severity: {severity}"));
        }
    }
    s
}

/// Recorded encoding kind: a plain string (`bcd`) or the `kind:` key of a
/// structured encoding map. Returns None when nothing is recorded.
fn encoding_kind(enc: &serde_norway::Value) -> Option<String> {
    if let Some(s) = enc.as_str() {
        return Some(s.to_string());
    }
    enc.as_mapping().and_then(|m| m.get("kind")).map(scalar)
}

fn encoding_str(enc: &serde_norway::Value, key: &str) -> Option<String> {
    enc.as_mapping()
        .and_then(|m| m.get(key))
        .map(|v| md_text(&scalar(v)))
}

/// Recorded units: the field-level `unit:`/`units:` value, else the unit the
/// structured `encoding:` map records (`unit:` / `lsb_units:`). Never derived
/// from the wire type.
fn units_cell(field: &PageField) -> String {
    if let Some(unit) = &field.unit {
        return md_text(unit);
    }
    if let Some(enc) = &field.encoding {
        if let Some(map) = enc.as_mapping() {
            for key in ["unit", "lsb_units"] {
                if let Some(v) = map.get(key) {
                    return md_text(&scalar(v));
                }
            }
        }
    }
    NOT_RECORDED.to_string()
}

fn access_cell(field: &PageField) -> String {
    match field.access {
        Some(access) => format!("`{}`", access.canonical_name()),
        None => NOT_RECORDED.to_string(),
    }
}

fn category_cell(field: &PageField) -> String {
    match &field.field_category {
        Some(category) => {
            let mut s = md_text(category);
            if !field.tags.is_empty() {
                s.push_str(&format!(
                    " · tags: {}",
                    field
                        .tags
                        .iter()
                        .map(|t| md_text(t))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            s
        }
        None => {
            if field.tags.is_empty() {
                NOT_RECORDED.to_string()
            } else {
                format!(
                    "not recorded · tags: {}",
                    field
                        .tags
                        .iter()
                        .map(|t| md_text(t))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

fn volatility_cell(field: &PageField) -> String {
    use crate::ir::pages::ChangeSourceMetadata;
    let Some(source) = field.change_source() else {
        return NOT_RECORDED.to_string();
    };
    let name = match source {
        ChangeSourceMetadata::Static => "`static`",
        ChangeSourceMetadata::Dynamic => "`dynamic`",
        ChangeSourceMetadata::Unknown => "`unknown`",
    };
    if field.metadata.change_source.is_some() {
        // Recorded metadata renders verbatim.
        name.to_string()
    } else {
        // change_source() fell through to the is_dynamic flag: render the
        // inferred value with its derivation, never as recorded metadata.
        match field.flags.as_ref().and_then(|f| f.is_dynamic) {
            Some(dynamic) => format!("{name} (implied by `is_dynamic: {dynamic}`)"),
            None => format!("{name} (implied by the `is_dynamic` flag)"),
        }
    }
}

fn read_effect_cell(field: &PageField) -> String {
    let recorded = field.metadata.read_effect;
    let mut s = field
        .read_effects()
        .iter()
        .map(|e| {
            let name = format!("`{}`", e.canonical_name());
            if recorded == Some(*e) {
                // Recorded metadata renders verbatim.
                name
            } else {
                // read_effects() appends exactly one effect implied by the
                // recorded access; mark it as derived, not recorded metadata.
                match field.access {
                    Some(access) => {
                        format!("{name} (implied by `{}` access)", access.canonical_name())
                    }
                    None => format!("{name} (implied by access)"),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" + ");
    if s.is_empty() {
        s.push_str(NOT_RECORDED);
    }
    match field.metadata.read_effect_scope {
        Some(ReadEffectScope::Byte) => s.push_str(" (byte scope)"),
        Some(ReadEffectScope::Field) => s.push_str(" (field scope)"),
        None => {}
    }
    if let Some(provenance) = &field.metadata.read_effect_provenance {
        s.push_str(&format!(" — {}", cite(Some(provenance))));
    }
    if let Some(authority) = &field.metadata.authority {
        s.push_str(&format!("; authority: {}", cite(Some(authority))));
    }
    s
}

/// Revision applicability of the union table. The window shown is the
/// recorded `since`/`until` window — never an introduction date inferred
/// from cumulative history: a field with no version keys renders as
/// `all revisions`, not `3.0+`.
fn revision_cell(field: &PageField, spec_id: Option<&str>) -> String {
    // The union table renders the recorded window against the family domain
    // of the page's own spec block, falling back to the CMIS domain for
    // spec-less pages exactly like the resolver does.
    let family_revs = crate::ir::version::family_revs_for_spec(spec_id);
    let applicable = applicable_revs(field, family_revs);
    let revs: Vec<&Rev> = applicable.iter().collect();
    let mut parts = vec![if revs.len() == family_revs.len() {
        "all revisions".to_string()
    } else if revs.len() == 1 {
        revs[0].as_str().to_string()
    } else {
        format!(
            "{}–{}",
            revs.first().unwrap().as_str(),
            revs.last().unwrap().as_str()
        )
    }];
    if let Some(changes) = &field.changed_in {
        let revs = changes
            .iter()
            .map(|c| c.rev.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("changed in {revs}"));
    }
    if let Some(renamed) = &field.renamed_from {
        parts.push(format!("renamed in {}", renamed.rev));
    }
    if let Some(aliases) = &field.aliased_to {
        let revs = aliases
            .iter()
            .map(|a| a.rev.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("aliased in {revs}"));
    }
    parts.join("; ")
}

fn description_cell(field: &PageField) -> String {
    let mut s = String::new();
    if let Some(present_when) = &field.present_when {
        s.push_str(&format!(
            "present when {} — ",
            render_condition(present_when)
        ));
    }
    if let Some(description) = &field.description {
        s.push_str(&md_text(description));
    } else {
        // Display names and presence conditions are not descriptions.
        s.push_str(NOT_RECORDED);
    }
    s
}

/// Render a `present_when` condition (string or mapping of predicates).
fn render_condition(value: &serde_norway::Value) -> String {
    if let Some(s) = value.as_str() {
        return md_text(s);
    }
    if let Some(map) = value.as_mapping() {
        let parts: Vec<String> = map
            .iter()
            .map(|(k, v)| format!("{}: {}", scalar(k), scalar(v)))
            .collect();
        if !parts.is_empty() {
            return md_text(&parts.join(", "));
        }
    }
    md_text(&format!("{value:?}"))
}

/// Inline enum values (`enum_inline:`) for bool / small-int fields.
/// `heading` is the Markdown heading prefix for per-field subheads (one
/// level below the enclosing section heading).
fn inline_enum_section(heading: &str, fields: &[PageField]) -> String {
    let annotated: Vec<&PageField> = fields.iter().filter(|f| f.enum_inline.is_some()).collect();
    if annotated.is_empty() {
        return String::new();
    }
    let mut out = String::from("Inline enum values:\n\n");
    for field in annotated {
        out.push_str(&format!("{} `{}`\n\n", heading, md_code(&field.name)));
        let Some(entries) = field.enum_inline.as_ref().and_then(|v| v.as_sequence()) else {
            out.push_str("enum_inline is recorded but not a list — see the source schema.\n\n");
            continue;
        };
        out.push_str("| Value | Name | Description |\n|---|---|---|\n");
        for entry in entries {
            let map = entry.as_mapping();
            let value = map
                .and_then(|m| m.get("value"))
                .map(|v| match v.as_u64() {
                    Some(n) => super::value_hex(n),
                    None => md_text(&scalar(v)),
                })
                .unwrap_or_else(|| "not recorded".to_string());
            let name = map
                .and_then(|m| m.get("name"))
                .map(|v| format!("`{}`", md_code(&scalar(v))))
                .unwrap_or_else(|| NOT_RECORDED.to_string());
            let description = map
                .and_then(|m| m.get("description"))
                .map(|v| md_text(&scalar(v)))
                .unwrap_or_else(|| NOT_RECORDED.to_string());
            out.push_str(&format!("| {value} | {name} | {description} |\n"));
        }
        out.push('\n');
    }
    out
}

/// Polymorphic variant tables (`variants:`,).
fn variants_section(heading: &str, fields: &[PageField]) -> String {
    let annotated: Vec<&PageField> = fields.iter().filter(|f| f.variants.is_some()).collect();
    if annotated.is_empty() {
        return String::new();
    }
    let mut out = String::from("Polymorphic variants:\n\n");
    for field in annotated {
        out.push_str(&format!("{} `{}`\n\n", heading, md_code(&field.name)));
        let Some(variants) = field.variants.as_ref() else {
            unreachable!("filtered above");
        };
        let discriminator = variants
            .as_mapping()
            .and_then(|m| m.get("discriminator"))
            .map(|d| {
                d.as_mapping()
                    .and_then(|dm| dm.get("field"))
                    .map(scalar)
                    .unwrap_or_else(|| scalar(d))
            });
        match discriminator {
            Some(d) => out.push_str(&format!("Discriminated by `{}`:\n\n", md_code(&d))),
            None => out.push_str("Discriminator: not recorded\n\n"),
        }
        let Some(cases) = variants
            .as_mapping()
            .and_then(|m| m.get("cases"))
            .and_then(|c| c.as_sequence())
        else {
            out.push_str(
                "variants are recorded but no case list could be read — see the source schema.\n\n",
            );
            continue;
        };
        out.push_str("| When | Type / enum | Description |\n|---|---|---|\n");
        for case in cases {
            let map = case.as_mapping();
            let when = map
                .and_then(|m| m.get("when"))
                .map(|w| {
                    if let Some(v) = w.as_mapping().and_then(|m| m.get("value")) {
                        match v.as_u64() {
                            Some(n) => super::value_hex(n),
                            None => md_text(&scalar(v)),
                        }
                    } else if let Some(range) = w
                        .as_mapping()
                        .and_then(|m| m.get("range"))
                        .and_then(|r| r.as_sequence())
                    {
                        if range.len() == 2 {
                            format!(
                                "{}–{}",
                                range[0]
                                    .as_u64()
                                    .map(super::value_hex)
                                    .unwrap_or_else(|| md_text(&scalar(&range[0]))),
                                range[1]
                                    .as_u64()
                                    .map(super::value_hex)
                                    .unwrap_or_else(|| md_text(&scalar(&range[1])))
                            )
                        } else {
                            md_text(&format!("{w:?}"))
                        }
                    } else {
                        md_text(&scalar(w))
                    }
                })
                .unwrap_or_else(|| NOT_RECORDED.to_string());
            let ty = map
                .and_then(|m| m.get("enum_ref"))
                .map(|v| format!("`{}`", md_code(&scalar(v))))
                .or_else(|| {
                    map.and_then(|m| m.get("type"))
                        .map(|v| format!("`{}`", md_code(&scalar(v))))
                })
                .unwrap_or_else(|| "—".to_string());
            let description = map
                .and_then(|m| m.get("description"))
                .map(|v| md_text(&scalar(v)))
                .unwrap_or_else(|| NOT_RECORDED.to_string());
            out.push_str(&format!("| {when} | {ty} | {description} |\n"));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::pages::{Access, ChangeSourceMetadata, FieldFlags, ReadEffectMetadata};

    #[test]
    fn offset_cell_prefers_expression() {
        let mut f = plain_field("f");
        f.offset = Some(3);
        f.offset_expr = Some("128 + (lane - 1) * 2".into());
        assert_eq!(offset_cell(&f), "`128 + (lane - 1) * 2`");
        f.offset_expr = None;
        assert_eq!(offset_cell(&f), "3");
        f.offset = None;
        assert_eq!(offset_cell(&f), NOT_RECORDED);
    }

    fn plain_field(name: &str) -> PageField {
        PageField {
            name: name.to_string(),
            display_name: None,
            offset: None,
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: None,
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

    #[test]
    fn units_cell_reads_field_unit_then_encoding() {
        let mut f = plain_field("f");
        assert_eq!(units_cell(&f), NOT_RECORDED);
        f.unit = Some("0.1 ms".into());
        assert_eq!(units_cell(&f), "0.1 ms");
        f.unit = None;
        f.encoding = Some(
            serde_norway::from_str("{ kind: scaled_signed, lsb_units: \"0.001 GHz\" }").unwrap(),
        );
        assert_eq!(units_cell(&f), "0.001 GHz");
    }

    #[test]
    fn revision_cell_all_revisions_without_metadata() {
        let f = plain_field("f");
        assert_eq!(revision_cell(&f, None), "all revisions");
    }

    #[test]
    fn volatility_cell_marks_flag_implied_values() {
        let mut f = plain_field("f");
        assert_eq!(volatility_cell(&f), NOT_RECORDED);
        // Recorded change_source renders verbatim, unmarked.
        f.metadata.change_source = Some(ChangeSourceMetadata::Static);
        assert_eq!(volatility_cell(&f), "`static`");
        // Values the resolver infers from the is_dynamic flag are marked as
        // implied, never presented as recorded metadata.
        f.metadata.change_source = None;
        f.flags = Some(FieldFlags {
            is_dynamic: Some(true),
            ..Default::default()
        });
        assert_eq!(
            volatility_cell(&f),
            "`dynamic` (implied by `is_dynamic: true`)"
        );
        f.flags = Some(FieldFlags {
            is_dynamic: Some(false),
            ..Default::default()
        });
        assert_eq!(
            volatility_cell(&f),
            "`static` (implied by `is_dynamic: false`)"
        );
    }

    #[test]
    fn read_effect_cell_marks_access_implied_values() {
        let mut f = plain_field("f");
        // Access alone implies an effect: mark it as implied by that access.
        f.access = Some(Access::RoCor);
        assert_eq!(
            read_effect_cell(&f),
            "`clear_on_read` (implied by `ro_cor` access)"
        );
        f.access = Some(Access::Wo);
        assert_eq!(
            read_effect_cell(&f),
            "`write_only` (implied by `wo` access)"
        );
        // A recorded effect plus a different implied one renders both, with
        // only the implied one marked.
        f.metadata.read_effect = Some(ReadEffectMetadata::ClearOnRead);
        assert_eq!(
            read_effect_cell(&f),
            "`clear_on_read` + `write_only` (implied by `wo` access)"
        );
        // The same value recorded as metadata renders verbatim, unmarked.
        f.access = Some(Access::RoCor);
        f.metadata.read_effect = Some(ReadEffectMetadata::ClearOnRead);
        assert_eq!(read_effect_cell(&f), "`clear_on_read`");
        // A recorded `none` alongside the implied effect keeps both visible.
        f.metadata.read_effect = Some(ReadEffectMetadata::None);
        assert_eq!(
            read_effect_cell(&f),
            "`none` + `clear_on_read` (implied by `ro_cor` access)"
        );
    }
}
