//! Direct primary-source matrix and control regressions for Lower Memory (Page 00h)
//! presence windows across CMIS 3.0, 4.0, 5.0, 5.1, 5.2, 5.3 (5.4 excluded).
//!
//! Covers physical bytes 8, 26, 27, 31, 39, 40, 41, 42, 56, 57, 60, 61, 62
//! for the twenty fields gaining `since` windows.
//!
//! Expectations are pinned to the literal primary specifications:
//! - 3.0: CMIS3p0_Third_Party_Spec.pdf
//! - 4.0: CMIS4p0_Third_Party_Spec.pdf
//! - 5.0: CMIS5p0_Third_Party_Spec.pdf
//! - 5.1: CMIS5p1_Third_Party_Spec.pdf
//! - 5.2: OIF-CMIS-05.2.pdf
//! - 5.3: OIF-CMIS-05.3.pdf

const V3_0_SOURCE: &str = include_str!("../src/generated/v3_0/CmisLowerMemory.rs");
const V4_0_SOURCE: &str = include_str!("../src/generated/v4_0/CmisLowerMemory.rs");
const V5_0_SOURCE: &str = include_str!("../src/generated/v5_0/CmisLowerMemory.rs");
const V5_1_SOURCE: &str = include_str!("../src/generated/v5_1/CmisLowerMemory.rs");
const V5_2_SOURCE: &str = include_str!("../src/generated/v5_2/CmisLowerMemory.rs");
const V5_3_SOURCE: &str = include_str!("../src/generated/v5_3/CmisLowerMemory.rs");

struct FieldOracle {
    name: &'static str,
    ty: &'static str,
    expected_types: Option<[&'static str; 6]>,
    first_defined: &'static str,
    // Expected presence across [3.0, 4.0, 5.0, 5.1, 5.2, 5.3]
    expected_presence: [bool; 6],
    anchor: &'static str,
}

impl FieldOracle {
    fn expected_type(&self, rev_idx: usize) -> &'static str {
        if let Some(types) = self.expected_types {
            types[rev_idx]
        } else {
            self.ty
        }
    }
}

const ORACLES: [FieldOracle; 20] = [
    // ---- byte 8: CDB completion flags (since 4.0; 3.0 T21 p51 reserves 8.7-3) ----
    FieldOracle {
        name: "cdb_cmd_complete_flag_1",
        ty: "bool",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T21 p51 `8 7-3 Reserved` / 4.0 T8-5 p85 `6 L-CDB block 1 complete`",
    },
    FieldOracle {
        name: "cdb_cmd_complete_flag_2",
        ty: "bool",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T21 p51 `8 7-3 Reserved` / 4.0 T8-5 p85 `7 L-CDB block 2 complete`",
    },
    // ---- byte 26: global controls ----
    FieldOracle {
        name: "low_pwr_allow_request_hw",
        ty: "bool",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T23 p53 `26 7-6 Reserved` / 4.0 T8-7 p87 `6 LowPwr`",
    },
    FieldOracle {
        name: "bank_broadcast_enable",
        ty: "bool",
        expected_types: None,
        first_defined: "5.2",
        expected_presence: [false, false, false, false, true, true],
        anchor: "5.1 T8-10 p129 `26 7 - Reserved` / 5.2 T8-11 p132 `26 7 BankBroadcastEnable`",
    },
    // ---- byte 27: MCI speed configuration ----
    FieldOracle {
        name: "mci_speed_configuration",
        ty: "u8",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-11 p132 `27-28 All - Reserved[2]` / 5.3 T8-11 p149 `3-0 MciSpeedConfiguration`",
    },
    // ---- byte 31: CDB completion masks (since 4.0; 3.0 T24 p53 reserves 31.7-1) ----
    FieldOracle {
        name: "cdb_cmd_complete_mask_1",
        ty: "bool",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T24 p53 `31 7-1 Reserved` / 4.0 T8-8 p88 `6 M-CDB Block 1 complete`",
    },
    FieldOracle {
        name: "cdb_cmd_complete_mask_2",
        ty: "bool",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T24 p53 `31 7-1 Reserved` / 4.0 T8-8 p88 `7 M-CDB Block 2 complete`",
    },
    // ---- bytes 39-40: active firmware revisions (since 4.0; 3.0 T17 p48 reserves 37-63) ----
    FieldOracle {
        name: "module_active_firmware_major_revision",
        ty: "u8",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T17 p48 `37-63 27 Reserved` / 4.0 T8-11 p90 `39 7-0 Active Module firmware major revision`",
    },
    FieldOracle {
        name: "module_active_firmware_minor_revision",
        ty: "u8",
        expected_types: None,
        first_defined: "4.0",
        expected_presence: [false, true, true, true, true, true],
        anchor: "3.0 T17 p48 `37-63 27 Reserved` / 4.0 T8-11 p90 `40 7-0 Active Module firmware minor revision`",
    },
    // ---- byte 41: module fault cause (since 5.0; 4.0 T8-1 p82 reserves 41-63) ----
    FieldOracle {
        name: "module_fault_cause",
        ty: "CmisModuleFaultCause",
        expected_types: Some([
            "",
            "",
            "CmisModuleFaultCausePre5p3",
            "CmisModuleFaultCausePre5p3",
            "CmisModuleFaultCausePre5p3",
            "CmisModuleFaultCause",
        ]),
        first_defined: "5.0",
        expected_presence: [false, false, true, true, true, true],
        anchor: "4.0 T8-1 p82 `41-63 23 Reserved Area` / 5.0 T8-15 p117 `41 7-0 ModuleFaultCause` / 5.0 T8-15 p117 `4-31: Reserved (fault codes)` / 5.3 T8-16 p153 `4: Transmitter fault`",
    },
    // ---- byte 42: password command result (since 5.3; 5.2 T8-4 p126 reserves 42-63) ----
    FieldOracle {
        name: "password_cmd_result",
        ty: "CmisPasswordCmdResult",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-17 p154 `3-0 PasswordCmdResult`",
    },
    // ---- bytes 56-57: extended module info (since 5.3) ----
    FieldOracle {
        name: "cmis_sm_support",
        ty: "CmisSmSupport",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-18 p154 `56 7-0 CmisSmSupport`",
    },
    FieldOracle {
        name: "module_function_type",
        ty: "CmisModuleFunctionType",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-18 p154 `57 7-0 ModuleFunctionType`",
    },
    // ---- bytes 60-61: subtype and fiber face type (since 5.3) ----
    FieldOracle {
        name: "sff8024_module_subtype",
        ty: "Sff8024TransceiverSubtype",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-18 p154 `3-0 SFF8024ModuleSubtype`",
    },
    FieldOracle {
        name: "sff8024_fiber_face_type",
        ty: "Sff8024FiberFaceType",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-18 p154 `1-0 SFF8024FiberFaceType`",
    },
    // ---- byte 62: low power restrictions (since 5.3) ----
    FieldOracle {
        name: "low_power_restrictions_validity",
        ty: "bool",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-19 p155 `7 ValidityIndication`",
    },
    FieldOracle {
        name: "low_power_restrictions_cdb_query_unsupported",
        ty: "bool",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-19 p155 `3 CdbQueryUnsupported`",
    },
    FieldOracle {
        name: "low_power_restrictions_cdb_fw_query_unsupported",
        ty: "bool",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-19 p155 `2 CdbFwQueryUnsupported`",
    },
    FieldOracle {
        name: "low_power_restrictions_cdb_cmds_unsupported",
        ty: "bool",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-19 p155 `1 CdbCmdsUnsupported`",
    },
    FieldOracle {
        name: "low_power_restrictions_cdb_fw_cmds_unsupported",
        ty: "bool",
        expected_types: None,
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        anchor: "5.2 T8-4 p126 `42-63 22 - Reserved[22]` / 5.3 T8-19 p155 `0 CdbFwCmdsUnsupported`",
    },
];

struct ControlOracle {
    name: &'static str,
    ty: &'static str,
}

const CONTROL_ORACLES: [ControlOracle; 5] = [
    ControlOracle {
        name: "sff8024_identifier",
        ty: "Sff8024Identifier",
    },
    ControlOracle {
        name: "cmis_revision",
        ty: "u8",
    },
    ControlOracle {
        name: "memory_model",
        ty: "bool",
    },
    ControlOracle {
        name: "module_state",
        ty: "CmisModuleState",
    },
    ControlOracle {
        name: "temp_mon_value",
        ty: "i16",
    },
];

#[test]
fn primary_source_lower_memory_presence_matrix() {
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
            asserted_count += 1;
            let field_decl = format!("pub {}: {},", oracle.name, oracle.expected_type(i));
            let present = src.contains(&field_decl);
            let expected = oracle.expected_presence[i];
            if present != expected {
                failures.push(format!(
                    "MISMATCH {} @ {}: generated type is {}, want {} (first defined {}; {})",
                    oracle.name,
                    rev_name,
                    if present { "present" } else { "absent" },
                    if expected { "present" } else { "absent" },
                    oracle.first_defined,
                    oracle.anchor,
                ));
            }
        }
    }
    assert_eq!(asserted_count, 120, "expected exactly 120 asserted cells");
    assert!(
        failures.is_empty(),
        "Lower Memory presence matrix failures ({}/120):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn controls_nonempty_and_control_fields() {
    let sources = [
        ("3.0", V3_0_SOURCE),
        ("4.0", V4_0_SOURCE),
        ("5.0", V5_0_SOURCE),
        ("5.1", V5_1_SOURCE),
        ("5.2", V5_2_SOURCE),
        ("5.3", V5_3_SOURCE),
    ];
    for (rev_name, src) in sources {
        // Control 1: nonempty (> 80 fields)
        let field_count = src.matches("    pub ").count();
        assert!(
            field_count >= 80,
            "CONTROL nonempty failed for {rev_name}: found only {field_count} fields"
        );
        // Control 2: control fields present across all 6 revisions
        for control in &CONTROL_ORACLES {
            let decl = format!("pub {}: {},", control.name, control.ty);
            assert!(
                src.contains(&decl),
                "CONTROL field missing in {rev_name}: {}",
                control.name
            );
        }
    }
    // Control 3: latest revision v5_3 has all 20 windowed fields
    for oracle in &ORACLES {
        let field_decl = format!("pub {}: {},", oracle.name, oracle.expected_type(5));
        assert!(
            V5_3_SOURCE.contains(&field_decl),
            "CONTROL latest field missing in 5.3: {}",
            oracle.name
        );
    }
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
            if !oracle.expected_presence[rev_idx] {
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
            let expected = oracle.expected_presence[rev_idx];
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
    assert!(
        failures.is_empty(),
        "Catalog resolution failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn exhaustive_byte2_speed_and_neighbour_decodes() {
    use openxcvr_cmis::generated::{v3_0, v4_0, v5_0, v5_1, v5_2, v5_3};

    for b in 0..=255u8 {
        let mut buf = [0u8; 128];
        buf[2] = b;

        // 3.0: 2-bit speed at bits 3-2; bit 7 memory_model
        let d3 = v3_0::CmisLowerMemory::decode(&buf);
        assert_eq!(
            d3.mci_max_speed,
            (b >> 2) & 0x03,
            "v3_0 speed mismatch for byte 0x{b:02x}: got {}, want {}",
            d3.mci_max_speed,
            (b >> 2) & 0x03
        );
        assert_eq!(d3.memory_model, (b >> 7) & 1 != 0);

        // 4.0: 2-bit speed at bits 3-2 (aliased to twi_max_speed_cmis4); bit 7 memory_model
        let d4 = v4_0::CmisLowerMemory::decode(&buf);
        assert_eq!(
            d4.twi_max_speed_cmis4,
            (b >> 2) & 0x03,
            "v4_0 speed mismatch for byte 0x{b:02x}: got {}, want {}",
            d4.twi_max_speed_cmis4,
            (b >> 2) & 0x03
        );
        assert_eq!(d4.memory_model, (b >> 7) & 1 != 0);

        // 5.0: 2-bit speed at bits 3-2; bit 7 memory_model; bit 6 stepped_config_only
        let d5_0 = v5_0::CmisLowerMemory::decode(&buf);
        assert_eq!(
            d5_0.mci_max_speed,
            (b >> 2) & 0x03,
            "v5_0 speed mismatch for byte 0x{b:02x}: got {}, want {}",
            d5_0.mci_max_speed,
            (b >> 2) & 0x03
        );
        assert_eq!(d5_0.memory_model, (b >> 7) & 1 != 0);
        assert_eq!(d5_0.stepped_config_only, (b >> 6) & 1 != 0);

        // 5.1: 2-bit speed at bits 3-2; bit 7 memory_model; bit 6 stepped_config_only
        let d5_1 = v5_1::CmisLowerMemory::decode(&buf);
        assert_eq!(
            d5_1.mci_max_speed,
            (b >> 2) & 0x03,
            "v5_1 speed mismatch for byte 0x{b:02x}: got {}, want {}",
            d5_1.mci_max_speed,
            (b >> 2) & 0x03
        );
        assert_eq!(d5_1.memory_model, (b >> 7) & 1 != 0);
        assert_eq!(d5_1.stepped_config_only, (b >> 6) & 1 != 0);

        // 5.2: 2-bit speed at bits 3-2; bit 7 memory_model; bit 6 stepped_config_only
        let d5_2 = v5_2::CmisLowerMemory::decode(&buf);
        assert_eq!(
            d5_2.mci_max_speed,
            (b >> 2) & 0x03,
            "v5_2 speed mismatch for byte 0x{b:02x}: got {}, want {}",
            d5_2.mci_max_speed,
            (b >> 2) & 0x03
        );
        assert_eq!(d5_2.memory_model, (b >> 7) & 1 != 0);
        assert_eq!(d5_2.stepped_config_only, (b >> 6) & 1 != 0);

        // 5.3: 4-bit speed at bits 5-2; bit 7 memory_model; bit 6 stepped_config_only; bits 1-0 auto_commissioning
        let d5_3 = v5_3::CmisLowerMemory::decode(&buf);
        assert_eq!(
            d5_3.mci_max_speed,
            (b >> 2) & 0x0f,
            "v5_3 speed mismatch for byte 0x{b:02x}: got {}, want {}",
            d5_3.mci_max_speed,
            (b >> 2) & 0x0f
        );
        assert_eq!(d5_3.memory_model, (b >> 7) & 1 != 0);
        assert_eq!(d5_3.stepped_config_only, (b >> 6) & 1 != 0);
        assert_eq!(d5_3.auto_commissioning, b & 0x03);
    }
}

#[test]
fn bit_field_setters_preserve_neighbouring_bits() {
    use openxcvr_cmis::generated::v5_3::CmisLowerMemory;

    // Test byte 26 setters:
    // bit 7: bank_broadcast_enable
    // bit 6: low_pwr_allow_request_hw
    let mut buf = [0x55u8; 128]; // 0b0101_0101
    CmisLowerMemory::set_bank_broadcast_enable(&mut buf, true);
    assert_eq!(buf[26], 0x55 | 0x80);
    CmisLowerMemory::set_bank_broadcast_enable(&mut buf, false);
    assert_eq!(buf[26], 0x55 & !0x80);

    let mut buf = [0x55u8; 128];
    CmisLowerMemory::set_low_pwr_allow_request_hw(&mut buf, false);
    assert_eq!(buf[26], 0x55 & !0x40);
    CmisLowerMemory::set_low_pwr_allow_request_hw(&mut buf, true);
    assert_eq!(buf[26], 0x55 | 0x40);

    // Test byte 27 setter:
    // bits 3-0: mci_speed_configuration
    let mut buf = [0xF0u8; 128];
    CmisLowerMemory::set_mci_speed_configuration(&mut buf, 0x0A);
    assert_eq!(buf[27], 0xFA);

    // Test byte 31 setters:
    // bit 6: cdb_cmd_complete_mask_1
    // bit 7: cdb_cmd_complete_mask_2
    // Drive each retained setter 0->1 and 1->0 with neighbouring bits checked (A2).
    let mut buf = [0x15u8; 128]; // 0b0001_0101 (bit 6 = 0, bit 7 = 0)

    // cdb_cmd_complete_mask_1: drive 0 -> 1
    CmisLowerMemory::set_cdb_cmd_complete_mask_1(&mut buf, true);
    assert_eq!(buf[31], 0x15 | 0x40, "mask_1 0->1 transition failed");
    // cdb_cmd_complete_mask_1: drive 1 -> 0
    CmisLowerMemory::set_cdb_cmd_complete_mask_1(&mut buf, false);
    assert_eq!(buf[31], 0x15, "mask_1 1->0 transition failed");

    // cdb_cmd_complete_mask_2: drive 0 -> 1
    CmisLowerMemory::set_cdb_cmd_complete_mask_2(&mut buf, true);
    assert_eq!(buf[31], 0x15 | 0x80, "mask_2 0->1 transition failed");
    // cdb_cmd_complete_mask_2: drive 1 -> 0
    CmisLowerMemory::set_cdb_cmd_complete_mask_2(&mut buf, false);
    assert_eq!(buf[31], 0x15, "mask_2 1->0 transition failed");

    // Cross-check: opposite bit already 1
    let mut buf = [0x95u8; 128]; // bit 7 = 1, bit 6 = 0
    CmisLowerMemory::set_cdb_cmd_complete_mask_1(&mut buf, true);
    assert_eq!(buf[31], 0xD5, "mask_1 0->1 with bit 7=1 failed");
    CmisLowerMemory::set_cdb_cmd_complete_mask_1(&mut buf, false);
    assert_eq!(buf[31], 0x95, "mask_1 1->0 with bit 7=1 failed");

    let mut buf = [0x55u8; 128]; // bit 6 = 1, bit 7 = 0
    CmisLowerMemory::set_cdb_cmd_complete_mask_2(&mut buf, true);
    assert_eq!(buf[31], 0xD5, "mask_2 0->1 with bit 6=1 failed");
    CmisLowerMemory::set_cdb_cmd_complete_mask_2(&mut buf, false);
    assert_eq!(buf[31], 0x55, "mask_2 1->0 with bit 6=1 failed");
}

fn base_v4_read_classifications() -> [openxcvr_core::types::management::ByteClassification; 128] {
    use openxcvr_core::types::management::{ByteClassification, ChangeSource, ReadEffect};
    std::array::from_fn(|index| match index {
        0..=2 => ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::None],
            ChangeSource::Static,
        ),
        3 => ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::None],
            ChangeSource::Dynamic,
        ),
        4..=7 => ByteClassification::from_masks(0x00, 0x0f, 0xf0, 0x00, &[], ChangeSource::Dynamic),
        8 => ByteClassification::from_masks(
            0xc7,
            0x00,
            0x38,
            0x00,
            &[ReadEffect::ClearOnRead],
            ChangeSource::Dynamic,
        ),
        9..=11 => ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::ClearOnRead],
            ChangeSource::Dynamic,
        ),
        12 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        13 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        14..=25 => {
            ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Dynamic)
        }
        26 => ByteClassification::from_masks(0x00, 0xf0, 0x07, 0x08, &[], ChangeSource::Dynamic),
        27 => ByteClassification::from_masks(0x00, 0x0f, 0xf0, 0x00, &[], ChangeSource::Dynamic),
        28..=30 => {
            ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static)
        }
        31 => ByteClassification::from_masks(0x00, 0xc7, 0x38, 0x00, &[], ChangeSource::Dynamic),
        32..=34 => {
            ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Dynamic)
        }
        35 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        36 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        37 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Dynamic),
        38 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Dynamic),
        39 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Static),
        40 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Static),
        41 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Dynamic),
        42 => ByteClassification::from_masks(0x00, 0x0f, 0xf0, 0x00, &[], ChangeSource::Dynamic),
        43..=55 => {
            ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static)
        }
        56 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Static),
        57 => ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Static),
        58 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        59 => ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        60 => ByteClassification::from_masks(0x00, 0x0f, 0xf0, 0x00, &[], ChangeSource::Static),
        61 => ByteClassification::from_masks(0x00, 0x03, 0xfc, 0x00, &[], ChangeSource::Static),
        62 => ByteClassification::from_masks(0x00, 0x8f, 0x70, 0x00, &[], ChangeSource::Static),
        63..=84 => {
            ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static)
        }
        85..=117 => ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::None],
            ChangeSource::Static,
        ),
        118..=125 => {
            ByteClassification::from_masks(0x00, 0x00, 0x00, 0xff, &[], ChangeSource::Dynamic)
        }
        126 => ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::None],
            ChangeSource::Dynamic,
        ),
        127 => ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::None],
            ChangeSource::Dynamic,
        ),
        _ => unreachable!("array index"),
    })
}

struct ClassificationDelta {
    byte: usize,
    unknown_to_uncovered: u8,
    change_source_delta: Option<(
        openxcvr_core::types::management::ChangeSource,
        openxcvr_core::types::management::ChangeSource,
    )>,
}

// Explicit expected-delta set between base and head for CMIS 4.0 Lower Memory:
// Mask deltas: bits moving unknown -> uncovered when presence windows are narrowed.
// ChangeSource deltas: bytes 27, 41, 42 lose all field coverage at 4.0, so the generator
// naturally defaults them to Static (empty coverage default), changing Dynamic -> Static.
const EXPECTED_DELTAS: &[ClassificationDelta] = &[
    ClassificationDelta {
        byte: 26,
        unknown_to_uncovered: 0x80, // bank_broadcast_enable (bit 7)
        change_source_delta: None,  // remains Dynamic
    },
    ClassificationDelta {
        byte: 27,
        unknown_to_uncovered: 0x0f, // mci_speed_configuration (bits 3:0)
        change_source_delta: Some((
            openxcvr_core::types::management::ChangeSource::Dynamic,
            openxcvr_core::types::management::ChangeSource::Static,
        )),
    },
    ClassificationDelta {
        byte: 41,
        unknown_to_uncovered: 0xff, // module_fault_cause (bits 7:0)
        change_source_delta: Some((
            openxcvr_core::types::management::ChangeSource::Dynamic,
            openxcvr_core::types::management::ChangeSource::Static,
        )),
    },
    ClassificationDelta {
        byte: 42,
        unknown_to_uncovered: 0x0f, // password_cmd_result (bits 3:0)
        change_source_delta: Some((
            openxcvr_core::types::management::ChangeSource::Dynamic,
            openxcvr_core::types::management::ChangeSource::Static,
        )),
    },
    ClassificationDelta {
        byte: 56,
        unknown_to_uncovered: 0xff, // cmis_sm_support (bits 7:0)
        change_source_delta: None,  // remains Static
    },
    ClassificationDelta {
        byte: 57,
        unknown_to_uncovered: 0xff, // module_function_type (bits 7:0)
        change_source_delta: None,  // remains Static
    },
    ClassificationDelta {
        byte: 60,
        unknown_to_uncovered: 0x0f, // sff8024_module_subtype (bits 3:0)
        change_source_delta: None,  // remains Static
    },
    ClassificationDelta {
        byte: 61,
        unknown_to_uncovered: 0x03, // sff8024_fiber_face_type (bits 1:0)
        change_source_delta: None,  // remains Static
    },
    ClassificationDelta {
        byte: 62,
        unknown_to_uncovered: 0x8f, // low power restrictions (bits 7, 3:0)
        change_source_delta: None,  // remains Static
    },
];

#[test]
fn v4_read_classification_exact_transformation() {
    use openxcvr_cmis::{generated::v4_0::CmisLowerMemory, read_planning::CmisReadPlanner};
    use openxcvr_core::{
        types::management::{ChangeSource, NonPassiveReason, Range, RangeClassified},
        Rev,
    };

    let base = base_v4_read_classifications();
    let head = CmisLowerMemory::read_classifications();
    assert_eq!(base.len(), 128);
    assert_eq!(head.len(), 128);

    // 1. Strict comparison across all 128 bytes covering EVERY ByteClassification member:
    // readable_mask, write_only_mask, known_effects, unknown_mask, uncovered_mask, change_source.
    for i in 0..128 {
        let delta = EXPECTED_DELTAS.iter().find(|d| d.byte == i);
        let mask = delta.map(|d| d.unknown_to_uncovered).unwrap_or(0);

        assert_eq!(
            head[i].readable_mask, base[i].readable_mask,
            "byte {i}: readable_mask changed"
        );
        assert_eq!(
            head[i].write_only_mask, base[i].write_only_mask,
            "byte {i}: write_only_mask changed"
        );
        assert_eq!(
            head[i].known_effects, base[i].known_effects,
            "byte {i}: known_effects changed"
        );

        assert_eq!(
            head[i].unknown_mask,
            base[i].unknown_mask & !mask,
            "byte {i}: unknown_mask mismatch"
        );
        assert_eq!(
            head[i].uncovered_mask,
            base[i].uncovered_mask | mask,
            "byte {i}: uncovered_mask mismatch"
        );

        match delta.and_then(|d| d.change_source_delta) {
            Some((expected_base_cs, expected_head_cs)) => {
                assert_eq!(
                    base[i].change_source, expected_base_cs,
                    "byte {i}: base change_source mismatch"
                );
                assert_eq!(
                    head[i].change_source, expected_head_cs,
                    "byte {i}: head change_source mismatch"
                );
            }
            None => {
                assert_eq!(
                    head[i].change_source, base[i].change_source,
                    "byte {i}: change_source changed unexpectedly: head is {:?}, base is {:?}",
                    head[i].change_source, base[i].change_source
                );
            }
        }
    }

    // 2. Assertions that bytes 27, 41 and 42 are wholly uncovered at head:
    for &byte in &[27, 41, 42] {
        assert_eq!(
            head[byte].uncovered_mask, 0xff,
            "byte {byte} uncovered_mask != 0xff"
        );
        assert_eq!(
            head[byte].readable_mask, 0x00,
            "byte {byte} readable_mask != 0x00"
        );
        assert_eq!(
            head[byte].unknown_mask, 0x00,
            "byte {byte} unknown_mask != 0x00"
        );
        assert_eq!(
            head[byte].write_only_mask, 0x00,
            "byte {byte} write_only_mask != 0x00"
        );
        assert!(
            head[byte].known_effects.is_empty(),
            "byte {byte} known_effects not empty"
        );
        assert_eq!(
            head[byte].change_source,
            ChangeSource::Static,
            "byte {byte} change_source != Static"
        );
        assert!(
            !head[byte].is_qualified_none(),
            "byte {byte} must not be qualified none"
        );
        assert_eq!(
            head[byte].passive_refusal(),
            Some(NonPassiveReason::MetadataAbsent),
            "byte {byte} passive_refusal != MetadataAbsent"
        );
    }

    // 3. Relevant Lower Memory admission/classification path refuses a range containing them:
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).expect("CMIS 4.0 planner available");
    for &byte in &[27, 41, 42] {
        let single_range = Range {
            address: 0x50,
            bank: 0,
            page: 0,
            offset: byte as u8,
            len: 1,
        };
        assert_eq!(
            planner.classify_range(Rev::CMIS_4_0, single_range),
            Ok(RangeClassified::NonPassive {
                byte: byte as u8,
                reason: NonPassiveReason::MetadataAbsent,
            }),
            "planner must refuse wholly uncovered single-byte range at byte {byte}"
        );
    }

    // Also verify multi-byte ranges starting at or covering bytes 27, 41, 42 are refused:
    let range_at_27 = Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 27,
        len: 4, // covers 27..=30
    };
    assert_eq!(
        planner.classify_range(Rev::CMIS_4_0, range_at_27),
        Ok(RangeClassified::NonPassive {
            byte: 27,
            reason: NonPassiveReason::MetadataAbsent,
        }),
        "planner must refuse multi-byte range starting at uncovered byte 27"
    );

    let range_at_41 = Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 41,
        len: 2, // covers 41 and 42
    };
    assert_eq!(
        planner.classify_range(Rev::CMIS_4_0, range_at_41),
        Ok(RangeClassified::NonPassive {
            byte: 41,
            reason: NonPassiveReason::MetadataAbsent,
        }),
        "planner must refuse multi-byte range starting at uncovered byte 41"
    );

    let range_at_42 = Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 42,
        len: 3, // covers 42..=44
    };
    assert_eq!(
        planner.classify_range(Rev::CMIS_4_0, range_at_42),
        Ok(RangeClassified::NonPassive {
            byte: 42,
            reason: NonPassiveReason::MetadataAbsent,
        }),
        "planner must refuse multi-byte range starting at uncovered byte 42"
    );
}
