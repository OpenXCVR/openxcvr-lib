//! Property suite for the generated page surface (brainstorm: prove the
//! decoders never panic on hostile input). Mechanically derived from
//! `crates/openxcvr-cmis/src/generated/` — see `tests/README.md` for the regeneration recipe.
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
mod p_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_bank_broadcast_enable(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_mci_speed_configuration(&mut buf, s[7 % s.len()]);
            Page::set_module_state_changed_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_abnormal_fw_indication_mask(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[38 % s.len()], s[39 % s.len()], s[40 % s.len()], s[41 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[39 % s.len()], s[40 % s.len()], s[41 % s.len()], s[42 % s.len()]]));
            Page::set_bank_select(&mut buf, s[40 % s.len()]);
            Page::set_page_select(&mut buf, s[41 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_attenuation_at_53p1ghz(&mut buf, s[20 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[29 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[30 % s.len()]);
            Page::set_mci_flow_control_duration_encoding(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_mci_flow_control_duration(&mut buf, s[32 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 33));
            Page::set_page_checksum(&mut buf, s[34 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 35));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_length_multiplier_smf2(&mut buf, s[10 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_network_path_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_vdm_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[19 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[20 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[21 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[22 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[23 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_e_pps_supported(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_timing_page15h_supported(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[30 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[31 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[32 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[33 % s.len()], s[34 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[34 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[36 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[40 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[41 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[42 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[45 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[46 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[47 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[48 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[49 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[55 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[56 % s.len()]);
            Page::set_bank_broadcast_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[72 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[73 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[76 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[77 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[81 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[82 % s.len()] & 1 == 1);
            Page::set_versatile_control_set_supported(&mut buf, s[83 % s.len()] & 1 == 1);
            Page::set_unidir_reconfig_supported(&mut buf, s[84 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[85 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[86 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[87 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[88 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[89 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[90 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[91 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[92 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[93 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[94 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[95 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[96 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[97 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[98 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[99 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[100 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[101 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[102 % s.len()]);
            Page::set_max_duration_bpc(&mut buf, s[103 % s.len()]);
            Page::set_page_0ch_supported(&mut buf, s[104 % s.len()] & 1 == 1);
            Page::set_page_0dh_supported(&mut buf, s[105 % s.len()] & 1 == 1);
            Page::set_page_60h_supported(&mut buf, s[106 % s.len()] & 1 == 1);
            Page::set_page_61h_supported(&mut buf, s[107 % s.len()] & 1 == 1);
            Page::set_page_62h_supported(&mut buf, s[108 % s.len()] & 1 == 1);
            Page::set_extra_lane_banks_supported(&mut buf, s[109 % s.len()]);
            Page::set_nad_banks_supported(&mut buf, s[110 % s.len()]);
            Page::set_scratch_pad_supported(&mut buf, s[111 % s.len()]);
            Page::set_password_entry_supported(&mut buf, s[112 % s.len()]);
            Page::set_password_entry_result_supported(&mut buf, s[113 % s.len()]);
            Page::set_full_page_read_supported(&mut buf, s[114 % s.len()]);
            Page::set_host_lane_switching_supported(&mut buf, s[115 % s.len()] & 1 == 1);
            Page::set_link_training_supported(&mut buf, s[116 % s.len()] & 1 == 1);
            Page::set_media_lane_switching_supported(&mut buf, s[117 % s.len()] & 1 == 1);
            Page::set_page_checksum(&mut buf, s[118 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage02hThresholds {
    //! Family `CmisPage02hThresholds` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPage02hThresholds;
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
            Page::set_page_checksum(&mut buf, s[0 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_temp_mon_threshold(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_vcc_mon_threshold(&mut buf, idx, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_aux1_mon_threshold(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_aux2_mon_threshold(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_aux3_mon_threshold(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_custom_mon_threshold(&mut buf, idx, u16::from_le_bytes([s[5 % s.len()], s[6 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_tx_threshold(&mut buf, idx, u16::from_le_bytes([s[6 % s.len()], s[7 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_laser_bias_current_threshold(&mut buf, idx, u16::from_le_bytes([s[7 % s.len()], s[8 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_rx_threshold(&mut buf, idx, u16::from_le_bytes([s[8 % s.len()], s[9 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage03hUserEeprom {
    //! Family `CmisPage03hUserEeprom` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPage03hUserEeprom;
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
            Page::set_user_data(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_grid_supported_300_ghz(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_grid_low_channel_300_ghz(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_grid_high_channel_300_ghz(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[13 % s.len()], s[14 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_output_power_tx_relative_thresholds_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[18 % s.len()], s[19 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[19 % s.len()], s[20 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[20 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage05hFormFactorSignals {
    //! Family `CmisPage05hFormFactorSignals` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPage05hFormFactorSignals;
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
            Page::set_reserved_dual_mode_pin_control_bit_7(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_select_tx_disable(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_select_rx_losl(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_select_tx_fault_dd(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_select_tx_disable_q(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_select_rx_losl_q(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_hw_rx_los_module_wide(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_hw_tx_fault_mode(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_mask_hw_tx_disable(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_mask_hw_tx_disable_dd(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_tx_disable_modes(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_custom_cmis_ff_245_254(&mut buf, arr::<10>(&s, 11));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage0ChModuleManagement {
    //! Family `CmisPage0ChModuleManagement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage0ChModuleManagement;
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

mod p_CmisPage0DhFirmwareManagement {
    //! Family `CmisPage0DhFirmwareManagement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage0DhFirmwareManagement;
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
            Page::set_reserved_page_0dh_132_135(&mut buf, arr::<4>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage10hLaneControl {
    //! Family `CmisPage10hLaneControl` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPage10hLaneControl;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_dp_deinit_lane(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_input_polarity_flip_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_disable_tx(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_auto_squelch_disable_tx(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_squelch_force_tx(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_adaptive_input_eq_freeze_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_adaptive_input_eq_store_tx(&mut buf, idx, s[6 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_polarity_flip_rx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_disable_rx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_auto_squelch_disable_rx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_dp_init_lane(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_immediate_lane(&mut buf, idx, s[11 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_app_sel_code_lane(&mut buf, idx, s[12 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_data_path_id_lane(&mut buf, idx, s[13 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_explicit_control_lane(&mut buf, idx, s[14 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_adaptive_input_eq_enable_tx(&mut buf, idx, s[15 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_adaptive_input_eq_recall_tx(&mut buf, idx, s[16 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_host_controlled_input_eq_target_tx(&mut buf, idx, s[17 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_cdr_enable_tx(&mut buf, idx, s[18 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_cdr_enable_rx(&mut buf, idx, s[19 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_output_eq_pre_cursor_target_rx(&mut buf, idx, s[20 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_output_eq_post_cursor_target_rx(&mut buf, idx, s[21 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_output_amplitude_target_rx(&mut buf, idx, s[22 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_immediate_tx(&mut buf, idx, s[23 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_immediate_rx(&mut buf, idx, s[24 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_dp_init_lane(&mut buf, idx, s[25 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_immediate_lane(&mut buf, idx, s[26 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_app_sel_code_lane(&mut buf, idx, s[27 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_data_path_id_lane(&mut buf, idx, s[28 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_explicit_control_lane(&mut buf, idx, s[29 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_adaptive_input_eq_enable_tx(&mut buf, idx, s[30 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_adaptive_input_eq_recall_tx(&mut buf, idx, s[31 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_host_controlled_input_eq_target_tx(&mut buf, idx, s[32 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_cdr_enable_tx(&mut buf, idx, s[33 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_cdr_enable_rx(&mut buf, idx, s[34 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_output_eq_pre_cursor_target_rx(&mut buf, idx, s[35 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_output_eq_post_cursor_target_rx(&mut buf, idx, s[36 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_output_amplitude_target_rx(&mut buf, idx, s[37 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_immediate_tx(&mut buf, idx, s[38 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_immediate_rx(&mut buf, idx, s[39 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_dp_state_changed_mask(&mut buf, idx, s[40 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_failure_mask_tx(&mut buf, idx, s[41 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_los_mask_tx(&mut buf, idx, s[42 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_cdr_lol_mask_tx(&mut buf, idx, s[43 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_adaptive_input_eq_fail_mask_tx(&mut buf, idx, s[44 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_high_alarm_mask_tx(&mut buf, idx, s[45 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_low_alarm_mask_tx(&mut buf, idx, s[46 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_high_warning_mask_tx(&mut buf, idx, s[47 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_low_warning_mask_tx(&mut buf, idx, s[48 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_laser_bias_high_alarm_mask_tx(&mut buf, idx, s[49 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_laser_bias_low_alarm_mask_tx(&mut buf, idx, s[50 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_laser_bias_high_warning_mask_tx(&mut buf, idx, s[51 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_laser_bias_low_warning_mask_tx(&mut buf, idx, s[52 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_los_mask_rx(&mut buf, idx, s[53 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_cdr_lol_mask_rx(&mut buf, idx, s[54 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_high_alarm_mask_rx(&mut buf, idx, s[55 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_low_alarm_mask_rx(&mut buf, idx, s[56 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_high_warning_mask_rx(&mut buf, idx, s[57 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_optical_power_low_warning_mask_rx(&mut buf, idx, s[58 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_status_changed_mask_rx(&mut buf, idx, s[59 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage11hLaneStatus;
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

mod p_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage12hTunableLaser;
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
            Page::set_output_power_tx_hi_alarm_rel_thr(&mut buf, s[0 % s.len()]);
            Page::set_output_power_tx_hi_warn_rel_thr(&mut buf, s[1 % s.len()]);
            Page::set_output_power_tx_lo_alarm_rel_thr(&mut buf, s[2 % s.len()]);
            Page::set_output_power_tx_lo_warn_rel_thr(&mut buf, s[3 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_relative_output_power_thresholds_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[5 % s.len()], s[6 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[11 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage13hDiagnosticsControl {
    //! Family `CmisPage13hDiagnosticsControl` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPage13hDiagnosticsControl;
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
            Page::set_host_prbs_generator_clock_source(&mut buf, s[0 % s.len()]);
            Page::set_media_prbs_generator_clock_source(&mut buf, s[1 % s.len()]);
            Page::set_start_stop_is_global(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_reset_error_information(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_auto_restart_gating(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_measurement_time(&mut buf, s[5 % s.len()]);
            Page::set_update_period_select(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_host_prbs_checker_clock_source(&mut buf, s[7 % s.len()]);
            Page::set_media_prbs_checker_clock_source(&mut buf, s[8 % s.len()]);
            Page::set_custom_diagnostics_control(&mut buf, arr::<10>(&s, 9));
            Page::set_loss_of_reference_clock_mask(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_user_pattern(&mut buf, arr::<32>(&s, 11));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_side_generator_enable(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_generator_data_invert(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_generator_swap_symbol_bits(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_generator_pre_fec_enable(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_generator_pattern_select(&mut buf, idx, openxcvr_cmis::generated::CmisDiagnosticsPatternId::from(s[4 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_generator_enable(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_generator_data_invert(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_generator_swap_symbol_bits(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_generator_pre_fec_enable(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_generator_pattern_select(&mut buf, idx, openxcvr_cmis::generated::CmisDiagnosticsPatternId::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_checker_enable(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_checker_data_invert(&mut buf, idx, s[11 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_checker_swap_symbol_bits(&mut buf, idx, s[12 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_checker_post_fec_enable(&mut buf, idx, s[13 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_checker_pattern_select(&mut buf, idx, openxcvr_cmis::generated::CmisDiagnosticsPatternId::from(s[14 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_checker_enable(&mut buf, idx, s[15 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_checker_data_invert(&mut buf, idx, s[16 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_checker_swap_symbol_bits(&mut buf, idx, s[17 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_checker_post_fec_enable(&mut buf, idx, s[18 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_checker_pattern_select(&mut buf, idx, openxcvr_cmis::generated::CmisDiagnosticsPatternId::from(s[19 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_output_loopback_enable(&mut buf, idx, s[20 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_side_input_loopback_enable(&mut buf, idx, s[21 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_output_loopback_enable(&mut buf, idx, s[22 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_side_input_loopback_enable(&mut buf, idx, s[23 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_scratchpad(&mut buf, idx, s[24 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_pattern_check_gating_complete_mask_host(&mut buf, idx, s[25 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_pattern_check_gating_complete_mask_media(&mut buf, idx, s[26 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_pattern_generator_lol_mask_host(&mut buf, idx, s[27 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_pattern_generator_lol_mask_media(&mut buf, idx, s[28 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_pattern_checker_lol_mask_host(&mut buf, idx, s[29 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_pattern_checker_lol_mask_media(&mut buf, idx, s[30 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage15hTiming {
    //! Family `CmisPage15hTiming` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPage15hTiming;
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

mod p_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage19hStatusExtension;
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

mod p_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_interface_gid_nad(&mut buf, idx, s[4 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_interface_gid_nad(&mut buf, idx, s[5 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage20hVdmDescriptors1To64;
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

mod p_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage21hVdmDescriptors65To128;
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

mod p_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage22hVdmDescriptors129To192;
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

mod p_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage23hVdmDescriptors193To256;
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

mod p_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage28hVdmThresholds1To16;
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

mod p_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage29hVdmThresholds17To32;
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

mod p_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage2AhVdmThresholds33To48;
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

mod p_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage2BhVdmThresholds49To64;
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

mod p_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage2ChVdmFlags;
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

mod p_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_power_saving_mode(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_monitoring_duty_cycle(&mut buf, s[2 % s.len()]);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage60hLaneDatapathManagementExtensions {
    //! Family `CmisPage60hLaneDatapathManagementExtensions` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage60hLaneDatapathManagementExtensions;
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

mod p_CmisPage61hLaneDatapathMonitoring {
    //! Family `CmisPage61hLaneDatapathMonitoring` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage61hLaneDatapathMonitoring;
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

mod p_CmisPage62hLaneSupervisionThresholds {
    //! Family `CmisPage62hLaneSupervisionThresholds` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage62hLaneSupervisionThresholds;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_output_power_threshold_hi_alarm_tx(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_power_threshold_lo_alarm_tx(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_power_threshold_hi_warning_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_power_threshold_lo_warning_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage6DhMediaLaneSwitching {
    //! Family `CmisPage6DhMediaLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage6DhMediaLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_media_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_media_lane_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_media_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::union::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA0hCdbEplSegment1 {
    //! Family `CmisPageA0hCdbEplSegment1` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA0hCdbEplSegment1;
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
            Page::set_epl_segment_1(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA1hCdbEplSegment2 {
    //! Family `CmisPageA1hCdbEplSegment2` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA1hCdbEplSegment2;
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
            Page::set_epl_segment_2(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA2hCdbEplSegment3 {
    //! Family `CmisPageA2hCdbEplSegment3` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA2hCdbEplSegment3;
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
            Page::set_epl_segment_3(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA3hCdbEplSegment4 {
    //! Family `CmisPageA3hCdbEplSegment4` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA3hCdbEplSegment4;
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
            Page::set_epl_segment_4(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA4hCdbEplSegment5 {
    //! Family `CmisPageA4hCdbEplSegment5` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA4hCdbEplSegment5;
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
            Page::set_epl_segment_5(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA5hCdbEplSegment6 {
    //! Family `CmisPageA5hCdbEplSegment6` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA5hCdbEplSegment6;
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
            Page::set_epl_segment_6(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA6hCdbEplSegment7 {
    //! Family `CmisPageA6hCdbEplSegment7` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA6hCdbEplSegment7;
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
            Page::set_epl_segment_7(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA7hCdbEplSegment8 {
    //! Family `CmisPageA7hCdbEplSegment8` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA7hCdbEplSegment8;
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
            Page::set_epl_segment_8(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA8hCdbEplSegment9 {
    //! Family `CmisPageA8hCdbEplSegment9` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA8hCdbEplSegment9;
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
            Page::set_epl_segment_9(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageA9hCdbEplSegment10 {
    //! Family `CmisPageA9hCdbEplSegment10` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageA9hCdbEplSegment10;
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
            Page::set_epl_segment_10(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageAAhCdbEplSegment11 {
    //! Family `CmisPageAAhCdbEplSegment11` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageAAhCdbEplSegment11;
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
            Page::set_epl_segment_11(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageABhCdbEplSegment12 {
    //! Family `CmisPageABhCdbEplSegment12` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageABhCdbEplSegment12;
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
            Page::set_epl_segment_12(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageAChCdbEplSegment13 {
    //! Family `CmisPageAChCdbEplSegment13` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageAChCdbEplSegment13;
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
            Page::set_epl_segment_13(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageADhCdbEplSegment14 {
    //! Family `CmisPageADhCdbEplSegment14` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageADhCdbEplSegment14;
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
            Page::set_epl_segment_14(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageAEhCdbEplSegment15 {
    //! Family `CmisPageAEhCdbEplSegment15` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageAEhCdbEplSegment15;
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
            Page::set_epl_segment_15(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_CmisPageAFhCdbEplSegment16 {
    //! Family `CmisPageAFhCdbEplSegment16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::CmisPageAFhCdbEplSegment16;
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
            Page::set_epl_segment_16(&mut buf, arr::<128>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_module_state_changed_mask(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[32 % s.len()], s[33 % s.len()], s[34 % s.len()], s[35 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[33 % s.len()], s[34 % s.len()], s[35 % s.len()], s[36 % s.len()]]));
            Page::set_bank_select(&mut buf, s[34 % s.len()]);
            Page::set_page_select(&mut buf, s[35 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v3_0_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[28 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[29 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 30));
            Page::set_page_checksum(&mut buf, s[31 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 32));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v3_0_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage01hAdvertising;
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
            Page::set_module_hardware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[2 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[3 % s.len()]);
            Page::set_length_om5(&mut buf, s[4 % s.len()]);
            Page::set_length_om4(&mut buf, s[5 % s.len()]);
            Page::set_length_om3(&mut buf, s[6 % s.len()]);
            Page::set_length_om2(&mut buf, s[7 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[8 % s.len()], s[9 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[9 % s.len()], s[10 % s.len()]]));
            Page::set_diagnostic_pages_supported(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[14 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[15 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[16 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[17 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[18 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[23 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[24 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[25 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[26 % s.len()], s[27 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[27 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[29 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[35 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[40 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[41 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[42 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[45 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[46 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[47 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[48 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[49 % s.len()]);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[55 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[56 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[65 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[69 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[72 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[73 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[76 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[77 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_page_checksum(&mut buf, s[80 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[16 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage11hLaneStatus;
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

mod p_v3_0_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage12hTunableLaser;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage19hStatusExtension;
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

mod p_v3_0_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v3_0_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v3_0::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_module_state_changed_mask(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[35 % s.len()], s[36 % s.len()], s[37 % s.len()], s[38 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[36 % s.len()], s[37 % s.len()], s[38 % s.len()], s[39 % s.len()]]));
            Page::set_bank_select(&mut buf, s[37 % s.len()]);
            Page::set_page_select(&mut buf, s[38 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v4_0_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_reserved_administrative_208_cmis4(&mut buf, s[20 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[29 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[30 % s.len()]);
            Page::set_reserved_administrative_213_bit7_cmis4(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_reserved_administrative_213_bits6_0_cmis4(&mut buf, s[32 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 33));
            Page::set_page_checksum(&mut buf, s[34 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 35));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v4_0_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_vdm_pages_supported(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[17 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[18 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[19 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[20 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[21 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[26 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[27 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[28 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[29 % s.len()], s[30 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[30 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[32 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[38 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[40 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[41 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[42 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[43 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[44 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[45 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[46 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[47 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[48 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[49 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[52 % s.len()]);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[55 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[56 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[68 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[72 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[73 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[76 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[77 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[79 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[81 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[82 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[83 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[84 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[85 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[86 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[87 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[88 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[89 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[90 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[91 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[92 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[93 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[94 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[95 % s.len()]);
            Page::set_page_checksum(&mut buf, s[96 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[16 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage11hLaneStatus;
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

mod p_v4_0_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage12hTunableLaser;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage19hStatusExtension;
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

mod p_v4_0_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage20hVdmDescriptors1To64;
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

mod p_v4_0_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage21hVdmDescriptors65To128;
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

mod p_v4_0_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage22hVdmDescriptors129To192;
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

mod p_v4_0_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage23hVdmDescriptors193To256;
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

mod p_v4_0_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage28hVdmThresholds1To16;
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

mod p_v4_0_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage29hVdmThresholds17To32;
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

mod p_v4_0_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage2AhVdmThresholds33To48;
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

mod p_v4_0_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage2BhVdmThresholds49To64;
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

mod p_v4_0_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage2ChVdmFlags;
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

mod p_v4_0_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v4_0_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v4_0::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_module_state_changed_mask(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[35 % s.len()], s[36 % s.len()], s[37 % s.len()], s[38 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[36 % s.len()], s[37 % s.len()], s[38 % s.len()], s[39 % s.len()]]));
            Page::set_bank_select(&mut buf, s[37 % s.len()]);
            Page::set_page_select(&mut buf, s[38 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v5_0_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[28 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[29 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 30));
            Page::set_page_checksum(&mut buf, s[31 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 32));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v5_0_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_vdm_pages_supported(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[17 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[18 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[19 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[20 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[21 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_e_pps_supported(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_timing_page15h_supported(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[28 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[29 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[30 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[31 % s.len()], s[32 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[32 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[34 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[40 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[41 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[42 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[45 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[46 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[47 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[48 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[49 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[54 % s.len()]);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[55 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[56 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[70 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[72 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[73 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[74 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[76 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[77 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[81 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[82 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[83 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[84 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[85 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[86 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[87 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[88 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[89 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[90 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[91 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[92 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[93 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[94 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[95 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[96 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[97 % s.len()]);
            Page::set_page_checksum(&mut buf, s[98 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[16 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage11hLaneStatus;
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

mod p_v5_0_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage12hTunableLaser;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage19hStatusExtension;
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

mod p_v5_0_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage20hVdmDescriptors1To64;
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

mod p_v5_0_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage21hVdmDescriptors65To128;
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

mod p_v5_0_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage22hVdmDescriptors129To192;
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

mod p_v5_0_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage23hVdmDescriptors193To256;
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

mod p_v5_0_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage28hVdmThresholds1To16;
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

mod p_v5_0_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage29hVdmThresholds17To32;
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

mod p_v5_0_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage2AhVdmThresholds33To48;
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

mod p_v5_0_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage2BhVdmThresholds49To64;
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

mod p_v5_0_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage2ChVdmFlags;
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

mod p_v5_0_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_0_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_0::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_module_state_changed_mask(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[35 % s.len()], s[36 % s.len()], s[37 % s.len()], s[38 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[36 % s.len()], s[37 % s.len()], s[38 % s.len()], s[39 % s.len()]]));
            Page::set_bank_select(&mut buf, s[37 % s.len()]);
            Page::set_page_select(&mut buf, s[38 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v5_1_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[28 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[29 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 30));
            Page::set_page_checksum(&mut buf, s[31 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 32));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v5_1_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_network_path_pages_supported(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_vdm_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[18 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[19 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[20 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[21 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[22 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_e_pps_supported(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_timing_page15h_supported(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[29 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[30 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[31 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[32 % s.len()], s[33 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[33 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[35 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[40 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[41 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[42 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[45 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[46 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[47 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[48 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[49 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[55 % s.len()]);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[56 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[71 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[72 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[73 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[75 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[76 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[77 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[81 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[82 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[83 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[84 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[85 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[86 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[87 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[88 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[89 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[90 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[91 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[92 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[93 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[94 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[95 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[96 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[97 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[98 % s.len()]);
            Page::set_page_checksum(&mut buf, s[99 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[16 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage11hLaneStatus;
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

mod p_v5_1_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage12hTunableLaser;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage19hStatusExtension;
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

mod p_v5_1_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage20hVdmDescriptors1To64;
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

mod p_v5_1_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage21hVdmDescriptors65To128;
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

mod p_v5_1_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage22hVdmDescriptors129To192;
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

mod p_v5_1_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage23hVdmDescriptors193To256;
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

mod p_v5_1_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage28hVdmThresholds1To16;
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

mod p_v5_1_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage29hVdmThresholds17To32;
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

mod p_v5_1_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage2AhVdmThresholds33To48;
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

mod p_v5_1_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage2BhVdmThresholds49To64;
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

mod p_v5_1_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage2ChVdmFlags;
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

mod p_v5_1_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_1_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_1::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_bank_broadcast_enable(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_module_state_changed_mask(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[36 % s.len()], s[37 % s.len()], s[38 % s.len()], s[39 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[37 % s.len()], s[38 % s.len()], s[39 % s.len()], s[40 % s.len()]]));
            Page::set_bank_select(&mut buf, s[38 % s.len()]);
            Page::set_page_select(&mut buf, s[39 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v5_2_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[28 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[29 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 30));
            Page::set_page_checksum(&mut buf, s[31 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 32));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v5_2_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_network_path_pages_supported(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_vdm_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[18 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[19 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[20 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[21 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[22 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_e_pps_supported(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_timing_page15h_supported(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[29 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[30 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[31 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[32 % s.len()], s[33 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[33 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[35 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[40 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[41 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[42 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[45 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[46 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[47 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[48 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[49 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[55 % s.len()]);
            Page::set_bank_broadcast_supported(&mut buf, s[56 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[72 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[73 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[76 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[77 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[81 % s.len()] & 1 == 1);
            Page::set_unidir_reconfig_supported(&mut buf, s[82 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[83 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[84 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[85 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[86 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[87 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[88 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[89 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[90 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[91 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[92 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[93 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[94 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[95 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[96 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[97 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[98 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[99 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[100 % s.len()]);
            Page::set_page_checksum(&mut buf, s[101 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[16 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage11hLaneStatus;
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

mod p_v5_2_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage12hTunableLaser;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage19hStatusExtension;
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

mod p_v5_2_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage20hVdmDescriptors1To64;
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

mod p_v5_2_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage21hVdmDescriptors65To128;
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

mod p_v5_2_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage22hVdmDescriptors129To192;
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

mod p_v5_2_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage23hVdmDescriptors193To256;
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

mod p_v5_2_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage28hVdmThresholds1To16;
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

mod p_v5_2_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage29hVdmThresholds17To32;
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

mod p_v5_2_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage2AhVdmThresholds33To48;
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

mod p_v5_2_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage2BhVdmThresholds49To64;
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

mod p_v5_2_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage2ChVdmFlags;
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

mod p_v5_2_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_2_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_2::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_bank_broadcast_enable(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_mci_speed_configuration(&mut buf, s[7 % s.len()]);
            Page::set_module_state_changed_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[37 % s.len()], s[38 % s.len()], s[39 % s.len()], s[40 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[38 % s.len()], s[39 % s.len()], s[40 % s.len()], s[41 % s.len()]]));
            Page::set_bank_select(&mut buf, s[39 % s.len()]);
            Page::set_page_select(&mut buf, s[40 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v5_3_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_attenuation_at_53p1ghz(&mut buf, s[20 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[29 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[30 % s.len()]);
            Page::set_mci_flow_control_duration_encoding(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_mci_flow_control_duration(&mut buf, s[32 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 33));
            Page::set_page_checksum(&mut buf, s[34 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 35));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v5_3_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_length_multiplier_smf2(&mut buf, s[10 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_network_path_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_vdm_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[19 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[20 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[21 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[22 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[23 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_e_pps_supported(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_timing_page15h_supported(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[30 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[31 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[32 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[33 % s.len()], s[34 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[34 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[36 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[40 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[41 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[42 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[45 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[46 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[47 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[48 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[49 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[55 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[56 % s.len()]);
            Page::set_bank_broadcast_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[72 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[73 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[76 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[77 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[81 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[82 % s.len()] & 1 == 1);
            Page::set_versatile_control_set_supported(&mut buf, s[83 % s.len()] & 1 == 1);
            Page::set_unidir_reconfig_supported(&mut buf, s[84 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[85 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[86 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[87 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[88 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[89 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[90 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[91 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[92 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[93 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[94 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[95 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[96 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[97 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[98 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[99 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[100 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[101 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[102 % s.len()]);
            Page::set_max_duration_bpc(&mut buf, s[103 % s.len()]);
            Page::set_nad_banks_supported(&mut buf, s[104 % s.len()]);
            Page::set_scratch_pad_supported(&mut buf, s[105 % s.len()]);
            Page::set_password_entry_supported(&mut buf, s[106 % s.len()]);
            Page::set_password_entry_result_supported(&mut buf, s[107 % s.len()]);
            Page::set_full_page_read_supported(&mut buf, s[108 % s.len()]);
            Page::set_host_lane_switching_supported(&mut buf, s[109 % s.len()] & 1 == 1);
            Page::set_link_training_supported(&mut buf, s[110 % s.len()] & 1 == 1);
            Page::set_page_checksum(&mut buf, s[111 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[10 % s.len()], s[11 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[16 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage11hLaneStatus;
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

mod p_v5_3_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage12hTunableLaser;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage19hStatusExtension;
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

mod p_v5_3_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage20hVdmDescriptors1To64;
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

mod p_v5_3_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage21hVdmDescriptors65To128;
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

mod p_v5_3_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage22hVdmDescriptors129To192;
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

mod p_v5_3_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage23hVdmDescriptors193To256;
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

mod p_v5_3_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage28hVdmThresholds1To16;
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

mod p_v5_3_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage29hVdmThresholds17To32;
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

mod p_v5_3_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage2AhVdmThresholds33To48;
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

mod p_v5_3_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage2BhVdmThresholds49To64;
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

mod p_v5_3_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage2ChVdmFlags;
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

mod p_v5_3_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_power_saving_mode(&mut buf, s[1 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_3_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_3::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisLowerMemory {
    //! Family `CmisLowerMemory` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisLowerMemory;
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
            Page::set_sff8024_identifier(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_cmis_revision(&mut buf, s[1 % s.len()]);
            Page::set_software_reset(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_low_pwr_request_sw(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_squelch_method_select(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_low_pwr_allow_request_hw(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_bank_broadcast_enable(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_mci_speed_configuration(&mut buf, s[7 % s.len()]);
            Page::set_module_state_changed_mask(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_module_firmware_error_mask(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_data_path_firmware_error_mask(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_abnormal_fw_indication_mask(&mut buf, s[11 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_1(&mut buf, s[12 % s.len()] & 1 == 1);
            Page::set_cdb_cmd_complete_mask_2(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_alarm_mask(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_alarm_mask(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_temp_mon_high_warning_mask(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_temp_mon_low_warning_mask(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_alarm_mask(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_alarm_mask(&mut buf, s[19 % s.len()] & 1 == 1);
            Page::set_vcc_mon_high_warning_mask(&mut buf, s[20 % s.len()] & 1 == 1);
            Page::set_vcc_mon_low_warning_mask(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_alarm_mask(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_alarm_mask(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_aux1_mon_high_warning_mask(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_aux1_mon_low_warning_mask(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_alarm_mask(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_alarm_mask(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux2_mon_high_warning_mask(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux2_mon_low_warning_mask(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_alarm_mask(&mut buf, s[30 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_alarm_mask(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_aux3_mon_high_warning_mask(&mut buf, s[32 % s.len()] & 1 == 1);
            Page::set_aux3_mon_low_warning_mask(&mut buf, s[33 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_alarm_mask(&mut buf, s[34 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_alarm_mask(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_custom_mon_high_warning_mask(&mut buf, s[36 % s.len()] & 1 == 1);
            Page::set_custom_mon_low_warning_mask(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_password_change_entry_area(&mut buf, u32::from_le_bytes([s[38 % s.len()], s[39 % s.len()], s[40 % s.len()], s[41 % s.len()]]));
            Page::set_password_entry_area(&mut buf, u32::from_le_bytes([s[39 % s.len()], s[40 % s.len()], s[41 % s.len()], s[42 % s.len()]]));
            Page::set_bank_select(&mut buf, s[40 % s.len()]);
            Page::set_page_select(&mut buf, s[41 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn supplied_fields_ok_on_full_page(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
        ) {
            let buf: [u8; SIZE] = v.try_into().unwrap();
            prop_assert!(Page::decode_memory_model_supplied(&buf, BASE).is_ok(), "decode_memory_model_supplied");
            prop_assert!(Page::decode_module_state_supplied(&buf, BASE).is_ok(), "decode_module_state_supplied");
        }
        #[test]
        fn supplied_fields_total_on_arbitrary_input(
            bytes in prop::collection::vec(any::<u8>(), 0..=256),
            offset in 0..=300usize,
        ) {
            let _ = Page::decode_memory_model_supplied(&bytes, offset);
            let _ = Page::decode_module_state_supplied(&bytes, offset);
        }
    }
}

mod p_v5_4_CmisPage00hAdministrative {
    //! Family `CmisPage00hAdministrative` (128 B page, base offset 128).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage00hAdministrative;
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
            Page::set_sff8024_identifier_copy(&mut buf, openxcvr_cmis::generated::Sff8024Identifier::from(s[0 % s.len()]));
            Page::set_vendor_name(&mut buf, arr::<16>(&s, 1));
            Page::set_vendor_oui(&mut buf, arr::<3>(&s, 2));
            Page::set_vendor_pn(&mut buf, arr::<16>(&s, 3));
            Page::set_vendor_rev(&mut buf, arr::<2>(&s, 4));
            Page::set_vendor_sn(&mut buf, arr::<16>(&s, 5));
            Page::set_date_code_year(&mut buf, arr::<2>(&s, 6));
            Page::set_date_code_month(&mut buf, arr::<2>(&s, 7));
            Page::set_date_code_day_of_month(&mut buf, arr::<2>(&s, 8));
            Page::set_date_code_lot_code(&mut buf, arr::<2>(&s, 9));
            Page::set_clei_code(&mut buf, arr::<10>(&s, 10));
            Page::set_module_power_class(&mut buf, s[11 % s.len()]);
            Page::set_max_power(&mut buf, s[12 % s.len()]);
            Page::set_length_multiplier(&mut buf, s[13 % s.len()]);
            Page::set_base_length(&mut buf, s[14 % s.len()]);
            Page::set_connector_type(&mut buf, openxcvr_cmis::generated::Sff8024ConnectorType::from(s[15 % s.len()]));
            Page::set_attenuation_at_5ghz(&mut buf, s[16 % s.len()]);
            Page::set_attenuation_at_7ghz(&mut buf, s[17 % s.len()]);
            Page::set_attenuation_at_12p9ghz(&mut buf, s[18 % s.len()]);
            Page::set_attenuation_at_25p8ghz(&mut buf, s[19 % s.len()]);
            Page::set_attenuation_at_53p1ghz(&mut buf, s[20 % s.len()]);
            Page::set_media_lane_unsupported_lane_1(&mut buf, s[21 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_2(&mut buf, s[22 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_3(&mut buf, s[23 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_4(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_5(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_6(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_7(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_media_lane_unsupported_lane_8(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_far_end_configuration(&mut buf, s[29 % s.len()]);
            Page::set_media_interface_technology(&mut buf, s[30 % s.len()]);
            Page::set_mci_flow_control_duration_encoding(&mut buf, s[31 % s.len()] & 1 == 1);
            Page::set_mci_flow_control_duration(&mut buf, s[32 % s.len()]);
            Page::set_custom_administrative_221(&mut buf, arr::<1>(&s, 33));
            Page::set_page_checksum(&mut buf, s[34 % s.len()]);
            Page::set_custom_administrative_223_255(&mut buf, arr::<33>(&s, 35));
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
            prop_assert!(Page::decode_date_code_day_of_month_supplied(&buf, BASE).is_ok(), "decode_date_code_day_of_month_supplied");
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
            let _ = Page::decode_date_code_day_of_month_supplied(&bytes, offset);
            let _ = Page::decode_date_code_lot_code_supplied(&bytes, offset);
        }
    }
}

mod p_v5_4_CmisPage01hAdvertising {
    //! Family `CmisPage01hAdvertising` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage01hAdvertising;
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
            Page::set_module_inactive_firmware_major_revision(&mut buf, s[0 % s.len()]);
            Page::set_module_inactive_firmware_minor_revision(&mut buf, s[1 % s.len()]);
            Page::set_module_hardware_major_revision(&mut buf, s[2 % s.len()]);
            Page::set_module_hardware_minor_revision(&mut buf, s[3 % s.len()]);
            Page::set_length_multiplier_smf(&mut buf, s[4 % s.len()]);
            Page::set_base_length_smf(&mut buf, s[5 % s.len()]);
            Page::set_length_om5(&mut buf, s[6 % s.len()]);
            Page::set_length_om4(&mut buf, s[7 % s.len()]);
            Page::set_length_om3(&mut buf, s[8 % s.len()]);
            Page::set_length_om2(&mut buf, s[9 % s.len()]);
            Page::set_length_multiplier_smf2(&mut buf, s[10 % s.len()]);
            Page::set_nominal_wavelength(&mut buf, u16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_wavelength_tolerance(&mut buf, u16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_network_path_pages_supported(&mut buf, s[13 % s.len()] & 1 == 1);
            Page::set_vdm_pages_supported(&mut buf, s[14 % s.len()] & 1 == 1);
            Page::set_diagnostic_pages_supported(&mut buf, s[15 % s.len()] & 1 == 1);
            Page::set_coherent_pages_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_cmis_ff_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_page03h_supported(&mut buf, s[18 % s.len()] & 1 == 1);
            Page::set_banks_supported(&mut buf, s[19 % s.len()]);
            Page::set_mod_sel_wait_time_exponent(&mut buf, s[20 % s.len()]);
            Page::set_mod_sel_wait_time_mantissa(&mut buf, s[21 % s.len()]);
            Page::set_max_duration_dp_deinit(&mut buf, s[22 % s.len()]);
            Page::set_max_duration_dp_init(&mut buf, s[23 % s.len()]);
            Page::set_cooling_implemented(&mut buf, s[24 % s.len()] & 1 == 1);
            Page::set_e_pps_supported(&mut buf, s[25 % s.len()] & 1 == 1);
            Page::set_timing_page15h_supported(&mut buf, s[26 % s.len()] & 1 == 1);
            Page::set_aux3_mon_observable(&mut buf, s[27 % s.len()] & 1 == 1);
            Page::set_aux2_mon_observable(&mut buf, s[28 % s.len()] & 1 == 1);
            Page::set_aux1_mon_observable(&mut buf, s[29 % s.len()] & 1 == 1);
            Page::set_tx_input_clocking_capabilities(&mut buf, s[30 % s.len()]);
            Page::set_module_temp_max(&mut buf, s[31 % s.len()] as i8);
            Page::set_module_temp_min(&mut buf, s[32 % s.len()] as i8);
            Page::set_propagation_delay(&mut buf, u16::from_le_bytes([s[33 % s.len()], s[34 % s.len()]]));
            Page::set_operating_voltage_min(&mut buf, s[34 % s.len()]);
            Page::set_optical_detector_type(&mut buf, s[35 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_type(&mut buf, s[36 % s.len()]);
            Page::set_rx_power_measurement_type(&mut buf, s[37 % s.len()] & 1 == 1);
            Page::set_rx_los_type(&mut buf, s[38 % s.len()] & 1 == 1);
            Page::set_rx_los_is_fast(&mut buf, s[39 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_fast(&mut buf, s[40 % s.len()] & 1 == 1);
            Page::set_tx_disable_is_module_wide(&mut buf, s[41 % s.len()] & 1 == 1);
            Page::set_cdr_power_saved_per_lane(&mut buf, s[42 % s.len()]);
            Page::set_rx_output_level3_supported(&mut buf, s[43 % s.len()] & 1 == 1);
            Page::set_rx_output_level2_supported(&mut buf, s[44 % s.len()] & 1 == 1);
            Page::set_rx_output_level1_supported(&mut buf, s[45 % s.len()] & 1 == 1);
            Page::set_rx_output_level0_supported(&mut buf, s[46 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_max(&mut buf, s[47 % s.len()]);
            Page::set_rx_output_eq_post_cursor_max(&mut buf, s[48 % s.len()]);
            Page::set_rx_output_eq_pre_cursor_max(&mut buf, s[49 % s.len()]);
            Page::set_wavelength_is_controllable(&mut buf, s[50 % s.len()] & 1 == 1);
            Page::set_transmitter_is_tunable(&mut buf, s[51 % s.len()] & 1 == 1);
            Page::set_forced_squelch_tx_supported(&mut buf, s[52 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_tx_supported(&mut buf, s[53 % s.len()] & 1 == 1);
            Page::set_output_disable_tx_supported(&mut buf, s[54 % s.len()] & 1 == 1);
            Page::set_input_polarity_flip_tx_supported(&mut buf, s[55 % s.len()] & 1 == 1);
            Page::set_squelch_method_tx(&mut buf, s[56 % s.len()]);
            Page::set_bank_broadcast_supported(&mut buf, s[57 % s.len()] & 1 == 1);
            Page::set_auto_squelch_disable_rx_supported(&mut buf, s[58 % s.len()] & 1 == 1);
            Page::set_output_disable_rx_supported(&mut buf, s[59 % s.len()] & 1 == 1);
            Page::set_output_polarity_flip_rx_supported(&mut buf, s[60 % s.len()] & 1 == 1);
            Page::set_adaptive_input_eq_fail_flag_tx_supported(&mut buf, s[61 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_tx_supported(&mut buf, s[62 % s.len()] & 1 == 1);
            Page::set_los_flag_tx_supported(&mut buf, s[63 % s.len()] & 1 == 1);
            Page::set_failure_flag_tx_supported(&mut buf, s[64 % s.len()] & 1 == 1);
            Page::set_cdr_lol_flag_rx_supported(&mut buf, s[65 % s.len()] & 1 == 1);
            Page::set_los_flag_rx_supported(&mut buf, s[66 % s.len()] & 1 == 1);
            Page::set_custom_mon_supported(&mut buf, s[67 % s.len()] & 1 == 1);
            Page::set_aux3_mon_supported(&mut buf, s[68 % s.len()] & 1 == 1);
            Page::set_aux2_mon_supported(&mut buf, s[69 % s.len()] & 1 == 1);
            Page::set_aux1_mon_supported(&mut buf, s[70 % s.len()] & 1 == 1);
            Page::set_vcc_mon_supported(&mut buf, s[71 % s.len()] & 1 == 1);
            Page::set_temp_mon_supported(&mut buf, s[72 % s.len()] & 1 == 1);
            Page::set_tx_bias_current_scaling_factor(&mut buf, s[73 % s.len()]);
            Page::set_rx_optical_power_mon_supported(&mut buf, s[74 % s.len()] & 1 == 1);
            Page::set_tx_optical_power_mon_supported(&mut buf, s[75 % s.len()] & 1 == 1);
            Page::set_tx_bias_mon_supported(&mut buf, s[76 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_recall_buffers_supported(&mut buf, s[77 % s.len()]);
            Page::set_tx_input_eq_freeze_supported(&mut buf, s[78 % s.len()] & 1 == 1);
            Page::set_tx_input_adaptive_eq_supported(&mut buf, s[79 % s.len()] & 1 == 1);
            Page::set_tx_input_eq_host_control_supported(&mut buf, s[80 % s.len()] & 1 == 1);
            Page::set_tx_cdr_bypass_control_supported(&mut buf, s[81 % s.len()] & 1 == 1);
            Page::set_tx_cdr_supported(&mut buf, s[82 % s.len()] & 1 == 1);
            Page::set_versatile_control_set_supported(&mut buf, s[83 % s.len()] & 1 == 1);
            Page::set_unidir_reconfig_supported(&mut buf, s[84 % s.len()] & 1 == 1);
            Page::set_staged_set1_supported(&mut buf, s[85 % s.len()] & 1 == 1);
            Page::set_rx_output_eq_control_supported(&mut buf, s[86 % s.len()]);
            Page::set_rx_output_amplitude_control_supported(&mut buf, s[87 % s.len()] & 1 == 1);
            Page::set_rx_cdr_bypass_control_supported(&mut buf, s[88 % s.len()] & 1 == 1);
            Page::set_rx_cdr_supported(&mut buf, s[89 % s.len()] & 1 == 1);
            Page::set_cdb_instances_supported(&mut buf, s[90 % s.len()]);
            Page::set_cdb_background_mode_supported(&mut buf, s[91 % s.len()] & 1 == 1);
            Page::set_cdb_auto_paging_supported(&mut buf, s[92 % s.len()] & 1 == 1);
            Page::set_cdb_max_pages_epl(&mut buf, s[93 % s.len()]);
            Page::set_cdb_read_write_length_extension(&mut buf, s[94 % s.len()]);
            Page::set_cdb_command_trigger_method(&mut buf, s[95 % s.len()] & 1 == 1);
            Page::set_cdb_ext_max_busy_time(&mut buf, s[96 % s.len()]);
            Page::set_cdb_max_busy_spec_method(&mut buf, s[97 % s.len()] & 1 == 1);
            Page::set_cdb_max_busy_time(&mut buf, s[98 % s.len()]);
            Page::set_max_duration_module_pwr_dn(&mut buf, s[99 % s.len()]);
            Page::set_max_duration_module_pwr_up(&mut buf, s[100 % s.len()]);
            Page::set_max_duration_dp_tx_turn_off(&mut buf, s[101 % s.len()]);
            Page::set_max_duration_dp_tx_turn_on(&mut buf, s[102 % s.len()]);
            Page::set_max_duration_bpc(&mut buf, s[103 % s.len()]);
            Page::set_page_0ch_supported(&mut buf, s[104 % s.len()] & 1 == 1);
            Page::set_page_0dh_supported(&mut buf, s[105 % s.len()] & 1 == 1);
            Page::set_page_60h_supported(&mut buf, s[106 % s.len()] & 1 == 1);
            Page::set_page_61h_supported(&mut buf, s[107 % s.len()] & 1 == 1);
            Page::set_page_62h_supported(&mut buf, s[108 % s.len()] & 1 == 1);
            Page::set_extra_lane_banks_supported(&mut buf, s[109 % s.len()]);
            Page::set_nad_banks_supported(&mut buf, s[110 % s.len()]);
            Page::set_scratch_pad_supported(&mut buf, s[111 % s.len()]);
            Page::set_password_entry_supported(&mut buf, s[112 % s.len()]);
            Page::set_password_entry_result_supported(&mut buf, s[113 % s.len()]);
            Page::set_full_page_read_supported(&mut buf, s[114 % s.len()]);
            Page::set_host_lane_switching_supported(&mut buf, s[115 % s.len()] & 1 == 1);
            Page::set_link_training_supported(&mut buf, s[116 % s.len()] & 1 == 1);
            Page::set_media_lane_switching_supported(&mut buf, s[117 % s.len()] & 1 == 1);
            Page::set_page_checksum(&mut buf, s[118 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_app(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_app(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_app(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage04hLaserCapabilities {
    //! Family `CmisPage04hLaserCapabilities` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage04hLaserCapabilities;
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
            Page::set_grid_supported_75_ghz(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_grid_supported_33_ghz(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_grid_supported_100_ghz(&mut buf, s[2 % s.len()] & 1 == 1);
            Page::set_grid_supported_50_ghz(&mut buf, s[3 % s.len()] & 1 == 1);
            Page::set_grid_supported_25_ghz(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_grid_supported_12p5_ghz(&mut buf, s[5 % s.len()] & 1 == 1);
            Page::set_grid_supported_6p25_ghz(&mut buf, s[6 % s.len()] & 1 == 1);
            Page::set_grid_supported_3p125_ghz(&mut buf, s[7 % s.len()] & 1 == 1);
            Page::set_fine_tuning_supported(&mut buf, s[8 % s.len()] & 1 == 1);
            Page::set_grid_supported_150_ghz(&mut buf, s[9 % s.len()] & 1 == 1);
            Page::set_grid_supported_300_ghz(&mut buf, s[10 % s.len()] & 1 == 1);
            Page::set_grid_low_channel_300_ghz(&mut buf, i16::from_le_bytes([s[11 % s.len()], s[12 % s.len()]]));
            Page::set_grid_high_channel_300_ghz(&mut buf, i16::from_le_bytes([s[12 % s.len()], s[13 % s.len()]]));
            Page::set_fine_tuning_resolution(&mut buf, u16::from_le_bytes([s[13 % s.len()], s[14 % s.len()]]));
            Page::set_fine_tuning_low_offset(&mut buf, i16::from_le_bytes([s[14 % s.len()], s[15 % s.len()]]));
            Page::set_fine_tuning_high_offset(&mut buf, i16::from_le_bytes([s[15 % s.len()], s[16 % s.len()]]));
            Page::set_prog_output_power_per_lane_supported(&mut buf, s[16 % s.len()] & 1 == 1);
            Page::set_output_power_tx_relative_thresholds_supported(&mut buf, s[17 % s.len()] & 1 == 1);
            Page::set_prog_output_power_min(&mut buf, i16::from_le_bytes([s[18 % s.len()], s[19 % s.len()]]));
            Page::set_prog_output_power_max(&mut buf, i16::from_le_bytes([s[19 % s.len()], s[20 % s.len()]]));
            Page::set_page_checksum(&mut buf, s[20 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_low_channel(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_grid_high_channel(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage0ChModuleManagement {
    //! Family `CmisPage0ChModuleManagement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage0ChModuleManagement;
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

mod p_v5_4_CmisPage0DhFirmwareManagement {
    //! Family `CmisPage0DhFirmwareManagement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage0DhFirmwareManagement;
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
            Page::set_reserved_page_0dh_132_135(&mut buf, arr::<4>(&s, 0));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage11hLaneStatus {
    //! Family `CmisPage11hLaneStatus` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage11hLaneStatus;
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

mod p_v5_4_CmisPage12hTunableLaser {
    //! Family `CmisPage12hTunableLaser` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage12hTunableLaser;
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
            Page::set_output_power_tx_hi_alarm_rel_thr(&mut buf, s[0 % s.len()]);
            Page::set_output_power_tx_hi_warn_rel_thr(&mut buf, s[1 % s.len()]);
            Page::set_output_power_tx_lo_alarm_rel_thr(&mut buf, s[2 % s.len()]);
            Page::set_output_power_tx_lo_warn_rel_thr(&mut buf, s[3 % s.len()]);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_grid_spacing_tx(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_relative_output_power_thresholds_enable_tx(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_enable_tx(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_channel_number_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_offset_tx(&mut buf, idx, i16::from_le_bytes([s[4 % s.len()], s[5 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_tx(&mut buf, idx, i16::from_le_bytes([s[5 % s.len()], s[6 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_target_output_power_oor_mask_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_fine_tuning_out_of_range_mask_tx(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_not_accepted_mask_tx(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_invalid_channel_mask_tx(&mut buf, idx, s[9 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_wavelength_unlocked_mask_tx(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_tuning_complete_mask_tx(&mut buf, idx, s[11 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage14hDiagnosticsResults {
    //! Family `CmisPage14hDiagnosticsResults` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage14hDiagnosticsResults;
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
            Page::set_diagnostics_selector(&mut buf, openxcvr_cmis::generated::CmisDiagnosticsSelector::from(s[0 % s.len()]));
            Page::set_custom_diagnostics_selection(&mut buf, arr::<2>(&s, 1));
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage16hNetworkPath {
    //! Family `CmisPage16hNetworkPath` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage16hNetworkPath;
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
            Page::set_max_duration_np_deinit(&mut buf, s[0 % s.len()]);
            Page::set_max_duration_np_init(&mut buf, s[1 % s.len()]);
            Page::set_max_duration_np_tx_turn_off(&mut buf, s[2 % s.len()]);
            Page::set_max_duration_np_tx_turn_on(&mut buf, s[3 % s.len()]);
            Page::set_replace_hp_signal_tx_supported(&mut buf, s[4 % s.len()] & 1 == 1);
            Page::set_replace_hp_signal_rx_supported(&mut buf, s[5 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_npid_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_np_in_use_lane(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_npid_lane(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_np_in_use_lane(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_deinit_lane(&mut buf, idx, s[4 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_hp_source_rx(&mut buf, idx, s[5 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_np_source_tx(&mut buf, idx, s[6 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs0_apply_np_init_lane(&mut buf, idx, s[7 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_apply_np_init_lane(&mut buf, idx, s[8 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_mux_granularity(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[9 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_ext_app_descriptor(&mut buf, idx, s[10 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage18hControlSetExtension {
    //! Family `CmisPage18hControlSetExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage18hControlSetExtension;
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
            Page::set_scs0_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 0));
            Page::set_scs1_versatile_control_set_parameter_space(&mut buf, arr::<56>(&s, 1));
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_scs0_nad_block_index(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_scs1_nad_block_index(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage19hStatusExtension {
    //! Family `CmisPage19hStatusExtension` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage19hStatusExtension;
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

mod p_v5_4_CmisPage1ChNormalizedApplicationDescriptors {
    //! Family `CmisPage1ChNormalizedApplicationDescriptors` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage1ChNormalizedApplicationDescriptors;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_host_interface_id_nad(&mut buf, idx, openxcvr_cmis::generated::Sff8024HostElectricalInterface::from(s[0 % s.len()])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_lane_count_nad(&mut buf, idx, s[1 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_lane_count_nad(&mut buf, idx, s[2 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_network_path_indicator_nad(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_host_interface_gid_nad(&mut buf, idx, s[4 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_media_interface_gid_nad(&mut buf, idx, s[5 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage1DhHostLaneSwitching {
    //! Family `CmisPage1DhHostLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage1DhHostLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_host_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage20hVdmDescriptors1To64 {
    //! Family `CmisPage20hVdmDescriptors1To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage20hVdmDescriptors1To64;
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

mod p_v5_4_CmisPage21hVdmDescriptors65To128 {
    //! Family `CmisPage21hVdmDescriptors65To128` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage21hVdmDescriptors65To128;
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

mod p_v5_4_CmisPage22hVdmDescriptors129To192 {
    //! Family `CmisPage22hVdmDescriptors129To192` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage22hVdmDescriptors129To192;
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

mod p_v5_4_CmisPage23hVdmDescriptors193To256 {
    //! Family `CmisPage23hVdmDescriptors193To256` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage23hVdmDescriptors193To256;
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

mod p_v5_4_CmisPage28hVdmThresholds1To16 {
    //! Family `CmisPage28hVdmThresholds1To16` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage28hVdmThresholds1To16;
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

mod p_v5_4_CmisPage29hVdmThresholds17To32 {
    //! Family `CmisPage29hVdmThresholds17To32` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage29hVdmThresholds17To32;
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

mod p_v5_4_CmisPage2AhVdmThresholds33To48 {
    //! Family `CmisPage2AhVdmThresholds33To48` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage2AhVdmThresholds33To48;
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

mod p_v5_4_CmisPage2BhVdmThresholds49To64 {
    //! Family `CmisPage2BhVdmThresholds49To64` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage2BhVdmThresholds49To64;
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

mod p_v5_4_CmisPage2ChVdmFlags {
    //! Family `CmisPage2ChVdmFlags` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage2ChVdmFlags;
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

mod p_v5_4_CmisPage2DhVdmMasks {
    //! Family `CmisPage2DhVdmMasks` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage2DhVdmMasks;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_high_alarm_tc_mask(&mut buf, idx, s[0 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_alarm_tc_mask(&mut buf, idx, s[1 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_high_warning_tc_mask(&mut buf, idx, s[2 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_low_warning_tc_mask(&mut buf, idx, s[3 % s.len()] & 1 == 1) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage2FhVdmAdvertisement {
    //! Family `CmisPage2FhVdmAdvertisement` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage2FhVdmAdvertisement;
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
            Page::set_freeze_request(&mut buf, s[0 % s.len()] & 1 == 1);
            Page::set_power_saving_mode(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_monitoring_duty_cycle(&mut buf, s[2 % s.len()]);
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage60hLaneDatapathManagementExtensions {
    //! Family `CmisPage60hLaneDatapathManagementExtensions` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage60hLaneDatapathManagementExtensions;
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

mod p_v5_4_CmisPage61hLaneDatapathMonitoring {
    //! Family `CmisPage61hLaneDatapathMonitoring` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage61hLaneDatapathMonitoring;
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

mod p_v5_4_CmisPage62hLaneSupervisionThresholds {
    //! Family `CmisPage62hLaneSupervisionThresholds` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage62hLaneSupervisionThresholds;
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
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_output_power_threshold_hi_alarm_tx(&mut buf, idx, i16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_power_threshold_lo_alarm_tx(&mut buf, idx, i16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_power_threshold_hi_warning_tx(&mut buf, idx, i16::from_le_bytes([s[2 % s.len()], s[3 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
                {
                    let before = buf;
                    match Page::set_output_power_threshold_lo_warning_tx(&mut buf, idx, i16::from_le_bytes([s[3 % s.len()], s[4 % s.len()]])) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage6DhMediaLaneSwitching {
    //! Family `CmisPage6DhMediaLaneSwitching` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage6DhMediaLaneSwitching;
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
            Page::set_max_redirection_commit_duration(&mut buf, s[0 % s.len()]);
            Page::set_enable_media_lane_redirection(&mut buf, s[1 % s.len()] & 1 == 1);
            Page::set_commit_media_lane_redirection(&mut buf, s[2 % s.len()] & 1 == 1);
            let _ = Page::decode(&buf);
        }
        #[test]
        fn indexed_setters_reject_out_of_range(
            v in prop::collection::vec(any::<u8>(), SIZE..=SIZE),
            s in prop::collection::vec(any::<u8>(), 32..=32),
        ) {
            let mut buf: [u8; SIZE] = v.try_into().unwrap();
            let indexes: [usize; 17] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 16, 64, 9999,
                usize::MAX - 1, usize::MAX,
            ];
            for idx in indexes {
                {
                    let before = buf;
                    match Page::set_redirection_of_media_lane(&mut buf, idx, s[0 % s.len()]) {
                        Ok(()) => {}
                        Err(e) => {
                            prop_assert!(matches!(e, SpanError::InvalidIndex(_)));
                            prop_assert_eq!(buf, before);
                        }
                    }
                }
            }
            let _ = Page::decode(&buf);
        }
    }
}

mod p_v5_4_CmisPage9FhCdbMessage {
    //! Family `CmisPage9FhCdbMessage` (128 B page, base offset 0).
    use super::*;
    type Page = openxcvr_cmis::generated::v5_4::CmisPage9FhCdbMessage;
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
            Page::set_cmdid(&mut buf, openxcvr_cmis::generated::CmisCdbCommandId::from(u16::from_le_bytes([s[0 % s.len()], s[1 % s.len()]])));
            Page::set_epl_length(&mut buf, u16::from_le_bytes([s[1 % s.len()], s[2 % s.len()]]));
            Page::set_lpl_length(&mut buf, s[2 % s.len()]);
            Page::set_cdb_chk_code(&mut buf, s[3 % s.len()]);
            Page::set_rpl_length(&mut buf, s[4 % s.len()]);
            Page::set_rpl_chk_code(&mut buf, s[5 % s.len()]);
            Page::set_lpl(&mut buf, arr::<120>(&s, 6));
            let _ = Page::decode(&buf);
        }
    }
}
