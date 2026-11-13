//! Regression: the page 22h `format_id` field is a genuine two-byte field
//! (SFF-8636 v2.12 § 7, Tables 7-1/7-2: "128-129 | 2 | Format ID | CA1Bh —
//! Indicates page 22h has the calibration format for Optical Modules"; the
//! loopback entry is 100Bh). The schema (`size_bytes: 2`, enum values up to
//! 0xFFFF) and the Rust page decoder must agree: the raw value is read as a
//! 16-bit big-endian integer, never truncated to the first byte.

use openxcvr_sff8636::generated::union::Sff8636Page22hHighAccuracyTiming;
use openxcvr_sff8636::generated::v2_12;
use openxcvr_sff8636::generated::Sff8636Page22hFormatId;

/// Page 22h base is byte 128 of the 256-byte upper map: the page decoder's
/// buffer starts at flat offset 128, so format_id (flat 128-129) is
/// `buf[0..2]`.
fn page22h(first_two: [u8; 2]) -> Sff8636Page22hHighAccuracyTiming {
    let mut buf = [0u8; 128];
    buf[0] = first_two[0];
    buf[1] = first_two[1];
    Sff8636Page22hHighAccuracyTiming::decode(&buf)
}

#[test]
fn format_id_decodes_optical_calibration_0xca1b() {
    let page = page22h([0xCA, 0x1B]);
    assert_eq!(
        page.format_id,
        Sff8636Page22hFormatId::OpticalModuleCalibration
    );
    assert_eq!(
        page.format_id.repr(),
        0xCA1B,
        "repr must carry the full 16-bit value"
    );
}

#[test]
fn format_id_decodes_loopback_calibration_0x100b() {
    let page = page22h([0x10, 0x0B]);
    assert_eq!(
        page.format_id,
        Sff8636Page22hFormatId::LoopbackModuleCalibration
    );
    assert_eq!(page.format_id.repr(), 0x100B);
}

#[test]
fn format_id_does_not_truncate_reserved_values() {
    // 0xCA1C falls in the 0xCA1C..=0xFFFF reserved range: the first byte
    // alone (0xCA) also decodes to a reserved value, so assert on the exact
    // repr to prove both bytes reached the enum, and that a two-byte value
    // whose low byte alone would be a *different* enum's territory is never
    // confused with a one-byte read.
    let page = page22h([0xAB, 0x1B]);
    let repr = page.format_id.repr();
    assert_eq!(repr, 0xAB1B, "16-bit value must survive decode unchanged");
    assert!(!matches!(
        page.format_id,
        Sff8636Page22hFormatId::OpticalModuleCalibration
    ));
}

#[test]
fn format_id_decodes_full_width_at_revision_2_12() {
    let mut buf = [0u8; 128];
    buf[0] = 0xCA;
    buf[1] = 0x1B;
    let page = v2_12::Sff8636Page22hHighAccuracyTiming::decode(&buf);
    assert_eq!(page.format_id.repr(), 0xCA1B);
}
