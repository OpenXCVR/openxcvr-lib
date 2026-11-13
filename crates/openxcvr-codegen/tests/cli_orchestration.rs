//! CLI dispatch and fail-closed preflight contracts.
use openxcvr_codegen::emitters::{
    c::CEmitter, csharp::CSharpEmitter, go::GoEmitter, java::JavaEmitter, openapi::OpenApiEmitter,
    php::PhpEmitter, python::PythonEmitter, rust::RustEmitter, typescript::TypeScriptEmitter,
    Emitter,
};
use openxcvr_codegen::parser;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ENUM: &str = "schema: openxcvr.enum.v2
kind: enum
id: SampleState
title: Synthetic state
entries:
  - { value: 1, name: ready, description: Ready }
";
const PAGE: &str = "schema: openxcvr.page.v2
kind: page
id: sample_page
title: Synthetic page
page_address: '01h'
size_bytes: 128
fields:
  - { name: value, offset: 128, type: u8 }
";
const MACHINE: &str = "schema: openxcvr.state_machine.v2
kind: state_machine
id: SampleMachine
title: Synthetic machine
spec: { id: cmis, revision: '5.3' }
state_enum: SampleState
scope: { kind: per_module, cardinality: 1 }
initial_state: Ready
states:
  - { name: Ready, enum_name: ready, enum_value: 1, class: stable, is_initial: true }
transitions: []
";

fn fixture(root: &Path, machine: &str) {
    let dir = root.join("synthetic");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, content) in [
        ("enum.yaml", ENUM),
        ("page.yaml", PAGE),
        ("machine.yaml", machine),
        (
            "_index.yaml",
            "schema: openxcvr.schema_index.v1\nkind: memory_map\nincludes: [enum.yaml, page.yaml]\nstate_machines: [machine.yaml]\n",
        ),
    ] {
        std::fs::write(dir.join(name), content).unwrap();
    }
}

fn run(schemas: &Path, out: &Path, lang: Option<&str>) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"));
    cmd.args(["generate", "--spec", "synthetic", "--schemas"])
        .arg(schemas)
        .arg("--out")
        .arg(out);
    if let Some(lang) = lang {
        cmd.args(["--lang", lang]);
    }
    cmd.output().unwrap()
}

fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry.path().strip_prefix(dir).unwrap().to_path_buf(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn unsupported_language_is_rejected_without_changing_existing_output() {
    let temp = tempfile::tempdir().unwrap();
    let schemas = temp.path().join("schemas");
    fixture(&schemas, MACHINE);
    let out = temp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("SampleState.rs"), "existing enum").unwrap();
    let before = files(&out);

    let result = run(&schemas, &out, Some("javascript"));
    assert!(!result.status.success(), "accepted an unsupported language");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unsupported --lang"), "{stderr}");
    assert!(stderr.contains("javascript"), "{stderr}");
    assert!(
        result.stdout.is_empty(),
        "reported success on invalid input"
    );
    assert_eq!(
        files(&out),
        before,
        "changed output for an unsupported language"
    );
}

#[test]
fn every_language_reports_all_invalid_pages_without_changing_output() {
    let temp = tempfile::tempdir().unwrap();
    let schemas = temp.path().join("schemas");
    fixture(&schemas, MACHINE);
    let spec = schemas.join("synthetic");
    for name in ["bad-a.yaml", "bad-b.yaml"] {
        std::fs::write(
            spec.join(name),
            PAGE.replace("type: u8", "type: not_a_type"),
        )
        .unwrap();
    }
    std::fs::write(
        spec.join("_index.yaml"),
        "schema: openxcvr.schema_index.v1\nkind: memory_map\nincludes: [enum.yaml, page.yaml, bad-a.yaml, bad-b.yaml]\nstate_machines: [machine.yaml]\n",
    )
    .unwrap();

    for lang in [
        "rust",
        "python",
        "go",
        "php",
        "c",
        "typescript",
        "java",
        "csharp",
        "openapi",
        "markdown",
    ] {
        let out = temp.path().join(lang);
        std::fs::create_dir_all(out.join("nested")).unwrap();
        std::fs::write(out.join("SampleState.rs"), "existing enum").unwrap();
        std::fs::write(out.join("nested/keep.txt"), "unrelated output").unwrap();
        let before = files(&out);
        let result = run(&schemas, &out, Some(lang));
        assert!(!result.status.success(), "{lang} accepted invalid pages");
        let stderr = String::from_utf8_lossy(&result.stderr);
        for expected in [
            "failed to load 2 page schema(s)",
            "bad-a.yaml",
            "bad-b.yaml",
        ] {
            assert!(
                stderr.contains(expected),
                "{lang}: missing {expected:?} in {stderr}"
            );
        }
        assert!(
            result.stdout.is_empty(),
            "{lang} reported success on invalid input"
        );
        assert_eq!(
            files(&out),
            before,
            "{lang} changed files before preflight completed"
        );
    }
}

#[test]
fn each_language_keeps_its_emitter_and_filenames() {
    let temp = tempfile::tempdir().unwrap();
    let schemas = temp.path().join("schemas");
    fixture(&schemas, MACHINE);
    let enum_schema =
        parser::enums::load_enum_schema(&schemas.join("synthetic/enum.yaml")).unwrap();
    let page = parser::pages::load_page_schema(&schemas.join("synthetic/page.yaml")).unwrap();
    let cases: &[(&str, &dyn Emitter, &str, &str)] = &[
        ("rust", &RustEmitter, "SampleState.rs", "sample_page.rs"),
        (
            "python",
            &PythonEmitter,
            "enums/sample_state.py",
            "sample_page.py",
        ),
        ("go", &GoEmitter, "enums/sample_state.go", "sample_page.go"),
        (
            "php",
            &PhpEmitter,
            "Enums/SampleState.php",
            "sample_page.php",
        ),
        (
            "c",
            &CEmitter,
            "enums/sample_state.h",
            "pages/sample_page.h",
        ),
        (
            "typescript",
            &TypeScriptEmitter,
            "enums/sample_state.ts",
            "sample_page.ts",
        ),
        ("java", &JavaEmitter, "SampleState.java", "SamplePage.java"),
        ("csharp", &CSharpEmitter, "SampleState.cs", "sample_page.cs"),
        (
            "openapi",
            &OpenApiEmitter,
            "enums/sample_state.schema.json",
            "pages/sample_page.schema.json",
        ),
    ];
    for &(lang, emitter, enum_file, page_file) in cases {
        let out = temp.path().join(lang);
        let result = run(&schemas, &out, Some(lang));
        assert!(
            result.status.success(),
            "{lang}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(out.join(enum_file)).unwrap(),
            emitter.emit_enum(&enum_schema),
            "{lang} enum"
        );
        let expected_page = if lang == "c" {
            CEmitter.emit_page_header(&page)
        } else {
            emitter.emit_page(&page)
        };
        assert_eq!(
            std::fs::read_to_string(out.join(page_file)).unwrap(),
            expected_page,
            "{lang} page"
        );
        if lang == "c" {
            assert_eq!(
                std::fs::read_to_string(out.join("pages/sample_page.c")).unwrap(),
                CEmitter.emit_page_source(&page, page_file)
            );
        }
    }
}

#[test]
fn default_rust_writes_the_preflighted_machine_and_module_wiring() {
    let temp = tempfile::tempdir().unwrap();
    let schemas = temp.path().join("schemas");
    fixture(&schemas, MACHINE);
    let out = temp.path().join("out");
    let result = run(&schemas, &out, None);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let machine =
        parser::state_machines::load_state_machine_schema(&schemas.join("synthetic/machine.yaml"))
            .unwrap();
    let enums = [parser::enums::load_enum_schema(&schemas.join("synthetic/enum.yaml")).unwrap()];
    let expected =
        openxcvr_codegen::emitters::rust::state_machines::emit_state_machine_rust(&machine, &enums)
            .unwrap();
    assert_eq!(
        std::fs::read_to_string(out.join("fsm/sample_machine.rs")).unwrap(),
        expected
    );
    assert!(std::fs::read_to_string(out.join("fsm/mod.rs"))
        .unwrap()
        .contains("pub mod sample_machine;"));
    assert!(std::fs::read_to_string(out.join("mod.rs"))
        .unwrap()
        .contains("pub mod fsm;"));
    let explicit = temp.path().join("explicit");
    assert!(run(&schemas, &explicit, Some("rust")).status.success());
    assert_eq!(files(&out), files(&explicit));
}

#[test]
fn every_language_rejects_machine_enum_mismatch_before_writing_files() {
    let temp = tempfile::tempdir().unwrap();
    let schemas = temp.path().join("schemas");
    fixture(
        &schemas,
        &MACHINE.replace("state_enum: SampleState", "state_enum: MissingState"),
    );
    for lang in [
        "rust",
        "python",
        "go",
        "php",
        "c",
        "typescript",
        "java",
        "csharp",
        "openapi",
        "markdown",
    ] {
        let out = temp.path().join(lang);
        std::fs::create_dir_all(out.join("fsm")).unwrap();
        std::fs::write(out.join("SampleState.rs"), "existing enum").unwrap();
        std::fs::write(out.join("fsm/sample_machine.rs"), "existing machine").unwrap();
        let before = files(&out);
        let result = run(&schemas, &out, Some(lang));
        assert!(
            !result.status.success(),
            "{lang} accepted a missing state enum"
        );
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains("names state enum MissingState, which is not among"),
            "{lang}: {stderr}"
        );
        assert_eq!(
            files(&out),
            before,
            "{lang} changed files before preflight completed"
        );
    }
}
