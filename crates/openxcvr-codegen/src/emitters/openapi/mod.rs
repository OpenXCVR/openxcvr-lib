pub mod enums;
pub mod pages;

use crate::emitters::Emitter;
use crate::ir::enums::EnumSchema;
use crate::ir::pages::PageSchema;

pub struct OpenApiEmitter;

impl Emitter for OpenApiEmitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String {
        enums::emit_enum(schema)
    }

    fn emit_page(&self, schema: &PageSchema) -> String {
        pages::emit_page(schema)
    }
}

/// Re-export of the centralized casing primitive. The transform
/// logic lives only in `crate::ir::naming`; this is a thin forwarding shim so
/// the OpenAPI sub-emitters can keep `use super::camel_to_snake;`.
pub fn camel_to_snake(s: &str) -> String {
    crate::ir::naming::camel_to_snake(s)
}

/// Recorded description text, trimmed, if one is recorded: a recorded but
/// empty/whitespace description counts as not recorded (the same semantics
/// the Rust field docs use). Absence stays absence — nothing substitutes.
pub(super) fn recorded_description(description: &Option<String>) -> Option<String> {
    description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
}

/// Recorded spec identity as the `x-openxcvr-spec` extension value: the
/// recorded id, the recorded revision string when present, and recorded
/// errata (rendered as scalar strings, exactly as the Markdown reference
/// cites them) when present — nothing inferred.
pub(super) fn spec_extension(spec: &crate::ir::pages::SpecRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("id".to_string(), serde_json::Value::String(spec.id.clone()));
    if let Some(revision) = &spec.revision {
        obj.insert(
            "revision".to_string(),
            serde_json::Value::String(revision.clone()),
        );
    }
    if !spec.errata.is_empty() {
        let errata = spec
            .errata
            .iter()
            .map(|e| {
                if e.as_mapping().is_some() {
                    yaml_value_to_json(e)
                } else {
                    serde_json::Value::String(crate::emitters::markdown::scalar(e))
                }
            })
            .collect::<Vec<_>>();
        obj.insert("errata".to_string(), serde_json::Value::Array(errata));
    }
    serde_json::Value::Object(obj)
}

/// Convert a recorded YAML value (enum/page provenance) to a JSON value
/// verbatim: nothing is summarized, nested structures keep their shape, and
/// scalar kinds map onto their JSON equivalents. Non-string mapping keys
/// (none are recorded today) render through the shared scalar helper so they
/// stay recorded rather than being dropped. serde_json string-escaping of
/// the values happens at serialization, as for every other JSON string.
pub(super) fn yaml_value_to_json(value: &serde_norway::Value) -> serde_json::Value {
    use serde_json::Value as Json;
    match value {
        serde_norway::Value::Null => Json::Null,
        serde_norway::Value::Bool(b) => Json::Bool(*b),
        serde_norway::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Json::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Json::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                Json::Number(
                    serde_json::Number::from_f64(f)
                        .expect("YAML float without a JSON representation"),
                )
            } else {
                Json::String(crate::emitters::markdown::scalar(value))
            }
        }
        serde_norway::Value::String(s) => Json::String(s.clone()),
        serde_norway::Value::Sequence(seq) => {
            Json::Array(seq.iter().map(yaml_value_to_json).collect())
        }
        serde_norway::Value::Mapping(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let key = if let Some(s) = k.as_str() {
                    s.to_string()
                } else {
                    crate::emitters::markdown::scalar(k)
                };
                out.insert(key, yaml_value_to_json(v));
            }
            Json::Object(out)
        }
        serde_norway::Value::Tagged(tagged) => yaml_value_to_json(&tagged.value),
    }
}
