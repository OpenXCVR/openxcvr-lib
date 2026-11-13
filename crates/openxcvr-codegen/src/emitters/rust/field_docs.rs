//! Schema-derived rustdoc for generated page struct fields.
//!
//! Everything shown here is read straight off the resolved `PageField` IR and
//! explicitly labelled as recorded — nothing is inferred, derived from names,
//! or upgraded from one revision's citation into another's meaning. The
//! rendering rules live with the (private) helpers below; this module exists
//! so the rust emitter can document each selected struct field from its
//! paired resolved `PageField` without importing the markdown reference
//! emitter's table-oriented escaping.

use crate::ir::enums::{EnumEntry, EnumSchema};
use crate::ir::pages::PageField;

/// Rustdoc lines for one selected struct field, at the struct member's
/// 4-space indentation. Every physical line of recorded text becomes its
/// own `///` line — there is no path by which a raw `\n` or `\r` from the
/// schema reaches the generated source outside that line discipline.
///
/// Everything is labelled as *recorded*: nothing here derives prose from
/// names, types, or one revision's citation; `display_name` never stands in
/// for a missing `description`.
pub(super) fn field_doc_lines(f: &PageField) -> String {
    let mut sections: Vec<Vec<String>> = Vec::new();

    // 1. Summary: the recorded description, verbatim (escaped), or an
    //    explicit "not recorded" — display_name is never a substitute.
    let description = f
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let mut summary: Vec<String> = Vec::new();
    if let Some(d) = description {
        for line in physical_lines(d) {
            summary.push(escape_line(&line));
        }
        sections.push(summary);
    } else {
        sections.push(vec!["Description: not recorded.".to_string()]);
    }

    // 2. Units: only an explicit field `unit` (parser alias included) or a
    //    recorded *string* `encoding.unit`. Absence omits the line entirely
    //    — the struct-level note explains that omission means unrecorded,
    //    not unitless; no unit is ever inferred from the field type or name.
    //    `encoding.lsb_units` — numeric or textual — is never a unit; it
    //    stays under encoding metadata below.
    let recorded_unit = f.unit.clone().or_else(|| {
        f.encoding
            .as_ref()
            .and_then(|e| e.get("unit"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });
    if let Some(unit) = recorded_unit {
        let lines = physical_lines(&format!("Recorded units: {unit}."));
        let section = lines.into_iter().map(|l| escape_line(&l)).collect();
        sections.push(section);
    }

    // 3. Recorded encoding/scaling metadata (lsb_units), if any — labelled
    //    as exactly what it is: recorded LSB/scaling metadata. It is never
    //    promoted to a unit, and it gets no blanket semantic denial either
    //    (recorded text such as `1/256 deg C` can legitimately contain
    //    units); the struct note carries the raw-not-asserted-scaled caveat.
    if let Some(lsb) = f
        .encoding
        .as_ref()
        .and_then(|e| e.get("lsb_units"))
        .map(crate::emitters::markdown::scalar)
    {
        let lines = physical_lines(&format!(
            "Recorded encoding/scaling metadata: lsb_units: {lsb}."
        ));
        let section = lines.into_iter().map(|l| escape_line(&l)).collect();
        sections.push(section);
    }

    // 4. Provenance: an explicit quotation, then the remaining citation
    //    metadata — both qualified "as recorded". A citation recorded
    //    against one revision is never evidence about another.
    if let Some(provenance) = &f.provenance {
        sections.extend(provenance_doc_sections(provenance));
    }

    // 5. since/until, only as recorded schema bounds. These are version
    //    bounds of the field itself; applicability qualification stays the
    //    independent read-revision decision and is never reinterpreted here.
    if f.since.is_some() || f.until.is_some() {
        let bounds: Vec<String> = [
            f.since.as_ref().map(|v| format!("since {v}")),
            f.until.as_ref().map(|v| format!("until {v}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let lines = physical_lines(&format!("Recorded schema bounds: {}.", bounds.join(", ")));
        let section = lines.into_iter().map(|l| escape_line(&l)).collect();
        sections.push(section);
    }

    render_doc_sections(&sections, "    ")
}

/// Struct-level rustdoc emitted once per generated page struct, above the
/// derive attribute. It carries the shared caveats that would otherwise be
/// repeated on every field: omitted unit metadata means *unrecorded* (never
/// a claim of unitless), and recorded units or LSB/scaling metadata never
/// assert that the raw struct member has already been scaled.
pub(super) fn struct_doc_lines() -> String {
    [
        "/// Field documentation is schema-recorded metadata. A field with no",
        "/// Recorded units line has no unit metadata recorded in the schema:",
        "/// absence means unrecorded, not unitless. Recorded units and recorded",
        "/// encoding/scaling metadata (lsb_units) do not assert that the raw",
        "/// struct member is already scaled to that unit.",
    ]
    .join("\n")
        + "\n"
}

/// Rustdoc lines for one generated enum (0-indent, above the derive
/// attribute of the `pub enum` or the range-only `pub struct` wrapper).
/// Everything is schema-recorded metadata labelled as recorded; the enum
/// schema language records an optional enum-level `description:` at the enum
/// root — when one is recorded it renders verbatim (trimmed), and when none
/// is the summary states that explicitly — `title` is a display name and
/// never substitutes for one.
pub(super) fn enum_doc_lines(schema: &EnumSchema) -> String {
    let mut sections: Vec<Vec<String>> = Vec::new();

    // 1. Summary: the recorded enum-root description, or the explicit
    //    statement that none is recorded — never a summary derived from the
    //    id or title. A recorded but whitespace-only description counts as
    //    not recorded (the same semantics the entry descriptions use).
    let description = schema
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    match description {
        Some(d) => {
            let section = physical_lines(&format!("Description (as recorded): {d}"))
                .into_iter()
                .map(|l| escape_line(&l))
                .collect();
            sections.push(section);
        }
        None => sections.push(vec!["Description: not recorded.".to_string()]),
    }

    // 2. Recorded spec identity (with errata as recorded), or its explicit
    //    absence — same rendering the Markdown reference uses.
    let spec_section = match &schema.spec {
        Some(spec) => {
            let mut line = format!("Spec (as recorded): {}", spec.id);
            if let Some(rev) = &spec.revision {
                line.push_str(&format!(" revision {rev}"));
            }
            if !spec.errata.is_empty() {
                // Map-form errata (`{ revision, source }`) render as
                // "revision (source)", the same shape the Markdown reference
                // uses; plain-string errata render verbatim.
                let errata = spec
                    .errata
                    .iter()
                    .map(|e| {
                        if let Some(map) = e.as_mapping() {
                            let rev = map
                                .get("revision")
                                .map(crate::emitters::markdown::scalar)
                                .unwrap_or_default();
                            let source = map
                                .get("source")
                                .map(crate::emitters::markdown::scalar)
                                .unwrap_or_default();
                            format!("{rev} ({source})")
                        } else {
                            crate::emitters::markdown::scalar(e)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                line.push_str(&format!("; errata: {errata}"));
            }
            line.push('.');
            physical_lines(&line)
                .into_iter()
                .map(|l| escape_line(&l))
                .collect()
        }
        None => vec!["Spec: not recorded.".to_string()],
    };
    sections.push(spec_section);

    // 3. Recorded provenance — the same rendering the page field docs use —
    //    or its explicit absence.
    match &schema.provenance {
        Some(provenance) => sections.extend(provenance_doc_sections(provenance)),
        None => sections.push(vec!["Provenance: not recorded.".to_string()]),
    }

    // 4. Revision applicability: the shared recorded-fact statement, the
    //    same words the Markdown reference emits (unmodeled stays explicit;
    //    nothing implies every revision is supported).
    let (statement, annotated) = crate::emitters::enum_revision_applicability(schema);
    let section = physical_lines(&statement)
        .into_iter()
        .map(|l| escape_line(&l))
        .collect();
    sections.push(section);

    // 5. Entry revision windows, exactly as recorded. Range entries never
    //    become variants, so this line is the only place their window is
    //    recorded on the Rust surface.
    if !annotated.is_empty() {
        let windows = annotated
            .iter()
            .map(|entry| {
                let mut line = entry.name.clone();
                if let Some(since) = &entry.since {
                    line.push_str(&format!(" since {since}"));
                }
                if let Some(until) = &entry.until {
                    line.push_str(&format!(" until {until}"));
                }
                line
            })
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        let section = physical_lines(&format!("Entry revision windows: {windows}."))
            .into_iter()
            .map(|l| escape_line(&l))
            .collect();
        sections.push(section);
    }

    render_doc_sections(&sections, "")
}

/// Rustdoc lines for one generated enum variant (4-indent, above the variant
/// declaration): the recorded entry description as the summary — or the
/// explicit not-recorded line — then the entry's recorded revision bounds.
/// The generated catch-all variant takes no docs: it is generator-authored,
/// not a schema entry.
pub(super) fn variant_doc_lines(entry: &EnumEntry) -> String {
    let mut sections: Vec<Vec<String>> = Vec::new();
    let description = entry
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    match description {
        Some(d) => {
            let section = physical_lines(d)
                .into_iter()
                .map(|l| escape_line(&l))
                .collect();
            sections.push(section);
        }
        None => sections.push(vec!["Description: not recorded.".to_string()]),
    }
    if entry.since.is_some() || entry.until.is_some() {
        let bounds: Vec<String> = [
            entry.since.as_ref().map(|v| format!("since {v}")),
            entry.until.as_ref().map(|v| format!("until {v}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        let section = physical_lines(&format!("Recorded schema bounds: {}.", bounds.join(", ")))
            .into_iter()
            .map(|l| escape_line(&l))
            .collect();
        sections.push(section);
    }
    render_doc_sections(&sections, "    ")
}

/// Recorded provenance rendered as one or two labelled sections: an explicit
/// quotation first (when the provenance is a mapping that carries `quote`),
/// then the remaining citation keys joined on one line. Plain-string or
/// exotic provenance renders whole under the same "as recorded" label;
/// nothing is ever invented or summarized. Shared by the page field docs and
/// the enum docs so the two surfaces cite identically.
fn provenance_doc_sections(provenance: &serde_norway::Value) -> Vec<Vec<String>> {
    let mut sections = Vec::new();
    if let Some(map) = provenance.as_mapping() {
        if let Some(quote) = map.get("quote") {
            let rendered = crate::emitters::markdown::scalar(quote);
            let lines = physical_lines(&format!("Recorded specification quotation: {rendered}"));
            sections.push(lines.into_iter().map(|l| escape_line(&l)).collect());
        }
        let rest: Vec<String> = map
            .iter()
            .filter(|(k, _)| crate::emitters::markdown::scalar(k) != "quote")
            .map(|(k, v)| {
                let key = crate::emitters::markdown::scalar(k);
                let value = if v.is_sequence() {
                    crate::emitters::markdown::scalar_seq(v)
                        .unwrap_or_else(|| crate::emitters::markdown::scalar(v))
                } else {
                    crate::emitters::markdown::scalar(v)
                };
                format!("{key}: {value}")
            })
            .collect();
        if !rest.is_empty() {
            let lines = physical_lines(&format!(
                "Schema provenance (as recorded): {}",
                rest.join(" \u{b7} ")
            ));
            sections.push(lines.into_iter().map(|l| escape_line(&l)).collect());
        }
    } else {
        // Plain-string or exotic provenance: render it under the same
        // "as recorded" label; never invent a quotation out of it.
        let rendered = crate::emitters::markdown::scalar(provenance);
        let lines = physical_lines(&format!("Schema provenance (as recorded): {rendered}"));
        sections.push(lines.into_iter().map(|l| escape_line(&l)).collect());
    }
    sections
}

/// Render labelled doc sections as `///` lines at the given indentation.
/// Sections are separated by a bare `///` line; an empty physical line inside
/// a section renders as a bare `///` line too. Every line here is a doc
/// comment, so the emitted documentation is strippable line-by-line without
/// touching any other token of the generated source.
fn render_doc_sections(sections: &[Vec<String>], indent: &str) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            out.push_str(&format!("{indent}///\n"));
        }
        for line in section {
            if line.is_empty() {
                out.push_str(&format!("{indent}///\n"));
            } else {
                out.push_str(&format!("{indent}/// {line}\n"));
            }
        }
    }
    out
}

/// Split recorded text into physical doc lines: CRLF and lone CR are line
/// breaks exactly like LF, every line is trimmed (leading indentation would
/// otherwise become a Markdown indented code block — a doctest risk), and
/// no `\r` survives into the generated source.
fn physical_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(|line| line.trim().to_string())
        .collect()
}

/// Make one physical line of recorded text inert in both Rust source and
/// rustdoc (CommonMark) rendering. Rust-source safety comes from the line
/// discipline itself; this is the Markdown side: backslash escapes for every
/// character that could open a link, code span/fence (backtick OR tilde —
/// CommonMark §4.5 treats `~~~` exactly like ```), HTML tag, entity, or
/// emphasis, plus line-leading constructs (headings, lists, setext rules).
/// Parens/quotes stay literal: they cannot open a construct once brackets,
/// backticks, tildes, and angle brackets are escaped.
fn escape_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => out.push_str("\\\\"),
            '`' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '[' | ']' | '<' | '>' | '*' | '&' => {
                out.push('\\');
                out.push(c);
            }
            '_' => {
                // Escape underscore runs of two or more (`__bold__`), and
                // single underscores unless they are strictly intraword —
                // flanked by alphanumerics on BOTH sides. A single
                // underscore at a word boundary (`_reserved_`, `(_flag_)`,
                // `_Note_:`) is a CommonMark §6.2 emphasis delimiter and
                // must be escaped to stay literal; `snake_case` stays
                // readable.
                let mut run = 1;
                while i + run < chars.len() && chars[i + run] == '_' {
                    run += 1;
                }
                let intraword = run == 1
                    && i > 0
                    && i + 1 < chars.len()
                    && chars[i - 1].is_alphanumeric()
                    && chars[i + 1].is_alphanumeric();
                if run >= 2 || !intraword {
                    for _ in 0..run {
                        out.push_str("\\_");
                    }
                } else {
                    out.push('_');
                }
                i += run - 1;
            }
            _ => out.push(c),
        }
        i += 1;
    }
    // Line-leading constructs: `-`/`+`/`#`/`=` (bullets, headings, setext
    // rules) are escaped with a leading backslash; an ordered-list marker
    // (`1.` / `1)`) is escaped at its punctuation — a backslash before a
    // digit is not a valid CommonMark escape.
    let mut first = out.clone();
    let chars: Vec<char> = first.chars().collect();
    if let Some(c0) = chars.first().copied() {
        if matches!(c0, '-' | '+' | '#' | '=' | '.') {
            first.insert(0, '\\');
        } else if c0.is_ascii_digit() {
            let digits = chars.iter().take_while(|c| c.is_ascii_digit()).count();
            if matches!(chars.get(digits), Some('.') | Some(')')) {
                let byte = first
                    .char_indices()
                    .nth(digits)
                    .map(|(i, _)| i)
                    .unwrap_or(first.len());
                first.insert(byte, '\\');
            }
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use crate::emitters::rust::pages::emit_page;
    use crate::emitters::rust::per_rev::emit_page_per_rev;
    use crate::ir::pages::{FieldType, PageField, PageSchema};
    use crate::ir::version::Rev;

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

    fn schema(id: &str, fields: Vec<PageField>) -> PageSchema {
        PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v2".to_string(),
            kind: "page".to_string(),
            id: id.to_string(),
            title: id.to_string(),
            page_address: "0x00".to_string(),
            size_bytes: Some(32),
            field_includes: vec![],
            fields,
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        }
    }

    /// Doc lines must sit directly above their struct field declaration.
    fn doc_lines_above<'a>(out: &'a str, field_decl: &str) -> Vec<&'a str> {
        let lines: Vec<&str> = out.lines().collect();
        let at = lines
            .iter()
            .position(|l| *l == field_decl)
            .unwrap_or_else(|| panic!("missing declaration {field_decl:?} in:\n{out}"));
        let mut doc = Vec::new();
        for line in lines[..at].iter().rev() {
            if line.trim_start().starts_with("///") {
                doc.push(line.trim_start());
            } else {
                break;
            }
        }
        doc.reverse();
        doc
    }

    #[test]
    fn escape_line_kills_line_leading_markdown_constructs() {
        assert_eq!(
            super::escape_line("# Heading"),
            "\\# Heading",
            "heading marker must be escaped"
        );
        assert_eq!(
            super::escape_line("1. ordered item"),
            "1\\. ordered item",
            "ordered-list marker escapes at its punctuation (a backslash before a digit is not a valid escape)"
        );
        assert_eq!(super::escape_line("- bullet"), "\\- bullet");
        assert_eq!(super::escape_line("= setext"), "\\= setext");
        // Intraword underscores stay readable; emphasis runs are escaped.
        assert_eq!(super::escape_line("snake_case_word"), "snake_case_word");
        assert_eq!(super::escape_line("__bold__"), "\\_\\_bold\\_\\_");
    }

    #[test]
    fn escape_line_makes_tilde_fences_inert() {
        // `~~~` opens a code fence exactly like ``` in CommonMark §4.5; an
        // unescaped tilde fence in a doc line becomes an executable doctest
        // (or a live code block) during `cargo test --doc`. Every tilde must
        // be backslash-escaped so the fence can never open.
        assert_eq!(super::escape_line("~~~"), "\\~\\~\\~");
        assert_eq!(super::escape_line("~~~rust"), "\\~\\~\\~rust");
        assert_eq!(
            super::escape_line("~~~rust compile_error!(\"must remain prose\");"),
            "\\~\\~\\~rust compile_error!(\"must remain prose\");"
        );
        assert_eq!(
            super::escape_line("close ~~~ then text"),
            "close \\~\\~\\~ then text",
            "a tilde run mid-line (a closing-fence candidate once any fence is
             open) must stay escaped too"
        );
        assert_eq!(super::escape_line("single ~ tilde"), "single \\~ tilde");
    }

    #[test]
    fn escape_line_escapes_word_boundary_single_underscores_but_keeps_intraword() {
        // A single underscore is an emphasis delimiter unless it is strictly
        // intraword (flanked by alphanumerics on BOTH sides). `_reserved_`,
        // `(_flag_)` and `_Note_:` all open/close <em> in CommonMark §6.2.
        assert_eq!(
            super::escape_line("_reserved_"),
            "\\_reserved\\_",
            "leading/trailing single underscores are emphasis delimiters"
        );
        assert_eq!(super::escape_line("(_flag_)"), "(\\_flag\\_)");
        assert_eq!(super::escape_line("_Note_:"), "\\_Note\\_:");
        assert_eq!(super::escape_line("a _flag_ b"), "a \\_flag\\_ b");
        // Strictly intraword single underscores stay readable.
        assert_eq!(super::escape_line("snake_case_word"), "snake_case_word");
        assert_eq!(
            super::escape_line("path_monitoring_interval"),
            "path_monitoring_interval"
        );
        // Adjacent to punctuation or line edges: escaped, never emphasis.
        assert_eq!(super::escape_line("_x"), "\\_x");
        assert_eq!(super::escape_line("x_"), "x\\_");
        assert_eq!(super::escape_line("a -_- b"), "a -\\_- b");
        // The struct-note/label prose itself renders underscores literally.
        assert_eq!(
            super::escape_line("no unit metadata recorded"),
            "no unit metadata recorded"
        );
    }

    #[test]
    fn recorded_description_becomes_doc_summary() {
        let mut f = field("tx_power", 0);
        f.description = Some("Measured Tx power.".into());
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub tx_power: u8,");
        assert!(!docs.is_empty(), "no field docs emitted: {out}");
        assert_eq!(docs[0], "/// Measured Tx power.");
    }

    #[test]
    fn missing_description_is_explicit_and_display_name_never_substitutes() {
        let mut f = field("vendor_name", 0);
        f.display_name = Some("VendorName".into());
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub vendor_name: u8,");
        assert!(!docs.is_empty(), "no field docs emitted: {out}");
        assert_eq!(
            docs[0], "/// Description: not recorded.",
            "with no recorded description the doc must open with the explicit \
             not-recorded line, never the display_name: {docs:?}"
        );
        // The display_name is not a summary: no doc line may consist of it.
        assert!(
            !docs.iter().any(|l| l.trim() == "/// VendorName"),
            "display_name must never substitute for a description: {docs:?}"
        );
    }

    #[test]
    fn units_come_from_field_or_string_encoding_unit() {
        let mut f = field("path_monitoring_interval", 0);
        f.unit = Some("0.1 ms".into());
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub path_monitoring_interval: u8,");
        assert!(
            docs.iter()
                .any(|l| l.contains("/// Recorded units: 0.1 ms.")),
            "{docs:?}"
        );
        assert!(
            !docs.iter().any(|l| l.contains("not asserted to be scaled")),
            "the raw-representation disclaimer is hoisted to the struct note, \
             not repeated per field: {docs:?}"
        );

        // A recorded *string* encoding.unit is a unit source too.
        let mut f2 = field("clock_data_recovery_lock_time", 1);
        f2.encoding = Some(serde_norway::from_str("{ kind: q16_16_unsigned, unit: ns }").unwrap());
        let out2 = emit_page(&schema("CmisTestPage", vec![f2]));
        let docs2 = doc_lines_above(&out2, "    pub clock_data_recovery_lock_time: u8,");
        assert!(
            docs2.iter().any(|l| l.contains("/// Recorded units: ns")),
            "{docs2:?}"
        );
    }

    #[test]
    fn missing_unit_line_is_omitted_entirely_not_marked_unitless() {
        // Absent unit metadata is represented by OMITTING the units line,
        // never by emitting "not recorded" boilerplate per field and never
        // by inferring unitless from the field type or name. The struct-level
        // note carries the unrecorded-not-unitless explanation once.
        let out = emit_page(&schema("CmisTestPage", vec![field("raw_byte", 0)]));
        let docs = doc_lines_above(&out, "    pub raw_byte: u8,");
        assert!(
            !docs.iter().any(|l| l.contains("Recorded units")),
            "no units line at all when no unit is recorded: {docs:?}"
        );
    }

    #[test]
    fn struct_note_explains_unit_absence_once_and_hoists_raw_disclaimer() {
        let mut with_unit = field("path_monitoring_interval", 0);
        with_unit.unit = Some("0.1 ms".into());
        let without_unit = field("raw_byte", 1);
        let out = emit_page(&schema("CmisTestPage", vec![with_unit, without_unit]));

        // The struct note appears exactly once, at struct level, directly
        // above the derive attribute.
        let note_openers = out
            .lines()
            .filter(|l| l.contains("unrecorded, not unitless"))
            .count();
        assert_eq!(note_openers, 1, "exactly one struct note per struct: {out}");
        let lines: Vec<&str> = out.lines().collect();
        let derive_at = lines
            .iter()
            .position(|l| *l == "#[derive(Debug, Clone)]")
            .expect("derive line");
        // Walk up through the note block: every line above the derive up to
        // (and including) the note opener must be 0-indent `///` doc lines.
        let mut i = derive_at;
        while i > 0 && lines[i - 1].starts_with("///") {
            i -= 1;
        }
        assert!(
            lines[i..derive_at]
                .iter()
                .any(|l| l.contains("unrecorded, not unitless")),
            "struct note must sit directly above the struct: {}",
            lines[i..derive_at + 1].join("\n")
        );

        // The hoisted disclaimer lives in the struct note, once.
        assert_eq!(
            out.lines()
                .filter(|l| l.contains("already scaled to that unit"))
                .count(),
            1,
            "raw-scaling disclaimer exactly once (struct note): {out}"
        );
        // ...and no longer on any per-field doc line.
        let field_docs: Vec<&str> = out.lines().filter(|l| l.starts_with("    ///")).collect();
        assert!(
            !field_docs
                .iter()
                .any(|l| l.contains("not asserted to be scaled") || l.contains("already scaled")),
            "per-field docs must not repeat the disclaimer: {field_docs:?}"
        );

        // A struct with no emittable (documented) fields gets no note: the
        // note talks about the field documentation below it.
        let mut skipped_only = field("param_value", 0);
        skipped_only.variants =
            Some(serde_norway::from_str("discriminator:\n  field: x\n").unwrap());
        let empty = emit_page(&schema("CmisTestPage", vec![skipped_only]));
        assert!(
            !empty.contains("unrecorded, not unitless"),
            "no struct note without documented fields: {empty}"
        );
    }

    #[test]
    fn lsb_units_stays_recorded_scaling_metadata_without_semantic_denial() {
        // String lsb_units (sff-8472 style): recorded LSB/scaling metadata.
        // It never satisfies the units section, and it is never blanket-labelled
        // "not a physical unit" — recorded text such as `1/256 deg C` contains
        // units; the honest label is exactly what was recorded.
        let mut f = field("temperature_high_alarm_threshold", 0);
        f.encoding = Some(
            serde_norway::from_str("{ kind: scaled_signed, lsb_units: '1/256 deg C' }").unwrap(),
        );
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub temperature_high_alarm_threshold: u8,");
        assert!(
            !docs.iter().any(|l| l.contains("Recorded units")),
            "lsb_units must not satisfy the units section: {docs:?}"
        );
        assert!(
            docs.iter()
                .any(|l| l
                    .contains("/// Recorded encoding/scaling metadata: lsb_units: 1/256 deg C.")),
            "lsb_units must appear as recorded scaling metadata: {docs:?}"
        );
        assert!(
            !docs.iter().any(|l| l.contains("not a physical unit")),
            "no blanket semantic denial of recorded lsb_units text: {docs:?}"
        );

        // Numeric lsb_units — still metadata, and no unit is derived from it.
        let mut f2 = field("numeric_lsb_field", 1);
        f2.encoding =
            Some(serde_norway::from_str("{ kind: scaled_unsigned, lsb_units: 0.5 }").unwrap());
        let out2 = emit_page(&schema("CmisTestPage", vec![f2]));
        let docs2 = doc_lines_above(&out2, "    pub numeric_lsb_field: u8,");
        assert!(
            !docs2.iter().any(|l| l.contains("Recorded units")),
            "numeric lsb_units must not create a unit: {docs2:?}"
        );
        assert!(
            docs2
                .iter()
                .any(|l| l.contains("/// Recorded encoding/scaling metadata: lsb_units: 0.5.")),
            "{docs2:?}"
        );
    }

    #[test]
    fn provenance_quotation_is_labelled_separately_from_description() {
        let mut f = field("identifier_copy", 0);
        f.description = Some("Copy of the SFF-8024 identifier byte.".into());
        f.provenance = Some(
            serde_norway::from_str(
                "{ section: '§ 8.3.1', pdf_page: 161, table: 'Table 8-27', quote: 'This Byte shall contain the same value' }",
            )
            .unwrap(),
        );
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub identifier_copy: u8,");
        let quote = docs
            .iter()
            .find(|l| l.contains("Recorded specification quotation:"))
            .unwrap_or_else(|| panic!("quotation line missing: {docs:?}"));
        assert!(quote.contains("This Byte shall contain the same value"));
        assert!(!quote.contains("Copy of the SFF-8024 identifier byte."));
        let prov = docs
            .iter()
            .find(|l| l.contains("Schema provenance (as recorded):"))
            .unwrap_or_else(|| panic!("remaining provenance line missing: {docs:?}"));
        assert!(prov.contains("section: § 8.3.1"));
        assert!(prov.contains("pdf_page: 161"));
        assert!(
            !prov.contains("quote:"),
            "quote must not repeat inside the citation list: {prov}"
        );
    }

    #[test]
    fn unusual_provenance_shapes_stay_recorded_not_summarized() {
        // Sequences (pdf_pages: [141, 142]) and unknown keys render verbatim.
        let mut f = field("seq_prov", 0);
        f.provenance = Some(
            serde_norway::from_str("{ pdf_pages: [141, 142], odd_key: { nested: map } }").unwrap(),
        );
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub seq_prov: u8,");
        let prov = docs
            .iter()
            .find(|l| l.contains("Schema provenance (as recorded):"))
            .unwrap_or_else(|| panic!("{docs:?}"));
        assert!(prov.contains("pdf_pages: 141, 142"), "{prov}");
        assert!(
            prov.contains("odd_key:"),
            "unknown keys stay recorded: {prov}"
        );
    }

    #[test]
    fn since_until_render_only_as_recorded_schema_bounds() {
        let mut f = field("latency_monitor", 0);
        f.since = Some("4.0".into());
        f.until = Some("5.1".into());
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub latency_monitor: u8,");
        assert!(
            docs.iter()
                .any(|l| l.contains("/// Recorded schema bounds: since 4.0")
                    && l.contains("until 5.1")),
            "{docs:?}"
        );
    }

    #[test]
    fn adversarial_text_is_rendered_inert_and_line_safe() {
        let mut f = field("adversarial", 0);
        f.description = Some(
            "line1 [link](http://x) <b>tag</b> `code` \"quoted\" \\ backslash _reserved_ (_flag_) _Note_: snake_case_word\r\nline2 ```rust fn main() {}``` ~~~rust compile_error!(\"must remain prose\");~~~\nline3 <script>alert(1)</script> __bold__ *em* &amp; #heading ~ single"
                .into(),
        );
        f.provenance = Some(
            serde_norway::from_str(
                "{ quote: 'multi\nline \\| quote with ``` fence and ~~~ tilde fence' }",
            )
            .unwrap(),
        );
        let out = emit_page(&schema("CmisTestPage", vec![f]));
        assert!(
            !out.contains('\r'),
            "CR bytes must not leak into generated source: {out:?}"
        );
        // Every physical line of the recorded text becomes its own /// line.
        assert!(out.contains("/// line1"), "{out}");
        assert!(out.contains("/// line2"), "{out}");
        assert!(out.contains("/// line3"), "{out}");
        // Markdown structure characters are backslash-escaped, never live.
        assert!(out.contains("\\[link\\](http://x)"), "{out}");
        assert!(out.contains("\\<b\\>tag\\</b\\>"), "{out}");
        assert!(out.contains("\\`code\\`"), "{out}");
        assert!(out.contains("\\`\\`\\`rust"), "{out}");
        assert!(out.contains("\\<script\\>"), "{out}");
        assert!(out.contains("\\_\\_bold\\_\\_"), "{out}");
        assert!(out.contains("\\*em\\*"), "{out}");
        assert!(out.contains("\\&amp;"), "{out}");
        // Word-boundary single underscores stay literal, not emphasis.
        assert!(out.contains("\\_reserved\\_"), "{out}");
        assert!(out.contains("(\\_flag\\_)"), "{out}");
        assert!(out.contains("\\_Note\\_:"), "{out}");
        // Intraword single underscore stays readable.
        assert!(out.contains("snake_case_word"), "{out}");
        // Tilde fences are inert: every tilde is backslash-escaped, so no
        // `~~~` fence can ever open (a fence would turn the compile_error!
        // line into an executable doctest).
        assert!(
            out.contains("\\~\\~\\~rust compile_error!(\"must remain prose\");\\~\\~\\~"),
            "{out}"
        );
        // A bare fence/code-span can never open: every backtick and every
        // tilde in a doc line must be backslash-escaped.
        for line in out.lines().filter(|l| l.trim_start().starts_with("///")) {
            let body = line.trim_start().trim_start_matches('/');
            for (i, c) in body.char_indices() {
                if c == '`' || c == '~' {
                    assert!(
                        i > 0 && body[..i].ends_with('\\'),
                        "live (unescaped) {c:?} in doc line: {line}"
                    );
                }
            }
        }
    }

    #[test]
    fn skipped_fields_get_no_dangling_docs() {
        // A variants field (variable type) is skipped by the classifier: no
        // struct field and therefore no doc block may mention it.
        let mut skipped = field("param_value", 0);
        skipped.variants = Some(serde_norway::from_str("discriminator:\n  field: x\n").unwrap());
        skipped.description = Some("Skipped variable-type field prose.".into());
        let mut plain = field("plain_byte", 4);
        plain.description = Some("Described plain field.".into());
        let out = emit_page(&schema("CmisTestPage", vec![skipped, plain]));
        assert!(out.contains("    pub plain_byte: u8,"), "{out}");
        assert!(
            !out.contains("pub param_value:"),
            "skipped field must not become a struct field: {out}"
        );
        let mut saw_doc = false;
        for line in out.lines() {
            if line.trim_start().starts_with("///") {
                saw_doc = true;
                assert!(
                    !line.contains("param_value") && !line.contains("Skipped variable-type"),
                    "dangling doc for skipped field: {line}"
                );
            }
        }
        assert!(saw_doc, "plain described field must be documented: {out}");
    }

    #[test]
    fn revision_filtered_fields_carry_their_own_docs() {
        let mut old = field("legacy_field", 0);
        old.description = Some("Present in every revision.".into());
        let mut late = field("new_field", 1);
        late.description = Some("Only since 4.0.".into());
        late.since = Some("4.0".into());
        let page = schema("CmisTestPage", vec![old, late]);
        let v3 = emit_page_per_rev(&page, Rev::V3_0);
        assert!(
            !v3.contains("new_field"),
            "4.0 field leaked into v3.0: {v3}"
        );
        assert!(v3.contains("/// Present in every revision."), "{v3}");
        let v4 = emit_page_per_rev(&page, Rev::V4_0);
        assert!(v4.contains("/// Only since 4.0."), "{v4}");
        assert!(v4.contains("/// Present in every revision."), "{v4}");
    }

    #[test]
    fn instanced_fields_get_docs() {
        let mut f = field("parameter_high_alarm_flag", 0);
        f.ty = FieldType::Bool;
        f.bit_offset = Some(0);
        f.instances = Some(crate::ir::pages::InstancesDesc {
            index: "param".into(),
            start: 1,
            end: 4,
        });
        f.description = Some("Per-parameter alarm flag.".into());
        let out = emit_page(&schema("Sff8636TestPage", vec![f]));
        let docs = doc_lines_above(&out, "    pub parameter_high_alarm_flag: [bool; 4],");
        assert!(docs.contains(&"/// Per-parameter alarm flag."), "{docs:?}");
    }

    // ── real-schema coverage: one representative page per family ─────────────

    fn load(relative: &str) -> PageSchema {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        crate::parser::pages::load_page_schema(&root.join(relative)).unwrap()
    }

    #[test]
    fn cmis_page00h_vendor_name_missing_description_and_recorded_quotation() {
        let page = load("cmis/page-00h-administrative.yaml");
        let out = emit_page(&page);
        let vendor = doc_lines_above(&out, "    pub vendor_name: [u8; 16],");
        assert_eq!(
            vendor[0], "/// Description: not recorded.",
            "vendor_name has no recorded description; that must be explicit, \
             never the display_name: {vendor:?}"
        );
        assert!(
            vendor
                .iter()
                .any(|l| l.contains("Recorded specification quotation:")
                    && l.contains("16 character read-only field")),
            "vendor_name carries a recorded quotation: {vendor:?}"
        );
        assert!(
            vendor
                .iter()
                .any(|l| l.contains("Schema provenance (as recorded): section: § 8.3.2.1")),
            "{vendor:?}"
        );
        // The identifier-copy quotation renders too.
        let ident = doc_lines_above(&out, "    pub sff8024_identifier_copy: Sff8024Identifier,");
        assert!(
            ident
                .iter()
                .any(|l| l.contains("Recorded specification quotation:")
                    && l.contains("SFF8024 Identifier Byte 00h:0")),
            "{ident:?}"
        );
    }

    #[test]
    fn sff8472_diagnostics_records_lsb_units_as_encoding_metadata() {
        let page = load("sff-8472/page-a2h-diagnostics-base.yaml");
        let out = emit_page(&page);
        let docs = doc_lines_above(&out, "    pub temperature_threshold: [i16; 4],");
        assert!(
            !docs.iter().any(|l| l.contains("Recorded units")),
            "diagnostics units are lsb_units, not recorded field units, so no \
             units line is emitted: {docs:?}"
        );
        assert!(
            docs.iter()
                .any(|l| l
                    .contains("/// Recorded encoding/scaling metadata: lsb_units: 1/256 deg C.")),
            "raw diagnostic scaling must stay recorded as encoding metadata: {docs:?}"
        );
        assert!(
            !docs.iter().any(|l| l.contains("not a physical unit")),
            "recorded lsb_units text `1/256 deg C` contains units; the blanket \
             denial must be gone: {docs:?}"
        );
    }

    #[test]
    fn sff8636_page20h_telemetry_quote_and_bounds() {
        let page = load("sff-8636/page-20h-pam4-monitoring-1.yaml");
        let out = emit_page(&page);
        let docs = doc_lines_above(&out, "    pub param_low_warning_mask: [bool; 24],");
        assert!(
            !docs.iter().any(|l| l.contains("Recorded units")),
            "absent unit metadata omits the units line entirely: {docs:?}"
        );
        assert!(
            docs.iter()
                .any(|l| l.contains("Recorded specification quotation:")
                    && l.contains("warning low")),
            "{docs:?}"
        );
        // param_value carries variants (variable type) and is skipped by the
        // classifier: its prose must never dangle above another field.
        assert!(
            !out.contains("Real-Time Value Registers"),
            "skipped param_value prose must not render: {out}"
        );
    }

    // ── Rust enum documentation (schema-recorded, like the page fields) ──────

    use crate::emitters::rust::enums::RustEmitter;
    use crate::emitters::Emitter;
    use crate::ir::enums::{EntryValue, EnumEntry, EnumSchema};
    use crate::parser::enums::load_enum_schema;

    fn enum_entry(name: &str, value: u64) -> EnumEntry {
        EnumEntry {
            name: name.to_string(),
            value: EntryValue::Single(value),
            description: None,
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
        }
    }

    fn enum_schema(id: &str, entries: Vec<EnumEntry>) -> EnumSchema {
        EnumSchema {
            spec: None,
            provenance: None,
            description: None,
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            id: id.to_string(),
            title: format!("{id} Test Title"),
            entry_includes: vec![],
            entries,
        }
    }

    /// Remove every `///` doc-comment line (0- or 4-indent) — the exact lines
    /// the enum/variant documentation is allowed to add and nothing else.
    fn strip_doc_lines(out: &str) -> String {
        out.lines()
            .filter(|l| !l.trim_start().starts_with("///"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    #[test]
    fn enum_docs_render_recorded_spec_provenance_and_applicability() {
        let mut schema = enum_schema(
            "CmisDataPathState",
            vec![
                enum_entry("dp_deactivated", 0x1),
                enum_entry("dp_tx_turn_on", 0x5),
            ],
        );
        schema.spec = Some(crate::ir::pages::SpecRef {
            id: "cmis".to_string(),
            revision: Some("5.3".to_string()),
            errata: vec![],
            source: None,
            parity_deferred: None,
        });
        schema.provenance = Some(
            serde_norway::from_str(
                "{ spec: 'CMIS 5.3 § 6.3.3 / Table 6-13', pdf_pages: [80, 81] }",
            )
            .unwrap(),
        );
        schema.entries[0].description = Some("Deactivated data path.".into());
        schema.entries[1].description = Some("Tx turn-on state.".into());
        schema.entries[1].since = Some("4.0".to_string());
        schema.entries[1].until = Some("5.2".to_string());

        let out = RustEmitter.emit_enum(&schema);

        // Enum-level docs sit directly above the derive attribute.
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(!enum_docs.is_empty(), "no enum-level docs: {out}");
        assert!(
            enum_docs.contains(&"/// Description: not recorded."),
            "no enum-level description exists in the IR; that is explicit and \
             the title never substitutes: {enum_docs:?}"
        );
        assert!(
            !enum_docs.iter().any(|l| l.contains("Test Title")),
            "title/display name is never a description: {enum_docs:?}"
        );
        assert!(
            enum_docs.contains(&"/// Spec (as recorded): cmis revision 5.3."),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs.contains(
                &"/// Schema provenance (as recorded): spec: CMIS 5.3 § 6.3.3 / Table 6-13 · pdf_pages: 80, 81"
            ),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs.iter().any(|l| l.starts_with(
                "/// Revision applicability: revision-qualified — entry revision windows are \
                 interpreted within the known CMIS family domain (3.0, 4.0, 5.0, 5.1, 5.2, 5.3, 5.4); \
                 entries without a window apply throughout that domain only, never outside it"
            )),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs
                .iter()
                .any(|l| l.contains("Entry revision windows: dp_tx_turn_on since 4.0 until 5.2")),
            "{enum_docs:?}"
        );

        // Variant docs sit directly above their variant declaration.
        let dp_docs = doc_lines_above(&out, "    DpTxTurnOn,");
        assert_eq!(
            dp_docs.first(),
            Some(&"/// Tx turn-on state."),
            "{dp_docs:?}"
        );
        assert!(
            dp_docs.contains(&"/// Recorded schema bounds: since 4.0, until 5.2."),
            "{dp_docs:?}"
        );
        let deact_docs = doc_lines_above(&out, "    DpDeactivated,");
        assert_eq!(
            deact_docs.first(),
            Some(&"/// Deactivated data path."),
            "{deact_docs:?}"
        );
        assert!(
            !deact_docs
                .iter()
                .any(|l| l.contains("Recorded schema bounds")),
            "unannotated entry carries no bounds line: {deact_docs:?}"
        );
    }

    #[test]
    fn enum_docs_state_missing_metadata_explicitly() {
        // No spec, no provenance, no entry descriptions, no revision windows.
        let schema = enum_schema("CmisDataPathState", vec![enum_entry("dp_deactivated", 0x1)]);
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(&"/// Description: not recorded."),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs.contains(&"/// Spec: not recorded."),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs.contains(&"/// Provenance: not recorded."),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs.iter().any(|l| {
                l.starts_with(
                "/// Revision applicability: not modeled — this enum is a canonical catalog, not a \
                 revision-qualified contract; the enum IR carries no revision metadata, and \
                 revision annotations in the source schema are not interpreted"
            )
            }),
            "unannotated enums state the not-modeled disclosure: {enum_docs:?}"
        );
        // Every schema-derived variant states its missing description.
        let deact_docs = doc_lines_above(&out, "    DpDeactivated,");
        assert_eq!(
            deact_docs,
            vec!["/// Description: not recorded."],
            "{deact_docs:?}"
        );
    }

    #[test]
    fn enum_docs_adversarial_text_is_inert_and_line_safe() {
        let mut schema = enum_schema("CmisAdversarial", vec![enum_entry("adversarial", 0x1)]);
        schema.entries[0].description = Some(
            "line1 [link](http://x) <b>tag</b> `code` \"quoted\" \\ backslash _reserved_ (_flag_) snake_case_word\r\nline2 ```rust fn main() {}``` ~~~rust compile_error!(\"must remain prose\");~~~\nline3 <script>alert(1)</script> __bold__ *em* &amp; #heading ~ single"
                .into(),
        );
        schema.spec = Some(crate::ir::pages::SpecRef {
            id: "cmis <script>".to_string(),
            revision: Some("5.3 <b>".to_string()),
            errata: vec![],
            source: None,
            parity_deferred: None,
        });
        let out = RustEmitter.emit_enum(&schema);
        assert!(!out.contains('\r'), "CR bytes must not leak: {out:?}");
        assert!(out.contains("/// line1"), "{out}");
        assert!(out.contains("/// line2"), "{out}");
        assert!(out.contains("/// line3"), "{out}");
        assert!(out.contains("\\[link\\](http://x)"), "{out}");
        assert!(out.contains("\\<b\\>tag\\</b\\>"), "{out}");
        assert!(out.contains("\\`code\\`"), "{out}");
        assert!(out.contains("\\`\\`\\`rust"), "{out}");
        assert!(out.contains("\\<script\\>"), "{out}");
        assert!(out.contains("\\_\\_bold\\_\\_"), "{out}");
        assert!(out.contains("\\*em\\*"), "{out}");
        assert!(out.contains("\\&amp;"), "{out}");
        assert!(out.contains("\\_reserved\\_"), "{out}");
        assert!(out.contains("snake_case_word"), "{out}");
        assert!(
            out.contains("\\~\\~\\~rust compile_error!(\"must remain prose\");\\~\\~\\~"),
            "{out}"
        );
        // No live (unescaped) backtick or tilde in any doc line.
        for line in out.lines().filter(|l| l.trim_start().starts_with("///")) {
            let body = line.trim_start().trim_start_matches('/');
            for (i, c) in body.char_indices() {
                if c == '`' || c == '~' {
                    assert!(
                        i > 0 && body[..i].ends_with('\\'),
                        "live (unescaped) {c:?} in doc line: {line}"
                    );
                }
            }
        }
    }

    #[test]
    fn enum_docs_are_doc_comment_lines_only() {
        // Description/spec/provenance metadata may only add `///` lines: the
        // fully-annotated emission minus its doc lines is byte-identical to
        // the emission of the same schema with exactly those nulled. since/
        // until are kept on both sides — they also drive the pre-existing
        // projection surface, which is baseline behavior, not documentation.
        let mut rich = enum_schema(
            "CmisDataPathState",
            vec![
                enum_entry("dp_deactivated", 0x1),
                enum_entry("dp_tx_turn_on", 0x5),
            ],
        );
        rich.spec = Some(crate::ir::pages::SpecRef {
            id: "cmis".to_string(),
            revision: Some("5.3".to_string()),
            errata: vec![],
            source: None,
            parity_deferred: None,
        });
        rich.provenance =
            Some(serde_norway::from_str("{ spec: 'CMIS 5.3', pdf_pages: [80] }").unwrap());
        rich.description = Some("Recorded root description.".into());
        rich.entries[0].description = Some("Deactivated.".into());
        rich.entries[1].description = Some("Turn-on.".into());
        rich.entries[1].since = Some("4.0".to_string());

        let mut bare = rich.clone();
        bare.spec = None;
        bare.provenance = None;
        bare.description = None;
        for e in &mut bare.entries {
            e.description = None;
        }

        let rich_out = RustEmitter.emit_enum(&rich);
        let bare_out = RustEmitter.emit_enum(&bare);
        assert_eq!(
            strip_doc_lines(&rich_out),
            strip_doc_lines(&bare_out),
            "description/spec/provenance may only add doc-comment lines"
        );
        // Non-triviality: the two sides really differ in doc lines only.
        assert!(rich_out.contains("/// Deactivated."), "{rich_out}");
        assert!(
            rich_out.contains("/// Description (as recorded): Recorded root description."),
            "{rich_out}"
        );
        assert!(!bare_out.contains("/// Deactivated."), "{bare_out}");
        // Both sides keep the since-driven docs and the projection surface.
        assert!(
            bare_out.contains("/// Recorded schema bounds: since 4.0.")
                && bare_out.contains("pub fn for_revision(")
        );
        // And a schema with no metadata at all emits ONLY the always-present
        // doc block — no doc line may appear that is not either a bare `///`
        // separator, one of the three not-recorded lines, or the shared
        // applicability statement.
        let none = enum_schema("CmisDataPathState", vec![enum_entry("dp_deactivated", 0x1)]);
        let none_out = RustEmitter.emit_enum(&none);
        for line in none_out
            .lines()
            .filter(|l| l.trim_start().starts_with("///"))
        {
            let t = line.trim_start();
            assert!(
                t == "///"
                    || t == "/// Description: not recorded."
                    || t == "/// Spec: not recorded."
                    || t == "/// Provenance: not recorded."
                    || t.starts_with("/// Revision applicability: not modeled"),
                "unexpected doc line for an unrecorded-metadata schema: {line}"
            );
        }
    }

    #[test]
    fn enum_docs_range_only_newtype_gets_enum_level_docs() {
        let mut schema = enum_schema(
            "CmisCdbCommandId",
            vec![EnumEntry {
                value: EntryValue::Range {
                    start: 0,
                    end: 0xffff,
                },
                provenance: None,
                ..enum_entry("vendor", 0)
            }],
        );
        schema.spec = Some(crate::ir::pages::SpecRef {
            id: "cmis".to_string(),
            revision: Some("5.3".to_string()),
            errata: vec![],
            source: None,
            parity_deferred: None,
        });
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("pub struct CmisCdbCommandId(pub u16);"),
            "{out}"
        );
        let docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            docs.contains(&"/// Spec (as recorded): cmis revision 5.3."),
            "range-only newtype keeps the enum-level docs: {docs:?}"
        );
    }

    #[test]
    fn enum_docs_render_recorded_root_description() {
        // The enum schema language records an optional enum-level description
        // at the enum root; when one is recorded it renders verbatim under
        // the "as recorded" label — never summarized from id or title.
        let mut schema = enum_schema("CmisCdbStatus", vec![enum_entry("success", 0x1)]);
        schema.description =
            Some("CdbStatus is an 8-bit status byte with bit 7 indicating busy.".into());
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(
                &"/// Description (as recorded): CdbStatus is an 8-bit status byte with bit 7 indicating busy."
            ),
            "{enum_docs:?}"
        );
        assert!(
            !enum_docs.contains(&"/// Description: not recorded."),
            "a recorded root description must replace the not-recorded line: {enum_docs:?}"
        );
        // The variant surface is untouched by the root description.
        let success_docs = doc_lines_above(&out, "    Success,");
        assert_eq!(
            success_docs,
            vec!["/// Description: not recorded."],
            "{success_docs:?}"
        );
    }

    #[test]
    fn enum_docs_root_description_adversarial_is_inert_and_line_safe() {
        let mut schema = enum_schema("CmisAdversarial", vec![enum_entry("adversarial", 0x1)]);
        schema.description = Some(
            "root1 [link](http://x) <b>`code`</b> _em_ ~~strike~~\r\nroot2 ```rust fn main() {}``` ~~~text fence~~~ <script>alert(1)</script> | pipe".into(),
        );
        let out = RustEmitter.emit_enum(&schema);
        assert!(!out.contains('\r'), "CR bytes must not leak: {out:?}");
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(
                &"/// Description (as recorded): root1 \\[link\\](http://x) \\<b\\>\\`code\\`\\</b\\> \\_em\\_ \\~\\~strike\\~\\~"
            ),
            "{enum_docs:?}"
        );
        let bt = "\\`".repeat(3);
        let expected_line2 = format!(
            "/// root2 {bt}rust fn main() {{}}{bt} \\~\\~\\~text fence\\~\\~\\~ \\<script\\>alert(1)\\</script\\> | pipe"
        );
        assert!(
            enum_docs.contains(&expected_line2.as_str()),
            "CRLF splits the root description into physical /// lines: {enum_docs:?}"
        );
        // No live (unescaped) backtick or tilde in any doc line.
        for line in enum_docs {
            let body = line.trim_start().trim_start_matches('/');
            for (i, c) in body.char_indices() {
                if c == '`' || c == '~' {
                    assert!(
                        i > 0 && body[..i].ends_with('\\'),
                        "live (unescaped) {c:?} in doc line: {line}"
                    );
                }
            }
        }
    }

    #[test]
    fn enum_docs_root_description_whitespace_only_is_not_recorded() {
        // A present but empty/whitespace description counts as not recorded
        // — the same semantics the entry descriptions and the OpenAPI
        // emitter use. Absence stays explicit, never silently dropped.
        for empty in ["", " \t\n "] {
            let mut schema = enum_schema("CmisCdbStatus", vec![enum_entry("success", 0x1)]);
            schema.description = Some(empty.into());
            let out = RustEmitter.emit_enum(&schema);
            let enum_docs =
                doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
            assert!(
                enum_docs.contains(&"/// Description: not recorded."),
                "empty recorded description must state not-recorded: {enum_docs:?}"
            );
        }
    }

    #[test]
    fn catch_all_variant_has_no_docs() {
        // The catch-all is generator-authored (not a schema entry); it gets
        // no doc comment, and the projection code comments are unchanged.
        let schema = enum_schema("CmisDataPathState", vec![enum_entry("dp_deactivated", 0x1)]);
        let out = RustEmitter.emit_enum(&schema);
        assert!(out.contains("    Unknown(u8),"), "{out}");
        assert_eq!(
            doc_lines_above(&out, "    Unknown(u8),"),
            Vec::<&str>::new(),
            "generated catch-all carries no schema doc: {out}"
        );
    }

    // ── real-schema coverage: one representative enum per family + shared 8024

    fn load_enum(relative: &str) -> EnumSchema {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        load_enum_schema(&root.join(relative)).unwrap()
    }

    #[test]
    fn cmis_module_state_enum_docs_from_real_schema() {
        let schema = load_enum("cmis/enums/module-state.yaml");
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(&"/// Spec (as recorded): cmis revision 5.3."),
            "{enum_docs:?}"
        );
        assert!(
            enum_docs
                .iter()
                .any(|l| l.contains("Schema provenance (as recorded):")
                    && l.contains("spec: CMIS 5.3 § 6.3.2 / Table 8-7")
                    && l.contains("pdf_pages: 80, 81, 82, 83, 144")),
            "{enum_docs:?}"
        );
        let low_pwr = doc_lines_above(&out, "    ModuleLowPwr,");
        assert!(
            low_pwr.iter().any(|l| l.contains(
                "ModuleLowPwr - steady state where the management interface is initialized"
            )),
            "{low_pwr:?}"
        );
        let reserved = doc_lines_above(&out, "    Reserved0,");
        assert_eq!(reserved, vec!["/// Reserved"], "{reserved:?}");
    }

    #[test]
    fn sff8024_identifier_enum_docs_from_real_schema() {
        let schema = load_enum("sff-8024/identifier-values.yaml");
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(&"/// Spec (as recorded): sff-8024 revision 4.14."),
            "{enum_docs:?}"
        );
        let gbic = doc_lines_above(&out, "    Gbic,");
        assert_eq!(gbic, vec!["/// GBIC"], "{gbic:?}");
    }

    #[test]
    fn sff8636_rate_select_enum_docs_from_real_schema() {
        let schema = load_enum("sff-8636/enums/rate-select.yaml");
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(&"/// Spec (as recorded): sff-8636 revision 2.11."),
            "{enum_docs:?}"
        );
        let low = doc_lines_above(&out, "    LowRate,");
        assert!(
            low.iter()
                .any(|l| l.contains("Version 1 less than 2.2 GBd")),
            "{low:?}"
        );
    }

    #[test]
    fn sff8472_rate_identifier_enum_docs_from_real_schema() {
        let schema = load_enum("sff-8472/enums/rate-identifier.yaml");
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(&"/// Spec (as recorded): sff-8472 revision 12.5a."),
            "{enum_docs:?}"
        );
        let unspecified = doc_lines_above(&out, "    Unspecified,");
        assert!(
            unspecified
                .iter()
                .any(|l| l.contains("Unspecified rate select behavior")),
            "{unspecified:?}"
        );
    }

    #[test]
    fn cmis_cdb_status_enum_docs_from_real_schema() {
        // End-to-end parse → generate for the one real enum whose schema
        // records a root `description:` — the input the previous blanket
        // "not recorded" line silently dropped.
        let schema = load_enum("cmis/enums/cdb-status.yaml");
        assert_eq!(
            schema.description.as_deref(),
            Some(
                "CdbStatus is an 8-bit status byte with bit 7 indicating busy, bit 6 \
                 indicating failed after completion, and bits 5-0 carrying the command \
                 result. When bit 7 is set, bit 6 is ignored by the coarse status rules.\n"
            ),
            "the parser must carry the recorded root description through the IR"
        );
        let out = RustEmitter.emit_enum(&schema);
        let enum_docs = doc_lines_above(&out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
        assert!(
            enum_docs.contains(
                &"/// Description (as recorded): CdbStatus is an 8-bit status byte with bit 7 indicating busy, bit 6 indicating failed after completion, and bits 5-0 carrying the command result. When bit 7 is set, bit 6 is ignored by the coarse status rules."
            ),
            "the recorded root description renders verbatim (trimmed): {enum_docs:?}"
        );
        // No false claim anywhere on this enum: every variant records a
        // description, so no not-recorded line may survive.
        assert!(
            !out.contains("Description: not recorded."),
            "CDB-status records every description; no not-recorded line may appear: {out}"
        );
        let success = doc_lines_above(&out, "    Success,");
        assert_eq!(
            success,
            vec!["/// Command completed successfully"],
            "{success:?}"
        );
    }
}
