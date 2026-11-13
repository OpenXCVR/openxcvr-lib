//! Property suite for the generated page surface (brainstorm: prove the
//! decoders never panic on hostile input). Mechanically derived from
//! `crates/openxcvr-sff8636/src/generated/` — see `tests/README.md` for the regeneration recipe.
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

mod p_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::union::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_pcie_assigned(&mut buf, arr::<2>(&s, 2));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[3 % s.len()], s[4 % s.len()], s[5 % s.len()], s[6 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::union::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_Sff8636Page02hUserEeprom {
    //! Family `Sff8636Page02hUserEeprom` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::Sff8636Page02hUserEeprom;
    const SIZE: usize = 128;
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
            Page::set_clei_code(&mut buf, arr::<10>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::union::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_Sff8636Page20hPam4Monitoring1 {
    //! Family `Sff8636Page20hPam4Monitoring1` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::union::Sff8636Page20hPam4Monitoring1;
    const SIZE: usize = 128;
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
            Page::set_reserved_other_config_251_255(&mut buf, arr::<5>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_Sff8636Page21hPam4Monitoring2 {
    //! Family `Sff8636Page21hPam4Monitoring2` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::union::Sff8636Page21hPam4Monitoring2;
    const SIZE: usize = 128;
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

mod p_Sff8636Page22hHighAccuracyTiming {
    //! Family `Sff8636Page22hHighAccuracyTiming` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::union::Sff8636Page22hHighAccuracyTiming;
    const SIZE: usize = 128;
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

mod p_v1_3_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_3::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[1 % s.len()], s[2 % s.len()], s[3 % s.len()], s[4 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v1_3_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_3::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v1_3_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_3::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v1_4_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_4::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[2 % s.len()], s[3 % s.len()], s[4 % s.len()], s[5 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v1_4_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_4::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v1_4_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_4::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v1_5_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_5::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[2 % s.len()], s[3 % s.len()], s[4 % s.len()], s[5 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v1_5_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_5::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v1_5_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v1_5::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_0_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_0::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[2 % s.len()], s[3 % s.len()], s[4 % s.len()], s[5 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v2_0_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_0::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v2_0_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_0::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_11_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_11::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_pcie_assigned(&mut buf, arr::<2>(&s, 2));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[3 % s.len()], s[4 % s.len()], s[5 % s.len()], s[6 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v2_11_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_11::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v2_11_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_11::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_11_Sff8636Page20hPam4Monitoring1 {
    //! Family `Sff8636Page20hPam4Monitoring1` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_11::Sff8636Page20hPam4Monitoring1;
    const SIZE: usize = 128;
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
            Page::set_reserved_other_config_251_255(&mut buf, arr::<5>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_11_Sff8636Page21hPam4Monitoring2 {
    //! Family `Sff8636Page21hPam4Monitoring2` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_11::Sff8636Page21hPam4Monitoring2;
    const SIZE: usize = 128;
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

mod p_v2_12_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_12::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_pcie_assigned(&mut buf, arr::<2>(&s, 2));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[3 % s.len()], s[4 % s.len()], s[5 % s.len()], s[6 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v2_12_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_12::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v2_12_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_12::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_12_Sff8636Page20hPam4Monitoring1 {
    //! Family `Sff8636Page20hPam4Monitoring1` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_12::Sff8636Page20hPam4Monitoring1;
    const SIZE: usize = 128;
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
            Page::set_reserved_other_config_251_255(&mut buf, arr::<5>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_12_Sff8636Page21hPam4Monitoring2 {
    //! Family `Sff8636Page21hPam4Monitoring2` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_12::Sff8636Page21hPam4Monitoring2;
    const SIZE: usize = 128;
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

mod p_v2_12_Sff8636Page22hHighAccuracyTiming {
    //! Family `Sff8636Page22hHighAccuracyTiming` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_12::Sff8636Page22hHighAccuracyTiming;
    const SIZE: usize = 128;
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

mod p_v2_5_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_5::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_pcie_assigned(&mut buf, arr::<2>(&s, 2));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[3 % s.len()], s[4 % s.len()], s[5 % s.len()], s[6 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v2_5_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_5::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v2_5_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_5::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v2_8_Sff8636LowerMemory {
    //! Family `Sff8636LowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_8::Sff8636LowerMemory;
    const SIZE: usize = 128;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_revision_compliance(&mut buf, openxcvr_sff8636::generated::Sff8636RevisionCompliance::from(s[1 % s.len()]));
            Page::set_pcie_assigned(&mut buf, arr::<2>(&s, 2));
            Page::set_password_change_entry(&mut buf, u32::from_le_bytes([s[3 % s.len()], s[4 % s.len()], s[5 % s.len()], s[6 % s.len()]]));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_flat_memory_supplied(&buf, BASE).is_ok(), "decode_flat_memory_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_flat_memory_supplied(&bytes, offset);
        }
    }
}

mod p_v2_8_Sff8636Page00hUpperAdministrative {
    //! Family `Sff8636Page00hUpperAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_8::Sff8636Page00hUpperAdministrative;
    const SIZE: usize = 128;
    const BASE: usize = 128;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_sff8636::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_connector_type(&mut buf, openxcvr_sff8636::generated::Sff8024ConnectorType::from(s[1 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 2));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 3));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 4));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 5));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 6));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_day(&mut buf, arr::<2>(&s, 9));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_sff8024_identifier_copy_supplied(&buf, BASE).is_ok(), "decode_sff8024_identifier_copy_supplied");
            prop_assert!(Page::decode_vendor_name_supplied(&buf, BASE).is_ok(), "decode_vendor_name_supplied");
            prop_assert!(Page::decode_vendor_oui_supplied(&buf, BASE).is_ok(), "decode_vendor_oui_supplied");
            prop_assert!(Page::decode_vendor_pn_supplied(&buf, BASE).is_ok(), "decode_vendor_pn_supplied");
            prop_assert!(Page::decode_vendor_rev_supplied(&buf, BASE).is_ok(), "decode_vendor_rev_supplied");
            prop_assert!(Page::decode_vendor_sn_supplied(&buf, BASE).is_ok(), "decode_vendor_sn_supplied");
            prop_assert!(Page::decode_date_code_year_supplied(&buf, BASE).is_ok(), "decode_date_code_year_supplied");
            prop_assert!(Page::decode_date_code_month_supplied(&buf, BASE).is_ok(), "decode_date_code_month_supplied");
            prop_assert!(Page::decode_date_code_lot_code_supplied(&buf, BASE).is_ok(), "decode_date_code_lot_code_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_sff8024_identifier_copy_supplied(&bytes, offset);
            let _ = Page::decode_vendor_name_supplied(&bytes, offset);
            let _ = Page::decode_vendor_oui_supplied(&bytes, offset);
            let _ = Page::decode_vendor_pn_supplied(&bytes, offset);
            let _ = Page::decode_vendor_rev_supplied(&bytes, offset);
            let _ = Page::decode_vendor_sn_supplied(&bytes, offset);
            let _ = Page::decode_date_code_year_supplied(&bytes, offset);
            let _ = Page::decode_date_code_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v2_8_Sff8636Page03hThresholdsAndControls {
    //! Family `Sff8636Page03hThresholdsAndControls` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_sff8636::generated::v2_8::Sff8636Page03hThresholdsAndControls;
    const SIZE: usize = 128;
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
            Page::set_reserved_control_byte_232(&mut buf, arr::<1>(&s, 0));
            Page::set_reserved_channel_monitor_masks_248_251(&mut buf, arr::<4>(&s, 1));
            Page::set_reserved_page03_252_255(&mut buf, arr::<4>(&s, 2));
            let _ = Page::decode(&buf);
        }
    }
}
