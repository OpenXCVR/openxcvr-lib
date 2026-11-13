//! Parser for `openxcvr.calibration.v1` schemas.
//!
//! Note on status: ("Calibration Variants DSL") is superseded
//! and explicitly advised against a calibration-specific DSL, but
//! `schemas/sff-8472/calibration-internal-vs-external.yaml` remains in the tree as
//! the sole document in the `openxcvr.calibration.v1` namespace. No emitter
//! currently consumes this schema; this loader exists solely to ensure
//! complete vocabulary validation across all 247 schema files without
//! altering generation output or visible-gap tables.

use crate::ir::calibration::CalibrationSchema;
use anyhow::{Context, Result};
use std::path::Path;

pub const ALLOWED_CALIBRATION_SCHEMAS: &[&str] = &["openxcvr.calibration.v1"];

pub fn parse_calibration_schema(yaml: &str) -> Result<CalibrationSchema> {
    let schema: CalibrationSchema = serde_norway::from_str(yaml)?;
    if !ALLOWED_CALIBRATION_SCHEMAS.contains(&schema.schema.as_str()) {
        anyhow::bail!(
            "unsupported calibration schema namespace: {}; allowed: openxcvr.calibration.v1",
            schema.schema
        );
    }
    Ok(schema)
}

pub fn load_calibration_schema(path: &Path) -> Result<CalibrationSchema> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_calibration_schema(&content)
        .with_context(|| format!("failed to parse calibration schema from {}", path.display()))
}
