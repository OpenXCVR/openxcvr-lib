//! Empty windows must fail validation even when occupancy checks do not apply.
//!
//! Enum-side coverage (Task 1 of the schema-owned DPSM repair): strict
//! presence for entry revision metadata, central rejection of invalid
//! metadata through `validate_enums`, unchanged raw-union rules, entry
//! resolution over the known family domain, and the real CLI gate.
use std::collections::BTreeSet;

use openxcvr_codegen::ir::enums::{EntryValue, EnumEntry, EnumSchema};
use openxcvr_codegen::ir::version::{applicable_revs_for_entry, resolve_enum_entries_for_rev, Rev};
use openxcvr_codegen::parser::enums::load_enum_schema;
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};
use openxcvr_codegen::validator::{validate_enums, validate_pages};

/// Build a minimal enum schema YAML. `spec` `None` omits the spec block.
fn enum_yaml(entries: &str, spec: Option<&str>) -> String {
    let spec_block = match spec {
        Some(id) => format!("spec:\n  id: {id}\n  revision: \"5.3\"\n"),
        None => String::new(),
    };
    format!(
        "schema: openxcvr.enum.v2\nkind: enum\nid: MetaEnum\ntitle: Meta Enum\n{spec_block}entries:\n{entries}\n"
    )
}

/// Load an enum schema through the real file loader (the same path the CLI
/// and the markdown probe use), from a throwaway temp directory.
fn load_enum(entries: &str, spec: Option<&str>) -> anyhow::Result<EnumSchema> {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("enum.yaml");
    std::fs::write(&path, enum_yaml(entries, spec)).unwrap();
    load_enum_schema(&path)
}

/// Assert that a schema deserializes cleanly and is then rejected by the
/// central `validate_enums` gate, with every context token present in the
/// diagnostic (schema / entry / key / value, as applicable).
fn expect_gate_rejection(entries: &str, spec: Option<&str>, context: &[&str]) {
    let schema = load_enum(entries, spec)
        .unwrap_or_else(|e| panic!("schema must deserialize for gate coverage: {e:#}"));
    let err = validate_enums(&[schema]).expect_err("validate_enums must reject this metadata");
    let diagnostic = format!("{err:#}");
    for token in context {
        assert!(
            diagnostic.contains(token),
            "missing {token:?}: {diagnostic}"
        );
    }
}

fn page(shape: &str, window: &str) -> String {
    format!(
        "schema: openxcvr.page.v2\nkind: page\nid: WindowPage\ntitle: Window\npage_address: lower\nsize_bytes: 128\nfields:\n  - name: window_field\n    offset: 0\n{shape}\n{window}\n"
    )
}

fn reject_window(shape: &str, window: &str, endpoints: &[&str]) {
    let schema = parse_page_schema(&page(shape, window)).unwrap();
    // A single field: no colliding neighbor is needed to make a window invalid.
    let err = validate_pages(&[schema]).expect_err("empty revision window must be rejected");
    let diagnostic = err.to_string();
    for context in [
        "WindowPage",
        "window_field",
        "empty revision window",
        "since",
        "until",
    ]
    .into_iter()
    .chain(endpoints.iter().copied())
    {
        assert!(
            diagnostic.contains(context),
            "missing {context}: {diagnostic}"
        );
    }
}

#[test]
fn inverted_window_is_rejected_without_neighbor() {
    reject_window(
        "    type: u8",
        "    since: '5.3'\n    until: '4.0'",
        &["5.3", "4.0"],
    );
}

#[test]
fn equal_endpoints_are_rejected_without_neighbor() {
    reject_window(
        "    type: u8",
        "    since: '4.0'\n    until: '4.0'",
        &["4.0"],
    );
}

#[test]
fn until_first_revision_is_rejected_without_neighbor() {
    reject_window("    type: u8", "    until: '3.0'", &["None", "3.0"]);
}

// Removing the guard, or putting it inside the collision-only branch, must fail
// these tests. All shapes have valid addressing and retain their exemptions.
macro_rules! exempt_shape {
    ($name:ident, $shape:expr) => {
        #[test]
        fn $name() {
            for (window, endpoints) in [
                ("    since: '5.3'\n    until: '4.0'", vec!["5.3", "4.0"]),
                ("    since: '4.0'\n    until: '4.0'", vec!["4.0"]),
                ("    until: '3.0'", vec!["None", "3.0"]),
            ] {
                reject_window($shape, window, &endpoints);
            }
            validate_pages(&[parse_page_schema(&page($shape, "")).unwrap()]).unwrap();
        }
    };
}
exempt_shape!(
    conditional_empty_window_is_rejected,
    "    type: u8\n    present_when: enabled"
);
exempt_shape!(
    variant_empty_window_is_rejected,
    "    type: u8\n    variants: []"
);
exempt_shape!(bitmap_empty_window_is_rejected, "    type: bitmap");
exempt_shape!(
    composite_empty_window_is_rejected,
    "    type: composite\n    size_bytes: 1"
);

#[test]
fn unknown_window_endpoints_fail_in_parser() {
    for key in ["since", "until"] {
        for value in ["v4.0", "40", "5.x", "6.0"] {
            let err = parse_page_schema(&page("    type: u8", &format!("    {key}: '{value}'")))
                .expect_err("unknown revision must not reach validation");
            assert!(err.to_string().contains(value), "{err:#}");
        }
    }
}

fn collect_regular_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if dir.exists() {
        for entry in walkdir::WalkDir::new(dir) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                files.push(entry.into_path());
            }
        }
    }
    files.sort();
    files
}

#[test]
fn generate_rejects_empty_window_before_emitting_pages() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = tmp.path().join("schemas");
    std::fs::create_dir(&schemas).unwrap();
    std::fs::write(
        schemas.join("_index.yaml"),
        "kind: memory_map\nincludes: [enum.yaml, page.yaml]\n",
    )
    .unwrap();
    std::fs::write(schemas.join("enum.yaml"), "schema: openxcvr.enum.v2\nkind: enum\nid: DummyEnum\ntitle: Dummy\nentries:\n  - { value: 0, name: zero }\n").unwrap();
    let schema_path = schemas.join("page.yaml");
    std::fs::write(
        &schema_path,
        page("    type: u8", "    since: '5.3'\n    until: '4.0'"),
    )
    .unwrap();
    // Exercise the filesystem loader as well as the in-memory parser above.
    load_page_schema(&schema_path).unwrap();
    let out = tmp.path().join("fresh-output");
    assert!(!out.exists());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"))
        .arg("generate")
        .arg("--schemas")
        .arg(&schemas)
        .arg("--out")
        .arg(&out)
        .arg("--lang")
        .arg("rust")
        .output()
        .unwrap();
    assert!(!output.status.success(), "malformed schema accepted");
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    for context in [
        "WindowPage",
        "window_field",
        "empty revision window",
        "since",
        "5.3",
        "until",
        "4.0",
    ] {
        assert!(
            diagnostic.contains(context),
            "missing {context}: {diagnostic}"
        );
    }
    // Validation must fail before emitting any code files; empty directories
    // are acceptable, but emitted code on validation failure is not.
    let files = collect_regular_files(&out);
    assert!(
        files.is_empty(),
        "expected zero generated code files in fresh destination on validation failure, found: {files:?}"
    );
}

#[test]
fn generate_emits_both_enums_and_pages_on_valid_input() {
    let tmp = tempfile::tempdir().unwrap();
    let schemas = tmp.path().join("schemas");
    std::fs::create_dir(&schemas).unwrap();
    std::fs::write(
        schemas.join("_index.yaml"),
        "kind: memory_map\nincludes: [enum.yaml, page.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        schemas.join("enum.yaml"),
        "schema: openxcvr.enum.v2\nkind: enum\nid: DummyEnum\ntitle: Dummy\nentries:\n  - { value: 0, name: zero }\n",
    )
    .unwrap();
    let schema_path = schemas.join("page.yaml");
    std::fs::write(
        &schema_path,
        page("    type: u8", "    since: '4.0'\n    until: '5.0'"),
    )
    .unwrap();
    load_page_schema(&schema_path).unwrap();
    let out = tmp.path().join("fresh-output");
    assert!(!out.exists());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"))
        .arg("generate")
        .arg("--schemas")
        .arg(&schemas)
        .arg("--out")
        .arg(&out)
        .arg("--lang")
        .arg("rust")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "valid schemas should generate successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let files = collect_regular_files(&out);
    let file_names: Vec<_> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(
        file_names.contains(&"DummyEnum.rs".to_string()),
        "expected DummyEnum.rs in output, found: {file_names:?}"
    );
    assert!(
        file_names.contains(&"WindowPage.rs".to_string()),
        "expected WindowPage.rs in output, found: {file_names:?}"
    );
}

// ─── Enum revision metadata (Task 1) ────────────────────────────────────────

/// Presence is strict for `since`/`until`: omission is `None`, a quoted known
/// string is accepted, and any present non-string — an explicit `null`, an
/// empty YAML value, a number, a bool, an array or a map — is a
/// deserialization error, never a silently unannotated entry. Entry-include
/// fragments pass through the same strict deserialization.
#[test]
fn enum_metadata_presence_is_strict() {
    let schema = load_enum("  - { value: 0, name: zero }\n", None).unwrap();
    assert!(schema.entries[0].since.is_none());
    assert!(schema.entries[0].until.is_none());

    let schema = load_enum(
        "  - { value: 0, name: zero, since: '4.0', until: '5.0' }\n",
        Some("cmis"),
    )
    .unwrap();
    assert_eq!(schema.entries[0].since.as_deref(), Some("4.0"));
    assert_eq!(schema.entries[0].until.as_deref(), Some("5.0"));

    for key in ["since", "until"] {
        for raw in [
            "null", "~", // explicit null
            "4.0", "4", "5.3", // unquoted numbers
            "true", "false", // bools
            "[a]", "{k: v}", // array and map
        ] {
            let yaml = format!("  - value: 0\n    name: zero\n    {key}: {raw}\n");
            let err = load_enum(&yaml, Some("cmis"))
                .expect_err("present non-string metadata must fail deserialization");
            let diagnostic = format!("{err:#}");
            assert!(
                diagnostic.contains("revision string"),
                "present {key}: {raw} must be a deserialization error: {diagnostic}"
            );
        }
        // An empty YAML value (`since:` with nothing after it) is an explicit
        // null and must be rejected the same way, not defaulted to omission.
        let yaml = format!("  - value: 0\n    name: zero\n    {key}:\n");
        let err = load_enum(&yaml, Some("cmis")).expect_err("empty YAML value must not load");
        assert!(
            format!("{err:#}").contains("revision string"),
            "empty value for {key}: {err:#}"
        );
    }

    // Entry-include fragments go through the same strict deserialization: a
    // number in an included file is a load error, not a silent default.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("entries")).unwrap();
    std::fs::write(
        tmp.path().join("entries/part.yaml"),
        "entries:\n  - value: 1\n    name: included_value\n    since: 4\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("enum.yaml"),
        "schema: openxcvr.enum.v2\nkind: enum\nid: MetaEnum\ntitle: Meta Enum\nspec:\n  id: cmis\nentry_includes:\n  - entries/part.yaml\nentries:\n  - value: 0\n    name: inline_value\n",
    )
    .unwrap();
    let err = load_enum_schema(&tmp.path().join("enum.yaml"))
        .expect_err("include-loaded entries must be presence-strict too");
    assert!(
        format!("{err:#}").contains("revision string"),
        "include strictness: {err:#}"
    );
}

/// Gate coverage: unknown/typo/empty revision strings pass string
/// deserialization and are rejected by `validate_enums` with
/// schema/entry/key/value context; equal, inverted and empty-intersection
/// windows are rejected; entries merged from `entry_includes` are gated too.
#[test]
fn enum_revision_metadata_rejects_invalid_windows() {
    for (key, value) in [
        ("since", "5.5"),
        ("since", "3.1"),
        ("since", "4"),
        ("since", "v4.0"),
        ("until", "5.5"),
        ("until", "v5.0"),
        ("since", ""),
        ("until", ""),
    ] {
        let entries = format!("  - value: 0\n    name: zero\n    {key}: '{value}'\n");
        let mut context = vec!["MetaEnum", "zero", key];
        if !value.is_empty() {
            context.push(value);
        }
        expect_gate_rejection(&entries, Some("cmis"), &context);
    }

    expect_gate_rejection(
        "  - value: 0\n    name: zero\n    since: '4.0'\n    until: '4.0'\n",
        Some("cmis"),
        &[
            "MetaEnum",
            "zero",
            "empty revision window",
            "since",
            "until",
            "4.0",
        ],
    );
    expect_gate_rejection(
        "  - value: 0\n    name: zero\n    since: '5.3'\n    until: '4.0'\n",
        Some("cmis"),
        &["MetaEnum", "zero", "empty revision window", "5.3", "4.0"],
    );
    expect_gate_rejection(
        "  - value: 0\n    name: zero\n    until: '3.0'\n",
        Some("cmis"),
        &["MetaEnum", "zero", "empty revision window", "3.0"],
    );

    // Entries merged from entry_includes are gated through the same checks.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("entries")).unwrap();
    std::fs::write(
        tmp.path().join("entries/part.yaml"),
        "entries:\n  - value: 1\n    name: included_value\n    since: '5.3'\n    until: '4.0'\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("enum.yaml"),
        "schema: openxcvr.enum.v2\nkind: enum\nid: MetaEnum\ntitle: Meta Enum\nspec:\n  id: cmis\nentry_includes:\n  - entries/part.yaml\nentries:\n  - value: 0\n    name: inline_value\n",
    )
    .unwrap();
    let schema = load_enum_schema(&tmp.path().join("enum.yaml")).unwrap();
    let err =
        validate_enums(&[schema]).expect_err("include-merged invalid window must be rejected");
    let diagnostic = format!("{err:#}");
    for token in [
        "MetaEnum",
        "included_value",
        "empty revision window",
        "5.3",
        "4.0",
    ] {
        assert!(diagnostic.contains(token), "missing {token}: {diagnostic}");
    }
}

/// Unsupported enum vocabulary is rejected whenever it is present —
/// null, empty value, empty string/list/map or an ordinary nonempty value —
/// with the not-implemented diagnostic; a present null is preserved as
/// presence rather than collapsed to omission.
#[test]
fn enum_unsupported_metadata_is_rejected_when_present() {
    for key in ["changed_in", "renamed_from", "aliased_to"] {
        for raw in [
            "null",
            "~",
            "''",
            "[]",
            "{}",
            "{ rev: '4.0', old_name: former }",
        ] {
            let entries = format!("  - value: 0\n    name: zero\n    {key}: {raw}\n");
            expect_gate_rejection(
                &entries,
                Some("cmis"),
                &[
                    "MetaEnum",
                    "zero",
                    key,
                    "declared but not implemented; only since/until are interpreted",
                ],
            );
        }
    }

    // Presence-preserving deserialization: an explicit null on an unsupported
    // key stays `Some` so the validator can reject it. A naive `Option<Value>`
    // would collapse it to `None` and pass as unannotated.
    let schema = load_enum(
        "  - value: 0\n    name: zero\n    changed_in: null\n",
        Some("cmis"),
    )
    .unwrap();
    assert_ne!(
        format!("{:?}", schema.entries[0].changed_in),
        "None",
        "present null must be preserved as presence"
    );
    let schema = load_enum(
        "  - value: 0\n    name: zero\n    renamed_from:\n",
        Some("cmis"),
    )
    .unwrap();
    assert_ne!(
        format!("{:?}", schema.entries[0].renamed_from),
        "None",
        "empty YAML value must be preserved as presence"
    );
}

/// Entry revision metadata requires a spec family with a modeled revision
/// domain (`cmis`, `sff-8636`, `sff-8472`): a schema whose `spec.id` is
/// missing or names another namespace is rejected when any entry metadata
/// (including unsupported markers) is present. A family schema whose window
/// names another family's revisions is rejected by the empty-window rule —
/// a cross-family rev string never silently matches the family domain.
#[test]
fn enum_revision_metadata_requires_family_spec() {
    for spec in [Some("sff-8024"), None] {
        expect_gate_rejection(
            "  - value: 0\n    name: zero\n    since: '4.0'\n",
            spec,
            &["MetaEnum", "zero", "spec.id cmis | sff-8636 | sff-8472"],
        );
    }
    // A CMIS revision string on an SFF-8636 schema is parseable but matches
    // no revision of the SFF-8636 domain: empty window, rejected.
    expect_gate_rejection(
        "  - value: 0\n    name: zero\n    since: '4.0'\n",
        Some("sff-8636"),
        &["MetaEnum", "zero", "empty revision window"],
    );
    // Unsupported markers count as revision metadata for the family requirement.
    let schema = load_enum(
        "  - value: 0\n    name: zero\n    changed_in: { rev: '4.0' }\n",
        Some("sff-8636"),
    )
    .unwrap();
    assert!(
        validate_enums(&[schema]).is_err(),
        "SFF schema with an unsupported marker must be rejected"
    );

    // Plain SFF enums without metadata stay valid, SFF enums with a window
    // inside their own family domain are valid, and a CMIS enum with a
    // valid window validates cleanly through the same gate.
    let sff = load_enum("  - { value: 0, name: zero }\n", Some("sff-8636")).unwrap();
    validate_enums(&[sff]).expect("SFF enum without revision metadata is valid");
    let sff_window = load_enum(
        "  - { value: 0, name: zero, since: '2.5' }\n  - { value: 1, name: one }\n",
        Some("sff-8636"),
    )
    .unwrap();
    validate_enums(&[sff_window]).expect("SFF-8636 enum with an in-family window is valid");
    let cmis = load_enum(
        "  - { value: 0, name: zero, since: '4.0' }\n  - { value: 1, name: one }\n",
        Some("cmis"),
    )
    .unwrap();
    validate_enums(&[cmis]).expect("CMIS enum with a valid window is valid");
}

/// The global raw-union rules are unchanged and revision-blind: duplicate
/// schema ids, duplicate entry names, duplicate singles (even with disjoint
/// revision windows), single/range and range/range overlaps all still reject;
/// an ordinary unknown entry key is rejected at parse time by the strict
/// vocabulary (naming the key), and is not confused with the three captured
/// unsupported keys, which parse and are rejected by the gate instead.
#[test]
fn enum_global_name_and_overlap_rules_are_unchanged() {
    // Duplicate schema id across two schemas.
    let a = load_enum("  - { value: 0, name: zero }\n", Some("cmis")).unwrap();
    let b = load_enum("  - { value: 1, name: one }\n", Some("cmis")).unwrap();
    let err = validate_enums(&[a, b]).expect_err("duplicate enum id must be rejected");
    assert!(format!("{err:#}").contains("duplicate enum id MetaEnum"));

    // Duplicate entry name.
    expect_gate_rejection(
        "  - { value: 0, name: twin }\n  - { value: 1, name: twin }\n",
        Some("cmis"),
        &["duplicate enum entry name", "twin"],
    );

    // Duplicate single value.
    expect_gate_rejection(
        "  - { value: 3, name: first }\n  - { value: 3, name: second }\n",
        Some("cmis"),
        &["overlaps enum entry"],
    );

    // Revision-disjoint duplicates prove there is no revision-aware
    // relaxation: `until: '5.0'` ({3.0, 4.0}) and `since: '5.0'`
    // ({5.0, 5.1, 5.2, 5.3}) never overlap, yet the raw union rule still
    // rejects the shared value.
    expect_gate_rejection(
        "  - { value: 3, name: early, until: '5.0' }\n  - { value: 3, name: late, since: '5.0' }\n",
        Some("cmis"),
        &["overlaps enum entry"],
    );

    // Single inside a range.
    expect_gate_rejection(
        "  - { value: 6, name: inside }\n  - { value: 5..7, name: around }\n",
        Some("cmis"),
        &["overlaps enum entry"],
    );

    // Overlapping ranges.
    expect_gate_rejection(
        "  - { value: 0..3, name: low }\n  - { value: 2..5, name: high }\n",
        Some("cmis"),
        &["overlaps enum entry"],
    );

    // An unrelated unknown entry key is rejected at parse time by the strict
    // vocabulary (`deny_unknown_fields` on `EnumEntry`), naming the key: it
    // is not confused with the three specifically captured unsupported keys
    // (changed_in / renamed_from / aliased_to), which deserialize into
    // presence-preserving fields and are rejected by the `validate_enums`
    // gate instead (covered above and in `enum_revision_metadata_requires_cmis`).
    let err = load_enum(
        "  - { value: 0, name: zero, not_a_revision_key: { any: thing } }\n",
        Some("cmis"),
    )
    .unwrap_err();
    let diagnostic = format!("{err:#}");
    assert!(
        diagnostic.contains("not_a_revision_key"),
        "parse error must name the unknown entry key: {diagnostic}"
    );
    assert!(
        diagnostic.contains("unknown field"),
        "parse error must say it is an unknown field: {diagnostic}"
    );
}

/// Entry resolution is bounded by the supplied known-revision slice:
/// unannotated entries apply only within it (never as a claim about a
/// revision outside it), `since: '4.0'` spans the five later catalog
/// revisions, a `since`/`until` window intersects it exactly, and
/// named-entry resolution never returns range entries.
#[test]
fn enum_entry_resolution_uses_only_supplied_known_revisions() {
    fn entry(name: &str, value: EntryValue, since: Option<&str>, until: Option<&str>) -> EnumEntry {
        EnumEntry {
            name: name.to_string(),
            value,
            description: None,
            aliases: vec![],
            tags: vec![],
            signaling: None,
            reach: None,
            bit_rate_gbps: None,
            vendor_specific: None,
            since: since.map(str::to_string),
            until: until.map(str::to_string),
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
            provenance: None,
        }
    }

    let unannotated = entry("plain", EntryValue::Single(1), None, None);
    let all: BTreeSet<Rev> = Rev::CMIS_ALL.iter().copied().collect();
    assert_eq!(applicable_revs_for_entry(&unannotated, Rev::CMIS_ALL), all);
    // Only the supplied slice — never a claim about revisions outside it.
    assert_eq!(
        applicable_revs_for_entry(&unannotated, &[Rev::V4_0]),
        [Rev::V4_0].into_iter().collect::<BTreeSet<_>>()
    );
    // Empty slice → empty set, no panic.
    assert!(applicable_revs_for_entry(&unannotated, &[]).is_empty());

    let since4 = entry("later", EntryValue::Single(2), Some("4.0"), None);
    let five_later: BTreeSet<Rev> = [
        Rev::V4_0,
        Rev::V5_0,
        Rev::V5_1,
        Rev::V5_2,
        Rev::V5_3,
        Rev::V5_4,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        applicable_revs_for_entry(&since4, Rev::CMIS_ALL),
        five_later
    );

    let windowed = entry("windowed", EntryValue::Single(3), Some("4.0"), Some("5.0"));
    assert_eq!(
        applicable_revs_for_entry(&windowed, Rev::CMIS_ALL),
        [Rev::V4_0].into_iter().collect::<BTreeSet<_>>()
    );

    // Named-entry resolution returns applicable singles only; range entries
    // stay folded into the Unknown<n> catch-all and are never named states.
    let schema = EnumSchema {
        schema: "openxcvr.enum.v2".to_string(),
        kind: "enum".to_string(),
        id: "MetaEnum".to_string(),
        title: "Meta Enum".to_string(),
        entry_includes: vec![],
        entries: vec![
            unannotated,
            since4,
            windowed,
            entry(
                "reserved_8_f",
                EntryValue::Range { start: 8, end: 15 },
                None,
                None,
            ),
        ],
        spec: None,
        provenance: None,
        description: None,
    };
    let names = |rev: Rev| {
        resolve_enum_entries_for_rev(&schema, rev)
            .into_iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(Rev::V3_0), vec!["plain"]);
    assert_eq!(names(Rev::V4_0), vec!["plain", "later", "windowed"]);
    assert_eq!(names(Rev::V5_3), vec!["plain", "later"]);
    for rev in Rev::CMIS_ALL {
        assert!(
            !names(*rev).contains(&"reserved_8_f"),
            "range entries must never resolve as named states"
        );
    }
}

/// The real CLI rejects invalid enum revision metadata before emitting any
/// output — for both code and Markdown generation — with descriptive
/// schema/entry/key diagnostics, no panic, and zero emitted files.
#[test]
fn generate_rejects_invalid_enum_metadata_before_emission() {
    let cases: &[(&str, &[&str])] = &[
        // Malformed revision string (deserializes as a string; the gate rejects it).
        (
            "    since: '5.5'",
            &["MetaEnum", "dp_tx_turn_on", "since", "5.5"],
        ),
        // Invalid window.
        (
            "    since: '5.3'\n    until: '4.0'",
            &[
                "MetaEnum",
                "dp_tx_turn_on",
                "empty revision window",
                "5.3",
                "4.0",
            ],
        ),
        // Unsupported vocabulary key.
        (
            "    renamed_from: { rev: '4.0', old_name: former }",
            &[
                "MetaEnum",
                "dp_tx_turn_on",
                "renamed_from",
                "declared but not implemented; only since/until are interpreted",
            ],
        ),
    ];
    for (metadata, expected) in cases {
        for lang in ["rust", "markdown"] {
            let tmp = tempfile::tempdir().unwrap();
            let schemas = tmp.path().join("schemas");
            std::fs::create_dir(&schemas).unwrap();
            std::fs::write(
                schemas.join("_index.yaml"),
                "kind: memory_map\nincludes: [enum.yaml]\n",
            )
            .unwrap();
            std::fs::write(
                schemas.join("enum.yaml"),
                format!(
                    "schema: openxcvr.enum.v2\nkind: enum\nid: MetaEnum\ntitle: Meta Enum\nspec:\n  id: cmis\nentries:\n  - value: 0x5\n    name: dp_tx_turn_on\n{metadata}\n  - value: 0x1\n    name: dp_deactivated\n"
                ),
            )
            .unwrap();
            let out = tmp.path().join("fresh-output");
            assert!(!out.exists());
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"))
                .arg("generate")
                .arg("--schemas")
                .arg(&schemas)
                .arg("--out")
                .arg(&out)
                .arg("--lang")
                .arg(lang)
                .output()
                .unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr).to_string();
            assert!(
                !output.status.success(),
                "invalid enum metadata must fail generation ({lang}):\n{diagnostic}"
            );
            assert!(
                !diagnostic.contains("panicked"),
                "no panic allowed ({lang}): {diagnostic}"
            );
            for token in *expected {
                assert!(
                    diagnostic.contains(token),
                    "missing {token:?} for lang={lang}: {diagnostic}"
                );
            }
            // Validation must fail before emitting anything; precreated empty
            // directories are acceptable, emitted files are not.
            let files = collect_regular_files(&out);
            assert!(
                files.is_empty(),
                "expected zero emitted files for lang={lang}, found: {files:?}"
            );
        }
    }
}
