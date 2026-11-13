use serde::{Deserialize, Deserializer};

use crate::ir::pages::SpecRef;

/// Unknown keys are rejected: every key the tree uses is declared, so a
/// misspelled enum-level key fails loud instead of being silently dropped.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumSchema {
    pub schema: String,
    pub kind: String,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub entry_includes: Vec<String>,
    #[serde(default)]
    pub entries: Vec<EnumEntry>,
    /// Spec identity (`spec:` block) recorded by the schema, if any.
    #[serde(default)]
    pub spec: Option<SpecRef>,
    /// Enum-level provenance (`provenance:` block) recorded by the schema.
    #[serde(default)]
    pub provenance: Option<serde_norway::Value>,
    /// Enum-level description (`description:` at the enum root) recorded by
    /// the schema, if any. An omitted key is `None`; no text is invented
    /// (see [`EnumEntry::description`] for the entry-level equivalent).
    #[serde(default)]
    pub description: Option<String>,
}

/// A single integer value or a contiguous range (e.g. `"0x27..0x7f"`).
/// Range entries exist in the YAML for reserved/vendor-specific blocks that
/// span many byte values; they cannot become single Rust enum variants.
#[derive(Debug, Clone)]
pub enum EntryValue {
    Single(u64),
    Range { start: u64, end: u64 },
}

impl EntryValue {
    pub fn as_single(&self) -> Option<u64> {
        match self {
            EntryValue::Single(v) => Some(*v),
            EntryValue::Range { .. } => None,
        }
    }
}

impl<'de> Deserialize<'de> for EntryValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct EntryValueVisitor;

        impl<'de> Visitor<'de> for EntryValueVisitor {
            type Value = EntryValue;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "an integer or a range string like \"0x27..0x7f\"")
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<EntryValue, E> {
                Ok(EntryValue::Single(v))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<EntryValue, E> {
                Ok(EntryValue::Single(v as u64))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<EntryValue, E> {
                let parts: Vec<&str> = v.splitn(2, "..").collect();
                if parts.len() != 2 {
                    return Err(E::custom(format!(
                        "expected range \"start..end\", got {v:?}"
                    )));
                }
                let parse = |s: &str| -> Result<u64, E> {
                    let s = s.trim();
                    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                        u64::from_str_radix(hex, 16)
                            .map_err(|e| E::custom(format!("bad hex {s:?}: {e}")))
                    } else {
                        s.parse::<u64>()
                            .map_err(|e| E::custom(format!("bad int {s:?}: {e}")))
                    }
                };
                Ok(EntryValue::Range {
                    start: parse(parts[0])?,
                    end: parse(parts[1])?,
                })
            }
        }

        d.deserialize_any(EntryValueVisitor)
    }
}

/// Unknown keys are rejected: a typo here would silently drop an entry
/// annotation (the version windows especially) and change the decoded
/// surface without any error.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumEntry {
    pub value: EntryValue,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub signaling: Option<SignalingInfo>,
    pub reach: Option<ReachInfo>,
    pub bit_rate_gbps: Option<f64>,
    pub vendor_specific: Option<serde_norway::Value>,
    /// First family revision at which the entry applies (inclusive), as a
    /// quoted known family revision string (e.g. `"4.0"`). Validated by
    /// `validator::validate_enums` — there is no parser-time validation for
    /// enum entries. Presence-strict: an omitted key is `None`, a present
    /// value must be a string (see `deserialize_present_string`).
    #[serde(default, deserialize_with = "deserialize_present_string")]
    pub since: Option<String>,
    /// First family revision at which the entry no longer applies
    /// (exclusive), same contract as [`EnumEntry::since`].
    #[serde(default, deserialize_with = "deserialize_present_string")]
    pub until: Option<String>,
    /// Unsupported enum vocabulary, captured raw so
    /// `validator::validate_enums` can reject its presence explicitly.
    /// Presence-preserving: any present value — including an explicit
    /// `null` — becomes `Some(Value)` (see `deserialize_present_raw`).
    #[serde(default, deserialize_with = "deserialize_present_raw")]
    pub changed_in: Option<serde_norway::Value>,
    /// Unsupported enum vocabulary; same contract as
    /// [`EnumEntry::changed_in`].
    #[serde(default, deserialize_with = "deserialize_present_raw")]
    pub renamed_from: Option<serde_norway::Value>,
    /// Unsupported enum vocabulary; same contract as
    /// [`EnumEntry::changed_in`].
    #[serde(default, deserialize_with = "deserialize_present_raw")]
    pub aliased_to: Option<serde_norway::Value>,
    /// Entry-level provenance (`spec`/`section`/`table`/`quote`) recorded on
    /// some SFF-8024 and SFF-8636 entries. Retained raw; no consumer yet —
    /// declared so the key is vocabulary, not a silent drop.
    #[serde(default)]
    pub provenance: Option<serde_norway::Value>,
}

/// Presence-strict deserialization for `since`/`until`: an omitted key is
/// `None`; a present value must be a string. An explicit `null` (including
/// an empty YAML value, which is a null), number, bool, array or map is a
/// deserialization error — a plain `Option<String>` would collapse a
/// present `null` to omission and let an invalid entry pass as unannotated.
fn deserialize_present_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_norway::Value::deserialize(deserializer)?;
    match value {
        serde_norway::Value::String(s) => Ok(Some(s)),
        other => Err(serde::de::Error::custom(format!(
            "expected a quoted revision string like \"4.0\", got {other:?}"
        ))),
    }
}

/// Presence-preserving deserialization for the unsupported
/// vocabulary keys: any present value — including an explicit `null` or an
/// empty YAML value — becomes `Some(Value)` so the validator rejects the
/// key's presence instead of letting it vanish into `None`.
fn deserialize_present_raw<'de, D>(deserializer: D) -> Result<Option<serde_norway::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(serde_norway::Value::deserialize(deserializer)?))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalingInfo {
    pub lane_count: Option<u32>,
    pub lane_rate_gbd: Option<f64>,
    pub modulation: Option<String>,
    pub fec: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReachInfo {
    pub kind: Option<String>,
    pub distance_m: Option<f64>,
}
