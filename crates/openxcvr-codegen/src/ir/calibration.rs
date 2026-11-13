//! Scaffold IR for `openxcvr.calibration.v1` schemas.
//!
//! Note on status: ("Calibration Variants DSL") is superseded
//! and explicitly advised against a calibration-specific DSL, but
//! `schemas/sff-8472/calibration-internal-vs-external.yaml` remains in the tree as
//! the sole document in the `openxcvr.calibration.v1` namespace. No emitter
//! currently consumes this schema; this IR and its strict loader exist solely to
//! ensure complete vocabulary validation across all 247 schema files without
//! altering generation output or visible-gap tables.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use crate::ir::pages::{FieldType, SpecRef};

pub fn deserialize_hex_u32<'de, D>(d: D) -> std::result::Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    struct HexVisitor;
    impl<'de> Visitor<'de> for HexVisitor {
        type Value = u32;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "an integer or hex string")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<u32, E> {
            u32::try_from(v).map_err(E::custom)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<u32, E> {
            u32::try_from(v).map_err(E::custom)
        }
        fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<u32, E> {
            let s = s.trim();
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u32::from_str_radix(hex, 16).map_err(|e| E::custom(format!("bad hex {s:?}: {e}")))
            } else if let Some(hex) = s.strip_suffix('h').or_else(|| s.strip_suffix('H')) {
                u32::from_str_radix(hex, 16).map_err(|e| E::custom(format!("bad hex {s:?}: {e}")))
            } else {
                s.parse::<u32>()
                    .map_err(|e| E::custom(format!("bad int {s:?}: {e}")))
            }
        }
    }
    d.deserialize_any(HexVisitor)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationSchema {
    pub schema: String,
    pub kind: String,
    pub id: String,
    pub title: String,
    pub spec: SpecRef,
    pub provenance: CalProvenance,
    pub calibration_mode: CalibrationMode,
    pub internal_mode: InternalMode,
    pub external_mode: ExternalMode,
    pub conversion_formula: ConversionFormula,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalProvenance {
    pub spec: String,
    #[serde(default)]
    pub pdf_pages: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationMode {
    pub field: String,
    pub offset: u32,
    pub page_address: String,
    pub size_bytes: u32,
    pub bits: Vec<CalibrationModeBit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationModeBit {
    pub position: u8,
    #[serde(deserialize_with = "deserialize_hex_u32")]
    pub mask: u32,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InternalMode {
    pub description: String,
    pub a2h_bytes_56_94: String,
    pub telemetry_encoding: TelemetryEncodingMap,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryEncodingMap {
    pub temperature: TelemetryEncoding,
    pub supply_voltage: TelemetryEncoding,
    pub tx_bias: TelemetryEncoding,
    pub tx_power: TelemetryEncoding,
    pub rx_power: TelemetryEncoding,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryEncoding {
    #[serde(rename = "type")]
    pub field_type_ir: FieldType,
    pub lsb_units: String,
    pub raw_offset: u16,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMode {
    pub description: String,
    pub linear_constants: LinearConstants,
    pub rx_power_polynomial: RxPowerPolynomial,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearConstants {
    pub base_offset: u16,
    pub page_address: String,
    pub entries: Vec<LinearEntry>,
    pub total_size_bytes: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearEntry {
    pub name: String,
    pub index: u8,
    pub offset: u16,
    pub size_bytes: u16,
    pub layout: Vec<LinearLayoutEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearLayoutEntry {
    pub field: String,
    pub bytes: Vec<u8>,
    #[serde(rename = "type")]
    pub field_type_ir: FieldType,
    pub encoding: String,
    pub unit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RxPowerPolynomial {
    pub base_offset: u16,
    pub page_address: String,
    pub size_bytes: u16,
    pub coefficients: u8,
    pub coefficient_size: u8,
    pub encoding: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionFormula {
    pub linear: String,
    pub polynomial: String,
}
