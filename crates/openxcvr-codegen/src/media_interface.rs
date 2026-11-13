//! Parser and planner for media interface variants.
//!
//! Case values accept YAML integers and integer strings (decimal or 0x hex) in the range 0..255.

use std::collections::{BTreeMap, HashMap};

use anyhow::{bail, Context, Result};

use crate::ir::enums::{EntryValue, EnumSchema};
use crate::ir::pages::PageSchema;

#[allow(clippy::type_complexity)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInterfacePlan {
    pub tables: Vec<(
        u8,
        String, /*enum id*/
        Vec<(u8, u8, String) /*lo, hi, name*/>,
    )>,
}

impl MediaInterfacePlan {
    pub fn resolve(&self, media_type: u8, code: u8) -> Option<&str> {
        for (mt, _enum_id, entries) in &self.tables {
            if *mt == media_type {
                for (lo, hi, name) in entries {
                    if code >= *lo && code <= *hi {
                        return Some(name.as_str());
                    }
                }
                return None;
            }
        }
        None
    }
}

pub fn plan_from_schemas(
    pages: &[PageSchema],
    enums: &[EnumSchema],
) -> Result<Option<MediaInterfacePlan>> {
    let mut declarations = Vec::new();
    let mut pages_missing_variants = Vec::new();
    let mut has_media_interface_field = false;

    for page in pages {
        for field in &page.fields {
            if field.name == "media_interface_id_app" {
                has_media_interface_field = true;
                if let Some(ref variants_val) = field.variants {
                    let decl = parse_declaration(&page.id, &field.name, variants_val)?;
                    declarations.push((page.id.clone(), decl));
                } else {
                    pages_missing_variants.push(page.id.clone());
                }
            }
        }
    }

    if !has_media_interface_field {
        return Ok(None);
    }

    if !pages_missing_variants.is_empty() {
        let missing_page = &pages_missing_variants[0];
        bail!("page '{missing_page}' field 'media_interface_id_app': field is missing 'variants'");
    }

    if declarations.is_empty() {
        return Ok(None);
    }

    // Check that declarations agree after normalizing media_type and lower_memory.media_type
    let (first_page, first_decl) = &declarations[0];
    for (page_id, decl) in &declarations[1..] {
        if decl != first_decl {
            bail!(
                "page '{page_id}' field 'media_interface_id_app': declarations disagree with page '{first_page}'"
            );
        }
    }

    // Build plan from first_decl
    let enum_map: HashMap<&str, &EnumSchema> = enums.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut tables = Vec::new();
    for (&case_val, enum_id) in &first_decl.cases {
        let enum_schema = enum_map.get(enum_id.as_str()).ok_or_else(|| {
            anyhow::anyhow!(
                "page '{first_page}' field 'media_interface_id_app': referenced enum '{enum_id}' not found"
            )
        })?;

        let mut entries: Vec<(u8, u8, String)> = Vec::new();
        for entry in &enum_schema.entries {
            if entry.name.is_empty()
                || !entry
                    .name
                    .chars()
                    .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
            {
                bail!(
                    "page '{first_page}' field 'media_interface_id_app': enum '{enum_id}' entry '{}': name does not match ^[a-z0-9_]+$",
                    entry.name
                );
            }
            match &entry.value {
                EntryValue::Single(v) => {
                    if *v > 255 {
                        bail!(
                            "page '{first_page}' field 'media_interface_id_app': enum '{enum_id}' entry '{}': value {v} is outside range 0..255",
                            entry.name
                        );
                    }
                    let byte_val = *v as u8;
                    entries.push((byte_val, byte_val, entry.name.clone()));
                }
                EntryValue::Range { start, end } => {
                    if start > end {
                        bail!(
                            "page '{first_page}' field 'media_interface_id_app': enum '{enum_id}' entry '{}': range lo > hi ({start} > {end})",
                            entry.name
                        );
                    }
                    if *start > 255 || *end > 255 {
                        bail!(
                            "page '{first_page}' field 'media_interface_id_app': enum '{enum_id}' entry '{}': range bound > 255 ({start}..{end})",
                            entry.name
                        );
                    }
                    entries.push((*start as u8, *end as u8, entry.name.clone()));
                }
            }
        }

        // Check for duplicate / overlapping codes covering the same code
        let mut covered = [0u8; 256];
        for (lo, hi, name) in &entries {
            for c in *lo..=*hi {
                if covered[c as usize] > 0 {
                    bail!(
                        "page '{first_page}' field 'media_interface_id_app': enum '{enum_id}' has overlapping entries covering the same code 0x{c:02x} ('{name}')"
                    );
                }
                covered[c as usize] += 1;
            }
        }

        entries.sort_by_key(|(lo, hi, _)| (*lo, *hi));
        tables.push((case_val, enum_id.clone(), entries));
    }

    tables.sort_by_key(|(val, _, _)| *val);

    Ok(Some(MediaInterfacePlan { tables }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedDeclaration {
    discriminator: String,
    cases: BTreeMap<u8, String>,
}

fn parse_declaration(
    page_id: &str,
    field_name: &str,
    val: &serde_norway::Value,
) -> Result<NormalizedDeclaration> {
    let map = val.as_mapping().ok_or_else(|| {
        anyhow::anyhow!("page '{page_id}' field '{field_name}': variants must be a mapping")
    })?;

    for (k, _) in map {
        let key_str = k.as_str().ok_or_else(|| {
            anyhow::anyhow!("page '{page_id}' field '{field_name}': variants key must be a string")
        })?;
        if key_str != "discriminator" && key_str != "cases" {
            bail!(
                "page '{page_id}' field '{field_name}': unexpected key '{key_str}' in variants (only 'discriminator' and 'cases' accepted)"
            );
        }
    }

    // Discriminator
    let disc_val = map.get("discriminator").ok_or_else(|| {
        anyhow::anyhow!("page '{page_id}' field '{field_name}': variants missing 'discriminator'")
    })?;
    let disc_map = disc_val.as_mapping().ok_or_else(|| {
        anyhow::anyhow!("page '{page_id}' field '{field_name}': discriminator must be a mapping")
    })?;

    for (k, _) in disc_map {
        let key_str = k.as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "page '{page_id}' field '{field_name}': discriminator key must be a string"
            )
        })?;
        if key_str != "field" {
            bail!(
                "page '{page_id}' field '{field_name}': unexpected key '{key_str}' in discriminator (only 'field' accepted)"
            );
        }
    }

    let field_val = disc_map
        .get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "page '{page_id}' field '{field_name}': discriminator missing string 'field'"
            )
        })?;

    let normalized_disc = match field_val {
        "media_type" | "lower_memory.media_type" => "media_type".to_string(),
        other => {
            bail!(
                "page '{page_id}' field '{field_name}': unsupported discriminator field '{other}'"
            );
        }
    };

    // Cases
    let cases_val = map.get("cases").ok_or_else(|| {
        anyhow::anyhow!("page '{page_id}' field '{field_name}': variants missing 'cases'")
    })?;

    let mut cases = BTreeMap::new();
    if let Some(seq) = cases_val.as_sequence() {
        for case_item in seq {
            let item_map = case_item.as_mapping().ok_or_else(|| {
                anyhow::anyhow!(
                    "page '{page_id}' field '{field_name}': case item must be a mapping"
                )
            })?;

            for (k, _) in item_map {
                let key_str = k.as_str().ok_or_else(|| {
                    anyhow::anyhow!(
                        "page '{page_id}' field '{field_name}': case item key must be a string"
                    )
                })?;
                if key_str != "when" && key_str != "enum_ref" {
                    bail!(
                        "page '{page_id}' field '{field_name}': unexpected key '{key_str}' in case item (only 'when' and 'enum_ref' accepted)"
                    );
                }
            }

            let when_val = item_map
                .get("when")
                .and_then(|v| v.as_mapping())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "page '{page_id}' field '{field_name}': case item missing 'when' mapping"
                    )
                })?;

            for (k, _) in when_val {
                let key_str = k.as_str().ok_or_else(|| {
                    anyhow::anyhow!(
                        "page '{page_id}' field '{field_name}': case 'when' key must be a string"
                    )
                })?;
                if key_str != "value" {
                    bail!(
                        "page '{page_id}' field '{field_name}': unexpected key '{key_str}' in case 'when' (only 'value' accepted)"
                    );
                }
            }

            let raw_val = when_val.get("value").ok_or_else(|| {
                anyhow::anyhow!(
                    "page '{page_id}' field '{field_name}': case 'when' missing 'value'"
                )
            })?;
            let byte_val = parse_u8_value(raw_val).with_context(|| {
                format!(
                    "page '{page_id}' field '{field_name}': case value not an integer in 0..255"
                )
            })?;
            let enum_ref = item_map
                .get("enum_ref")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "page '{page_id}' field '{field_name}': case item missing string 'enum_ref'"
                    )
                })?;

            if cases.contains_key(&byte_val) {
                bail!(
                    "page '{page_id}' field '{field_name}': duplicate case value 0x{byte_val:02x}"
                );
            }
            cases.insert(byte_val, enum_ref.to_string());
        }
    } else if let Some(case_map) = cases_val.as_mapping() {
        for (k, v) in case_map {
            let byte_val = parse_u8_value(k).with_context(|| {
                format!(
                    "page '{page_id}' field '{field_name}': case value not an integer in 0..255"
                )
            })?;
            let enum_ref = if let Some(s) = v.as_str() {
                s.to_string()
            } else if let Some(m) = v.as_mapping() {
                for (mk, _) in m {
                    let key_str = mk.as_str().ok_or_else(|| {
                        anyhow::anyhow!(
                            "page '{page_id}' field '{field_name}': case mapping key must be a string"
                        )
                    })?;
                    if key_str != "enum_ref" {
                        bail!(
                            "page '{page_id}' field '{field_name}': unexpected key '{key_str}' in case mapping (only 'enum_ref' accepted)"
                        );
                    }
                }
                m.get("enum_ref")
                    .and_then(|e| e.as_str())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "page '{page_id}' field '{field_name}': case mapping missing 'enum_ref'"
                        )
                    })?
                    .to_string()
            } else {
                bail!("page '{page_id}' field '{field_name}': unexpected value in cases mapping");
            };
            if cases.contains_key(&byte_val) {
                bail!(
                    "page '{page_id}' field '{field_name}': duplicate case value 0x{byte_val:02x}"
                );
            }
            cases.insert(byte_val, enum_ref);
        }
    } else {
        bail!("page '{page_id}' field '{field_name}': cases must be a sequence or mapping");
    }

    Ok(NormalizedDeclaration {
        discriminator: normalized_disc,
        cases,
    })
}

fn parse_u8_value(val: &serde_norway::Value) -> Result<u8> {
    if let Some(n) = val.as_i64() {
        if (0..=255).contains(&n) {
            return Ok(n as u8);
        } else {
            bail!("integer {n} outside 0..255");
        }
    }
    if let Some(n) = val.as_u64() {
        if n <= 255 {
            return Ok(n as u8);
        } else {
            bail!("integer {n} outside 0..255");
        }
    }
    if let Some(s) = val.as_str() {
        let s = s.trim();
        let n = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16)?
        } else {
            s.parse::<u64>()?
        };
        if n <= 255 {
            return Ok(n as u8);
        } else {
            bail!("integer {n} outside 0..255");
        }
    }
    bail!("not an integer or string");
}
