//! Regression probes for the YAML parser semantics the schema pipeline relies
//! on, pinned when the parser moved from `serde_yml` to `serde_norway`.
//!
//! These probe the exact semantics real schemas use: flow-style mappings
//! (`page-28h` instance metadata), unquoted hex scalars inside flow ranges
//! (`page-9fh` LPL byte ranges), explicit `null`s (`state-machines` guards and
//! timeouts), `Value`/`from_value` template inheritance (`_index.yaml`
//! metadata), and untagged enums (the only serde enum representation the
//! parsers use). No schema uses YAML tags, anchors, aliases, or merge keys,
//! so none are probed here. Malformed and duplicate input must fail closed:
//! an unreadable schema is an error, never a default.

use serde::Deserialize;
use serde_norway::{from_str, from_value, Value};

#[derive(Debug, Deserialize, PartialEq)]
struct Instances {
    index: String,
    start: u64,
    end: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Timed {
    timeout: Option<u16>,
}

/// Mirrors `MetadataTemplate` in parser/pages.rs: template metadata is a
/// nested mapping with optional `access` and a `flags` sub-mapping.
#[derive(Debug, Deserialize, PartialEq)]
struct Template {
    access: Option<String>,
    flags: Option<TemplateFlags>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TemplateFlags {
    is_required: Option<bool>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum Effect {
    Simple(String),
    Detailed { effect: String, note: String },
}

#[test]
fn flow_mapping_matches_block_style() {
    let flow = from_str::<Instances>("{ index: threshold_set, start: 1, end: 16 }").unwrap();
    let block = from_str::<Instances>("index: threshold_set\nstart: 1\nend: 16\n").unwrap();
    assert_eq!(
        flow,
        Instances {
            index: "threshold_set".to_string(),
            start: 1,
            end: 16,
        }
    );
    assert_eq!(
        flow, block,
        "flow and block mappings must decode identically"
    );
}

#[test]
fn unquoted_hex_scalar_is_integer_and_range_string_is_untouched() {
    let value = from_str::<Value>("value: 0x10").unwrap();
    assert_eq!(
        value.get("value").and_then(|v| v.as_u64()),
        Some(16),
        "a bare unquoted hex scalar must decode as an integer"
    );
    // Flow ranges like `0x00..0x78` are plain string scalars; the endpoints
    // are split downstream, so the parser must deliver the text verbatim.
    let range = from_str::<Value>("{ value: 0x00..0x78 }").unwrap();
    assert_eq!(
        range.get("value").and_then(|v| v.as_str()),
        Some("0x00..0x78")
    );
}

#[test]
fn explicit_null_is_value_null_and_option_none() {
    let value = from_str::<Value>("timeout: null").unwrap();
    assert_eq!(value.get("timeout"), Some(&Value::Null));
    let typed: Timed = from_str("timeout: null").unwrap();
    assert_eq!(typed.timeout, None);
}

#[test]
fn from_value_reads_nested_mapping_like_template_inheritance() {
    // Mirrors parser/pages.rs: an `_index.yaml` is parsed to `Value`, a
    // template mapping is extracted, then `from_value` decodes it into the
    // inherited metadata struct.
    let index = from_str::<Value>(
        "templates:\n  control:\n    access: ro\n    flags: { is_required: true }\n",
    )
    .unwrap();
    let template = index.get("templates").unwrap().get("control").unwrap();
    let inherited: Template = from_value(template.clone()).unwrap();
    assert_eq!(
        inherited,
        Template {
            access: Some("ro".to_string()),
            flags: Some(TemplateFlags {
                is_required: Some(true),
            }),
        }
    );
}

#[test]
fn untagged_enum_accepts_string_and_mapping_arms() {
    assert_eq!(
        from_str::<Effect>("none").unwrap(),
        Effect::Simple("none".to_string())
    );
    assert_eq!(
        from_str::<Effect>("{ effect: none, note: why }").unwrap(),
        Effect::Detailed {
            effect: "none".to_string(),
            note: "why".to_string(),
        }
    );
}

#[test]
fn duplicate_keys_fail_closed() {
    let value = from_str::<Value>("a: 1\na: 2");
    let err = value.unwrap_err().to_string();
    assert!(
        err.contains("duplicate entry with key \"a\""),
        "duplicate mapping keys must be a hard error, got: {err}"
    );
    #[derive(Deserialize)]
    struct Dup {
        a: u8,
    }
    assert_eq!(from_str::<Dup>("a: 1").unwrap().a, 1);
    assert!(from_str::<Dup>("a: 1\na: 2").is_err());
}

#[test]
fn malformed_input_fails_closed() {
    // Tab indentation is invalid YAML and must error, not default.
    assert!(from_str::<Value>("a:\n\tb: 1").is_err());
    // Unclosed flow mapping must error.
    assert!(from_str::<Value>("{ a: 1").is_err());
    // A typed decode must reject a document whose root is a scalar.
    assert!(from_str::<Timed>("just a scalar").is_err());
}
