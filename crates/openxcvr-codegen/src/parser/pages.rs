use crate::ir::pages::{
    Access, FieldFlags, FieldMetadata, PageField, PageSchema, ReadEffectMetadata, ReadEffectScope,
};
use crate::ir::version::resolve_field_for_rev;
use crate::ir::version::Rev;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// Recognised page schema namespace strings.
/// Exported so that `validator/mod.rs` can share this constant rather than
/// duplicating the list.  Keep this list in sync with the `#[error]` string
/// below (see Finding #4 — `// keep in sync with ALLOWED_PAGE_SCHEMAS`).
pub(crate) const ALLOWED_PAGE_SCHEMAS: &[&str] = &["openxcvr.page.v1", "openxcvr.page.v2"];

#[derive(Debug, thiserror::Error)]
pub enum ParsePageError {
    #[error("field {field:?}: read_effect none requires nonempty read_effect_provenance")]
    UncitedReadEffectNone { field: String },
    #[error("field {field:?}: contradictory read-effect assertions")]
    ConflictingReadEffect { field: String },
    #[error("YAML parse error: {source}")]
    Yaml {
        #[from]
        source: serde_norway::Error,
    },
    // keep in sync with ALLOWED_PAGE_SCHEMAS
    #[error(
        "unsupported schema namespace: {namespace}; allowed: openxcvr.page.v1, openxcvr.page.v2"
    )]
    UnsupportedNamespace { namespace: String },
    /// A `since`, `until`, `changed_in[].rev`, `renamed_from.rev`, or
    /// `aliased_to[].rev` string is not a valid family revision
    /// (CMIS 3.0-5.4, SFF-8636 1.3-2.12, SFF-8472 9.3-12.5; intermediate
    /// published revisions snap to the nearest admitted revision).
    /// § "Alternatives Considered" mandates fail-loud, not silent
    /// fallthrough.
    #[error(
        "field {field:?}: invalid rev string {value:?} in {key:?}; \
         valid values: CMIS 3.0, 4.0, 5.0, 5.1, 5.2, 5.3, 5.4; \
         SFF-8636 1.3, 1.4, 1.5, 2.0, 2.5, 2.8, 2.11, 2.12; \
         SFF-8472 9.3, 9.5, 10.2, 10.4, 11.0, 11.3, 11.4, 12.3, 12.4, 12.5"
    )]
    InvalidRevString {
        field: String,
        key: &'static str,
        value: String,
    },
    /// A malformed `content_switch` declaration on a field.
    #[error("field {field:?}: malformed content_switch: {reason}")]
    ContentSwitch { field: String, reason: String },
}

/// Validate a field's `content_switch` declaration (B2): rev strings parse,
/// settling names are known, the data range is a sane inclusive window
/// range, and the bounds are non-empty and ascending.
fn validate_content_switch(field: &crate::ir::pages::PageField) -> Result<(), ParsePageError> {
    use crate::ir::pages::ContentSwitchDesc;
    let Some(switch) = &field.content_switch else {
        return Ok(());
    };
    let invalid = |reason: String| ParsePageError::ContentSwitch {
        field: field.name.clone(),
        reason,
    };
    let ContentSwitchDesc {
        data_range: [start, end],
        bound,
    } = switch;
    if end < start {
        return Err(invalid(format!(
            "data_range [{start}, {end}] is not ascending"
        )));
    }
    if *end > 255 {
        return Err(invalid(format!(
            "data_range end {end} is outside the 256-byte page window"
        )));
    }
    if bound.is_empty() {
        return Err(invalid(
            "bound must declare at least one settling entry".into(),
        ));
    }
    let mut previous: Option<crate::ir::version::Rev> = None;
    for entry in bound {
        let rev = crate::ir::version::Rev::parse(&entry.since).map_err(|_| {
            ParsePageError::InvalidRevString {
                field: field.name.clone(),
                key: "content_switch.bound[].since",
                value: entry.since.clone(),
            }
        })?;
        if let Some(previous) = previous {
            if rev <= previous {
                return Err(invalid(format!(
                    "bound revisions must be ascending, got {} after {}",
                    entry.since,
                    previous.as_str()
                )));
            }
        }
        previous = Some(rev);
        match entry.settling.as_str() {
            "tnack_advertised" | "tddcs" => {}
            other => {
                return Err(invalid(format!(
                    "unknown settling {other:?}; expected tnack_advertised or tddcs"
                )))
            }
        }
    }
    Ok(())
}

/// Validate every rev string in a single field.  Returns the first error found.
fn validate_field_revs(field: &crate::ir::pages::PageField) -> Result<(), ParsePageError> {
    if let Some(s) = &field.since {
        Rev::parse(s).map_err(|_| ParsePageError::InvalidRevString {
            field: field.name.clone(),
            key: "since",
            value: s.clone(),
        })?;
    }
    if let Some(s) = &field.until {
        Rev::parse(s).map_err(|_| ParsePageError::InvalidRevString {
            field: field.name.clone(),
            key: "until",
            value: s.clone(),
        })?;
    }
    if let Some(entries) = &field.changed_in {
        for entry in entries {
            Rev::parse(&entry.rev).map_err(|_| ParsePageError::InvalidRevString {
                field: field.name.clone(),
                key: "changed_in[].rev",
                value: entry.rev.clone(),
            })?;
        }
    }
    if let Some(entry) = &field.renamed_from {
        Rev::parse(&entry.rev).map_err(|_| ParsePageError::InvalidRevString {
            field: field.name.clone(),
            key: "renamed_from.rev",
            value: entry.rev.clone(),
        })?;
    }
    if let Some(entries) = &field.aliased_to {
        for entry in entries {
            Rev::parse(&entry.rev).map_err(|_| ParsePageError::InvalidRevString {
                field: field.name.clone(),
                key: "aliased_to[].rev",
                value: entry.rev.clone(),
            })?;
        }
    }
    Ok(())
}

/// Run `validate_field_revs` over every field in a slice.  Used by both
/// `parse_page_schema` (validating the parsed YAML body) and
/// `load_page_schema` (validating the merged result after applying
/// `field_includes`).  Returns the first error found.
///
/// § "Alternatives Considered" mandates fail-loud on any rev-string
/// typo regardless of where the field originated; the helper centralises that
/// contract so neither entry point can silently skip it.
fn validate_all_field_revs(fields: &[PageField]) -> Result<(), ParsePageError> {
    for field in fields {
        validate_field_revs(field)?;
        validate_content_switch(field)?;
    }
    Ok(())
}

pub fn parse_page_schema(yaml: &str) -> Result<PageSchema, ParsePageError> {
    let mut parsed: PageSchema = serde_norway::from_str(yaml)?;
    if !ALLOWED_PAGE_SCHEMAS.contains(&parsed.schema.as_str()) {
        return Err(ParsePageError::UnsupportedNamespace {
            namespace: parsed.schema.clone(),
        });
    }
    // Validate all rev strings up front.
    // Only v2 schemas carry version metadata, but the check is harmless on v1
    // since all Option fields will be None.
    capture_setter_policy(yaml, &mut parsed.fields)?;
    prepare_fields(&mut parsed.fields)?;
    Ok(parsed)
}

fn legacy_access_emits_setter(raw: &serde_norway::Value) -> bool {
    matches!(
        raw.get("access").and_then(|v| v.as_str()),
        Some("rw" | "rw_nv" | "rw_triggered" | "rw_v" | "rww")
    )
}

fn capture_revision_setter_policy(
    raw: &serde_norway::Value,
    changes: &mut Option<Vec<crate::ir::pages::ChangedInEntry>>,
) {
    if let (Some(changes), Some(raw_changes)) =
        (changes, raw.get("changed_in").and_then(|v| v.as_sequence()))
    {
        for (entry, raw) in changes.iter_mut().zip(raw_changes) {
            if entry.access.is_some() {
                entry.setter_eligible = Some(legacy_access_emits_setter(raw));
            }
        }
    }
}

fn capture_setter_policy(yaml: &str, fields: &mut [PageField]) -> Result<(), ParsePageError> {
    let value: serde_norway::Value = serde_norway::from_str(yaml)?;
    if let Some(raw_fields) = value.get("fields").and_then(|v| v.as_sequence()) {
        for (field, raw) in fields.iter_mut().zip(raw_fields) {
            field.explicit_access = field.access;
            let ever = field
                .flags
                .as_ref()
                .is_some_and(|f| f.is_ever_writable == Some(true));
            field.setter_eligible = Some(ever || legacy_access_emits_setter(raw));
            capture_revision_setter_policy(raw, &mut field.changed_in);
        }
    }
    Ok(())
}

fn validate_effects(field: &PageField) -> Result<(), ParsePageError> {
    let effects = field.read_effects();
    if effects.contains(&ReadEffectMetadata::None) && effects.len() > 1 {
        return Err(ParsePageError::ConflictingReadEffect {
            field: field.name.clone(),
        });
    }
    if effects.contains(&ReadEffectMetadata::None)
        && !field
            .metadata
            .read_effect_provenance
            .as_ref()
            .is_some_and(|value| match value {
                serde_norway::Value::String(text) => !text.trim().is_empty(),
                serde_norway::Value::Mapping(map) => !map.is_empty(),
                serde_norway::Value::Sequence(items) => !items.is_empty(),
                _ => false,
            })
    {
        return Err(ParsePageError::UncitedReadEffectNone {
            field: field.name.clone(),
        });
    }
    Ok(())
}

fn prepare_fields(fields: &mut [PageField]) -> Result<(), ParsePageError> {
    validate_all_field_revs(fields)?;
    for field in fields.iter_mut() {
        // Capture old output policy before legacy latching/template inheritance.
        if field.setter_eligible.is_none() {
            field.setter_eligible = Some(field.emits_setter());
        }
        field.normalize_legacy_access();
        validate_effects(field)?;
        for rev in Rev::CMIS_ALL {
            validate_effects(&resolve_field_for_rev(field, *rev))?;
        }
    }
    // A physical byte effect cannot coexist with an explicit side-effect-free
    // assertion on another constituent of that same byte, at that revision.
    for rev in Rev::CMIS_ALL {
        let applicable: Vec<_> = fields
            .iter()
            .filter(|f| crate::ir::version::applicable_revs(f, Rev::CMIS_ALL).contains(rev))
            .map(|f| resolve_field_for_rev(f, *rev))
            .collect();
        for effect in &applicable {
            if effect.metadata.read_effect_scope != Some(ReadEffectScope::Byte)
                || !effect
                    .read_effects()
                    .iter()
                    .any(|e| *e != ReadEffectMetadata::None)
            {
                continue;
            }
            let Ok(footprint) = effect.bit_coverage() else {
                continue;
            };
            for other in &applicable {
                if other.metadata.read_effect != Some(ReadEffectMetadata::None) {
                    continue;
                }
                let Ok(bits) = other.bit_coverage() else {
                    continue;
                };
                if bits.keys().any(|byte| footprint.contains_key(byte)) {
                    return Err(ParsePageError::ConflictingReadEffect {
                        field: other.name.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataTemplate {
    #[serde(default)]
    access: Option<Access>,
    #[serde(default)]
    flags: Option<FieldFlags>,
    #[serde(default)]
    behavior: Option<serde_norway::Value>,
    #[serde(default)]
    provenance: Option<serde_norway::Value>,
    #[serde(flatten)]
    metadata: FieldMetadata,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    changed_in: Option<Vec<crate::ir::pages::ChangedInEntry>>,
    // Declared template vocabulary the `_index.yaml` template sets use but
    // template inheritance does not apply: `field_type`, `tags` and `scope`
    // mirror the `PageField` keys of the same names, and applying them to
    // fields would change generated output. Retained (never read yet) so the
    // declared vocabulary is lossless instead of silently dropped — and
    // strict: a template key outside this vocabulary is rejected, which is
    // the only way a template typo becomes visible at all.
    #[allow(dead_code)]
    #[serde(default)]
    field_type: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    tags: Vec<String>,
    #[allow(dead_code)]
    #[serde(default)]
    scope: Option<serde_norway::Value>,
}

/// Deserialize every template in an index as `MetadataTemplate`, even the
/// ones no field references: an unreferenced template is still declared
/// vocabulary in a schema-adjacent document, and a typo in it must fail loud
/// now instead of the day a field starts inheriting it. Referenced templates
/// are deserialized again by the inheritance loop below, where the parsed
/// value is what inheritance consumes. Returns the number of templates
/// deserialized, so a guard can floor its own coverage.
fn deserialize_templates(
    value: &serde_norway::Value,
    index: &Path,
    label: &dyn Fn(&Path) -> String,
) -> Result<usize> {
    let Some(templates) = value.get("templates").and_then(|v| v.as_mapping()) else {
        return Ok(0);
    };
    for (name, template) in templates.iter() {
        let name = name
            .as_str()
            .with_context(|| format!("template names in {} must be strings", label(index)))?;
        serde_norway::from_value::<MetadataTemplate>(template.clone())
            .with_context(|| format!("invalid template {name:?} in {}", label(index)))?;
    }
    Ok(templates.len())
}

/// Validate the `templates:` map of one `_index.yaml`, whatever the index's
/// `kind:` — an error names the template and the index file it lives in.
///
/// A template is declared vocabulary whether or not anything inherits it.
/// The page-load pass (`inherit_templates`) only reaches the nearest owning
/// index of a page, so an index no page lives under — e.g. the
/// `cdb_command_registry` at `schemas/cmis/cdb-commands/_index.yaml` — would
/// otherwise never have its templates checked at all, and a typo there was
/// caught by nothing (review B3). Public so the strict-vocabulary guard
/// validates the whole tree with the same pass the loaders use — one
/// validator, no drift between production and test.
pub fn validate_index_templates(index: &Path) -> Result<usize> {
    validate_index_templates_with_labels(index, &|path| path.display().to_string())
}

pub(crate) fn validate_index_templates_with_labels(
    index: &Path,
    label: &dyn Fn(&Path) -> String,
) -> Result<usize> {
    let content = std::fs::read_to_string(index)
        .with_context(|| format!("failed to read {}", label(index)))?;
    let value: serde_norway::Value = serde_norway::from_str(&content)
        .with_context(|| format!("failed to parse index {}", label(index)))?;
    deserialize_templates(&value, index, label)
}

fn inherit_templates(
    schema: &mut PageSchema,
    path: &Path,
    label: &dyn Fn(&Path) -> String,
) -> Result<()> {
    // The nearest owning index supplies templates. Do not change the emitter's
    // historical policy when previously ignored template metadata is expanded.
    let Some(index) = path
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .map(|dir| dir.join("_index.yaml"))
        .find(|index| index.is_file())
    else {
        return Ok(());
    };
    // One strict index reader (Adv-C): the nearest owning index is loaded by
    // the same `load_index_file` the index loader and the markdown emitter
    // use, replacing this path's private lenient `Value` parse. A typo'd
    // top-level key (`templatez:`) used to drop the templates silently here
    // and return Ok; it now fails loud with the key and index named.
    let index_file = crate::parser::index::load_index_file_with_labels(&index, label)?;
    // Every template in the index — referenced or not — is validated by
    // `load_index_file` itself (`validate_index_templates`); the field loop
    // below then re-reads the templates it inherits from.
    let Some(templates) = &index_file.templates else {
        return Ok(());
    };
    for field in &mut schema.fields {
        let Some(template_name) = field.template.as_ref() else {
            continue;
        };
        let Some(template) = templates.get(template_name.as_str()) else {
            continue;
        };
        let mut inherited: MetadataTemplate = serde_norway::from_value(template.clone())
            .with_context(|| {
                format!(
                    "field {:?}: invalid template {:?} in {}",
                    field.name,
                    template_name,
                    label(&index)
                )
            })?;
        capture_revision_setter_policy(template, &mut inherited.changed_in);
        if field.setter_eligible.is_none() {
            field.setter_eligible = Some(field.emits_setter());
        }
        if field.access.is_none() {
            field.access = inherited.access;
        }
        if let Some(mut flags) = inherited.flags {
            if let Some(explicit) = &field.flags {
                flags.inherit(explicit);
            }
            field.flags = Some(flags);
        }
        if let Some(mut behavior) = inherited.behavior {
            if let Some(explicit) = &field.behavior {
                if let (Some(base), Some(overrides)) =
                    (behavior.as_mapping_mut(), explicit.as_mapping())
                {
                    base.extend(overrides.clone());
                } else {
                    behavior = explicit.clone();
                }
            }
            field.behavior = Some(behavior);
        }
        if field.provenance.is_none() {
            field.provenance = inherited.provenance;
        }
        if field.since.is_none() {
            field.since = inherited.since;
        }
        if field.until.is_none() {
            field.until = inherited.until;
        }
        if let Some(mut changes) = inherited.changed_in {
            // Stable ordering keeps field overrides after template defaults at
            // the same revision. Merge equal-revision entries key-wise before
            // resolving, so access-derived defaults cannot erase explicit flags.
            changes.extend(field.changed_in.take().unwrap_or_default());
            for entry in &changes {
                Rev::parse(&entry.rev).with_context(|| {
                    format!(
                        "field {:?}: invalid changed_in revision {:?}",
                        field.name, entry.rev
                    )
                })?;
            }
            changes.sort_by_key(|e| Rev::parse(&e.rev).expect("validated above"));
            let mut merged: Vec<crate::ir::pages::ChangedInEntry> = Vec::new();
            for entry in changes {
                if let Some(previous) = merged.last_mut().filter(|e| e.rev == entry.rev) {
                    previous.inherit(&entry);
                } else {
                    merged.push(entry);
                }
            }
            field.changed_in = Some(merged);
        }
        let mut metadata = inherited.metadata;
        metadata.inherit(&field.metadata);
        field.metadata = metadata;
    }
    Ok(())
}

/// gives a field fragment exactly one key (`fields:`); an unknown
/// top-level key — a misspelling of `fields:` — would otherwise discard the
/// whole fragment's fields silently with every gate green, so unknown keys
/// are denied on the wrapper too, not only on its elements.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageFieldFragment {
    #[serde(default)]
    fields: Vec<PageField>,
}

pub fn load_page_schema(path: &Path) -> Result<PageSchema> {
    load_page_schema_with_labels(path, &|path| path.display().to_string())
}

// Keep source-label presentation separate from filesystem and schema values.
pub(crate) fn load_page_schema_with_labels(
    path: &Path,
    label: &dyn Fn(&Path) -> String,
) -> Result<PageSchema> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", label(path)))?;
    let mut schema: PageSchema = parse_page_schema(&content)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to parse page schema from {}", label(path)))?;

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    for include in &schema.field_includes {
        let include_path = base_dir.join(include);
        let content = std::fs::read_to_string(&include_path).with_context(|| {
            format!(
                "failed to read field include {} referenced by {}",
                label(&include_path),
                label(path)
            )
        })?;
        let mut fragment: PageFieldFragment =
            serde_norway::from_str(&content).with_context(|| {
                format!(
                    "failed to parse field include {} referenced by {}",
                    label(&include_path),
                    label(path)
                )
            })?;
        capture_setter_policy(&content, &mut fragment.fields)?;
        schema.fields.extend(fragment.fields);
    }

    // Re-run rev validation against the merged field set so that
    // `field_includes` fragments are subject to the same fail-loud contract
    // as the inline body. § "Alternatives Considered".
    inherit_templates(&mut schema, path, label)?;
    prepare_fields(&mut schema.fields)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("rev validation failed for {}", label(path)))?;

    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::{load_page_schema, parse_page_schema};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "openxcvr-codegen-pages-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn load_page_schema_merges_field_includes() {
        let dir = temp_dir("merge");
        fs::create_dir_all(dir.join("fields")).unwrap();
        fs::write(
            dir.join("fields/part.yaml"),
            r#"fields:
  - name: included_field
    offset: 1
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();
        fs::write(
            dir.join("page.yaml"),
            r#"schema: openxcvr.page.v1
kind: page
id: TestPage
title: Test Page
page_address: 00h
field_includes:
  - fields/part.yaml
fields:
  - name: inline_field
    offset: 0
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();

        let schema = load_page_schema(&dir.join("page.yaml")).unwrap();

        assert_eq!(schema.fields.len(), 2);
        assert_eq!(schema.fields[0].name, "inline_field");
        assert_eq!(schema.fields[1].name, "included_field");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_page_schema_reports_missing_field_include() {
        let dir = temp_dir("missing");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("page.yaml"),
            r#"schema: openxcvr.page.v1
kind: page
id: TestPage
title: Test Page
page_address: 00h
field_includes:
  - fields/missing.yaml
fields: []
"#,
        )
        .unwrap();

        let err = load_page_schema(&dir.join("page.yaml")).unwrap_err();

        assert!(err.to_string().contains("failed to read field include"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_unknown_schema_namespace_version() {
        // v3 does not exist; the parser must reject, not accept.
        let yaml = r#"
schema: openxcvr.page.v3
kind: page
id: TestPage
title: T
page_address: "0x00"
"#;
        let err = parse_page_schema(yaml).expect_err("must reject v3");
        let msg = format!("{err}");
        assert!(
            msg.contains("openxcvr.page.v3") || msg.contains("unsupported schema namespace"),
            "expected namespace-version rejection, got: {msg}",
        );
    }

    #[test]
    fn accepts_v1_and_v2_namespaces() {
        let v1 = r#"
schema: openxcvr.page.v1
kind: page
id: TestPage
title: T
page_address: "0x00"
"#;
        parse_page_schema(v1).expect("v1 must parse");

        let v2 = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
"#;
        parse_page_schema(v2).expect("v2 must parse");
    }

    #[test]
    fn parses_field_since_until_changed_in() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: Test
page_address: "0x00"
fields:
  - name: f_old
    offset: 0
    type: u8
    until: "4.0"
  - name: f_new
    offset: 1
    type: u8
    since: "4.0"
  - name: f_changed
    offset: 2
    type: enum_ref
    enum_ref: dpsm-state
    changed_in:
      - rev: "4.0"
        enum_ref: dpsm-state-v4
        note: "4-state to 8-state widening"
"#;
        let parsed = parse_page_schema(yaml).expect("parse");
        assert_eq!(parsed.fields[0].until.as_deref(), Some("4.0"));
        assert_eq!(parsed.fields[1].since.as_deref(), Some("4.0"));
        let ci = parsed.fields[2].changed_in.as_ref().expect("changed_in");
        assert_eq!(ci.len(), 1);
        assert_eq!(ci[0].rev, "4.0");
        assert_eq!(ci[0].enum_ref.as_deref(), Some("dpsm-state-v4"));
    }

    /// fail-loud: a typo in `since:` (e.g. "v4.0" instead of "4.0")
    /// must be rejected at parse time, not silently produce wrong output.
    #[test]
    fn rejects_invalid_since_rev_string() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
fields:
  - name: bad_field
    offset: 0
    type: u8
    since: "v4.0"
"#;
        let err = parse_page_schema(yaml).expect_err("must reject 'v4.0' as since value");
        let msg = format!("{err}");
        assert!(
            msg.contains("v4.0") || msg.contains("invalid rev"),
            "expected invalid-rev error mentioning 'v4.0', got: {msg}"
        );
    }

    /// fail-loud: a typo in `changed_in[].rev` must also be rejected.
    #[test]
    fn rejects_invalid_changed_in_rev_string() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
fields:
  - name: f
    offset: 0
    type: u8
    changed_in:
      - rev: "40"
        note: "typo"
"#;
        let err = parse_page_schema(yaml).expect_err("must reject '40' as changed_in rev");
        let msg = format!("{err}");
        assert!(
            msg.contains("40") || msg.contains("invalid rev"),
            "expected invalid-rev error, got: {msg}"
        );
    }

    /// fail-loud: a typo in `renamed_from.rev` must be rejected.
    #[test]
    fn rejects_invalid_renamed_from_rev_string() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
fields:
  - name: new_field
    offset: 0
    type: u8
    renamed_from:
      rev: "5.x"
      old_name: old_field
"#;
        let err = parse_page_schema(yaml).expect_err("must reject '5.x' as renamed_from rev");
        let msg = format!("{err}");
        assert!(
            msg.contains("5.x") || msg.contains("invalid rev"),
            "expected invalid-rev error, got: {msg}"
        );
    }

    /// fail-loud (production-path): `since: "40"` in a
    /// `field_includes` fragment must be rejected by `load_page_schema`.
    #[test]
    fn load_page_schema_rejects_invalid_since_in_field_includes() {
        let dir = temp_dir("bad-since");
        fs::create_dir_all(dir.join("fields")).unwrap();
        fs::write(
            dir.join("fields/part.yaml"),
            r#"fields:
  - name: bad_since_field
    offset: 1
    size_bytes: 1
    type: u8
    since: "40"
"#,
        )
        .unwrap();
        fs::write(
            dir.join("page.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: TestPage
title: Test Page
page_address: 00h
field_includes:
  - fields/part.yaml
fields:
  - name: inline_field
    offset: 0
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();

        let err = load_page_schema(&dir.join("page.yaml"))
            .expect_err("must reject invalid since: \"40\" in field_includes fragment");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid rev string"),
            "expected 'invalid rev string', got: {msg}"
        );
        assert!(msg.contains("40"), "expected '40' in error, got: {msg}");
        assert!(
            msg.contains("since"),
            "expected 'since' in error, got: {msg}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    /// fail-loud (production-path): `changed_in[].rev: "4_0"` in a
    /// `field_includes` fragment must be rejected by `load_page_schema`.
    #[test]
    fn load_page_schema_rejects_invalid_changed_in_rev_in_field_includes() {
        let dir = temp_dir("bad-changed-in");
        fs::create_dir_all(dir.join("fields")).unwrap();
        fs::write(
            dir.join("fields/part.yaml"),
            r#"fields:
  - name: bad_changed_in_field
    offset: 1
    size_bytes: 1
    type: enum_ref
    enum_ref: dpsm-state
    changed_in:
      - rev: "4_0"
        enum_ref: dpsm-state-v4
"#,
        )
        .unwrap();
        fs::write(
            dir.join("page.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: TestPage
title: Test Page
page_address: 00h
field_includes:
  - fields/part.yaml
fields:
  - name: inline_field
    offset: 0
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();

        let err = load_page_schema(&dir.join("page.yaml"))
            .expect_err("must reject invalid changed_in.rev: \"4_0\" in field_includes fragment");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid rev string"),
            "expected 'invalid rev string', got: {msg}"
        );
        assert!(msg.contains("4_0"), "expected '4_0' in error, got: {msg}");
        assert!(
            msg.contains("changed_in[].rev"),
            "expected 'changed_in[].rev' in error, got: {msg}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    /// fail-loud (production-path): `renamed_from.rev: "5x1"` in a
    /// `field_includes` fragment must be rejected by `load_page_schema`.
    #[test]
    fn load_page_schema_rejects_invalid_renamed_from_rev_in_field_includes() {
        let dir = temp_dir("bad-renamed-from");
        fs::create_dir_all(dir.join("fields")).unwrap();
        fs::write(
            dir.join("fields/part.yaml"),
            r#"fields:
  - name: bad_renamed_from_field
    offset: 1
    size_bytes: 1
    type: u8
    renamed_from:
      rev: "5x1"
      old_name: legacy_name
"#,
        )
        .unwrap();
        fs::write(
            dir.join("page.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: TestPage
title: Test Page
page_address: 00h
field_includes:
  - fields/part.yaml
fields:
  - name: inline_field
    offset: 0
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();

        let err = load_page_schema(&dir.join("page.yaml"))
            .expect_err("must reject invalid renamed_from.rev: \"5x1\" in field_includes fragment");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid rev string"),
            "expected 'invalid rev string', got: {msg}"
        );
        assert!(msg.contains("5x1"), "expected '5x1' in error, got: {msg}");
        assert!(
            msg.contains("renamed_from.rev"),
            "expected 'renamed_from.rev' in error, got: {msg}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    /// fail-loud (production-path): `aliased_to[].rev: "6.0"` (unknown
    /// revision) in a `field_includes` fragment must be rejected.
    #[test]
    fn load_page_schema_rejects_invalid_aliased_to_rev_in_field_includes() {
        let dir = temp_dir("bad-aliased-to");
        fs::create_dir_all(dir.join("fields")).unwrap();
        fs::write(
            dir.join("fields/part.yaml"),
            r#"fields:
  - name: bad_aliased_to_field
    offset: 1
    size_bytes: 1
    type: u8
    aliased_to:
      - rev: "6.0"
        new_name: future_name
"#,
        )
        .unwrap();
        fs::write(
            dir.join("page.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: TestPage
title: Test Page
page_address: 00h
field_includes:
  - fields/part.yaml
fields:
  - name: inline_field
    offset: 0
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();

        let err = load_page_schema(&dir.join("page.yaml"))
            .expect_err("must reject unknown aliased_to.rev: \"6.0\" in field_includes fragment");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid rev string"),
            "expected 'invalid rev string', got: {msg}"
        );
        assert!(msg.contains("6.0"), "expected '6.0' in error, got: {msg}");
        assert!(
            msg.contains("aliased_to[].rev"),
            "expected 'aliased_to[].rev' in error, got: {msg}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    /// Field-level units are recorded under two spellings in the schema
    /// corpus: `unit:` (cmis, sff families) and `units:` (elsfp-1.0). The IR
    /// must deserialize both into `PageField::unit` — dropping the plural
    /// spelling would silently lose recorded units.
    #[test]
    fn parses_unit_singular_spelling() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
fields:
  - name: f
    offset: 0
    type: u16be
    unit: "0.1 ms"
"#;
        let parsed = parse_page_schema(yaml).expect("parse");
        assert_eq!(parsed.fields[0].unit.as_deref(), Some("0.1 ms"));
    }

    #[test]
    fn parses_unit_plural_alias_spelling() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: "0x00"
fields:
  - name: f
    offset: 0
    type: u16be
    units: "10 uW/LSB"
"#;
        let parsed = parse_page_schema(yaml).expect("parse");
        assert_eq!(
            parsed.fields[0].unit.as_deref(),
            Some("10 uW/LSB"),
            "the plural `units:` spelling must deserialize into PageField::unit"
        );
    }

    /// Page-level `spec:` and `provenance:` blocks must survive into the IR —
    /// the markdown reference emitter cites them.
    #[test]
    fn parses_page_level_spec_and_provenance() {
        let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestPage
title: T
page_address: lower
spec:
  id: cmis
  revision: "5.3"
  errata:
    - revision: "1.0"
      source: docs/stds/cmis/OIF-CMIS-05-Errata-01.0.pdf
provenance:
  spec: CMIS 5.3 § 8.2
  pdf_pages: [141, 142]
fields:
  - name: f
    offset: 0
    type: u8
"#;
        let parsed = parse_page_schema(yaml).expect("parse");
        let spec = parsed.spec.as_ref().expect("spec block captured");
        assert_eq!(spec.id, "cmis");
        assert_eq!(spec.revision.as_deref(), Some("5.3"));
        assert_eq!(spec.errata.len(), 1);
        let prov = parsed.provenance.as_ref().expect("provenance captured");
        assert_eq!(
            prov.get("spec").and_then(|v| v.as_str()),
            Some("CMIS 5.3 § 8.2")
        );
    }
}
