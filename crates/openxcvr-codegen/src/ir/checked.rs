//! Language-neutral checked-page resolution plan.
//!
//! [`plan_checked_page`] resolves every original field of a page for every
//! family revision — the page's own `spec.id` family domain (CMIS,
//! SFF-8636, SFF-8472; CMIS for spec-less schemas) — after `applicable_revs`
//! filtering and
//! records the stable emitted union: the set of original fields that emit a
//! value in at least one applicable revision, keyed by their ORIGINAL
//! identity so a resolver rename/alias projects into a stable member name.
//!
//! The plan is codegen-only and representation-neutral: it records the
//! resolved field per (member, revision) — including revision-shifted
//! footprints (`changed_in` width/type overrides) and byte relocation across
//! members — but refuses nothing on representation grounds. Each language
//! emitter checks destination representation and post-naming collisions
//! against its own supported extraction. What the shared plan DOES refuse is
//! ambiguity it owns: a duplicate original field name (stable member
//! collision) and a per-revision collision where two members' resolved names
//! coincide (page/field/revision context in the error).
use std::collections::BTreeMap;

use crate::ir::pages::{PageField, PageSchema};
use crate::ir::version::{applicable_revs, family_revs_for_spec, resolve_field_for_rev, Rev};

/// The revision domain a page's checked plan resolves against: the family
/// of the page's own `spec.id` (`cmis`, `sff-8636`, `sff-8472`). A schema
/// without a spec block keeps the historical CMIS domain so synthetic and
/// union inputs behave exactly as before.
pub fn plan_domain(schema: &PageSchema) -> &'static [Rev] {
    family_revs_for_spec(schema.spec.as_ref().map(|spec| spec.id.as_str()))
}

/// One stable union member: the ORIGINAL field, its schema order and its
/// original name. The original is kept unmodified (base `emit_value: false`
/// stays false here even when a revision promotes it) so language emitters
/// can compare against their raw output and project resolved reads into the
/// stable identity.
#[derive(Debug, Clone)]
pub struct CheckedMember {
    pub original_index: usize,
    pub original_name: String,
    pub original: PageField,
}

/// Per-revision resolution: one entry per plan member, index-aligned with
/// [`CheckedPagePlan::members`]. `None` where the member's applicability
/// window does not cover the revision; otherwise the revision-resolved field
/// (rename/alias applied, `changed_in` overrides accumulated).
#[derive(Debug, Clone)]
pub struct CheckedRevision {
    pub fields: Vec<Option<PageField>>,
}

/// The checked page plan: ordered stable union members plus one
/// [`CheckedRevision`] per family revision.
#[derive(Debug, Clone)]
pub struct CheckedPagePlan {
    pub members: Vec<CheckedMember>,
    pub revisions: BTreeMap<Rev, CheckedRevision>,
}

/// Resolve `schema` into a checked plan. Fails loud, with page/field/
/// revision context, on the two ambiguities the shared plan owns: duplicate
/// original member names and per-revision resolved-name collisions among
/// emitting members.
pub fn plan_checked_page(schema: &PageSchema) -> Result<CheckedPagePlan, String> {
    let domain = plan_domain(schema);
    // Pass 1: resolve every field for every family revision, in schema order.
    // resolved[i] is index-aligned with schema.fields; resolved[i][rev-slot]
    // is Some(field) where the field is applicable at that revision.
    let mut resolved: Vec<Vec<Option<PageField>>> = Vec::with_capacity(schema.fields.len());
    for original in &schema.fields {
        let applicable = applicable_revs(original, domain);
        let per_rev = domain
            .iter()
            .map(|&rev| {
                applicable
                    .contains(&rev)
                    .then(|| resolve_field_for_rev(original, rev))
            })
            .collect();
        resolved.push(per_rev);
    }

    // Pass 2: stable union membership — the original field emits a value in
    // at least one applicable revision. Ordering follows schema order.
    let mut members: Vec<CheckedMember> = Vec::new();
    let mut member_of_field: Vec<Option<usize>> = Vec::with_capacity(schema.fields.len());
    let mut seen_names: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, original) in schema.fields.iter().enumerate() {
        let in_union = resolved[index]
            .iter()
            .flatten()
            .any(|f: &PageField| f.emits_value());
        if !in_union {
            member_of_field.push(None);
            continue;
        }
        if let Some(&first) = seen_names.get(original.name.as_str()) {
            return Err(format!(
                "{}.{}: stable member collision with field #{} of the same name",
                schema.id, original.name, first
            ));
        }
        seen_names.insert(original.name.as_str(), members.len());
        member_of_field.push(Some(members.len()));
        members.push(CheckedMember {
            original_index: index,
            original_name: original.name.clone(),
            original: original.clone(),
        });
    }

    // Pass 3: per-revision view, index-aligned with the member list, plus the
    // per-revision resolved-name collision check among emitting members.
    // Member indices are assigned in schema order, so iterating fields in
    // schema order pushes exactly one entry per member in order.
    let mut revisions = BTreeMap::new();
    for (slot, &rev) in domain.iter().enumerate() {
        let mut fields = Vec::with_capacity(members.len());
        let mut names: BTreeMap<String, usize> = BTreeMap::new();
        for (field_index, per_rev) in resolved.iter().enumerate() {
            let Some(member_index) = member_of_field[field_index] else {
                continue;
            };
            debug_assert_eq!(fields.len(), member_index);
            let entry = per_rev[slot].clone();
            if let Some(field) = &entry {
                if field.emits_value() {
                    if let Some(&first_member) = names.get(field.name.as_str()) {
                        return Err(format!(
                            "{}.{} revision {}: resolved name {:?} collision with member {} \
                             ({:?}); rename/alias projection is ambiguous",
                            schema.id,
                            members[member_index].original_name,
                            rev.as_str(),
                            field.name,
                            first_member,
                            members[first_member].original_name
                        ));
                    }
                    names.insert(field.name.clone(), member_index);
                }
            }
            fields.push(entry);
        }
        debug_assert_eq!(fields.len(), members.len());
        revisions.insert(rev, CheckedRevision { fields });
    }

    Ok(CheckedPagePlan { members, revisions })
}
