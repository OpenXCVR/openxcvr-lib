//! Per-enum Markdown reference rendering.

use crate::ir::enums::{EntryValue, EnumEntry, EnumSchema};
use crate::ir::naming::camel_to_snake;

use super::{md_code, md_text, scalar, value_hex, GENERATED_BY};

const NOT_RECORDED: &str = "not recorded";

/// The `Revision applicability` bullet plus, when present, the per-entry
/// windows line. The statement text comes from the shared
/// [`crate::emitters::enum_revision_applicability`] helper so the Markdown
/// reference, the Rust rustdoc and the OpenAPI annotation state the same
/// recorded fact in the same words; only the bullet/code-span formatting is
/// Markdown-specific. Unannotated enums keep the canonical-catalog
/// disclosure byte-for-byte; annotated enums state the domain limit and list
/// each entry's window with its recorded (validated) revision strings.
fn revision_applicability_lines(schema: &EnumSchema) -> String {
    let (statement, annotated) = crate::emitters::enum_revision_applicability(schema);
    if annotated.is_empty() {
        return format!("- {statement}\n");
    }
    let mut out = format!("- {statement}\n");
    let windows = annotated
        .iter()
        .map(|entry| {
            let mut line = format!("`{}`", md_code(&entry.name));
            if let Some(since) = &entry.since {
                line.push_str(&format!(" since `{}`", md_code(since)));
            }
            if let Some(until) = &entry.until {
                line.push_str(&format!(" until `{}`", md_code(until)));
            }
            line
        })
        .collect::<Vec<_>>()
        .join(" · ");
    out.push_str(&format!("- Entry revision windows: {windows}\n"));
    out
}

pub fn enum_file_stem(schema: &EnumSchema) -> String {
    camel_to_snake(&schema.id)
}

/// One enum reference document.
pub fn emit_enum_doc(source_rel: &str, schema: &EnumSchema) -> String {
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
    // The enum-root `description:` the schema records, when one is recorded
    // (md_text applies the established table/HTML escaping and flattens
    // line breaks). Unrecorded stays absent — nothing substitutes, and the
    // references for enums without one are byte-identical to before.
    if let Some(desc) = schema
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        out.push_str(&format!("- Description: {}\n", md_text(desc)));
    }
    out.push_str(&format!("- Spec: {}\n", spec_line(schema)));
    out.push_str(&format!(
        "- Provenance: {}\n",
        super::cite(schema.provenance.as_ref())
    ));
    out.push_str(&revision_applicability_lines(schema));

    let tagged = schema.entries.iter().filter(|e| !e.tags.is_empty()).count();
    let vendor = schema
        .entries
        .iter()
        .filter(|e| e.vendor_specific.is_some())
        .count();
    if tagged > 0 || vendor > 0 {
        let mut parts: Vec<String> = Vec::new();
        if tagged > 0 {
            parts.push(format!(
                "entry `tags` ({} entr{} record{} them)",
                tagged,
                if tagged == 1 { "y" } else { "ies" },
                if tagged == 1 { "s" } else { "" }
            ));
        }
        if vendor > 0 {
            parts.push(format!(
                "entry `vendor_specific` ({} entr{} record{} it)",
                vendor,
                if vendor == 1 { "y" } else { "ies" },
                if vendor == 1 { "s" } else { "" }
            ));
        }
        out.push_str(&format!(
            "- Not rendered: {} — recorded in the source schema\n",
            parts.join(", ")
        ));
    }

    out.push_str("\n## Entries\n\n");
    out.push_str(&entries_table(&schema.entries));
    super::finish_doc(out)
}

/// The `spec:` identity line, or `not recorded`.
fn spec_line(schema: &EnumSchema) -> String {
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
            .map(|e| format!("`{}`", md_code(&scalar(e))))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("; errata: {errata}"));
    }
    s
}

fn value_cell(value: &EntryValue) -> String {
    match value {
        EntryValue::Single(v) => value_hex(*v),
        EntryValue::Range { start, end } => format!("{}–{}", value_hex(*start), value_hex(*end)),
    }
}

fn signaling_cell(entry: &EnumEntry) -> Option<String> {
    let sig = entry.signaling.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    if let Some(count) = sig.lane_count {
        parts.push(format!("{count} lane(s)"));
    }
    if let Some(rate) = sig.lane_rate_gbd {
        parts.push(format!("{rate} GBd"));
    }
    if let Some(modulation) = &sig.modulation {
        parts.push(modulation.clone());
    }
    if let Some(fec) = &sig.fec {
        parts.push(fec.clone());
    }
    Some(if parts.is_empty() {
        "recorded (no fields)".to_string()
    } else {
        parts.join(" · ")
    })
}

fn reach_cell(entry: &EnumEntry) -> Option<String> {
    let reach = entry.reach.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    if let Some(kind) = &reach.kind {
        parts.push(kind.clone());
    }
    if let Some(distance) = reach.distance_m {
        parts.push(format!("{distance} m"));
    }
    Some(if parts.is_empty() {
        "recorded (no fields)".to_string()
    } else {
        parts.join(" · ")
    })
}

fn entries_table(entries: &[EnumEntry]) -> String {
    let has_signaling = entries.iter().any(|e| e.signaling.is_some());
    let has_reach = entries.iter().any(|e| e.reach.is_some());
    let has_bit_rate = entries.iter().any(|e| e.bit_rate_gbps.is_some());

    let mut header = vec!["Value", "Name", "Aliases"];
    if has_signaling {
        header.push("Signaling");
    }
    if has_reach {
        header.push("Reach");
    }
    if has_bit_rate {
        header.push("Bit rate");
    }
    header.push("Description");

    let mut out = String::new();
    out.push_str(&format!("| {} |\n", header.join(" | ")));
    out.push_str(&format!("|{}|\n", vec!["---"; header.len()].join("|")));
    for entry in entries {
        let mut cells = vec![
            value_cell(&entry.value),
            format!("`{}`", md_code(&entry.name)),
            if entry.aliases.is_empty() {
                "—".to_string()
            } else {
                entry
                    .aliases
                    .iter()
                    .map(|a| format!("`{}`", md_code(a)))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        ];
        if has_signaling {
            cells.push(
                signaling_cell(entry)
                    .map(|s| md_text(&s))
                    .unwrap_or_else(|| "—".to_string()),
            );
        }
        if has_reach {
            cells.push(
                reach_cell(entry)
                    .map(|s| md_text(&s))
                    .unwrap_or_else(|| "—".to_string()),
            );
        }
        if has_bit_rate {
            cells.push(match entry.bit_rate_gbps {
                Some(rate) => format!("{rate} Gbps"),
                None => "—".to_string(),
            });
        }
        cells.push(
            entry
                .description
                .as_deref()
                .map(md_text)
                .unwrap_or_else(|| NOT_RECORDED.to_string()),
        );
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::enums::{EntryValue, EnumEntry};

    fn synth_enum(description: Option<&str>) -> EnumSchema {
        EnumSchema {
            spec: None,
            provenance: None,
            description: description.map(str::to_string),
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            id: "CmisCdbStatus".to_string(),
            title: "CMIS 5.3 CDB Status Byte".to_string(),
            entry_includes: vec![],
            entries: vec![EnumEntry {
                name: "success".to_string(),
                value: EntryValue::Single(0x01),
                description: Some("Command completed successfully".to_string()),
                aliases: vec![],
                tags: vec![],
                signaling: None,
                reach: None,
                bit_rate_gbps: None,
                vendor_specific: None,
                since: None,
                until: None,
                changed_in: None,
                renamed_from: None,
                aliased_to: None,
                provenance: None,
            }],
        }
    }

    #[test]
    fn enum_reference_renders_recorded_root_description() {
        let out = emit_enum_doc(
            "cmis/enums/cdb-status.yaml",
            &synth_enum(Some(
                "Recorded root description with | pipe and <b>tag</b>.",
            )),
        );
        let lines: Vec<&str> = out.lines().collect();
        let desc = lines
            .iter()
            .find(|l| l.starts_with("- Description:"))
            .unwrap_or_else(|| panic!("missing description bullet:\n{out}"));
        assert_eq!(
            *desc,
            "- Description: Recorded root description with \\| pipe and &lt;b&gt;tag&lt;/b&gt;.",
            "md_text escaping reuses the established table/HTML rules"
        );
        // The bullet sits with the recorded-facts list, after the identity
        // bullet and before the spec citation.
        let schema_at = lines
            .iter()
            .position(|l| l.starts_with("- Schema:"))
            .unwrap();
        let desc_at = lines
            .iter()
            .position(|l| l.starts_with("- Description:"))
            .unwrap();
        let spec_at = lines.iter().position(|l| l.starts_with("- Spec:")).unwrap();
        assert!(schema_at < desc_at && desc_at < spec_at, "{out}");
        // Multi-line recorded text stays one bullet: md_text maps each
        // CR and LF byte to a space (the established entries-table rule).
        let multi = emit_enum_doc(
            "cmis/enums/cdb-status.yaml",
            &synth_enum(Some("line one\r\nline two\nline three")),
        );
        assert!(
            multi
                .lines()
                .any(|l| l == "- Description: line one  line two line three"),
            "{multi}"
        );
    }

    #[test]
    fn enum_reference_omits_description_bullet_when_unrecorded() {
        // No recorded root description → no description bullet: the other
        // 44 enum references must stay byte-identical (regen equality).
        for absent in [None, Some(""), Some("  \n ")] {
            let out = emit_enum_doc("cmis/enums/module-state.yaml", &synth_enum(absent));
            assert!(
                !out.lines().any(|l| l.starts_with("- Description:")),
                "no description bullet may appear: {out}"
            );
        }
    }

    #[test]
    fn enum_reference_renders_real_cdb_status_description() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let schema =
            crate::parser::enums::load_enum_schema(&root.join("cmis/enums/cdb-status.yaml"))
                .unwrap();
        let out = emit_enum_doc("cmis/enums/cdb-status.yaml", &schema);
        assert!(
            out.lines().any(|l| l == "- Description: CdbStatus is an 8-bit status byte with bit 7 indicating busy, bit 6 indicating failed after completion, and bits 5-0 carrying the command result. When bit 7 is set, bit 6 is ignored by the coarse status rules."),
            "the real recorded root description renders in the reference:\n{out}"
        );
        // Real corpus counterproof: an enum without a root description
        // gains nothing.
        let module_state =
            crate::parser::enums::load_enum_schema(&root.join("cmis/enums/module-state.yaml"))
                .unwrap();
        let ms_out = emit_enum_doc("cmis/enums/module-state.yaml", &module_state);
        assert!(
            !ms_out.lines().any(|l| l.starts_with("- Description:")),
            "{ms_out}"
        );
    }

    #[test]
    fn value_cell_hex_and_range() {
        assert_eq!(value_cell(&EntryValue::Single(0x27)), "0x27");
        assert_eq!(
            value_cell(&EntryValue::Range {
                start: 0x27,
                end: 0x7f
            }),
            "0x27–0x7F"
        );
    }

    #[test]
    fn value_hex_zero_pads_to_two_digits() {
        assert_eq!(value_hex(0), "0x00");
        assert_eq!(value_hex(5), "0x05");
        assert_eq!(value_hex(0x100), "0x100");
    }
}
