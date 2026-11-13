#[test]
fn driver_writes_per_rev_when_metadata_present() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let schema_yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
fields:
  - { name: f_3, offset: 0, type: u8 }
  - { name: f_4, offset: 1, type: u8, since: "4.0" }
"#;
    let schema_path = tmp.path().join("test-page.yaml");
    std::fs::write(&schema_path, schema_yaml).unwrap();

    let parsed = openxcvr_codegen::parser::pages::load_page_schema(&schema_path).expect("parse");
    let out_dir = tmp.path().join("out");
    openxcvr_codegen::emitters::rust::pages::emit_page_dispatched(&parsed, &out_dir).expect("emit");

    // PascalCase filename convention: id is used verbatim as filename.
    let v3 = std::fs::read_to_string(out_dir.join("v3_0/TestPage.rs")).unwrap();
    let v4 = std::fs::read_to_string(out_dir.join("v4_0/TestPage.rs")).unwrap();
    assert!(!v3.contains("f_4"), "f_4 leaked into v3_0");
    assert!(v4.contains("f_4"), "f_4 missing from v4_0");
}

/// An unversioned page (no version metadata on any field) must be written to
/// `out_dir/{PascalCaseId}.rs` — not lowercased, not into a v*/ subdir.
#[test]
fn unversioned_page_produces_pascal_case_filename() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let schema_yaml = r#"
schema: openxcvr.page.v2
kind: page
id: MyTestPage
title: Test
page_address: "0x00"
fields:
  - { name: simple_field, offset: 0, type: u8 }
"#;
    let schema_path = tmp.path().join("my-test-page.yaml");
    std::fs::write(&schema_path, schema_yaml).unwrap();

    let parsed = openxcvr_codegen::parser::pages::load_page_schema(&schema_path).expect("parse");
    let out_dir = tmp.path().join("out");
    openxcvr_codegen::emitters::rust::pages::emit_page_dispatched(&parsed, &out_dir).expect("emit");

    // Compare actual directory entries, not case-insensitive path lookups.
    // Exactly one PascalCase file is expected, with no per-rev subdirectory.
    let entries: Vec<_> = std::fs::read_dir(&out_dir)
        .expect("read out_dir")
        .map(|entry| entry.expect("read directory entry").file_name())
        .collect();
    assert_eq!(entries, [std::ffi::OsString::from("MyTestPage.rs")]);
}
