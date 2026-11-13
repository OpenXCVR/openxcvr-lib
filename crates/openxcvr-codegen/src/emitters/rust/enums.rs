use crate::emitters::Emitter;
use crate::ir::enums::{EntryValue, EnumSchema};
use crate::ir::naming::{camel_to_snake, NamingPolicy, RustNaming};
use crate::ir::pages::PageSchema;
use std::collections::HashSet;

pub struct RustEmitter;

/// Revision-bound name projection block plus the revision-qualified decode
/// `from_rev`, emitted on every generated Rust enum. The projection surface
/// itself (type alias, factory, per-rev functions) appears only when the
/// schema's entries carry `since`/`until` metadata (see
/// [`crate::emitters::enum_projection_rows`]); `from_rev` is emitted either
/// way so a per-revision page decoder can call it uniformly on every enum it
/// references. The factory dispatches on the family's known revision bytes
/// (packed-BCD CMIS bytes, or SFF compliance codes); each per-rev function
/// names the applicable single entries and refuses
/// everything else. `invalid_expr` is the fold target for a raw the revision
/// does not name: `Self::{catch_all}(v)` for payload enums, `Self(v)` for a
/// range-only newtype.
fn projection_code(schema: &EnumSchema, repr: &str, invalid_expr: &str) -> String {
    // Family-specific revision-byte parameter name (`cmis_rev_byte` for the
    // CMIS family — byte-identical to the historical output; `sff_rev_byte`
    // for the SFF families, whose byte is a compliance code).
    let rev_byte = crate::emitters::rev_byte_param(schema);
    let Some(rows) = crate::emitters::enum_projection_rows(schema) else {
        // No entry carries revision metadata: the flat `From` conversion is
        // the correct decode at every revision, so `from_rev` is a total
        // pass-through. The unused `_{rev_byte}` keeps the exact call
        // shape of the gated form, so page decoders never need to know
        // which kind of enum they hold.
        return format!(
            "\nimpl {} {{\n    pub fn from_rev(_{}: u8, v: {repr}) -> Self {{\n        Self::from(v)\n    }}\n}}\n",
            schema.id,
            rev_byte
        );
    };
    let snake = camel_to_snake(&schema.id);
    let mut out = String::new();
    out.push_str(
        "\n// Revision-bound name projection: entry since/until windows resolved at\n// generator time (unannotated entries apply throughout the schema's known\n// family domain). The factory accepts exactly the family's known revision\n// bytes; a supported projection names the applicable single entries and\n// returns None for unnamed raws (reserved/vendor ranges included).\n",
    );
    out.push_str(&format!(
        "pub type {}Projection = fn({repr}) -> Option<&'static str>;\n\n",
        schema.id
    ));
    out.push_str(&format!(
        "impl {} {{\n    pub fn for_revision({}: u8) -> Option<{}Projection> {{\n        match {} {{\n",
        schema.id, rev_byte, schema.id, rev_byte
    ));
    for (rev, _) in &rows {
        out.push_str(&format!(
            "            {:#04x} => Some({snake}_projection_v{}),\n",
            rev.family_rev_byte(),
            rev.dirname()
        ));
    }
    out.push_str("            _ => None,\n        }\n    }\n");
    // Revision-qualified decode: `v` becomes a named variant only when this
    // revision's projection names that raw there; every other raw — reserved
    // at that revision, or any raw at an unknown revision byte — folds into
    // the catch-all and never borrows a later revision's names. Total: never
    // fails, never panics on untrusted EEPROM.
    out.push_str(&format!(
        "\n    pub fn from_rev({}: u8, v: {repr}) -> Self {{\n        if Self::for_revision({}).and_then(|proj| proj(v)).is_some() {{\n            Self::from(v)\n        }} else {{\n            {invalid_expr}\n        }}\n    }}\n",
        rev_byte, rev_byte
    ));
    out.push_str("}\n");
    for (rev, entries) in &rows {
        out.push_str(&format!(
            "\nfn {snake}_projection_v{}(raw: {repr}) -> Option<&'static str> {{\n    match raw {{\n",
            rev.dirname()
        ));
        for (value, name) in entries {
            out.push_str(&format!("        {value:#04x} => Some(\"{name}\"),\n"));
        }
        out.push_str("        _ => None,\n    }\n}\n");
    }
    out
}

impl Emitter for RustEmitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String {
        let max_single = schema
            .entries
            .iter()
            .filter_map(|e| e.value.as_single())
            .max()
            .unwrap_or(0);
        let repr = if max_single <= 0xFF {
            "u8"
        } else if max_single <= 0xFFFF {
            "u16"
        } else {
            "u32"
        };

        // Build deduplicated (entry, value, variant_name) list in one pass so the
        // enum body, From impl, and accessor methods all use consistent names.
        let mut used: HashSet<String> = HashSet::new();
        let single_entries: Vec<_> = schema
            .entries
            .iter()
            .filter_map(|e| match &e.value {
                EntryValue::Single(v) => {
                    let base = RustNaming.enum_variant(&e.name);
                    let variant = if used.insert(base.clone()) {
                        base
                    } else {
                        let mut candidate = format!("{base}V{v:02X}");
                        while !used.insert(candidate.clone()) {
                            candidate.push('_');
                        }
                        candidate
                    };
                    Some((e, *v, variant))
                }
                EntryValue::Range { .. } => None,
            })
            .collect();

        // Range-only enums (no single discriminant values) cannot be represented
        // as Rust enums — emit a newtype wrapper instead. It gets the same total
        // `From` + repr()/name() surface as the real-enum path (decode never
        // panics). The `repr` is the maximum range end, since no singles guide it.
        if single_entries.is_empty() {
            let max_range_end = schema
                .entries
                .iter()
                .filter_map(|e| {
                    if let EntryValue::Range { end, .. } = &e.value {
                        Some(*end)
                    } else {
                        None
                    }
                })
                .max()
                .unwrap_or(0xFF);
            let range_repr = if max_range_end <= 0xFF {
                "u8"
            } else if max_range_end <= 0xFFFF {
                "u16"
            } else {
                "u32"
            };
            let mut out = String::new();
            out.push_str("// Generated by openxcvr-codegen. DO NOT EDIT.\n");
            out.push_str(&format!("// Source: {}\n", schema.id));
            out.push('\n');
            // Schema-recorded rustdoc above the wrapper struct — doc lines
            // only, so the file stays byte-identical once stripped of `///`.
            out.push_str(&super::field_docs::enum_doc_lines(schema));
            out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
            out.push_str(&format!("pub struct {}(pub {range_repr});\n\n", schema.id));
            // Total `From` (never fails) + repr()/name() accessors mirroring the
            // real-enum path, so page decoders call `Type::from(..)` uniformly.
            out.push_str(&format!("impl From<{range_repr}> for {} {{\n", schema.id));
            out.push_str(&format!(
                "    fn from(v: {range_repr}) -> Self {{ Self(v) }}\n"
            ));
            out.push_str("}\n\n");
            out.push_str(&format!("impl {} {{\n", schema.id));
            out.push_str(&format!(
                "    pub fn repr(self) -> {range_repr} {{ self.0 }}\n"
            ));
            out.push_str("    pub fn name(self) -> String { format!(\"Unknown{}\", self.0) }\n");
            out.push_str("}\n");
            // An annotated range-only enum still gets its (all-refusing)
            // projection surface — the early return must not drop it. The
            // newtype has no catch-all variant, so a raw the revision does
            // not name folds back into the value itself.
            out.push_str(&projection_code(schema, range_repr, "Self(v)"));
            return out;
        }

        // Catch-all variant: absorbs every value not mapped to a single
        // (reserved/vendor ranges *and* any unlisted byte) so `From` is total and
        // decode never panics on untrusted EEPROM. Route its identifier through the
        // same `used` set — never reserve "Unknown" before the singles, which would
        // rename a legitimate `unknown` entry and break cross-binding name parity.
        // The rendered string is pinned to "Unknown{n}" regardless of the chosen
        // Rust identifier.
        let catch_all = {
            let base = "Unknown".to_string();
            if used.insert(base.clone()) {
                base
            } else {
                let mut candidate = format!("{base}_");
                while !used.insert(candidate.clone()) {
                    candidate.push('_');
                }
                candidate
            }
        };

        let mut out = String::new();
        out.push_str("// Generated by openxcvr-codegen. DO NOT EDIT.\n");
        out.push_str(&format!("// Source: {}\n", schema.id));
        out.push('\n');
        // Schema-recorded rustdoc above the derive attribute — doc lines
        // only, so the file stays byte-identical once stripped of `///`.
        out.push_str(&super::field_docs::enum_doc_lines(schema));
        out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
        out.push_str(&format!("pub enum {} {{\n", schema.id));
        // Singles in schema order; reserved/vendor ranges fold into the catch-all.
        // Each schema-derived variant carries its recorded entry documentation;
        // the generated catch-all takes none (it is not a schema entry).
        for (entry, _, variant) in &single_entries {
            out.push_str(&super::field_docs::variant_doc_lines(entry));
            out.push_str(&format!("    {variant},\n"));
        }
        out.push_str(&format!("    {catch_all}({repr}),\n"));
        out.push_str("}\n");

        // Total `From`: every value maps to a variant; unmapped → catch-all.
        out.push('\n');
        out.push_str(&format!("impl From<{repr}> for {} {{\n", schema.id));
        out.push_str(&format!("    fn from(v: {repr}) -> Self {{\n"));
        out.push_str("        match v {\n");
        for (_, v, variant) in &single_entries {
            out.push_str(&format!("            {v:#04x} => Self::{variant},\n"));
        }
        out.push_str(&format!("            other => Self::{catch_all}(other),\n"));
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n");

        // Inherent accessors: repr() recovers the raw byte; name() renders the
        // canonical string — catch-all → "Unknown{n}", otherwise the variant name
        // (byte-identical to the `format!("{:?}", ..)` form consumers used before).
        out.push('\n');
        out.push_str(&format!("impl {} {{\n", schema.id));
        out.push_str(&format!("    pub fn repr(self) -> {repr} {{\n"));
        out.push_str("        match self {\n");
        for (_, v, variant) in &single_entries {
            out.push_str(&format!("            Self::{variant} => {v:#04x},\n"));
        }
        out.push_str(&format!("            Self::{catch_all}(v) => v,\n"));
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("    pub fn name(self) -> String {\n");
        out.push_str("        match self {\n");
        out.push_str(&format!(
            "            Self::{catch_all}(v) => format!(\"Unknown{{v}}\"),\n"
        ));
        out.push_str("            other => format!(\"{other:?}\"),\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n");

        out.push_str(&projection_code(
            schema,
            repr,
            &format!("Self::{catch_all}(v)"),
        ));

        out
    }

    fn emit_page(&self, schema: &PageSchema) -> String {
        super::pages::emit_page(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emitters::Emitter;
    use crate::ir::enums::{EntryValue, EnumEntry, EnumSchema};

    fn make_entry(name: &str, value: u64) -> EnumEntry {
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

    #[test]
    fn test_emit_enum_disambiguates_duplicate_variants() {
        let schema = EnumSchema {
            spec: None,
            provenance: None,
            description: None,
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            id: "Sff8024BaseTCodes".to_string(),
            title: "Test".to_string(),
            entry_includes: vec![],
            entries: vec![
                make_entry("base_2_5g_t", 0x02),
                make_entry("base_25g_t", 0x05),
            ],
        };
        let out = RustEmitter.emit_enum(&schema);
        // 0x02 gets the base name; 0x05 gets V05 suffix. Variants are now bare
        // (no discriminant) since the enum carries a payload catch-all.
        assert!(
            out.contains("    Base25gT,\n"),
            "first variant keeps base name\n---\n{out}"
        );
        assert!(
            out.contains("    Base25gTV05,\n"),
            "duplicate gets V-suffix\n---\n{out}"
        );
        // From arms must match (total, infallible — no Ok/Err wrapping).
        assert!(
            out.contains("0x02 => Self::Base25gT,"),
            "From arm 0x02\n---\n{out}"
        );
        assert!(
            out.contains("0x05 => Self::Base25gTV05,"),
            "From arm 0x05\n---\n{out}"
        );
    }

    fn make_range_entry(name: &str, start: u64, end: u64) -> EnumEntry {
        EnumEntry {
            name: name.to_string(),
            value: EntryValue::Range { start, end },
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

    /// A reserved range plus singles must produce a TOTAL decoder: a payload
    /// catch-all variant, an infallible `From` with no `Err`, and no `#[repr]`
    /// (a repr-int enum cannot carry a data variant). This is the generic
    /// guarantee behind the reserved-nibble DoS fix across all 34 enum_ref
    /// fields — the Page 11h fixture proves it end-to-end across the bindings.
    #[test]
    fn test_emit_enum_reserved_range_becomes_total_catch_all() {
        let schema = EnumSchema {
            spec: None,
            provenance: None,
            description: None,
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            id: "CmisDataPathState".to_string(),
            title: "Test".to_string(),
            entry_includes: vec![],
            entries: vec![
                make_entry("dp_deactivated", 0x1),
                make_entry("dp_initialized", 0x7),
                make_range_entry("reserved_8_f", 0x8, 0xf),
            ],
        };
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("impl From<u8> for CmisDataPathState"),
            "total From impl\n---\n{out}"
        );
        assert!(!out.contains("TryFrom"), "no fallible TryFrom\n---\n{out}");
        assert!(!out.contains("Err("), "no Err arm\n---\n{out}");
        assert!(
            !out.contains("#[repr("),
            "no #[repr] (payload variant forbids it)\n---\n{out}"
        );
        assert!(
            out.contains("    Unknown(u8),\n"),
            "payload catch-all variant\n---\n{out}"
        );
        assert!(
            out.contains("other => Self::Unknown(other),"),
            "catch-all From arm\n---\n{out}"
        );
        assert!(
            !out.contains("0x08..0x0f") && !out.contains("(range — not a variant)"),
            "reserved range folded into catch-all, no stray comment\n---\n{out}"
        );
        assert!(
            out.contains("Self::Unknown(v) => format!(\"Unknown{v}\")"),
            "name() pins unmapped rendering to \"Unknown{{n}}\"\n---\n{out}"
        );
    }

    /// Range-only enums (no singles) still get a total `From` + repr()/name() so
    /// page decoders can call `Type::from(..)` uniformly (e.g. CmisCdbCommandId).
    #[test]
    fn test_emit_enum_range_only_newtype_is_total() {
        let schema = EnumSchema {
            spec: None,
            provenance: None,
            description: None,
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            id: "CmisCdbCommandId".to_string(),
            title: "Test".to_string(),
            entry_includes: vec![],
            entries: vec![make_range_entry("vendor", 0x0, 0xffff)],
        };
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("pub struct CmisCdbCommandId(pub u16);"),
            "u16 newtype (range end guides repr)\n---\n{out}"
        );
        assert!(
            out.contains("impl From<u16> for CmisCdbCommandId"),
            "total From\n---\n{out}"
        );
        assert!(!out.contains("TryFrom"), "no fallible TryFrom\n---\n{out}");
        assert!(
            out.contains("pub fn repr(self) -> u16") && out.contains("pub fn name(self) -> String"),
            "repr()/name() accessors present\n---\n{out}"
        );
    }

    // ── Revision-bound name projections (Task 2) ──────────────────────────────

    /// Extract one generated projection function body by name, so since/until
    /// filtering can be asserted per revision rather than over the whole file.
    fn projection_body<'a>(out: &'a str, fn_name: &str) -> &'a str {
        let start = out
            .find(&format!("fn {fn_name}("))
            .unwrap_or_else(|| panic!("missing fn {fn_name}\n---\n{out}"));
        out[start..]
            .split("}\n\n")
            .next()
            .unwrap_or_else(|| panic!("unterminated fn {fn_name}\n---\n{out}"))
    }

    #[test]
    fn revision_projection_is_additive_and_domain_bound() {
        let schema = crate::emitters::test_support::validated_dpsm_schema();
        let out = RustEmitter.emit_enum(&schema);

        // Additive: the flat surface is fully intact.
        assert!(
            out.contains("impl From<u8> for CmisDataPathState"),
            "flat From survives\n---\n{out}"
        );
        assert!(
            out.contains("0x05 => Self::DpTxTurnOn,"),
            "flat From arm for 0x05 survives\n---\n{out}"
        );
        assert!(
            out.contains("pub fn repr(self) -> u8"),
            "repr() survives\n---\n{out}"
        );
        assert!(
            out.contains("pub fn name(self) -> String"),
            "name() survives\n---\n{out}"
        );

        // Exact public interface.
        assert!(
            out.contains("pub type CmisDataPathStateProjection = fn(u8) -> Option<&'static str>;"),
            "projection type alias\n---\n{out}"
        );
        assert!(
            out.contains(
                "pub fn for_revision(cmis_rev_byte: u8) -> Option<CmisDataPathStateProjection>"
            ),
            "factory signature\n---\n{out}"
        );

        // Domain-bound dispatch: exactly the six packed-BCD catalog bytes.
        for (byte, rev) in [
            (0x30u8, "v3_0"),
            (0x40, "v4_0"),
            (0x50, "v5_0"),
            (0x51, "v5_1"),
            (0x52, "v5_2"),
            (0x53, "v5_3"),
            (0x54, "v5_4"),
        ] {
            assert!(
                out.contains(&format!(
                    "{byte:#04x} => Some(cmis_data_path_state_projection_{rev}),"
                )),
                "catalog byte {byte:#04x} binds its projection\n---\n{out}"
            );
        }
        assert!(out.contains("_ => None,"), "refusal arm\n---\n{out}");
        assert!(
            !out.contains("0x55 =>"),
            "0x55 (5.5) is not a known context\n---\n{out}"
        );
        assert!(
            !out.contains("0x31 =>"),
            "0x31 (3.1) is not a known context\n---\n{out}"
        );

        // since/until filtering: 3.0 lacks the 4.0 names; every 4.0+ revision
        // has them. Unannotated singletons 0–4 appear in every projection.
        let v3_0 = projection_body(&out, "cmis_data_path_state_projection_v3_0");
        assert!(
            v3_0.contains("0x04 => Some(\"DpActivated\")"),
            "unannotated entry present at 3.0\n---\n{v3_0}"
        );
        assert!(
            !v3_0.contains("DpTxTurnOn") && !v3_0.contains("DpTxTurnOff"),
            "since 4.0 entries absent at 3.0\n---\n{v3_0}"
        );
        for rev in ["v4_0", "v5_0", "v5_1", "v5_2", "v5_3"] {
            let body = projection_body(&out, &format!("cmis_data_path_state_projection_{rev}"));
            for (value, name) in [
                (0x00u8, "Reserved0"),
                (0x01, "DpDeactivated"),
                (0x02, "DpInit"),
                (0x03, "DpDeinit"),
                (0x04, "DpActivated"),
                (0x05, "DpTxTurnOn"),
                (0x06, "DpTxTurnOff"),
                (0x07, "DpInitialized"),
            ] {
                assert!(
                    body.contains(&format!("{value:#04x} => Some(\"{name}\")")),
                    "{rev} arm {value:#04x}\n---\n{body}"
                );
            }
        }

        // Canonical casing straight from the naming policy, identical in every
        // language — and no range arm: reserved 8..0xf stays unnamed.
        assert!(
            !out.contains("0x08 =>") && !out.contains("0x0f =>"),
            "range raws have no name\n---\n{out}"
        );
    }

    #[test]
    fn unannotated_enum_emission_is_unchanged() {
        let schema = crate::emitters::test_support::validated_unannotated_schema();
        let out = RustEmitter.emit_enum(&schema);
        // Same enum id as the projection fixture: with no entry metadata the
        // output must carry no projection surface at all.
        assert!(
            !out.contains("Projection"),
            "no projection items without entry metadata\n---\n{out}"
        );
        assert!(
            !out.contains("for_revision"),
            "no factory without entry metadata\n---\n{out}"
        );
        // The flat surface is still complete.
        assert!(out.contains("impl From<u8> for CmisDataPathState"));
        assert!(out.contains("pub fn name(self) -> String"));
    }

    /// The range-only early-return path must not silently drop an appended
    /// projection: a range-only schema whose entries carry metadata still
    /// emits the factory, with all-refusing callables (ranges never name).
    #[test]
    fn range_only_annotated_enum_still_emits_projection() {
        let schema = crate::emitters::test_support::validated_range_only_annotated_schema();
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("pub struct CmisDataPathState(pub u8);"),
            "range-only newtype path\n---\n{out}"
        );
        assert!(
            out.contains("pub type CmisDataPathStateProjection = fn(u8) -> Option<&'static str>;"),
            "range-only path keeps the projection type\n---\n{out}"
        );
        assert!(
            out.contains("pub fn for_revision("),
            "range-only path keeps the factory\n---\n{out}"
        );
        for rev in ["v3_0", "v4_0", "v5_0", "v5_1", "v5_2", "v5_3"] {
            let body = projection_body(&out, &format!("cmis_data_path_state_projection_{rev}"));
            assert!(
                !body.contains("Some("),
                "range-only matches never name ({rev})\n---\n{body}"
            );
            assert!(
                body.contains("_ => None,"),
                "({rev}) still refuses every raw\n---\n{body}"
            );
        }
    }

    /// `from_rev` on an annotated enum: the gated form — `Self::from(v)`
    /// only when this revision's projection names the raw, else the
    /// catch-all. A raw the revision reserves (the since-4.0 raws at 3.0)
    /// must not borrow a later revision's named variant.
    #[test]
    fn from_rev_is_revision_gated_on_annotated_enums() {
        let schema = crate::emitters::test_support::validated_dpsm_schema();
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("pub fn from_rev(cmis_rev_byte: u8, v: u8) -> Self {"),
            "gated from_rev signature\n---\n{out}"
        );
        assert!(
            out.contains(
                "if Self::for_revision(cmis_rev_byte).and_then(|proj| proj(v)).is_some() {"
            ),
            "from_rev consults the projection\n---\n{out}"
        );
        assert!(
            out.contains("            Self::from(v)\n        } else {\n            Self::Unknown(v)\n        }"),
            "a projection-named raw keeps the flat From decode; every other raw folds into the catch-all\n---\n{out}"
        );
    }

    /// `from_rev` on an unannotated enum: a total pass-through with the
    /// unused revision parameter keeping the gated call shape, so per-revision
    /// page decoders can call it on every enum uniformly.
    #[test]
    fn from_rev_passes_through_on_unannotated_enums() {
        let schema = crate::emitters::test_support::validated_unannotated_schema();
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("pub fn from_rev(_cmis_rev_byte: u8, v: u8) -> Self {"),
            "pass-through from_rev signature (parameter unused)\n---\n{out}"
        );
        assert!(
            out.contains("        Self::from(v)\n    }\n}"),
            "pass-through body is exactly the flat From decode\n---\n{out}"
        );
    }

    /// `from_rev` on a range-only annotated enum: same gated shape, but the
    /// fold target is the newtype value itself — there is no catch-all
    /// variant to route to.
    #[test]
    fn from_rev_on_range_only_annotated_enum_folds_to_self() {
        let schema = crate::emitters::test_support::validated_range_only_annotated_schema();
        let out = RustEmitter.emit_enum(&schema);
        assert!(
            out.contains("pub fn from_rev(cmis_rev_byte: u8, v: u8) -> Self {"),
            "range-only gated from_rev signature\n---\n{out}"
        );
        assert!(
            out.contains(
                "            Self::from(v)\n        } else {\n            Self(v)\n        }"
            ),
            "range-only fold target is Self(v)\n---\n{out}"
        );
    }
}
