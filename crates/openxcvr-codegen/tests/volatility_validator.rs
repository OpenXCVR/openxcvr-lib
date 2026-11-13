//! Opt-in D1 volatility gate tests.
//!
//! Mirrors the Python side (`tools/test_validate_schemas.py` check_volatility_*)
//! over the typed schemas the codegen loads: every rule the gate enforces has
//! a synthetic failing fixture that pins the diagnostic, plus the positive
//! shapes the task names (reversed field order, explicit-false is_dynamic,
//! mixed readable children, container+children, startup acknowledgment plus
//! Unknown, per-revision coverage, template/include resolution).

use openxcvr_codegen::ir::pages::{Access, ReadEffectMetadata, ReadEffectScope};
use openxcvr_codegen::ir::version::Rev;
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};
use openxcvr_codegen::validator::validate_volatility;

fn page(fields: &str) -> String {
    format!(
        "schema: openxcvr.page.v1\nkind: page\nid: Synthetic\ntitle: Synthetic\n\
         page_address: lower\nsize_bytes: 16\nfields:\n{fields}"
    )
}

fn upper_page(fields: &str) -> String {
    format!(
        "schema: openxcvr.page.v1\nkind: page\nid: SyntheticUpper\ntitle: Synthetic Upper\n\
         page_address: 01h\nsize_bytes: 128\nfields:\n{fields}"
    )
}

fn check(fields: &str) -> anyhow::Result<()> {
    let schema = parse_page_schema(&page(fields))?;
    validate_volatility(&[schema])
}

/// A fully covered 16-byte page exercising every legal D1 shape: explicit
/// Unknown reserved bits, mixed readable children, instance expansion, a
/// revision swap, a template user and a byte-scoped startup acknowledgment.
fn clean_fields() -> String {
    [
        "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n",
        "  - { name: temperature, offset: 1, size_bytes: 2, type: u16be, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: bit0, offset: 2, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: bits_1_2, offset: 2, bit_offset: 1, bit_width: 2, type: u2, access: rw, flags: { is_dynamic: false } }\n",
        "  - { name: reserved_2_3, offset: 2, bit_offset: 3, bit_width: 2, type: u2, access: unknown, flags: { is_dynamic: false } }\n",
        "  - { name: bits_5_7, offset: 2, bit_offset: 5, bit_width: 3, type: u3, access: ro_cor, flags: { is_dynamic: true } }\n",
        "  - name: lane_flag\n    offset_expr: '3 + i'\n    instances: { index: i, start: 0, end: 3 }\n    size_bytes: 1\n    type: u8\n    access: ro_cor\n    flags: { is_dynamic: true }\n",
        "  - { name: legacy_control, offset: 7, size_bytes: 1, type: u8, access: rw, until: '4.0', flags: { is_dynamic: false } }\n",
        "  - { name: modern_control, offset: 7, size_bytes: 1, type: u8, access: rw, since: '4.0', flags: { is_dynamic: false } }\n",
        "  - { name: telemetry, offset: 8, size_bytes: 6, type: bytes, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: status_control, offset: 14, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: data_not_ready, offset: 14, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: rx_los, offset: 14, bit_offset: 1, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: tx_disable, offset: 14, bit_offset: 7, bit_width: 1, type: bool, access: rw, flags: { is_dynamic: false } }\n",
        "  - { name: reserved_14, offset: 14, bit_offset: 2, bit_width: 5, type: u5, access: unknown, flags: { is_dynamic: false } }\n",
        "  - name: startup_ack\n    offset: 15\n    size_bytes: 1\n    type: u8\n    access: ro\n    read_effect: acknowledge_startup_interrupt\n    read_effect_scope: byte\n    read_effect_provenance: { source: primary, clause: startup }\n    flags: { is_dynamic: true }\n",
    ]
        .concat()
}

#[test]
fn clean_page_passes() {
    check(&clean_fields()).expect("a fully marked and covered page must pass");
}

#[test]
fn reversed_field_order_passes() {
    // Reverse the top-level field entries: coverage and conflict checks
    // must be order-independent, never resolved by list position.
    let mut entries: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in clean_fields().lines() {
        if line.starts_with("  - ") {
            if !current.is_empty() {
                entries.push(current.clone());
            }
            current = line.to_string();
        } else {
            current.push_str(line);
        }
        current.push('\n');
    }
    entries.push(current);
    let reversed: String = entries.into_iter().rev().collect();
    check(&reversed).expect("order must never decide coverage or conflicts");
}

#[test]
fn missing_access_marking_fails() {
    let fields = clean_fields().replace(
        "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n",
        "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, flags: { is_dynamic: false } }\n",
    );
    let err = check(&fields).expect_err("missing access must fail");
    assert!(
        err.to_string().contains("missing access marking"),
        "got: {err}"
    );
}

#[test]
fn omitted_is_dynamic_fails_but_explicit_false_passes() {
    let omitted = clean_fields().replace(
        "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n",
        "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro }\n",
    );
    let err = check(&omitted).expect_err("omitted is_dynamic must fail");
    assert!(
        err.to_string().contains("missing flags.is_dynamic"),
        "got: {err}"
    );
    // The clean tree carries an explicit false on `identifier`: it must pass
    // (omission and explicit false are distinguishable in the IR).
    check(&clean_fields()).expect("explicit false is a valid conservative marking");
}

#[test]
fn retired_latching_spelling_fails() {
    let fields = clean_fields().replace(
        "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n",
        "  - name: identifier\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: ro\n    behavior:\n      latching: clear_on_read\n    flags: { is_dynamic: false }\n",
    );
    let err = check(&fields).expect_err("behavior.latching is a retired spelling");
    assert!(
        err.to_string().contains("retired marking spelling"),
        "got: {err}"
    );
}

#[test]
fn bit7_removal_leaves_uncovered_bit() {
    // Byte 2 loses its bits 5-7 field: the hole must be reported even though
    // the byte has other fields.
    let fields = clean_fields().replace(
        "  - { name: bits_5_7, offset: 2, bit_offset: 5, bit_width: 3, type: u3, access: ro_cor, flags: { is_dynamic: true } }\n",
        "",
    );
    let err = check(&fields).expect_err("an intra-byte hole must fail");
    let msg = format!("{err}");
    assert!(msg.contains("byte 2"), "got: {msg}");
    assert!(msg.contains("[5, 6, 7]"), "got: {msg}");
}

#[test]
fn container_is_not_evidence_for_missing_child() {
    // The raw status_control container stays, but its bit-0 child is removed:
    // the container must not fill the hole.
    let fields = clean_fields().replace(
        "  - { name: data_not_ready, offset: 14, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "",
    );
    let err = check(&fields).expect_err("a container cannot fill a missing child");
    let msg = format!("{err}");
    assert!(msg.contains("byte 14") && msg.contains("[0]"), "got: {msg}");
}

#[test]
fn conflicting_overlap_fails() {
    let fields = clean_fields().replace(
        "  - { name: bit0, offset: 2, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: bit0, offset: 2, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n  - { name: bit0_writable, offset: 2, bit_offset: 0, bit_width: 1, type: bool, access: rw, flags: { is_dynamic: false } }\n",
    );
    let err = check(&fields).expect_err("disagreeing overlap must fail");
    assert!(
        err.to_string().contains("disagreeing markings"),
        "got: {err}"
    );
}

#[test]
fn conditional_overlay_is_not_a_conflict() {
    let fields = clean_fields()
        .replace(
            "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n",
            "  - { name: identifier, offset: 0, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n  - name: alt_identifier\n    offset: 0\n    size_bytes: 1\n    type: u8\n    access: rw\n    flags: { is_dynamic: false }\n    present_when: 'identifier == 1'\n",
        );
    check(&fields).expect("present_when overlays are legal");
}

#[test]
fn invalid_bit_bounds_fail() {
    let fields = [
        "  - { name: bad_span, offset: 0, bit_offset: 6, bit_width: 3, type: u3, access: ro, flags: { is_dynamic: false } }\n",
        "  - { name: rest, offset: 1, size_bytes: 15, type: bytes, access: unknown, flags: { is_dynamic: false } }\n",
    ]
    .concat();
    let err = check(&fields).expect_err("a bit span crossing its byte must fail");
    assert!(err.to_string().contains("invalid bit bounds"), "got: {err}");
}

#[test]
fn effect_without_scope_fails() {
    let fields = clean_fields().replace(
        "  - { name: bit0, offset: 2, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: bit0, offset: 2, bit_offset: 0, bit_width: 1, type: bool, access: ro, read_effect: clear_on_read, flags: { is_dynamic: true } }\n",
    );
    let err = check(&fields).expect_err("explicit read_effect requires a scope");
    assert!(
        err.to_string().contains("requires read_effect_scope"),
        "got: {err}"
    );
}

#[test]
fn byte_scoped_effect_without_citation_fails() {
    let fields = clean_fields().replace(
        "    read_effect_provenance: { source: primary, clause: startup }\n",
        "",
    );
    let err = check(&fields).expect_err("byte-scoped effects need a citation");
    assert!(
        err.to_string().contains("read_effect_provenance citation"),
        "got: {err}"
    );
}

/// The parser rejects these shapes on the raw declaration (`validate_effects`
/// and the cross-field rule in `parser/pages.rs`), so a YAML fixture never
/// reaches the gate's own checks: the parse fails first. The two tests below
/// therefore parse the clean page and inject the contradictory typed state
/// afterwards — exactly the IR a programmatic emitter (or per-revision
/// resolution the parser did not see) could hand the gate. The gate must
/// hold on its own, not only behind the parser.
#[test]
fn access_effect_contradiction_reaches_the_gate() {
    // ro_cor implies ClearOnRead; an explicit read_effect: none on the same
    // field contradicts it (mirrors the Python gate's effect_contradiction).
    let mut schema = parse_page_schema(&page(&clean_fields())).unwrap();
    let f = &mut schema.fields[0];
    f.access = Some(Access::RoCor);
    f.metadata.read_effect = Some(ReadEffectMetadata::None);
    f.metadata.read_effect_scope = Some(ReadEffectScope::Field);
    let err = validate_volatility(&[schema]).expect_err("ro_cor + none must fail");
    assert!(
        err.to_string().contains("access/effect contradiction"),
        "got: {err}"
    );
    // Control: the same field with a matching explicit effect passes.
    let mut schema = parse_page_schema(&page(&clean_fields())).unwrap();
    let f = &mut schema.fields[0];
    f.access = Some(Access::RoCor);
    f.metadata.read_effect = Some(ReadEffectMetadata::ClearOnRead);
    f.metadata.read_effect_scope = Some(ReadEffectScope::Field);
    validate_volatility(&[schema]).expect("ro_cor + clear_on_read agrees");
}

#[test]
fn byte_effect_vs_none_neighbour_reaches_the_gate() {
    // Byte-scoped startup acknowledgment + an explicit read_effect: none
    // neighbour on byte 15 (markings aligned so only the effect rule can
    // fire): rejected by the gate over the typed IR.
    let mut schema = parse_page_schema(&page(&clean_fields())).unwrap();
    let mut neighbour = parse_page_schema(&page(
        "  - { name: safe_neighbour, offset: 15, size_bytes: 1, type: u8, access: ro, read_effect: none, read_effect_scope: field, read_effect_provenance: { source: synthetic test authority }, flags: { is_dynamic: true } }\n",
    ))
    .unwrap();
    let safe = neighbour.fields.remove(0);
    schema.fields.push(safe);
    let err = validate_volatility(&[schema]).expect_err("byte effect vs none neighbour");
    let msg = format!("{err}");
    assert!(
        msg.contains("side-effect-free field safe_neighbour") && msg.contains("byte 15"),
        "got: {msg}"
    );
}

#[test]
fn startup_acknowledgment_plus_unknown_passes() {
    // A byte-scoped startup acknowledgment coexisting with an Unknown
    // reserved constituent is representable and conservative: never a
    // contradiction, never readiness qualification.
    let fields = [
        "  - { name: filler, offset: 0, size_bytes: 15, type: bytes, access: unknown, flags: { is_dynamic: false } }\n",
        "  - name: startup_ack\n    offset: 15\n    size_bytes: 1\n    type: u8\n    access: ro\n    read_effect: acknowledge_startup_interrupt\n    read_effect_scope: byte\n    read_effect_provenance: { source: primary, clause: startup }\n    flags: { is_dynamic: true }\n",
        "  - { name: status_bits_0_6, offset: 15, bit_offset: 0, bit_width: 7, type: u7, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: unknown_bit_7, offset: 15, bit_offset: 7, bit_width: 1, type: bool, access: unknown, flags: { is_dynamic: false } }\n",
    ]
        .concat();
    check(&fields).expect("acknowledgment plus Unknown stays representable");
}

#[test]
fn revision_swap_keeps_coverage_and_its_loss_fails() {
    // Byte 7 swaps rw -> rw at 4.0: covered at every revision.
    check(&clean_fields()).expect("the swap keeps byte 7 covered");
    // Remove the successor: byte 7 must be uncovered at 4.0 and later.
    let fields = clean_fields().replace(
        "  - { name: modern_control, offset: 7, size_bytes: 1, type: u8, access: rw, since: '4.0', flags: { is_dynamic: false } }\n",
        "",
    );
    let err = check(&fields).expect_err("losing the successor leaves a hole");
    let msg = format!("{err}");
    assert!(msg.contains("byte 7"), "got: {msg}");
    assert!(msg.contains(Rev::V4_0.as_str()), "got: {msg}");
}

#[test]
fn uncovered_byte_fails() {
    let fields = clean_fields().replace(
        "  - { name: telemetry, offset: 8, size_bytes: 6, type: bytes, access: ro, flags: { is_dynamic: true } }\n",
        "",
    );
    let err = check(&fields).expect_err("an uncovered byte must fail");
    assert!(err.to_string().contains("byte 8 uncovered"), "got: {err}");
}

#[test]
fn template_and_field_include_resolution_is_validated() {
    // load_page_schema resolves templates and field_includes; the gate must
    // judge the resolved marking (template supplies both marking keys here).
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("_index.yaml"),
        "templates:\n  vol_ro_static:\n    access: ro\n    flags:\n      is_dynamic: false\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("fields")).unwrap();
    std::fs::write(
        dir.path().join("fields").join("part.yaml"),
        "fields:\n  - { name: identifier, offset: 0, size_bytes: 1, type: u8, template: vol_ro_static }\n  - { name: reserved, offset: 1, size_bytes: 15, type: bytes, access: unknown, flags: { is_dynamic: false } }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("page.yaml"),
        "schema: openxcvr.page.v1\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 16\nfield_includes:\n  - fields/part.yaml\nfields: []\n",
    )
    .unwrap();

    let schema = load_page_schema(&dir.path().join("page.yaml")).unwrap();
    validate_volatility(&[schema]).expect("a template-resolved marking satisfies the gate");

    // The same tree with a template missing is_dynamic must fail on the
    // resolved field, proving the gate sees the resolved marking.
    std::fs::write(
        dir.path().join("_index.yaml"),
        "templates:\n  vol_ro_static:\n    access: ro\n",
    )
    .unwrap();
    let schema = load_page_schema(&dir.path().join("page.yaml")).unwrap();
    let err = validate_volatility(&[schema]).expect_err("missing resolved is_dynamic must fail");
    assert!(
        err.to_string().contains("missing flags.is_dynamic"),
        "got: {err}"
    );
}

#[test]
fn upper_page_range_byte_hole_fails() {
    // Advisory (adopted): the base-128 upper-page byte range — where task 19
    // will make the gate mandatory — must be exercised. Flat offsets ≥128 on
    // a `page_address: 01h` 128-byte page: the clean form passes; a removed
    // byte and a removed bit each fail with the upper-page offset named.
    let fields = [
        "  - { name: identifier, offset: 128, size_bytes: 1, type: u8, access: ro, flags: { is_dynamic: false } }\n",
        "  - { name: reserved_129, offset: 129, size_bytes: 1, type: u8, access: unknown, flags: { is_dynamic: false } }\n",
        "  - { name: flag_bit0, offset: 130, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: flag_rest, offset: 130, bit_offset: 1, bit_width: 7, type: u7, access: unknown, flags: { is_dynamic: false } }\n",
        "  - { name: reserved_131, offset: 131, size_bytes: 1, type: u8, access: unknown, flags: { is_dynamic: false } }\n",
        "  - { name: telemetry, offset: 132, size_bytes: 6, type: bytes, access: ro, flags: { is_dynamic: true } }\n",
        "  - { name: reserved_tail, offset: 138, size_bytes: 118, type: bytes, access: unknown, flags: { is_dynamic: false } }\n",
    ]
    .concat();
    let schema = parse_page_schema(&upper_page(&fields)).unwrap();
    validate_volatility(&[schema]).expect("a fully marked upper page passes");

    let no_byte = fields.replace(
        "  - { name: telemetry, offset: 132, size_bytes: 6, type: bytes, access: ro, flags: { is_dynamic: true } }\n",
        "",
    );
    let schema = parse_page_schema(&upper_page(&no_byte)).unwrap();
    let err = validate_volatility(&[schema]).expect_err("an uncovered upper byte fails");
    assert!(err.to_string().contains("byte 132 uncovered"), "got: {err}");

    let no_bit = fields.replace(
        "  - { name: flag_bit0, offset: 130, bit_offset: 0, bit_width: 1, type: bool, access: ro, flags: { is_dynamic: true } }\n",
        "",
    );
    let schema = parse_page_schema(&upper_page(&no_bit)).unwrap();
    let err = validate_volatility(&[schema]).expect_err("an upper-page bit hole fails");
    let msg = format!("{err}");
    assert!(
        msg.contains("byte 130") && msg.contains("[0]"),
        "got: {msg}"
    );
}
