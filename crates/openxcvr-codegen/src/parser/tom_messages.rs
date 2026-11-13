//! Parser for `openxcvr.tom_message.v1` schemas.

use crate::ir::tom_messages::TomMessageSchema;
use anyhow::{Context, Result};
use std::path::Path;

pub const ALLOWED_TOM_MESSAGE_SCHEMAS: &[&str] = &["openxcvr.tom_message.v1"];

pub fn parse_tom_message_schema(yaml: &str) -> Result<TomMessageSchema> {
    let schema: TomMessageSchema = serde_norway::from_str(yaml)?;
    if !ALLOWED_TOM_MESSAGE_SCHEMAS.contains(&schema.schema.as_str()) {
        anyhow::bail!(
            "unsupported tom_message schema namespace: {}; allowed: openxcvr.tom_message.v1",
            schema.schema
        );
    }
    Ok(schema)
}

pub fn load_tom_message_schema(path: &Path) -> Result<TomMessageSchema> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_tom_message_schema(&content)
        .with_context(|| format!("failed to parse tom_message schema from {}", path.display()))
}
