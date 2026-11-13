use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{bail, ensure, Context};

use crate::expr::compute;
use crate::ir::enums::{EntryValue, EnumEntry, EnumSchema};
use crate::ir::pages::{
    page_base, Access, FieldType, InstancesDesc, PageField, PageSchema, ReadEffectMetadata,
    ReadEffectScope,
};
use crate::ir::version::{applicable_revs, resolve_field_for_rev};
use crate::parser::pages::ALLOWED_PAGE_SCHEMAS;

pub fn validate_enums(schemas: &[EnumSchema]) -> anyhow::Result<()> {
    ensure!(!schemas.is_empty(), "no enum schemas loaded");

    let mut ids = HashSet::new();
    for schema in schemas {
        ensure!(
            schema.schema == "openxcvr.enum.v2",
            "{}: unexpected enum schema {}",
            schema.id,
            schema.schema
        );
        ensure!(
            schema.kind == "enum",
            "{}: unexpected enum kind {}",
            schema.id,
            schema.kind
        );
        ensure!(!schema.id.trim().is_empty(), "enum schema has empty id");
        ensure!(
            ids.insert(schema.id.as_str()),
            "duplicate enum id {}",
            schema.id
        );
        ensure!(
            !schema.entries.is_empty(),
            "{}: enum has no entries",
            schema.id
        );

        let mut names = HashSet::new();
        let mut ranges: Vec<(u64, u64, &str)> = Vec::new();
        for entry in &schema.entries {
            ensure!(
                !entry.name.trim().is_empty(),
                "{}: enum entry has empty name",
                schema.id
            );
            ensure!(
                names.insert(entry.name.as_str()),
                "{}: duplicate enum entry name {}",
                schema.id,
                entry.name
            );
            let (start, end) = match entry.value {
                EntryValue::Single(value) => (value, value),
                EntryValue::Range { start, end } => {
                    ensure!(
                        start <= end,
                        "{}.{}: inverted range {}..{}",
                        schema.id,
                        entry.name,
                        start,
                        end
                    );
                    (start, end)
                }
            };
            for (prev_start, prev_end, prev_name) in &ranges {
                ensure!(
                    end < *prev_start || start > *prev_end,
                    "{}.{} overlaps enum entry {}",
                    schema.id,
                    entry.name,
                    prev_name
                );
            }
            ranges.push((start, end, entry.name.as_str()));
        }
    }

    validate_enum_revision_metadata(schemas)?;

    Ok(())
}

/// Central revision-metadata gate for enum schemas, invoked once at the end of
/// [`validate_enums`] after its unchanged structural/name/overlap checks.
/// Both real callers — the CLI generate sequence and the Markdown top-index
/// probe — thereby reject invalid metadata before resolution/emission.
///
/// Contract (schema-owned DPSM repair, spec § 2.2):
///
/// 1. `since`/`until` must be known family revision strings (`Rev::parse`
///    over 3.0/4.0/5.0/5.1/5.2/5.3/5.4); `5.5`, `3.1`, `4`, `v4.0` are errors
///    naming the entry and value.
/// 2. The applicability window over the known family domain must be
///    non-empty — equal, inverted and empty-intersection windows are
///    errors, never silently unreachable entries.
/// 3. Entry revision metadata — including rejected unsupported markers — is
///    CMIS-only: `spec.id` must be `cmis`. No CMIS revision domain is ever
///    applied to SFF.
/// 4. `changed_in`/`renamed_from`/`aliased_to` are declared but
///    not implemented; any presence rejects. This slice does not claim
///    the full enum vocabulary.
///
/// The raw-union duplicate/overlap rules stay revision-blind: entries with
/// the same value or overlapping ranges are rejected by `validate_enums`
/// regardless of their revision windows.
fn validate_enum_revision_metadata(schemas: &[EnumSchema]) -> anyhow::Result<()> {
    use crate::ir::version::{applicable_revs_for_entry, Rev};

    for schema in schemas {
        for entry in &schema.entries {
            // Rule 4 first: unsupported vocabulary fails visibly.
            for (key, value) in [
                ("changed_in", &entry.changed_in),
                ("renamed_from", &entry.renamed_from),
                ("aliased_to", &entry.aliased_to),
            ] {
                if let Some(raw) = value {
                    bail!(
                        "{}.{}: unsupported revision metadata key {} (value {raw:?}); \
                         declared but not implemented; only since/until are interpreted",
                        schema.id,
                        entry.name,
                        key
                    );
                }
            }

            let has_metadata = entry.since.is_some()
                || entry.until.is_some()
                || entry.changed_in.is_some()
                || entry.renamed_from.is_some()
                || entry.aliased_to.is_some();

            // Rule 3: revision metadata requires a spec family with a
            // modeled revision domain (cmis, sff-8636, sff-8472).
            if has_metadata {
                let spec_id = schema.spec.as_ref().map(|spec| spec.id.as_str());
                let has_family = spec_id
                    .and_then(crate::ir::version::Family::from_spec_id)
                    .is_some();
                ensure!(
                    has_family,
                    "{}.{}: entry revision metadata requires spec.id cmis | sff-8636 | sff-8472; \
                     this schema records {}",
                    schema.id,
                    entry.name,
                    spec_id.unwrap_or("<no spec block>")
                );
            }

            // Rule 1: known family revision strings, named by entry and value.
            // Must run before the window check: the resolver contract panics
            // on unvalidated rev strings.
            for (key, value) in [("since", &entry.since), ("until", &entry.until)] {
                if let Some(raw) = value {
                    Rev::parse(raw).with_context(|| {
                        format!(
                            "{}.{}: {key} must be a known CMIS family revision string",
                            schema.id, entry.name
                        )
                    })?;
                }
            }

            // Rule 2: non-empty applicability over the schema's own family
            // domain (CMIS when the schema carries no spec block, matching
            // the resolver's domain choice).
            if entry.since.is_some() || entry.until.is_some() {
                let family_revs = crate::ir::version::family_revs_for_spec(
                    schema.spec.as_ref().map(|s| s.id.as_str()),
                );
                ensure!(
                    !applicable_revs_for_entry(entry, family_revs).is_empty(),
                    "{}.{}: empty revision window (since {:?}, until {:?}) over the family domain",
                    schema.id,
                    entry.name,
                    entry.since,
                    entry.until
                );
            }
        }
    }

    Ok(())
}

pub fn validate_pages(schemas: &[PageSchema]) -> anyhow::Result<()> {
    // An enum-only namespace (SFF-8024) is a valid generate target. Rejecting
    // an empty page list here made `generate --spec sff-8024` fail even though
    // the registry loads and its enums render.
    if schemas.is_empty() {
        return Ok(());
    }

    let mut ids = HashSet::new();
    for schema in schemas {
        let family_revs =
            crate::ir::version::family_revs_for_spec(schema.spec.as_ref().map(|s| s.id.as_str()));
        ensure!(
            ALLOWED_PAGE_SCHEMAS.contains(&schema.schema.as_str()),
            "{}: unexpected page schema {}",
            schema.id,
            schema.schema
        );
        ensure!(
            schema.kind == "page",
            "{}: unexpected page kind {}",
            schema.id,
            schema.kind
        );
        ensure!(!schema.id.trim().is_empty(), "page schema has empty id");
        ensure!(
            ids.insert(schema.id.as_str()),
            "duplicate page id {}",
            schema.id
        );
        ensure!(
            !schema.fields.is_empty(),
            "{}: page has no fields",
            schema.id
        );

        let mut field_names = HashSet::new();
        // The union view can contain mutually exclusive revision fields.
        // Reject collisions at each applicable revision, never across revisions.
        use crate::ir::version::{applicable_revs, resolve_field_for_rev, Rev};
        let mut occupied: HashMap<(Rev, i64, u32), &str> = HashMap::new();
        for field in &schema.fields {
            ensure!(
                field_names.insert(field.name.as_str()),
                "{}: duplicate field name {}",
                schema.id,
                field.name
            );
            let revisions = applicable_revs(field, family_revs);
            ensure!(
                !revisions.is_empty(),
                "{}: field {} has an empty revision window (since {:?}, until {:?})",
                schema.id,
                field.name,
                field.since,
                field.until
            );
            validate_field_addressing(schema, field)?;
            for &rev in &revisions {
                validate_field_addressing(schema, &resolve_field_for_rev(field, rev))?;
            }

            if field.present_when.is_none()
                && field.variants.is_none()
                && !matches!(field.ty, FieldType::Bitmap | FieldType::Composite)
            {
                for &rev in &revisions {
                    let resolved = resolve_field_for_rev(field, rev);
                    for span in field_spans(schema, &resolved)? {
                        for (byte, bit) in span.bits() {
                            if let Some(prev) =
                                occupied.insert((rev, byte, bit), field.name.as_str())
                            {
                                bail!(
                                    "{}: fields {} and {} overlap at byte {} bit {} in revision {}",
                                    schema.id,
                                    prev,
                                    field.name,
                                    byte,
                                    bit,
                                    rev.as_str()
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    validate_inline_enums(schemas)?;

    Ok(())
}

struct E2cExemption {
    page_ids: &'static [&'static str],
    field_name: &'static str,
    expected_codes: &'static [u64],
    primary_source: &'static str,
}

/// Partial inline enum blocks whose printed spec tables omit some wire codes (§A4).
const E2C_PARTIAL_CODE_TABLES: &[E2cExemption] = &[
    // 1. C-CMIS 1.4 Table 11 pp.19-20: tx_filter_type codes 1..=3 printed.
    E2cExemption {
        page_ids: &["CCmisPage31hMediaLaneProvisioning"],
        field_name: "tx_filter_type",
        expected_codes: &[1, 2, 3],
        primary_source: "C-CMIS 1.4 Table 11 pp.19-20",
    },
    // 1a. C-CMIS 01.0 Table 10 p.17, 01.1 Table 10 p.17, 01.2 Table 11 p.18
    // and 01.3 Table 11 p.19 print the same three tx filter codes and no others.
    E2cExemption {
        page_ids: &[
            "CCmis10Page31hMediaLaneProvisioning",
            "CCmis11Page31hMediaLaneProvisioning",
            "CCmis12Page31hMediaLaneProvisioning",
            "CCmis13Page31hMediaLaneProvisioning",
        ],
        field_name: "tx_filter_type",
        expected_codes: &[1, 2, 3],
        primary_source:
            "C-CMIS 01.0 Table 10 p.17; 01.1 Table 10 p.17; 01.2 Table 11 p.18; 01.3 Table 11 p.19",
    },
    // 1b. C-CMIS 1.5 Table 11 p.23: tx_filter_type codes 1..=3 printed
    // (same partial table as 1.4, own page id per the revision-suffixed
    // companion namespace).
    E2cExemption {
        page_ids: &["CCmis15Page31hMediaLaneProvisioning"],
        field_name: "tx_filter_type",
        expected_codes: &[1, 2, 3],
        primary_source: "C-CMIS 1.5 Table 11 p.23",
    },
    // 1c. C-CMIS 1.5 Table 13 p.27: txFilterTypeValue codes 1..=3 printed
    // (new in 1.5, the applied-value readback of tx_filter_type).
    E2cExemption {
        page_ids: &["CCmis15Page33hMediaLaneFlagsAndStatus"],
        field_name: "tx_filter_type_value",
        expected_codes: &[1, 2, 3],
        primary_source: "C-CMIS 1.5 Table 13 p.27",
    },
    // 2. CMIS 5.3 Table 8-166 p.269: commit_redirection code 1b printed.
    E2cExemption {
        page_ids: &["CmisPage1DhHostLaneSwitching"],
        field_name: "commit_redirection",
        expected_codes: &[1],
        primary_source: "CMIS 5.3 Table 8-166 p.269",
    },
    // 2b. CMIS 5.4 Table 8-196 p.318: commit_media_lane_redirection code 1b
    // printed (Page 6Dh media lane switching; WO/SC commit trigger).
    E2cExemption {
        page_ids: &["CmisPage6DhMediaLaneSwitching"],
        field_name: "commit_media_lane_redirection",
        expected_codes: &[1],
        primary_source: "CMIS 5.4 Table 8-196 p.318",
    },
    // 3. CMIS 5.3 Table 8-169 p.273: monitored_resource codes 0..=7 and 15 printed.
    E2cExemption {
        page_ids: &["CmisPage20hVdmDescriptors1To64"],
        field_name: "monitored_resource",
        expected_codes: &[0, 1, 2, 3, 4, 5, 6, 7, 15],
        primary_source: "CMIS 5.3 Table 8-169 p.273",
    },
    // 4. ELSFP 1.0 Table 6 p.20: save_restore_status codes 01h-08h printed.
    E2cExemption {
        page_ids: &["ElsfpPage1ahLaneMgmt"],
        field_name: "save_restore_status",
        expected_codes: &[1, 2, 3, 4, 5, 6, 7, 8],
        primary_source: "ELSFP 1.0 Table 6 p.20",
    },
];

fn lookup_e2c(page_id: &str, field_name: &str) -> Option<&'static E2cExemption> {
    E2C_PARTIAL_CODE_TABLES
        .iter()
        .find(|e| e.field_name == field_name && e.page_ids.contains(&page_id))
}

fn field_type_inline_bit_width(ty: &FieldType) -> Option<u32> {
    match ty {
        FieldType::Bool => Some(1),
        FieldType::U2 => Some(2),
        FieldType::U3 => Some(3),
        FieldType::U4 => Some(4),
        FieldType::U5 => Some(5),
        FieldType::U6 => Some(6),
        FieldType::U7 => Some(7),
        FieldType::U8 | FieldType::I8 => Some(8),
        FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::F16Be
        | FieldType::X16Be => Some(16),
        FieldType::U32Be | FieldType::I32Be => Some(32),
        FieldType::U64Le | FieldType::U64Be => Some(64),
        FieldType::Ascii
        | FieldType::Bitmap
        | FieldType::Bytes
        | FieldType::Composite
        | FieldType::EnumRef => None,
    }
}

fn resolve_effective_bit_width(
    field: &PageField,
    changed_in_bit_width: Option<u32>,
    changed_in_ty: Option<&FieldType>,
) -> anyhow::Result<u32> {
    if let Some(w) = changed_in_bit_width {
        return Ok(w);
    }
    if let Some(ty) = changed_in_ty {
        if let Some(w) = field_type_inline_bit_width(ty) {
            return Ok(w);
        }
    }
    if let Some(w) = field.bit_width {
        return Ok(w);
    }
    if let Some(w) = field_type_inline_bit_width(&field.ty) {
        return Ok(w);
    }
    bail!(
        "cannot resolve effective bit width for field '{}' (type '{:?}')",
        field.name,
        field.ty
    )
}

fn check_negative_value(entry_val: &serde_norway::Value) -> Option<i64> {
    if let Some(map) = entry_val.as_mapping() {
        if let Some(val) = map.get("value") {
            if let Some(i) = val.as_i64() {
                if i < 0 {
                    return Some(i);
                }
            }
            if let Some(s) = val.as_str() {
                for part in s.split("..") {
                    let trimmed = part.trim();
                    if trimmed.starts_with('-') {
                        if let Ok(i) = trimmed.parse::<i64>() {
                            if i < 0 {
                                return Some(i);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn validate_single_inline_block(
    schema: &PageSchema,
    field: &PageField,
    block_origin: &str,
    block_val: &serde_norway::Value,
    ci_bit_width: Option<u32>,
    ci_ty: Option<&FieldType>,
    failures: &mut Vec<String>,
) {
    let entries_seq = match block_val.as_sequence() {
        Some(s) => s,
        None => {
            failures.push(format!(
                "{}.{} ({block_origin}): enum_inline must be a sequence",
                schema.id, field.name
            ));
            return;
        }
    };
    if entries_seq.is_empty() {
        failures.push(format!(
            "{}.{} ({block_origin}): enum_inline sequence is empty",
            schema.id, field.name
        ));
        return;
    }

    let w = match resolve_effective_bit_width(field, ci_bit_width, ci_ty) {
        Ok(w) => w,
        Err(e) => {
            failures.push(format!(
                "{}.{} ({block_origin}): {e}",
                schema.id, field.name
            ));
            return;
        }
    };

    let max_val: u64 = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };

    let mut names = HashSet::new();
    let mut entry_spans: Vec<(u64, u64)> = Vec::new();
    let mut has_negative = false;

    for entry_val in entries_seq {
        let is_neg = check_negative_value(entry_val);
        if let Some(neg) = is_neg {
            failures.push(format!(
                "{}.{} ({block_origin}): negative value in enum_inline: {neg}",
                schema.id, field.name
            ));
            has_negative = true;
        }

        let entry: EnumEntry = match serde_norway::from_value(entry_val.clone()) {
            Ok(e) => e,
            Err(err) => {
                failures.push(format!(
                    "{}.{} ({block_origin}): failed to parse entry: {err}",
                    schema.id, field.name
                ));
                continue;
            }
        };

        // R3: Unique names
        if !names.insert(entry.name.clone()) {
            failures.push(format!(
                "{}.{} ({block_origin}): duplicate entry name '{}'",
                schema.id, field.name, entry.name
            ));
        }

        if is_neg.is_some() {
            continue;
        }

        let (start, end) = match entry.value {
            EntryValue::Single(v) => (v, v),
            EntryValue::Range { start, end } => {
                if start > end {
                    failures.push(format!(
                        "{}.{} ({block_origin}): inverted range {start}..{end}",
                        schema.id, field.name
                    ));
                    continue;
                }
                (start, end)
            }
        };

        entry_spans.push((start, end));
    }

    // R2: Disjointness and covered code collection
    let mut covered = BTreeSet::new();
    for &(start, end) in &entry_spans {
        if end.saturating_sub(start) <= 65536 {
            for val in start..=end {
                if !covered.insert(val) {
                    failures.push(format!(
                        "{}.{} ({block_origin}): overlapping value {val}",
                        schema.id, field.name
                    ));
                    break;
                }
            }
        }
    }

    // Exemption E1 (instance-index legends)
    let is_e1_exempt = if let Some(inst) = &field.instances {
        if inst.start >= 0 && inst.start <= inst.end {
            let expected_inst: BTreeSet<u64> = (inst.start as u64..=inst.end as u64).collect();
            let w_bound = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };
            covered == expected_inst && (entries_seq.len() as u64 > w_bound || w >= 16)
        } else {
            false
        }
    } else {
        false
    };

    if is_e1_exempt {
        return;
    }

    // Exemption E2-c (spec-verified partial code tables)
    if let Some(e2c) = lookup_e2c(&schema.id, &field.name) {
        // R1: Width fit
        for &(_start, end) in &entry_spans {
            if end > max_val {
                failures.push(format!(
                    "{}.{} ({block_origin}): value {end} exceeds width {w} (max {max_val})",
                    schema.id, field.name
                ));
            }
        }

        // R4: Totality over expected printed code set
        let expected_set: BTreeSet<u64> = e2c.expected_codes.iter().copied().collect();
        if covered != expected_set {
            let missing: Vec<_> = expected_set.difference(&covered).copied().collect();
            let extra: Vec<_> = covered.difference(&expected_set).copied().collect();
            failures.push(format!(
                "{}.{} ({block_origin}): covered set does not match printed code set from {} (missing: {missing:?}, extra: {extra:?})",
                schema.id, field.name, e2c.primary_source
            ));
        }
        return;
    }

    // Normal block: R1 width fit
    for &(_start, end) in &entry_spans {
        if end > max_val {
            failures.push(format!(
                "{}.{} ({block_origin}): value {end} exceeds width {w} (max {max_val})",
                schema.id, field.name
            ));
        }
    }

    // Normal block: R4 totality over 0..=max_val
    if !has_negative {
        let mut missing_val: Option<u64> = None;
        if w <= 16 {
            for expected in 0..=max_val {
                if !covered.contains(&expected) {
                    missing_val = Some(expected);
                    break;
                }
            }
        } else {
            let mut sorted_spans = entry_spans.clone();
            sorted_spans.sort_by_key(|&(s, _)| s);
            let mut cur = 0u64;
            for &(s, e) in &sorted_spans {
                if s > cur {
                    missing_val = Some(cur);
                    break;
                }
                if e >= cur {
                    cur = e.saturating_add(1);
                }
            }
            if missing_val.is_none() && cur <= max_val {
                missing_val = Some(cur);
            }
        }

        if let Some(missing) = missing_val {
            failures.push(format!(
                "{}.{} ({block_origin}): missing value {missing} over range 0..={max_val}",
                schema.id, field.name
            ));
        }
    }
}

pub fn validate_inline_enums(schemas: &[PageSchema]) -> anyhow::Result<()> {
    use crate::ir::version::Rev;
    let mut failures: Vec<String> = Vec::new();

    for schema in schemas {
        for field in &schema.fields {
            if let Some(base_val) = &field.enum_inline {
                validate_single_inline_block(
                    schema,
                    field,
                    "base",
                    base_val,
                    None,
                    None,
                    &mut failures,
                );
            }

            if let Some(changed_in) = &field.changed_in {
                for change in changed_in {
                    if let Some(ci_val) = &change.enum_inline {
                        let origin = format!("changed_in {}", change.rev);
                        validate_single_inline_block(
                            schema,
                            field,
                            &origin,
                            ci_val,
                            change.bit_width,
                            change.ty.as_ref(),
                            &mut failures,
                        );
                    }
                }

                // R5: base block == highest changed_in block
                let inline_changes: Vec<_> = changed_in
                    .iter()
                    .filter(|c| c.enum_inline.is_some())
                    .collect();

                if !inline_changes.is_empty() {
                    let mut highest_rev: Option<Rev> = None;
                    let mut highest_val: Option<&serde_norway::Value> = None;
                    let mut parse_failed = false;

                    for change in &inline_changes {
                        match Rev::parse(&change.rev) {
                            Ok(rev) => {
                                if highest_rev.is_none() || Some(rev) > highest_rev {
                                    highest_rev = Some(rev);
                                    highest_val = change.enum_inline.as_ref();
                                }
                            }
                            Err(err) => {
                                failures.push(format!(
                                    "{}.{} (changed_in {}): invalid revision: {err}",
                                    schema.id, field.name, change.rev
                                ));
                                parse_failed = true;
                            }
                        }
                    }

                    if !parse_failed {
                        if let Some(highest_val) = highest_val {
                            let base_val = field.enum_inline.as_ref();
                            if base_val != Some(highest_val) {
                                failures.push(format!(
                                    "{}.{} (base): base enum_inline must equal highest changed_in (rev {}) block",
                                    schema.id,
                                    field.name,
                                    highest_rev.map(|r| r.as_str()).unwrap_or("unknown")
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        bail!(
            "inline enum validation failed with {} error(s):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    Ok(())
}

pub fn validate_page_enum_refs(pages: &[PageSchema], enums: &[EnumSchema]) -> anyhow::Result<()> {
    let enum_ids: HashSet<&str> = enums.iter().map(|schema| schema.id.as_str()).collect();
    for page in pages {
        for field in &page.fields {
            if let Some(enum_ref) = &field.enum_ref {
                ensure!(
                    enum_ids.contains(enum_ref.as_str()),
                    "{}.{}: unknown enum_ref {}",
                    page.id,
                    field.name,
                    enum_ref
                );
            }
            // : validate each changed_in[].enum_ref through the same
            // registry.  A typo in a versioned enum_ref would otherwise silently
            // produce wrong per-rev output.
            if let Some(entries) = &field.changed_in {
                for entry in entries {
                    if let Some(ci_enum_ref) = &entry.enum_ref {
                        ensure!(
                            enum_ids.contains(ci_enum_ref.as_str()),
                            "{}.{}: changed_in[rev={}]: unknown enum_ref {}",
                            page.id,
                            field.name,
                            entry.rev,
                            ci_enum_ref
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Opt-in D1 volatility gate.
///
/// Mirrors `tools/validate-schemas.py --volatility=<family>` over the typed
/// schemas the codegen loads: every field carries an access marking and
/// `flags.is_dynamic` after template/include/override resolution at every
/// applicable revision, every constituent bit of every defined byte is
/// covered (a whole-byte decoding container is not independent evidence that
/// fills missing child markings), overlapping semantic markings agree,
/// bit bounds are valid, explicit read-effect metadata carries a scope (and
/// a provenance citation when byte-scoped), a field's access and explicit
/// read effect never contradict, a byte-scoped physical effect never
/// coexists with an explicit side-effect-free neighbour, and the retired
/// `behavior.latching` spelling is rejected.
///
/// # Documented deviation: retired access spellings
///
/// The retired access spellings (`cor`, `rw_nv`, `rw_v`, `rw_triggered`,
/// `rw_with_state_guard`, `implementation_defined`) are invisible to this
/// gate: `Access` deserializes them through `#[serde(alias = …)]`
/// (`crates/openxcvr-codegen/src/ir/pages.rs:159-180`, `Access::Rw` aliases
/// `rw_nv`/`rw_triggered`/`rw_v`/`rw_with_state_guard`, `Access::RoCor`
/// aliases `cor`, `Access::Unknown` aliases `implementation_defined`), so
/// serde normalizes every alias to its canonical variant before the typed
/// `PageField.access`/`explicit_access` (`src/parser/pages.rs:213` sets the
/// latter from the already-deserialized value) reaches this function. A
/// scratch test in the fix round fed all five spellings through
/// `parse_page_schema` + `validate_volatility` and every one passed. This
/// cannot be repaired inside the validator: the raw spelling is gone from
/// the typed IR, and re-scanning raw YAML here would add a second schema
/// view beside the typed one. **The Python gate
/// (`tools/validate-schemas.py --volatility=…`) is the authority for the
/// retired-access-spelling rule until task 19 removes the serde aliases**;
/// `behavior.latching` (raw `serde_norway::Value`) is and remains checked here.
/// Not wired into `main` yet: task 19 makes the pass mandatory.
pub fn validate_volatility(schemas: &[PageSchema]) -> anyhow::Result<()> {
    for schema in schemas {
        let base = page_base(schema);
        let size = i64::from(schema.size_bytes.unwrap_or(128));
        for field in &schema.fields {
            if field
                .behavior
                .as_ref()
                .and_then(|b| b.get("latching"))
                .is_some()
            {
                bail!(
                    "{}.{}: retired marking spelling behavior.latching",
                    schema.id,
                    field.name
                );
            }
            for span in field_spans(schema, field)? {
                ensure!(
                    span.bit_start + span.bit_width <= 8,
                    "{}.{}: invalid bit bounds {}..{}",
                    schema.id,
                    field.name,
                    span.bit_start,
                    span.bit_start + span.bit_width - 1
                );
            }
        }
        // Defined byte range: lower pages from byte 0; other pages from
        // their first covered byte (mirrors the Python gate's --gaps
        // semantics — a page whose fields start later defines nothing before
        // them).
        let mut any_covered: BTreeMap<i64, u8> = BTreeMap::new();
        for field in &schema.fields {
            for (byte, mask) in field.bit_coverage().map_err(anyhow::Error::msg)? {
                *any_covered.entry(i64::from(byte)).or_default() |= mask;
            }
        }
        let lower = schema.page_address.eq_ignore_ascii_case("lower");
        let lo = if lower {
            0
        } else {
            any_covered.keys().next().copied().unwrap_or(base)
        };
        let family_revs =
            crate::ir::version::family_revs_for_spec(schema.spec.as_ref().map(|s| s.id.as_str()));
        for rev in family_revs {
            let resolved: Vec<PageField> = schema
                .fields
                .iter()
                .filter(|f| applicable_revs(f, family_revs).contains(rev))
                .map(|f| resolve_field_for_rev(f, *rev))
                .collect();
            for field in &resolved {
                ensure!(
                    field.access.is_some(),
                    "{}.{}: missing access marking at revision {}",
                    schema.id,
                    field.name,
                    rev.as_str()
                );
                ensure!(
                    field.flags.as_ref().and_then(|f| f.is_dynamic).is_some(),
                    "{}.{}: missing flags.is_dynamic marking at revision {}",
                    schema.id,
                    field.name,
                    rev.as_str()
                );
                // A field's access implies an effect (ro_cor implies
                // ClearOnRead, unknown implies Unknown): an explicit
                // side-effect-free `read_effect: none` on the same field
                // contradicts it (mirrors the Python gate's
                // effect_contradiction rule; the parser rejects this on the
                // raw declaration, the gate holds for the resolved IR too).
                let effects = field.read_effects();
                if effects.contains(&ReadEffectMetadata::None) && effects.len() > 1 {
                    bail!(
                        "{}.{}: access/effect contradiction: explicit read_effect \
                         none conflicts with {:?} at revision {}",
                        schema.id,
                        field.name,
                        effects
                            .iter()
                            .filter(|e| **e != ReadEffectMetadata::None)
                            .collect::<Vec<_>>(),
                        rev.as_str()
                    );
                }
                if let Some(effect) = field.metadata.read_effect {
                    ensure!(
                        field.metadata.read_effect_scope.is_some(),
                        "{}.{}: read_effect {effect:?} requires read_effect_scope at revision {}",
                        schema.id,
                        field.name,
                        rev.as_str()
                    );
                    if field.metadata.read_effect_scope == Some(ReadEffectScope::Byte)
                        && effect != ReadEffectMetadata::None
                        && field.metadata.read_effect_provenance.is_none()
                    {
                        bail!(
                            "{}.{}: byte-scoped read_effect {effect:?} requires a \
                             read_effect_provenance citation at revision {}",
                            schema.id,
                            field.name,
                            rev.as_str()
                        );
                    }
                }
            }
            let mut by_byte: BTreeMap<i64, Vec<&PageField>> = BTreeMap::new();
            for field in &resolved {
                for (byte, _mask) in field.bit_coverage().map_err(anyhow::Error::msg)? {
                    by_byte.entry(i64::from(byte)).or_default().push(field);
                }
            }
            for byte in lo..base + size {
                let Some(fields) = by_byte.get(&byte) else {
                    bail!(
                        "{}: byte {} uncovered at revision {}",
                        schema.id,
                        byte,
                        rev.as_str()
                    );
                };
                let subs: Vec<&PageField> =
                    fields.iter().copied().filter(|f| is_sub_byte(f)).collect();
                if !subs.is_empty() {
                    let mut mask = 0u8;
                    for f in &subs {
                        if let Some(m) = f
                            .bit_coverage()
                            .map_err(anyhow::Error::msg)?
                            .get(&(byte as u32))
                        {
                            mask |= m;
                        }
                    }
                    let holes: Vec<u32> = (0..8).filter(|b| mask >> b & 1 == 0).collect();
                    ensure!(
                        holes.is_empty(),
                        "{}: byte {} bits {:?} uncovered at revision {} \
                         (a whole-byte container is not marking evidence)",
                        schema.id,
                        byte,
                        holes,
                        rev.as_str()
                    );
                }
                // Disagreeing overlapping markings (unconditional fields
                // only; present_when overlays are legal).
                type Mark = (Option<Access>, Option<bool>);
                let mut whole: Option<(Mark, &str)> = None;
                let mut per_bit: [Option<(Mark, &str)>; 8] = [None; 8];
                for f in fields.iter().filter(|f| f.present_when.is_none()) {
                    let mark: Mark = (f.access, f.flags.as_ref().and_then(|fl| fl.is_dynamic));
                    if is_sub_byte(f) {
                        let m = f
                            .bit_coverage()
                            .map_err(anyhow::Error::msg)?
                            .get(&(byte as u32))
                            .copied()
                            .unwrap_or(0);
                        for bit in 0..8u32 {
                            if m >> bit & 1 == 0 {
                                continue;
                            }
                            match per_bit[bit as usize] {
                                Some((prev, prev_name)) if prev != mark => bail!(
                                    "{}: fields {} and {} overlap at byte {} bit {} \
                                     with disagreeing markings at revision {}",
                                    schema.id,
                                    prev_name,
                                    f.name,
                                    byte,
                                    bit,
                                    rev.as_str()
                                ),
                                None => per_bit[bit as usize] = Some((mark, f.name.as_str())),
                                _ => {}
                            }
                        }
                    } else if let Some((prev, prev_name)) = whole {
                        ensure!(
                            prev == mark,
                            "{}: fields {} and {} overlap at byte {} with disagreeing \
                             markings at revision {}",
                            schema.id,
                            prev_name,
                            f.name,
                            byte,
                            rev.as_str()
                        );
                    } else {
                        whole = Some((mark, f.name.as_str()));
                    }
                }
                // A physical byte effect cannot coexist with an explicit
                // side-effect-free assertion on the same byte (the parser
                // enforces this pre-resolution; checked again per revision).
                for f in fields {
                    if f.metadata.read_effect_scope == Some(ReadEffectScope::Byte)
                        && f.read_effects()
                            .iter()
                            .any(|e| *e != ReadEffectMetadata::None)
                    {
                        for other in fields {
                            if other.metadata.read_effect == Some(ReadEffectMetadata::None) {
                                bail!(
                                    "{}.{}: byte-scoped read effect conflicts with explicit \
                                     side-effect-free field {} at byte {} revision {}",
                                    schema.id,
                                    f.name,
                                    other.name,
                                    byte,
                                    rev.as_str()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

struct BitSpan {
    byte: i64,
    bit_start: u32,
    bit_width: u32,
    byte_width: u32,
}

impl BitSpan {
    fn bits(&self) -> Vec<(i64, u32)> {
        if self.bit_width < 8 || self.byte_width == 1 {
            return (self.bit_start..self.bit_start + self.bit_width)
                .map(|bit| (self.byte, bit))
                .collect();
        }

        let mut bits = Vec::new();
        for byte in self.byte..self.byte + self.byte_width as i64 {
            for bit in 0..8 {
                bits.push((byte, bit));
            }
        }
        bits
    }
}

fn validate_field_addressing(schema: &PageSchema, field: &PageField) -> anyhow::Result<()> {
    ensure!(
        !field.name.trim().is_empty(),
        "{}: field has empty name",
        schema.id
    );
    ensure!(
        field.offset.is_some() ^ field.offset_expr.is_some(),
        "{}.{}: exactly one of offset or offset_expr is required",
        schema.id,
        field.name
    );

    if matches!(
        field.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) {
        ensure!(
            field.size_bytes.unwrap_or(0) > 0,
            "{}.{}: {} field requires positive size_bytes",
            schema.id,
            field.name,
            field_type_name(&field.ty)
        );
    }

    if let Some(inst) = &field.instances {
        ensure!(
            inst.start <= inst.end,
            "{}.{}: invalid instance range {}..{}",
            schema.id,
            field.name,
            inst.start,
            inst.end
        );
        ensure!(
            !inst.index.trim().is_empty(),
            "{}.{}: instance index name is empty",
            schema.id,
            field.name
        );
    } else {
        ensure!(
            field.offset_expr.is_none() && field.bit_offset_expr.is_none(),
            "{}.{}: expressions require instances",
            schema.id,
            field.name
        );
    }

    for span in field_spans(schema, field)? {
        ensure!(
            span.byte >= 0,
            "{}.{}: negative page-relative byte offset {}",
            schema.id,
            field.name,
            span.byte
        );
        let page_size = schema.size_bytes.unwrap_or(128) as i64;
        ensure!(
            span.byte + span.byte_width as i64 <= page_size,
            "{}.{}: byte span {}..{} exceeds page size {}",
            schema.id,
            field.name,
            span.byte,
            span.byte + span.byte_width as i64 - 1,
            page_size
        );
        ensure!(
            span.bit_width > 0 && span.bit_width <= 8,
            "{}.{}: invalid bit_width {}",
            schema.id,
            field.name,
            span.bit_width
        );
        ensure!(
            span.bit_start < 8 && span.bit_start + span.bit_width <= 8,
            "{}.{}: bit span {}..{} crosses a byte",
            schema.id,
            field.name,
            span.bit_start,
            span.bit_start + span.bit_width - 1
        );
    }

    Ok(())
}

fn field_spans(schema: &PageSchema, field: &PageField) -> anyhow::Result<Vec<BitSpan>> {
    let base = page_base(schema);
    let byte_width = byte_width(field)?;
    let bit_width = bit_width(field);

    let mut spans = Vec::new();
    if let Some(inst) = &field.instances {
        for i in inst.start..=inst.end {
            let flat = resolve_offset(field, inst, i, &schema.id)?;
            let bit_start = resolve_bit_offset(field, inst, i, &schema.id)?;
            spans.push(BitSpan {
                byte: flat - base,
                bit_start,
                bit_width,
                byte_width,
            });
        }
    } else {
        spans.push(BitSpan {
            byte: field.offset.unwrap() as i64 - base,
            bit_start: field.bit_offset.unwrap_or(0),
            bit_width,
            byte_width,
        });
    }
    Ok(spans)
}

fn resolve_offset(
    field: &PageField,
    inst: &InstancesDesc,
    i: i64,
    page_id: &str,
) -> anyhow::Result<i64> {
    if let Some(expr) = &field.offset_expr {
        compute(expr, &inst.index, i).map_err(|e| {
            anyhow::anyhow!(
                "{}.{}: bad offset_expr {:?}: {e}",
                page_id,
                field.name,
                expr
            )
        })
    } else {
        Ok(field.offset.unwrap() as i64)
    }
}

fn resolve_bit_offset(
    field: &PageField,
    inst: &InstancesDesc,
    i: i64,
    page_id: &str,
) -> anyhow::Result<u32> {
    let value = if let Some(expr) = &field.bit_offset_expr {
        compute(expr, &inst.index, i).map_err(|e| {
            anyhow::anyhow!(
                "{}.{}: bad bit_offset_expr {:?}: {e}",
                page_id,
                field.name,
                expr
            )
        })?
    } else {
        field.bit_offset.unwrap_or(0) as i64
    };
    ensure!(
        value >= 0,
        "{}.{}: negative bit offset {}",
        page_id,
        field.name,
        value
    );
    Ok(value as u32)
}

fn bit_width(field: &PageField) -> u32 {
    field.bit_width.unwrap_or(match field.ty {
        FieldType::Bool => 1,
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        _ => 8,
    })
}

fn byte_width(field: &PageField) -> anyhow::Result<u32> {
    if is_sub_byte(field) {
        return Ok(1);
    }

    let width = match field.ty {
        FieldType::Bool
        | FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7
        | FieldType::U8
        | FieldType::I8
        | FieldType::EnumRef => 1,
        FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::F16Be
        | FieldType::X16Be => 2,
        FieldType::U32Be | FieldType::I32Be => 4,
        FieldType::U64Le | FieldType::U64Be => 8,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite | FieldType::Bitmap => {
            field.size_bytes.unwrap_or(1)
        }
    };

    ensure!(width > 0, "{}: byte width must be positive", field.name);
    Ok(width)
}

fn is_sub_byte(field: &PageField) -> bool {
    matches!(
        field.ty,
        FieldType::Bool
            | FieldType::U2
            | FieldType::U3
            | FieldType::U4
            | FieldType::U5
            | FieldType::U6
            | FieldType::U7
    ) || field.bit_offset.is_some()
        || field.bit_offset_expr.is_some()
        || field.bit_width.is_some()
}

fn field_type_name(ty: &FieldType) -> &'static str {
    match ty {
        FieldType::Bool => "bool",
        FieldType::U2 => "u2",
        FieldType::U3 => "u3",
        FieldType::U4 => "u4",
        FieldType::U5 => "u5",
        FieldType::U6 => "u6",
        FieldType::U7 => "u7",
        FieldType::U8 => "u8",
        FieldType::I8 => "i8",
        FieldType::U16Be => "u16be",
        FieldType::I16Be => "i16be",
        FieldType::U16Le => "u16le",
        FieldType::U32Be => "u32be",
        FieldType::I32Be => "i32be",
        FieldType::U64Le => "u64le",
        FieldType::U64Be => "u64be",
        FieldType::F16Be => "f16be",
        FieldType::X16Be => "x16be",
        FieldType::Ascii => "ascii",
        FieldType::Bitmap => "bitmap",
        FieldType::Bytes => "bytes",
        FieldType::Composite => "composite",
        FieldType::EnumRef => "enum_ref",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, offset: u32) -> PageField {
        PageField {
            name: name.to_string(),
            display_name: None,
            offset: Some(offset),
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: Some(1),
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
            since: None,
            until: None,
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
        }
    }

    #[test]
    fn validates_fields_in_both_halves_of_256_byte_page() {
        let schema = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v1".to_string(),
            kind: "page".to_string(),
            id: "Sff8472A0hMemory".to_string(),
            title: "SFF-8472 A0h Memory".to_string(),
            page_address: "A0h".to_string(),
            size_bytes: Some(256),
            field_includes: vec![],
            fields: vec![field("lower_half", 0), field("upper_half", 200)],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };

        validate_pages(&[schema]).expect("both halves are valid within an A0h page");
    }
}
