//! Strict schema vocabulary: unknown keys in schema documents must be
//! rejected, and every schema document a production loader exists for must
//! keep loading through those loaders.
//!
//! Coverage, measured against the `schemas/` tree (the counts below are the
//! current census, recorded so the next reader knows what these guards own;
//! loader and template assertions are floors and path pins, while bucket
//! totality is an exact census):
//!
//! * the **289 strict-namespace documents** — 156 `openxcvr.page.v1` +
//!   6 `openxcvr.page.v2` + 86 `openxcvr.enum.v2` +
//!   2 `openxcvr.state_machine.v1` + 1 `openxcvr.state_machine.v2` +
//!   33 `openxcvr.cdb_command.v1` +
//!   4 `openxcvr.tom_message.v1` + 1 `openxcvr.calibration.v1` — each loaded
//!   by its production loader;
//! * the **16 strict index documents** — 15 `openxcvr.schema_index.v1` +
//!   1 `openxcvr.cdb_command_index.v1` — each loaded strictly through
//!   `IndexFile` (and their 36 templates validated);
//! * the **118 field/entry fragments**, deserialized transitively by the
//!   strict documents that include them.
//!
//! All 423 schema files under `schemas/` are covered across these buckets:
//! 289 strict documents + 16 strict index documents + 118 fragments = 423 schema
//! files total.
//!
//! The vocabulary declaration made every key the tree already
//! writes a declared field; these tests pin the second half of the change:
//! a key outside that vocabulary — a misspelling of a real one — fails loud
//! with the key named in the error, and strictness cannot silently reject
//! a real document. Documents are classified by the production probe
//! (`parser::index::probe_schema_kind`), never a test-local copy, and the
//! guards assert that every strict-namespace document found is loaded and
//! every `_index.yaml` found is strictly loaded through IndexFile and has
//! its templates validated — a silently skipped one fails. Fragment wrapper
//! keys (`fields:` / `entries:`, the whole vocabulary gives a
//! fragment) are gated by the same contract.
//!
//! Each rejection test has a paired control: the same document with the
//! correctly spelled key must load, so a rejection caused by a missing
//! required field cannot masquerade as strictness.

use std::path::{Path, PathBuf};

use openxcvr_codegen::parser::calibration::load_calibration_schema;
use openxcvr_codegen::parser::cdb_commands::load_cdb_command_schema;
use openxcvr_codegen::parser::enums::load_enum_schema;
use openxcvr_codegen::parser::index::{load_index_file, probe_schema_kind, IndexFile, IndexLoader};
use openxcvr_codegen::parser::pages::{load_page_schema, validate_index_templates};
use openxcvr_codegen::parser::state_machines::load_state_machine_schema;
use openxcvr_codegen::parser::tom_messages::load_tom_message_schema;

const PAGE_HEADER: &str =
    "schema: openxcvr.page.v1\nkind: page\nid: P\ntitle: P\npage_address: 00h\n";
const ENUM_HEADER: &str = "schema: openxcvr.enum.v2\nkind: enum\nid: E\ntitle: E\n";

// ── guard: the whole schemas/ tree loads through the production loaders ─────

fn schema_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

fn walk_yaml(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "yaml") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_schema_document_loads_through_the_production_loaders() {
    let root = schema_root();
    let files = walk_yaml(&root);
    assert!(!files.is_empty(), "no YAML found under {}", root.display());
    let mut strict_found = 0usize;
    let mut loaded = 0usize;
    let mut loaded_paths: Vec<PathBuf> = Vec::new();
    let mut failures = Vec::new();
    for path in &files {
        // Classification is the production probe itself, not a test-local
        // reimplementation: a divergent copy once skipped the two ELSFP
        // pages that begin with a `---` document marker (review B1).
        let kind = probe_schema_kind(path);
        // The strict namespaces are the ones a production loader exists
        // for. All 289 schema documents in the tree are covered here.
        // The rest of the tree is covered by the other guards in this file:
        // fragments are deserialized transitively by the strict documents
        // that include them, and `_index.yaml` templates by the
        // index-template guard below.
        let strict = matches!(
            kind.as_deref(),
            Some("openxcvr.page.v1")
                | Some("openxcvr.page.v2")
                | Some("openxcvr.enum.v2")
                | Some("openxcvr.state_machine.v1")
                | Some("openxcvr.state_machine.v2")
                | Some("openxcvr.cdb_command.v1")
                | Some("openxcvr.tom_message.v1")
                | Some("openxcvr.calibration.v1")
        );
        if !strict {
            continue;
        }
        strict_found += 1;
        let result = match kind.as_deref() {
            Some("openxcvr.page.v1") | Some("openxcvr.page.v2") => {
                load_page_schema(path).map(|_| ())
            }
            Some("openxcvr.enum.v2") => load_enum_schema(path).map(|_| ()),
            Some("openxcvr.state_machine.v1") | Some("openxcvr.state_machine.v2") => {
                load_state_machine_schema(path).map(|_| ())
            }
            Some("openxcvr.cdb_command.v1") => load_cdb_command_schema(path).map(|_| ()),
            Some("openxcvr.tom_message.v1") => load_tom_message_schema(path).map(|_| ()),
            Some("openxcvr.calibration.v1") => load_calibration_schema(path).map(|_| ()),
            // Unreachable while the strict set above and this dispatch stay
            // in sync: a namespace counted strict but not dispatched falls
            // through here and trips the loaded == strict_found assertion.
            _ => continue,
        };
        match result {
            Ok(()) => {
                loaded += 1;
                loaded_paths.push(path.clone());
            }
            Err(err) => failures.push(format!("{}: {err:#}", path.display())),
        }
    }
    // Floor, not a census: the corpus holds well over a hundred documents,
    // and the test must fail rather than pass vacuously if the walk ever
    // stops seeing them.
    assert!(
        loaded >= 100,
        "only {loaded} schema documents loaded under {}; failures:\n{}",
        root.display(),
        failures.join("\n")
    );
    // Every strict-namespace document the classifier finds must be
    // dispatched and loaded: `loaded == strict_found` is what turns a
    // silently skipped document into a failure instead of a smaller
    // passing number.
    assert_eq!(
        loaded,
        strict_found,
        "loaded {loaded} of {strict_found} strict-namespace documents found under {}; \
         every document classified into a strict namespace must be loaded, \
         a silently skipped one is the defect this guard exists to catch; \
         failures:\n{}",
        root.display(),
        failures.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{} schema document(s) failed to load through the production loaders:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Regression pin (review B1): the two ELSFP pages that begin with a
    // YAML document marker (`---`) are strict-namespace documents and must
    // be loaded, not skipped by a test-local classifier that gives up on
    // the first non-`schema:` line while production keeps scanning.
    for rel in [
        "elsfp-1.0/page-1ah-lane-mgmt.yaml",
        "elsfp-1.0/page-1bh-lane-controls-monitors.yaml",
    ] {
        let path = root.join(rel);
        assert!(
            loaded_paths.contains(&path),
            "{} must be loaded by the guard: a leading `---` marker is not a \
             reason to skip a schema document",
            path.display()
        );
    }
}

// ── guard: every schema YAML file belongs to a covered bucket ──────────────

#[test]
fn every_schema_yaml_file_is_in_a_covered_bucket() {
    let root = schema_root();
    let files = walk_yaml(&root);

    // Document namespaces covered by strict production loaders.
    let covered_namespaces = [
        "openxcvr.page.v1",
        "openxcvr.page.v2",
        "openxcvr.enum.v2",
        "openxcvr.state_machine.v1",
        "openxcvr.state_machine.v2",
        "openxcvr.cdb_command.v1",
        "openxcvr.tom_message.v1",
        "openxcvr.calibration.v1",
    ];

    let mut docs = Vec::new();
    let mut indexes = Vec::new();
    let mut fragments = Vec::new();
    let mut uncovered = Vec::new();

    for path in &files {
        if path.file_name().is_some_and(|n| n == "_index.yaml") {
            indexes.push(path.clone());
        } else if let Some(kind) = probe_schema_kind(path) {
            if covered_namespaces.contains(&kind.as_str()) {
                docs.push(path.clone());
            } else {
                uncovered.push((path.clone(), format!("uncovered namespace: {kind}")));
            }
        } else {
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let is_fragment = content
                .lines()
                .any(|l| l.starts_with("fields:") || l.starts_with("entries:"));
            if is_fragment {
                fragments.push(path.clone());
            } else {
                uncovered.push((
                    path.clone(),
                    "not an index, known document, or fragment".to_string(),
                ));
            }
        }
    }

    assert!(
        uncovered.is_empty(),
        "{} schema file(s) are not in any covered bucket:\n{}",
        uncovered.len(),
        uncovered
            .iter()
            .map(|(p, reason)| format!("{}: {reason}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(
        files.len(),
        423,
        "census mismatch: expected 423 schema YAML files under {}",
        root.display()
    );
    assert_eq!(docs.len(), 289, "expected 289 covered schema documents");
    assert_eq!(indexes.len(), 16, "expected 16 index files");
    assert_eq!(fragments.len(), 118, "expected 118 fragments");
    assert_eq!(
        docs.len() + indexes.len() + fragments.len(),
        files.len(),
        "bucket totality failed: docs ({}) + indexes ({}) + fragments ({}) != files ({})",
        docs.len(),
        indexes.len(),
        fragments.len(),
        files.len()
    );
}

// ── guard: the templates of every _index.yaml load, whatever its kind ──────

fn validate_real_index_file(path: &Path) -> anyhow::Result<IndexFile> {
    let parsed = load_index_file(path)?;
    if parsed.kind.is_none() {
        anyhow::bail!("{}: real _index.yaml must declare `kind:`", path.display());
    }
    if parsed.schema.is_none() {
        anyhow::bail!(
            "{}: real _index.yaml must declare `schema:`",
            path.display()
        );
    }
    Ok(parsed)
}

/// Review B3: the templates of **every** `_index.yaml` under `schemas/` are
/// deserialized, whatever the index's `kind:`. The page-load path validates
/// only the nearest owning index of a page, so an index no page lives under
/// was read by nothing — a typo in the `cdb_payload` template of the
/// `cdb_command_registry` at `cmis/cdb-commands/_index.yaml` passed every
/// gate. This guard walks the tree itself (no depth limit, unlike the
/// loader's walker) and validates each index with the same production pass
/// the loader calls, then runs the unfiltered loader over the whole tree so
/// that path is exercised on the real corpus too.
#[test]
fn every_index_template_in_the_tree_loads() {
    let root = schema_root();
    let indexes: Vec<PathBuf> = walk_yaml(&root)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "_index.yaml"))
        .collect();
    assert!(
        !indexes.is_empty(),
        "no _index.yaml found under {}",
        root.display()
    );
    // Floor, not a census: the tree holds 16 today.
    assert!(
        indexes.len() >= 5,
        "only {} _index.yaml files found under {}",
        indexes.len(),
        root.display()
    );
    // Regression pin (review B3): the index no page lives under must be in
    // the validated set — the page-load template pass cannot reach it.
    let cdb_commands = root.join("cmis/cdb-commands/_index.yaml");
    assert!(
        indexes.contains(&cdb_commands),
        "{} must be found by the index walk: an index no page lives under \
         is exactly the one this guard exists to cover",
        cdb_commands.display()
    );
    let mut failures = Vec::new();
    let mut indexes_loaded = 0usize;
    let mut templates_validated = 0usize;
    // Adv-D & Adv-2 pin: every *real* `_index.yaml` declares `kind:` and `schema:`.
    // Enforced here, not in the parser, because test fixtures author their own
    // `_index.yaml` without `kind:` or `schema:` (15 fixture sites) — parser
    // enforcement would break them, and the contract forbids editing fixtures
    // to make this pass. Without the pin a missing `kind:` or `schema:` parses
    // and fails far downstream; the pin moves the failure to the file that lacks it.
    for index in &indexes {
        match validate_real_index_file(index) {
            Ok(_) => indexes_loaded += 1,
            Err(err) => failures.push(format!("{}: {err:#}", index.display())),
        }
        match validate_index_templates(index) {
            Ok(count) => templates_validated += count,
            Err(err) => failures.push(format!("{}: {err:#}", index.display())),
        }
    }
    assert_eq!(
        indexes_loaded,
        indexes.len(),
        "loaded {indexes_loaded} of {} _index.yaml files found under {}; \
         every index document must load strictly through IndexFile; \
         failures:\n{}",
        indexes.len(),
        root.display(),
        failures.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{} index file(s) failed strict loading or template validation:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Floor for the same reason as the document floor: a walk that stops
    // seeing templates must fail instead of passing vacuously (the tree
    // holds 36 today).
    assert!(
        templates_validated >= 30,
        "only {templates_validated} index templates validated under {}",
        root.display()
    );
    // The unfiltered production loader reads every index its walker reaches
    // (all of them today) and now validates each one's templates on that
    // path; it must also read the real tree cleanly.
    IndexLoader::collect_page_schemas(&root, None)
        .unwrap_or_else(|e| panic!("the production loader must read the tree cleanly: {e:#}"));
}

// ── misspelled keys must fail loud and be named ─────────────────────────────

/// A field-level `acess:` for `access:` inside a page document.
#[test]
fn rejects_misspelled_page_field_key() {
    let bad = format!(
        "{PAGE_HEADER}fields:\n  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    acess: rw\n"
    );
    let good = bad.replace("acess:", "access:");
    let dir = tempfile::tempdir().expect("tempdir");
    let page = dir.path().join("page.yaml");
    std::fs::write(&page, &good).expect("write control page");
    load_page_schema(&page).unwrap_or_else(|e| {
        panic!("control must load: the document is valid apart from the typo under test: {e:#}")
    });
    std::fs::write(&page, &bad).expect("write page");

    let err = load_page_schema(&page).expect_err("`acess:` must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("acess"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("page.yaml"),
        "error must name the document: {msg}"
    );
}

/// A misspelled key inside an `_index.yaml` template: the error must also
/// name the template and its `_index.yaml`, or the author cannot find the
/// defect (MetadataTemplate is only reachable through the template path).
#[test]
fn rejects_misspelled_template_key_naming_the_template_and_its_index() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("_index.yaml"),
        "kind: memory_map\ntemplates:\n  base:\n    acess: rw\n",
    )
    .expect("write index");
    let pages = dir.path().join("pages");
    std::fs::create_dir_all(&pages).expect("mkdir pages");
    let page = pages.join("p.yaml");
    std::fs::write(
        &page,
        format!(
            "{PAGE_HEADER}fields:\n  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    template: base\n"
        ),
    )
    .expect("write page");

    let err = load_page_schema(&page)
        .expect_err("a misspelled template key must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("acess"),
        "error must name the misspelled key: {msg}"
    );
    assert!(msg.contains("base"), "error must name the template: {msg}");
    assert!(
        msg.contains("_index.yaml"),
        "error must name the template's _index.yaml: {msg}"
    );
}

/// An entry-level `descripton:` for `description:` inside an enum document.
#[test]
fn rejects_misspelled_enum_entry_key() {
    let bad = format!("{ENUM_HEADER}entries:\n  - value: 1\n    name: a\n    descripton: typo\n");
    let good = bad.replace("descripton:", "description:");
    let dir = tempfile::tempdir().expect("tempdir");
    let enum_path = dir.path().join("enum.yaml");
    std::fs::write(&enum_path, &good).expect("write control enum");
    load_enum_schema(&enum_path).unwrap_or_else(|e| {
        panic!("control must load: the document is valid apart from the typo under test: {e:#}")
    });
    std::fs::write(&enum_path, &bad).expect("write enum");

    let err = load_enum_schema(&enum_path)
        .expect_err("`descripton:` must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("descripton"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("enum.yaml"),
        "error must name the document: {msg}"
    );
}

// ── fragment wrappers: the one key gives them must be exact ────────

/// A field fragment whose wrapper key is misspelled (`fieldz:` for
/// `fields:`). gives a fragment exactly one key, and a typo there
/// silently discards the whole fragment's fields with every gate green, so
/// the wrapper must deny unknown keys too.
#[test]
fn rejects_misspelled_fragment_wrapper_key_in_field_include() {
    let dir = tempfile::tempdir().expect("tempdir");
    let page = dir.path().join("page.yaml");
    let fragment = dir.path().join("part.yaml");
    let fragment_body = "  - name: g\n    offset: 1\n    size_bytes: 1\n    type: u8\n";
    // Control: the correctly spelled wrapper loads, and the fragment's
    // field really lands in the page (2 fields: inline + included).
    std::fs::write(&fragment, format!("fields:\n{fragment_body}")).expect("write control fragment");
    std::fs::write(
        &page,
        format!(
            "{PAGE_HEADER}field_includes:\n  - part.yaml\nfields:\n  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n"
        ),
    )
    .expect("write control page");
    let schema = load_page_schema(&page).unwrap_or_else(|e| {
        panic!("control must load: the document is valid apart from the typo under test: {e:#}")
    });
    assert_eq!(
        schema.fields.len(),
        2,
        "control must load the inline field and the fragment's field"
    );
    // The typo: `fieldz:` instead of `fields:`.
    std::fs::write(&fragment, format!("fieldz:\n{fragment_body}")).expect("rewrite fragment");

    let err =
        load_page_schema(&page).expect_err("`fieldz:` must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("fieldz"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("part.yaml"),
        "error must name the fragment file: {msg}"
    );
}

/// An entry fragment whose wrapper key is misspelled (`entriez:` for
/// `entries:`): same contract as the field fragment above.
#[test]
fn rejects_misspelled_fragment_wrapper_key_in_entry_include() {
    let dir = tempfile::tempdir().expect("tempdir");
    let enum_path = dir.path().join("enum.yaml");
    let fragment = dir.path().join("part.yaml");
    let fragment_body = "  - value: 2\n    name: b\n";
    // Control: the correctly spelled wrapper loads, and the fragment's
    // entry really lands in the enum (2 entries: inline + included).
    std::fs::write(&fragment, format!("entries:\n{fragment_body}"))
        .expect("write control fragment");
    std::fs::write(
        &enum_path,
        format!(
            "{ENUM_HEADER}entry_includes:\n  - part.yaml\nentries:\n  - value: 1\n    name: a\n"
        ),
    )
    .expect("write control enum");
    let schema = load_enum_schema(&enum_path).unwrap_or_else(|e| {
        panic!("control must load: the document is valid apart from the typo under test: {e:#}")
    });
    assert_eq!(
        schema.entries.len(),
        2,
        "control must load the inline entry and the fragment's entry"
    );
    // The typo: `entriez:` instead of `entries:`.
    std::fs::write(&fragment, format!("entriez:\n{fragment_body}")).expect("rewrite fragment");

    let err = load_enum_schema(&enum_path)
        .expect_err("`entriez:` must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("entriez"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("part.yaml"),
        "error must name the fragment file: {msg}"
    );
}

// ── templates: every template in an _index.yaml is validated ───────────────

/// A typo in an `_index.yaml` template that no field references must still
/// be rejected: the template is declared vocabulary whether or not a field
/// currently inherits it, and an unused template's defect must fail loud
/// now instead of the day a field starts referencing it.
#[test]
fn rejects_typo_in_unreferenced_index_template() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("_index.yaml"),
        "kind: memory_map\ntemplates:\n  used:\n    access: rw\n  unused:\n    acess: rw\n",
    )
    .expect("write index");
    let pages = dir.path().join("pages");
    std::fs::create_dir_all(&pages).expect("mkdir pages");
    let page = pages.join("p.yaml");
    std::fs::write(
        &page,
        format!(
            "{PAGE_HEADER}fields:\n  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    template: used\n"
        ),
    )
    .expect("write page");

    let err = load_page_schema(&page)
        .expect_err("a typo in an unreferenced template must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("acess"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("unused"),
        "error must name the template no field references: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the template's _index.yaml: {msg}"
    );
}

// ── cdb_command_registry: an index no page lives under is still gated ───────

/// The real `schemas/cmis/cdb-commands/_index.yaml`: a `cdb_command_registry`
/// whose templates no page can inherit, because no page lives under that
/// directory. It is the tree's one index the page-load template pass never
/// reads (review B3), so the loader itself must gate its templates.
fn cdb_commands_index() -> PathBuf {
    schema_root().join("cmis/cdb-commands/_index.yaml")
}

/// Rewrite the real cdb-commands index into `dir`, misspelling the first
/// `access:` inside `template` as `acess:`. The fixture mirrors the corpus
/// file exactly because it is the corpus file, read at run time — it cannot
/// drift from what the schemas tree actually declares.
fn write_cdb_commands_index_with_typo(dir: &Path, template: &str) -> PathBuf {
    let content =
        std::fs::read_to_string(cdb_commands_index()).expect("read the real cdb-commands index");
    let marker = format!("  {template}:");
    let (head, tail) = content
        .split_once(&marker)
        .unwrap_or_else(|| panic!("{template} must be a template of the real cdb-commands index"));
    let index = dir.join("_index.yaml");
    std::fs::write(
        &index,
        format!("{head}{marker}{}", tail.replacen("access:", "acess:", 1)),
    )
    .expect("write index");
    index
}

/// Review B3's named artifact: a typo in the unreferenced `cdb_payload`
/// template must fail the production loader, naming the template and the
/// index file. Before this round nothing read this index at all, so the
/// typo passed every gate.
#[test]
fn rejects_typo_in_cdb_command_registry_cdb_payload_template() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Control: the real index, unmodified, must load through the loader.
    std::fs::copy(cdb_commands_index(), dir.path().join("_index.yaml"))
        .expect("copy the real cdb-commands index");
    IndexLoader::collect_page_schemas(dir.path(), None).unwrap_or_else(|e| {
        panic!("control must load: the real cdb-commands index is valid: {e:#}")
    });
    // The typo: `acess:` in `cdb_payload`, a template no field references.
    write_cdb_commands_index_with_typo(dir.path(), "cdb_payload");

    let err = IndexLoader::collect_page_schemas(dir.path(), None)
        .expect_err("a typo in the unreferenced cdb_payload template must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("acess"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("cdb_payload"),
        "error must name the template: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

/// Same contract for `cdb_cmd_parameter`, the registry's other parameter
/// template: every template in the index is gated, not only the one the
/// review named.
#[test]
fn rejects_typo_in_cdb_command_registry_cdb_cmd_parameter_template() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Control: the real index, unmodified, must load through the loader.
    std::fs::copy(cdb_commands_index(), dir.path().join("_index.yaml"))
        .expect("copy the real cdb-commands index");
    IndexLoader::collect_page_schemas(dir.path(), None).unwrap_or_else(|e| {
        panic!("control must load: the real cdb-commands index is valid: {e:#}")
    });
    // The typo: `acess:` in `cdb_cmd_parameter`.
    write_cdb_commands_index_with_typo(dir.path(), "cdb_cmd_parameter");

    let err = IndexLoader::collect_page_schemas(dir.path(), None)
        .expect_err("a typo in the cdb_cmd_parameter template must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("acess"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("cdb_cmd_parameter"),
        "error must name the template: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

// ── flattened FieldMetadata keys: typo rejected, valid key still lands ─────

/// `emit_valeu:` — a typo of the `FieldMetadata` key `emit_value` flattened
/// into `PageField` — must be rejected, while a correctly spelled
/// `emit_value:` still populates the field's metadata. Pins that
/// `deny_unknown_fields` and `#[serde(flatten)]` coexist on `PageField`
/// rather than the deny swallowing the flattened vocabulary.
#[test]
fn rejects_misspelled_flattened_metadata_key_on_page_field() {
    let bad = format!(
        "{PAGE_HEADER}fields:\n  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    emit_valeu: false\n"
    );
    let good = bad.replace("emit_valeu:", "emit_value:");
    let dir = tempfile::tempdir().expect("tempdir");
    let page = dir.path().join("page.yaml");
    std::fs::write(&page, &good).expect("write control page");
    let schema = load_page_schema(&page).unwrap_or_else(|e| {
        panic!("control must load: the document is valid apart from the typo under test: {e:#}")
    });
    assert_eq!(
        schema.fields[0].metadata.emit_value,
        Some(false),
        "a valid emit_value must populate the flattened metadata"
    );
    std::fs::write(&page, &bad).expect("write page");

    let err =
        load_page_schema(&page).expect_err("`emit_valeu:` must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("emit_valeu"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("page.yaml"),
        "error must name the document: {msg}"
    );
}

/// The same flattened-key typo on an `_index.yaml` template must be
/// rejected, and a correctly spelled `emit_value:` on the template must
/// still populate the inheriting field's metadata.
#[test]
fn rejects_misspelled_flattened_metadata_key_on_index_template() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pages = dir.path().join("pages");
    std::fs::create_dir_all(&pages).expect("mkdir pages");
    let page = pages.join("p.yaml");
    std::fs::write(
        &page,
        format!(
            "{PAGE_HEADER}fields:\n  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    template: base\n"
        ),
    )
    .expect("write page");

    // Control: a valid `emit_value:` on the template populates the
    // inheriting field's metadata.
    std::fs::write(
        dir.path().join("_index.yaml"),
        "kind: memory_map\ntemplates:\n  base:\n    emit_value: false\n",
    )
    .expect("write control index");
    let schema = load_page_schema(&page).unwrap_or_else(|e| {
        panic!("control must load: the document is valid apart from the typo under test: {e:#}")
    });
    assert_eq!(
        schema.fields[0].metadata.emit_value,
        Some(false),
        "a valid template emit_value must populate the inheriting field's metadata"
    );

    // The typo: `emit_valeu:` on the template.
    std::fs::write(
        dir.path().join("_index.yaml"),
        "kind: memory_map\ntemplates:\n  base:\n    emit_valeu: false\n",
    )
    .expect("rewrite index");
    let err = load_page_schema(&page)
        .expect_err("a template's `emit_valeu:` must be rejected, not silently dropped");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("emit_valeu"),
        "error must name the misspelled key: {msg}"
    );
    assert!(msg.contains("base"), "error must name the template: {msg}");
    assert!(
        msg.contains("_index.yaml"),
        "error must name the template's _index.yaml: {msg}"
    );
}

// ── strict _index.yaml vocabulary ──────────────────────────────────────────

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn setup_cmis_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let cmis_src = schema_root().join("cmis");
    let sff8024_src = schema_root().join("sff-8024");
    copy_dir_all(&cmis_src, &dir.path().join("cmis")).expect("copy cmis");
    copy_dir_all(&sff8024_src, &dir.path().join("sff-8024")).expect("copy sff-8024");
    let cmis_index = dir.path().join("cmis/_index.yaml");
    (dir, cmis_index)
}

/// A misspelled top-level `templates:` key (`templatez:`) in `schemas/cmis/_index.yaml`.
/// Review Adv-2 / PR #99 follow-up: previously `templatez:` silently passed every gate
/// and dropped 3 templates, mutating 128 lines of generated catalog code.
#[test]
fn rejects_misspelled_templates_key_in_index() {
    let (dir, cmis_index) = setup_cmis_fixture();
    // Control: the unmodified real cmis schemas load cleanly.
    IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .unwrap_or_else(|e| panic!("control must load: unmodified cmis index is valid: {e:#}"));

    // The typo: `templates:` renamed `templatez:` in schemas/cmis/_index.yaml.
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = content.replace("templates:", "templatez:");
    assert_ne!(content, bad, "index must declare templates:");
    std::fs::write(&cmis_index, bad).expect("write index");

    let err = IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .expect_err("`templatez:` in _index.yaml must fail, not silently drop templates");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("templatez"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

/// A typo inside `spec:` (`tytle:` for `title:`) in `schemas/cmis/_index.yaml`.
#[test]
fn rejects_misspelled_spec_key_in_index() {
    let (dir, cmis_index) = setup_cmis_fixture();
    // Control: the unmodified real cmis schemas load cleanly.
    IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .unwrap_or_else(|e| panic!("control must load: unmodified cmis index is valid: {e:#}"));

    // The typo: `title:` renamed `tytle:` inside spec:.
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = content.replace("title:", "tytle:");
    assert_ne!(content, bad, "index must declare title:");
    std::fs::write(&cmis_index, bad).expect("write index");

    let err = IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .expect_err("`tytle:` in _index.yaml spec must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("tytle"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

/// An unknown top-level key in `schemas/cmis/_index.yaml`.
#[test]
fn rejects_unknown_top_level_key_in_index() {
    let (dir, cmis_index) = setup_cmis_fixture();
    // Control: the unmodified real cmis schemas load cleanly.
    IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .unwrap_or_else(|e| panic!("control must load: unmodified cmis index is valid: {e:#}"));

    // The typo: unknown top-level key.
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = format!("unknown_top_level_key: true\n{content}");
    std::fs::write(&cmis_index, bad).expect("write index");

    let err = IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .expect_err("unknown top-level key in _index.yaml must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown_top_level_key"),
        "error must name the unknown key: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

/// An unknown kind value in `schemas/cmis/_index.yaml`.
#[test]
fn rejects_unknown_kind_value_in_index() {
    let (dir, cmis_index) = setup_cmis_fixture();
    // Control: the unmodified real cmis schemas load cleanly.
    IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .unwrap_or_else(|e| panic!("control must load: unmodified cmis index is valid: {e:#}"));

    // The typo: unknown kind value.
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = content.replace("kind: memory_map", "kind: unknown_kind");
    assert_ne!(content, bad, "index must declare kind: memory_map");
    std::fs::write(&cmis_index, bad).expect("write index");

    let err = IndexLoader::collect_page_schemas(dir.path(), Some("cmis"))
        .expect_err("unknown kind value in _index.yaml must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown_kind") || msg.contains("kind"),
        "error must name the unknown kind or key: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

// ── one strict index reader: the page-load path and the namespace value ─────

/// Adv-C: the page-load path must read the nearest `_index.yaml` through
/// the same strict reader (`load_index_file`) the index loader and the
/// markdown emitter use, not a private lenient `Value` parse. With
/// `templates:` misspelled as `templatez:` in the nearest index,
/// `load_page_schema` used to return Ok and silently drop every template
/// the page's fields inherit; it must fail loud naming the key and index.
#[test]
fn load_page_schema_rejects_misspelled_templates_key_in_the_nearest_index() {
    let (dir, cmis_index) = setup_cmis_fixture();
    // Control: the unmodified real cmis page loads and really inherits from
    // the index's templates — `access: ro` comes from `vdm_descriptor_field`,
    // the page's templated fields declare no access of their own.
    let page = dir.path().join("cmis/page-20h-vdm-descriptors-1-64.yaml");
    let control = load_page_schema(&page).unwrap_or_else(|e| {
        panic!(
            "control must load: the real cmis page inherits a template from the real index: {e:#}"
        )
    });
    assert!(
        control
            .fields
            .iter()
            .any(|f| f.access == Some(openxcvr_codegen::ir::pages::Access::Ro)),
        "control must inherit `access: ro` from the index's vdm_descriptor_field template"
    );

    // The typo: `templates:` renamed `templatez:` in the nearest index.
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = content.replace("templates:", "templatez:");
    assert_ne!(content, bad, "index must declare templates:");
    std::fs::write(&cmis_index, bad).expect("write index");

    let err = load_page_schema(&page).expect_err(
        "`templatez:` in the nearest index must fail the page load, not silently drop templates",
    );
    let msg = format!("{err:#}");
    assert!(
        msg.contains("templatez"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

/// Adv-B: the index `schema:` namespace must be checked against the declared
/// index namespaces, as page documents are checked against
/// `ALLOWED_PAGE_SCHEMAS`. `openxcvr.schema_index.v2` in the real cmis index
/// must be rejected naming the value and the file; it used to parse as a
/// free-form `Option<String>` that nothing consumes.
#[test]
fn rejects_unknown_index_schema_namespace() {
    let (_dir, cmis_index) = setup_cmis_fixture();
    // Control: the unmodified real cmis index loads through the strict reader.
    load_index_file(&cmis_index)
        .unwrap_or_else(|e| panic!("control must load: the real cmis index is valid: {e:#}"));

    // The typo: schema: openxcvr.schema_index.v1 → openxcvr.schema_index.v2.
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = content.replace("openxcvr.schema_index.v1", "openxcvr.schema_index.v2");
    assert_ne!(content, bad, "index must declare openxcvr.schema_index.v1");
    std::fs::write(&cmis_index, bad).expect("write index");

    let err = load_index_file(&cmis_index)
        .expect_err("an unknown index schema namespace must fail, not parse as free-form text");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("openxcvr.schema_index.v2"),
        "error must name the unknown namespace value: {msg}"
    );
    assert!(
        msg.contains("_index.yaml"),
        "error must name the index file: {msg}"
    );
}

// ── state-machine rejection tests ─────────────────────────────────────────

/// An unknown top-level key in a copy of the real module state machine
/// (`schemas/cmis/state-machines/module-state-machine.yaml`) must be
/// rejected by `load_state_machine_schema`: the v2-typed parser keeps the
/// `deny_unknown_fields` discipline the strict vocabulary guard owns, so a
/// key outside the declared v1/v2 vocabulary fails loud naming the key and
/// the document instead of being silently dropped.
#[test]
fn rejects_unknown_key_in_state_machine() {
    let src = schema_root().join("cmis/state-machines/module-state-machine.yaml");
    let content = std::fs::read_to_string(&src).expect("read state machine");
    let dir = tempfile::tempdir().expect("tempdir");
    let machine = dir.path().join("machine.yaml");
    std::fs::write(&machine, &content).expect("write control machine");
    load_state_machine_schema(&machine).unwrap_or_else(|e| {
        panic!("control must load: the real state-machine document is valid: {e:#}")
    });

    let bad = format!("unknown_top_level_key: true\n{content}");
    std::fs::write(&machine, bad).expect("write bad machine");
    let err = load_state_machine_schema(&machine)
        .expect_err("unknown top-level key in state_machine must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown_top_level_key"),
        "error must name the unknown key: {msg}"
    );
    assert!(
        msg.contains("machine.yaml"),
        "error must name the document file: {msg}"
    );
}

// ── cdb_command, tom_message, and calibration rejection tests ──────────────

#[test]
fn rejects_unknown_top_level_key_in_cdb_command() {
    let src = schema_root().join("cmis/cdb-commands/0040-module-features.yaml");
    let content = std::fs::read_to_string(&src).expect("read cdb command");
    let dir = tempfile::tempdir().expect("tempdir");
    let cmd_file = dir.path().join("cmd.yaml");
    std::fs::write(&cmd_file, &content).expect("write control cmd");
    load_cdb_command_schema(&cmd_file).unwrap_or_else(|e| {
        panic!("control must load: the real cdb command document is valid: {e:#}")
    });

    let bad = format!("unknown_top_level_key: true\n{content}");
    std::fs::write(&cmd_file, bad).expect("write bad cmd");
    let err = load_cdb_command_schema(&cmd_file)
        .expect_err("unknown top-level key in cdb_command must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown_top_level_key"),
        "error must name the unknown key: {msg}"
    );
    assert!(
        msg.contains("cmd.yaml"),
        "error must name the document file: {msg}"
    );
}

#[test]
fn rejects_unknown_field_key_in_cdb_command() {
    let src = schema_root().join("cmis/cdb-commands/0040-module-features.yaml");
    let content = std::fs::read_to_string(&src).expect("read cdb command");
    let dir = tempfile::tempdir().expect("tempdir");
    let cmd_file = dir.path().join("cmd.yaml");
    std::fs::write(&cmd_file, &content).expect("write control cmd");
    load_cdb_command_schema(&cmd_file).unwrap_or_else(|e| {
        panic!("control must load: the real cdb command document is valid: {e:#}")
    });

    let bad = content.replace("offset: 136", "offfset: 136");
    assert_ne!(content, bad, "document must declare offset: 136");
    std::fs::write(&cmd_file, bad).expect("write bad cmd");
    let err = load_cdb_command_schema(&cmd_file)
        .expect_err("misspelled field key in cdb_command must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("offfset"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("cmd.yaml"),
        "error must name the document file: {msg}"
    );
}

#[test]
fn rejects_unknown_top_level_key_in_tom_message() {
    let src = schema_root().join("sff-8472/tom-messages/tom-2a0h-ctrl-cmd-rsp.yaml");
    let content = std::fs::read_to_string(&src).expect("read tom message");
    let dir = tempfile::tempdir().expect("tempdir");
    let tom_file = dir.path().join("tom.yaml");
    std::fs::write(&tom_file, &content).expect("write control tom");
    load_tom_message_schema(&tom_file).unwrap_or_else(|e| {
        panic!("control must load: the real tom message document is valid: {e:#}")
    });

    let bad = format!("unknown_top_level_key: true\n{content}");
    std::fs::write(&tom_file, bad).expect("write bad tom");
    let err = load_tom_message_schema(&tom_file)
        .expect_err("unknown top-level key in tom_message must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown_top_level_key"),
        "error must name the unknown key: {msg}"
    );
    assert!(
        msg.contains("tom.yaml"),
        "error must name the document file: {msg}"
    );
}

#[test]
fn rejects_unknown_field_key_in_tom_message() {
    let src = schema_root().join("sff-8472/tom-messages/tom-2a0h-ctrl-cmd-rsp.yaml");
    let content = std::fs::read_to_string(&src).expect("read tom message");
    let dir = tempfile::tempdir().expect("tempdir");
    let tom_file = dir.path().join("tom.yaml");
    std::fs::write(&tom_file, &content).expect("write control tom");
    load_tom_message_schema(&tom_file).unwrap_or_else(|e| {
        panic!("control must load: the real tom message document is valid: {e:#}")
    });

    let bad = content.replace("offset_bits: 0", "offfset_bits: 0");
    assert_ne!(content, bad, "document must declare offset_bits: 0");
    std::fs::write(&tom_file, bad).expect("write bad tom");
    let err = load_tom_message_schema(&tom_file)
        .expect_err("misspelled field key in tom_message must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("offfset_bits"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("tom.yaml"),
        "error must name the document file: {msg}"
    );
}

#[test]
fn rejects_unknown_top_level_key_in_calibration() {
    let src = schema_root().join("sff-8472/calibration-internal-vs-external.yaml");
    let content = std::fs::read_to_string(&src).expect("read calibration");
    let dir = tempfile::tempdir().expect("tempdir");
    let cal_file = dir.path().join("cal.yaml");
    std::fs::write(&cal_file, &content).expect("write control cal");
    load_calibration_schema(&cal_file).unwrap_or_else(|e| {
        panic!("control must load: the real calibration document is valid: {e:#}")
    });

    let bad = format!("unknown_top_level_key: true\n{content}");
    std::fs::write(&cal_file, bad).expect("write bad cal");
    let err = load_calibration_schema(&cal_file)
        .expect_err("unknown top-level key in calibration must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown_top_level_key"),
        "error must name the unknown key: {msg}"
    );
    assert!(
        msg.contains("cal.yaml"),
        "error must name the document file: {msg}"
    );
}

#[test]
fn rejects_unknown_nested_key_in_calibration() {
    let src = schema_root().join("sff-8472/calibration-internal-vs-external.yaml");
    let content = std::fs::read_to_string(&src).expect("read calibration");
    let dir = tempfile::tempdir().expect("tempdir");
    let cal_file = dir.path().join("cal.yaml");
    std::fs::write(&cal_file, &content).expect("write control cal");
    load_calibration_schema(&cal_file).unwrap_or_else(|e| {
        panic!("control must load: the real calibration document is valid: {e:#}")
    });

    let bad = content.replace("page_address: A0h", "page_addrez: A0h");
    assert_ne!(content, bad, "document must declare page_address: A0h");
    std::fs::write(&cal_file, bad).expect("write bad cal");
    let err = load_calibration_schema(&cal_file)
        .expect_err("misspelled key in calibration must fail, not pass silently");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("page_addrez"),
        "error must name the misspelled key: {msg}"
    );
    assert!(
        msg.contains("cal.yaml"),
        "error must name the document file: {msg}"
    );
}

// ── PR #101 Adv-2: schema: presence pin on real _index.yaml files ──────────

#[test]
fn real_index_schema_pin_catches_missing_schema() {
    let (_dir, cmis_index) = setup_cmis_fixture();
    let content = std::fs::read_to_string(&cmis_index).expect("read index");
    let bad = content
        .lines()
        .filter(|l| !l.trim_start().starts_with("schema:"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(content, bad, "index must declare schema:");
    std::fs::write(&cmis_index, bad).expect("write index");

    // Deleting `schema:` must be caught by the real-index validation pin!
    let err = validate_real_index_file(&cmis_index)
        .expect_err("deleting `schema:` from a copy of a real _index.yaml must be caught by the schema presence pin");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("schema"),
        "error must name missing schema: {msg}"
    );
}
