//! Property suite for the generated page surface (brainstorm: prove the
//! decoders never panic on hostile input). Mechanically derived from
//! `crates/openxcvr-sff8472/src/generated/` — see `tests/README.md` for the regeneration recipe.
//!
//! Properties per generated page decoder family:
//! * `decode` accepts arbitrary bytes of the declared page size without
//!   panicking, and is deterministic (identical debug rendering).
//! * fixed-size mutators accept any value (derived from an arbitrary seed)
//!   without panicking, and the page still decodes afterwards.
//! * indexed setters accept in-range lane indexes, refuse every out-of-range
//!   index with `SpanError::InvalidIndex`, and leave the buffer untouched on
//!   refusal.
//! * `decode_*_supplied` returns `Ok` for every field when the full page is
//!   supplied at its base offset, and never panics for arbitrary
//!   (bytes, offset[, index]) input.

#![allow(non_snake_case)]

use proptest::prelude::*;

/// Bounded case count so `cargo test` stays reasonable: every family runs
/// these properties on a handful of arbitrary inputs, not a long campaign.
fn cfg() -> proptest::test_runner::Config {
    proptest::test_runner::Config::with_cases(24)
}

fn arr<const K: usize>(s: &[u8], k: usize) -> [u8; K] {
    std::array::from_fn(|j| s[(k + j) % s.len()])
}

use openxcvr_core::supplied::SpanError;
mod p_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::union::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_page_a2h_00h_user_eeprom {
    //! Family `PageA2h00hUserEeprom` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::PageA2h00hUserEeprom;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_page_a2h_01h_user_eeprom {
    //! Family `PageA2h01hUserEeprom` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::PageA2h01hUserEeprom;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_page_a2h_02h_tunability_rdt_rpm {
    //! Family `PageA2h02hTunabilityRdtRpm` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::union::PageA2h02hTunabilityRdtRpm;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8690_channel_tuning_controls(&mut buf, arr::<28>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_page_a2h_03h_high_accuracy_timing {
    //! Family `PageA2h03hHighAccuracyTiming` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::union::PageA2h03hHighAccuracyTiming;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::union::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v10_2_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v10_2::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v10_2_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v10_2::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v10_4_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v10_4::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v10_4_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v10_4::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v11_0_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_0::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v11_0_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_0::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v11_3_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_3::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v11_3_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_3::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v11_4_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_4::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v11_4_page_a2h_02h_tunability_rdt_rpm {
    //! Family `PageA2h02hTunabilityRdtRpm` (256 B page, base offset 0).
    //! At this revision the page decodes only `rdt_value` (A2h byte 131,
    //! `since: "11.4"`); the SFF-8690/RPM surface is 12.5-only.
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_4::PageA2h02hTunabilityRdtRpm;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v11_4_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v11_4::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v12_3_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_3::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v12_3_page_a2h_02h_tunability_rdt_rpm {
    //! Family `PageA2h02hTunabilityRdtRpm` (256 B page, base offset 0).
    //! At this revision the page decodes only `rdt_value` (A2h byte 131,
    //! `since: "11.4"`); the SFF-8690/RPM surface is 12.5-only.
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_3::PageA2h02hTunabilityRdtRpm;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v12_3_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_3::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v12_4_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_4::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v12_4_page_a2h_02h_tunability_rdt_rpm {
    //! Family `PageA2h02hTunabilityRdtRpm` (256 B page, base offset 0).
    //! At this revision the page decodes only `rdt_value` (A2h byte 131,
    //! `since: "11.4"`); the SFF-8690/RPM surface is 12.5-only.
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_4::PageA2h02hTunabilityRdtRpm;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v12_4_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_4::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v12_5_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_5::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v12_5_page_a2h_02h_tunability_rdt_rpm {
    //! Family `PageA2h02hTunabilityRdtRpm` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_5::PageA2h02hTunabilityRdtRpm;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8690_channel_tuning_controls(&mut buf, arr::<28>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v12_5_page_a2h_03h_high_accuracy_timing {
    //! Family `PageA2h03hHighAccuracyTiming` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_5::PageA2h03hHighAccuracyTiming;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v12_5_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v12_5::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v9_3_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v9_3::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v9_3_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v9_3::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

mod p_v9_5_page_a0h_base_memory {
    //! Family `PageA0hBaseMemory` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v9_5::PageA0hBaseMemory;
    const SIZE: usize = 256;
    const BASE: usize = 0;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        #[test]
        fn fixed_mutators_accept_any_values(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8472::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_extended_identifier(&mut buf, openxcvr_sff8472::generated::Sff8472ExtendedIdentifier::from(s[1 % s.len()]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_supplied(&buf, BASE).is_ok(), "decode_date_code_supplied");
            let mut ok_idx = 0;
            for idx in 0..=10i64 {
                match Page::decode_options_supplied(&buf, BASE, idx) {
                    Ok(_) => ok_idx += 1,
                    Err(e) => prop_assert!(matches!(e, SpanError::InvalidIndex(_)), "decode_options_supplied"),
                }
            }
            prop_assert!(ok_idx > 0, "decode_options_supplied: no in-range index decodes on a full page");
            prop_assert!(matches!(Page::decode_options_supplied(&buf, BASE, -1), Err(SpanError::InvalidIndex(_))));
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
            index in -3i64..=3,
        ) {
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_supplied(&bytes, offset);
            let _ = Page::decode_options_supplied(&bytes, offset, index);
        }
    }
}

mod p_v9_5_page_a2h_diagnostics_base {
    //! Family `PageA2hDiagnosticsBase` (256 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8472::generated::v9_5::PageA2hDiagnosticsBase;
    const SIZE: usize = 256;
    proptest! {
        #![proptest_config(cfg())]
        #[test]
        fn decode_is_total_and_deterministic(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            let a = Page::decode(&buf);
            let b = Page::decode(&buf);
            prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}
