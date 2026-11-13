use serde_json::{json, Value};

use crate::ir::pages::{FieldType, PageField, PageSchema};

use super::camel_to_snake;

fn field_base_schema(field: &PageField) -> Value {
    match &field.ty {
        FieldType::Bool => json!({"type": "boolean"}),
        FieldType::U2 => json!({"type": "integer", "minimum": 0, "maximum": 3}),
        FieldType::U3 => json!({"type": "integer", "minimum": 0, "maximum": 7}),
        FieldType::U4 => json!({"type": "integer", "minimum": 0, "maximum": 15}),
        FieldType::U5 => json!({"type": "integer", "minimum": 0, "maximum": 31}),
        FieldType::U6 => json!({"type": "integer", "minimum": 0, "maximum": 63}),
        FieldType::U7 => json!({"type": "integer", "minimum": 0, "maximum": 127}),
        FieldType::U8 => json!({"type": "integer", "minimum": 0, "maximum": 255}),
        FieldType::I8 => json!({"type": "integer", "minimum": -128, "maximum": 127}),
        FieldType::U16Be | FieldType::U16Le => {
            json!({"type": "integer", "minimum": 0, "maximum": 65535})
        }
        FieldType::I16Be => json!({"type": "integer", "minimum": -32768, "maximum": 32767}),
        FieldType::U32Be => json!({"type": "integer", "minimum": 0, "maximum": 4294967295u64}),
        FieldType::I32Be => {
            json!({"type": "integer", "minimum": -2147483648i64, "maximum": 2147483647})
        }
        FieldType::U64Le | FieldType::U64Be => json!({"type": "integer", "minimum": 0}),
        FieldType::F16Be => {
            json!({"type": "number", "x-openxcvr-note": "CMIS f16be encoding: m × 10^(s-24)"})
        }
        FieldType::X16Be => {
            json!({"x-openxcvr-note": "polymorphic float/int union — see schema YAML for full constraints"})
        }
        FieldType::Ascii => json!({"type": "string"}),
        FieldType::Bitmap | FieldType::Bytes => {
            json!({"type": "string", "contentEncoding": "base64"})
        }
        FieldType::Composite => {
            json!({"type": "object", "x-openxcvr-note": "complex type — see schema YAML for full constraints"})
        }
        FieldType::EnumRef => {
            if let Some(ref_id) = &field.enum_ref {
                let snake = camel_to_snake(ref_id);
                json!({"$ref": format!("../enums/{snake}.schema.json")})
            } else {
                json!({"type": "integer"})
            }
        }
    }
}

fn field_schema(field: &PageField) -> Value {
    let base = field_base_schema(field);
    if let Some(inst) = &field.instances {
        let count = (inst.end - inst.start + 1) as u64;
        json!({"type": "array", "items": base, "minItems": count, "maxItems": count})
    } else {
        base
    }
}

/// Recorded unit, mirroring the Rust field-doc semantics exactly: an
/// explicit field `unit` (or a recorded *string* `encoding.unit`).
/// `encoding.lsb_units` is never a unit — it stays scaling metadata.
fn recorded_unit(field: &PageField) -> Option<String> {
    field.unit.clone().or_else(|| {
        field
            .encoding
            .as_ref()
            .and_then(|e| e.get("unit"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    })
}

/// Attach the RECORDED field metadata as JSON Schema annotations, at the
/// property-schema level only (never inside `items`): description, recorded
/// unit, recorded lsb_units scaling metadata, recorded since/until bounds,
/// and recorded provenance. No assertive validation keyword is touched.
fn annotate_property(prop: &mut serde_json::Map<String, Value>, field: &PageField) {
    if let Some(desc) = super::recorded_description(&field.description) {
        prop.insert("description".to_string(), json!(desc));
    }
    if let Some(unit) = recorded_unit(field) {
        prop.insert("x-openxcvr-unit".to_string(), json!(unit));
    }
    if let Some(lsb) = field
        .encoding
        .as_ref()
        .and_then(|e| e.get("lsb_units"))
        .map(crate::emitters::markdown::scalar)
    {
        prop.insert("x-openxcvr-lsb-units".to_string(), json!(lsb));
    }
    if let Some(since) = &field.since {
        prop.insert("x-openxcvr-since".to_string(), json!(since));
    }
    if let Some(until) = &field.until {
        prop.insert("x-openxcvr-until".to_string(), json!(until));
    }
    if let Some(provenance) = &field.provenance {
        prop.insert(
            "x-openxcvr-provenance".to_string(),
            super::yaml_value_to_json(provenance),
        );
    }
}

pub fn emit_page(schema: &PageSchema) -> String {
    let id = format!("{}.schema.json", camel_to_snake(&schema.id));

    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();

    for field in schema.value_fields() {
        let mut prop = match field_schema(field) {
            Value::Object(map) => map,
            other => unreachable!("field_schema always emits an object schema: {other}"),
        };
        // Property name first, then the property schema with its recorded
        // metadata annotations — a field named `description` or `x-openxcvr-*`
        // is a property like any other; its annotations nest one level deeper.
        annotate_property(&mut prop, field);
        properties.insert(field.name.clone(), Value::Object(prop));
        if field.present_when.is_none() {
            required.push(json!(field.name));
        }
    }

    let mut obj = serde_json::Map::new();
    obj.insert(
        "$schema".to_string(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    obj.insert("$id".to_string(), json!(id));
    obj.insert("title".to_string(), json!(schema.id));
    obj.insert("type".to_string(), json!("object"));
    obj.insert("properties".to_string(), Value::Object(properties));
    if !required.is_empty() {
        obj.insert("required".to_string(), json!(required));
    }
    // Narrow x-openxcvr annotations of RECORDED page metadata: spec identity
    // and recorded provenance. No page-level description exists in the page
    // schema language, so none is invented.
    if let Some(spec) = &schema.spec {
        obj.insert("x-openxcvr-spec".to_string(), super::spec_extension(spec));
    }
    if let Some(provenance) = &schema.provenance {
        obj.insert(
            "x-openxcvr-provenance".to_string(),
            super::yaml_value_to_json(provenance),
        );
    }

    serde_json::to_string_pretty(&Value::Object(obj)).unwrap() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::pages::{FieldType, PageField, PageSchema};

    fn make_field(name: &str, ty: FieldType) -> PageField {
        PageField {
            name: name.to_string(),
            display_name: None,
            offset: Some(0),
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: None,
            ty,
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

    fn make_schema() -> PageSchema {
        PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "CmisLowerMemory".to_string(),
            title: "CMIS Lower Memory".to_string(),
            page_address: "lower".to_string(),
            size_bytes: Some(128),
            field_includes: vec![],
            fields: vec![
                make_field("identifier", FieldType::U8),
                make_field("active", FieldType::Bool),
                make_field("vendor_name", FieldType::Ascii),
            ],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        }
    }

    #[test]
    fn test_emit_page_schema_id() {
        let out = emit_page(&make_schema());
        assert!(
            out.contains(r#""$id": "cmis_lower_memory.schema.json""#),
            "missing $id\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_page_type_object() {
        let out = emit_page(&make_schema());
        assert!(
            out.contains(r#""type": "object""#),
            "missing type:object\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_page_properties() {
        let out = emit_page(&make_schema());
        assert!(
            out.contains(r#""identifier""#) && out.contains(r#""maximum": 255"#),
            "missing u8 property\n---\n{out}"
        );
        assert!(
            out.contains(r#""active""#) && out.contains(r#""type": "boolean""#),
            "missing bool property\n---\n{out}"
        );
        assert!(
            out.contains(r#""vendor_name""#) && out.contains(r#""type": "string""#),
            "missing ascii property\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_page_required() {
        let out = emit_page(&make_schema());
        assert!(
            out.contains(r#""required""#),
            "missing required array\n---\n{out}"
        );
        assert!(
            out.contains(r#""identifier""#),
            "identifier should be required\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_page_enum_ref() {
        let mut schema = make_schema();
        let mut f = make_field("module_state", FieldType::EnumRef);
        f.enum_ref = Some("CmisModuleState".to_string());
        schema.fields.push(f);
        let out = emit_page(&schema);
        assert!(
            out.contains(r#""$ref": "../enums/cmis_module_state.schema.json""#),
            "missing $ref\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_page_instances_as_array() {
        use crate::ir::pages::InstancesDesc;
        let mut schema = make_schema();
        let mut f = make_field("tx_power", FieldType::U16Be);
        f.instances = Some(InstancesDesc {
            index: "lane".to_string(),
            start: 0,
            end: 7,
        });
        schema.fields.push(f);
        let out = emit_page(&schema);
        assert!(
            out.contains(r#""minItems": 8"#) && out.contains(r#""maxItems": 8"#),
            "instances field should be array with min/maxItems=8\n---\n{out}"
        );
    }

    #[test]
    fn test_emit_page_present_when_not_required() {
        let mut schema = make_schema();
        schema.fields.clear();
        let mut f = make_field("optional_field", FieldType::U8);
        f.present_when = Some(serde_norway::Value::String("some_condition".to_string()));
        schema.fields.push(f);
        let out = emit_page(&schema);
        // required array should be absent or empty
        assert!(
            !out.contains(r#""required""#),
            "optional field should not appear in required\n---\n{out}"
        );
    }

    // ── schema-recorded metadata annotations ─────────────────────────────────

    use serde_json::Value;

    fn parse(out: &str) -> Value {
        serde_json::from_str(out).unwrap()
    }

    /// Remove ONLY the exact annotation paths this emitter introduces, at the
    /// exact levels it introduces them: root extensions and per-property keys
    /// inside `properties.<name>` — never a recursive strip of keys named
    /// `description` or `x-*` (a user property can be named exactly that).
    fn remove_page_annotation_paths(mut v: Value) -> Value {
        if let Value::Object(ref mut root) = v {
            for key in ["x-openxcvr-spec", "x-openxcvr-provenance"] {
                root.remove(key);
            }
            if let Some(Value::Object(ref mut props)) = root.get_mut("properties") {
                for (_name, prop) in props.iter_mut() {
                    if let Value::Object(ref mut prop) = prop {
                        for key in [
                            "description",
                            "x-openxcvr-unit",
                            "x-openxcvr-lsb-units",
                            "x-openxcvr-since",
                            "x-openxcvr-until",
                            "x-openxcvr-provenance",
                        ] {
                            prop.remove(key);
                        }
                    }
                }
            }
        }
        v
    }

    fn annotated_page_schema() -> PageSchema {
        let mut schema = make_schema();
        schema.spec = Some(crate::ir::pages::SpecRef {
            id: "cmis".to_string(),
            revision: Some("5.3".to_string()),
            errata: vec![],
            source: None,
            parity_deferred: None,
        });
        schema.provenance =
            Some(serde_norway::from_str("{ section: '§ 8.3.1', pdf_page: 161 }").unwrap());

        let mut described = make_field("vendor_name", FieldType::Ascii);
        described.description = Some("Vendor name.".into());
        described.provenance = Some(
            serde_norway::from_str(
                "{ section: '§ 8.3.2.1', quote: '16 character read-only field' }",
            )
            .unwrap(),
        );
        described.since = Some("4.0".to_string());
        described.until = Some("5.2".to_string());
        let mut unit_field = make_field("fine_interval_length", FieldType::U16Be);
        unit_field.unit = Some("0.1 ms".into());
        let mut lsb_field = make_field("temperature_threshold", FieldType::I16Be);
        lsb_field.encoding = Some(
            serde_norway::from_str("{ kind: scaled_signed, lsb_units: '1/256 deg C' }").unwrap(),
        );
        let mut lanes = make_field("tx_power", FieldType::U16Be);
        lanes.instances = Some(crate::ir::pages::InstancesDesc {
            index: "lane".to_string(),
            start: 0,
            end: 7,
        });
        lanes.description = Some("Per-lane Tx power.".into());
        let mut enum_field = make_field("module_state", FieldType::EnumRef);
        enum_field.enum_ref = Some("CmisModuleState".to_string());
        enum_field.description = Some("Module state.".into());

        schema.fields = vec![described, unit_field, lsb_field, lanes, enum_field];
        schema
    }

    #[test]
    fn page_property_annotations_carry_recorded_metadata() {
        let out = parse(&emit_page(&annotated_page_schema()));
        let props = out["properties"].as_object().unwrap();

        let vendor = &props["vendor_name"];
        assert_eq!(
            vendor["description"].as_str().unwrap(),
            "Vendor name.",
            "recorded description"
        );
        assert_eq!(
            vendor["x-openxcvr-provenance"],
            serde_json::json!({
                "section": "§ 8.3.2.1",
                "quote": "16 character read-only field"
            }),
            "recorded provenance converts verbatim"
        );
        assert_eq!(
            vendor["x-openxcvr-since"].as_str().unwrap(),
            "4.0",
            "recorded since bound"
        );
        assert_eq!(
            vendor["x-openxcvr-until"].as_str().unwrap(),
            "5.2",
            "recorded until bound"
        );
        // The validation surface of the property is untouched.
        assert_eq!(vendor["type"], serde_json::json!("string"));

        // Units mirror the Rust field-doc semantics: an explicit field unit
        // (or string encoding.unit) is a unit; lsb_units never is.
        let fine = &props["fine_interval_length"];
        assert_eq!(fine["x-openxcvr-unit"].as_str().unwrap(), "0.1 ms");
        assert!(fine.get("x-openxcvr-lsb-units").is_none());
        let temp = &props["temperature_threshold"];
        assert_eq!(
            temp["x-openxcvr-lsb-units"].as_str().unwrap(),
            "1/256 deg C",
            "recorded scaling metadata, never promoted to a unit"
        );
        assert!(temp.get("x-openxcvr-unit").is_none());
        assert!(
            temp.get("description").is_none(),
            "no recorded description → no description key (nothing substitutes)"
        );

        // Instanced fields annotate at the array (property) level, never
        // inside items.
        let lanes = &props["tx_power"];
        assert_eq!(lanes["description"].as_str().unwrap(), "Per-lane Tx power.");
        assert_eq!(lanes["type"], serde_json::json!("array"));
        assert!(
            lanes["items"].get("description").is_none(),
            "annotations must not leak into items: {lanes}"
        );

        // $ref siblings: annotations sit beside the $ref, both legal and
        // non-mutating in draft 2020-12.
        let module_state = &props["module_state"];
        assert_eq!(
            module_state["$ref"].as_str().unwrap(),
            "../enums/cmis_module_state.schema.json"
        );
        assert_eq!(
            module_state["description"].as_str().unwrap(),
            "Module state."
        );
    }

    #[test]
    fn page_root_annotations_and_absence_when_unrecorded() {
        let annotated = parse(&emit_page(&annotated_page_schema()));
        assert_eq!(
            annotated["x-openxcvr-spec"],
            serde_json::json!({"id": "cmis", "revision": "5.3"})
        );
        assert_eq!(
            annotated["x-openxcvr-provenance"],
            serde_json::json!({"section": "§ 8.3.1", "pdf_page": 161})
        );

        // make_schema() records no spec/provenance → the extensions are
        // simply absent; no invented root description either.
        let bare = parse(&emit_page(&make_schema()));
        let root = bare.as_object().unwrap();
        assert!(root.get("x-openxcvr-spec").is_none());
        assert!(root.get("x-openxcvr-provenance").is_none());
        assert!(
            root.get("description").is_none(),
            "no page-level description is recorded anywhere; none is invented"
        );
        // Title keeps the schema.id identifier contract.
        assert_eq!(
            root.get("title"),
            Some(&serde_json::json!("CmisLowerMemory"))
        );
    }

    #[test]
    fn page_baseline_validation_keys_unchanged_and_note_preserved() {
        let out = parse(&emit_page(&annotated_page_schema()));
        let root = out.as_object().unwrap();
        let mut keys: Vec<&str> = root.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "$id",
                "$schema",
                "properties",
                "required",
                "title",
                "type",
                "x-openxcvr-provenance",
                "x-openxcvr-spec",
            ]
        );
        assert_eq!(root.get("type"), Some(&serde_json::json!("object")));
        let required: Vec<&str> = root["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"vendor_name"));
        assert!(required.contains(&"module_state"));

        // The pre-existing x-openxcvr-note (baseline, NOT an introduced
        // annotation) survives untouched on f16be fields.
        let mut f16 = make_field("snr", FieldType::F16Be);
        f16.description = Some("Recorded SNR.".into());
        let mut schema = make_schema();
        schema.fields = vec![f16];
        let f16_out = parse(&emit_page(&schema));
        let snr = &f16_out["properties"]["snr"];
        assert_eq!(
            snr["x-openxcvr-note"].as_str().unwrap(),
            "CMIS f16be encoding: m × 10^(s-24)",
            "baseline x-openxcvr-note is preserved"
        );
        assert_eq!(snr["description"].as_str().unwrap(), "Recorded SNR.");
    }

    #[test]
    fn page_adversarial_property_names_survive_annotation_removal() {
        // A page can legitimately have fields named `description` or
        // `x-openxcvr-<anything>`: those are properties, never annotation
        // paths, and the exact-path removal must not touch them.
        let mut schema = make_schema();
        let mut description_field = make_field("description", FieldType::U8);
        description_field.description = Some("A field whose NAME is description.".into());
        let mut x_field = make_field("x-openxcvr-spec", FieldType::Bool);
        x_field.description = Some("A field whose NAME is x-openxcvr-spec.".into());
        let mut unit_named = make_field("x-openxcvr-unit", FieldType::U8);
        unit_named.unit = Some("ms".into());
        schema.fields = vec![description_field, x_field, unit_named];

        let out = parse(&emit_page(&schema));
        let props = out["properties"].as_object().unwrap();
        assert!(
            props.contains_key("description")
                && props.contains_key("x-openxcvr-spec")
                && props.contains_key("x-openxcvr-unit"),
            "user-named properties are emitted as properties: {props:?}"
        );
        // The annotation for the field named `description` lands at
        // properties.description.description — one level deeper, exactly where
        // every other property's annotation lands.
        assert_eq!(
            props["description"]["description"].as_str().unwrap(),
            "A field whose NAME is description."
        );
        assert_eq!(
            props["x-openxcvr-unit"]["x-openxcvr-unit"]
                .as_str()
                .unwrap(),
            "ms"
        );

        // Full-cycle proof: strip the exact annotation paths and compare
        // with the metadata-free emission of the same schema — the
        // user-named properties survive both.
        let mut bare = schema.clone();
        for f in &mut bare.fields {
            f.description = None;
            f.unit = None;
            f.encoding = None;
            f.provenance = None;
            f.since = None;
            f.until = None;
        }
        let bare_out = parse(&emit_page(&bare));
        let stripped = remove_page_annotation_paths(out);
        assert_eq!(
            stripped, bare_out,
            "exact-path removal keeps user-named properties and reproduces the \
             metadata-free schema"
        );
        assert!(
            stripped["properties"]
                .as_object()
                .unwrap()
                .contains_key("description")
                && stripped["properties"]
                    .as_object()
                    .unwrap()
                    .contains_key("x-openxcvr-spec"),
            "user properties named like annotations survive the strip: {stripped}"
        );
    }

    // ── real-schema coverage: one representative page per family ─────────────

    fn load_page(relative: &str) -> PageSchema {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        crate::parser::pages::load_page_schema(&root.join(relative)).unwrap()
    }

    #[test]
    fn real_family_page_annotations() {
        // CMIS Page 00h: recorded provenance (quotation) on vendor_name, and
        // the recorded map-form erratum on the spec identity.
        let cmis = parse(&emit_page(&load_page("cmis/page-00h-administrative.yaml")));
        assert_eq!(
            cmis["x-openxcvr-spec"],
            serde_json::json!({
                "id": "cmis",
                "revision": "5.3",
                "errata": [{"revision": "1.0", "source": "docs/stds/cmis/OIF-CMIS-05-Errata-01.0.pdf"}]
            })
        );
        let vendor = &cmis["properties"]["vendor_name"];
        assert!(
            vendor["x-openxcvr-provenance"]["quote"]
                .as_str()
                .unwrap()
                .contains("16 character read-only field"),
            "recorded quotation on vendor_name: {vendor}"
        );
        assert!(
            vendor.get("description").is_none(),
            "vendor_name has no recorded description; nothing substitutes: {vendor}"
        );

        // SFF-8636 Page 20h: recorded quotation on the warning masks.
        let sff8636 = parse(&emit_page(&load_page(
            "sff-8636/page-20h-pam4-monitoring-1.yaml",
        )));
        assert_eq!(
            sff8636["x-openxcvr-spec"],
            serde_json::json!({"id": "sff-8636", "revision": "2.11"})
        );
        let mask = &sff8636["properties"]["param_low_warning_mask"];
        assert!(
            mask["x-openxcvr-provenance"]["quote"]
                .as_str()
                .unwrap()
                .contains("warning low"),
            "{mask}"
        );

        // SFF-8472 diagnostics: recorded lsb_units scaling metadata.
        let sff8472 = parse(&emit_page(&load_page(
            "sff-8472/page-a2h-diagnostics-base.yaml",
        )));
        assert_eq!(
            sff8472["x-openxcvr-spec"],
            serde_json::json!({"id": "sff-8472", "revision": "12.5a"})
        );
        let temp = &sff8472["properties"]["temperature_threshold"];
        assert_eq!(
            temp["x-openxcvr-lsb-units"].as_str().unwrap(),
            "1/256 deg C",
            "{temp}"
        );
        assert!(temp.get("x-openxcvr-unit").is_none());

        // CMIS Page 2Fh: an explicit recorded field unit.
        let vdm = parse(&emit_page(&load_page(
            "cmis/page-2fh-vdm-advertisement.yaml",
        )));
        let fine = &vdm["properties"]["fine_interval_length"];
        assert_eq!(
            fine["x-openxcvr-unit"].as_str().unwrap(),
            "0.1 ms",
            "{fine}"
        );
    }
}
