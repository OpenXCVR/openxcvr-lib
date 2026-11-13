//! Value emission is independent of catalog discovery and physical read authority.
use openxcvr_codegen::emitters::{
    c::CEmitter, csharp::CSharpEmitter, go::GoEmitter, java::JavaEmitter, openapi::OpenApiEmitter,
    php::PhpEmitter, python::PythonEmitter, rust::RustEmitter, typescript::TypeScriptEmitter,
    Emitter,
};
use openxcvr_codegen::parser::pages::load_page_schema;

fn schema(control: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("page.yaml");
    std::fs::write(
        &path,
        format!(
            r#"
schema: openxcvr.page.v1
kind: page
id: SyntheticValues
title: Test
page_address: lower
fields:
  - name: legacy_reserved
    offset: 0
    type: bitmap
    access: ro
    field_type: Reserved
    tags: [Reserved]
  - name: retained
    offset: 1
    type: u8
    access: rw
  - name: omitted_scalar
    offset: 2
    type: u8
    access: rw
    {control}
  - name: omitted_array
    offset_expr: "3 + lane"
    type: u8
    access: rw
    instances: {{ index: lane, start: 0, end: 1 }}
    {control}
  - name: omitted_bitmap
    offset: 5
    type: bitmap
    access: ro
    field_type: Reserved
    tags: [Reserved]
    {control}
"#
        ),
    )
    .unwrap();
    load_page_schema(&path).unwrap()
}

fn check(emitter: &dyn Emitter) {
    let legacy = schema("");
    let explicit_default = schema("emit_value: true");
    assert_eq!(
        emitter.emit_page(&legacy),
        emitter.emit_page(&explicit_default),
        "the default must preserve every legacy value surface"
    );
    let hidden = schema("emit_value: false");
    let mut absent = legacy.clone();
    absent.fields.retain(|f| !f.name.starts_with("omitted_"));
    assert_eq!(
        emitter.emit_page(&hidden),
        emitter.emit_page(&absent),
        "disabled values must not leak through decoding, setters or serialization"
    );
    // Retain all classification/catalog inputs; the emitter must not mutate IR.
    assert_eq!(hidden.fields.len(), legacy.fields.len());
    assert_eq!(hidden.fields[4].field_category.as_deref(), Some("Reserved"));
}

macro_rules! value_test {
    ($name:ident, $emitter:expr) => {
        #[test]
        fn $name() {
            check(&$emitter);
        }
    };
}
value_test!(rust, RustEmitter);
value_test!(c, CEmitter);
value_test!(python, PythonEmitter);
value_test!(typescript, TypeScriptEmitter);
value_test!(go, GoEmitter);
value_test!(java, JavaEmitter);
value_test!(csharp, CSharpEmitter);
value_test!(php, PhpEmitter);
value_test!(openapi, OpenApiEmitter);

#[test]
fn c_source_obeys_the_same_control() {
    let hidden = schema("emit_value: false");
    let mut absent = schema("");
    absent.fields.retain(|f| !f.name.starts_with("omitted_"));
    assert_eq!(
        CEmitter.emit_page_source(&hidden, "values.h"),
        CEmitter.emit_page_source(&absent, "values.h")
    );
}

#[test]
fn metadata_only_fields_remain_in_catalog_and_read_classifications() {
    use openxcvr_codegen::emitters::rust::catalog::{derive_rows, CatalogPageInfo};
    use std::collections::BTreeMap;
    let mut visible = schema("");
    visible.id = "CmisLowerMemory".to_owned();
    let mut hidden = visible.clone();
    for field in &mut hidden.fields {
        field.metadata.emit_value = Some(false);
    }
    let visible_output = RustEmitter.emit_page(&visible);
    let hidden_output = RustEmitter.emit_page(&hidden);
    // Classification uses complete physical metadata, never the value view.
    let marker = "    pub fn read_classifications()";
    assert_eq!(
        visible_output
            .split_once(marker)
            .unwrap()
            .1
            .split_once("\n    }\n")
            .unwrap()
            .0,
        hidden_output
            .split_once(marker)
            .unwrap()
            .1
            .split_once("\n    }\n")
            .unwrap()
            .0
    );
    let map = BTreeMap::<String, CatalogPageInfo>::new();
    assert_eq!(
        derive_rows(&[visible], "cmis", &map).unwrap(),
        derive_rows(&[hidden], "cmis", &map).unwrap()
    );
}

#[test]
fn c_supplied_wire_readers_obey_the_same_control() {
    let mut hidden = schema("emit_value: false");
    hidden.id = "Sff8636LowerMemory".to_owned();
    let mut absent = hidden.clone();
    absent.fields.retain(|f| !f.name.starts_with("omitted_"));
    assert_eq!(CEmitter.emit_page(&hidden), CEmitter.emit_page(&absent));
    assert_eq!(
        CEmitter.emit_page_source(&hidden, "values.h"),
        CEmitter.emit_page_source(&absent, "values.h")
    );
}

#[test]
fn real_new_reserved_row_is_metadata_only_and_legacy_rows_keep_their_policy() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/cmis/lower-memory.yaml");
    let real = load_page_schema(&path).unwrap();
    let new = real
        .fields
        .iter()
        .find(|f| f.name == "reserved_module_status_cmis53")
        .unwrap();
    assert!(!new.emits_value());
    for old in real
        .fields
        .iter()
        .filter(|f| f.name.starts_with("reserved_") && f.name.ends_with("cmis4"))
    {
        assert!(old.emits_value(), "historical policy changed: {}", old.name);
    }
    let mut absent = real.clone();
    absent.fields.retain(|f| f.name != new.name);
    for emitter in [
        &PythonEmitter as &dyn Emitter,
        &TypeScriptEmitter,
        &GoEmitter,
        &JavaEmitter,
        &CSharpEmitter,
        &PhpEmitter,
        &OpenApiEmitter,
        &CEmitter,
    ] {
        assert_eq!(emitter.emit_page(&real), emitter.emit_page(&absent));
    }
    // Rust also emits physical classifications, so compare only value code.
    let rust = RustEmitter.emit_page(&real);
    assert!(!rust.contains("reserved_module_status_cmis53"));
}

#[test]
fn emission_control_rejects_non_boolean_values() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("invalid.yaml");
    std::fs::write(&path, "schema: openxcvr.page.v1\nkind: page\nid: Test\ntitle: T\npage_address: lower\nfields:\n  - { name: sample, type: u8, offset: 0, emit_value: maybe }\n").unwrap();
    assert!(load_page_schema(&path).is_err());
}
