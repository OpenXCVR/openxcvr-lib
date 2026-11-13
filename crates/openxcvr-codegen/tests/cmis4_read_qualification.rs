//! Source-bound qualifications A and B; execute every missing/unknown mutation.
use openxcvr_codegen::{
    emitters::rust::per_rev::emit_page_per_rev,
    ir::{
        pages::{PageSchema, ReadEffectMetadata},
        version::{applicable_revs, resolve_field_for_rev, Rev},
    },
    parser::pages::load_page_schema,
};

fn load(name: &str) -> PageSchema {
    load_page_schema(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/cmis")
            .join(name),
    )
    .unwrap()
}
fn table_function(schema: &PageSchema) -> String {
    let output = emit_page_per_rev(schema, Rev::V4_0);
    let start = output.find("    pub fn read_classifications()").unwrap();
    let end = start + output[start..].find("\n    }\n").unwrap() + "\n    }\n".len();
    output[start..end].into()
}
fn qualification_mutations(schema: PageSchema, first: u32, last: u32, base: u32) {
    let mut programs = format!("mod baseline {{ {} }}\nfn main() {{ for i in {}..={} {{ assert!(baseline::read_classifications()[i].is_qualified_none()); }}", table_function(&schema), first-base, last-base);
    let mut mutations = String::new();
    let mut count = 0;
    for field in &schema.fields {
        if !applicable_revs(field, Rev::CMIS_ALL).contains(&Rev::V4_0) {
            continue;
        }
        let resolved = resolve_field_for_rev(field, Rev::V4_0);
        let coverage = resolved.bit_coverage().unwrap();
        let Some((byte, _)) = coverage
            .iter()
            .find(|(byte, _)| (first..=last).contains(byte))
        else {
            continue;
        };
        assert!(
            coverage.iter().all(|(b, _)| (first..=last).contains(b)),
            "{} straddles qualification",
            field.name
        );
        assert_eq!(
            resolved.metadata.read_effect,
            Some(ReadEffectMetadata::None),
            "{}",
            field.name
        );
        let provenance = resolved.metadata.read_effect_provenance.as_ref().unwrap();
        assert_eq!(
            provenance["source"].as_str(),
            Some("CMIS 4.0 (CMIS4p0_Third_Party_Spec)"),
            "{}",
            field.name
        );
        for key in ["section", "table", "row", "basis"] {
            assert!(
                !provenance[key].as_str().unwrap().is_empty(),
                "{} {key}",
                field.name
            );
        }
        assert!(provenance["pdf_page"].is_sequence());
        for missing in [false, true] {
            let mut mutated = schema.clone();
            if missing {
                // DateCode has both a composite and subfield view. Remove all
                // redundant evidence for this constituent, not just one alias.
                let mask = coverage.iter().find(|(b, _)| *b == byte).unwrap().1;
                mutated.fields.retain(|f| {
                    !f.bit_coverage()
                        .unwrap()
                        .iter()
                        .any(|(b, m)| b == byte && m & mask != 0)
                });
            } else {
                let f = mutated
                    .fields
                    .iter_mut()
                    .find(|f| f.name == field.name)
                    .unwrap();
                f.changed_in = None;
                f.metadata.read_effect = Some(ReadEffectMetadata::Unknown);
                f.metadata.read_effect_provenance = None;
            }
            mutations.push_str(&format!(
                "mod mutation{count} {{ {} }}\n",
                table_function(&mutated)
            ));
            programs.push_str(&format!(
                "assert!(!mutation{count}::read_classifications()[{}].is_qualified_none(), {:?});",
                byte - base,
                format!("{} missing={missing}", field.name)
            ));
            count += 1;
        }
    }
    assert!(count > 0);
    programs.push_str("}\n");
    programs.push_str(&mutations);
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("mutations.rs");
    std::fs::write(&source, programs).unwrap();
    // pass compiles and runs main; the temporary runner drops before scratch.
    trybuild::TestCases::new().pass(&source);
    println!("executed {count} missing/unknown constituent mutations for {first}..={last}");
}
#[test]
fn a_every_page00_constituent_has_40_authority_and_fails_closed_when_unknown_or_missing() {
    qualification_mutations(load("page-00h-administrative.yaml"), 128, 222, 128);
}
#[test]
fn b_every_byte2_constituent_has_40_authority_and_fails_closed_when_unknown_or_missing() {
    qualification_mutations(load("lower-memory.yaml"), 2, 2, 0);
}
#[test]
fn b_table_8_2_field_boundaries_are_not_copied_from_53() {
    let schema = load("lower-memory.yaml");
    let fields: Vec<_> = schema
        .fields
        .iter()
        .filter(|f| applicable_revs(f, Rev::CMIS_ALL).contains(&Rev::V4_0))
        .map(|f| resolve_field_for_rev(f, Rev::V4_0))
        .collect();
    let speed = fields
        .iter()
        .find(|f| f.name == "twi_max_speed_cmis4")
        .expect("Table 8-2 TWI Maximum speed, not 5.3 MciMaxSpeed");
    assert_eq!(speed.bit_offset, Some(2));
    assert_eq!(speed.bit_width, Some(2));
    assert!(!fields.iter().any(
        |f| ["stepped_config_only", "auto_commissioning", "mci_max_speed"]
            .contains(&f.name.as_str())
    ));
}

#[test]
fn mutually_exclusive_speed_fields_are_not_a_union_layout_collision() {
    use openxcvr_codegen::{parser::pages::parse_page_schema, validator::validate_pages};
    let yaml = "schema: openxcvr.page.v2\nkind: page\nid: Test\ntitle: Test\npage_address: lower\nsize_bytes: 128\nfields:\n  - { name: old_speed, offset: 2, bit_offset: 2, bit_width: 2, type: u2, since: '4.0', until: '5.0' }\n  - { name: new_speed, offset: 2, bit_offset: 2, bit_width: 4, type: u4, since: '5.0' }\n";
    validate_pages(&[parse_page_schema(yaml).unwrap()]).unwrap();
    let overlapping = yaml.replace("until: '5.0'", "until: '5.1'");
    assert!(validate_pages(&[parse_page_schema(&overlapping).unwrap()])
        .unwrap_err()
        .to_string()
        .contains("overlap"));
}

#[test]
fn speed_bit_width_override_is_revision_scoped_and_restores_later_width() {
    use openxcvr_codegen::parser::pages::parse_page_schema;
    let schema = parse_page_schema("schema: openxcvr.page.v2\nkind: page\nid: Test\ntitle: Test\npage_address: lower\nsize_bytes: 128\nfields:\n  - name: speed\n    offset: 2\n    bit_offset: 2\n    bit_width: 4\n    type: u4\n    changed_in:\n      - { rev: '4.0', type: u2, bit_width: 2 }\n      - { rev: '5.0', type: u4, bit_width: 4 }\n").unwrap();
    for (rev, width) in [
        (Rev::V3_0, 4),
        (Rev::V4_0, 2),
        (Rev::V5_0, 4),
        (Rev::V5_3, 4),
    ] {
        assert_eq!(
            resolve_field_for_rev(&schema.fields[0], rev).bit_width,
            Some(width)
        );
    }
    for width in [0, 9] {
        let mut invalid = schema.clone();
        invalid.fields[0].changed_in.as_mut().unwrap()[0].bit_width = Some(width);
        assert!(openxcvr_codegen::validator::validate_pages(&[invalid]).is_err());
    }
}

#[test]
fn twi_speed_codes_do_not_acquire_53_fast_mode_meaning() {
    let schema = load("lower-memory.yaml");
    let field = schema
        .fields
        .iter()
        .find(|f| f.name == "mci_max_speed")
        .unwrap();
    let v4 = resolve_field_for_rev(field, Rev::V4_0);
    let codes = format!("{:?}", v4.enum_inline);
    assert!(codes.contains("reserved_2_3"), "{codes}");
    assert!(!codes.contains("i2c_3p4mhz"));
    assert_eq!(v4.bit_width, Some(2));
    assert_eq!(v4.name, "twi_max_speed_cmis4");

    // CMIS 3.0 (T18 p49), 5.0 (T8-4 p108), 5.1 (T8-4 p124), 5.2 (T8-5 p127)
    // define 2 bits (3-2) with 5-4 Reserved and codes 00b/01b/10b/11b only.
    // They do not acquire the 5.3 4-bit width or 3.4 MHz fast mode code.
    for rev in [Rev::V3_0, Rev::V5_0, Rev::V5_1, Rev::V5_2] {
        let resolved = resolve_field_for_rev(field, rev);
        let codes = format!("{:?}", resolved.enum_inline);
        assert!(codes.contains("reserved_2_3"), "{rev:?}: {codes}");
        assert!(!codes.contains("i2c_3p4mhz"), "{rev:?}: {codes}");
        assert_eq!(resolved.bit_width, Some(2), "{rev:?}");
        assert_eq!(resolved.name, field.name, "{rev:?}");
    }

    // Only CMIS 5.3 (T8-5 pp. 142-143) introduces the 4-bit width and 3.4 MHz code.
    let v5_3 = resolve_field_for_rev(field, Rev::V5_3);
    assert_eq!(v5_3.enum_inline, field.enum_inline);
    assert_eq!(v5_3.bit_width, field.bit_width);
    assert_eq!(v5_3.name, field.name);
}

#[test]
fn reserved_administrative_names_and_value_provenance_are_40_only() {
    use openxcvr_codegen::emitters::{
        c::CEmitter, csharp::CSharpEmitter, go::GoEmitter, java::JavaEmitter,
        openapi::OpenApiEmitter, php::PhpEmitter, python::PythonEmitter, rust::RustEmitter,
        typescript::TypeScriptEmitter, Emitter,
    };

    let schema = load("page-00h-administrative.yaml");

    // Byte 214 untouched case: uses aliased_to mechanism
    let later_214 = "reserved_mci_related_advertisement";
    let reserved_214 = "reserved_administrative_214_cmis4";
    let field_214 = schema
        .fields
        .iter()
        .find(|f| f.name == later_214)
        .expect("field 214 must exist");
    let v4_214 = resolve_field_for_rev(field_214, Rev::V4_0);
    assert_eq!(v4_214.name, reserved_214);
    assert_eq!(
        (v4_214.offset, v4_214.bit_offset, v4_214.bit_width),
        (Some(214), None, None)
    );
    assert_eq!(v4_214.ty, field_214.ty);
    let prov_214 = v4_214.provenance.as_ref().unwrap();
    assert_eq!(prov_214["table"].as_str(), Some("Table 8-15"));
    assert!(
        prov_214["quote"].as_str().unwrap().contains("reserved")
            || prov_214["quote"].as_str().unwrap().contains("Reserved")
    );
    for rev in [Rev::V3_0, Rev::V5_0, Rev::V5_1, Rev::V5_2, Rev::V5_3] {
        let resolved = resolve_field_for_rev(field_214, rev);
        assert_eq!(resolved.name, later_214);
        assert_eq!(resolved.provenance, field_214.provenance);
        assert_eq!(resolved.emits_setter(), field_214.emits_setter());
    }

    // Three subject fields under Option D repair shape:
    // Windowed reserved entries (since 4.0, until 5.0) and functional entries (since 5.3)
    let subject_cases = [
        (
            "attenuation_at_53p1ghz",
            "reserved_administrative_208_cmis4",
            208,
            None,
            None,
            "Table 8-21",
        ),
        (
            "mci_flow_control_duration_encoding",
            "reserved_administrative_213_bit7_cmis4",
            213,
            Some(7),
            Some(1),
            "Table 8-15",
        ),
        (
            "mci_flow_control_duration",
            "reserved_administrative_213_bits6_0_cmis4",
            213,
            Some(0),
            Some(7),
            "Table 8-15",
        ),
    ];

    let output_v4 = emit_page_per_rev(&schema, Rev::V4_0);
    assert!(output_v4.contains(&format!("pub {reserved_214}:")));
    assert!(!output_v4.contains(later_214));

    for (later, reserved, offset, bit, width, table) in subject_cases {
        // Reserved entry: applicable at 4.0 only
        let res_field = schema
            .fields
            .iter()
            .find(|f| f.name == reserved)
            .unwrap_or_else(|| panic!("reserved field {reserved} must exist in schema"));
        let res_revs = applicable_revs(res_field, Rev::CMIS_ALL);
        assert_eq!(
            res_revs,
            std::collections::BTreeSet::from([Rev::V4_0]),
            "{reserved} must be applicable at CMIS 4.0 only"
        );
        assert!(
            !res_field.emits_value(),
            "{reserved} base field must have emit_value: false"
        );

        let v4 = resolve_field_for_rev(res_field, Rev::V4_0);
        assert_eq!(v4.name, reserved);
        assert_eq!(
            (v4.offset, v4.bit_offset, v4.bit_width),
            (Some(offset), bit, width)
        );
        assert!(
            v4.encoding.is_none(),
            "{reserved} must carry no encoding at 4.0"
        );
        assert!(
            v4.emits_value(),
            "{reserved} must have emit_value: true at 4.0"
        );
        let provenance = v4.provenance.as_ref().unwrap();
        assert_eq!(provenance["table"].as_str(), Some(table));
        assert!(
            provenance["quote"].as_str().unwrap().contains("reserved")
                || provenance["quote"].as_str().unwrap().contains("Reserved")
        );

        // Functional entry: applicable at 5.3 only
        let func_field = schema
            .fields
            .iter()
            .find(|f| f.name == later)
            .unwrap_or_else(|| panic!("functional field {later} must exist in schema"));
        let func_revs = applicable_revs(func_field, Rev::CMIS_ALL);
        assert_eq!(
            func_revs,
            std::collections::BTreeSet::from([Rev::V5_3, Rev::V5_4]),
            "{later} must be applicable since CMIS 5.3 (5.3 and 5.4)"
        );
        assert!(
            func_field.aliased_to.is_none(),
            "{later} must not carry aliased_to in Option D"
        );

        let v5_3 = resolve_field_for_rev(func_field, Rev::V5_3);
        assert_eq!(v5_3.name, later);

        // v4 emission contains reserved name, not later name
        assert!(output_v4.contains(&format!("pub {reserved}:")));
        assert!(
            !output_v4.contains(later),
            "later name in declarations/accessors: {later}"
        );
    }

    // Metamorphic guard & suppression pin: base emit_value: false suppresses the three
    // reserved entries from union and all flat language surfaces. Emitting schema vs
    // schema without the reserved entries must be byte-for-byte identical, and none
    // of the flat surfaces may contain the reserved names.
    let reserved_names = [
        "reserved_administrative_208_cmis4",
        "reserved_administrative_213_bit7_cmis4",
        "reserved_administrative_213_bits6_0_cmis4",
    ];
    let mut without_reserved = schema.clone();
    without_reserved
        .fields
        .retain(|f| !reserved_names.contains(&f.name.as_str()));

    let flat_emitters: [(&str, Box<dyn Emitter>); 9] = [
        ("rust", Box::new(RustEmitter)),
        ("c", Box::new(CEmitter)),
        ("python", Box::new(PythonEmitter)),
        ("typescript", Box::new(TypeScriptEmitter)),
        ("go", Box::new(GoEmitter)),
        ("java", Box::new(JavaEmitter)),
        ("csharp", Box::new(CSharpEmitter)),
        ("php", Box::new(PhpEmitter)),
        ("openapi", Box::new(OpenApiEmitter)),
    ];

    for (name, emitter) in &flat_emitters {
        let output = emitter.emit_page(&schema);
        let output_without = emitter.emit_page(&without_reserved);
        assert_eq!(
            output, output_without,
            "Flat/union emission for {name} must be identical to tree without reserved entries"
        );
        for res_name in reserved_names {
            assert!(
                !output.contains(res_name),
                "Flat/union emission for {name} must not contain reserved symbol {res_name}"
            );
        }
    }
    assert_eq!(
        CEmitter.emit_page_source(&schema, "pages/cmis_page00h_administrative.h"),
        CEmitter.emit_page_source(&without_reserved, "pages/cmis_page00h_administrative.h"),
        "C page source must be identical to tree without reserved entries"
    );
}
