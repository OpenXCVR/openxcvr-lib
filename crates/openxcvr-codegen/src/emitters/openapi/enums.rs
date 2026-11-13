use serde_json::{json, Value};

use crate::ir::enums::{EntryValue, EnumSchema};

use super::camel_to_snake;

pub fn emit_enum(schema: &EnumSchema) -> String {
    let id = format!("{}.schema.json", camel_to_snake(&schema.id));

    // oneOf items carry the recorded per-entry metadata as annotations:
    // description plus the entry's recorded since/until window. `const` and
    // `title` keep their existing validation/identifier roles unchanged.
    let one_of: Vec<Value> = schema
        .entries
        .iter()
        .filter_map(|entry| {
            if let EntryValue::Single(v) = &entry.value {
                let mut item = serde_json::Map::new();
                item.insert("const".to_string(), json!(v));
                if let Some(desc) = super::recorded_description(&entry.description) {
                    item.insert("description".to_string(), json!(desc));
                }
                item.insert("title".to_string(), json!(entry.name));
                if let Some(since) = &entry.since {
                    item.insert("x-openxcvr-since".to_string(), json!(since));
                }
                if let Some(until) = &entry.until {
                    item.insert("x-openxcvr-until".to_string(), json!(until));
                }
                Some(Value::Object(item))
            } else {
                None
            }
        })
        .collect();

    let mut obj = serde_json::Map::new();
    obj.insert(
        "$schema".to_string(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    obj.insert("$id".to_string(), json!(id));
    obj.insert("title".to_string(), json!(schema.id));
    // The enum-root `description:` the schema records, as the JSON Schema
    // root description annotation — omitted when none is recorded, never a
    // substitute title/display-name restatement.
    if let Some(desc) = super::recorded_description(&schema.description) {
        obj.insert("description".to_string(), json!(desc));
    }
    obj.insert("type".to_string(), json!("integer"));
    if !one_of.is_empty() {
        obj.insert("oneOf".to_string(), json!(one_of));
    }
    // Narrow x-openxcvr annotations of RECORDED metadata only: spec identity,
    // recorded provenance, and the shared revision-applicability statement
    // (unmodeled stays explicit — nothing implies every revision applies).
    // No assertive validation keywords are added or changed.
    if let Some(spec) = &schema.spec {
        obj.insert("x-openxcvr-spec".to_string(), super::spec_extension(spec));
    }
    if let Some(provenance) = &schema.provenance {
        obj.insert(
            "x-openxcvr-provenance".to_string(),
            super::yaml_value_to_json(provenance),
        );
    }
    let (statement, _) = crate::emitters::enum_revision_applicability(schema);
    obj.insert(
        "x-openxcvr-revision-applicability".to_string(),
        json!(statement),
    );

    serde_json::to_string_pretty(&Value::Object(obj)).unwrap() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::enums::{EntryValue, EnumEntry, EnumSchema};

    fn make_schema() -> EnumSchema {
        EnumSchema {
            spec: None,
            provenance: None,
            description: None,
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            id: "CmisModuleState".to_string(),
            title: "CMIS Module State".to_string(),
            entry_includes: vec![],
            entries: vec![
                EnumEntry {
                    name: "module_low_pwr".to_string(),
                    value: EntryValue::Single(1),
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
                },
                EnumEntry {
                    name: "module_ready".to_string(),
                    value: EntryValue::Single(3),
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
                },
                EnumEntry {
                    name: "reserved_6_7".to_string(),
                    value: EntryValue::Range { start: 6, end: 7 },
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
                },
            ],
        }
    }

    #[test]
    fn test_emit_enum_schema_field() {
        let out = emit_enum(&make_schema());
        assert!(
            out.contains(r#""$schema": "https://json-schema.org/draft/2020-12/schema""#),
            "missing $schema\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_enum_id() {
        let out = emit_enum(&make_schema());
        assert!(
            out.contains(r#""$id": "cmis_module_state.schema.json""#),
            "missing $id\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_enum_type_integer() {
        let out = emit_enum(&make_schema());
        assert!(
            out.contains(r#""type": "integer""#),
            "missing type:integer\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_enum_one_of_entries() {
        let out = emit_enum(&make_schema());
        assert!(
            out.contains(r#""const": 1"#) && out.contains(r#""title": "module_low_pwr""#),
            "missing oneOf entry for module_low_pwr\n---\n{out}"
        );
        assert!(
            out.contains(r#""const": 3"#) && out.contains(r#""title": "module_ready""#),
            "missing oneOf entry for module_ready\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_enum_range_skipped() {
        let out = emit_enum(&make_schema());
        assert!(
            !out.contains("reserved_6_7"),
            "range entry should be skipped\n---\n{out}"
        );
    }

    // ── schema-recorded metadata annotations ─────────────────────────────────

    use serde_json::Value;

    fn parse(out: &str) -> Value {
        serde_json::from_str(out).unwrap()
    }

    /// Remove ONLY the exact annotation paths this emitter introduces, at
    /// the exact levels it introduces them — never a recursive strip of keys
    /// named `description` or `x-*` (a schema could record such names).
    fn remove_enum_annotation_paths(mut v: Value) -> Value {
        if let Value::Object(ref mut root) = v {
            for key in [
                "description",
                "x-openxcvr-spec",
                "x-openxcvr-provenance",
                "x-openxcvr-revision-applicability",
            ] {
                root.remove(key);
            }
            if let Some(Value::Array(items)) = root.get_mut("oneOf") {
                for item in items {
                    if let Value::Object(ref mut item) = item {
                        for key in ["description", "x-openxcvr-since", "x-openxcvr-until"] {
                            item.remove(key);
                        }
                    }
                }
            }
        }
        v
    }

    fn annotated_schema() -> EnumSchema {
        let mut schema = make_schema();
        schema.description = Some("Recorded enum-level description.".into());
        schema.spec = Some(crate::ir::pages::SpecRef {
            id: "cmis".to_string(),
            revision: Some("5.3".to_string()),
            errata: vec![],
            source: None,
            parity_deferred: None,
        });
        schema.provenance = Some(
            serde_norway::from_str("{ spec: 'CMIS 5.3 § 6.3.2 / Table 8-7', pdf_pages: [80, 81] }")
                .unwrap(),
        );
        schema.entries[0].description = Some("ModuleLowPwr steady state.".into());
        schema.entries[1].description = Some("ModuleReady operational state.".into());
        schema.entries[1].since = Some("4.0".to_string());
        schema.entries[1].until = Some("5.2".to_string());
        schema
    }

    #[test]
    fn enum_schema_annotations_carry_recorded_metadata() {
        let out = parse(&emit_enum(&annotated_schema()));
        let root = out.as_object().unwrap();

        // Title stays the user-visible identifier contract: schema.id.
        assert_eq!(
            root.get("title"),
            Some(&serde_json::json!("CmisModuleState"))
        );

        assert_eq!(
            root.get("description"),
            Some(&serde_json::json!("Recorded enum-level description.")),
            "the recorded root description is carried as the root description \
             annotation, nothing inferred"
        );
        assert_eq!(
            root.get("x-openxcvr-spec"),
            Some(&serde_json::json!({"id": "cmis", "revision": "5.3"})),
            "recorded spec identity, nothing inferred"
        );
        assert_eq!(
            root.get("x-openxcvr-provenance"),
            Some(&serde_json::json!({
                "spec": "CMIS 5.3 § 6.3.2 / Table 8-7",
                "pdf_pages": [80, 81]
            })),
            "recorded provenance converts verbatim (sequences stay sequences)"
        );
        assert!(
            root.get("x-openxcvr-revision-applicability")
                .and_then(Value::as_str)
                .unwrap()
                .starts_with(
                    "Revision applicability: revision-qualified — entry revision windows are "
                ),
            "shared recorded applicability statement"
        );

        let one_of = root.get("oneOf").unwrap().as_array().unwrap();
        let module_low_pwr = &one_of[0];
        assert_eq!(
            module_low_pwr.get("description"),
            Some(&serde_json::json!("ModuleLowPwr steady state."))
        );
        assert!(module_low_pwr.get("x-openxcvr-since").is_none());
        let module_ready = &one_of[1];
        assert_eq!(
            module_ready.get("description"),
            Some(&serde_json::json!("ModuleReady operational state."))
        );
        assert_eq!(
            module_ready.get("x-openxcvr-since"),
            Some(&serde_json::json!("4.0"))
        );
        assert_eq!(
            module_ready.get("x-openxcvr-until"),
            Some(&serde_json::json!("5.2"))
        );
    }

    #[test]
    fn unannotated_enum_states_revision_not_modeled() {
        let out = parse(&emit_enum(&make_schema()));
        let root = out.as_object().unwrap();
        assert!(
            root.get("x-openxcvr-revision-applicability")
                .and_then(Value::as_str)
                .unwrap()
                .starts_with(
                    "Revision applicability: not modeled — this enum is a canonical catalog"
                ),
            "unmodeled must be explicit, never implied all-supported: {root:?}"
        );
        // No recorded spec/provenance → the extensions are simply absent.
        assert!(root.get("x-openxcvr-spec").is_none());
        assert!(root.get("x-openxcvr-provenance").is_none());
    }

    #[test]
    fn enum_validation_surface_keys_unchanged() {
        let out = parse(&emit_enum(&annotated_schema()));
        let root = out.as_object().unwrap();
        let mut keys: Vec<&str> = root.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "$id",
                "$schema",
                "description",
                "oneOf",
                "title",
                "type",
                "x-openxcvr-provenance",
                "x-openxcvr-revision-applicability",
                "x-openxcvr-spec",
            ],
            "no extra assertive validation keywords appear at the root"
        );
        assert_eq!(root.get("type"), Some(&serde_json::json!("integer")));
        for item in root.get("oneOf").unwrap().as_array().unwrap() {
            let mut item_keys: Vec<&str> = item
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            item_keys.sort_unstable();
            // const/title preserved; only description (and the recorded
            // since/until window) may join an item.
            assert!(
                item_keys == vec!["const", "description", "title"]
                    || item_keys
                        == vec![
                            "const",
                            "description",
                            "title",
                            "x-openxcvr-since",
                            "x-openxcvr-until"
                        ],
                "unexpected oneOf item keys: {item_keys:?}"
            );
            assert!(item.get("const").is_some() && item.get("title").is_some());
        }
        // Range entries still never yield oneOf items.
        assert_eq!(root.get("oneOf").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn enum_metadata_json_escapes_recorded_text() {
        let mut schema = make_schema();
        schema.entries[0].description =
            Some("quote \" backslash \\ newline \n unicode ✓ <b>`~</b>".into());
        let raw = emit_enum(&schema);
        let out = parse(&raw);
        let one_of = out["oneOf"].as_array().unwrap();
        assert_eq!(
            one_of[0]["description"].as_str().unwrap(),
            "quote \" backslash \\ newline \n unicode ✓ <b>`~</b>",
            "JSON string round-trips the recorded text exactly"
        );
        // Control characters must be JSON-escaped in the raw bytes.
        assert!(
            raw.contains("\\n"),
            "raw output must escape the newline: {raw}"
        );
    }

    #[test]
    fn enum_annotations_removed_by_exact_paths_equal_metadata_free_emission() {
        let rich = annotated_schema();
        let mut bare = rich.clone();
        bare.spec = None;
        bare.provenance = None;
        for e in &mut bare.entries {
            e.description = None;
            e.since = None;
            e.until = None;
        }
        let rich_out = parse(&emit_enum(&rich));
        let bare_out = parse(&emit_enum(&bare));
        // Strip the exact annotation paths from BOTH sides: the revision-
        // applicability statement is always emitted (unmodeled stays
        // explicit), so it appears on the metadata-free side too.
        assert_eq!(
            remove_enum_annotation_paths(rich_out),
            remove_enum_annotation_paths(bare_out),
            "removing exactly the introduced annotation paths must equalize \
             rich and metadata-free emissions"
        );
    }

    #[test]
    fn enum_root_description_absent_when_unrecorded() {
        // Unrecorded stays absent — the extension is omitted, never a
        // placeholder. A recorded but whitespace-only description counts as
        // unrecorded (same semantics as the Rust field docs).
        let out = parse(&emit_enum(&make_schema()));
        assert!(
            out.as_object().unwrap().get("description").is_none(),
            "no root description key may appear: {out}"
        );
        let mut blank = make_schema();
        blank.description = Some("   \n ".into());
        let out = parse(&emit_enum(&blank));
        assert!(
            out.as_object().unwrap().get("description").is_none(),
            "whitespace-only description counts as unrecorded: {out}"
        );
    }

    #[test]
    fn enum_root_description_json_escapes_recorded_text() {
        let mut schema = make_schema();
        schema.description =
            Some("quote \" backslash \\ newline \n CR\r unicode ✓ <b>`~</b>".into());
        let raw = emit_enum(&schema);
        let out = parse(&raw);
        assert_eq!(
            out["description"].as_str().unwrap(),
            "quote \" backslash \\ newline \n CR\r unicode ✓ <b>`~</b>",
            "JSON round-trips the recorded root description exactly"
        );
        assert!(
            raw.contains("\\n") && raw.contains("\\r"),
            "raw output must escape control characters: {raw}"
        );
        // One JSON string, not a broken structure.
        assert_eq!(
            out["description"].as_str().unwrap().matches('\n').count(),
            1
        );
    }

    // ── real-schema coverage: one representative enum per family + shared 8024 ─

    fn load_enum(relative: &str) -> EnumSchema {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        crate::parser::enums::load_enum_schema(&root.join(relative)).unwrap()
    }

    #[test]
    fn real_family_enum_annotations() {
        let cases = [
            (
                "cmis/enums/module-state.yaml",
                serde_json::json!({"id": "cmis", "revision": "5.3"}),
                "CMIS 5.3 § 6.3.2 / Table 8-7",
                "module_low_pwr",
                "ModuleLowPwr - steady state where the management interface is initialized",
            ),
            (
                "sff-8024/identifier-values.yaml",
                serde_json::json!({"id": "sff-8024", "revision": "4.14"}),
                "SFF-8024 4.14 Table 4-1",
                "gbic",
                "GBIC",
            ),
            (
                "sff-8636/enums/rate-select.yaml",
                serde_json::json!({"id": "sff-8636", "revision": "2.11"}),
                "SFF-8636 v2.11 § 6.2.7.2",
                "low_rate",
                "Version 1 less than 2.2 GBd",
            ),
            (
                "sff-8472/enums/rate-identifier.yaml",
                serde_json::json!({"id": "sff-8472", "revision": "12.5a"}),
                "SFF-8472 v12.5a § 5.7",
                "unspecified",
                "Unspecified rate select behavior",
            ),
        ];
        for (rel, spec, prov_name, entry_name, entry_desc) in cases {
            let schema = load_enum(rel);
            let out = parse(&emit_enum(&schema));
            assert_eq!(
                out["x-openxcvr-spec"], spec,
                "recorded spec identity for {rel}: {out}"
            );
            assert_eq!(
                out["x-openxcvr-provenance"]["spec"].as_str().unwrap(),
                prov_name,
                "recorded provenance for {rel}: {out}"
            );
            let item = out["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["title"] == entry_name)
                .unwrap_or_else(|| panic!("missing oneOf item {entry_name} for {rel}: {out}"));
            assert!(
                item["description"]
                    .as_str()
                    .unwrap()
                    .starts_with(entry_desc),
                "recorded entry description for {rel}: {item}"
            );
            assert_eq!(
                out["title"].as_str().unwrap(),
                schema.id,
                "title keeps the schema.id identifier contract for {rel}"
            );
            assert!(
                out.as_object().unwrap().get("description").is_none(),
                "unrecorded root description stays absent for {rel}: {out}"
            );
        }
    }

    #[test]
    fn real_cdb_status_root_description_carried() {
        // End-to-end parse → generate for the one real enum whose schema
        // records a root `description:`.
        let schema = load_enum("cmis/enums/cdb-status.yaml");
        let out = parse(&emit_enum(&schema));
        assert_eq!(
            out["description"].as_str().unwrap(),
            "CdbStatus is an 8-bit status byte with bit 7 indicating busy, bit 6 indicating failed after completion, and bits 5-0 carrying the command result. When bit 7 is set, bit 6 is ignored by the coarse status rules.",
            "the recorded root description is carried verbatim (trimmed): {out}"
        );
    }
}
