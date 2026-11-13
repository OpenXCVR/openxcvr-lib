//! Per-spec and top-level index rendering, including the schema-kind
//! coverage inventory.
//!
//! The inventory walks the spec's schema directory and classifies every
//! schema kind it finds: kinds the generate pipeline renders, organizational
//! index files, and kinds with no IR/loader support — the last are rendered
//! as explicit visible gaps, never silently omitted.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use walkdir::WalkDir;

use crate::ir::enums::EnumSchema;
use crate::ir::pages::PageSchema;
use crate::ir::version::{Family, Rev};
use crate::parser::index::{load_index_file, IndexLoader, SpecIdentity};

use super::enums::enum_file_stem;
use super::pages::page_file_stem;
use super::{md_code, md_text, GENERATED_BY};

fn read_spec_identity(spec_index: &Path) -> anyhow::Result<SpecIdentity> {
    let index = load_index_file(spec_index)?;
    Ok(index.spec.unwrap_or_default())
}

/// Rendered / not-rendered coverage statement for one schema kind.
fn kind_coverage(kind: &str) -> &'static str {
    match kind {
        "page" => "rendered — per-page file under `pages/`",
        "enum" => "rendered — per-enum file under `enums/`",
        "memory_map" | "enum_registry" | "cdb_command_registry" => {
            "organizational index — not rendered as content"
        }
        "cdb_command" => "not rendered — scaffold IR exists (`ir::cdb_commands`) but it is not wired into the generate pipeline (visible gap)",
        "tom_message" => "not rendered — scaffold IR exists (`ir::tom_messages`) but it is not wired into the generate pipeline (visible gap)",
        "state_machine" => "rendered as Rust behaviour tables under generated/fsm/ — not rendered as Markdown reference content",
        "calibration" => "not rendered — scaffold IR exists (`ir::calibration`) but it is not wired into the generate pipeline (visible gap)",
        "(include fragment)" => {
            "field/entry include fragment — merged into the including page or enum schema, not a standalone schema"
        }
        "(schema without kind)" => "not rendered — schema file without a `kind:` key cannot be classified (visible gap)",
        "(unreadable)" => "not rendered — file could not be parsed as YAML (visible gap)",
        _ => "not rendered — unrecognized schema kind (visible gap)",
    }
}

/// Count schema files per top-level `kind:` under a spec directory.
/// Files without a `kind:` are include fragments; unreadable files get
/// their own visible-gap bucket. Nothing is silently dropped.
fn scan_kinds(spec_dir: &Path) -> anyhow::Result<BTreeMap<String, usize>> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in WalkDir::new(spec_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml");
        if !is_yaml {
            continue;
        }
        let bucket = match serde_norway::from_str::<serde_norway::Value>(
            &std::fs::read_to_string(path)
                .with_context(|| format!("failed to inventory {}", path.display()))?,
        )
        .ok()
        {
            Some(value) => match value.get("kind").and_then(|k| k.as_str()) {
                Some(kind) => kind.to_string(),
                None => {
                    if value.get("schema").is_some() {
                        "(schema without kind)".to_string()
                    } else {
                        "(include fragment)".to_string()
                    }
                }
            },
            None => "(unreadable)".to_string(),
        };
        *counts.entry(bucket).or_default() += 1;
    }
    Ok(counts)
}

/// The per-spec index: page and enum listings plus the kind inventory.
pub fn emit_spec_index(
    schemas_root: &Path,
    spec: &str,
    spec_filter: Option<&str>,
    enums: Vec<(&Path, &EnumSchema)>,
    pages: Vec<(&Path, &PageSchema)>,
) -> anyhow::Result<String> {
    let spec_dir = schemas_root.join(spec);
    let identity = read_spec_identity(&spec_dir.join("_index.yaml"))?;

    let mut out = String::new();
    out.push_str(&format!(
        "# {spec} — generated schema reference\n\n{GENERATED_BY}\n\n"
    ));
    push_spec_identity(&mut out, spec, &identity);
    if spec_filter.is_some_and(|filter| filter != spec) {
        out.push_str(
            "- Reached via imports: this spec's schemas are loaded through the \
             `imports:` of another spec's index, not generated directly as a \
             `--spec` target\n",
        );
    }

    let mut pages: Vec<&(&Path, &PageSchema)> = pages.iter().collect();
    pages.sort_by_key(|(_, schema)| schema.id.clone());
    let mut enums: Vec<&(&Path, &EnumSchema)> = enums.iter().collect();
    enums.sort_by_key(|(_, schema)| schema.id.clone());

    out.push_str(&format!("\n## Pages ({})\n\n", pages.len()));
    if pages.is_empty() {
        out.push_str("This spec defines no page schemas.\n");
    } else {
        out.push_str("| Page | Title | Address | Size | Fields |\n|---|---|---|---|---|\n");
        for (_, schema) in pages {
            out.push_str(&format!(
                "| [`{}`](pages/{}.md) | {} | `{}` | {} | {} |\n",
                md_code(&schema.id),
                page_file_stem(schema),
                md_text(&schema.title),
                md_code(&schema.page_address),
                schema
                    .size_bytes
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "not recorded".to_string()),
                schema.fields.len()
            ));
        }
    }

    out.push_str(&format!("\n## Enums ({})\n\n", enums.len()));
    if enums.is_empty() {
        out.push_str("This spec defines no enum schemas.\n");
    } else {
        out.push_str("| Enum | Title | Entries |\n|---|---|---|\n");
        for (_, schema) in enums {
            out.push_str(&format!(
                "| [`{}`](enums/{}.md) | {} | {} |\n",
                md_code(&schema.id),
                enum_file_stem(schema),
                md_text(&schema.title),
                schema.entries.len()
            ));
        }
    }

    out.push_str("\n## Schema-kind coverage\n\n");
    out.push_str(&format!(
        "Every schema kind present under `schemas/{spec}/`:\n\n"
    ));
    out.push_str("| Kind | Files | Coverage |\n|---|---|---|\n");
    for (kind, count) in scan_kinds(&spec_dir)? {
        let kind_label = if kind.starts_with('(') {
            kind.clone()
        } else {
            format!("`{kind}`")
        };
        out.push_str(&format!(
            "| {kind_label} | {count} | {} |\n",
            kind_coverage(&kind)
        ));
    }
    out.push_str(
        "\nFile counts cover the whole directory tree; only schemas reachable \
         from `_index.yaml` are emitted.\n",
    );
    Ok(out)
}

/// The top-level index: every spec directory under the schemas root, with
/// rendered status. Rendered status is the state of the output directory
/// (accumulated across `--spec` runs); specs without generated output are
/// probed by loading and validating their schemas so the reason is accurate.
pub fn emit_top_index(schemas_root: &Path, out_root: &Path) -> anyhow::Result<String> {
    let mut specs = Vec::new();
    for entry in std::fs::read_dir(schemas_root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("_index.yaml").is_file() {
            specs.push(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("non-UTF8 spec directory"))?,
            );
        }
    }
    specs.sort();

    let mut out = String::new();
    out.push_str(&format!(
        "# Generated schema reference\n\n{GENERATED_BY}\n\n"
    ));
    out.push_str(
        "Reference documentation for the OpenXCVR schemas, generated from the same \
         codegen IR and revision resolver that emit the Rust and binding surfaces. \
         Field units, access, volatility, read effects, revision applicability, \
         provenance and descriptions are rendered from the resolved \
         schema metadata; `not recorded` marks metadata the schema does not carry.\n\n",
    );
    out.push_str(
        "## Specs\n\n| Spec | Title | Revisions | Status | Pages | Enums |\n|---|---|---|---|---|---|\n",
    );

    for spec in &specs {
        let identity = read_spec_identity(&schemas_root.join(spec).join("_index.yaml"))?;
        let spec_out = out_root.join(spec);
        if spec_out.join("index.md").is_file() {
            let pages = count_md(&spec_out.join("pages"))?;
            let enums = count_md(&spec_out.join("enums"))?;
            let revisions = revision_cell(spec, &identity);
            out.push_str(&format!(
                "| [`{}`]({}/index.md) | {} | {revisions} | rendered | {pages} | {enums} |\n",
                md_code(spec),
                md_code(spec),
                identity
                    .title
                    .as_deref()
                    .map(md_text)
                    .unwrap_or_else(|| "not recorded".to_string()),
            ));
        } else {
            let reason = md_text(&probe_spec(schemas_root, spec));
            let revisions = revision_cell(spec, &identity);
            out.push_str(&format!(
                "| `{}` | {} | {revisions} | {reason} | — | — |\n",
                md_code(spec),
                identity
                    .title
                    .as_deref()
                    .map(md_text)
                    .unwrap_or_else(|| "not recorded".to_string()),
            ));
        }
    }

    out.push_str("\n## All-schema kind inventory\n\n");
    out.push_str("All namespace directories are inventoried, including those without generated output. Counts include index-unreachable files; output counts describe this documentation tree, not decoder qualification.\n\n");
    out.push_str("| Spec | Kind | Files | Coverage |\n|---|---|---|---|\n");
    for spec in &specs {
        for (kind, count) in scan_kinds(&schemas_root.join(spec))? {
            let coverage = match kind.as_str() {
                "page" | "enum" => {
                    let dir = if kind == "page" { "pages" } else { "enums" };
                    let rendered = count_md(&out_root.join(spec).join(dir))?;
                    format!(
                        "{} — {rendered}/{count} schema files have output in this tree",
                        if rendered == 0 {
                            "not rendered"
                        } else {
                            "rendered"
                        }
                    )
                }
                _ => kind_coverage(&kind).to_string(),
            };
            out.push_str(&format!(
                "| `{}` | `{}` | {count} | {coverage} |\n",
                md_code(spec),
                md_code(&kind)
            ));
        }
    }

    out.push_str(
        "\n## Regenerating\n\n```sh\ntools/regen-markdown.sh            # every schemas/*/_index.yaml namespace\ntools/regen-markdown.sh cmis       # one namespace\n```\n",
    );
    Ok(out)
}

/// Revision list for a windowed family, taken from the codegen revision
/// domain. Companion namespaces have no domain and return `None`.
fn modelled_revisions(spec: &str) -> Option<String> {
    Family::from_spec_id(spec).map(|family| {
        family
            .revs()
            .iter()
            .map(|rev| rev.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// Top-index revisions cell. Windowed families show the domain list; every
/// other namespace keeps the single document revision from its index.
fn revision_cell(spec: &str, identity: &SpecIdentity) -> String {
    if let Some(list) = modelled_revisions(spec) {
        format!("`{}`", md_code(&list))
    } else {
        identity
            .revision
            .as_deref()
            .map(|revision| format!("`{}`", md_code(revision)))
            .unwrap_or_else(|| "not recorded".to_string())
    }
}

/// Runtime label corresponding to a document revision that is not itself in
/// the family domain. `12.5a` matches runtime `12.5`; a revision that parses
/// inside the family matches nothing extra.
fn runtime_label_for_document(spec: &str, document_revision: &str) -> Option<&'static str> {
    let family = Family::from_spec_id(spec)?;
    if Rev::parse(document_revision).is_ok_and(|rev| rev.family() == family) {
        return None;
    }
    family
        .revs()
        .iter()
        .map(|rev| rev.as_str())
        .filter(|label| {
            document_revision.strip_prefix(label).is_some_and(|rest| {
                !rest.is_empty() && !rest.starts_with(|c: char| c.is_ascii_digit())
            })
        })
        .max_by_key(|label| label.len())
}

fn errata_list(errata: &[serde_norway::Value]) -> String {
    errata
        .iter()
        .map(|item| {
            if let Some(text) = item.as_str() {
                format!("`{}`", md_code(text))
            } else if let Some(map) = item.as_mapping() {
                let revision = map.get("revision").map(super::scalar).unwrap_or_default();
                let source = map.get("source").map(super::scalar).unwrap_or_default();
                format!("`{}` (`{}`)", md_code(&revision), md_code(&source))
            } else {
                format!("`{}`", md_code(&super::scalar(item)))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn push_spec_identity(out: &mut String, spec: &str, identity: &SpecIdentity) {
    let title = identity
        .title
        .as_deref()
        .map(md_text)
        .unwrap_or_else(|| "not recorded".to_string());
    let document_revision = identity
        .revision
        .as_deref()
        .map(|revision| format!("`{}`", md_code(revision)))
        .unwrap_or_else(|| "not recorded".to_string());
    let source = identity
        .source
        .as_deref()
        .map(|source| format!("`{}`", md_code(source)))
        .unwrap_or_else(|| "not recorded".to_string());
    if let Some(list) = modelled_revisions(spec) {
        out.push_str(&format!(
            "- Spec: {title} (from `schemas/{spec}/_index.yaml`)\n"
        ));
        out.push_str(&format!(
            "- Document revision: {document_revision}, source {source}"
        ));
        if !identity.errata.is_empty() {
            out.push_str(&format!("; errata: {}", errata_list(&identity.errata)));
        }
        out.push('\n');
        out.push_str(&format!(
            "- Modelled revisions: `{}` (codegen revision domain for this family; not a single revision)\n",
            md_code(&list)
        ));
        if let Some(document) = identity.revision.as_deref() {
            if let Some(runtime) = runtime_label_for_document(spec, document) {
                out.push_str(&format!(
                    "- Runtime revision: `{}`. Document revision `{}` is the cited specification's label and is not a runtime revision label.\n",
                    md_code(runtime),
                    md_code(document)
                ));
            }
        }
    } else {
        out.push_str(&format!(
            "- Spec: {title}, revision {document_revision}, source {source} (from `schemas/{spec}/_index.yaml`)\n"
        ));
    }
}

fn count_md(dir: &Path) -> anyhow::Result<usize> {
    if !dir.try_exists()? {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|x| x == "md") {
            count += 1;
        }
    }
    Ok(count)
}

/// Load and validate a spec's schemas to state accurately why it has no
/// generated output. A clean load means the spec is simply not part of the
/// generated set.
fn probe_spec(schemas_root: &Path, spec: &str) -> String {
    match probe_load(schemas_root, spec) {
        Ok(()) => {
            "not rendered — schemas load and validate cleanly; no generated output in this tree"
                .to_string()
        }
        Err(err) => format!("not rendered — schemas fail validation: {err:#}"),
    }
}

// Retain legacy root redaction only for opaque loader/catalog failures.
// Validation labels are rendered at their source, not rewritten in the message.
fn loader_source_error(schemas_root: &Path, error: impl std::fmt::Display) -> anyhow::Error {
    let normalized = crate::parser::index::normalize_path(schemas_root);
    let mut detail = format!("{error:#}");
    for root in [schemas_root, normalized.as_path()] {
        let prefix = format!(
            "{}{}",
            root.display()
                .to_string()
                .trim_end_matches(std::path::is_separator),
            std::path::MAIN_SEPARATOR
        );
        detail = detail.replace(&prefix, "schemas/");
    }
    anyhow::anyhow!(detail)
}

fn probe_load(schemas_root: &Path, spec: &str) -> anyhow::Result<()> {
    let source_label = |path: &Path| format!("schemas/{}", super::source_rel(schemas_root, path));
    let enum_paths = IndexLoader::collect_enum_schemas(schemas_root, Some(spec))
        .map_err(|error| loader_source_error(schemas_root, error))?;
    let mut enum_schemas = Vec::new();
    for path in &enum_paths {
        enum_schemas.push(crate::parser::enums::load_enum_schema_with_labels(
            path,
            &source_label,
        )?);
    }
    crate::validator::validate_enums(&enum_schemas)?;
    let page_paths = IndexLoader::collect_page_schemas(schemas_root, Some(spec))
        .map_err(|error| loader_source_error(schemas_root, error))?;
    let mut page_pairs = Vec::new();
    for path in &page_paths {
        let schema = crate::parser::pages::load_page_schema_with_labels(path, &source_label)?;
        page_pairs.push((path.clone(), schema));
    }
    let page_schemas: Vec<PageSchema> = page_pairs.iter().map(|(_, s)| s.clone()).collect();
    crate::validator::validate_pages(&page_schemas)?;
    crate::validator::validate_page_enum_refs(&page_schemas, &enum_schemas)?;
    // The same shared per-namespace presence validation the driver runs, so
    // this probe can never report a clean load for a namespace generation
    // rejects (presence classification included).
    let info = crate::emitters::rust::catalog::page_catalog_info(&page_paths)
        .map_err(|error| loader_source_error(schemas_root, error))?;
    let label = |page_id: &str, path: Option<&str>| {
        path.map(|path| source_label(Path::new(path)))
            .unwrap_or_else(|| page_id.to_string())
    };
    crate::presence::validate_presence_by_namespace_with_labels(
        schemas_root,
        &page_pairs,
        &info,
        &label,
    )
    .map_err(|e| anyhow::anyhow!("presence classification: {e}"))?;
    Ok(())
}
