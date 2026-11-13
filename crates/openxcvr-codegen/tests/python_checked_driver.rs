//! CLI driver coverage for the checked Python revision pages
//! (python-checked-pages plan, Task 2).
//!
//! Target selection is derived from the owning CMIS namespace plus
//! `is_versioned()` — never a hand-maintained dispatch table: an explicit
//! `--spec cmis` run, a mixed/default run and an SFF-only run select exactly
//! the versioned CMIS pages. A synthetic raw-valid but checked-invalid
//! mapping must fail the whole invocation BEFORE any output file (raw enums
//! and pages included) is written or changed.
use std::collections::BTreeMap;
use std::path::Path;

fn run_cli(schemas: &Path, spec: Option<&str>, out: &Path) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"));
    cmd.arg("generate")
        .arg("--schemas")
        .arg(schemas)
        .arg("--lang")
        .arg("python")
        .arg("--out")
        .arg(out);
    if let Some(spec) = spec {
        cmd.arg("--spec").arg(spec);
    }
    cmd.output().expect("codegen CLI runs")
}

const ENUM_YAML: &str = concat!(
    "schema: openxcvr.enum.v2\n",
    "kind: enum\n",
    "id: TestEnum\n",
    "title: T\n",
    "entries:\n",
    "  - { value: 0x0, name: off, description: d }\n",
    "  - { value: 0x1..0x3, name: range, description: d }\n",
);

const SFF_ENUM_YAML: &str = concat!(
    "schema: openxcvr.enum.v2\n",
    "kind: enum\n",
    "id: TestSffEnum\n",
    "title: T\n",
    "entries:\n",
    "  - { value: 0x0, name: off, description: d }\n",
    "  - { value: 0x1..0x3, name: range, description: d }\n",
);

const VERSIONED_PAGE_YAML: &str = concat!(
    "schema: openxcvr.page.v2\n",
    "kind: page\n",
    "id: TestVersionedPage\n",
    "title: T\n",
    "page_address: \"01h\"\n",
    "size_bytes: 128\n",
    "fields:\n",
    "  - { name: always, offset: 128, type: u8 }\n",
    "  - { name: only_4_0, offset: 129, type: u8, since: \"4.0\", until: \"5.0\" }\n",
    "  - { name: promoted_4_0, offset: 130, type: u8, since: \"4.0\", until: \"5.0\", emit_value: false, changed_in: [ { rev: \"4.0\", emit_value: true } ] }\n",
);

const PLAIN_PAGE_YAML: &str = concat!(
    "schema: openxcvr.page.v2\n",
    "kind: page\n",
    "id: TestPlainPage\n",
    "title: T\n",
    "page_address: \"02h\"\n",
    "size_bytes: 128\n",
    "fields:\n",
    "  - { name: value, offset: 128, type: u8 }\n",
);

const SFF_PAGE_YAML: &str = concat!(
    "schema: openxcvr.page.v2\n",
    "kind: page\n",
    "id: TestSffPage\n",
    "title: T\n",
    "page_address: \"03h\"\n",
    "size_bytes: 128\n",
    "fields:\n",
    "  - { name: value, offset: 128, type: u8 }\n",
);

/// Build a synthetic schemas tree: a cmis namespace (enum + versioned page +
/// unversioned page) and an sff-8636 namespace (enum + page).
fn write_fixture(root: &Path) {
    let cmis = root.join("cmis");
    std::fs::create_dir_all(cmis.join("enums")).unwrap();
    std::fs::create_dir_all(cmis.join("pages")).unwrap();
    std::fs::write(
        cmis.join("_index.yaml"),
        concat!(
            "schema: openxcvr.schema_index.v1\n",
            "kind: memory_map\n",
            "includes:\n",
            "  - enums/test-enum.yaml\n",
            "  - pages/versioned.yaml\n",
            "  - pages/plain.yaml\n",
        ),
    )
    .unwrap();
    std::fs::write(cmis.join("enums/test-enum.yaml"), ENUM_YAML).unwrap();
    std::fs::write(cmis.join("pages/versioned.yaml"), VERSIONED_PAGE_YAML).unwrap();
    std::fs::write(cmis.join("pages/plain.yaml"), PLAIN_PAGE_YAML).unwrap();

    let sff = root.join("sff-8636");
    std::fs::create_dir_all(sff.join("enums")).unwrap();
    std::fs::create_dir_all(sff.join("pages")).unwrap();
    std::fs::write(
        sff.join("_index.yaml"),
        concat!(
            "schema: openxcvr.schema_index.v1\n",
            "kind: memory_map\n",
            "includes:\n",
            "  - enums/test-enum.yaml\n",
            "  - pages/sff-page.yaml\n",
        ),
    )
    .unwrap();
    std::fs::write(sff.join("enums/test-enum.yaml"), SFF_ENUM_YAML).unwrap();
    std::fs::write(sff.join("pages/sff-page.yaml"), SFF_PAGE_YAML).unwrap();
}

fn snapshot(dir: &Path) -> BTreeMap<String, Option<u64>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let name = entry.file_name();
            let rel = if prefix.is_empty() {
                name.to_string_lossy().into_owned()
            } else {
                format!("{prefix}/{}", name.to_string_lossy())
            };
            if path.is_dir() {
                stack.push((path, rel));
                continue;
            }
            let hash = std::fs::read(&path)
                .map(|bytes| {
                    let mut hash: u64 = 1469598103934665603;
                    for b in bytes {
                        hash ^= u64::from(b);
                        hash = hash.wrapping_mul(1099511628211);
                    }
                    hash
                })
                .ok();
            files.insert(rel, hash);
        }
    }
    files
}

/// Positive control + explicit-spec selection: `--spec cmis` writes nonempty
/// raw enums/pages and exactly one checked sibling for the versioned page;
/// the unversioned CMIS page gets none.
#[test]
fn explicit_cmis_spec_writes_checked_sibling_for_versioned_page_only() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());

    let out = tmp.path().join("out");
    let status = run_cli(tmp.path(), Some("cmis"), &out);
    assert!(
        status.status.success(),
        "cmis run failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let checked = out.join("test_versioned_page_revision.py");
    let code = std::fs::read_to_string(&checked)
        .unwrap_or_else(|_| panic!("checked sibling missing: {}", checked.display()));
    assert!(!code.is_empty());
    assert!(code.contains("class TestVersionedPageChecked:"));
    assert!(code.contains("class TestVersionedPageCheckedValues:"));
    assert!(code.contains("promoted_4_0"));

    // Positive control: raw enum + raw page outputs are nonempty.
    let enum_out = std::fs::read_to_string(out.join("enums").join("test_enum.py")).unwrap();
    assert!(!enum_out.is_empty() && enum_out.contains("class TestEnum"));
    let raw_page = std::fs::read_to_string(out.join("test_versioned_page.py")).unwrap();
    assert!(raw_page.contains("class TestVersionedPage:"));

    // Unversioned CMIS page: raw output, no checked sibling.
    assert!(out.join("test_plain_page.py").exists());
    assert!(!out.join("test_plain_page_revision.py").exists());
}

/// Mixed/default invocation (no --spec): the same CMIS-owned versioned page
/// gets its checked sibling; the SFF page does not.
#[test]
fn default_mixed_invocation_selects_cmis_owned_versioned_pages_only() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());

    let out = tmp.path().join("out");
    let status = run_cli(tmp.path(), None, &out);
    assert!(
        status.status.success(),
        "default run failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(out.join("test_versioned_page_revision.py").exists());
    assert!(out.join("test_sff_page.py").exists());
    assert!(!out.join("test_sff_page_revision.py").exists());
    assert!(!out.join("test_plain_page_revision.py").exists());
}

/// SFF-only generation writes no checked CMIS output at all.
#[test]
fn sff_only_generation_writes_no_checked_output() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());

    let out = tmp.path().join("out");
    let status = run_cli(tmp.path(), Some("sff-8636"), &out);
    assert!(
        status.status.success(),
        "sff run failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let mut revision_files = Vec::new();
    let mut stack = vec![out.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_revision.py"))
            {
                revision_files.push(path.display().to_string());
            }
        }
    }
    assert!(
        revision_files.is_empty(),
        "SFF-only run must not write checked output: {revision_files:?}"
    );
}

/// A raw-valid but checked-invalid mapping (bool→u8 changed_in override) must
/// fail the invocation BEFORE any file is written: the pre-existing output
/// set and hashes — the raw enum sentinel included — stay byte-identical, and
/// no partial generated file appears.
#[test]
fn checked_planning_failure_changes_no_output_file() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());

    let out = tmp.path().join("out");
    let ok = run_cli(tmp.path(), Some("cmis"), &out);
    assert!(ok.status.success(), "baseline run must succeed");
    let before = snapshot(&out);
    assert!(
        before.contains_key("enums/test_enum.py"),
        "enum sentinel must exist after the baseline run"
    );

    // Introduce the offending override: raw-valid (base bool field), but the
    // checked emitter must refuse the int/bool representation flip.
    std::fs::write(
        tmp.path().join("cmis/pages/versioned.yaml"),
        concat!(
            "schema: openxcvr.page.v2\n",
            "kind: page\n",
            "id: TestVersionedPage\n",
            "title: T\n",
            "page_address: \"01h\"\n",
            "size_bytes: 128\n",
            "fields:\n",
            "  - { name: always, offset: 128, type: u8 }\n",
            "  - { name: only_4_0, offset: 129, type: u8, since: \"4.0\", until: \"5.0\" }\n",
            "  - { name: retyped, offset: 131, type: bool, changed_in: [ { rev: \"5.0\", type: u8 } ] }\n",
        ),
    )
    .unwrap();

    let failed = run_cli(tmp.path(), Some("cmis"), &out);
    assert!(
        !failed.status.success(),
        "checked-invalid mapping must fail the invocation"
    );
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(
        stderr.contains("TestVersionedPage"),
        "error must name the page: {stderr}"
    );
    assert!(
        stderr.contains("retyped") && stderr.contains("5.0"),
        "error must name the field and revision: {stderr}"
    );

    let after = snapshot(&out);
    assert_eq!(
        before, after,
        "failed invocation must not add, remove or rewrite any output file"
    );
}
