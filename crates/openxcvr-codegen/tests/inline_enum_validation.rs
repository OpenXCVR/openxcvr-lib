//! Inline enum validation and per-revision code set regression tests.
//!
//! Enforces:
//! 1. Per-revision far_end_configuration and media_interface_technology code sets
//!    as printed by each CMIS revision (CMIS 3.0, 4.0, 5.0, 5.1, 5.2, 5.3).
//! 2. Condition C-d inline-block checks (for the blocks S adds plus mci_max_speed):
//!    - values fit declared bit_width
//!    - totality and disjointness over 0..2^width-1
//!    - unique names within each block
//!    - base block == highest changed_in block
//! 3. The ten real schema defect blocks of the inline-totality qualification
//!    (`inline-totality/report.md` §4, `classification.json`): each cited
//!    primary table prints a reserved span the block omitted. Every block must
//!    record exactly the code set its table prints — width fit, disjointness,
//!    unique names, totality over the printed code set, the printed reserved
//!    span present as a range entry, and no code the table does not print.
//! 4. module_fault_cause per-revision provenance: the changed_in entries at
//!    5.1 and 5.2 cite their own table and PDF page (advisory A1 of the
//!    enum-revision-code-sets review), and the decoded type per revision is
//!    unchanged (CmisModuleFaultCausePre5p3 at 5.0/5.1/5.2,
//!    CmisModuleFaultCause at 5.3).

use std::collections::BTreeSet;
use std::path::Path;

use openxcvr_codegen::ir::pages::{FieldType, PageField, PageSchema};
use openxcvr_codegen::ir::version::{resolve_field_for_rev, Rev};
use openxcvr_codegen::parser::pages::load_page_schema;
use openxcvr_codegen::validator::validate_inline_enums;

fn parse_values(v: &serde_norway::Value) -> Vec<u64> {
    if let Some(n) = v.as_u64() {
        return vec![n];
    }
    if let Some(i) = v.as_i64() {
        if i >= 0 {
            return vec![i as u64];
        }
        panic!("negative value in enum_inline: {i}");
    }
    if let Some(s) = v.as_str() {
        if s.contains("..") {
            let parts: Vec<&str> = s.splitn(2, "..").collect();
            let parse_num = |p: &str| -> u64 {
                let p = p.trim();
                if let Some(hex) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16).unwrap_or_else(|e| panic!("bad hex {p}: {e}"))
                } else {
                    p.parse::<u64>()
                        .unwrap_or_else(|e| panic!("bad int {p}: {e}"))
                }
            };
            let start = parse_num(parts[0]);
            let end = parse_num(parts[1]);
            assert!(start <= end, "inverted range: {s}");
            return (start..=end).collect();
        }
        let p = s.trim();
        let val = if let Some(hex) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).unwrap_or_else(|e| panic!("bad hex {p}: {e}"))
        } else {
            p.parse::<u64>()
                .unwrap_or_else(|e| panic!("bad int {p}: {e}"))
        };
        return vec![val];
    }
    panic!("unrecognized value format: {v:?}");
}

fn get_bit_width(
    field: &PageField,
    override_width: Option<u32>,
    override_ty: Option<&FieldType>,
) -> u8 {
    if let Some(w) = override_width {
        return w as u8;
    }
    if let Some(w) = field.bit_width {
        return w as u8;
    }
    let ty = override_ty.unwrap_or(&field.ty);
    match ty {
        FieldType::Bool => 1,
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        FieldType::U8 => 8,
        _ => panic!(
            "cannot infer bit width for field '{}' with type '{:?}'",
            field.name, ty
        ),
    }
}

fn verify_inline_block(
    field_name: &str,
    block_name: &str,
    entries: &[serde_norway::Value],
    width: u8,
) {
    let max_val = (1u64 << width) - 1;
    let mut names = BTreeSet::new();
    let mut covered = BTreeSet::new();

    for entry in entries {
        let map = entry
            .as_mapping()
            .unwrap_or_else(|| panic!("{field_name} {block_name}: entry not a mapping: {entry:?}"));
        let name_val = map
            .get("name")
            .unwrap_or_else(|| panic!("{field_name} {block_name}: entry missing name: {entry:?}"));
        let name = name_val.as_str().expect("name must be string");

        assert!(
            names.insert(name.to_string()),
            "C-d violation in {field_name} ({block_name}): duplicate entry name '{name}'"
        );

        let val_yaml = map
            .get("value")
            .unwrap_or_else(|| panic!("{field_name} {block_name}: entry missing value: {entry:?}"));
        let values = parse_values(val_yaml);

        for val in values {
            assert!(
                val <= max_val,
                "C-d violation in {field_name} ({block_name}): value {val} exceeds width {width} (max {max_val})"
            );
            assert!(
                covered.insert(val),
                "C-d violation in {field_name} ({block_name}): overlapping value {val}"
            );
        }
    }

    // Totality check
    for expected in 0..=max_val {
        assert!(
            covered.contains(&expected),
            "C-d violation in {field_name} ({block_name}): missing value {expected} over range 0..={max_val}"
        );
    }
}

#[test]
fn inline_blocks_cd_check_for_s_and_mci_max_speed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis");

    let lower_mem =
        load_page_schema(&root.join("lower-memory.yaml")).expect("lower-memory schema must load");
    let page00 = load_page_schema(&root.join("page-00h-administrative.yaml"))
        .expect("page00 schema must load");

    let targets = [
        (&lower_mem, "mci_max_speed"),
        (&page00, "far_end_configuration"),
        (&page00, "media_interface_technology"),
    ];

    for (page, field_name) in targets {
        let field = page
            .fields
            .iter()
            .find(|f| f.name == field_name)
            .unwrap_or_else(|| panic!("field {field_name} not found in page {}", page.id));

        let base_entries = field
            .enum_inline
            .as_ref()
            .and_then(|v| v.as_sequence())
            .unwrap_or_else(|| panic!("field {field_name} must have base enum_inline"));

        let base_width = get_bit_width(field, None, None);
        verify_inline_block(field_name, "base", base_entries, base_width);

        let changed_in = field.changed_in.as_ref().unwrap_or_else(|| {
            panic!("field {field_name} must have changed_in entries with enum_inline")
        });

        let inline_changes: Vec<_> = changed_in
            .iter()
            .filter(|c| c.enum_inline.is_some())
            .collect();

        assert!(
            !inline_changes.is_empty(),
            "field {field_name} must have changed_in entries defining enum_inline"
        );

        let mut highest_rev: Option<Rev> = None;
        let mut highest_block: Option<&Vec<serde_norway::Value>> = None;

        for change in &inline_changes {
            let rev = Rev::parse(&change.rev).expect("rev must be valid CMIS rev");
            let entries = change
                .enum_inline
                .as_ref()
                .and_then(|v| v.as_sequence())
                .unwrap();
            let width = get_bit_width(field, change.bit_width, change.ty.as_ref());
            verify_inline_block(
                field_name,
                &format!("changed_in {}", change.rev),
                entries,
                width,
            );

            if highest_rev.is_none() || Some(rev) > highest_rev {
                highest_rev = Some(rev);
                highest_block = Some(entries);
            }
        }

        // Base block == highest changed_in block
        let highest_entries = highest_block.unwrap();
        assert_eq!(
            base_entries,
            highest_entries,
            "C-d violation in {field_name}: base enum_inline must equal highest changed_in (rev {:?}) block",
            highest_rev.unwrap()
        );
    }
}

/// One of the ten real schema defect blocks from the inline-totality
/// qualification: the cited primary table prints a reserved span the block
/// omitted. The block must record exactly the code set that table prints.
struct ReservedSpanCase {
    /// Family directory under schemas/ (e.g. "c-cmis-1.4").
    family_dir: &'static str,
    /// Page schema file inside the family directory.
    page_file: &'static str,
    field: &'static str,
    /// Effective width of the field, as the C-d check computes it.
    width: u8,
    /// Inclusive bounds of the code set the cited table prints. Coverage
    /// must equal this set exactly: every printed code named once, and no
    /// code the table does not print.
    printed_lo: u64,
    printed_hi: u64,
    /// The printed reserved span the block omitted: (name, lo, hi).
    reserved: (&'static str, u64, u64),
    /// Primary-source anchor, quoted in failure messages.
    anchor: &'static str,
}

const RESERVED_SPAN_CASES: &[ReservedSpanCase] = &[
    // OIF-C-CMIS-01.4 Table 11, PDF p20, byte 136 bits 3-0:
    // "0011-1011 - reserved" (custom 1100-1111 already recorded).
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-31h-media-lane-provisioning.yaml",
        field: "tx_stat_mnt_insertion",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_11", 3, 11),
        anchor: "OIF-C-CMIS-01.4 Table 11, PDF p20: 0011-1011 - reserved",
    },
    // OIF-C-CMIS-01.4 Table 17, PDF p29, byte 137 bits 4-7.
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-38h-data-path-host-if-config.yaml",
        field: "rx_cons_act",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_11", 3, 11),
        anchor: "OIF-C-CMIS-01.4 Table 17, PDF p29: 0011-1011 - reserved",
    },
    // OIF-C-CMIS-01.4 Table 17, PDF p29, byte 137 bits 3-0.
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-38h-data-path-host-if-config.yaml",
        field: "tx_cons_act",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_11", 3, 11),
        anchor: "OIF-C-CMIS-01.4 Table 17, PDF p29: 0011-1011 - reserved",
    },
    // OIF-C-CMIS-01.4 Table 17, PDF p30, byte 145 bits 4-7.
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-38h-data-path-host-if-config.yaml",
        field: "rx_rplc_sig_insertion",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_11", 3, 11),
        anchor: "OIF-C-CMIS-01.4 Table 17, PDF p30: 0011-1011 - reserved",
    },
    // OIF-C-CMIS-01.4 Table 17, PDF p30, byte 145 bits 3-0.
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-38h-data-path-host-if-config.yaml",
        field: "tx_rplc_sig_insertion",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_11", 3, 11),
        anchor: "OIF-C-CMIS-01.4 Table 17, PDF p30: 0011-1011 - reserved",
    },
    // OIF-C-CMIS-01.4 Table 17, PDF p30, byte 146 bits 3-0:
    // "0010-1011 - reserved".
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-38h-data-path-host-if-config.yaml",
        field: "tx_cstat_lck_insertion",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_2_11", 2, 11),
        anchor: "OIF-C-CMIS-01.4 Table 17, PDF p30: 0010-1011 - reserved",
    },
    // OIF-C-CMIS-01.4 Table 22, PDF p36, byte 129 bits 2-0:
    // "110b~111b: Reserved".
    ReservedSpanCase {
        family_dir: "c-cmis-1.4",
        page_file: "page-41h-rx-signal-power-advertisement.yaml",
        field: "rx_los_type",
        width: 3,
        printed_lo: 0,
        printed_hi: 7,
        reserved: ("reserved_6_7", 6, 7),
        anchor: "OIF-C-CMIS-01.4 Table 22, PDF p36: 110b~111b: Reserved",
    },
    // OIF-ELSFP-CMIS-01.0 Table 6, PDF p20, register 185 SaveRestoreConfirm:
    // "05h-07h: reserved for future status codes". The table prints codes
    // 01h-08h only, so the printed code set is 1..=8; codes 00h and
    // 09h-FFh are not printed and must not be invented.
    ReservedSpanCase {
        family_dir: "elsfp-1.0",
        page_file: "page-1ah-lane-mgmt.yaml",
        field: "save_restore_status",
        width: 8,
        printed_lo: 1,
        printed_hi: 8,
        reserved: ("reserved_5_7", 5, 7),
        anchor: "OIF-ELSFP-CMIS-01.0 Table 6, PDF p20: 05h-07h: reserved for future status codes",
    },
    // OIF-ELSFP-CMIS-01.0 Table 2, PDF p13:
    // "3-8 Reserved for future standardization" (9-15 vendor specific).
    ReservedSpanCase {
        family_dir: "elsfp-1.0",
        page_file: "page-1ah-lane-mgmt.yaml",
        field: "fault_code",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_8", 3, 8),
        anchor: "OIF-ELSFP-CMIS-01.0 Table 2, PDF p13: 3-8 Reserved for future standardization",
    },
    // OIF-ELSFP-CMIS-01.0 Table 3, PDF p14:
    // "3-8 Reserved for future standardization" (9-15 vendor specific).
    ReservedSpanCase {
        family_dir: "elsfp-1.0",
        page_file: "page-1ah-lane-mgmt.yaml",
        field: "warning_code",
        width: 4,
        printed_lo: 0,
        printed_hi: 15,
        reserved: ("reserved_3_8", 3, 8),
        anchor: "OIF-ELSFP-CMIS-01.0 Table 3, PDF p14: 3-8 Reserved for future standardization",
    },
];

#[test]
fn reserved_span_defect_blocks_record_printed_code_sets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");

    for case in RESERVED_SPAN_CASES {
        let page = load_page_schema(&root.join(case.family_dir).join(case.page_file))
            .unwrap_or_else(|e| panic!("{} must load: {e}", case.page_file));
        let field = page
            .fields
            .iter()
            .find(|f| f.name == case.field)
            .unwrap_or_else(|| panic!("{} not found in {}", case.field, case.page_file));
        let entries = field
            .enum_inline
            .as_ref()
            .and_then(|v| v.as_sequence())
            .unwrap_or_else(|| panic!("{} must have enum_inline", case.field));

        let max_val = (1u64 << case.width) - 1;
        let mut names = BTreeSet::new();
        let mut covered = BTreeSet::new();
        let mut found_reserved = false;

        for entry in entries {
            let map = entry
                .as_mapping()
                .unwrap_or_else(|| panic!("{}: entry not a mapping: {entry:?}", case.field));
            let name = map
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("{}: entry missing name: {entry:?}", case.field));
            assert!(
                names.insert(name.to_string()),
                "{}: duplicate entry name '{name}' ({})",
                case.field,
                case.anchor
            );

            let val_yaml = map
                .get("value")
                .unwrap_or_else(|| panic!("{}: entry missing value: {entry:?}", case.field));
            let values = parse_values(val_yaml);

            for val in &values {
                assert!(
                    val <= &max_val,
                    "{}: value {val} exceeds width {} (max {max_val}) ({})",
                    case.field,
                    case.width,
                    case.anchor
                );
                assert!(
                    covered.insert(*val),
                    "{}: overlapping value {val} ({})",
                    case.field,
                    case.anchor
                );
            }

            if name == case.reserved.0 {
                let expected: BTreeSet<u64> = (case.reserved.1..=case.reserved.2).collect();
                let actual: BTreeSet<u64> = values.iter().copied().collect();
                assert_eq!(
                    actual, expected,
                    "{}: entry '{}' must cover exactly {}..={} as printed by {}",
                    case.field, case.reserved.0, case.reserved.1, case.reserved.2, case.anchor
                );
                found_reserved = true;
            }
        }

        // Totality over the printed code set: every code the cited table
        // prints is covered exactly once, and nothing else is recorded.
        let printed: BTreeSet<u64> = (case.printed_lo..=case.printed_hi).collect();
        assert_eq!(
            covered, printed,
            "{}: enum_inline must cover exactly {}..={} as printed by {} ({}); covered {:?}",
            case.field, case.printed_lo, case.printed_hi, case.anchor, case.page_file, covered
        );

        // The printed reserved span the block omitted must be present.
        assert!(
            found_reserved,
            "{}: must record the printed reserved span {} ({}..={}) from {}",
            case.field, case.reserved.0, case.reserved.1, case.reserved.2, case.anchor
        );
    }
}

#[test]
fn module_fault_cause_changed_in_cites_each_revisions_own_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis");
    let lower_mem =
        load_page_schema(&root.join("lower-memory.yaml")).expect("lower-memory schema must load");
    let field = lower_mem
        .fields
        .iter()
        .find(|f| f.name == "module_fault_cause")
        .expect("module_fault_cause not found");

    let changed_in = field
        .changed_in
        .as_ref()
        .expect("module_fault_cause must have changed_in entries");

    // (rev, table, pdf_page, enum_ref) — all read directly from the PDFs:
    // 5.0 Table 8-15 p117, 5.1 Table 8-15 p133, 5.2 Table 8-16 p136,
    // 5.3 Table 8-16 (Lower Memory) p153.
    let expectations: [(&str, &str, i64, &str); 4] = [
        ("5.0", "Table 8-15", 117, "CmisModuleFaultCausePre5p3"),
        ("5.1", "Table 8-15", 133, "CmisModuleFaultCausePre5p3"),
        ("5.2", "Table 8-16", 136, "CmisModuleFaultCausePre5p3"),
        (
            "5.3",
            "Table 8-16 (Lower Memory)",
            153,
            "CmisModuleFaultCause",
        ),
    ];

    for (rev, table, pdf_page, enum_ref) in expectations {
        let entry = changed_in.iter().find(|c| c.rev == rev).unwrap_or_else(|| {
            panic!("module_fault_cause must carry an explicit changed_in entry at rev {rev}")
        });
        assert_eq!(
            entry.enum_ref.as_deref(),
            Some(enum_ref),
            "module_fault_cause at {rev} must select {enum_ref}"
        );
        let provenance = entry
            .provenance
            .as_ref()
            .and_then(|v| v.as_mapping())
            .unwrap_or_else(|| {
                panic!("module_fault_cause changed_in at {rev} must carry provenance")
            });
        assert_eq!(
            provenance.get("table").and_then(|t| t.as_str()),
            Some(table),
            "module_fault_cause at {rev} must cite its own table {table}"
        );
        assert_eq!(
            provenance.get("pdf_page").and_then(|p| p.as_i64()),
            Some(pdf_page),
            "module_fault_cause at {rev} must cite its own PDF page {pdf_page}"
        );
    }

    // The decoded type must not change: per-revision resolution still selects
    // the pre-5.3 code set at 5.0/5.1/5.2 and the full code set at 5.3.
    for rev in [Rev::V5_0, Rev::V5_1, Rev::V5_2] {
        let resolved = resolve_field_for_rev(field, rev);
        assert_eq!(
            resolved.enum_ref.as_deref(),
            Some("CmisModuleFaultCausePre5p3"),
            "module_fault_cause decoded type at {} must stay CmisModuleFaultCausePre5p3",
            rev.as_str()
        );
    }
    let resolved = resolve_field_for_rev(field, Rev::V5_3);
    assert_eq!(
        resolved.enum_ref.as_deref(),
        Some("CmisModuleFaultCause"),
        "module_fault_cause decoded type at 5.3 must stay CmisModuleFaultCause"
    );
}

#[test]
fn per_revision_code_sets_for_far_end_and_media_interface() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis");
    let page00 = load_page_schema(&root.join("page-00h-administrative.yaml"))
        .expect("page00 schema must load");

    let far_end_field = page00
        .fields
        .iter()
        .find(|f| f.name == "far_end_configuration")
        .expect("far_end_configuration not found");

    let media_tech_field = page00
        .fields
        .iter()
        .find(|f| f.name == "media_interface_technology")
        .expect("media_interface_technology not found");

    for &rev in Rev::CMIS_ALL {
        let r_far = resolve_field_for_rev(far_end_field, rev);
        let far_entries = r_far
            .enum_inline
            .as_ref()
            .and_then(|v| v.as_sequence())
            .expect("far_end_configuration must have enum_inline");

        let far_names: Vec<String> = far_entries
            .iter()
            .map(|e| {
                e.as_mapping()
                    .unwrap()
                    .get("name")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();

        if rev < Rev::V5_3 {
            // CMIS 3.0, 4.0, 5.0, 5.1, 5.2: 27-31 Reserved
            assert!(
                far_names.contains(&"reserved_27_31".to_string()),
                "CMIS {} far_end_configuration must contain reserved_27_31",
                rev.as_str()
            );
            assert!(
                !far_names.contains(&"sixteen_lane_modules".to_string()),
                "CMIS {} far_end_configuration must NOT contain sixteen_lane_modules (code 27 reserved before 5.3)",
                rev.as_str()
            );
            assert!(
                !far_names.contains(&"custom".to_string()),
                "CMIS {} far_end_configuration must NOT contain custom (code 31 reserved before 5.3)",
                rev.as_str()
            );
        } else {
            // CMIS 5.3: 27 16-lane, 28-30 Reserved, 31 Custom
            assert!(
                far_names.contains(&"sixteen_lane_modules".to_string()),
                "CMIS 5.3 far_end_configuration must contain sixteen_lane_modules"
            );
            assert!(
                far_names.contains(&"reserved_28_30".to_string()),
                "CMIS 5.3 far_end_configuration must contain reserved_28_30"
            );
            assert!(
                far_names.contains(&"custom".to_string()),
                "CMIS 5.3 far_end_configuration must contain custom"
            );
            assert!(
                !far_names.contains(&"reserved_27_31".to_string()),
                "CMIS 5.3 far_end_configuration must NOT contain reserved_27_31"
            );
        }

        let r_media = resolve_field_for_rev(media_tech_field, rev);
        let media_entries = r_media
            .enum_inline
            .as_ref()
            .and_then(|v| v.as_sequence())
            .expect("media_interface_technology must have enum_inline");

        let media_names: Vec<String> = media_entries
            .iter()
            .map(|e| {
                e.as_mapping()
                    .unwrap()
                    .get("name")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();

        if rev < Rev::V5_0 {
            // CMIS 3.0, 4.0: 10h-FFh Reserved
            assert!(
                media_names.contains(&"reserved_10_ff".to_string()),
                "CMIS {} media_interface_technology must contain reserved_10_ff",
                rev.as_str()
            );
            assert!(
                !media_names.contains(&"c_band_tunable_laser".to_string()),
                "CMIS {} media_interface_technology must NOT contain c_band_tunable_laser (defined in 5.0)",
                rev.as_str()
            );
            assert!(
                !media_names.contains(&"copper_near_far_linear_active_equalizers".to_string()),
                "CMIS {} media_interface_technology must NOT contain linear active equalizers (defined in 5.3)",
                rev.as_str()
            );
        } else if rev < Rev::V5_3 {
            // CMIS 5.0, 5.1, 5.2: 10h/11h defined, 12h-FFh Reserved
            assert!(
                media_names.contains(&"c_band_tunable_laser".to_string()),
                "CMIS {} media_interface_technology must contain c_band_tunable_laser",
                rev.as_str()
            );
            assert!(
                media_names.contains(&"l_band_tunable_laser".to_string()),
                "CMIS {} media_interface_technology must contain l_band_tunable_laser",
                rev.as_str()
            );
            assert!(
                media_names.contains(&"reserved_12_ff".to_string()),
                "CMIS {} media_interface_technology must contain reserved_12_ff",
                rev.as_str()
            );
            assert!(
                !media_names.contains(&"copper_near_far_linear_active_equalizers".to_string()),
                "CMIS {} media_interface_technology must NOT contain linear active equalizers (defined in 5.3)",
                rev.as_str()
            );
        } else {
            // CMIS 5.3: 10h/11h defined, 12h/13h/14h defined, 15h-FFh Reserved
            assert!(
                media_names.contains(&"c_band_tunable_laser".to_string()),
                "CMIS 5.3 media_interface_technology must contain c_band_tunable_laser"
            );
            assert!(
                media_names.contains(&"l_band_tunable_laser".to_string()),
                "CMIS 5.3 media_interface_technology must contain l_band_tunable_laser"
            );
            assert!(
                media_names.contains(&"copper_near_far_linear_active_equalizers".to_string()),
                "CMIS 5.3 media_interface_technology must contain copper_near_far_linear_active_equalizers"
            );
            assert!(
                media_names.contains(&"reserved_15_ff".to_string()),
                "CMIS 5.3 media_interface_technology must contain reserved_15_ff"
            );
        }
    }
}

// ── General inline-enum totality validator tests (B8) ──────────────────────

#[test]
fn tree_wide_inline_enums_valid_across_all_spec_families() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "yaml") {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut pages = Vec::new();
    for path in &files {
        let kind = openxcvr_codegen::parser::index::probe_schema_kind(path);
        if matches!(
            kind.as_deref(),
            Some("openxcvr.page.v1") | Some("openxcvr.page.v2")
        ) {
            let page = load_page_schema(path)
                .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()));
            pages.push(page);
        }
    }

    let mut total_blocks = 0;
    for page in &pages {
        for field in &page.fields {
            if field.enum_inline.is_some() {
                total_blocks += 1;
            }
            if let Some(changed_in) = &field.changed_in {
                for change in changed_in {
                    if change.enum_inline.is_some() {
                        total_blocks += 1;
                    }
                }
            }
        }
    }

    // 142 = 112 (CMIS 5.3 baseline) + 14 with the CMIS 5.4 stage-2
    // GridSpacingTx<n> windowing (the through-5.3 code set where 1001b is
    // Reserved, recorded as a changed_in block, and the since-5.4 code set
    // where 1001b is the 300 GHz grid spacing, CMIS 5.4 Table 8-109)
    // + 15 inline blocks in the c-cmis-1.5 companion namespace
    // + 1 in cmis-lt-1.0 (50h host preset storage level)
    // + 10 in c-cmis-1.1/1.2/1.3 (threshold-select and tx-filter enums,
    //   plus the 01.3 Rx LOS type)
    // + 3 in c-cmis-1.0 (two threshold-select enums and the tx-filter enum).
    assert_eq!(
        total_blocks, 155,
        "expected 155 inline enum blocks across all page schemas"
    );
    validate_inline_enums(&pages).expect("all inline enums on main must pass validation");
}

#[test]
fn validate_inline_enums_fails_on_missing_code_in_normal_block() {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: TestMissingCode
title: Test Missing Code
page_address: 00h
fields:
  - name: code_field
    offset: 0
    type: u2
    enum_inline:
      - { value: 0, name: zero }
      - { value: 1, name: one }
      - { value: 2, name: two }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    let err = validate_inline_enums(&[schema]).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("TestMissingCode"),
        "expected page id in error: {msg}"
    );
    assert!(
        msg.contains("code_field"),
        "expected field name in error: {msg}"
    );
    assert!(
        msg.contains("missing value 3"),
        "expected missing value 3 in error: {msg}"
    );
}

#[test]
fn validate_inline_enums_fails_on_missing_code_in_e2_c_block() {
    // E2-c block tx_filter_type expects 1..=3. If code 3 is missing, it must fail.
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: CCmisPage31hMediaLaneProvisioning
title: C-CMIS Page 31h
page_address: 31h
fields:
  - name: tx_filter_type
    offset: 128
    type: u8
    enum_inline:
      - { value: 1, name: root_raised_cosine }
      - { value: 2, name: raised_cosine }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    let err = validate_inline_enums(&[schema]).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("tx_filter_type"),
        "expected field name in error: {msg}"
    );
}

#[test]
fn validate_inline_enums_fails_on_overlapping_range() {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: TestOverlap
title: Test Overlap
page_address: 00h
fields:
  - name: overlap_field
    offset: 0
    type: u2
    enum_inline:
      - { value: 0, name: zero }
      - { value: "1..2", name: one_to_two }
      - { value: "2..3", name: two_to_three }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    let err = validate_inline_enums(&[schema]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("overlap"), "expected overlap in error: {msg}");
}

#[test]
fn validate_inline_enums_fails_on_duplicate_name() {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: TestDupName
title: Test Duplicate Name
page_address: 00h
fields:
  - name: dup_name_field
    offset: 0
    type: u2
    enum_inline:
      - { value: 0, name: same_name }
      - { value: 1, name: same_name }
      - { value: 2, name: two }
      - { value: 3, name: three }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    let err = validate_inline_enums(&[schema]).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate"),
        "expected duplicate in error: {msg}"
    );
}

#[test]
fn validate_inline_enums_fails_on_negative_code_value() {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: TestNegVal
title: Test Negative Value
page_address: 00h
fields:
  - name: neg_field
    offset: 0
    type: u2
    enum_inline:
      - { value: -1, name: neg }
      - { value: 0, name: zero }
      - { value: 1, name: one }
      - { value: 2, name: two }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    let err = validate_inline_enums(&[schema]).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("negative"),
        "expected negative in error: {msg}"
    );
}

#[test]
fn validate_inline_enums_fails_on_base_not_equal_highest_changed_in() {
    let yaml = r#"
schema: openxcvr.page.v2
kind: page
id: TestBaseHighest
title: Test Base Highest
page_address: 00h
fields:
  - name: rev_field
    offset: 0
    type: u2
    enum_inline:
      - { value: 0, name: zero }
      - { value: 1, name: one }
      - { value: 2, name: two }
      - { value: 3, name: three_base }
    changed_in:
      - rev: "5.0"
        enum_inline:
          - { value: 0, name: zero }
          - { value: 1, name: one }
          - { value: 2, name: two }
          - { value: 3, name: three_v5_0 }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    let err = validate_inline_enums(&[schema]).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("base") || msg.contains("changed_in"),
        "expected base/changed_in mismatch in error: {msg}"
    );
}

#[test]
fn validate_inline_enums_passes_on_e1_instance_legend_block() {
    let yaml = r#"
schema: openxcvr.page.v1
kind: page
id: TestE1Legend
title: Test E1 Legend
page_address: 00h
fields:
  - name: legend_field
    offset: 0
    type: bool
    instances:
      index: lane
      start: 0
      end: 3
    enum_inline:
      - { value: 0, name: lane_0 }
      - { value: 1, name: lane_1 }
      - { value: 2, name: lane_2 }
      - { value: 3, name: lane_3 }
"#;
    let schema: PageSchema = serde_norway::from_str(yaml).unwrap();
    validate_inline_enums(&[schema]).expect("E1 instance legend block must pass");
}
