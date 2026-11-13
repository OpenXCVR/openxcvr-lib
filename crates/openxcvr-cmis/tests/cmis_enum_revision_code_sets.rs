//! Test-first regression for CMIS revision-specific enum code sets.
//!
//! Asserts that:
//! - v5_0, v5_1, v5_2 module_fault_cause decode leaves codes 4-6 unnamed (folding into Unknown)
//!   because CMIS 5.0 Table 8-15 p117, 5.1 Table 8-15 p133, and 5.2 Table 8-16 p136
//!   print "4-31: Reserved (fault codes)".
//! - v5_3 module_fault_cause decode names codes 4-6:
//!   4: TransmitterFault, 5: ReceiverFault, 6: TemperatureRelatedFault
//!   per CMIS 5.3 Table 8-16 (Lower Memory) p153.
//! - 0x07 is unnamed as a negative control at every revision (5.0, 5.1, 5.2, 5.3)
//!   because it is Reserved across all of them.

use openxcvr_cmis::generated::{v5_0, v5_1, v5_2, v5_3};

fn lower_memory_with_fault_cause(code: u8) -> [u8; 128] {
    let mut buf = [0u8; 128];
    buf[0] = 0x18; // QSFP-DD identifier
    buf[41] = code; // 00h:41 ModuleFaultCause
    buf
}

#[test]
fn module_fault_cause_decode_per_revision() {
    let later_only_codes: &[(u8, &str)] = &[
        (0x04, "TransmitterFault"),
        (0x05, "ReceiverFault"),
        (0x06, "TemperatureRelatedFault"),
    ];

    for &(code, name_5_3) in later_only_codes {
        let b5_0 = lower_memory_with_fault_cause(code);
        let n5_0 = v5_0::CmisLowerMemory::decode(&b5_0)
            .module_fault_cause
            .name();
        assert!(
            n5_0.starts_with("Unknown"),
            "CMIS 5.0 must leave code {code:#04x} unnamed (Reserved 4-31 per 5.0 Table 8-15 p117), got {n5_0}"
        );

        let b5_1 = lower_memory_with_fault_cause(code);
        let n5_1 = v5_1::CmisLowerMemory::decode(&b5_1)
            .module_fault_cause
            .name();
        assert!(
            n5_1.starts_with("Unknown"),
            "CMIS 5.1 must leave code {code:#04x} unnamed (Reserved 4-31 per 5.1 Table 8-15 p133), got {n5_1}"
        );

        let b5_2 = lower_memory_with_fault_cause(code);
        let n5_2 = v5_2::CmisLowerMemory::decode(&b5_2)
            .module_fault_cause
            .name();
        assert!(
            n5_2.starts_with("Unknown"),
            "CMIS 5.2 must leave code {code:#04x} unnamed (Reserved 4-31 per 5.2 Table 8-16 p136), got {n5_2}"
        );

        // Control: 5.3 is the first revision to define codes 4-6
        let b5_3 = lower_memory_with_fault_cause(code);
        let n5_3 = v5_3::CmisLowerMemory::decode(&b5_3)
            .module_fault_cause
            .name();
        assert_eq!(
            n5_3, name_5_3,
            "CMIS 5.3 must name code {code:#04x} as {name_5_3} (per 5.3 Table 8-16 p153)"
        );
    }

    // Negative control: 0x07 is Reserved (fault codes) at every revision 5.0-5.3
    let b_neg = lower_memory_with_fault_cause(0x07);
    let neg_5_0 = v5_0::CmisLowerMemory::decode(&b_neg)
        .module_fault_cause
        .name();
    assert!(
        neg_5_0.starts_with("Unknown"),
        "Negative control: 0x07 must be unnamed at 5.0, got {neg_5_0}"
    );

    let neg_5_1 = v5_1::CmisLowerMemory::decode(&b_neg)
        .module_fault_cause
        .name();
    assert!(
        neg_5_1.starts_with("Unknown"),
        "Negative control: 0x07 must be unnamed at 5.1, got {neg_5_1}"
    );

    let neg_5_2 = v5_2::CmisLowerMemory::decode(&b_neg)
        .module_fault_cause
        .name();
    assert!(
        neg_5_2.starts_with("Unknown"),
        "Negative control: 0x07 must be unnamed at 5.2, got {neg_5_2}"
    );

    let neg_5_3 = v5_3::CmisLowerMemory::decode(&b_neg)
        .module_fault_cause
        .name();
    assert!(
        neg_5_3.starts_with("Unknown"),
        "Negative control: 0x07 must be unnamed at 5.3, got {neg_5_3}"
    );
}
