//! Real-IR and synthetic coverage of the Python checked revision emitter
//! (python-checked-pages plan, Task 2).
//!
//! Real IR: the seven target CMIS pages plan to the settled member counts
//! (111/45/126/43/31/3/7 = 366), six reuse their raw values type and only
//! Page 00h needs a stable CheckedValues union, Page00h's three CMIS4-only
//! members decode by direct per-revision reads in the 0x40 branch only, and
//! Lower memory's aliased/width-shifted `mci_max_speed` reads a per-revision
//! footprint under its stable original name. Synthetic schemas pin the
//! fail-loud refusals: incompatible Python representation, non-emittable
//! applicable member, unsupported shapes/extents/footprints/extractions.
use std::path::{Path, PathBuf};

use openxcvr_codegen::emitters::python::emit_checked_page;
use openxcvr_codegen::ir::checked::plan_checked_page;
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};

fn schemas_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

/// (schema id, path under schemas/, expected stable member count, whether the
/// page needs a CheckedValues union instead of its raw values type)
const TARGETS: &[(&str, &str, usize, bool)] = &[
    ("CmisLowerMemory", "cmis/lower-memory.yaml", 114, false),
    (
        "CmisPage00hAdministrative",
        "cmis/page-00h-administrative.yaml",
        45,
        true,
    ),
    (
        "CmisPage01hAdvertising",
        "cmis/page-01h-advertising.yaml",
        135,
        false,
    ),
    (
        "CmisPage11hLaneStatus",
        "cmis/page-11h-lane-status.yaml",
        43,
        false,
    ),
    (
        "CmisPage14hDiagnosticsResults",
        "cmis/page-14h-diag-results.yaml",
        31,
        false,
    ),
    (
        "CmisPage20hVdmDescriptors1To64",
        "cmis/page-20h-vdm-descriptors-1-64.yaml",
        3,
        false,
    ),
    (
        "CmisPage9FhCdbMessage",
        "cmis/page-9fh-cdb-message.yaml",
        7,
        false,
    ),
];

fn load(rel: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    load_page_schema(&schemas_root().join(rel)).expect("load real schema")
}

fn emit_real(rel: &str) -> (String, usize) {
    let schema = load(rel);
    let plan = plan_checked_page(&schema).expect("plan");
    let count = plan.members.len();
    let code = emit_checked_page(&schema, &plan).expect("emit checked page");
    (code, count)
}

/// The `if cmis_rev_byte == 0xNN:` branch of decode_for_revision.
fn branch(code: &str, byte: u8) -> String {
    let marker = format!("if cmis_rev_byte == 0x{byte:02X}:");
    let start = code
        .find(&marker)
        .unwrap_or_else(|| panic!("no {marker} branch in:\n{code}"));
    let rest = &code[start + marker.len()..];
    let end = rest
        .find("\n        if cmis_rev_byte == 0x")
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

/// The seven target pages emit with the settled stable member counts; exactly
/// one (Page00h) requires a CheckedValues union; the other six reuse their
/// raw values type by import.
#[test]
fn seven_targets_counts_and_values_type_identities() {
    let mut total = 0usize;
    for (id, rel, expected, needs_values) in TARGETS {
        let (code, count) = emit_real(rel);
        assert_eq!(&count, expected, "{id} stable member count");
        total += count;
        let stem = openxcvr_codegen::ir::naming::camel_to_snake(id);
        assert!(
            code.contains(&format!("class {id}Presence:")),
            "{id}: presence dataclass missing"
        );
        assert!(
            code.contains(&format!("class {id}Checked:")),
            "{id}: checked dataclass missing"
        );
        assert!(
            code.contains("def decode_for_revision(cls, cmis_rev_byte: int, buf: bytes)"),
            "{id}: decode_for_revision classmethod missing"
        );
        if *needs_values {
            assert!(
                code.contains(&format!("class {id}CheckedValues:")),
                "{id}: CheckedValues union missing"
            );
            assert!(
                !code.contains(&format!("from .{stem} import")),
                "{id}: must not import its raw type when CheckedValues is used"
            );
            assert!(code.contains(&format!("values: {id}CheckedValues")));
        } else {
            assert!(
                !code.contains("CheckedValues"),
                "{id}: raw values type must be reused, not a CheckedValues union"
            );
            assert!(
                code.contains(&format!("from .{stem} import {id}")),
                "{id}: raw values import missing"
            );
            assert!(code.contains(&format!("values: {id}")));
        }
    }
    assert_eq!(total, 378, "total stable members across the seven pages");
}

/// Page00h's three CMIS4-only reserved members decode by direct reads only in
/// the 0x40 branch (buf[80] u8; buf[85] bit7 bool; buf[85] bits6:0 u7) and
/// default to 0/False/0 elsewhere; presence flags cover them.
#[test]
fn page00h_cmis4_only_members_direct_reads_in_0x40_branch_only() {
    let (code, _) = emit_real("cmis/page-00h-administrative.yaml");
    let at4 = branch(&code, 0x40);
    let at53 = branch(&code, 0x53);
    let at30 = branch(&code, 0x30);
    for needle in [
        "reserved_administrative_208_cmis4=buf[80]",
        "reserved_administrative_213_bit7_cmis4=bool((buf[85] >> 7) & 0x1)",
        "reserved_administrative_213_bits6_0_cmis4=buf[85] & 0x7F",
    ] {
        assert!(
            at4.contains(needle),
            "0x40 branch missing direct read {needle}:\n{at4}"
        );
    }
    for other in [&at53, &at30] {
        assert!(other.contains("reserved_administrative_208_cmis4=0"));
        assert!(other.contains("reserved_administrative_213_bit7_cmis4=False"));
        assert!(other.contains("reserved_administrative_213_bits6_0_cmis4=0"));
    }
    // Presence flags: present at 0x40, absent at 0x53/0x30.
    assert!(at4.contains("reserved_administrative_208_cmis4=True"));
    assert!(at53.contains("reserved_administrative_208_cmis4=False"));
    assert!(at30.contains("reserved_administrative_208_cmis4=False"));
}

/// Lower memory's `mci_max_speed` is aliased to `twi_max_speed_cmis4` at 4.0
/// and narrows to u2 (bits 3:2) through 5.2, widening to u4 (bits 5:2) at
/// 5.3. The stable member name stays `mci_max_speed` in every branch while
/// the read footprint shifts per revision — impossible for a flat
/// decode-then-mask approach.
#[test]
fn lower_memory_mci_max_speed_stable_name_shifted_footprint() {
    let (code, _) = emit_real("cmis/lower-memory.yaml");
    for byte in [0x30, 0x40, 0x50, 0x51, 0x52] {
        let b = branch(&code, byte);
        assert!(
            b.contains("mci_max_speed=(buf[2] >> 2) & 0x3"),
            "0x{byte:02X} branch must read the u2 footprint:\n{b}"
        );
        assert!(
            !b.contains("twi_max_speed_cmis4"),
            "0x{byte:02X} branch must keep the stable original member name:\n{b}"
        );
    }
    let at53 = branch(&code, 0x53);
    assert!(
        at53.contains("mci_max_speed=(buf[2] >> 2) & 0xF"),
        "0x53 branch must read the u4 footprint:\n{at53}"
    );
}

/// The Lower memory module embeds the revision cross-check (buf[1]) after the
/// length check; upper pages do not.
#[test]
fn lower_memory_embeds_revision_cross_check() {
    let lower = emit_real("cmis/lower-memory.yaml").0;
    assert!(lower.contains("if buf[1] != cmis_rev_byte:"));
    let upper = emit_real("cmis/page-01h-advertising.yaml").0;
    assert!(!upper.contains("buf[1] != cmis_rev_byte"));
}

// ─── synthetic refusals ───────────────────────────────────────────────────────

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

fn refusal(fields: &str, context: &[&str]) {
    let schema = parse_page_schema(&page_yaml(fields)).expect("parse");
    let plan = plan_checked_page(&schema).expect("plan");
    let err =
        emit_checked_page(&schema, &plan).expect_err("unrepresentable mapping must be refused");
    for token in context {
        assert!(err.contains(token), "missing {token:?}: {err}");
    }
}

/// A bool→u8 `changed_in` override is raw-valid (the raw page emits the base
/// bool) but flips the stable Python annotation bool→int: refused with
/// page/field/revision context, never silently retyped.
#[test]
fn incompatible_python_representation_is_refused() {
    refusal(
        "  - { name: kind_flag, offset: 128, type: bool, changed_in: [ { rev: \"5.0\", type: u8 } ] }\n",
        &[
            "TestPage",
            "kind_flag",
            "5.0",
            "incompatible Python representation",
        ],
    );
}

/// A str→bytes override (text to binary) is likewise refused.
#[test]
fn text_to_bytes_representation_is_refused() {
    refusal(
        "  - { name: label, offset: 128, size_bytes: 4, type: ascii, changed_in: [ { rev: \"5.0\", type: bytes } ] }\n",
        &["TestPage", "label", "5.0", "incompatible Python representation"],
    );
}

/// A scalar↔array shape change (instances appear on a resolved revision)
/// never happens through changed_in (no such key), but a base field whose
/// extent is unknown (bytes without size_bytes) is refused with shape
/// context.
#[test]
fn unknown_payload_extent_is_refused() {
    refusal(
        "  - { name: blob, offset: 128, type: bytes }\n",
        &["TestPage", "blob", "requires size_bytes"],
    );
}

/// An extraction extent beyond the page buffer is refused.
#[test]
fn out_of_page_extent_is_refused() {
    refusal(
        "  - { name: late, offset: 300, type: u8 }\n",
        &["TestPage", "late", "exceeds the 128-byte page"],
    );
}

/// A sub-byte footprint crossing the byte boundary would silently wrap under
/// the shared extractors; refused.
#[test]
fn cross_byte_bit_footprint_is_refused() {
    refusal(
        "  - { name: wide, offset: 128, bit_offset: 6, bit_width: 4, type: u4 }\n",
        &["TestPage", "wide", "bit extent"],
    );
}

/// A scalar with only an offset expression (no instances) has no direct-read
/// extraction; refused rather than silently omitted.
#[test]
fn expression_addressed_scalar_is_refused() {
    refusal(
        "  - { name: roam, offset_expr: \"128 + lane\", type: u8 }\n",
        &["TestPage", "roam", "unsupported extraction"],
    );
}

/// An offset below the page base is not representable; refused.
#[test]
fn negative_page_offset_is_refused() {
    refusal(
        "  - { name: low, offset: 8, type: u8 }\n",
        &["TestPage", "low", "extraction"],
    );
}

/// A union member that is applicable at a revision but suppressed there by
/// `emit_value` has no representable presence/value pair: refused, matching
/// the C checked emitter's contract.
#[test]
fn applicable_member_non_emittable_is_refused() {
    refusal(
        "  - { name: later, offset: 128, type: u8, changed_in: [ { rev: \"5.0\", emit_value: false } ] }\n",
        &["TestPage", "later", "5.0", "non-emittable"],
    );
}

// ─── two-byte enum_ref footprint (format_id) ───────────────────────────────────

/// A two-byte enum_ref whose first byte sits at the FINAL page byte is
/// refused at generation: decode_scalar reads a 16-bit big-endian
/// `unpack_from('>H')` for `size_bytes: 2`, so the admission footprint must
/// demand both bytes inside the page. Regression: `wire_bytes` previously
/// pinned enum_ref to one byte, letting such a field pass preflight and
/// emit a decoder that raises struct.error on the final-page-byte buffer
/// instead of failing closed at generation.
#[test]
fn two_byte_enum_ref_at_final_page_byte_is_refused() {
    refusal(
        "  - { name: fmt, offset: 255, size_bytes: 2, type: enum_ref, enum_ref: test-fmt }\n",
        &["TestPage", "fmt", "exceeds the 128-byte page"],
    );
}

/// A two-byte enum_ref that fits the page (last two bytes) is admitted and
/// its decoder reads the full 16-bit big-endian width at the page-relative
/// offset — admission and extraction stay consistent in the accepted
/// direction too (no over-rejection).
#[test]
fn two_byte_enum_ref_within_page_reads_sixteen_bit_width() {
    let schema = parse_page_schema(&page_yaml(
        "  - { name: fmt, offset: 254, size_bytes: 2, type: enum_ref, enum_ref: test-fmt }\n",
    ))
    .expect("parse");
    let plan = plan_checked_page(&schema).expect("plan");
    let code = emit_checked_page(&schema, &plan).expect("emit checked page");
    assert!(
        code.contains("fmt=struct.unpack_from('>H', buf, 126)[0]"),
        "a page-fitting 2-byte enum_ref must read the full 16-bit width:\n{code}"
    );
}

/// A declared enum_ref width decode_scalar does not implement (3 bytes)
/// fails closed at generation rather than admitting a footprint the
/// emitted decoder cannot read.
#[test]
fn wider_enum_ref_size_is_refused() {
    refusal(
        "  - { name: fmt3, offset: 128, size_bytes: 3, type: enum_ref, enum_ref: test-fmt }\n",
        &["TestPage", "fmt3", "unsupported enum_ref size_bytes"],
    );
}

/// The real two-byte `format_id` fields (SFF-8636 v2.12 § 7 and SFF-8472
/// v12.5a § 13 "128-129 | 2 | Format ID | CA1Bh") decode at full 16-bit
/// width in the checked path under the declared two-byte footprint.
#[test]
fn real_format_id_decodes_at_sixteen_bit_width() {
    let (code, _) = emit_real("sff-8636/page-22h-high-accuracy-timing.yaml");
    assert!(
        code.contains("format_id=struct.unpack_from('>H', buf, 0)[0]"),
        "SFF-8636 page 22h format_id must read 16-bit at page-relative 0:\n{code}"
    );
    let (code, _) = emit_real("sff-8472/page-a2h-03h-high-accuracy-timing.yaml");
    assert!(
        code.contains("format_id=struct.unpack_from('>H', buf, 128)[0]"),
        "SFF-8472 A2h page 03h format_id must read 16-bit at buf offset 128:\n{code}"
    );
}
