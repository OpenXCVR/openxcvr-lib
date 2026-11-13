use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use crate::ir::cdb_commands::deserialize_hex_u16;
use crate::ir::pages::{Access, FieldFlags, SpecRef};

pub fn deserialize_opt_hex_u16_vec<'de, D>(d: D) -> std::result::Result<Option<Vec<u16>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct HexWrap(#[serde(deserialize_with = "deserialize_hex_u16")] u16);

    let opt = Option::<Vec<HexWrap>>::deserialize(d)?;
    Ok(opt.map(|v| v.into_iter().map(|w| w.0).collect()))
}

pub fn deserialize_opt_hex_or_dec_i64<'de, D>(d: D) -> std::result::Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct IntVisitor;
    impl<'de> Visitor<'de> for IntVisitor {
        type Value = Option<i64>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "an integer or hex string")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Option<i64>, E> {
            i64::try_from(v).map(Some).map_err(E::custom)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Option<i64>, E> {
            Ok(Some(v))
        }
        fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<Option<i64>, E> {
            let s = s.trim();
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                i64::from_str_radix(hex, 16)
                    .map(Some)
                    .map_err(|e| E::custom(format!("bad hex {s:?}: {e}")))
            } else {
                s.parse::<i64>()
                    .map(Some)
                    .map_err(|e| E::custom(format!("bad int {s:?}: {e}")))
            }
        }
        fn visit_none<E: de::Error>(self) -> std::result::Result<Option<i64>, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(
            self,
            d2: D2,
        ) -> std::result::Result<Option<i64>, D2::Error> {
            d2.deserialize_any(IntVisitor)
        }
    }
    d.deserialize_option(IntVisitor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TomFieldType {
    Bool,
    U2,
    U3,
    U4,
    U5,
    U6,
    U7,
    U8,
    #[serde(rename = "u16be")]
    U16Be,
    U24,
    Bitmap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TomConstraintType {
    FixedValue,
    Range,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomMessageSchema {
    pub schema: String,
    pub kind: String,
    pub id: String,
    pub title: String,
    pub spec: SpecRef,
    pub provenance: TomProvenance,
    #[serde(
        default,
        deserialize_with = "crate::ir::cdb_commands::deserialize_opt_hex_u16"
    )]
    pub tom_code: Option<u16>,
    #[serde(default, deserialize_with = "deserialize_opt_hex_u16_vec")]
    pub tom_code_set: Option<Vec<u16>>,
    pub description: String,
    pub msg: TomMsg,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomProvenance {
    pub spec: String,
    #[serde(default)]
    pub pdf_pages: Option<Vec<u32>>,
    #[serde(default)]
    pub figure: Option<String>,
    #[serde(default)]
    pub figures: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomMsg {
    pub length_bits: u32,
    pub fields: Vec<TomField>,
    #[serde(default)]
    pub variants: Option<TomVariants>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomVariants {
    pub discriminator: TomDiscriminator,
    pub cases: Vec<TomCase>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomDiscriminator {
    pub field: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomCase {
    pub value: u8,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub provenance: Option<TomCaseProvenance>,
    pub fields: Vec<TomField>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomCaseProvenance {
    pub section: String,
    #[serde(default)]
    pub pdf_pages: Option<Vec<u32>>,
    #[serde(default)]
    pub figure: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomField {
    pub name: String,
    pub offset_bits: u32,
    pub size_bits: u32,
    #[serde(rename = "type")]
    pub field_type_ir: TomFieldType,
    pub access: Access,
    pub field_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub flags: FieldFlags,
    pub provenance: TomFieldProvenance,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub constraints: Option<Vec<TomConstraint>>,
    #[serde(default)]
    pub encoding: Option<TomEncoding>,
    #[serde(default)]
    pub scope: Option<TomScope>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomFieldProvenance {
    pub section: String,
    pub quote: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomConstraint {
    #[serde(rename = "type")]
    pub constraint_type: TomConstraintType,
    #[serde(default, deserialize_with = "deserialize_opt_hex_or_dec_i64")]
    pub value: Option<i64>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomEncoding {
    pub kind: String,
    #[serde(default)]
    pub values: Option<Vec<TomEnumValue>>,
    #[serde(default)]
    pub lsb_units: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomEnumValue {
    pub value: i64,
    pub name: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomScope {
    #[serde(default)]
    pub module: Option<bool>,
}
