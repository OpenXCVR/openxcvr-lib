use std::collections::BTreeSet;
use std::str::FromStr;

use crate::ir::enums::{EnumEntry, EnumSchema};
use crate::ir::pages::{AliasedToEntry, PageField};

/// Error returned when a rev string does not match any known revision.
#[derive(Debug, thiserror::Error)]
#[error(
    "invalid revision string {value:?}; expected one of: 3.0, 4.0, 5.0, 5.1, 5.2, 5.3, 5.4 \
     (CMIS); 1.3, 1.4, 1.5, 2.0, 2.5, 2.8, 2.11, 2.12 (SFF-8636); \
     9.3, 9.5, 10.2, 10.4, 11.0, 11.3, 11.4, 12.3, 12.4, 12.5 (SFF-8472)"
)]
pub struct InvalidRevError {
    pub value: String,
}

/// A spec family with a modeled revision domain. Window resolution
/// (`applicable_revs` and friends) is always computed within ONE family's
/// domain; a rev string from another family never matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Cmis,
    Sff8636,
    Sff8472,
}

impl Family {
    /// The family's known-revision domain, in order. Used for
    /// `applicable_revs` computation and per-revision emission.
    pub fn revs(self) -> &'static [Rev] {
        match self {
            Family::Cmis => Rev::CMIS_ALL,
            Family::Sff8636 => Rev::SFF8636_ALL,
            Family::Sff8472 => Rev::SFF8472_ALL,
        }
    }

    /// Family of a spec directory id (`cmis`, `sff-8636`, `sff-8472`).
    /// Companion/unknown namespaces have no modeled revision domain.
    pub fn from_spec_id(spec_id: &str) -> Option<Family> {
        match spec_id {
            "cmis" => Some(Family::Cmis),
            "sff-8636" => Some(Family::Sff8636),
            "sff-8472" => Some(Family::Sff8472),
            _ => None,
        }
    }
}

/// The revision domain a schema with this spec id resolves its windows
/// against. `None` (no spec block, or a companion namespace) keeps the
/// status-quo CMIS domain so synthetic/union inputs behave exactly as
/// before; families with a modeled domain resolve within their own.
pub fn family_revs_for_spec(spec_id: Option<&str>) -> &'static [Rev] {
    spec_id
        .and_then(Family::from_spec_id)
        .map(Family::revs)
        .unwrap_or(Rev::CMIS_ALL)
}

/// Family-known revs, in order. Used for `applicable_revs` computation.
/// Declaration order is chronological within a family; cross-family
/// comparisons never occur (window resolution is always within one
/// family's domain slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rev {
    V3_0,
    V4_0,
    V5_0,
    V5_1,
    V5_2,
    V5_3,
    V5_4,
    Sff8636_1_3,
    Sff8636_1_4,
    Sff8636_1_5,
    Sff8636_2_0,
    Sff8636_2_5,
    Sff8636_2_8,
    Sff8636_2_11,
    Sff8636_2_12,
    Sff8472_9_3,
    Sff8472_9_5,
    Sff8472_10_2,
    Sff8472_10_4,
    Sff8472_11_0,
    Sff8472_11_3,
    Sff8472_11_4,
    Sff8472_12_3,
    Sff8472_12_4,
    Sff8472_12_5,
}

impl Rev {
    pub const CMIS_ALL: &[Rev] = &[
        Rev::V3_0,
        Rev::V4_0,
        Rev::V5_0,
        Rev::V5_1,
        Rev::V5_2,
        Rev::V5_3,
        Rev::V5_4,
    ];
    pub const SFF8636_ALL: &[Rev] = &[
        Rev::Sff8636_1_3,
        Rev::Sff8636_1_4,
        Rev::Sff8636_1_5,
        Rev::Sff8636_2_0,
        Rev::Sff8636_2_5,
        Rev::Sff8636_2_8,
        Rev::Sff8636_2_11,
        Rev::Sff8636_2_12,
    ];
    pub const SFF8472_ALL: &[Rev] = &[
        Rev::Sff8472_9_3,
        Rev::Sff8472_9_5,
        Rev::Sff8472_10_2,
        Rev::Sff8472_10_4,
        Rev::Sff8472_11_0,
        Rev::Sff8472_11_3,
        Rev::Sff8472_11_4,
        Rev::Sff8472_12_3,
        Rev::Sff8472_12_4,
        Rev::Sff8472_12_5,
    ];

    /// The family this revision belongs to.
    pub fn family(self) -> Family {
        match self {
            Rev::V3_0 | Rev::V4_0 | Rev::V5_0 | Rev::V5_1 | Rev::V5_2 | Rev::V5_3 | Rev::V5_4 => {
                Family::Cmis
            }
            Rev::Sff8636_1_3
            | Rev::Sff8636_1_4
            | Rev::Sff8636_1_5
            | Rev::Sff8636_2_0
            | Rev::Sff8636_2_5
            | Rev::Sff8636_2_8
            | Rev::Sff8636_2_11
            | Rev::Sff8636_2_12 => Family::Sff8636,
            Rev::Sff8472_9_3
            | Rev::Sff8472_9_5
            | Rev::Sff8472_10_2
            | Rev::Sff8472_10_4
            | Rev::Sff8472_11_0
            | Rev::Sff8472_11_3
            | Rev::Sff8472_11_4
            | Rev::Sff8472_12_3
            | Rev::Sff8472_12_4
            | Rev::Sff8472_12_5 => Family::Sff8472,
        }
    }

    /// Parse a revision string, returning an error for unrecognised values.
    ///
    /// Valid strings: CMIS "3.0"–"5.4", SFF-8636 "1.3"–"2.12",
    /// SFF-8472 "9.3"–"12.5" (family domains, see [`Family::revs`]).
    /// Typos like "v4.0", "4", "5.x" are rejected — § "Alternatives
    /// Considered" mandates that rev-string typos fail loud, not silently.
    /// Intermediate published revisions outside the emitted domains
    /// (e.g. SFF-8636 "1.9", "2.10") are rejected: annotations snap to
    /// the nearest admitted revision instead of naming an unemitted one.
    pub fn parse(s: &str) -> Result<Rev, InvalidRevError> {
        let rev = match s {
            "3.0" => Rev::V3_0,
            "4.0" => Rev::V4_0,
            "5.0" => Rev::V5_0,
            "5.1" => Rev::V5_1,
            "5.2" => Rev::V5_2,
            "5.3" => Rev::V5_3,
            "5.4" => Rev::V5_4,
            "1.3" => Rev::Sff8636_1_3,
            "1.4" => Rev::Sff8636_1_4,
            "1.5" => Rev::Sff8636_1_5,
            "2.0" => Rev::Sff8636_2_0,
            "2.5" => Rev::Sff8636_2_5,
            "2.8" => Rev::Sff8636_2_8,
            "2.11" => Rev::Sff8636_2_11,
            "2.12" => Rev::Sff8636_2_12,
            "9.3" => Rev::Sff8472_9_3,
            "9.5" => Rev::Sff8472_9_5,
            "10.2" => Rev::Sff8472_10_2,
            "10.4" => Rev::Sff8472_10_4,
            "11.0" => Rev::Sff8472_11_0,
            "11.3" => Rev::Sff8472_11_3,
            "11.4" => Rev::Sff8472_11_4,
            "12.3" => Rev::Sff8472_12_3,
            "12.4" => Rev::Sff8472_12_4,
            "12.5" => Rev::Sff8472_12_5,
            other => {
                return Err(InvalidRevError {
                    value: other.to_string(),
                })
            }
        };
        Ok(rev)
    }

    /// Canonical spec revision string ("5.3", "2.11", "12.5"); inverse of
    /// [`Rev::parse`].
    pub fn as_str(self) -> &'static str {
        match self {
            Rev::V3_0 => "3.0",
            Rev::V4_0 => "4.0",
            Rev::V5_0 => "5.0",
            Rev::V5_1 => "5.1",
            Rev::V5_2 => "5.2",
            Rev::V5_3 => "5.3",
            Rev::V5_4 => "5.4",
            Rev::Sff8636_1_3 => "1.3",
            Rev::Sff8636_1_4 => "1.4",
            Rev::Sff8636_1_5 => "1.5",
            Rev::Sff8636_2_0 => "2.0",
            Rev::Sff8636_2_5 => "2.5",
            Rev::Sff8636_2_8 => "2.8",
            Rev::Sff8636_2_11 => "2.11",
            Rev::Sff8636_2_12 => "2.12",
            Rev::Sff8472_9_3 => "9.3",
            Rev::Sff8472_9_5 => "9.5",
            Rev::Sff8472_10_2 => "10.2",
            Rev::Sff8472_10_4 => "10.4",
            Rev::Sff8472_11_0 => "11.0",
            Rev::Sff8472_11_3 => "11.3",
            Rev::Sff8472_11_4 => "11.4",
            Rev::Sff8472_12_3 => "12.3",
            Rev::Sff8472_12_4 => "12.4",
            Rev::Sff8472_12_5 => "12.5",
        }
    }

    /// Suffix of the per-revision output directory / module (`v{dirname}`):
    /// "5_3" for `generated/v5_3/` and `pub mod v5_3`.
    pub fn dirname(self) -> &'static str {
        match self {
            Rev::V3_0 => "3_0",
            Rev::V4_0 => "4_0",
            Rev::V5_0 => "5_0",
            Rev::V5_1 => "5_1",
            Rev::V5_2 => "5_2",
            Rev::V5_3 => "5_3",
            Rev::V5_4 => "5_4",
            Rev::Sff8636_1_3 => "1_3",
            Rev::Sff8636_1_4 => "1_4",
            Rev::Sff8636_1_5 => "1_5",
            Rev::Sff8636_2_0 => "2_0",
            Rev::Sff8636_2_5 => "2_5",
            Rev::Sff8636_2_8 => "2_8",
            Rev::Sff8636_2_11 => "2_11",
            Rev::Sff8636_2_12 => "2_12",
            Rev::Sff8472_9_3 => "9_3",
            Rev::Sff8472_9_5 => "9_5",
            Rev::Sff8472_10_2 => "10_2",
            Rev::Sff8472_10_4 => "10_4",
            Rev::Sff8472_11_0 => "11_0",
            Rev::Sff8472_11_3 => "11_3",
            Rev::Sff8472_11_4 => "11_4",
            Rev::Sff8472_12_3 => "12_3",
            Rev::Sff8472_12_4 => "12_4",
            Rev::Sff8472_12_5 => "12_5",
        }
    }

    /// The family-specific hardware revision byte this revision decodes
    /// from: the packed-BCD MajorRevision byte for CMIS (Lower:1), the
    /// revision-compliance code for SFF-8636 (Lower:1, Table 6-4) and the
    /// SFF-8472 compliance code (A0h:94, Table 8-8). Generated projection
    /// factories dispatch on exactly these bytes; the consumer side decodes
    /// them with `openxcvr_core::Rev::from_major_byte_cmis` /
    /// `from_compliance_byte_sff8636` / `from_compliance_byte_sff8472`.
    pub fn family_rev_byte(self) -> u8 {
        match self {
            Rev::V3_0 | Rev::V4_0 | Rev::V5_0 | Rev::V5_1 | Rev::V5_2 | Rev::V5_3 | Rev::V5_4 => {
                self.major_byte_cmis()
            }
            Rev::Sff8636_1_3 => 0x03,
            Rev::Sff8636_1_4 => 0x04,
            Rev::Sff8636_1_5 => 0x05,
            Rev::Sff8636_2_0 => 0x06,
            Rev::Sff8636_2_5 => 0x07,
            Rev::Sff8636_2_8 => 0x08,
            Rev::Sff8636_2_11 => 0x09,
            Rev::Sff8636_2_12 => 0x0A,
            Rev::Sff8472_9_3 => 0x01,
            Rev::Sff8472_9_5 => 0x02,
            Rev::Sff8472_10_2 => 0x03,
            Rev::Sff8472_10_4 => 0x04,
            Rev::Sff8472_11_0 => 0x05,
            Rev::Sff8472_11_3 => 0x06,
            Rev::Sff8472_11_4 => 0x07,
            Rev::Sff8472_12_3 => 0x08,
            Rev::Sff8472_12_4 => 0x09,
            Rev::Sff8472_12_5 => 0x0A,
        }
    }

    /// The packed-BCD revision byte this revision decodes from — major in the
    /// high nibble, minor in the low nibble (CMIS 5.3 § 8.2.1), e.g. `0x51`
    /// for 5.1. This is the same mapping `openxcvr_core::Rev::from_major_byte_cmis`
    /// decodes on the consumer side; generated revision-bound projection
    /// factories accept exactly these bytes. Derived from [`Rev::as_str`],
    /// not a hand-pasted table.
    pub fn major_byte_cmis(self) -> u8 {
        debug_assert!(matches!(self.family(), Family::Cmis));
        let s = self.as_str();
        let (major, minor) = s
            .split_once('.')
            .expect("Rev::as_str is always major.minor");
        let major: u8 = major.parse().expect("major nibble is numeric");
        let minor: u8 = minor.parse().expect("minor nibble is numeric");
        (major << 4) | minor
    }
}

impl FromStr for Rev {
    type Err = InvalidRevError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Rev::parse(s)
    }
}

/// Materialize per-rev overrides on a `PageField`. Returns a clone with:
///
/// - `name`: `renamed_from.old_name` when `rev < renamed_from.rev` (else current
///   name); then `aliased_to[i].new_name` for the highest `aliased_to[i].rev <=
///   rev` (if any — takes precedence over renamed_from).
/// - Partial `changed_in` overrides accumulate in revision order. Omitted keys
///   inherit the last applicable value, including permission/effect authority.
///
/// Caller must have already filtered the field through `applicable_revs`
/// (i.e. the field is present in `rev`).
///
/// # Precondition
/// All rev strings in `field.changed_in[].rev`, `field.renamed_from.rev`,
/// and `field.aliased_to[].rev` must have been validated by the parser
/// (which they are — see `crates/openxcvr-codegen/src/parser/pages.rs`).
/// Passing an invalid rev string will panic via `.expect()`, matching the
/// existing pattern in `applicable_revs`.
pub fn resolve_field_for_rev(field: &PageField, rev: Rev) -> PageField {
    let mut resolved = field.clone();

    // ── name resolution ──────────────────────────────────────────────────────
    // Override revs are family-qualified: an entry naming another family's
    // revision never applies (mirrors the family-qualified window matching
    // in `applicable_revs`).
    let same_family = |other: Rev, rev: Rev| other.family() == rev.family();
    // Step 1: apply renamed_from if rev is before the rename.
    if let Some(rf) = &field.renamed_from {
        let rename_rev = Rev::parse(&rf.rev).expect("renamed_from.rev validated at parse time");
        if same_family(rename_rev, rev) && rev < rename_rev {
            resolved.name = rf.old_name.clone();
        }
    }
    // Step 2: apply aliased_to (highest entry.rev <= rev wins; overrides rename).
    if let Some(aliases) = &field.aliased_to {
        let best: Option<&AliasedToEntry> = aliases
            .iter()
            .filter(|e| {
                let r = Rev::parse(&e.rev).expect("aliased_to.rev validated at parse time");
                same_family(r, rev) && r <= rev
            })
            .max_by_key(|e| Rev::parse(&e.rev).expect("aliased_to.rev validated at parse time"));
        if let Some(entry) = best {
            resolved.name = entry.new_name.clone();
        }
    }

    // ── type / enum_ref resolution ────────────────────────────────────────────
    if let Some(changes) = &field.changed_in {
        let mut applicable: Vec<_> = changes
            .iter()
            .filter(|e| {
                let r = Rev::parse(&e.rev).expect("changed_in.rev validated at parse time");
                same_family(r, rev) && r <= rev
            })
            .collect();
        applicable
            .sort_by_key(|e| Rev::parse(&e.rev).expect("changed_in.rev validated at parse time"));
        let mut permission_changed = false;
        let mut revision_setter_policy = None;
        for entry in applicable {
            if let Some(new_ty) = &entry.ty {
                resolved.ty = new_ty.clone();
            }
            if let Some(width) = entry.bit_width {
                resolved.bit_width = Some(width);
            }
            if entry.enum_inline.is_some() {
                resolved.enum_inline.clone_from(&entry.enum_inline);
            }
            if entry.enum_ref.is_some() {
                resolved.enum_ref.clone_from(&entry.enum_ref);
            }
            if let Some(access) = entry.access {
                resolved.access = Some(access);
                resolved.explicit_access = Some(access);
                revision_setter_policy = entry.setter_eligible;
                // Old base permission flags must not override new access.
                if let Some(flags) = &mut resolved.flags {
                    flags.is_readable = access.is_readable();
                    flags.is_writable = access.is_writable();
                }
                permission_changed = true;
            }
            if let Some(flags) = &entry.flags {
                resolved
                    .flags
                    .get_or_insert_with(Default::default)
                    .inherit(flags);
                permission_changed |=
                    flags.is_writable.is_some() || flags.is_ever_writable.is_some();
            }
            if entry.provenance.is_some() {
                resolved.provenance.clone_from(&entry.provenance);
            }
            if let Some(behavior) = &entry.behavior {
                crate::ir::pages::inherit_behavior(&mut resolved.behavior, behavior);
                // Only recognized latching assertions affect permission; reset
                // domains and other behavior must preserve the legacy API policy.
                permission_changed |= matches!(
                    resolved
                        .behavior
                        .as_ref()
                        .and_then(|b| b.get("latching"))
                        .and_then(|v| v.as_str()),
                    Some(
                        "clear_on_read"
                            | "clear_on_read_when_condition_clears"
                            | "implementation_defined"
                    )
                );
                resolved.normalize_legacy_access();
            }
            resolved.metadata.inherit(&entry.metadata);
        }
        if permission_changed {
            // Ever-writable opts into the legacy API, not permission: resolved
            // RO/COR, Unknown and explicit writable=false still veto a setter.
            resolved.setter_eligible = Some(
                resolved.writable() == Some(true)
                    && (revision_setter_policy.unwrap_or_else(|| {
                        resolved
                            .access
                            .is_some_and(crate::ir::pages::Access::emits_setter)
                    }) || resolved
                        .flags
                        .as_ref()
                        .is_some_and(|f| f.is_ever_writable == Some(true))),
            );
        }
    }

    resolved
}

/// Compute the set of revs in `family_revs` for which `field` is present.
///
/// - `since` (inclusive): field is absent before this rev.
/// - `until` (exclusive): field is absent at and after this rev.
/// - If neither is set, the field is present in all `family_revs`.
///
/// Returns an empty set if `family_revs` is empty.
///
/// # Precondition
/// All rev strings in `field.since` / `field.until` must have already been
/// validated by `parse_page_schema`. Passing
/// a field with an invalid rev string will panic via `.expect()`.
pub fn applicable_revs(field: &PageField, family_revs: &[Rev]) -> BTreeSet<Rev> {
    let since: Option<Rev> = field
        .since
        .as_deref()
        .map(|s| Rev::parse(s).expect("since was validated at parse time"));
    let until: Option<Rev> = field
        .until
        .as_deref()
        .map(|s| Rev::parse(s).expect("until was validated at parse time"));

    family_revs
        .iter()
        .copied()
        // A window endpoint from ANOTHER family never matches: comparisons
        // are family-qualified, so a cross-family rev string yields an empty
        // set (rejected by the validator's empty-window rule) instead of
        // silently overlapping a foreign domain.
        .filter(|r| {
            since.is_none_or(|s| s.family() == r.family() && *r >= s)
                && until.is_none_or(|u| u.family() == r.family() && *r < u)
        })
        .collect()
}

/// Compute the set of revs in `family_revs` for which an enum entry is
/// applicable. Mirrors [`applicable_revs`] for entries: `since` inclusive,
/// `until` exclusive, and an unannotated entry applies to every supplied
/// family revision — only within the known domain, never as a claim about a
/// revision outside it.
///
/// Returns an empty set if `family_revs` is empty.
///
/// # Precondition
/// All rev strings in `entry.since` / `entry.until` must have been validated
/// by a successful `validate_enums` run over the containing schema (there is
/// no parser-time validation for enum entries). Passing an entry with an
/// invalid rev string will panic via `.expect()`.
pub fn applicable_revs_for_entry(entry: &EnumEntry, family_revs: &[Rev]) -> BTreeSet<Rev> {
    let since: Option<Rev> = entry
        .since
        .as_deref()
        .map(|s| Rev::parse(s).expect("since was validated by validate_enums"));
    let until: Option<Rev> = entry
        .until
        .as_deref()
        .map(|s| Rev::parse(s).expect("until was validated by validate_enums"));

    family_revs
        .iter()
        .copied()
        // Family-qualified matching, as in `applicable_revs`.
        .filter(|r| {
            since.is_none_or(|s| s.family() == r.family() && *r >= s)
                && until.is_none_or(|u| u.family() == r.family() && *r < u)
        })
        .collect()
}

/// Resolve the applicable **single** entries of an enum schema at `rev`,
/// over the family domain the schema's `spec.id` names (CMIS when the
/// schema carries no spec block, matching the historical union-run
/// behavior). Range entries stay folded into the `Unknown<n>` catch-all
/// and are never returned as named states.
///
/// # Precondition
/// All rev strings in the schema's entry metadata must have been validated
/// by a successful `validate_enums` run over the schema (there is no
/// parser-time validation for enum entries). Invalid rev strings panic via
/// `.expect()` in [`applicable_revs_for_entry`].
pub fn resolve_enum_entries_for_rev(schema: &EnumSchema, rev: Rev) -> Vec<&EnumEntry> {
    let family_revs = family_revs_for_spec(schema.spec.as_ref().map(|s| s.id.as_str()));
    schema
        .entries
        .iter()
        .filter(|entry| entry.value.as_single().is_some())
        .filter(|entry| applicable_revs_for_entry(entry, family_revs).contains(&rev))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::pages::{FieldType, PageField};

    fn mk(name: &str, since: Option<&str>, until: Option<&str>) -> PageField {
        PageField {
            name: name.into(),
            display_name: None,
            offset: Some(0),
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: None,
            ty: FieldType::U8,
            enum_ref: None,
            composite_ref: None,
            encoding: None,
            unit: None,
            access: None,
            metadata: Default::default(),
            setter_eligible: None,
            explicit_access: None,
            field_category: None,
            tags: vec![],
            flags: None,
            description: None,
            instances: None,
            variants: None,
            present_when: None,
            content_switch: None,
            enum_inline: None,
            provenance: None,
            scope: None,
            behavior: None,
            constraints: None,
            default_value: None,
            template: None,
            supplement: None,
            since: since.map(str::to_string),
            until: until.map(str::to_string),
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
        }
    }

    #[test]
    fn no_metadata_means_all_revs() {
        let s = applicable_revs(&mk("f", None, None), Rev::CMIS_ALL);
        assert_eq!(s.len(), 7);
    }

    #[test]
    fn since_4_0_excludes_3_0() {
        let s = applicable_revs(&mk("f", Some("4.0"), None), Rev::CMIS_ALL);
        assert!(!s.contains(&Rev::V3_0));
        assert!(s.contains(&Rev::V4_0));
        assert!(s.contains(&Rev::V5_3));
    }

    #[test]
    fn until_4_0_keeps_3_0_only() {
        let s = applicable_revs(&mk("f", None, Some("4.0")), Rev::CMIS_ALL);
        assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![Rev::V3_0]);
    }

    /// since + until window: V4_0, V5_0, V5_1 (inclusive since, exclusive until).
    #[test]
    fn since_and_until_both_set_returns_window() {
        let s = applicable_revs(&mk("f", Some("4.0"), Some("5.2")), Rev::CMIS_ALL);
        let expected: BTreeSet<Rev> = [Rev::V4_0, Rev::V5_0, Rev::V5_1].into_iter().collect();
        assert_eq!(s, expected);
    }

    /// since == earliest rev → same result as no since.
    #[test]
    fn since_3_0_returns_all_revs() {
        let s = applicable_revs(&mk("f", Some("3.0"), None), Rev::CMIS_ALL);
        assert_eq!(s.len(), 7);
    }

    /// Empty family_revs → empty set, no panic.
    #[test]
    fn empty_family_revs_does_not_panic() {
        let s = applicable_revs(&mk("f", None, None), &[]);
        assert!(s.is_empty());
    }

    /// Invalid since string → parse error.
    #[test]
    fn invalid_since_string_errors() {
        let err = Rev::parse("v4.0").expect_err("must error on 'v4.0'");
        assert!(
            err.to_string().contains("v4.0"),
            "error should mention the bad value"
        );
    }

    /// Invalid until string → parse error.
    #[test]
    fn invalid_until_string_errors() {
        let err = Rev::parse("40").expect_err("must error on '40'");
        assert!(err.to_string().contains("40"));
    }

    /// FromStr impl delegates to Rev::parse.
    #[test]
    fn from_str_delegates_to_parse() {
        assert_eq!("5.3".parse::<Rev>().unwrap(), Rev::V5_3);
        assert!("5.x".parse::<Rev>().is_err());
    }

    /// The SFF-8636 domain parses, orders chronologically and renders.
    #[test]
    fn sff8636_domain_round_trips() {
        let expected = [
            ("1.3", "1_3", 0x03u8),
            ("1.4", "1_4", 0x04),
            ("1.5", "1_5", 0x05),
            ("2.0", "2_0", 0x06),
            ("2.5", "2_5", 0x07),
            ("2.8", "2_8", 0x08),
            ("2.11", "2_11", 0x09),
            ("2.12", "2_12", 0x0A),
        ];
        for (s, dir, byte) in expected {
            let rev = Rev::parse(s).unwrap();
            assert_eq!(rev.family(), Family::Sff8636);
            assert_eq!(rev.as_str(), s);
            assert_eq!(rev.dirname(), dir);
            assert_eq!(rev.family_rev_byte(), byte);
            assert!(Rev::SFF8636_ALL.contains(&rev));
        }
        assert!(Rev::SFF8636_ALL.windows(2).all(|w| w[0] < w[1]));
        // Unadmitted intermediate revisions stay rejected: annotations snap
        // to the nearest admitted revision instead of naming an unemitted one.
        assert!(Rev::parse("1.9").is_err());
        assert!(Rev::parse("2.10").is_err());
    }

    /// The SFF-8472 domain parses, orders chronologically and renders.
    #[test]
    fn sff8472_domain_round_trips() {
        let expected = [
            ("9.3", "9_3", 0x01u8),
            ("9.5", "9_5", 0x02),
            ("10.2", "10_2", 0x03),
            ("10.4", "10_4", 0x04),
            ("11.0", "11_0", 0x05),
            ("11.3", "11_3", 0x06),
            ("11.4", "11_4", 0x07),
            ("12.3", "12_3", 0x08),
            ("12.4", "12_4", 0x09),
            ("12.5", "12_5", 0x0A),
        ];
        for (s, dir, byte) in expected {
            let rev = Rev::parse(s).unwrap();
            assert_eq!(rev.family(), Family::Sff8472);
            assert_eq!(rev.as_str(), s);
            assert_eq!(rev.dirname(), dir);
            assert_eq!(rev.family_rev_byte(), byte);
            assert!(Rev::SFF8472_ALL.contains(&rev));
        }
        assert!(Rev::SFF8472_ALL.windows(2).all(|w| w[0] < w[1]));
    }

    /// family_rev_byte for CMIS keeps the packed-BCD major byte.
    #[test]
    fn family_rev_byte_cmis_is_major_byte() {
        for rev in Rev::CMIS_ALL {
            assert_eq!(rev.family_rev_byte(), rev.major_byte_cmis());
        }
    }

    /// Window matching is family-qualified: a since/until naming another
    /// family's revision yields an empty set over this family's domain,
    /// never a cross-family overlap.
    #[test]
    fn cross_family_windows_never_match() {
        let f = mk("f", Some("4.0"), None); // CMIS rev string
        assert!(applicable_revs(&f, Rev::SFF8636_ALL).is_empty());
        let g = mk("g", Some("2.5"), None); // SFF-8636 rev string
        assert!(applicable_revs(&g, Rev::CMIS_ALL).is_empty());
        // In-family windows keep the documented semantics.
        let h = mk("h", Some("2.5"), Some("2.11"));
        let s: Vec<_> = applicable_revs(&h, Rev::SFF8636_ALL).into_iter().collect();
        assert_eq!(s, vec![Rev::Sff8636_2_5, Rev::Sff8636_2_8]);
    }

    /// family_revs_for_spec routes spec ids to their family domains, with
    /// the CMIS domain as the spec-less fallback.
    #[test]
    fn family_revs_for_spec_routes_domains() {
        assert_eq!(family_revs_for_spec(Some("cmis")), Rev::CMIS_ALL);
        assert_eq!(family_revs_for_spec(Some("sff-8636")), Rev::SFF8636_ALL);
        assert_eq!(family_revs_for_spec(Some("sff-8472")), Rev::SFF8472_ALL);
        assert_eq!(family_revs_for_spec(None), Rev::CMIS_ALL);
        assert_eq!(family_revs_for_spec(Some("sff-8024")), Rev::CMIS_ALL);
    }

    /// major_byte_cmis mirrors openxcvr_core::Rev::from_major_byte_cmis for
    /// every known revision (the generated projection factories dispatch on
    /// these bytes).
    #[test]
    fn major_byte_matches_the_core_decode() {
        let cases = [
            (Rev::V3_0, 0x30u8),
            (Rev::V4_0, 0x40),
            (Rev::V5_0, 0x50),
            (Rev::V5_1, 0x51),
            (Rev::V5_2, 0x52),
            (Rev::V5_3, 0x53),
            (Rev::V5_4, 0x54),
        ];
        for (rev, byte) in cases {
            assert_eq!(rev.major_byte_cmis(), byte);
        }
    }
}
