//! Shared language-neutral checked-page resolution plan (python-checked-pages
//! plan, Task 1).
//!
//! `ir::checked::plan_checked_page` resolves every original field for every
//! family revision after `applicable_revs` filtering, records the stable
//! emitted union keyed by ORIGINAL identity, and refuses ambiguous
//! original/resolved name collisions per revision. These tests pin that
//! contract on synthetic schemas before any Python emission exists:
//! promotion of a base `emit_value: false` field by `changed_in`, inclusive
//! `since` / exclusive `until` windows, per-revision shifted bit widths and
//! byte relocation across members, rename/alias identity stability, and
//! fail-loud collision refusal with page/field/revision context.
use std::collections::BTreeMap;

use openxcvr_codegen::ir::checked::{plan_checked_page, CheckedPagePlan};
use openxcvr_codegen::ir::version::Rev;
use openxcvr_codegen::parser::pages::parse_page_schema;

fn page_yaml(fields: &str) -> String {
    format!(
        concat!(
            "schema: openxcvr.page.v2\n",
            "kind: page\n",
            "id: TestPage\n",
            "title: T\n",
            "page_address: \"01h\"\n",
            "size_bytes: 128\n",
            "fields:\n",
            "{fields}"
        ),
        fields = fields
    )
}

fn plan(fields: &str) -> CheckedPagePlan {
    let schema = parse_page_schema(&page_yaml(fields)).expect("parse");
    plan_checked_page(&schema).expect("checked page plan")
}

fn refusal(fields: &str, context: &[&str]) {
    let schema = parse_page_schema(&page_yaml(fields)).expect("parse");
    let err = plan_checked_page(&schema).expect_err("ambiguous resolution must be refused");
    for token in context {
        assert!(err.contains(token), "missing {token:?}: {err}");
    }
}

fn resolved(
    plan: &CheckedPagePlan,
    member: usize,
    rev: Rev,
) -> &Option<openxcvr_codegen::ir::pages::PageField> {
    &plan
        .revisions
        .get(&rev)
        .unwrap_or_else(|| panic!("no revision {rev:?}"))
        .fields[member]
}

fn member(plan: &CheckedPagePlan, name: &str) -> usize {
    plan.members
        .iter()
        .position(|m| m.original_name == name)
        .unwrap_or_else(|| {
            panic!(
                "no member {name:?}: {:?}",
                plan.members
                    .iter()
                    .map(|m| &m.original_name)
                    .collect::<Vec<_>>()
            )
        })
}

/// A base `emit_value: false` field promoted by `changed_in` at 4.0 belongs
/// to the stable union, is absent before/after its applicability window, and
/// its resolved field at 4.0 emits a value. The base-emitting sibling is a
/// member everywhere. Member order follows original schema order.
#[test]
fn emit_value_false_promoted_at_4_0_joins_stable_union() {
    let p = plan(concat!(
        "  - { name: base_field, offset: 128, type: u8 }\n",
        "  - { name: promoted_4_0, offset: 130, type: u8, since: \"4.0\", until: \"5.0\", emit_value: false, changed_in: [ { rev: \"4.0\", emit_value: true } ] }\n",
    ));
    assert_eq!(p.members.len(), 2);
    assert_eq!(p.members[0].original_name, "base_field");
    assert_eq!(p.members[0].original_index, 0);
    assert_eq!(p.members[1].original_name, "promoted_4_0");
    assert_eq!(p.members[1].original_index, 1);
    // Stable identity carries the ORIGINAL (unpromoted) field.
    assert!(!p.members[1].original.emits_value());
    // 3.0: promoted field not applicable.
    assert!(resolved(&p, 1, Rev::V3_0).is_none());
    assert!(resolved(&p, 0, Rev::V3_0).is_some());
    // 4.0: applicable and emitting after changed_in promotion.
    let at4 = resolved(&p, 1, Rev::V4_0)
        .as_ref()
        .expect("applicable at 4.0");
    assert!(at4.emits_value());
    // 5.x: window closed (until exclusive).
    assert!(resolved(&p, 1, Rev::V5_0).is_none());
    assert!(resolved(&p, 1, Rev::V5_3).is_none());
}

/// A field that never emits a value in any applicable revision is NOT a
/// union member — the plan records no presence for it.
#[test]
fn never_emitting_field_is_not_a_union_member() {
    let p = plan(concat!(
        "  - { name: emitted, offset: 128, type: u8 }\n",
        "  - { name: hidden, offset: 129, type: u8, emit_value: false }\n",
    ));
    assert_eq!(p.members.len(), 1);
    assert_eq!(p.members[0].original_name, "emitted");
}

/// `since` is inclusive, `until` exclusive, across the six family revisions.
#[test]
fn applicability_window_is_since_inclusive_until_exclusive() {
    let p = plan("  - { name: windowed, offset: 128, type: u8, since: \"4.0\", until: \"5.2\" }\n");
    assert_eq!(p.members.len(), 1);
    assert!(resolved(&p, 0, Rev::V3_0).is_none());
    assert!(resolved(&p, 0, Rev::V4_0).is_some());
    assert!(resolved(&p, 0, Rev::V5_0).is_some());
    assert!(resolved(&p, 0, Rev::V5_1).is_some());
    assert!(resolved(&p, 0, Rev::V5_2).is_none());
    assert!(resolved(&p, 0, Rev::V5_3).is_none());
}

/// The plan carries the per-revision RESOLVED field, so a member whose read
/// footprint shifts across revisions (changed_in bit_width) is recorded with
/// each revision's own footprint, and two members may relocate across the
/// same wire byte in disjoint windows.
#[test]
fn shifted_widths_and_relocated_bytes_are_recorded_per_revision() {
    let p = plan(concat!(
        "  - { name: wide_then_narrow, offset: 128, type: u8, bit_offset: 0, bit_width: 4, changed_in: [ { rev: \"5.0\", bit_width: 2 } ] }\n",
        "  - { name: old_owner, offset: 130, type: u8, until: \"5.0\" }\n",
        "  - { name: new_owner, offset: 130, type: u8, since: \"5.0\" }\n",
    ));
    assert_eq!(p.members.len(), 3);
    // One member, per-revision shifted footprint.
    let w3 = resolved(&p, 0, Rev::V3_0).as_ref().expect("at 3.0");
    let w5 = resolved(&p, 0, Rev::V5_0).as_ref().expect("at 5.0");
    assert_eq!(w3.effective_bit_width(), 4);
    assert_eq!(w5.effective_bit_width(), 2);
    // Byte 130 relocates from old_owner to new_owner at 5.0: both members
    // stay in the union, each absent exactly where the other applies.
    assert!(resolved(&p, 1, Rev::V4_0).is_some());
    assert!(resolved(&p, 2, Rev::V4_0).is_none());
    assert!(resolved(&p, 1, Rev::V5_0).is_none());
    assert!(resolved(&p, 2, Rev::V5_0).is_some());
}

/// renamed_from / aliased_to change the RESOLVED name but never the stable
/// member identity: original_name and original.name stay the schema's current
/// spelling.
#[test]
fn rename_and_alias_keep_original_identity() {
    let p = plan(concat!(
        "  - { name: vendor_sn, offset: 128, type: u8, renamed_from: { rev: \"5.0\", old_name: vendor_id } }\n",
        "  - { name: lane_flag, offset: 129, type: u8, aliased_to: [ { rev: \"5.1\", new_name: lane_status_flag } ] }\n",
    ));
    let renamed = &p.members[member(&p, "vendor_sn")];
    assert_eq!(renamed.original_name, "vendor_sn");
    assert_eq!(renamed.original.name, "vendor_sn");
    assert_eq!(
        resolved(&p, member(&p, "vendor_sn"), Rev::V4_0)
            .as_ref()
            .expect("applicable")
            .name,
        "vendor_id"
    );
    assert_eq!(
        resolved(&p, member(&p, "vendor_sn"), Rev::V5_0)
            .as_ref()
            .expect("applicable")
            .name,
        "vendor_sn"
    );
    let aliased = &p.members[member(&p, "lane_flag")];
    assert_eq!(aliased.original_name, "lane_flag");
    assert_eq!(
        resolved(&p, member(&p, "lane_flag"), Rev::V5_0)
            .as_ref()
            .expect("applicable")
            .name,
        "lane_flag"
    );
    assert_eq!(
        resolved(&p, member(&p, "lane_flag"), Rev::V5_1)
            .as_ref()
            .expect("applicable")
            .name,
        "lane_status_flag"
    );
}

/// Two original fields sharing a name is a stable-member collision, refused
/// with page/field context.
#[test]
fn duplicate_original_names_are_refused() {
    refusal(
        concat!(
            "  - { name: twin, offset: 128, type: u8 }\n",
            "  - { name: twin, offset: 129, type: u8 }\n",
        ),
        &["TestPage", "twin", "stable member collision"],
    );
}

/// A rename that projects a resolved name onto another member's resolved
/// name is a per-revision collision, refused naming the page, both members
/// and the first colliding revision — which for a `renamed_from` projection
/// is the FIRST family revision, since the old spelling applies to every
/// revision before the rename.
#[test]
fn per_revision_resolved_name_collision_is_refused() {
    refusal(
        concat!(
            "  - { name: current_name, offset: 128, type: u8, renamed_from: { rev: \"5.0\", old_name: legacy_name } }\n",
            "  - { name: legacy_name, offset: 129, type: u8 }\n",
        ),
        &["TestPage", "current_name", "legacy_name", "3.0", "collision"],
    );
}

/// A same-name alias projection (aliased_to new_name equal to another
/// member's resolved name at that revision) is likewise refused.
#[test]
fn per_revision_alias_collision_is_refused() {
    refusal(
        concat!(
            "  - { name: a_flag, offset: 128, type: u8, aliased_to: [ { rev: \"5.2\", new_name: shared_spelling } ] }\n",
            "  - { name: shared_spelling, offset: 129, type: u8 }\n",
        ),
        &["TestPage", "a_flag", "shared_spelling", "5.2", "collision"],
    );
}

/// The plan covers every family revision, each mapping index-aligned with the
/// ordered member list (None where a member is not applicable).
#[test]
fn revisions_cover_the_whole_family_domain() {
    let p = plan("  - { name: only, offset: 128, type: u8 }\n");
    let mut expected = BTreeMap::new();
    for rev in Rev::CMIS_ALL {
        expected.insert(*rev, true);
    }
    assert_eq!(p.revisions.len(), expected.len());
    for (rev, _) in expected {
        assert!(
            resolved(&p, 0, rev).is_some(),
            "unconditional member absent at {rev:?}"
        );
    }
}
