//! Per-namespace presence validation tests (Stage 1 B1 supplement repair).
//!
//! Before the repair, `validate_presence` classified ALL loaded pages under
//! the REQUESTED `--spec`, so a supplement run (`--spec c-cmis-1.4`,
//! `--spec elsfp-1.0`) failed with `presence classification does not know
//! spec …` even though its own pages carry no `present_when`, and a run
//! with `--spec` omitted skipped presence classification entirely. The
//! Markdown top-index probe did not run presence validation at all, so
//! `docs/reference/index.md` claimed "schemas load and validate cleanly"
//! for namespaces generation rejects.
//!
//! The repair classifies presence per OWNING schema namespace (the first
//! path component below the schemas root): namespaces with a `SpecFamily`
//! are classified exactly as a `--spec <family>` run, with their own pages
//! as the resolution context; namespaces without one may not declare
//! `present_when` at all. Real-repo tests use the repository schemas;
//! synthetic fixtures are labelled synthetic in each test's comment.

use std::path::{Path, PathBuf};
use std::process::Command;

use openxcvr_codegen::emitters::markdown;
use openxcvr_codegen::parser::index::IndexLoader;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run the real CLI; returns (success, stdout, stderr).
fn run_generate(
    schemas: &Path,
    spec: Option<&str>,
    lang: &str,
    out: &Path,
) -> (bool, String, String) {
    run_generate_in(None, schemas, spec, lang, out)
}

fn run_generate_in(
    cwd: Option<&Path>,
    schemas: &Path,
    spec: Option<&str>,
    lang: &str,
    out: &Path,
) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"));
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.arg("generate")
        .arg("--schemas")
        .arg(schemas)
        .arg("--out")
        .arg(out)
        .arg("--lang")
        .arg(lang);
    if let Some(spec) = spec {
        cmd.arg("--spec").arg(spec);
    }
    let output = cmd.output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ─── synthetic fixture builders ─────────────────────────────────────────────

fn memory_map_index(spec_id: &str, imports: &[&str], includes: &[&str]) -> String {
    let mut index = format!(
        "schema: openxcvr.schema_index.v1\nspec:\n  id: {spec_id}\n  title: {spec_id} synthetic\n  revision: \"1.0\"\n"
    );
    if !imports.is_empty() {
        index.push_str("imports:\n");
        for import in imports {
            index.push_str(&format!("  - {import}\n"));
        }
    }
    index.push_str("kind: memory_map\nincludes:\n  - enum.yaml\n");
    for include in includes {
        index.push_str(&format!("  - {include}\n"));
    }
    index
}

fn enum_yaml(id: &str) -> String {
    format!(
        "schema: openxcvr.enum.v2\nkind: enum\nid: {id}\ntitle: {id} synthetic\nentries:\n  - {{ value: 0, name: zero }}\n"
    )
}

fn page_yaml(id: &str, fields: &str) -> String {
    format!(
        "schema: openxcvr.page.v2\nkind: page\nid: {id}\ntitle: {id} synthetic\npage_address: lower\nfields:\n{fields}"
    )
}

const PLAIN_FIELD: &str = "  - { name: plain, offset: 0, type: u8 }\n";
/// A well-formed Equals condition on a same-page field.
const GATED_ON_PLAIN: &str =
    "  - { name: gated, offset: 1, type: u8, present_when: { field: plain, equals: 1 } }\n";
/// A well-formed condition whose reference does not resolve.
const GATED_ON_MISSING: &str =
    "  - { name: gated, offset: 1, type: u8, present_when: \"no_such_field == 1\" }\n";

/// One synthetic namespace directory: `_index.yaml` plus one enum and one
/// page. Ids are distinct per namespace because the driver validates id
/// uniqueness across the whole loaded set.
fn write_spec(
    root: &Path,
    name: &str,
    enum_id: &str,
    page_id: &str,
    imports: &[&str],
    page_fields: &str,
) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("_index.yaml"),
        memory_map_index(name, imports, &["page.yaml"]),
    )
    .unwrap();
    std::fs::write(dir.join("enum.yaml"), enum_yaml(enum_id)).unwrap();
    std::fs::write(dir.join("page.yaml"), page_yaml(page_id, page_fields)).unwrap();
}

// ─── real repo schemas: supplement runs must generate again ─────────────────

#[test]
fn c_cmis_markdown_generation_succeeds() {
    // Real repo schemas. Red before the repair: the whole loaded set was
    // classified under the requested spec "c-cmis-1.4", which has no
    // SpecFamily.
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(
        &repo_root().join("schemas"),
        Some("c-cmis-1.4"),
        "markdown",
        &out,
    );
    assert!(ok, "c-cmis-1.4 markdown generation must succeed:\n{stderr}");
    assert!(
        out.join("c-cmis-1.4").join("index.md").is_file(),
        "c-cmis-1.4 reference tree missing in {}",
        out.display()
    );
}

#[test]
fn c_cmis_rust_generation_succeeds() {
    // Real repo schemas; every language shares the same validation gate.
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(
        &repo_root().join("schemas"),
        Some("c-cmis-1.4"),
        "rust",
        &out,
    );
    assert!(ok, "c-cmis-1.4 rust generation must succeed:\n{stderr}");
}

#[test]
fn elsfp_markdown_generation_succeeds() {
    // Real repo schemas. Needs both repairs: the presence classification
    // per owning namespace AND the six redundant `bit_width: 16`
    // declarations removed from the ELSFP u16be fields (red before with
    // the width error).
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(
        &repo_root().join("schemas"),
        Some("elsfp-1.0"),
        "markdown",
        &out,
    );
    assert!(ok, "elsfp-1.0 markdown generation must succeed:\n{stderr}");
    assert!(
        out.join("elsfp-1.0").join("index.md").is_file(),
        "elsfp-1.0 reference tree missing in {}",
        out.display()
    );
}

// ─── omitted --spec no longer bypasses presence validation ──────────────────

#[test]
fn omitted_spec_still_validates_presence() {
    // Synthetic: a cmis-namespace page whose present_when references a
    // nonexistent field. Before the repair, omitting --spec skipped
    // presence classification entirely, so this schema generated cleanly.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schemas");
    write_spec(
        &root,
        "cmis",
        "CmisProbeEnum",
        "CmisProbePage",
        &[],
        &format!("{PLAIN_FIELD}{GATED_ON_MISSING}"),
    );
    // Paths are also used for reference matching: keep native catalog data,
    // and make only the diagnostic label portable.
    let paths = IndexLoader::collect_page_schemas(&root, Some("cmis")).unwrap();
    let info = openxcvr_codegen::emitters::rust::catalog::page_catalog_info(&paths).unwrap();
    assert_eq!(
        info["CmisProbePage"].path.as_deref(),
        Some(paths[0].display().to_string().as_str())
    );
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(&root, None, "rust", &out);
    assert!(
        !ok,
        "an omitted --spec must not bypass presence classification:\n{stderr}"
    );
    // Presence diagnostics label the schema by its source file.
    for token in [
        "presence classification",
        "cmis/page.yaml",
        "gated",
        "no_such_field",
    ] {
        assert!(
            stderr.contains(token),
            "missing {token:?} in presence diagnostic:\n{stderr}"
        );
    }
}

// ─── imported pages validate under their own family ─────────────────────────

#[test]
fn supplement_run_validates_imported_pages_under_their_own_family() {
    // Synthetic: supplement-1.0 imports ../cmis/_index.yaml; the cmis page
    // carries an unresolvable present_when. A --spec supplement-1.0 run
    // must fail naming the CMIS page (classified under Family::Cmis with
    // the cmis namespace's own pages), not reject the supplement's spec id.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schemas");
    write_spec(
        &root,
        "cmis",
        "CmisProbeEnum",
        "CmisProbePage",
        &[],
        &format!("{PLAIN_FIELD}{GATED_ON_MISSING}"),
    );
    write_spec(
        &root,
        "supplement-1.0",
        "SuppProbeEnum",
        "SuppPage",
        &["../cmis/_index.yaml"],
        PLAIN_FIELD,
    );
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(&root, Some("supplement-1.0"), "markdown", &out);
    assert!(
        !ok,
        "imported cmis pages must still be presence-classified:\n{stderr}"
    );
    for token in [
        "presence classification",
        "cmis/page.yaml",
        "gated",
        "no_such_field",
    ] {
        assert!(
            stderr.contains(token),
            "missing {token:?} in presence diagnostic:\n{stderr}"
        );
    }
}

// ─── namespaces without a SpecFamily may not declare present_when ───────────

#[test]
fn supplement_owned_present_when_is_rejected_naming_namespace_page_field() {
    // Synthetic: a well-formed Equals condition on a supplement-owned page.
    // No SpecFamily exists for the namespace, so there is no revision model
    // to classify against — an explicit error naming namespace, page and
    // field, never a silent pass-through.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schemas");
    write_spec(
        &root,
        "supplement-1.0",
        "SuppProbeEnum",
        "SuppPage",
        &[],
        &format!("{PLAIN_FIELD}{GATED_ON_PLAIN}"),
    );
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(&root, Some("supplement-1.0"), "markdown", &out);
    assert!(
        !ok,
        "present_when in a family-less namespace must fail generation:\n{stderr}"
    );
    for token in [
        "presence classification",
        "supplement-1.0",
        "supplement-1.0/page.yaml",
        "SuppPage",
        "gated",
        "not supported",
    ] {
        assert!(
            stderr.contains(token),
            "missing {token:?} in presence diagnostic:\n{stderr}"
        );
    }
}

#[test]
fn supplement_without_present_when_generates() {
    // Synthetic: the same supplement namespace with no present_when
    // anywhere generates cleanly — declaring no conditions, it needs no
    // family.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schemas");
    write_spec(
        &root,
        "supplement-1.0",
        "SuppProbeEnum",
        "SuppPage",
        &[],
        PLAIN_FIELD,
    );
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(&root, Some("supplement-1.0"), "markdown", &out);
    assert!(
        ok,
        "family-less namespace without present_when must generate:\n{stderr}"
    );
    assert!(
        out.join("supplement-1.0").join("index.md").is_file(),
        "supplement reference tree missing in {}",
        out.display()
    );
}

// ─── the Markdown index agrees with generation ──────────────────────────────

#[test]
fn markdown_top_index_reports_presence_failure() {
    let mut indexes = Vec::new();
    for tmp in [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()] {
        // Synthetic: a cmis-namespace page generation rejects (unresolvable
        // present_when). The top index probe must run the same shared presence
        // validation, so the index row reports the presence failure instead of
        // claiming the namespace loads and validates cleanly.
        let root = tmp.path().join("schemas");
        write_spec(
            &root,
            "cmis",
            "CmisProbeEnum",
            "CmisProbePage",
            &[],
            &format!("{PLAIN_FIELD}{GATED_ON_MISSING}"),
        );

        // Generation rejects the namespace…
        let out = tmp.path().join("out");
        let (ok, _stdout, stderr) = run_generate(&root, Some("cmis"), "markdown", &out);
        assert!(
            !ok,
            "generation must reject the unresolvable present_when:\n{stderr}"
        );

        // …and the index row for the un-rendered namespace reports the same
        // presence failure (the probe is what regen-markdown.sh exercises).
        let index = markdown::index::emit_top_index(&root, &tmp.path().join("reference")).unwrap();
        // The loader normalizes paths before validation. Equivalent spellings of
        // its root must not leak an absolute worktree path into the diagnostic.
        let separator = std::path::MAIN_SEPARATOR;
        for equivalent in [
            root.join("cmis/.."),
            PathBuf::from(format!("{}{separator}{separator}", root.display())),
        ] {
            let other = markdown::index::emit_top_index(&equivalent, &tmp.path().join("reference"))
                .unwrap();
            assert_eq!(index, other, "root spelling changed a presence diagnostic");
        }
        assert!(
            index.contains("not rendered — schemas fail validation: presence classification"),
            "index row must report the presence failure:\n{index}"
        );
        for token in ["cmis/page.yaml", "gated", "no_such_field"] {
            assert!(
                index.contains(token),
                "index row must carry the presence diagnostic ({token}):\n{index}"
            );
        }
        assert!(
            !index.contains("load and validate cleanly"),
            "a namespace generation rejects must never be reported as a clean load:\n{index}"
        );
        indexes.push(index);
    }
    assert_eq!(
        indexes[0], indexes[1],
        "presence diagnostic depends on its worktree"
    );
}

#[test]
fn markdown_presence_label_preserves_a_path_like_literal() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schemas");
    let value = format!(
        "{}/literal\\kept",
        root.display()
            .to_string()
            .replace(std::path::MAIN_SEPARATOR, "/")
    );
    write_spec(
        &root,
        "cmis",
        "CmisProbeEnum",
        "CmisProbePage",
        &[],
        &format!(
            "{PLAIN_FIELD}  - {{ name: gated, offset: 1, type: u8, present_when: {{ field: plain, equals: '{}' }} }}\n",
            value.replace('\'', "''")
        ),
    );
    let index = markdown::index::emit_top_index(&root, &tmp.path().join("reference")).unwrap();
    for token in [
        "schemas/cmis/page.yaml",
        "presence classification",
        "is not true/false/decimal/0x-hex",
        &format!("{value:?}"),
    ] {
        assert!(
            index.contains(token),
            "missing diagnostic {token:?}:\n{index}"
        );
    }
}

/// The synthetic fixtures above stay meaningful: without the bad condition
/// the same cmis-namespace page classifies cleanly through the real CLI.
/// (`--lang markdown`: the Rust catalog emission additionally requires the
/// real `CmisLowerMemory` schema, which this fixture does not model.)
#[test]
fn synthetic_fixture_classifies_cleanly_without_the_condition() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schemas");
    write_spec(
        &root,
        "cmis",
        "CmisProbeEnum",
        "CmisProbePage",
        &[],
        &format!("{PLAIN_FIELD}{GATED_ON_PLAIN}"),
    );
    let paths = IndexLoader::collect_page_schemas(&root, Some("cmis")).unwrap();
    assert_eq!(paths.len(), 1, "fixture must collect exactly one page");
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate(&root, Some("cmis"), "markdown", &out);
    assert!(
        ok,
        "a resolvable Equals condition in a family namespace must classify:\n{stderr}"
    );
}

// ─── schemas root normalization in namespace_of ─────────────────────────────

#[test]
fn dot_slash_schemas_root_markdown_generation_succeeds() {
    // (h) Real repo schemas, run with working directory = repo root.
    // Red before the repair: `presence::namespace_of` stripped the raw
    // `./schemas` prefix without lexical normalization, so page paths
    // normalized by IndexLoader fell into "ungrouped" and failed presence
    // classification.
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate_in(
        Some(&repo_root()),
        Path::new("./schemas"),
        Some("cmis"),
        "markdown",
        &out,
    );
    assert!(
        ok,
        "--schemas ./schemas markdown generation must succeed:\n{stderr}"
    );
}

#[test]
fn folded_dot_dot_schemas_root_rust_generation_succeeds() {
    // (i) Real repo schemas, run with working directory = repo root.
    // Red before the repair: `schemas/../schemas` failed prefix stripping
    // against normalized page paths.
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    let (ok, _stdout, stderr) = run_generate_in(
        Some(&repo_root()),
        Path::new("schemas/../schemas"),
        Some("cmis"),
        "rust",
        &out,
    );
    assert!(
        ok,
        "--schemas schemas/../schemas rust generation must succeed:\n{stderr}"
    );
}

#[test]
fn namespace_of_normalizes_schemas_root_and_page_path() {
    // (j) Unit-level: `presence::namespace_of(Path::new("./schemas"), Path::new("schemas/cmis/x.yaml")) == Some("cmis")`,
    // and the same for root `schemas/../schemas`; a path outside the root returns None.
    use openxcvr_codegen::presence::namespace_of;

    assert_eq!(
        namespace_of(Path::new("./schemas"), Path::new("schemas/cmis/x.yaml")).as_deref(),
        Some("cmis")
    );
    assert_eq!(
        namespace_of(
            Path::new("schemas/../schemas"),
            Path::new("schemas/cmis/x.yaml")
        )
        .as_deref(),
        Some("cmis")
    );
    assert_eq!(
        namespace_of(Path::new("./schemas"), Path::new("other/cmis/x.yaml")),
        None
    );
}
