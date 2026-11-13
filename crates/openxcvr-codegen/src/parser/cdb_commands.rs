//! Parser for `openxcvr.cdb_command.v1` schemas.

use crate::ir::cdb_commands::CdbCommandSchema;
use anyhow::{Context, Result};
use std::path::Path;

pub const ALLOWED_CDB_COMMAND_SCHEMAS: &[&str] = &["openxcvr.cdb_command.v1"];

pub fn parse_cdb_command_schema(yaml: &str) -> Result<CdbCommandSchema> {
    let schema: CdbCommandSchema = serde_norway::from_str(yaml)?;
    if !ALLOWED_CDB_COMMAND_SCHEMAS.contains(&schema.schema.as_str()) {
        anyhow::bail!(
            "unsupported cdb_command schema namespace: {}; allowed: openxcvr.cdb_command.v1",
            schema.schema
        );
    }
    Ok(schema)
}

pub fn load_cdb_command_schema(path: &Path) -> Result<CdbCommandSchema> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_cdb_command_schema(&content)
        .with_context(|| format!("failed to parse cdb_command schema from {}", path.display()))
}
