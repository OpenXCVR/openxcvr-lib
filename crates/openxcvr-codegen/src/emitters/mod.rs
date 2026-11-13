pub mod c;
pub mod csharp;
pub mod go;
pub mod java;
pub mod markdown;
pub mod openapi;
pub mod php;
pub mod python;
pub mod rust;
pub mod typescript;

use crate::ir::enums::{EnumEntry, EnumSchema};
use crate::ir::naming::{NamingPolicy, RustNaming};
use crate::ir::pages::PageSchema;
use crate::ir::version::{resolve_enum_entries_for_rev, Rev};

pub trait Emitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String;
    fn emit_page(&self, schema: &PageSchema) -> String;
}

/// (raw value, canonical name) pairs of the single entries applicable at
/// one revision.
pub(crate) type ProjectionEntries = Vec<(u64, String)>;

/// Per-revision projection rows for a revision-annotated enum schema, shared
/// by the eight projection emitters (Rust, Python, Go, C, PHP, TypeScript,
/// Java, C#). For every revision in the family domain the schema's
/// `spec.id` names ([`crate::ir::version::family_revs_for_spec`] — CMIS
/// when the schema carries no spec block), the [`ProjectionEntries`] of
/// the applicable single entries — names rendered by the canonical naming
/// policy ([`RustNaming::enum_variant`],) so the literals are
/// identical in every language. Range entries never yield rows (they fold
/// into the caller's `Unknown<n>` catch-all).
///
/// Returns `None` when no entry carries `since`/`until` metadata: projection
/// emission is controlled by entry metadata, not the enum id, so
/// unannotated schemas emit nothing and stay byte-identical.
///
/// Precondition: the schema has passed `validate_enums` (rev strings are
/// trusted; invalid strings panic in the resolver, mirroring the field
/// resolver contract).
pub(crate) fn enum_projection_rows(schema: &EnumSchema) -> Option<Vec<(Rev, ProjectionEntries)>> {
    if !schema
        .entries
        .iter()
        .any(|entry| entry.since.is_some() || entry.until.is_some())
    {
        return None;
    }
    let naming = RustNaming;
    let family_revs =
        crate::ir::version::family_revs_for_spec(schema.spec.as_ref().map(|s| s.id.as_str()));
    Some(
        family_revs
            .iter()
            .copied()
            .map(|rev| {
                (
                    rev,
                    resolve_enum_entries_for_rev(schema, rev)
                        .into_iter()
                        .map(|entry| {
                            (
                                entry
                                    .value
                                    .as_single()
                                    .expect("resolver yields single entries only"),
                                naming.enum_variant(&entry.name),
                            )
                        })
                        .collect::<ProjectionEntries>(),
                )
            })
            .collect(),
    )
}

/// The emitted parameter name for a family's revision byte in projection
/// surfaces (`for_revision` / `from_rev`): `cmis_rev_byte` for the CMIS
/// family (byte-identical to the historical output), `sff_rev_byte` for the
/// SFF families, whose byte is a compliance code rather than a packed-BCD
/// major revision.
pub(crate) fn rev_byte_param(schema: &EnumSchema) -> &'static str {
    match schema.spec.as_ref().map(|s| s.id.as_str()) {
        Some("sff-8636") | Some("sff-8472") => "sff_rev_byte",
        _ => "cmis_rev_byte",
    }
}

/// CamelCase spelling of [`rev_byte_param`] for the camel-cased languages
/// (Go, TypeScript, Java, C#): `cmisRevByte` / `sffRevByte`.
pub(crate) fn rev_byte_param_camel(schema: &EnumSchema) -> &'static str {
    match schema.spec.as_ref().map(|s| s.id.as_str()) {
        Some("sff-8636") | Some("sff-8472") => "sffRevByte",
        _ => "cmisRevByte",
    }
}

/// The recorded revision-applicability statement for an enum schema, plus the
/// entries whose `since`/`until` windows it summarizes. The statement text is
/// shared verbatim by the Markdown reference, the Rust rustdoc emitter and
/// the OpenAPI emitter, so every surface states the same recorded fact in
/// the same words; each surface formats the per-entry windows itself.
///
/// Unannotated enums keep the canonical-catalog disclosure: revision is not
/// modeled for them, and nothing implies the enum applies to every revision.
pub(crate) fn enum_revision_applicability(schema: &EnumSchema) -> (String, Vec<&EnumEntry>) {
    let annotated: Vec<&EnumEntry> = schema
        .entries
        .iter()
        .filter(|e| e.since.is_some() || e.until.is_some())
        .collect();
    if annotated.is_empty() {
        (
            "Revision applicability: not modeled — this enum is a canonical catalog, not a \
             revision-qualified contract; the enum IR carries no revision metadata, and \
             revision annotations in the source schema are not interpreted"
                .to_string(),
            annotated,
        )
    } else {
        let domain_revs =
            crate::ir::version::family_revs_for_spec(schema.spec.as_ref().map(|s| s.id.as_str()));
        let domain = domain_revs
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let family = match schema.spec.as_ref().map(|s| s.id.as_str()) {
            Some("sff-8636") => "SFF-8636",
            Some("sff-8472") => "SFF-8472",
            _ => "CMIS",
        };
        (
            format!(
                "Revision applicability: revision-qualified — entry revision windows are \
                 interpreted within the known {family} family domain ({domain}); entries without a \
                 window apply throughout that domain only, never outside it"
            ),
            annotated,
        )
    }
}

/// Shared synthetic fixtures for the eight emitter projection tests. Both
/// builders validate the synthetic IR through the real central gate before
/// the emitter sees it, exactly like the production driver does.
#[cfg(test)]
pub(crate) mod test_support {
    use crate::ir::enums::{EntryValue, EnumEntry, EnumSchema};
    use crate::ir::pages::SpecRef;
    use crate::validator::validate_enums;

    fn entry(name: &str, value: u64, since: Option<&str>) -> EnumEntry {
        EnumEntry {
            name: name.to_string(),
            value: EntryValue::Single(value),
            description: None,
            aliases: vec![],
            tags: vec![],
            signaling: None,
            reach: None,
            bit_rate_gbps: None,
            vendor_specific: None,
            since: since.map(str::to_string),
            until: None,
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
            provenance: None,
        }
    }

    fn range_entry(name: &str, start: u64, end: u64) -> EnumEntry {
        EnumEntry {
            name: name.to_string(),
            value: EntryValue::Range { start, end },
            description: None,
            aliases: vec![],
            tags: vec![],
            signaling: None,
            reach: None,
            bit_rate_gbps: None,
            vendor_specific: None,
            since: None,
            until: None,
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
            provenance: None,
        }
    }

    fn dpsm_shaped(with_metadata: bool) -> EnumSchema {
        let since = |name: &str, value: u64| {
            entry(name, value, if with_metadata { Some("4.0") } else { None })
        };
        EnumSchema {
            schema: "openxcvr.enum.v2".to_string(),
            kind: "enum".to_string(),
            // Same id in both fixtures: emission must be driven by entry
            // metadata, never by the enum id.
            id: "CmisDataPathState".to_string(),
            title: "Test".to_string(),
            entry_includes: vec![],
            entries: vec![
                entry("reserved_0", 0x0, None),
                entry("dp_deactivated", 0x1, None),
                entry("dp_init", 0x2, None),
                entry("dp_deinit", 0x3, None),
                entry("dp_activated", 0x4, None),
                since("dp_tx_turn_on", 0x5),
                since("dp_tx_turn_off", 0x6),
                since("dp_initialized", 0x7),
                range_entry("reserved_8_f", 0x8, 0xf),
            ],
            spec: Some(SpecRef {
                id: "cmis".to_string(),
                revision: Some("5.3".to_string()),
                errata: vec![],
                source: None,
                parity_deferred: None,
            }),
            provenance: None,
            description: None,
        }
    }

    /// DPSM-shaped schema with `since: "4.0"` on singletons 5/6/7, validated
    /// through `validate_enums` before any direct emission.
    pub(crate) fn validated_dpsm_schema() -> EnumSchema {
        let schema = dpsm_shaped(true);
        validate_enums(std::slice::from_ref(&schema)).expect("synthetic DPSM schema must validate");
        schema
    }

    /// Same id, same entries, no revision metadata: must emit no projection.
    pub(crate) fn validated_unannotated_schema() -> EnumSchema {
        let schema = dpsm_shaped(false);
        validate_enums(std::slice::from_ref(&schema))
            .expect("synthetic unannotated schema must validate");
        schema
    }

    /// Range-only schema (no single values) whose first entry carries
    /// metadata: covers the range-only emission paths, where an appended
    /// projection must not be silently suppressed by an early return.
    pub(crate) fn validated_range_only_annotated_schema() -> EnumSchema {
        let mut schema = dpsm_shaped(true);
        schema.entries = vec![
            EnumEntry {
                since: Some("4.0".to_string()),
                provenance: None,
                ..range_entry("reserved_8_f", 0x08, 0x0f)
            },
            range_entry("vendor", 0x40, 0xff),
        ];
        validate_enums(std::slice::from_ref(&schema))
            .expect("synthetic range-only schema must validate");
        schema
    }
}
