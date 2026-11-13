use crate::parser::pages::ALLOWED_PAGE_SCHEMAS;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Recognised `schema:` namespace strings for index documents (`_index.yaml`),
/// declared the same way `ALLOWED_PAGE_SCHEMAS` (`parser::pages`) declares the
/// page namespaces. An index claiming a namespace outside this list fails
/// loud (Adv-B) instead of parsing as free-form text nothing consumes.
pub(crate) const ALLOWED_INDEX_SCHEMAS: &[&str] =
    &["openxcvr.schema_index.v1", "openxcvr.cdb_command_index.v1"];

/// Valid `kind:` values for schema index documents (`_index.yaml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexKind {
    MemoryMap,
    EnumRegistry,
    CdbCommandRegistry,
}

/// Identity block of a spec recorded in `_index.yaml` (`spec:` key).
/// Unknown keys are rejected: a typo inside `spec:` (e.g. `tytle:`)
/// must fail loud rather than silently dropping spec identity metadata.
#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpecIdentity {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub errata: Vec<serde_norway::Value>,
    #[serde(default)]
    pub parity_deferred: Option<bool>,
}

/// A parsed `_index.yaml` document. Unknown keys are rejected: every key
/// defined by ADRs or code is declared, so an unrecognized key fails loud
/// rather than being silently ignored.
///
/// `composites:` is deliberately **rejected**, not declared:
/// (§ "Component 2 — Composites", which shows the definition "in _index.yaml
/// or page file") defines it, and § 2 ("Top-level structure") lists
/// it among a *page* schema's optional top-level keys — not among an
/// `_index.yaml` document's keys. No document in the tree uses it and codegen
/// does not resolve composites — the same decision round 4 (A8) took for the
/// state-machine and page roots. A loud named failure beats a declared-but-dead
/// key accepted silently: whoever first writes `composites:` extends this IR
/// and the loaders together (see `ir/state_machines.rs` for the pattern).
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexFile {
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub spec: Option<SpecIdentity>,
    #[serde(default)]
    pub imports: Vec<String>,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub kind: Option<IndexKind>,
    #[serde(default)]
    pub templates: Option<BTreeMap<String, serde_norway::Value>>,
    #[serde(default)]
    pub cdb_commands: Vec<String>,
    #[serde(default)]
    pub state_machines: Vec<String>,
    #[serde(default)]
    pub tom_messages: Vec<String>,
}

/// Load and strictly validate one `_index.yaml` document: unknown top-level
/// keys, unknown spec keys, unknown kinds, an unknown `schema:` namespace,
/// and invalid templates are all rejected. Shared by the index loader, the
/// markdown emitter and the page-load template path (`inherit_templates`)
/// so all production readers agree — there is exactly one strict index
/// reader.
pub fn load_index_file(path: &Path) -> Result<IndexFile> {
    load_index_file_with_labels(path, &|path| path.display().to_string())
}

pub(crate) fn load_index_file_with_labels(
    path: &Path,
    label: &dyn Fn(&Path) -> String,
) -> Result<IndexFile> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", label(path)))?;
    let index: IndexFile = serde_norway::from_str(&content)
        .with_context(|| format!("failed to parse index {}", label(path)))?;
    // The namespace is checked like `ALLOWED_PAGE_SCHEMAS` checks a page's:
    // a typo'd or future namespace value fails loud naming the value and
    // the file (Adv-B) instead of free-form text nothing consumes.
    if let Some(schema) = &index.schema {
        anyhow::ensure!(
            ALLOWED_INDEX_SCHEMAS.contains(&schema.as_str()),
            "unsupported index namespace: {} in {}; allowed: {}",
            schema,
            label(path),
            ALLOWED_INDEX_SCHEMAS.join(", ")
        );
    }
    crate::parser::pages::validate_index_templates_with_labels(path, label)?;
    Ok(index)
}

/// The `schema:` namespace a schema document declares, found by scanning for
/// the first `schema:`-prefixed non-comment line and skipping anything passed
/// on the way (blank lines, comments, a leading `---` document marker).
/// Public so the strict-vocabulary guard test classifies with the same probe
/// the index loader uses — one classifier, no drift between production and
/// test.
pub fn probe_schema_kind(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(value) = trimmed.strip_prefix("schema:") else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        return (!value.is_empty()).then(|| value.to_string());
    }
    None
}

pub struct IndexLoader;

/// Which `_index.yaml` key supplies the schema entries being collected.
#[derive(Clone, Copy)]
enum EntrySource {
    /// The `includes:` key (pages, enums).
    Includes,
    /// The `state_machines:` key.
    StateMachines,
}

impl IndexLoader {
    /// Collect paths to all enum (openxcvr.enum.v2) schemas reachable from
    /// `_index.yaml` files under `schemas_root`.
    pub fn collect_enum_schemas(
        schemas_root: &Path,
        spec_filter: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        Self::collect_by_schema_kinds(schemas_root, spec_filter, &["openxcvr.enum.v2"])
    }

    /// Collect paths to all state-machine schemas
    /// (openxcvr.state_machine.v1 and v2) listed under the `state_machines:`
    /// key of `_index.yaml` files under `schemas_root`.
    ///
    /// Unlike `includes:` entries, `state_machines:` entries name schemas the
    /// generate pipeline treats as behaviour-table sources;
    /// both namespace versions are collected in the
    /// same pass so a spec can mix them.
    pub fn collect_state_machine_schemas(
        schemas_root: &Path,
        spec_filter: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        Self::collect_by_entry_source(
            schemas_root,
            spec_filter,
            crate::emitters::rust::state_machines::STATE_MACHINE_SCHEMAS,
            EntrySource::StateMachines,
        )
    }

    /// Collect paths to all page schemas (openxcvr.page.v1 and openxcvr.page.v2)
    /// reachable from `_index.yaml` files under `schemas_root`.
    ///
    /// Both namespace versions are collected in the same pass so that a memory
    /// map can mix v1 and v2 page schemas without any change to the caller.
    pub fn collect_page_schemas(
        schemas_root: &Path,
        spec_filter: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        Self::collect_by_schema_kinds(schemas_root, spec_filter, ALLOWED_PAGE_SCHEMAS)
    }

    fn collect_by_schema_kinds(
        schemas_root: &Path,
        spec_filter: Option<&str>,
        target_schemas: &[&str],
    ) -> Result<Vec<PathBuf>> {
        Self::collect_by_entry_source(
            schemas_root,
            spec_filter,
            target_schemas,
            EntrySource::Includes,
        )
    }

    fn collect_by_entry_source(
        schemas_root: &Path,
        spec_filter: Option<&str>,
        target_schemas: &[&str],
        entry_source: EntrySource,
    ) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        let mut seen_indexes = HashSet::new();
        let mut seen_schemas = HashSet::new();

        let walker = walkdir::WalkDir::new(schemas_root)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() == "_index.yaml");

        for entry in walker {
            let index_path = entry.path();
            let parent = index_path.parent().unwrap_or(Path::new("."));

            if let Some(filter) = spec_filter {
                // Match the spec directory by exact name, not substring: a
                // substring test lets `--spec cmis` also select `c-cmis-1.4`
                // (and any future `*cmis*` sibling), leaking that spec's pages
                // into the cmis emission. The top-level spec dir name equals the
                // spec id by convention (schemas/<id>/_index.yaml). Nested
                // indexes (e.g. cmis/cdb-commands) are reached via the entry
                // index's imports/includes, so excluding them here is correct.
                let dir_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name != filter {
                    continue;
                }
            }

            Self::collect_from_index(
                index_path,
                target_schemas,
                entry_source,
                &mut seen_indexes,
                &mut seen_schemas,
                &mut paths,
            )?;
        }

        Ok(paths)
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_from_index(
        index_path: &Path,
        target_schemas: &[&str],
        entry_source: EntrySource,
        seen_indexes: &mut HashSet<PathBuf>,
        seen_schemas: &mut HashSet<PathBuf>,
        paths: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let index_path = normalize_path(index_path);
        if !seen_indexes.insert(index_path.clone()) {
            return Ok(());
        }

        let parent = index_path.parent().unwrap_or(Path::new("."));
        let index = load_index_file(&index_path)?;

        match index.kind {
            Some(IndexKind::EnumRegistry) | Some(IndexKind::MemoryMap) => {}
            Some(IndexKind::CdbCommandRegistry) | None => return Ok(()),
        }

        for import in &index.imports {
            let import_path = normalize_path(&parent.join(import));
            anyhow::ensure!(
                import_path.exists(),
                "missing import {} referenced by {}",
                import_path.display(),
                index_path.display()
            );
            Self::collect_from_index(
                &import_path,
                target_schemas,
                entry_source,
                seen_indexes,
                seen_schemas,
                paths,
            )?;
        }

        let entries: &[String] = match entry_source {
            EntrySource::Includes => &index.includes,
            EntrySource::StateMachines => &index.state_machines,
        };
        let missing_kind = match entry_source {
            EntrySource::Includes => "include",
            EntrySource::StateMachines => "state machine schema",
        };
        for include in entries {
            let schema_path = normalize_path(&parent.join(include));
            anyhow::ensure!(
                schema_path.exists(),
                "missing {missing_kind} {} referenced by {}",
                schema_path.display(),
                index_path.display()
            );
            if probe_schema_kind(&schema_path)
                .as_deref()
                .map(|k| target_schemas.contains(&k))
                .unwrap_or(false)
                && seen_schemas.insert(schema_path.clone())
            {
                paths.push(schema_path);
            }
        }

        Ok(())
    }
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::pages::load_page_schema;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "openxcvr-codegen-index-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    /// Integration test: a memory_map _index.yaml that references one v1 page
    /// and one v2 page — both must be collected by collect_page_schemas.
    #[test]
    fn collect_page_schemas_accepts_v1_and_v2() {
        let dir = temp_dir("v1v2");
        fs::create_dir_all(&dir).unwrap();

        // Write a v1 page schema
        fs::write(
            dir.join("page-v1.yaml"),
            r#"schema: openxcvr.page.v1
kind: page
id: PageV1
title: V1 Page
page_address: lower
fields:
  - name: status
    offset: 0
    type: u8
"#,
        )
        .unwrap();

        // Write a v2 page schema with a since annotation
        fs::write(
            dir.join("page-v2.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: PageV2
title: V2 Page
page_address: lower
fields:
  - name: compat_flag
    offset: 1
    type: u8
    since: "4.0"
  - name: legacy_status
    offset: 2
    type: u8
    until: "5.0"
"#,
        )
        .unwrap();

        // Write a _index.yaml referencing both
        fs::write(
            dir.join("_index.yaml"),
            r#"kind: memory_map
includes:
  - page-v1.yaml
  - page-v2.yaml
"#,
        )
        .unwrap();

        let collected = IndexLoader::collect_page_schemas(&dir, None).unwrap();
        let names: Vec<String> = collected
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert!(
            names.contains(&"page-v1.yaml".to_string()),
            "v1 page must be collected; got: {names:?}"
        );
        assert!(
            names.contains(&"page-v2.yaml".to_string()),
            "v2 page must be collected; got: {names:?}"
        );
        assert_eq!(collected.len(), 2, "exactly two pages; got: {names:?}");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// Integration test: collect_page_schemas → load_page_schema must reject an
    /// invalid rev string in a `field_includes` fragment (full production-path
    /// call sequence).  Exercises the collect→load→validate pipeline that the
    /// adversarial review identified as unvalidated after commit c42eb95.
    #[test]
    fn collect_then_load_rejects_invalid_revs_in_field_includes() {
        let dir = temp_dir("collect-bad-rev");
        // Mirror the directory layout from collect_page_schemas_accepts_v1_and_v2:
        // schemas live directly in `dir`; _index.yaml is in `dir` too.
        // The fragment lives in `dir/fields/`; the page lives in `dir/schemas/`.
        // The fragment path inside the page is relative to the page's directory,
        // so the page at `schemas/page-00h.yaml` refers to `../fields/part.yaml`.
        fs::create_dir_all(dir.join("fields")).unwrap();
        fs::create_dir_all(dir.join("schemas")).unwrap();

        // Fragment with an invalid since rev string ("bogus")
        fs::write(
            dir.join("fields/part.yaml"),
            r#"fields:
  - name: bad_since_field
    offset: 1
    size_bytes: 1
    type: u8
    since: "bogus"
"#,
        )
        .unwrap();

        // v2 page that pulls in the fragment
        fs::write(
            dir.join("schemas/page-00h.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: PageWithBadFragment
title: Test Page
page_address: 00h
field_includes:
  - ../fields/part.yaml
fields:
  - name: inline_field
    offset: 0
    size_bytes: 1
    type: u8
"#,
        )
        .unwrap();

        // _index.yaml referencing the page
        fs::write(
            dir.join("_index.yaml"),
            r#"kind: memory_map
includes:
  - schemas/page-00h.yaml
"#,
        )
        .unwrap();

        // Step 1: collection succeeds — it only resolves index entries, not YAML bodies
        let collected = IndexLoader::collect_page_schemas(&dir, None)
            .expect("collect_page_schemas must succeed");
        assert_eq!(collected.len(), 1, "exactly one page collected");

        // Step 2: loading must fail with a rev validation error
        for entry_path in &collected {
            let err = load_page_schema(entry_path)
                .expect_err("load_page_schema must reject invalid since: \"bogus\" in fragment");
            let msg = format!("{err:#}");
            assert!(
                msg.contains("invalid rev string"),
                "expected 'invalid rev string', got: {msg}"
            );
            assert!(
                msg.contains("bogus"),
                "expected 'bogus' in error, got: {msg}"
            );
        }

        fs::remove_dir_all(&dir).ok();
    }

    /// Regression: `--spec cmis` must NOT pull in the sibling `c-cmis-1.4`
    /// spec. The filter used to be a substring test (`dir_name.contains`),
    /// so `"c-cmis-1.4".contains("cmis")` selected c-cmis as an extra entry
    /// point and leaked its pages into the cmis emission (the 16 orphan
    /// CCmis*.rs files). The filter is now an exact dir-name match.
    #[test]
    fn spec_filter_is_exact_not_substring() {
        let root = temp_dir("spec-filter-exact");
        let cmis = root.join("cmis");
        let ccmis = root.join("c-cmis-1.4");
        fs::create_dir_all(&cmis).unwrap();
        fs::create_dir_all(&ccmis).unwrap();

        // cmis entry index + one page
        fs::write(
            cmis.join("page-cmis.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: CmisPage
title: CMIS Page
page_address: lower
fields:
  - name: a
    offset: 0
    type: u8
"#,
        )
        .unwrap();
        fs::write(
            cmis.join("_index.yaml"),
            "kind: memory_map\nincludes:\n  - page-cmis.yaml\n",
        )
        .unwrap();

        // c-cmis entry index + one page (must be excluded under --spec cmis)
        fs::write(
            ccmis.join("page-ccmis.yaml"),
            r#"schema: openxcvr.page.v2
kind: page
id: CCmisPage
title: C-CMIS Page
page_address: lower
fields:
  - name: b
    offset: 0
    type: u8
"#,
        )
        .unwrap();
        fs::write(
            ccmis.join("_index.yaml"),
            "kind: memory_map\nincludes:\n  - page-ccmis.yaml\n",
        )
        .unwrap();

        let collected = IndexLoader::collect_page_schemas(&root, Some("cmis")).unwrap();
        let names: Vec<String> = collected
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert!(
            names.contains(&"page-cmis.yaml".to_string()),
            "cmis page must be collected under --spec cmis; got: {names:?}"
        );
        assert!(
            !names.contains(&"page-ccmis.yaml".to_string()),
            "c-cmis-1.4 page must NOT leak into --spec cmis; got: {names:?}"
        );
        assert_eq!(
            collected.len(),
            1,
            "exactly the cmis page, no c-cmis leak; got: {names:?}"
        );

        fs::remove_dir_all(&root).ok();
    }
}
