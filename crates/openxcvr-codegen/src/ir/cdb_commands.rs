use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use serde_norway::Value;

use crate::ir::pages::{Access, FieldFlags, FieldType, SpecRef};

pub fn deserialize_hex_u16<'de, D>(d: D) -> std::result::Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    struct HexVisitor;
    impl<'de> Visitor<'de> for HexVisitor {
        type Value = u16;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "an integer or hex string")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<u16, E> {
            u16::try_from(v).map_err(E::custom)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<u16, E> {
            u16::try_from(v).map_err(E::custom)
        }
        fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<u16, E> {
            let s = s.trim();
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u16::from_str_radix(hex, 16).map_err(|e| E::custom(format!("bad hex {s:?}: {e}")))
            } else if let Some(hex) = s.strip_suffix('h').or_else(|| s.strip_suffix('H')) {
                u16::from_str_radix(hex, 16).map_err(|e| E::custom(format!("bad hex {s:?}: {e}")))
            } else {
                s.parse::<u16>()
                    .map_err(|e| E::custom(format!("bad int {s:?}: {e}")))
            }
        }
    }
    d.deserialize_any(HexVisitor)
}

pub fn deserialize_opt_hex_u16<'de, D>(d: D) -> std::result::Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct HexWrap(#[serde(deserialize_with = "deserialize_hex_u16")] u16);

    let opt = Option::<HexWrap>::deserialize(d)?;
    Ok(opt.map(|w| w.0))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdbLength {
    Fixed(u32),
    Variable,
}

impl<'de> Deserialize<'de> for CdbLength {
    fn deserialize<D>(d: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LengthVisitor;
        impl<'de> Visitor<'de> for LengthVisitor {
            type Value = CdbLength;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "an integer or the string \"variable\"")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<CdbLength, E> {
                u32::try_from(v).map(CdbLength::Fixed).map_err(E::custom)
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<CdbLength, E> {
                u32::try_from(v).map(CdbLength::Fixed).map_err(E::custom)
            }
            fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<CdbLength, E> {
                if s.eq_ignore_ascii_case("variable") {
                    Ok(CdbLength::Variable)
                } else if let Ok(val) = s.parse::<u32>() {
                    Ok(CdbLength::Fixed(val))
                } else {
                    Err(E::custom(format!(
                        "expected integer or \"variable\", got {s:?}"
                    )))
                }
            }
        }
        d.deserialize_any(LengthVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdbCommandGroup {
    CapabilitiesInquiry,
    ApplicationInformation,
    DataRecording,
    DiagnosticsDebug,
    FirmwareManagement,
    PerformanceMonitoring,
    Security,
    ModuleCommands,
    BulkReadPages,
    BulkWritePages,
    Bert,
    VersatileControlSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdbCommandClass {
    Required,
    ConditionallyRequired,
    Advertised,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdbProcessingMode {
    Foreground,
    Configurable,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdbTimeSource {
    Advertised,
    Variable,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbCommandSchema {
    pub schema: String,
    pub kind: String,
    pub id: String,
    pub title: String,
    pub spec: SpecRef,
    pub provenance: CdbProvenance,
    #[serde(deserialize_with = "deserialize_hex_u16")]
    pub cmd_id: u16,
    pub available_since: String,
    /// Upper window bound for commands removed by a later CMIS revision.
    /// Only used today by CMD 0281h (reserved placeholder in 4.0..=5.3,
    /// removed by CMIS 5.4). Absent means the command is still defined in
    /// the newest revision the tree models.
    #[serde(default)]
    pub available_until: Option<String>,
    pub cmd_id_group: CdbCommandGroup,
    pub command_class: CdbCommandClass,
    pub description: String,
    #[serde(default)]
    pub advertisement: Option<CdbAdvertisement>,
    #[serde(default)]
    pub preconditions: Vec<CdbPrecondition>,
    pub max_completion_time: CdbMaxCompletionTime,
    pub processing_mode: CdbProcessingMode,
    #[serde(default)]
    pub length_extensibility: Option<CdbLengthExtensibility>,
    pub cmd: CdbBody,
    pub reply: CdbReply,
    pub behavior: CdbBehavior,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbProvenance {
    pub spec: String,
    #[serde(default)]
    pub pdf_pages: Vec<u32>,
    pub table: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbAdvertisement {
    #[serde(default)]
    pub bit: Option<String>,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_hex_u16")]
    pub via_cmd: Option<u16>,
    #[serde(default)]
    pub field: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbPrecondition {
    pub field: String,
    #[serde(default)]
    pub equals: Option<Value>,
    #[serde(default)]
    pub in_range: Option<Vec<i64>>,
    #[serde(default)]
    pub in_set: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbMaxCompletionTime {
    pub source: CdbTimeSource,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbBody {
    pub lpl: CdbSegment,
    #[serde(default)]
    pub epl: Option<CdbSegment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbReply {
    pub status_codes: Vec<CdbStatus>,
    #[serde(default)]
    pub detailed_status_codes: Option<Vec<CdbStatus>>,
    pub lpl: CdbSegment,
    #[serde(default)]
    pub epl: Option<CdbSegment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbSegment {
    pub length: CdbLength,
    pub fields: Vec<CdbField>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbStatus {
    #[serde(deserialize_with = "deserialize_hex_u16")]
    pub value: u16,
    pub name: String,
    pub meaning: String,
    pub since: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbField {
    pub name: String,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub bit_offset: Option<u32>,
    #[serde(default)]
    pub bit_width: Option<u32>,
    #[serde(default)]
    pub size_bytes: Option<u32>,
    #[serde(rename = "type")]
    pub field_type_ir: FieldType,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub field_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub access: Option<Access>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub composite_ref: Option<String>,
    #[serde(default)]
    pub epl_page: Option<String>,
    #[serde(default)]
    pub length_from: Option<String>,
    #[serde(default)]
    pub provenance: Option<CdbFieldProvenance>,
    #[serde(default)]
    pub scope: Option<CdbScope>,
    #[serde(default)]
    pub encoding: Option<CdbEncoding>,
    #[serde(default)]
    pub enum_inline: Option<Vec<CdbEnumInline>>,
    #[serde(default)]
    pub constraints: Option<CdbConstraints>,
    #[serde(default)]
    pub behavior: Option<CdbFieldBehavior>,
    #[serde(default)]
    pub flags: Option<FieldFlags>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbFieldProvenance {
    pub spec: String,
    pub table: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbScope {
    #[serde(default)]
    pub data_path: Option<String>,
    #[serde(default)]
    pub cdb_instance: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbEncoding {
    pub kind: String,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub record_lengths_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    #[serde(default)]
    pub lsb_units: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbEnumInline {
    pub value: Value,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbConstraints {
    #[serde(default)]
    pub equals: Option<i64>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbFieldBehavior {
    #[serde(default)]
    pub bit_mapping: Option<String>,
    #[serde(default)]
    pub required_commands_may_be_unset: Option<String>,
    #[serde(default)]
    pub bits: Option<BTreeMap<u32, String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbBehavior {
    pub abort_supported: bool,
    pub retry_safe: bool,
    #[serde(default)]
    pub side_effects: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdbLengthExtensibility {
    pub cmd_extensible: bool,
    pub reply_extensible: bool,
}
