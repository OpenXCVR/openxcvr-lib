//! Java requires a public class to live in a file of the same name. The emitter
//! writes `{JavaNaming.type_name(id)}.java`; a leftover `{schema.id}.java` when
//! the id is snake_case is the 6-file `gradle build` failure on parity.

use openxcvr_codegen::ir::naming::{JavaNaming, NamingPolicy};
use std::fs;
use std::path::PathBuf;

fn generated_java_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bindings/java/src/main/java/com/openxcvr/generated")
}

#[test]
fn snake_case_schema_id_becomes_pascal_case_java_type() {
    assert_eq!(
        JavaNaming.type_name("page_a2h_diagnostics_base"),
        "PageA2hDiagnosticsBase"
    );
}

#[test]
fn committed_java_files_match_their_public_class() {
    let dir = generated_java_dir();
    assert!(
        dir.is_dir(),
        "missing generated java dir: {}",
        dir.display()
    );
    let mut mismatches = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("java") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(&path).unwrap();
        let class = text
            .lines()
            .find_map(|l| {
                l.strip_prefix("public final class ")
                    .or_else(|| l.strip_prefix("public class "))
                    .or_else(|| l.strip_prefix("public enum "))
            })
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("<no public type>");
        if class != stem {
            mismatches.push(format!(
                "{} declares {}",
                path.file_name().unwrap().to_string_lossy(),
                class
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "Java public type must match filename (parity gradle compile). mismatches:\n{}",
        mismatches.join("\n")
    );
}
