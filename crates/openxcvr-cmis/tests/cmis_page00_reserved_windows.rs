//! Semantic coverage for CMIS Page 00h bytes 208 and 213 (Option D repair).
//! Binds condition C5 from the independent review.

use openxcvr_cmis::generated::catalog::CATALOG;
use openxcvr_core::field_catalog::{Scaling, Unit};
use openxcvr_core::rev::Rev;
use openxcvr_core::types::management::ReadEffect;

const V3_SOURCE: &str = include_str!("../src/generated/v3_0/CmisPage00hAdministrative.rs");
const V4_SOURCE: &str = include_str!("../src/generated/v4_0/CmisPage00hAdministrative.rs");
const V5_0_SOURCE: &str = include_str!("../src/generated/v5_0/CmisPage00hAdministrative.rs");
const V5_1_SOURCE: &str = include_str!("../src/generated/v5_1/CmisPage00hAdministrative.rs");
const V5_2_SOURCE: &str = include_str!("../src/generated/v5_2/CmisPage00hAdministrative.rs");
const V5_3_SOURCE: &str = include_str!("../src/generated/v5_3/CmisPage00hAdministrative.rs");

fn count_struct_fields(source: &str) -> usize {
    let start = source
        .find("pub struct CmisPage00hAdministrative {")
        .expect("struct start");
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("struct end");
    let struct_body = &rest[..end];
    struct_body
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("pub ")
                && trimmed.contains(':')
                && !trimmed.starts_with("pub struct")
        })
        .count()
}

#[test]
fn exact_per_revision_member_counts() {
    assert_eq!(
        count_struct_fields(V3_SOURCE),
        36,
        "v3_0 must have exactly 36 members"
    );
    assert_eq!(
        count_struct_fields(V4_SOURCE),
        39,
        "v4_0 must have exactly 39 members"
    );
    assert_eq!(
        count_struct_fields(V5_0_SOURCE),
        36,
        "v5_0 must have exactly 36 members"
    );
    assert_eq!(
        count_struct_fields(V5_1_SOURCE),
        36,
        "v5_1 must have exactly 36 members"
    );
    assert_eq!(
        count_struct_fields(V5_2_SOURCE),
        36,
        "v5_2 must have exactly 36 members"
    );
    assert_eq!(
        count_struct_fields(V5_3_SOURCE),
        39,
        "v5_3 must have exactly 39 members"
    );
}

#[test]
fn absence_of_member_setter_and_catalog_row_at_208_213_in_30_and_50_52() {
    let subject_names = [
        "attenuation_at_53p1ghz",
        "mci_flow_control_duration_encoding",
        "mci_flow_control_duration",
        "reserved_administrative_208_cmis4",
        "reserved_administrative_213_bit7_cmis4",
        "reserved_administrative_213_bits6_0_cmis4",
    ];

    let rev_sources = [
        ("v3_0", V3_SOURCE, Rev::CMIS_3_0),
        ("v5_0", V5_0_SOURCE, Rev::CMIS_5_0),
        ("v5_1", V5_1_SOURCE, Rev::CMIS_5_1),
        ("v5_2", V5_2_SOURCE, Rev::CMIS_5_2),
    ];

    for (rev_name, source, rev) in rev_sources {
        for name in subject_names {
            assert!(
                !source.contains(&format!("pub {name}:")),
                "{rev_name} must not contain member {name}"
            );
            assert!(
                !source.contains(&format!("pub fn set_{name}")),
                "{rev_name} must not contain setter for {name}"
            );
        }

        let active_at_208_213: Vec<_> = CATALOG
            .rows
            .iter()
            .filter(|r| {
                r.page_label == "00h"
                    && (r.offset == 208 || r.offset == 213)
                    && r.since.is_none_or(|s| rev >= s)
                    && r.until.is_none_or(|u| rev < u)
            })
            .collect();
        assert!(
            active_at_208_213.is_empty(),
            "{rev_name} must have no catalog rows at offset 208 or 213, but found: {:?}",
            active_at_208_213.iter().map(|r| r.name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn cmis4_catalog_resolves_reserved_raw_storage() {
    let rev = Rev::CMIS_4_0;
    let active_at_208_213: Vec<_> = CATALOG
        .rows
        .iter()
        .filter(|r| {
            r.page_label == "00h"
                && (r.offset == 208 || r.offset == 213)
                && r.since.is_none_or(|s| rev >= s)
                && r.until.is_none_or(|u| rev < u)
        })
        .collect();

    assert_eq!(
        active_at_208_213.len(),
        3,
        "CMIS 4.0 must have exactly 3 active rows at 208/213"
    );

    let row_208 = active_at_208_213
        .iter()
        .find(|r| r.name == "reserved_administrative_208_cmis4")
        .expect("reserved_administrative_208_cmis4 must be active at 4.0");
    assert_eq!(row_208.offset, 208);
    assert_eq!(row_208.scaling, Scaling::None);
    assert_eq!(row_208.category, Some("Reserved"));
    assert_eq!(row_208.read_effects, &[ReadEffect::None]);

    let row_213_bit7 = active_at_208_213
        .iter()
        .find(|r| r.name == "reserved_administrative_213_bit7_cmis4")
        .expect("reserved_administrative_213_bit7_cmis4 must be active at 4.0");
    assert_eq!(row_213_bit7.offset, 213);
    assert_eq!(row_213_bit7.bit_offset, 7);
    assert_eq!(row_213_bit7.bit_width, 1);
    assert_eq!(row_213_bit7.scaling, Scaling::None);
    assert_eq!(row_213_bit7.category, Some("Reserved"));
    assert_eq!(row_213_bit7.read_effects, &[ReadEffect::None]);

    let row_213_bits6_0 = active_at_208_213
        .iter()
        .find(|r| r.name == "reserved_administrative_213_bits6_0_cmis4")
        .expect("reserved_administrative_213_bits6_0_cmis4 must be active at 4.0");
    assert_eq!(row_213_bits6_0.offset, 213);
    assert_eq!(row_213_bits6_0.bit_offset, 0);
    assert_eq!(row_213_bits6_0.bit_width, 7);
    assert_eq!(row_213_bits6_0.scaling, Scaling::None);
    assert_eq!(row_213_bits6_0.category, Some("Reserved"));
    assert_eq!(row_213_bits6_0.read_effects, &[ReadEffect::None]);

    // Functional names must not be active at 4.0
    for later in [
        "attenuation_at_53p1ghz",
        "mci_flow_control_duration_encoding",
        "mci_flow_control_duration",
    ] {
        assert!(
            !active_at_208_213.iter().any(|r| r.name == later),
            "{later} must not be active at 4.0"
        );
    }
}

#[test]
fn cmis53_retains_scaled_decibel_and_functional_names() {
    let rev = Rev::CMIS_5_3;
    let active_at_208_213: Vec<_> = CATALOG
        .rows
        .iter()
        .filter(|r| {
            r.page_label == "00h"
                && (r.offset == 208 || r.offset == 213)
                && r.since.is_none_or(|s| rev >= s)
                && r.until.is_none_or(|u| rev < u)
        })
        .collect();

    assert_eq!(
        active_at_208_213.len(),
        3,
        "CMIS 5.3 must have exactly 3 active rows at 208/213"
    );

    let row_208 = active_at_208_213
        .iter()
        .find(|r| r.name == "attenuation_at_53p1ghz")
        .expect("attenuation_at_53p1ghz must be active at 5.3");
    assert_eq!(row_208.offset, 208);
    match &row_208.scaling {
        Scaling::Scaled(s) => assert_eq!(s.unit, Unit::Decibel),
        other => panic!("expected Scaled(Decibel), got {other:?}"),
    }
    assert_eq!(row_208.category, Some("OperationalParameter"));
    assert_eq!(row_208.read_effects, &[ReadEffect::Unknown]);

    let row_213_bit7 = active_at_208_213
        .iter()
        .find(|r| r.name == "mci_flow_control_duration_encoding")
        .expect("mci_flow_control_duration_encoding must be active at 5.3");
    assert_eq!(row_213_bit7.offset, 213);
    assert_eq!(row_213_bit7.bit_offset, 7);
    assert_eq!(row_213_bit7.bit_width, 1);
    assert_eq!(row_213_bit7.read_effects, &[ReadEffect::Unknown]);

    let row_213_bits6_0 = active_at_208_213
        .iter()
        .find(|r| r.name == "mci_flow_control_duration")
        .expect("mci_flow_control_duration must be active at 5.3");
    assert_eq!(row_213_bits6_0.offset, 213);
    assert_eq!(row_213_bits6_0.bit_offset, 0);
    assert_eq!(row_213_bits6_0.bit_width, 7);
    assert_eq!(row_213_bits6_0.read_effects, &[ReadEffect::Unknown]);

    // Reserved names must not be active at 5.3
    for reserved in [
        "reserved_administrative_208_cmis4",
        "reserved_administrative_213_bit7_cmis4",
        "reserved_administrative_213_bits6_0_cmis4",
    ] {
        assert!(
            !active_at_208_213.iter().any(|r| r.name == reserved),
            "{reserved} must not be active at 5.3"
        );
    }
}

#[test]
fn byte_213_setters_preserve_neighbouring_bits() {
    use openxcvr_cmis::generated::v4_0::CmisPage00hAdministrative as Page00V4;
    use openxcvr_cmis::generated::v5_3::CmisPage00hAdministrative as Page00V53;

    for val in 0..=255u8 {
        // v4_0 setters
        let mut buf_v4_bit7 = [0xa5u8; 128];
        Page00V4::set_reserved_administrative_213_bit7_cmis4(&mut buf_v4_bit7, val & 0x80 != 0);
        assert_eq!(
            buf_v4_bit7[213 - 128] & 0x7f,
            0xa5 & 0x7f,
            "v4 bit 7 setter must preserve bits 6:0"
        );

        let mut buf_v4_bits6_0 = [0xa5u8; 128];
        Page00V4::set_reserved_administrative_213_bits6_0_cmis4(&mut buf_v4_bits6_0, val);
        assert_eq!(
            buf_v4_bits6_0[213 - 128] & 0x80,
            0xa5 & 0x80,
            "v4 bits 6:0 setter must preserve bit 7"
        );

        // v5_3 setters
        let mut buf_v5_bit7 = [0xa5u8; 128];
        Page00V53::set_mci_flow_control_duration_encoding(&mut buf_v5_bit7, val & 0x80 != 0);
        assert_eq!(
            buf_v5_bit7[213 - 128] & 0x7f,
            0xa5 & 0x7f,
            "v5.3 bit 7 setter must preserve bits 6:0"
        );

        let mut buf_v5_bits6_0 = [0xa5u8; 128];
        Page00V53::set_mci_flow_control_duration(&mut buf_v5_bits6_0, val);
        assert_eq!(
            buf_v5_bits6_0[213 - 128] & 0x80,
            0xa5 & 0x80,
            "v5.3 bits 6:0 setter must preserve bit 7"
        );
    }
}
