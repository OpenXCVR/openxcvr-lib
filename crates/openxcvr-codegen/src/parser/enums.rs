use crate::ir::enums::{EnumEntry, EnumSchema};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// gives an entry fragment exactly one key (`entries:`); an
/// unknown top-level key — a misspelling of `entries:` — would otherwise
/// discard the whole fragment's entries silently with every gate green, so
/// unknown keys are denied on the wrapper too, not only on its elements.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnumEntryFragment {
    #[serde(default)]
    entries: Vec<EnumEntry>,
}

pub fn load_enum_schema(path: &Path) -> Result<EnumSchema> {
    load_enum_schema_with_labels(path, &|path| path.display().to_string())
}

// Only diagnostic labels vary; reads and include resolution use the original paths.
pub(crate) fn load_enum_schema_with_labels(
    path: &Path,
    label: &dyn Fn(&Path) -> String,
) -> Result<EnumSchema> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", label(path)))?;
    let mut schema: EnumSchema = serde_norway::from_str(&content)
        .with_context(|| format!("failed to parse enum schema from {}", label(path)))?;

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    for include in &schema.entry_includes {
        let include_path = base_dir.join(include);
        let content = std::fs::read_to_string(&include_path).with_context(|| {
            format!(
                "failed to read entry include {} referenced by {}",
                label(&include_path),
                label(path)
            )
        })?;
        let fragment: EnumEntryFragment = serde_norway::from_str(&content).with_context(|| {
            format!(
                "failed to parse entry include {} referenced by {}",
                label(&include_path),
                label(path)
            )
        })?;
        schema.entries.extend(fragment.entries);
    }

    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::load_enum_schema;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "openxcvr-codegen-enums-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn load_enum_schema_merges_entry_includes() {
        let dir = temp_dir("merge");
        fs::create_dir_all(dir.join("entries")).unwrap();
        fs::write(
            dir.join("entries/part.yaml"),
            r#"entries:
  - value: 1
    name: included_value
"#,
        )
        .unwrap();
        fs::write(
            dir.join("enum.yaml"),
            r#"schema: openxcvr.enum.v2
kind: enum
id: TestEnum
title: Test Enum
entry_includes:
  - entries/part.yaml
entries:
  - value: 0
    name: inline_value
"#,
        )
        .unwrap();

        let schema = load_enum_schema(&dir.join("enum.yaml")).unwrap();

        assert_eq!(schema.entries.len(), 2);
        assert_eq!(schema.entries[0].name, "inline_value");
        assert_eq!(schema.entries[1].name, "included_value");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_enum_schema_reports_missing_entry_include() {
        let dir = temp_dir("missing");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("enum.yaml"),
            r#"schema: openxcvr.enum.v2
kind: enum
id: TestEnum
title: Test Enum
entry_includes:
  - entries/missing.yaml
entries: []
"#,
        )
        .unwrap();

        let err = load_enum_schema(&dir.join("enum.yaml")).unwrap_err();

        assert!(err.to_string().contains("failed to read entry include"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_enum_schema_carries_root_description() {
        // An enum-root `description:` is carried through the IR exactly as
        // recorded (block-scalar trailing newline included); an omitted key
        // stays None. Nothing is invented in either direction.
        let dir = temp_dir("rootdesc");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("with.yaml"),
            "schema: openxcvr.enum.v2\nkind: enum\nid: TestEnum\ntitle: Test Enum\ndescription: >\n  Root prose line one\n  continued.\nentries:\n  - value: 0\n    name: zero\n",
        )
        .unwrap();
        fs::write(
            dir.join("without.yaml"),
            "schema: openxcvr.enum.v2\nkind: enum\nid: TestEnum\ntitle: Test Enum\nentries:\n  - value: 0\n    name: zero\n",
        )
        .unwrap();

        let with = load_enum_schema(&dir.join("with.yaml")).unwrap();
        assert_eq!(
            with.description.as_deref(),
            Some("Root prose line one continued.\n")
        );
        let without = load_enum_schema(&dir.join("without.yaml")).unwrap();
        assert_eq!(without.description, None);
        fs::remove_dir_all(dir).unwrap();
    }
}
