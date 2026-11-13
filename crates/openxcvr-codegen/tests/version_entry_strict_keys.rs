//! Strict version-entry parsing: `changed_in`, `aliased_to` and `renamed_from`
//! entries must reject any key `resolve_field_for_rev` never applies (with the
//! sole exception of the `note:` human annotation on `changed_in`).
//!
//! These entry types historically accepted every key — the flattened
//! `FieldMetadata` silently discarded everything it did not declare — so a
//! schema author could write `encoding:`, `display_name:` or an arbitrary key
//! inside a version entry, get a clean generate, and have the override do
//! nothing. The rejection tests below pin that an unknown key fails loud and
//! names the key; the positive tests pin that every key the resolver *does*
//! apply still parses and still lands on the resolved field, and that `note:`
//! is accepted under `changed_in` without affecting resolution.

use openxcvr_codegen::ir::pages::{
    Access, ChangeSourceMetadata, FieldType, ReadEffectMetadata, ReadEffectScope,
};
use openxcvr_codegen::ir::version::{resolve_field_for_rev, Rev};
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};
use std::path::PathBuf;

fn page(fields: &str) -> String {
    format!(
        "schema: openxcvr.page.v2\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 128\nfields:\n{fields}"
    )
}

/// Parse the page and require rejection, with the offending key named.
fn reject(fields: &str, key: &str) {
    match parse_page_schema(&page(fields)) {
        Ok(_) => panic!(
            "BUG: key `{key}` inside a version entry is silently accepted and \
             dropped by the parser — it must be rejected"
        ),
        Err(err) => {
            let msg = format!("{err}");
            assert!(
                msg.contains(key),
                "parser rejected the schema but the error does not name `{key}`: {msg}"
            );
        }
    }
}

// ── changed_in: unknown keys must fail loud ─────────────────────────────────

/// A field whose `changed_in` entry carries `extra` (one YAML key line).
fn changed_in_field(extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!("        {extra}\n")
    };
    format!(
        "  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    changed_in:\n      - rev: \"5.0\"\n{extra}"
    )
}

#[test]
fn changed_in_rejects_keys_the_resolver_never_applies() {
    // `encoding` and `display_name` exist on `PageField` but not on the
    // version entry — historically the exact trap: a plausible key that the
    // resolver never applies. explicitly specifies `note:` on
    // `changed_in` entries, which is accepted as a human annotation; all other
    // undeclared keys must be rejected.
    for (key, yaml) in [
        ("encoding", "encoding: \"bcd\""),
        ("display_name", "display_name: \"Revision name\""),
        ("totally_bogus_key", "totally_bogus_key: 1"),
    ] {
        reject(&changed_in_field(yaml), key);
    }
}

#[test]
fn changed_in_accepts_note_annotation_without_affecting_resolution() {
    // defines changed_in entries as { rev, enum_ref?, type?, note? }.
    // `note:` is accepted as a human annotation, stored on ChangedInEntry,
    // inherited across template merges, and has no effect on resolution.
    let yaml = "  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    changed_in:\n      - rev: \"5.0\"\n        note: \"4-state widened to 8-state at CMIS 5.0\"\n        type: u16be\n";
    let schema =
        parse_page_schema(&page(yaml)).expect("`note:` inside `changed_in` must be accepted per ");
    assert_eq!(
        schema.fields[0].changed_in.as_ref().unwrap()[0]
            .note
            .as_deref(),
        Some("4-state widened to 8-state at CMIS 5.0")
    );

    let at_v4 = resolve_field_for_rev(&schema.fields[0], Rev::V4_0);
    assert_eq!(at_v4.ty, FieldType::U8);

    let at_v5 = resolve_field_for_rev(&schema.fields[0], Rev::V5_0);
    assert_eq!(at_v5.ty, FieldType::U16Be);
}

// ── aliased_to / renamed_from: unknown keys must fail loud ─────────────────

#[test]
fn aliased_to_rejects_unknown_keys() {
    reject(
        "  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    aliased_to:\n      - { rev: \"5.0\", new_name: f_v5, encoding: \"bcd\" }\n",
        "encoding",
    );
    reject(
        "  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    aliased_to:\n      - { rev: \"5.0\", new_name: f_v5, note: \"not allowed here\" }\n",
        "note",
    );
}

#[test]
fn renamed_from_rejects_unknown_keys() {
    reject(
        "  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    renamed_from: { rev: \"5.0\", old_name: f_v4, display_name: \"nope\" }\n",
        "display_name",
    );
    reject(
        "  - name: f\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    renamed_from: { rev: \"5.0\", old_name: f_v4, note: \"not allowed here\" }\n",
        "note",
    );
}

// ── positive controls: every applied key still accepted and applied ────────

/// One `changed_in` entry carrying every key `resolve_field_for_rev` applies.
const ALL_APPLIED_KEYS: &str = "  - name: f\n    offset: 0\n    size_bytes: 2\n    type: u8\n    access: rw\n    changed_in:\n      - rev: \"5.0\"\n        bit_width: 4\n        enum_inline:\n          - { value: 0, name: zero, description: synthetic }\n        enum_ref: repro-enum\n        type: u16be\n        access: ro\n        flags: { is_dynamic: true }\n        provenance: { section: synthetic }\n        behavior: { reset_domain: module }\n        emit_value: false\n        read_effect: clear_on_read\n        read_effect_scope: field\n        read_effect_provenance: synthetic test authority\n        change_source: dynamic\n        authority: { clause: synthetic authority }\n";

#[test]
fn every_changed_in_key_the_resolver_applies_is_accepted_and_applied() {
    let schema = parse_page_schema(&page(ALL_APPLIED_KEYS))
        .expect("every resolver-applied key must still parse");

    // `rev` gates the whole entry: at 4.0 (before the entry) nothing applies.
    let before = resolve_field_for_rev(&schema.fields[0], Rev::V4_0);
    assert_eq!(before.ty, FieldType::U8);
    assert_eq!(before.bit_width, None);
    assert_eq!(before.access, Some(Access::Rw));
    assert_eq!(before.enum_ref, None);
    assert_eq!(before.metadata.read_effect, None);

    // At 5.3 every key has landed on the resolved field.
    let resolved = resolve_field_for_rev(&schema.fields[0], Rev::V5_3);
    assert_eq!(resolved.ty, FieldType::U16Be, "`type` not applied");
    assert_eq!(resolved.bit_width, Some(4), "`bit_width` not applied");
    assert_eq!(resolved.access, Some(Access::Ro), "`access` not applied");
    assert_eq!(
        resolved.enum_ref.as_deref(),
        Some("repro-enum"),
        "`enum_ref` not applied"
    );
    assert_eq!(
        resolved
            .enum_inline
            .as_ref()
            .and_then(|v| v.as_sequence())
            .and_then(|s| s.first())
            .and_then(|e| e.get("name"))
            .and_then(|n| n.as_str()),
        Some("zero"),
        "`enum_inline` not applied"
    );
    assert_eq!(
        resolved.flags.as_ref().and_then(|f| f.is_dynamic),
        Some(true),
        "`flags` not applied"
    );
    assert_eq!(
        resolved
            .provenance
            .as_ref()
            .and_then(|v| v.get("section"))
            .and_then(|s| s.as_str()),
        Some("synthetic"),
        "`provenance` not applied"
    );
    assert_eq!(
        resolved
            .behavior
            .as_ref()
            .and_then(|v| v.get("reset_domain"))
            .and_then(|d| d.as_str()),
        Some("module"),
        "`behavior` not applied"
    );
    assert_eq!(
        resolved.metadata.emit_value,
        Some(false),
        "`emit_value` not applied"
    );
    assert_eq!(
        resolved.metadata.read_effect,
        Some(ReadEffectMetadata::ClearOnRead),
        "`read_effect` not applied"
    );
    assert_eq!(
        resolved.metadata.read_effect_scope,
        Some(ReadEffectScope::Field),
        "`read_effect_scope` not applied"
    );
    assert_eq!(
        resolved
            .metadata
            .read_effect_provenance
            .as_ref()
            .and_then(|v| v.as_str()),
        Some("synthetic test authority"),
        "`read_effect_provenance` not applied"
    );
    assert_eq!(
        resolved.metadata.change_source,
        Some(ChangeSourceMetadata::Dynamic),
        "`change_source` not applied"
    );
    assert_eq!(
        resolved
            .metadata
            .authority
            .as_ref()
            .and_then(|v| v.get("clause"))
            .and_then(|c| c.as_str()),
        Some("synthetic authority"),
        "`authority` not applied"
    );
}

#[test]
fn aliased_to_and_renamed_from_keys_still_apply() {
    let schema = parse_page_schema(&page(
        "  - name: f_v5\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    renamed_from: { rev: \"5.0\", old_name: f_v4 }\n    aliased_to:\n      - { rev: \"5.1\", new_name: f_alias }\n",
    ))
    .expect("renamed_from/aliased_to keys must still parse");
    // Before the rename: the old name.
    let at_v4 = resolve_field_for_rev(&schema.fields[0], Rev::V4_0);
    assert_eq!(at_v4.name, "f_v4", "`renamed_from.old_name` not applied");
    // At 5.3 the alias (highest entry rev <= 5.3) takes precedence over the
    // rename — the documented `resolve_field_for_rev` precedence.
    let at_v5_3 = resolve_field_for_rev(&schema.fields[0], Rev::V5_3);
    assert_eq!(
        at_v5_3.name, "f_alias",
        "`aliased_to[].new_name` not applied"
    );
}

// ── PASS fixture gate: production loader must accept ok/ fixtures ───────────

#[test]
fn schema_v2_validation_ok_fixtures_pass_production_loader() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ok_dir = manifest_dir.join("../../tests/schema-v2-validation/ok");
    assert!(ok_dir.is_dir(), "missing fixture dir: {}", ok_dir.display());

    let mut page_count = 0;
    for entry in std::fs::read_dir(&ok_dir).expect("read ok dir") {
        let entry = entry.expect("read entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            // As documented in tests/schema-v2-validation/README.md, enum fixtures
            // are named enum-*.yaml and the page fixture is page.yaml.
            // Page fixtures carry `schema: openxcvr.page.v2` / `kind: page`.
            let content = std::fs::read_to_string(&path).expect("read fixture");
            if content.contains("kind: page") {
                load_page_schema(&path).unwrap_or_else(|err| {
                    panic!(
                        "failed to load production page fixture {}: {err:#}",
                        path.display()
                    )
                });
                page_count += 1;
            }
        }
    }
    assert!(
        page_count > 0,
        "no page fixtures found in {}",
        ok_dir.display()
    );
}
