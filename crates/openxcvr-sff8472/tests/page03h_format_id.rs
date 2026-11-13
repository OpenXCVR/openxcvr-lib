//! Regression: the A2h page 03h `format_id` field is a genuine two-byte
//! field (SFF-8472 v12.5a § 13, Tables 13-1/13-2: "128-129 | 2 | Format ID |
//! CA1Bh - Indicates page 03h has the calibration format for Optical
//! Modules"; the loopback entry is 100Bh). The schema (`size_bytes: 2`, enum
//! values up to 0xFFFF) and the Rust page decoder must agree: the raw value
//! is read as a 16-bit big-endian integer, never truncated to the first
//! byte.

use openxcvr_sff8472::generated::union::PageA2h03hHighAccuracyTiming;
use openxcvr_sff8472::generated::v12_5;
use openxcvr_sff8472::generated::Sff8472Page03hFormatId;

fn page03h(first_two: [u8; 2]) -> PageA2h03hHighAccuracyTiming {
    let mut buf = [0u8; 256];
    // Page-buffer bytes: A2h page 03h is a 256-byte page whose recorded
    // offsets are page-buffer bytes from 0, so format_id sits at 128-129.
    buf[128] = first_two[0];
    buf[129] = first_two[1];
    PageA2h03hHighAccuracyTiming::decode(&buf)
}

#[test]
fn format_id_decodes_optical_calibration_0xca1b() {
    let page = page03h([0xCA, 0x1B]);
    assert_eq!(
        page.format_id,
        Sff8472Page03hFormatId::OpticalModuleCalibration
    );
    assert_eq!(
        page.format_id.repr(),
        0xCA1B,
        "repr must carry the full 16-bit value"
    );
}

#[test]
fn format_id_decodes_loopback_calibration_0x100b() {
    let page = page03h([0x10, 0x0B]);
    assert_eq!(
        page.format_id,
        Sff8472Page03hFormatId::LoopbackModuleCalibration
    );
    assert_eq!(page.format_id.repr(), 0x100B);
}

#[test]
fn format_id_does_not_truncate_reserved_values() {
    // A two-byte value whose low byte alone would decode differently must
    // reach the enum unchanged — proving both bytes were read.
    let page = page03h([0xAB, 0x1B]);
    assert_eq!(page.format_id.repr(), 0xAB1B);
    assert!(!matches!(
        page.format_id,
        Sff8472Page03hFormatId::OpticalModuleCalibration
    ));
}

#[test]
fn format_id_decodes_full_width_at_revision_12_5() {
    let mut buf = [0u8; 256];
    buf[128] = 0xCA;
    buf[129] = 0x1B;
    let page = v12_5::PageA2h03hHighAccuracyTiming::decode(&buf);
    assert_eq!(page.format_id.repr(), 0xCA1B);
}
