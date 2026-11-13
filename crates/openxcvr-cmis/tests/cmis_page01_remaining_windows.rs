//! Direct 206-cell primary-source matrix and control regressions for Page 01h
//! remaining capability windows across CMIS 3.0, 4.0, 5.0, 5.1, 5.2, 5.3 (5.4 excluded).
//!
//! Covers physical bytes 128-132, 137, 145, 163-175, 251-254 (following PR #92).
//!
//! Expectations are pinned to the literal primary specifications:
//! - 3.0: CMIS3p0_Third_Party_Spec.pdf
//! - 4.0: CMIS4p0_Third_Party_Spec.pdf
//! - 5.0: CMIS5p0_Third_Party_Spec.pdf
//! - 5.1: CMIS5p1_Third_Party_Spec.pdf
//! - 5.2: OIF-CMIS-05.2.pdf
//! - 5.3: OIF-CMIS-05.3.pdf

const V3_0_SOURCE: &str = include_str!("../src/generated/v3_0/CmisPage01hAdvertising.rs");
const V4_0_SOURCE: &str = include_str!("../src/generated/v4_0/CmisPage01hAdvertising.rs");
const V5_0_SOURCE: &str = include_str!("../src/generated/v5_0/CmisPage01hAdvertising.rs");
const V5_1_SOURCE: &str = include_str!("../src/generated/v5_1/CmisPage01hAdvertising.rs");
const V5_2_SOURCE: &str = include_str!("../src/generated/v5_2/CmisPage01hAdvertising.rs");
const V5_3_SOURCE: &str = include_str!("../src/generated/v5_3/CmisPage01hAdvertising.rs");

struct FieldOracle {
    name: &'static str,
    ty: &'static str,
    off: u16,
    bit: u8,
    width: u8,
    // Expected presence across [3.0, 4.0, 5.0, 5.1, 5.2, 5.3].
    // None = changed meaning or unqualified in source -> not asserted.
    expected_presence: [Option<bool>; 6],
}

const ORACLES: [FieldOracle; 35] = [
    // ---- bytes 128-131: firmware / hardware revisions ----
    FieldOracle {
        name: "module_inactive_firmware_major_revision",
        ty: "u8",
        off: 128,
        bit: 0,
        width: 8,
        expected_presence: [
            None,
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "module_inactive_firmware_minor_revision",
        ty: "u8",
        off: 129,
        bit: 0,
        width: 8,
        expected_presence: [
            None,
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "module_hardware_major_revision",
        ty: "u8",
        off: 130,
        bit: 0,
        width: 8,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "module_hardware_minor_revision",
        ty: "u8",
        off: 131,
        bit: 0,
        width: 8,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    // ---- byte 132 ----
    FieldOracle {
        name: "length_multiplier_smf",
        ty: "u8",
        off: 132,
        bit: 6,
        width: 2,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "base_length_smf",
        ty: "u8",
        off: 132,
        bit: 0,
        width: 6,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    // ---- byte 137 ----
    FieldOracle {
        name: "length_multiplier_smf2",
        ty: "u8",
        off: 137,
        bit: 6,
        width: 2,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    // ---- byte 145 ----
    FieldOracle {
        name: "cooling_implemented",
        ty: "bool",
        off: 145,
        bit: 7,
        width: 1,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "tx_input_clocking_capabilities",
        ty: "u8",
        off: 145,
        bit: 5,
        width: 2,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "e_pps_supported",
        ty: "bool",
        off: 145,
        bit: 4,
        width: 1,
        expected_presence: [
            Some(false),
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "timing_page15h_supported",
        ty: "bool",
        off: 145,
        bit: 3,
        width: 1,
        expected_presence: [
            Some(false),
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "aux3_mon_observable",
        ty: "bool",
        off: 145,
        bit: 2,
        width: 1,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "aux2_mon_observable",
        ty: "bool",
        off: 145,
        bit: 1,
        width: 1,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "aux1_mon_observable",
        ty: "bool",
        off: 145,
        bit: 0,
        width: 1,
        expected_presence: [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    // ---- bytes 163-166: CDB advertising ----
    FieldOracle {
        name: "cdb_instances_supported",
        ty: "u8",
        off: 163,
        bit: 6,
        width: 2,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_background_mode_supported",
        ty: "bool",
        off: 163,
        bit: 5,
        width: 1,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_auto_paging_supported",
        ty: "bool",
        off: 163,
        bit: 4,
        width: 1,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_max_pages_epl",
        ty: "u8",
        off: 163,
        bit: 0,
        width: 4,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_read_write_length_extension",
        ty: "u8",
        off: 164,
        bit: 0,
        width: 8,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_command_trigger_method",
        ty: "bool",
        off: 165,
        bit: 7,
        width: 1,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_ext_max_busy_time",
        ty: "u8",
        off: 165,
        bit: 0,
        width: 5,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_max_busy_spec_method",
        ty: "bool",
        off: 166,
        bit: 7,
        width: 1,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "cdb_max_busy_time",
        ty: "u8",
        off: 166,
        bit: 0,
        width: 7,
        expected_presence: [Some(false), Some(true), None, None, Some(true), Some(true)],
    },
    // ---- bytes 167-169: additional durations ----
    FieldOracle {
        name: "max_duration_module_pwr_dn",
        ty: "u8",
        off: 167,
        bit: 4,
        width: 4,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "max_duration_module_pwr_up",
        ty: "u8",
        off: 167,
        bit: 0,
        width: 4,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "max_duration_dp_tx_turn_off",
        ty: "u8",
        off: 168,
        bit: 4,
        width: 4,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "max_duration_dp_tx_turn_on",
        ty: "u8",
        off: 168,
        bit: 0,
        width: 4,
        expected_presence: [
            Some(false),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ],
    },
    FieldOracle {
        name: "max_duration_bpc",
        ty: "u8",
        off: 169,
        bit: 0,
        width: 4,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    // ---- byte 175 ----
    FieldOracle {
        name: "nad_banks_supported",
        ty: "u8",
        off: 175,
        bit: 0,
        width: 4,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    // ---- bytes 251-252 ----
    FieldOracle {
        name: "scratch_pad_supported",
        ty: "u8",
        off: 251,
        bit: 6,
        width: 2,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    FieldOracle {
        name: "password_entry_supported",
        ty: "u8",
        off: 251,
        bit: 4,
        width: 2,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    FieldOracle {
        name: "password_entry_result_supported",
        ty: "u8",
        off: 251,
        bit: 2,
        width: 2,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    FieldOracle {
        name: "full_page_read_supported",
        ty: "u8",
        off: 251,
        bit: 0,
        width: 2,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    FieldOracle {
        name: "host_lane_switching_supported",
        ty: "bool",
        off: 252,
        bit: 7,
        width: 1,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
    FieldOracle {
        name: "link_training_supported",
        ty: "bool",
        off: 252,
        bit: 6,
        width: 1,
        expected_presence: [
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
        ],
    },
];

fn expected_decode_str(off: u16, bit: u8, width: u8) -> String {
    let buf_idx = off - 128;
    if width == 8 {
        format!("buf[{buf_idx}]")
    } else if width == 1 {
        format!("(buf[{buf_idx}] >> {bit}) & 1 != 0")
    } else {
        let mask = (1u32 << width) - 1;
        if bit == 0 {
            format!("buf[{buf_idx}] & {mask:#04x}")
        } else {
            format!("(buf[{buf_idx}] >> {bit}) & {mask:#04x}")
        }
    }
}

#[test]
fn primary_source_206_cell_presence_matrix() {
    let sources = [
        ("3.0", V3_0_SOURCE),
        ("4.0", V4_0_SOURCE),
        ("5.0", V5_0_SOURCE),
        ("5.1", V5_1_SOURCE),
        ("5.2", V5_2_SOURCE),
        ("5.3", V5_3_SOURCE),
    ];

    let mut failures = Vec::new();
    let mut asserted_count = 0;
    for oracle in &ORACLES {
        for (i, (rev_name, src)) in sources.iter().enumerate() {
            if let Some(expected) = oracle.expected_presence[i] {
                asserted_count += 1;
                let field_decl = format!("pub {}: {},", oracle.name, oracle.ty);
                let present = src.contains(&field_decl);
                if present != expected {
                    failures.push(format!(
                        "MISMATCH {} @ {}: generated type is {}, want {}",
                        oracle.name,
                        rev_name,
                        if present { "present" } else { "absent" },
                        if expected { "present" } else { "absent" },
                    ));
                }
            }
        }
    }
    assert_eq!(asserted_count, 206, "expected exactly 206 asserted cells");
    assert!(
        failures.is_empty(),
        "206-cell primary matrix failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn controls_nonempty_common_bias_latest_and_addresses() {
    let sources = [
        ("3.0", V3_0_SOURCE),
        ("4.0", V4_0_SOURCE),
        ("5.0", V5_0_SOURCE),
        ("5.1", V5_1_SOURCE),
        ("5.2", V5_2_SOURCE),
        ("5.3", V5_3_SOURCE),
    ];
    for (rev_name, src) in sources {
        // Control: nonempty (> 90 fields)
        let field_count = src.matches("    pub ").count();
        assert!(
            field_count >= 90,
            "CONTROL nonempty failed for {rev_name}: found only {field_count} fields"
        );
        // Control: common bias field in all 6
        assert!(
            src.contains("pub tx_bias_current_scaling_factor: u8,"),
            "CONTROL common bias field missing in {rev_name}"
        );
    }
    // Control: latest revision v5_3 has all 35 fields
    for oracle in &ORACLES {
        let field_decl = format!("pub {}: {},", oracle.name, oracle.ty);
        assert!(
            V5_3_SOURCE.contains(&field_decl),
            "CONTROL latest field missing in 5.3: {}",
            oracle.name
        );
    }
    // Control: address and bit decoder expressions on every exposed field
    for oracle in &ORACLES {
        let dec_pattern = format!(
            "{}: {},",
            oracle.name,
            expected_decode_str(oracle.off, oracle.bit, oracle.width)
        );
        for (i, (rev_name, src)) in sources.iter().enumerate() {
            if oracle.expected_presence[i] == Some(true) {
                assert!(
                    src.contains(&dec_pattern),
                    "CONTROL decoder address mismatch in {rev_name} for {}: expected `{}`",
                    oracle.name,
                    dec_pattern
                );
            }
        }
    }
}

#[test]
fn decode_at_first_defined_revision() {
    // 1. v3_0 hardware revision fields (offset 130 and 131, buf indices 2 and 3)
    let mut buf = [0u8; 128];
    buf[2] = 0x12; // major
    buf[3] = 0x34; // minor
    let decoded_v3 = openxcvr_cmis::generated::v3_0::CmisPage01hAdvertising::decode(&buf);
    let debug_v3 = format!("{decoded_v3:?}");
    assert!(
        debug_v3.contains("module_hardware_major_revision: 18"),
        "v3_0 real-type decode must expose module_hardware_major_revision, got {debug_v3}"
    );
    assert!(
        debug_v3.contains("module_hardware_minor_revision: 52"),
        "v3_0 real-type decode must expose module_hardware_minor_revision, got {debug_v3}"
    );

    // 2. v4_0 duration fields (offset 167 and 168, buf indices 39 and 40)
    let mut buf_v4 = [0u8; 128];
    buf_v4[39] = (0x5 << 4) | 0x6; // pwr_dn=5, pwr_up=6
    buf_v4[40] = (0x7 << 4) | 0x8; // tx_turn_off=7, tx_turn_on=8
    let decoded_v4 = openxcvr_cmis::generated::v4_0::CmisPage01hAdvertising::decode(&buf_v4);
    assert_eq!(decoded_v4.max_duration_module_pwr_dn, 5);
    assert_eq!(decoded_v4.max_duration_module_pwr_up, 6);
    assert_eq!(decoded_v4.max_duration_dp_tx_turn_off, 7);
    assert_eq!(decoded_v4.max_duration_dp_tx_turn_on, 8);

    // 3. v5_3 fields
    let mut buf_v53 = [0u8; 128];
    buf_v53[9] = 0x80; // offset 137 length_multiplier_smf2 = 2
    buf_v53[41] = 0x09; // offset 169 max_duration_bpc = 9
    buf_v53[47] = 0x03; // offset 175 nad_banks_supported = 3
    buf_v53[124] = 0xC0; // offset 252 host_lane_switching=true, link_training=true
    let decoded_v53 = openxcvr_cmis::generated::v5_3::CmisPage01hAdvertising::decode(&buf_v53);
    assert_eq!(decoded_v53.length_multiplier_smf2, 2);
    assert_eq!(decoded_v53.max_duration_bpc, 9);
    assert_eq!(decoded_v53.nad_banks_supported, 3);
    assert!(decoded_v53.host_lane_switching_supported);
    assert!(decoded_v53.link_training_supported);
}

#[test]
fn byte251_neighbour_false_assertions() {
    use openxcvr_cmis::generated::v5_3::CmisPage01hAdvertising;

    // Byte 251 (buf index 123) layout:
    // bits 7:6 - scratch_pad_supported (0xC0)
    // bits 5:4 - password_entry_supported (0x30)
    // bits 3:2 - password_entry_result_supported (0x0C)
    // bits 1:0 - full_page_read_supported (0x03)

    // Case 1: scratch_pad_supported only (0xC0)
    let mut buf = [0u8; 128];
    buf[123] = 0xC0;
    let decoded = CmisPage01hAdvertising::decode(&buf);
    assert_eq!(decoded.scratch_pad_supported, 3);
    assert_eq!(
        decoded.password_entry_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(
        decoded.password_entry_result_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(
        decoded.full_page_read_supported, 0,
        "neighbour must be false/0"
    );

    // Case 2: password_entry_supported only (0x30)
    let mut buf = [0u8; 128];
    buf[123] = 0x30;
    let decoded = CmisPage01hAdvertising::decode(&buf);
    assert_eq!(
        decoded.scratch_pad_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(decoded.password_entry_supported, 3);
    assert_eq!(
        decoded.password_entry_result_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(
        decoded.full_page_read_supported, 0,
        "neighbour must be false/0"
    );

    // Case 3: password_entry_result_supported only (0x0C)
    let mut buf = [0u8; 128];
    buf[123] = 0x0C;
    let decoded = CmisPage01hAdvertising::decode(&buf);
    assert_eq!(
        decoded.scratch_pad_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(
        decoded.password_entry_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(decoded.password_entry_result_supported, 3);
    assert_eq!(
        decoded.full_page_read_supported, 0,
        "neighbour must be false/0"
    );

    // Case 4: full_page_read_supported only (0x03)
    let mut buf = [0u8; 128];
    buf[123] = 0x03;
    let decoded = CmisPage01hAdvertising::decode(&buf);
    assert_eq!(
        decoded.scratch_pad_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(
        decoded.password_entry_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(
        decoded.password_entry_result_supported, 0,
        "neighbour must be false/0"
    );
    assert_eq!(decoded.full_page_read_supported, 3);

    // Setters test for byte 251 fields on v5_3
    let mut set_buf = [0x55u8; 128];
    CmisPage01hAdvertising::set_scratch_pad_supported(&mut set_buf, 0x2);
    assert_eq!(set_buf[123], (0x55 & !0xC0) | (0x2 << 6));

    let mut set_buf = [0x55u8; 128];
    CmisPage01hAdvertising::set_password_entry_supported(&mut set_buf, 0x1);
    assert_eq!(set_buf[123], (0x55 & !0x30) | (0x1 << 4));

    let mut set_buf = [0x55u8; 128];
    CmisPage01hAdvertising::set_password_entry_result_supported(&mut set_buf, 0x2);
    assert_eq!(set_buf[123], (0x55 & !0x0C) | (0x2 << 2));

    let mut set_buf = [0x55u8; 128];
    CmisPage01hAdvertising::set_full_page_read_supported(&mut set_buf, 0x3);
    assert_eq!(set_buf[123], (0x55 & !0x03) | 0x3);
}

#[test]
fn older_types_do_not_expose_unsupported_fields_or_setters() {
    let sources = [
        ("3.0", V3_0_SOURCE),
        ("4.0", V4_0_SOURCE),
        ("5.0", V5_0_SOURCE),
        ("5.1", V5_1_SOURCE),
        ("5.2", V5_2_SOURCE),
    ];
    let mut failures = Vec::new();
    for (rev_idx, (rev_name, src)) in sources.iter().enumerate() {
        for oracle in &ORACLES {
            if oracle.expected_presence[rev_idx] == Some(false) {
                let field_decl = format!("pub {}:", oracle.name);
                if src.contains(&field_decl) {
                    failures.push(format!("field leak: {rev_name} exposes {field_decl}"));
                }
                let setter = format!("pub fn set_{}(", oracle.name);
                if src.contains(&setter) {
                    failures.push(format!("setter leak: {rev_name} exposes {setter}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Older types leak unsupported fields or setters:\n{}",
        failures.join("\n")
    );
}

#[test]
fn v4_and_v53_read_classification_masks() {
    use openxcvr_cmis::generated::{union, v4_0, v5_3};

    let v4_classes = v4_0::CmisPage01hAdvertising::read_classifications();
    let v5_classes = v5_3::CmisPage01hAdvertising::read_classifications();
    let union_classes = union::CmisPage01hAdvertising::read_classifications();

    // v5_3 and union classifications remain identical
    assert_eq!(v5_classes, union_classes);

    // Index 9 (byte 137): length_multiplier_smf2 moved since 5.3 -> bits 7:6 move unknown -> uncovered
    assert_eq!(v4_classes[9].readable_mask, 0x00);
    assert_eq!(v4_classes[9].unknown_mask, 0x00);
    assert_eq!(v4_classes[9].uncovered_mask, 0xff);
    assert_eq!(v4_classes[9].write_only_mask, 0x00);
    assert!(!v4_classes[9].is_qualified_none());

    // Index 41 (byte 169): max_duration_bpc moved since 5.3 -> bits 3:0 move unknown -> uncovered
    assert_eq!(v4_classes[41].readable_mask, 0x00);
    assert_eq!(v4_classes[41].unknown_mask, 0x00);
    assert_eq!(v4_classes[41].uncovered_mask, 0xff);
    assert_eq!(v4_classes[41].write_only_mask, 0x00);
    assert!(!v4_classes[41].is_qualified_none());

    // Index 47 (byte 175): nad_banks_supported moved since 5.3 -> bits 3:0 move unknown -> uncovered
    assert_eq!(v4_classes[47].readable_mask, 0x00);
    assert_eq!(v4_classes[47].unknown_mask, 0x00);
    assert_eq!(v4_classes[47].uncovered_mask, 0xff);
    assert_eq!(v4_classes[47].write_only_mask, 0x00);
    assert!(!v4_classes[47].is_qualified_none());

    // Index 123 (byte 251): scratch_pad, password_entry*, full_page_read moved since 5.3 -> all 8 bits move unknown -> uncovered
    assert_eq!(v4_classes[123].readable_mask, 0x00);
    assert_eq!(v4_classes[123].unknown_mask, 0x00);
    assert_eq!(v4_classes[123].uncovered_mask, 0xff);
    assert_eq!(v4_classes[123].write_only_mask, 0x00);
    assert!(!v4_classes[123].is_qualified_none());

    // Index 124 (byte 252): host_lane_switching, link_training moved since 5.3 -> bits 7:6 move unknown -> uncovered
    assert_eq!(v4_classes[124].readable_mask, 0x00);
    assert_eq!(v4_classes[124].unknown_mask, 0x00);
    assert_eq!(v4_classes[124].uncovered_mask, 0xff);
    assert_eq!(v4_classes[124].write_only_mask, 0x00);
    assert!(!v4_classes[124].is_qualified_none());
}

#[test]
fn catalog_logical_windows() {
    use openxcvr_cmis::generated::catalog::CATALOG;
    use openxcvr_core::rev::Rev;

    let revs = [
        (Rev::CMIS_3_0, 0),
        (Rev::CMIS_4_0, 1),
        (Rev::CMIS_5_0, 2),
        (Rev::CMIS_5_1, 3),
        (Rev::CMIS_5_2, 4),
        (Rev::CMIS_5_3, 5),
    ];

    let mut failures = Vec::new();
    for oracle in &ORACLES {
        for (rev, rev_idx) in revs {
            if let Some(expected) = oracle.expected_presence[rev_idx] {
                let active = CATALOG.rows.iter().any(|row| {
                    row.name == oracle.name
                        && row.since.is_none_or(|s| rev >= s)
                        && row.until.is_none_or(|u| rev < u)
                });
                if active != expected {
                    failures.push(format!(
                        "Catalog resolution mismatch for {} at {:?}: active={}, want {}",
                        oracle.name, rev, active, expected
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Catalog resolution failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
