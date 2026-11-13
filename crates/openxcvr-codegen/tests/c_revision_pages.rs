//! Revision-checked C Page 01h/11h emission (checked-pages plan, Task 2).
//!
//! Covers the additive `*_revision.{h,c}` outputs: emission restricted to the
//! CMIS family and the two target page IDs (SFF families emit none), real
//! resolver/emitter coverage of since/until windows, compatible changed_in
//! type/width overrides, rename/alias member stability, refusal of
//! unrepresentable mappings (naming page/original field/revision), empty
//! applicability, CLI failure propagation without checked output, and
//! exhaustive member/revision coverage of the real CMIS pages.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use openxcvr_codegen::emitters::c::revision::{
    is_cmis_generation, plan_checked_page, CheckedPageOutput, CHECKED_PAGE_IDS,
};
use openxcvr_codegen::emitters::c::CEmitter;
use openxcvr_codegen::ir::version::{applicable_revs, Rev};
use openxcvr_codegen::parser::pages::{load_page_schema, parse_page_schema};

fn schemas_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

fn run_cli(schemas: &Path, spec: &str, out: &Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_openxcvr-codegen"))
        .arg("generate")
        .arg("--schemas")
        .arg(schemas)
        .arg("--spec")
        .arg(spec)
        .arg("--lang")
        .arg("c")
        .arg("--out")
        .arg(out)
        .output()
        .expect("codegen CLI runs")
}

fn collect_files(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("dir entry");
        if entry.path().is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

/// Member names of a generated `typedef struct <type_name> { ... }` block.
fn struct_members(text: &str, type_name: &str) -> Vec<String> {
    let open = format!("typedef struct {type_name} {{");
    let start = text
        .find(&open)
        .unwrap_or_else(|| panic!("no {open} in:\n{text}"));
    let body_start = start + open.len();
    let body_end = text[body_start..]
        .find("} ")
        .unwrap_or_else(|| panic!("no closing brace for {type_name} in:\n{text}"));
    text[body_start..body_start + body_end]
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let l = l.trim_end_matches(';');
            let name = l
                .split_whitespace()
                .nth(1)
                .unwrap_or_else(|| panic!("bad member line {l:?}"));
            name.split('[').next().unwrap().to_string()
        })
        .collect()
}

/// The decode body emitted for one `case 0xNN: { ... }` block.
fn case_block(source: &str, byte: u8) -> String {
    let marker = format!("case 0x{byte:02X}: {{");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("no {marker} in:\n{source}"));
    let rest = &source[start + marker.len()..];
    let end = rest
        .find("break;")
        .unwrap_or_else(|| panic!("no break in case 0x{byte:02X}:\n{source}"));
    rest[..end].to_string()
}

fn plan(schema_yaml: &str, header_path: &str) -> CheckedPageOutput {
    let schema = parse_page_schema(schema_yaml).expect("parse");
    plan_checked_page(&schema, header_path).expect("checked page plan")
}

fn refusal(schema_yaml: &str, context: &[&str]) {
    let schema = parse_page_schema(schema_yaml).expect("parse");
    let err = plan_checked_page(&schema, "pages/x_revision.h")
        .expect_err("unrepresentable mapping must be refused");
    for token in context {
        assert!(err.contains(token), "missing {token:?}: {err}");
    }
}

fn page_yaml(id: &str, fields: &str) -> String {
    let address = if id == "CmisPage11hLaneStatus" {
        "11h"
    } else {
        "01h"
    };
    format!(
        "schema: openxcvr.page.v2\nkind: page\nid: {id}\ntitle: T\npage_address: \"{address}\"\nsize_bytes: 128\nfields:\n{fields}"
    )
}

/// The CLI emits the four checked files for `--spec cmis` (settled curated
/// pair) and the derived family-owned versioned sets for the SFF families;
/// pages the shared C extraction cannot represent are skipped with a
/// stderr notice (SFF-8636 page 22h and SFF-8472 A2h page 03h: 16-bit
/// enum_ref format_id) and emit nothing.
#[test]
fn checked_pages_emit_only_for_target_cmis_pages() {
    let tmp = tempfile::tempdir().unwrap();

    let cmis_out = tmp.path().join("cmis");
    let output = run_cli(&schemas_root(), "cmis", &cmis_out);
    assert!(
        output.status.success(),
        "cmis generation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut revision_files: Vec<String> = collect_files(&cmis_out.join("pages"))
        .into_iter()
        .filter(|n| n.contains("_revision"))
        .collect();
    revision_files.sort();
    assert_eq!(
        revision_files,
        vec![
            "cmis_page01h_advertising_revision.c",
            "cmis_page01h_advertising_revision.h",
            "cmis_page11h_lane_status_revision.c",
            "cmis_page11h_lane_status_revision.h",
        ],
        "unexpected checked page file set"
    );
    let umbrella =
        std::fs::read_to_string(cmis_out.join("openxcvr_generated.h")).expect("umbrella");
    for stem in ["cmis_page01h_advertising", "cmis_page11h_lane_status"] {
        assert!(
            umbrella.contains(&format!("#include \"pages/{stem}_revision.h\"")),
            "umbrella missing {stem}: {umbrella}"
        );
    }

    // SFF-8636: every family-owned versioned page except page 22h.
    let out = tmp.path().join("sff-8636");
    let output = run_cli(&schemas_root(), "sff-8636", &out);
    assert!(
        output.status.success(),
        "sff-8636 generation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut revision_files: Vec<String> = collect_files(&out.join("pages"))
        .into_iter()
        .filter(|n| n.contains("_revision"))
        .collect();
    revision_files.sort();
    assert_eq!(
        revision_files,
        vec![
            "sff8636_lower_memory_revision.c",
            "sff8636_lower_memory_revision.h",
            "sff8636_page00h_upper_administrative_revision.c",
            "sff8636_page00h_upper_administrative_revision.h",
            "sff8636_page03h_thresholds_and_controls_revision.c",
            "sff8636_page03h_thresholds_and_controls_revision.h",
            "sff8636_page20h_pam4_monitoring1_revision.c",
            "sff8636_page20h_pam4_monitoring1_revision.h",
            "sff8636_page21h_pam4_monitoring2_revision.c",
            "sff8636_page21h_pam4_monitoring2_revision.h",
        ],
        "unexpected sff-8636 checked page file set"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Sff8636Page22hHighAccuracyTiming"),
        "the page 22h skip must be reported, not silent"
    );

    // SFF-8472: every family-owned versioned page except A2h page 03h.
    let out = tmp.path().join("sff-8472");
    let output = run_cli(&schemas_root(), "sff-8472", &out);
    assert!(
        output.status.success(),
        "sff-8472 generation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut revision_files: Vec<String> = collect_files(&out.join("pages"))
        .into_iter()
        .filter(|n| n.contains("_revision"))
        .collect();
    revision_files.sort();
    assert_eq!(
        revision_files,
        vec![
            "page_a0h_base_memory_revision.c",
            "page_a0h_base_memory_revision.h",
            "page_a2h_02h_tunability_rdt_rpm_revision.c",
            "page_a2h_02h_tunability_rdt_rpm_revision.h",
            "page_a2h_diagnostics_base_revision.c",
            "page_a2h_diagnostics_base_revision.h",
        ],
        "unexpected sff-8472 checked page file set"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("page_a2h_03h_high_accuracy_timing"),
        "the A2h page 03h skip must be reported, not silent"
    );
}

/// Real resolver + emitter coverage of the transform vocabulary: since/until
/// windows, compatible changed_in width and type overrides, and rename/alias
/// entries that must never leak semantic source names into public members.
#[test]
fn checked_pages_resolve_transforms_and_refuse_unrepresentable_mappings() {
    let fields = r#"  - { name: win, offset: 128, type: u8, since: "4.0", until: "5.2" }
  - name: narrowed
    offset: 129
    type: u4
    changed_in:
      - { rev: "5.0", bit_width: 2 }
  - name: widened8
    offset: 130
    type: u4
    changed_in:
      - { rev: "5.1", type: u8 }
  - name: kept_name
    offset: 131
    type: u8
    renamed_from: { rev: "5.0", old_name: old_name }
  - name: aliased_member
    offset: 132
    type: u8
    aliased_to:
      - { rev: "5.2", new_name: alias_name }
"#;
    let yaml = page_yaml("CmisPage01hAdvertising", fields);
    let plan = plan(&yaml, "pages/cmis_page01h_advertising_revision.h");

    // One presence flag per original member name; renamed/aliased semantic
    // source names never appear as public members.
    for member in ["win", "narrowed", "widened8", "kept_name", "aliased_member"] {
        assert!(
            plan.header.contains(&format!("bool {member};")),
            "missing presence member {member}:\n{}",
            plan.header
        );
    }
    assert!(!plan.header.contains("old_name"), "{}", plan.header);
    assert!(!plan.header.contains("alias_name"), "{}", plan.header);

    // since 4.0 / until 5.2 (exclusive): applicable at 0x40, 0x50, 0x51 only.
    let b30 = case_block(&plan.source, 0x30);
    let b40 = case_block(&plan.source, 0x40);
    let b50 = case_block(&plan.source, 0x50);
    let b51 = case_block(&plan.source, 0x51);
    let b52 = case_block(&plan.source, 0x52);
    let b53 = case_block(&plan.source, 0x53);
    for block in [&b30, &b52, &b53] {
        assert!(!block.contains("decoded.present.win"), "{block}");
    }
    for block in [&b40, &b50, &b51] {
        assert!(block.contains("decoded.present.win = true;"), "{block}");
        assert!(block.contains("decoded.values.win = buf[0];"), "{block}");
    }

    // Compatible changed_in bit_width: extraction uses the resolved width.
    for block in [&b30, &b40] {
        assert!(
            block.contains("decoded.values.narrowed = (uint8_t)((buf[1] >> 0) & 0xFu);"),
            "{block}"
        );
    }
    for block in [&b50, &b51, &b52, &b53] {
        assert!(
            block.contains("decoded.values.narrowed = (uint8_t)((buf[1] >> 0) & 0x3u);"),
            "{block}"
        );
    }

    // Compatible changed_in type within the same C member representation:
    // u4 nibble extraction before 5.1, whole-byte u8 extraction from 5.1.
    for block in [&b30, &b40, &b50] {
        assert!(
            block.contains("decoded.values.widened8 = (uint8_t)((buf[2] >> 0) & 0xFu);"),
            "{block}"
        );
    }
    for block in [&b51, &b52, &b53] {
        assert!(
            block.contains("decoded.values.widened8 = buf[2];"),
            "{block}"
        );
    }

    // Rename/alias keep the stable member name and unchanged extraction.
    for block in [&b30, &b40, &b50, &b51, &b52, &b53] {
        assert!(
            block.contains("decoded.values.kept_name = buf[3];"),
            "{block}"
        );
        assert!(
            block.contains("decoded.values.aliased_member = buf[4];"),
            "{block}"
        );
    }

    // ── Refusals: every message names page, original field and revision ──
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: bad_type
    offset: 128
    type: u8
    changed_in:
      - { rev: "5.0", type: u16be }
"#,
        ),
        &["CmisPage01hAdvertising", "bad_type", "5.0", "uint16_t"],
    );
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: bad_width
    offset: 128
    type: u4
    changed_in:
      - { rev: "5.1", bit_width: 12 }
"#,
        ),
        &["CmisPage01hAdvertising", "bad_width", "5.1", "12"],
    );
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: bad_class
    offset: 128
    type: u8
    changed_in:
      - { rev: "5.2", type: bitmap }
"#,
        ),
        &["CmisPage01hAdvertising", "bad_class", "5.2"],
    );
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: bad_array
    offset_expr: "128 + i"
    instances: { index: i, start: 0, end: 3 }
    type: u8
    changed_in:
      - { rev: "5.0", type: u16be }
"#,
        ),
        &["CmisPage01hAdvertising", "bad_array", "5.0", "uint16_t"],
    );
    // Resolved extent past the page window: a 16-bit member whose second
    // byte lies outside the 128-byte page must not be silently narrowed.
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            "  - { name: late, offset: 255, type: u16be, size_bytes: 2 }\n",
        ),
        &["CmisPage01hAdvertising", "late", "extent"],
    );
    // Scalar/string (array) shape and signedness are not interchangeable.
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: text
    offset: 128
    size_bytes: 1
    type: ascii
    changed_in:
      - { rev: "5.0", type: u8 }
"#,
        ),
        &["CmisPage01hAdvertising", "text", "5.0", "incompatible"],
    );
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: signedness
    offset: 128
    type: u8
    changed_in:
      - { rev: "4.0", type: i8 }
"#,
        ),
        &["CmisPage01hAdvertising", "signedness", "4.0", "int8_t"],
    );
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: hidden
    offset: 128
    type: u8
    changed_in:
      - { rev: "5.3", emit_value: false }
"#,
        ),
        &["CmisPage01hAdvertising", "hidden", "5.3", "non-emittable"],
    );
    // Previously unsupported raw field becomes emittable but has no stable
    // destination. This must not create a new member in the old raw struct.
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: no_destination
    offset: 128
    type: bitmap
    changed_in:
      - { rev: "4.0", type: u8 }
"#,
        ),
        &[
            "CmisPage01hAdvertising",
            "no_destination",
            "4.0",
            "no raw destination",
        ],
    );
    // A metadata-only original has no raw destination even if changed_in
    // tries to enable a decoded value in one revision.
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - name: metadata_only
    offset: 128
    type: u8
    emit_value: false
    changed_in:
      - { rev: "5.1", emit_value: true }
"#,
        ),
        &[
            "CmisPage01hAdvertising",
            "metadata_only",
            "5.1",
            "no raw destination",
        ],
    );
    // Invalid masks must return a contextual refusal, not panic on a shift.
    for width in [12, 32, 64] {
        refusal(
            &page_yaml(
                "CmisPage01hAdvertising",
                &format!("  - {{ name: huge_width, offset: 128, type: u4, bit_width: {width} }}\n"),
            ),
            &["CmisPage01hAdvertising", "huge_width", "3.0", "width"],
        );
    }
    // Stable-member collision: two original fields with the same name.
    refusal(
        &page_yaml(
            "CmisPage01hAdvertising",
            "  - { name: dup, offset: 128, type: u8 }\n  - { name: dup, offset: 129, type: u8 }\n",
        ),
        &["CmisPage01hAdvertising", "dup"],
    );
}

/// Empty applicability is not an error: the member keeps its presence flag
/// and is simply never set; a page with no emitted members still emits a
/// C99-valid `_unused` presence member.
#[test]
fn checked_pages_emit_empty_applicability() {
    let empty = plan(
        &page_yaml(
            "CmisPage01hAdvertising",
            r#"  - { name: never, offset: 128, type: u8, since: "5.3", until: "4.0" }
"#,
        ),
        "pages/cmis_page01h_advertising_revision.h",
    );
    assert!(
        empty.header.contains("bool never;"),
        "empty-window member must keep its presence flag:\n{}",
        empty.header
    );
    for byte in [0x30, 0x40, 0x50, 0x51, 0x52, 0x53] {
        let block = case_block(&empty.source, byte);
        assert!(
            !block.contains("decoded.present.never"),
            "{byte:02X}: {block}"
        );
    }
    assert_eq!(case_block(&empty.source, 0x30).trim(), "", "3.0 case");

    let no_members = plan(
        &page_yaml(
            "CmisPage01hAdvertising",
            "  - { name: blob, offset: 128, type: bitmap, size_bytes: 4 }\n",
        ),
        "pages/cmis_page01h_advertising_revision.h",
    );
    let members = struct_members(
        &no_members.header,
        "openxcvr_cmis_page01h_advertising_presence",
    );
    assert_eq!(members, vec!["_unused"], "{}", no_members.header);
}

/// Compile/run synthetic output too: string extraction shares the existing
/// helper, and both empty cases (no members / no applicable members at a
/// revision) must be valid C99 under strict warnings.
#[test]
fn checked_synthetic_outputs_compile_and_run() {
    for (fields, checks) in [
        (
            "  - { name: ignored, offset: 128, type: bitmap }\n",
            "assert(!out.present._unused);",
        ),
        (
            "  - { name: future, offset: 128, type: u8, since: '5.3' }\n",
            "assert(!out.present.future); assert(out.values.future == 0);",
        ),
        (
            "  - { name: text, offset: 128, type: ascii, size_bytes: 4 }\n",
            "assert(out.present.text); assert(strcmp(out.values.text, \"AB\") == 0);",
        ),
    ] {
        let schema = parse_page_schema(&page_yaml("CmisPage01hAdvertising", fields)).unwrap();
        let planned = plan_checked_page(&schema, "pages/checked.h").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("pages")).unwrap();
        std::fs::write(
            tmp.path().join("pages/cmis_page01h_advertising.h"),
            CEmitter.emit_page_header(&schema),
        )
        .unwrap();
        std::fs::write(tmp.path().join("pages/checked.h"), planned.header).unwrap();
        std::fs::write(tmp.path().join("checked.c"), planned.source).unwrap();
        std::fs::write(tmp.path().join("test.c"), format!(
            "#include \"pages/checked.h\"\n#include <assert.h>\n#include <string.h>\nint main(void) {{ uint8_t buf[128] = {{'A','B',' ',' '}}; openxcvr_cmis_page01h_advertising_checked out; assert(openxcvr_decode_cmis_page01h_advertising_for_revision(0x30, buf, 128, &out) == OPENXCVR_OK); {checks} return 0; }}\n"
        )).unwrap();
        let compile = std::process::Command::new("cc")
            .args(["-std=c99", "-Wall", "-Wextra", "-Werror", "-I"])
            .arg(tmp.path())
            .arg("-I")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bindings/c/include"))
            .arg(tmp.path().join("checked.c"))
            .arg(tmp.path().join("test.c"))
            .arg("-o")
            .arg(tmp.path().join("test"))
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = std::process::Command::new(tmp.path().join("test"))
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
}

/// A representability refusal in the CLI must fail the run nonzero and
/// leave no checked output behind (preflight precedes every write).
#[test]
fn checked_page_cli_propagates_refusal_without_checked_outputs() {
    let tmp = tempfile::tempdir().unwrap();
    let spec_dir = tmp.path().join("schemas").join("cmis");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("_index.yaml"),
        "kind: memory_map\nincludes: [enum.yaml, valid.yaml, page.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        spec_dir.join("enum.yaml"),
        "schema: openxcvr.enum.v2\nkind: enum\nid: DummyEnum\ntitle: Dummy\nentries:\n  - { value: 0, name: zero }\n",
    ).unwrap();
    // A successful checked plan is visited before the refused plan: no
    // checked output from EITHER page may be written by this invocation.
    std::fs::write(
        spec_dir.join("valid.yaml"),
        page_yaml(
            "CmisPage11hLaneStatus",
            "  - { name: valid, offset: 128, type: u8 }\n",
        ),
    )
    .unwrap();
    // Validation-legal schema (the u16be override stays inside the page at
    // every applicable revision) whose checked member cannot stay a uint8_t.
    std::fs::write(
        spec_dir.join("page.yaml"),
        r#"schema: openxcvr.page.v2
kind: page
id: CmisPage01hAdvertising
title: T
page_address: "01h"
size_bytes: 128
fields:
  - { name: plain, offset: 128, type: u8 }
  - name: widened
    offset: 254
    type: u8
    changed_in:
      - { rev: "5.0", type: u16be }
"#,
    )
    .unwrap();
    let out = tmp.path().join("out");
    let output = run_cli(tmp.path().join("schemas").as_path(), "cmis", &out);
    assert!(
        !output.status.success(),
        "representability refusal must fail the CLI"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for token in ["CmisPage01hAdvertising", "widened", "5.0"] {
        assert!(stderr.contains(token), "missing {token:?}: {stderr}");
    }
    let pages = out.join("pages");
    if pages.exists() {
        let leaked: Vec<String> = collect_files(&pages)
            .into_iter()
            .filter(|n| n.contains("_revision"))
            .collect();
        assert!(
            leaked.is_empty(),
            "checked files written on refusal: {leaked:?}"
        );
    }
}

/// The spec gate: the curated CMIS checked list gates only the CMIS family
/// (the c-cmis-1.4 supplement and no-spec union runs do not); the SFF
/// families use the derived selection instead.
#[test]
fn checked_page_spec_gate_is_cmis_only() {
    assert!(is_cmis_generation(Some("cmis")));
    for spec in ["sff-8636", "sff-8472", "c-cmis-1.4", ""] {
        assert!(!is_cmis_generation(Some(spec)), "{spec}");
    }
    assert!(!is_cmis_generation(None));
    assert_eq!(
        CHECKED_PAGE_IDS,
        &["CmisPage01hAdvertising", "CmisPage11hLaneStatus"]
    );
}

fn load_real(name: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    load_page_schema(&schemas_root().join("cmis").join(name)).expect("real page schema")
}

/// Exhaustive, schema-derived coverage: every raw C member keeps a presence
/// flag, and each revision case block sets presence exactly for the
/// applicable members. Landmark windows are pinned independently of the
/// resolver, per the checked-pages plan.
#[test]
fn real_checked_pages_cover_every_raw_member_and_revision() {
    let cases = [
        (
            "page-01h-advertising.yaml",
            "CmisPage01hAdvertising",
            "cmis_page01h_advertising",
            // (member, first rev, [revs where absent])
            vec![
                (
                    "module_inactive_firmware_major_revision",
                    Rev::V4_0,
                    vec![Rev::V3_0],
                ),
                ("cdb_instances_supported", Rev::V4_0, vec![Rev::V3_0]),
                ("e_pps_supported", Rev::V5_0, vec![Rev::V3_0, Rev::V4_0]),
                (
                    "timing_page15h_supported",
                    Rev::V5_0,
                    vec![Rev::V3_0, Rev::V4_0],
                ),
                (
                    "network_path_pages_supported",
                    Rev::V5_1,
                    vec![Rev::V3_0, Rev::V4_0, Rev::V5_0],
                ),
                (
                    "bank_broadcast_supported",
                    Rev::V5_2,
                    vec![Rev::V3_0, Rev::V4_0, Rev::V5_0, Rev::V5_1],
                ),
                (
                    "password_entry_supported",
                    Rev::V5_3,
                    vec![Rev::V3_0, Rev::V4_0, Rev::V5_0, Rev::V5_1, Rev::V5_2],
                ),
                (
                    "full_page_read_supported",
                    Rev::V5_3,
                    vec![Rev::V3_0, Rev::V4_0, Rev::V5_0, Rev::V5_1, Rev::V5_2],
                ),
            ],
        ),
        (
            "page-11h-lane-status.yaml",
            "CmisPage11hLaneStatus",
            "cmis_page11h_lane_status",
            vec![
                ("dp_state_host_lane", Rev::V3_0, vec![]),
                ("output_status_rx", Rev::V5_0, vec![Rev::V3_0, Rev::V4_0]),
                ("output_status_tx", Rev::V5_0, vec![Rev::V3_0, Rev::V4_0]),
                (
                    "output_status_changed_flag_rx",
                    Rev::V5_0,
                    vec![Rev::V3_0, Rev::V4_0],
                ),
                (
                    "dp_init_pending_lane",
                    Rev::V5_0,
                    vec![Rev::V3_0, Rev::V4_0],
                ),
            ],
        ),
    ];

    for (file, id, suffix, landmarks) in cases {
        let schema = load_real(file);
        assert_eq!(schema.id, id);

        let emitter = CEmitter;
        let raw_header = emitter.emit_page_header(&schema);
        let raw_members = struct_members(&raw_header, &format!("openxcvr_{suffix}"));

        let header_path = format!("pages/{suffix}_revision.h");
        let plan = plan_checked_page(&schema, &header_path).expect("real checked plan");

        let presence = struct_members(&plan.header, &format!("openxcvr_{suffix}_presence"));
        assert_eq!(
            presence, raw_members,
            "{id}: presence flags must mirror the raw value members"
        );

        // Derived expectation: presence lines per case block == applicable
        // members at that revision, computed from the schema's own
        // since/until metadata through applicable_revs.
        for rev in Rev::CMIS_ALL {
            let byte = rev.major_byte_cmis();
            let block = case_block(&plan.source, byte);
            let mut expected = BTreeSet::new();
            for field in schema.value_fields() {
                if !raw_members.contains(&field.name) {
                    continue;
                }
                if applicable_revs(field, Rev::CMIS_ALL).contains(rev) {
                    expected.insert(field.name.clone());
                }
            }
            let set_lines: Vec<&str> = block
                .lines()
                .map(str::trim)
                .filter(|l| l.starts_with("decoded.present."))
                .collect();
            let set: BTreeSet<&str> = set_lines
                .iter()
                .map(|l| {
                    l.trim_start_matches("decoded.present.")
                        .trim_end_matches(" = true;")
                })
                .collect();
            let expected_ref: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
            assert_eq!(
                set,
                expected_ref,
                "{id} rev {}: presence set drift",
                rev.as_str()
            );
            for member in &expected {
                assert!(
                    block.contains(&format!("decoded.values.{member}")),
                    "{id} rev {}: no extraction for {member}:\n{block}",
                    rev.as_str()
                );
            }
        }

        // Independent landmark pins (hardcoded windows, not resolver output).
        for (member, first, absent) in &landmarks {
            for rev in Rev::CMIS_ALL {
                let byte = rev.major_byte_cmis();
                let block = case_block(&plan.source, byte);
                let pinned_absent = absent.contains(rev);
                let pinned_present = *rev >= *first && !pinned_absent;
                let line = format!("decoded.present.{member} = true;");
                assert_eq!(
                    block.contains(&line),
                    pinned_present,
                    "{id} {member} at rev {}: presence expected {}",
                    rev.as_str(),
                    pinned_present
                );
            }
        }

        // The six catalog bytes are derived from Rev::CMIS_ALL/major_byte_cmis.
        for rev in Rev::CMIS_ALL {
            let byte = rev.major_byte_cmis();
            assert!(
                plan.source.contains(&format!("case 0x{byte:02X}:")),
                "{id}: missing case 0x{byte:02X}"
            );
        }
        assert_eq!(
            plan.source.matches("case 0x").count(),
            7 + 7, // admission switch + decode switch
            "{id}: unexpected case count"
        );
        // Never the flat decoder, and errors precede any decode: the local
        // result is now declared kernel-style before the validation
        // statements, so the invariant is that no `decoded.values.`
        // assignment (the decode itself) precedes the error returns.
        assert!(!plan
            .source
            .contains("openxcvr_decode_cmis_page01h_advertising("));
        assert!(!plan
            .source
            .contains("openxcvr_decode_cmis_page11h_lane_status("));
        assert!(
            plan.source
                .find("OPENXCVR_ERR_UNSUPPORTED_REVISION")
                .unwrap()
                < plan.source.find("decoded.values.").unwrap()
        );
    }
}

/// Kernel-style C recipes reject declarations after statements (the supported
/// LTS kernels build with -Wdeclaration-after-statement; see the CMIS plugin
/// kernel-build recipe). The generated decode function must declare `decoded`
/// before its first statement, and the header must name the exact revision
/// catalog so generated docs cannot drift from the README contract.
#[test]
fn checked_page_sources_keep_declarations_before_statements() {
    for (file, suffix) in [
        ("page-01h-advertising.yaml", "cmis_page01h_advertising"),
        ("page-11h-lane-status.yaml", "cmis_page11h_lane_status"),
    ] {
        let schema = load_real(file);
        let plan = plan_checked_page(&schema, &format!("pages/{suffix}_revision.h"))
            .expect("real checked plan");
        let decl = format!("openxcvr_{suffix}_checked decoded = {{0}};");
        let decl_at = plan
            .source
            .find(&decl)
            .unwrap_or_else(|| panic!("{suffix}: missing decoded declaration"));
        let first_if = plan
            .source
            .find("if (")
            .unwrap_or_else(|| panic!("{suffix}: no statements"));
        assert!(
            decl_at < first_if,
            "{suffix}: declaration after statement:\n{}",
            plan.source
        );
        // The revision catalog is derived from the family domain, so the
        // comment cannot drift from the admission switch (the old hand-written
        // comment omitted 0x54 while the switch admitted it).
        for (byte, label) in [
            ("0x30", "3.0"),
            ("0x40", "4.0"),
            ("0x50", "5.0"),
            ("0x51", "5.1"),
            ("0x52", "5.2"),
            ("0x53", "5.3"),
            ("0x54", "5.4"),
        ] {
            assert!(
                plan.header.contains(&format!("{byte} (CMIS {label})")),
                "{suffix}: header must name {byte} (CMIS {label}):\n{}",
                plan.header
            );
            assert!(
                plan.source.contains(&format!("case {byte}:")),
                "{suffix}: source must admit {byte}"
            );
        }
    }
}
