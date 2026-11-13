//! Regression: CMIS 5.3 page 13h pattern-select setters are writable u4 enum
//! fields where lane pairs share a byte (Table 8-110/8-112/8-116/8-118
//! addressing). Until the sub-byte enum RMW repair these generated setters
//! wrote the whole byte, so programming one lane destroyed its neighbour
//! lane's nibble. This exercises the *shipped* generated surface — the
//! emitter-level proof lives in openxcvr-codegen's
//! `subbyte_enum_setters` compile-and-execute tests.
use openxcvr_cmis::generated::{CmisDiagnosticsPatternId, CmisPage13hDiagnosticsControl};

#[test]
fn pattern_select_setters_preserve_the_paired_lane_nibble() {
    // Lane pairs: lanes 1/2 → byte 20, 3/4 → 21, 5/6 → 22, 7/8 → 23
    // (page-relative). Even lanes occupy the high nibble, odd the low.
    for bg in 0u8..=255 {
        for raw in 0u8..=15 {
            let value = CmisDiagnosticsPatternId::from(raw);
            for byte in [20usize, 21, 22, 23] {
                // Low-nibble lane (odd) must leave the high nibble intact.
                let mut low = [0u8; 128];
                low[byte] = bg;
                CmisPage13hDiagnosticsControl::set_host_side_generator_pattern_select(
                    &mut low,
                    (byte - 20) * 2,
                    value,
                )
                .unwrap();
                assert_eq!(
                    low[byte],
                    (bg & 0xf0) | raw,
                    "odd lane clobbered neighbour: bg={bg:#04x} raw={raw} byte={byte}"
                );

                // High-nibble lane (even) must leave the low nibble intact.
                let mut high = [0u8; 128];
                high[byte] = bg;
                CmisPage13hDiagnosticsControl::set_host_side_generator_pattern_select(
                    &mut high,
                    (byte - 20) * 2 + 1,
                    value,
                )
                .unwrap();
                assert_eq!(
                    high[byte],
                    (bg & 0x0f) | (raw << 4),
                    "even lane clobbered neighbour: bg={bg:#04x} raw={raw} byte={byte}"
                );
            }
        }
    }
}

#[test]
fn checker_pattern_select_setters_preserve_the_paired_lane_nibble() {
    // Checker fields: host side at bytes 36-39, media side at 44-47,
    // same nibble pairing as the generator fields.
    for bg in 0u8..=255 {
        for raw in 0u8..=15 {
            let value = CmisDiagnosticsPatternId::from(raw);
            for byte in [36usize, 37, 38, 39, 44, 45, 46, 47] {
                let low_index = if byte < 44 { byte - 36 } else { byte - 44 };
                let mut low = [0u8; 128];
                low[byte] = bg;
                if byte < 44 {
                    CmisPage13hDiagnosticsControl::set_host_side_checker_pattern_select(
                        &mut low,
                        low_index * 2,
                        value,
                    )
                    .unwrap();
                } else {
                    CmisPage13hDiagnosticsControl::set_media_side_checker_pattern_select(
                        &mut low,
                        low_index * 2,
                        value,
                    )
                    .unwrap();
                }
                assert_eq!(
                    low[byte],
                    (bg & 0xf0) | raw,
                    "odd lane clobbered: byte={byte}"
                );

                let mut high = [0u8; 128];
                high[byte] = bg;
                if byte < 44 {
                    CmisPage13hDiagnosticsControl::set_host_side_checker_pattern_select(
                        &mut high,
                        low_index * 2 + 1,
                        value,
                    )
                    .unwrap();
                } else {
                    CmisPage13hDiagnosticsControl::set_media_side_checker_pattern_select(
                        &mut high,
                        low_index * 2 + 1,
                        value,
                    )
                    .unwrap();
                }
                assert_eq!(
                    high[byte],
                    (bg & 0x0f) | (raw << 4),
                    "even lane clobbered: byte={byte}"
                );
            }
        }
    }
}
