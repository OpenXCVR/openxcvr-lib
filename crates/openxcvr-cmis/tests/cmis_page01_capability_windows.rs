//! Direct 24-cell primary-source matrix and control regressions for Page 01h
//! capability windows across CMIS 3.0, 4.0, 5.0, 5.1, 5.2, 5.3 (5.4 excluded).
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
    first_defined: &'static str,
    // Presence across [3.0, 4.0, 5.0, 5.1, 5.2, 5.3]
    expected_presence: [bool; 6],
    buf_index: usize,
    bit: u8,
}

const ORACLES: [FieldOracle; 4] = [
    FieldOracle {
        name: "network_path_pages_supported",
        first_defined: "5.1",
        expected_presence: [false, false, false, true, true, true],
        buf_index: 14, // Byte 142
        bit: 7,
    },
    FieldOracle {
        name: "bank_broadcast_supported",
        first_defined: "5.2",
        expected_presence: [false, false, false, false, true, true],
        buf_index: 28, // Byte 156
        bit: 7,
    },
    FieldOracle {
        name: "unidir_reconfig_supported",
        first_defined: "5.2",
        expected_presence: [false, false, false, false, true, true],
        buf_index: 34, // Byte 162
        bit: 6,
    },
    FieldOracle {
        name: "versatile_control_set_supported",
        first_defined: "5.3",
        expected_presence: [false, false, false, false, false, true],
        buf_index: 34, // Byte 162
        bit: 7,
    },
];

#[test]
fn primary_source_24_cell_presence_matrix() {
    let sources = [
        ("3.0", V3_0_SOURCE),
        ("4.0", V4_0_SOURCE),
        ("5.0", V5_0_SOURCE),
        ("5.1", V5_1_SOURCE),
        ("5.2", V5_2_SOURCE),
        ("5.3", V5_3_SOURCE),
    ];

    let mut failures = Vec::new();
    for oracle in &ORACLES {
        for (i, (rev_name, src)) in sources.iter().enumerate() {
            let field_decl = format!("pub {}: bool,", oracle.name);
            let present = src.contains(&field_decl);
            let expected = oracle.expected_presence[i];
            if present != expected {
                failures.push(format!(
                    "MISMATCH {} @ {}: generated type is {}, want {} (first defined {})",
                    oracle.name,
                    rev_name,
                    if present { "present" } else { "absent" },
                    if expected { "present" } else { "absent" },
                    oracle.first_defined,
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "24-cell primary matrix failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn controls_nonempty_and_common_bias() {
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
    // Control: latest revision v5_3 has all 4 fields
    for oracle in &ORACLES {
        let field_decl = format!("pub {}: bool,", oracle.name);
        assert!(
            V5_3_SOURCE.contains(&field_decl),
            "CONTROL latest field missing in 5.3: {}",
            oracle.name
        );
    }
    // Control: address and bit decoder expressions in 5.3
    for oracle in &ORACLES {
        let dec_expr = format!(
            "{}: (buf[{}] >> {}) & 1 != 0,",
            oracle.name, oracle.buf_index, oracle.bit
        );
        assert!(
            V5_3_SOURCE.contains(&dec_expr),
            "CONTROL decoder bit address mismatch in 5.3 for {}: expected {}",
            oracle.name,
            dec_expr
        );
    }
}

#[test]
fn nonzero_reserved_bits_and_accessors() {
    use openxcvr_cmis::generated::v5_3::CmisPage01hAdvertising;

    for oracle in &ORACLES {
        let mut buf = [0u8; 128];
        // Set only that bit
        buf[oracle.buf_index] = 1 << oracle.bit;
        let decoded = CmisPage01hAdvertising::decode(&buf);

        // Verify decoded field evaluates to true
        match oracle.name {
            "network_path_pages_supported" => assert!(decoded.network_path_pages_supported),
            "bank_broadcast_supported" => assert!(decoded.bank_broadcast_supported),
            "unidir_reconfig_supported" => assert!(decoded.unidir_reconfig_supported),
            "versatile_control_set_supported" => assert!(decoded.versatile_control_set_supported),
            _ => unreachable!(),
        }

        // Verify setter modifies only that bit
        let mut set_buf = [0x55u8; 128];
        match oracle.name {
            "network_path_pages_supported" => {
                CmisPage01hAdvertising::set_network_path_pages_supported(&mut set_buf, true);
                assert_eq!(set_buf[oracle.buf_index], 0x55 | (1 << oracle.bit));
                CmisPage01hAdvertising::set_network_path_pages_supported(&mut set_buf, false);
                assert_eq!(set_buf[oracle.buf_index], 0x55 & !(1 << oracle.bit));
            }
            "bank_broadcast_supported" => {
                CmisPage01hAdvertising::set_bank_broadcast_supported(&mut set_buf, true);
                assert_eq!(set_buf[oracle.buf_index], 0x55 | (1 << oracle.bit));
                CmisPage01hAdvertising::set_bank_broadcast_supported(&mut set_buf, false);
                assert_eq!(set_buf[oracle.buf_index], 0x55 & !(1 << oracle.bit));
            }
            "unidir_reconfig_supported" => {
                CmisPage01hAdvertising::set_unidir_reconfig_supported(&mut set_buf, true);
                assert_eq!(set_buf[oracle.buf_index], 0x55 | (1 << oracle.bit));
                CmisPage01hAdvertising::set_unidir_reconfig_supported(&mut set_buf, false);
                assert_eq!(set_buf[oracle.buf_index], 0x55 & !(1 << oracle.bit));
            }
            "versatile_control_set_supported" => {
                CmisPage01hAdvertising::set_versatile_control_set_supported(&mut set_buf, true);
                assert_eq!(set_buf[oracle.buf_index], 0x55 | (1 << oracle.bit));
                CmisPage01hAdvertising::set_versatile_control_set_supported(&mut set_buf, false);
                assert_eq!(set_buf[oracle.buf_index], 0x55 & !(1 << oracle.bit));
            }
            _ => unreachable!(),
        }
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
    for (rev_name, src) in sources {
        let rev_idx = match rev_name {
            "3.0" => 0,
            "4.0" => 1,
            "5.0" => 2,
            "5.1" => 3,
            "5.2" => 4,
            _ => unreachable!(),
        };
        for oracle in &ORACLES {
            if !oracle.expected_presence[rev_idx] {
                let setter = format!("pub fn set_{}(", oracle.name);
                if src.contains(&setter) {
                    failures.push(format!("setter leak: {rev_name} exposes {setter}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Older types leak setters for unannounced fields:\n{}",
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

    // Byte 142 (index 14) and Byte 156 (index 28) at CMIS 4.0:
    // Bit 7 is uncovered, not unknown.
    // Index 14 (byte 142): unknown_mask has bit 7 cleared (0x7f), uncovered_mask has bit 7 set (0x80)
    assert_eq!(v4_classes[14].readable_mask, 0x00);
    assert_eq!(v4_classes[14].unknown_mask, 0x7f);
    assert_eq!(v4_classes[14].uncovered_mask, 0x80);
    assert_eq!(v4_classes[14].write_only_mask, 0x00);
    assert!(!v4_classes[14].is_qualified_none());

    // Index 28 (byte 156): unknown_mask has bit 7 cleared (0x07), uncovered_mask has bit 7 set (0xf8)
    assert_eq!(v4_classes[28].readable_mask, 0x00);
    assert_eq!(v4_classes[28].unknown_mask, 0x07);
    assert_eq!(v4_classes[28].uncovered_mask, 0xf8);
    assert_eq!(v4_classes[28].write_only_mask, 0x00);
    assert!(!v4_classes[28].is_qualified_none());
}

#[test]
fn catalog_logical_windows_and_rowcount() {
    use openxcvr_cmis::generated::catalog::CATALOG;
    use openxcvr_core::rev::Rev;

    // Row count increased from 901 to 902 with the CMIS revision-specific enum code
    // sets repair (Option S): module_fault_cause splits into a CMIS 5.0-5.2 row
    // (CmisModuleFaultCausePre5p3) and a CMIS 5.3 row (CmisModuleFaultCause).
    // Row count increased from 902 to 915 with the CMIS 5.4 advertisement and
    // lower-memory deltas: AbnormalFwIndicationFlag (00h:8.3),
    // AbnormalFwIndicationMask (00h:31.3), SFF8024HeatsinkType (00h:61.7-4),
    // DefaultInputPolarityTx<lane> (01h:171), DefaultOutputPolarityRx<lane>
    // (01h:172), PageChSupported/PageDhSupported (01h:173),
    // Page60h/61h/62hSupported (01h:174.7-5), ExtraLaneBanksSupported
    // (01h:174.4-0) and MediaLaneSwitchingSupported (01h:252.5) — all
    // since CMIS 5.4.
    // Row count increased from 915 to 923 with the CMIS 5.4 NAD/control-set
    // deltas: HostInterfaceGIDNAD/MediaInterfaceGIDNAD + ReservedNADByte7
    // (1Ch:6-7), RedirectionCommitResultLane<lane> (1Dh:168-175) and
    // ReservedPage1Dh176To183, plus the U4->U8 NADBlockIndex changed_in
    // splits for SCS0/SCS1/ACS (18h/19h).
    // Row count increased from 923 to 1019 with the six new CMIS 5.4 pages
    // (all fields since 5.4, one row each): Page 0Ch Module Management
    // (28 fields, Tables 8-69..73), Page 0Dh Firmware Management (29 fields,
    // Tables 8-74..76), Page 60h Lane and Datapath Management Extensions
    // (13 fields, Tables 8-187..189), Page 61h Lane and Datapath Monitoring
    // (5 fields, Tables 8-190..191), Page 62h Lane Supervision Thresholds
    // (5 fields, Tables 8-192..194) and Page 6Dh Media Lane Switching
    // (16 fields, Tables 8-195..196).
    // Row count increased from 1019 to 1036 with the stage-2 CMIS 5.4
    // deltas (+17 net): nine new since-5.4 fields (LSR-01/02/04/05:
    // GridSupported300GHz + the 300 GHz channel pair + the 04h:196.6
    // advertisement + RelativeOutputPowerThresholdsEnableTx + the four
    // 12h:216-217 relative threshold offsets), the VDM since-window repair
    // (all 15 VDM pages' ungated fields gain since 4.0 - no row splits -
    // while each of the four descriptor pages' observable_type splits into
    // a through-5.3 row and a since-5.4 row carrying
    // CmisVdmObservableType5p4, +4), and the 2Fh deltas (DutyCycleSupport +
    // MonitoringDutyCycle new rows, +2; the byte-128 and byte-144 reserved
    // spans each split into an until-5.3 row and a shrunken since-5.4 row,
    // +2; PowerSavingSupport/PowerSavingMode gain since 5.3 without
    // splitting). LSR-07 (FineTuning* Rqd.->Adv. flags) and BEH-01
    // (NPInUseLane default behavior) change flags/behavior only and add no
    // rows.
    // Row count increased from 1036 to 1037 with the 5.4 qualification of
    // the ConfigStatus code set (Table 8-101): config_status_lane (Page 11h)
    // gains a changed_in 5.4 enum_ref to CmisConfigStatus5p4, splitting into
    // a through-5.3 row and a since-5.4 row. The freeze-handshake (M7)
    // changed_in behavior entries on 2Fh add no rows.
    assert_eq!(
        CATALOG.rows.len(),
        1037,
        "expected 1037 CMIS catalog rows after the 5.4 qualification deltas"
    );

    let revs = [
        (Rev::CMIS_3_0, 0),
        (Rev::CMIS_4_0, 1),
        (Rev::CMIS_5_0, 2),
        (Rev::CMIS_5_1, 3),
        (Rev::CMIS_5_2, 4),
        (Rev::CMIS_5_3, 5),
    ];

    for oracle in &ORACLES {
        for (rev, rev_idx) in revs {
            let active = CATALOG.rows.iter().any(|row| {
                row.name == oracle.name
                    && row.since.is_none_or(|s| rev >= s)
                    && row.until.is_none_or(|u| rev < u)
            });
            let expected = oracle.expected_presence[rev_idx];
            assert_eq!(
                active, expected,
                "Catalog resolution mismatch for {} at {:?}: active={}, want {}",
                oracle.name, rev, active, expected
            );
        }
    }
}
