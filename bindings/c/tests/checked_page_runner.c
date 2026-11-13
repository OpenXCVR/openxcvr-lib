/* Test-only canonical projection runner for the revision-checked CMIS
 * pages. Invoked by tests/checked_page_differential.py as the C half of the
 * C/Rust differential:
 *
 *     checked_page_runner <01h|11h> <rev-byte> <seed>
 *
 * <seed> is either a decimal LCG seed, or "dpsm-<hex nibble>" which also
 * fills bytes 0..3 with the nibble mirrored into both nibble positions so
 * every raw DPSM value can be swept per revision. The output is one
 * canonical line per presence flag and per present value member:
 *
 *     rev <decimal>
 *     present <member> 0|1
 *     value <member> <n>
 *     value <member>[<i>] <n>
 *     interp dp_state_host_lane[<i>] <name|Unknown<n>>
 *
 * The member projection below is a compact one-per-page X-macro list (the
 * raw struct member set, which the presence struct mirrors 1:1); the
 * differential script additionally parses the generated headers and asserts
 * this list is exactly that member set, so a silently omitted member fails
 * the test. Absent members must be zero: asserted here and re-checked from
 * the parsed output by the script. Raw enum storage is printed unremapped;
 * dp_state_host_lane is additionally interpreted through the existing
 * openxcvr_cmis_data_path_state_for_revision projection (the differential's
 * independent landmark against a shared-resolver common bug).
 */
#include "openxcvr/openxcvr.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PAGE01H_MEMBERS(X) \
X(module_inactive_firmware_major_revision, u8) \
    X(module_inactive_firmware_minor_revision, u8) \
    X(module_hardware_major_revision, u8) \
    X(module_hardware_minor_revision, u8) \
    X(length_multiplier_smf, u8) \
    X(base_length_smf, u8) \
    X(length_om5, u8) \
    X(length_om4, u8) \
    X(length_om3, u8) \
    X(length_om2, u8) \
    X(length_multiplier_smf2, u8) \
    X(nominal_wavelength, u16) \
    X(wavelength_tolerance, u16) \
    X(network_path_pages_supported, flag) \
    X(vdm_pages_supported, flag) \
    X(diagnostic_pages_supported, flag) \
    X(coherent_pages_supported, flag) \
    X(cmis_ff_supported, flag) \
    X(page03h_supported, flag) \
    X(banks_supported, u8) \
    X(mod_sel_wait_time_exponent, u8) \
    X(mod_sel_wait_time_mantissa, u8) \
    X(max_duration_dp_deinit, u8) \
    X(max_duration_dp_init, u8) \
    X(cooling_implemented, flag) \
    X(e_pps_supported, flag) \
    X(timing_page15h_supported, flag) \
    X(aux3_mon_observable, flag) \
    X(aux2_mon_observable, flag) \
    X(aux1_mon_observable, flag) \
    X(tx_input_clocking_capabilities, u8) \
    X(module_temp_max, i8) \
    X(module_temp_min, i8) \
    X(propagation_delay, u16) \
    X(operating_voltage_min, u8) \
    X(optical_detector_type, flag) \
    X(rx_output_eq_type, u8) \
    X(rx_power_measurement_type, flag) \
    X(rx_los_type, flag) \
    X(rx_los_is_fast, flag) \
    X(tx_disable_is_fast, flag) \
    X(tx_disable_is_module_wide, flag) \
    X(cdr_power_saved_per_lane, u8) \
    X(rx_output_level3_supported, flag) \
    X(rx_output_level2_supported, flag) \
    X(rx_output_level1_supported, flag) \
    X(rx_output_level0_supported, flag) \
    X(tx_input_eq_max, u8) \
    X(rx_output_eq_post_cursor_max, u8) \
    X(rx_output_eq_pre_cursor_max, u8) \
    X(wavelength_is_controllable, flag) \
    X(transmitter_is_tunable, flag) \
    X(forced_squelch_tx_supported, flag) \
    X(auto_squelch_disable_tx_supported, flag) \
    X(output_disable_tx_supported, flag) \
    X(input_polarity_flip_tx_supported, flag) \
    X(squelch_method_tx, u8) \
    X(bank_broadcast_supported, flag) \
    X(auto_squelch_disable_rx_supported, flag) \
    X(output_disable_rx_supported, flag) \
    X(output_polarity_flip_rx_supported, flag) \
    X(adaptive_input_eq_fail_flag_tx_supported, flag) \
    X(cdr_lol_flag_tx_supported, flag) \
    X(los_flag_tx_supported, flag) \
    X(failure_flag_tx_supported, flag) \
    X(cdr_lol_flag_rx_supported, flag) \
    X(los_flag_rx_supported, flag) \
    X(custom_mon_supported, flag) \
    X(aux3_mon_supported, flag) \
    X(aux2_mon_supported, flag) \
    X(aux1_mon_supported, flag) \
    X(vcc_mon_supported, flag) \
    X(temp_mon_supported, flag) \
    X(tx_bias_current_scaling_factor, u8) \
    X(rx_optical_power_mon_supported, flag) \
    X(tx_optical_power_mon_supported, flag) \
    X(tx_bias_mon_supported, flag) \
    X(tx_input_eq_recall_buffers_supported, u8) \
    X(tx_input_eq_freeze_supported, flag) \
    X(tx_input_adaptive_eq_supported, flag) \
    X(tx_input_eq_host_control_supported, flag) \
    X(tx_cdr_bypass_control_supported, flag) \
    X(tx_cdr_supported, flag) \
    X(versatile_control_set_supported, flag) \
    X(unidir_reconfig_supported, flag) \
    X(staged_set1_supported, flag) \
    X(rx_output_eq_control_supported, u8) \
    X(rx_output_amplitude_control_supported, flag) \
    X(rx_cdr_bypass_control_supported, flag) \
    X(rx_cdr_supported, flag) \
    X(cdb_instances_supported, u8) \
    X(cdb_background_mode_supported, flag) \
    X(cdb_auto_paging_supported, flag) \
    X(cdb_max_pages_epl, u8) \
    X(cdb_read_write_length_extension, u8) \
    X(cdb_command_trigger_method, flag) \
    X(cdb_ext_max_busy_time, u8) \
    X(cdb_max_busy_spec_method, flag) \
    X(cdb_max_busy_time, u8) \
    X(max_duration_module_pwr_dn, u8) \
    X(max_duration_module_pwr_up, u8) \
    X(max_duration_dp_tx_turn_off, u8) \
    X(max_duration_dp_tx_turn_on, u8) \
    X(max_duration_bpc, u8) \
    X(nad_banks_supported, u8) \
    X(host_interface_id_app, a7_u8) \
    X(host_lane_count_app, a7_u8) \
    X(media_lane_count_app, a7_u8) \
    X(scratch_pad_supported, u8) \
    X(password_entry_supported, u8) \
    X(password_entry_result_supported, u8) \
    X(full_page_read_supported, u8) \
    X(host_lane_switching_supported, flag) \
    X(link_training_supported, flag) \
    X(page_checksum, u8) \

#define PAGE11H_MEMBERS(X) \
X(dp_state_host_lane, a8_u8) \
    X(output_status_rx, a8_flag) \
    X(output_status_tx, a8_flag) \
    X(dp_state_changed_flag, a8_flag) \
    X(failure_flag_tx, a8_flag) \
    X(los_flag_tx, a8_flag) \
    X(cdr_lol_flag_tx, a8_flag) \
    X(adaptive_input_eq_fail_flag_tx, a8_flag) \
    X(optical_power_high_alarm_flag_tx, a8_flag) \
    X(optical_power_low_alarm_flag_tx, a8_flag) \
    X(optical_power_high_warning_flag_tx, a8_flag) \
    X(optical_power_low_warning_flag_tx, a8_flag) \
    X(laser_bias_high_alarm_flag_tx, a8_flag) \
    X(laser_bias_low_alarm_flag_tx, a8_flag) \
    X(laser_bias_high_warning_flag_tx, a8_flag) \
    X(laser_bias_low_warning_flag_tx, a8_flag) \
    X(los_flag_rx, a8_flag) \
    X(cdr_lol_flag_rx, a8_flag) \
    X(optical_power_high_alarm_flag_rx, a8_flag) \
    X(optical_power_low_alarm_flag_rx, a8_flag) \
    X(optical_power_high_warning_flag_rx, a8_flag) \
    X(optical_power_low_warning_flag_rx, a8_flag) \
    X(output_status_changed_flag_rx, a8_flag) \
    X(optical_power_tx, a8_u16) \
    X(laser_bias_tx, a8_u16) \
    X(optical_power_rx, a8_u16) \
    X(config_status_lane, a8_u8) \
    X(app_sel_code_lane, a8_u8) \
    X(data_path_id_lane, a8_u8) \
    X(explicit_control_lane, a8_flag) \
    X(adaptive_input_eq_enable_tx, a8_flag) \
    X(adaptive_input_eq_recalled_tx, a8_u8) \
    X(host_controlled_input_eq_target_tx, a8_u8) \
    X(cdr_enable_tx, a8_flag) \
    X(cdr_enabled_rx, a8_flag) \
    X(output_eq_pre_cursor_target_rx, a8_u8) \
    X(output_eq_post_cursor_target_rx, a8_u8) \
    X(output_amplitude_target_rx, a8_u8) \
    X(dp_init_pending_lane, a8_flag) \
    X(media_lane_to_wavelength_mapping_tx, a8_u8) \
    X(media_lane_to_fiber_mapping_tx, a8_u8) \
    X(media_lane_to_wavelength_mapping_rx, a8_u8) \
    X(media_lane_to_fiber_mapping_rx, a8_u8) \

static uint32_t lcg_fill(uint8_t *buf, uint32_t x) {
    for (size_t i = 0; i < 128; ++i) {
        x = 1664525u * x + 1013904223u;
        buf[i] = (uint8_t)(x >> 24);
    }
    return x;
}

/* seed: decimal LCG seed, or "dpsm-<hex>" for the raw DPSM sweep. */
static void fill_buffer(uint8_t *buf, const char *seed) {
    uint32_t x = 0;
    if (strncmp(seed, "dpsm-", 5) == 0) {
        x = (uint32_t)strtoul(seed + 5, NULL, 16);
        if (x > 15) {
            fprintf(stderr, "dpsm nibble out of range: %s\n", seed);
            exit(2);
        }
        lcg_fill(buf, x);
        for (size_t i = 0; i < 4; ++i)
            buf[i] = (uint8_t)(x | (x << 4));
    } else {
        x = (uint32_t)strtoul(seed, NULL, 10);
        lcg_fill(buf, x);
    }
}

#define EMIT_u8(m) do { \
    printf("present %s %d\n", #m, (int)c.present.m); \
    if (c.present.m) printf("value %s %u\n", #m, (unsigned)c.values.m); \
    else assert(c.values.m == 0); \
} while (0)

#define EMIT_i8(m) do { \
    printf("present %s %d\n", #m, (int)c.present.m); \
    if (c.present.m) printf("value %s %d\n", #m, (int)c.values.m); \
    else assert(c.values.m == 0); \
} while (0)

#define EMIT_u16(m) do { \
    printf("present %s %d\n", #m, (int)c.present.m); \
    if (c.present.m) printf("value %s %u\n", #m, (unsigned)c.values.m); \
    else assert(c.values.m == 0); \
} while (0)

#define EMIT_flag(m) do { \
    printf("present %s %d\n", #m, (int)c.present.m); \
    if (c.present.m) printf("value %s %d\n", #m, c.values.m ? 1 : 0); \
    else assert(!c.values.m); \
} while (0)

#define FMT_u8(v) ((unsigned)(v))
#define FMT_u16(v) ((unsigned)(v))
#define FMT_flag(v) ((v) ? 1u : 0u)

#define EMIT_a7_u8(m) EMIT_ARRAY(m, 7, u8)
#define EMIT_a8_u8(m) EMIT_ARRAY(m, 8, u8)
#define EMIT_a8_u16(m) EMIT_ARRAY(m, 8, u16)
#define EMIT_a8_flag(m) EMIT_ARRAY(m, 8, flag)

#define EMIT_ARRAY(m, n, tag) do { \
    printf("present %s %d\n", #m, (int)c.present.m); \
    for (size_t i = 0; i < n; ++i) { \
        if (c.present.m) \
            printf("value %s[%zu] %u\n", #m, i, FMT_##tag(c.values.m[i])); \
        else \
            assert(c.values.m[i] == 0); \
    } \
} while (0)

#define PASTE(a, b) a##b
#define PROJECT(m, tag) PASTE(EMIT_, tag)(m)

#define X(m, tag) PROJECT(m, tag);
static void run_01h(uint8_t rev, const uint8_t *buf) {
    openxcvr_cmis_page01h_advertising_checked c;
    memset(&c, 0, sizeof(c));
    if (openxcvr_decode_cmis_page01h_advertising_for_revision(
            rev, buf, 128, &c) != OPENXCVR_OK) {
        fprintf(stderr, "01h decode failed\n");
        exit(2);
    }
    printf("rev %u\n", (unsigned)rev);
    PAGE01H_MEMBERS(X)
}

static void run_11h(uint8_t rev, const uint8_t *buf) {
    openxcvr_cmis_page11h_lane_status_checked c;
    memset(&c, 0, sizeof(c));
    if (openxcvr_decode_cmis_page11h_lane_status_for_revision(
            rev, buf, 128, &c) != OPENXCVR_OK) {
        fprintf(stderr, "11h decode failed\n");
        exit(2);
    }
    printf("rev %u\n", (unsigned)rev);
    PAGE11H_MEMBERS(X)
    /* Raw nibbles interpreted through the existing revision projection:
     * a known name, or Unknown<n> for reserved-at-this-revision raws. */
    openxcvr_cmis_data_path_state_projection project =
        openxcvr_cmis_data_path_state_for_revision(rev);
    assert(project != NULL);
    for (size_t i = 0; i < 8; ++i) {
        uint8_t raw = c.values.dp_state_host_lane[i];
        const char *name = project(raw);
        if (name != NULL)
            printf("interp dp_state_host_lane[%zu] %s\n", i, name);
        else
            printf("interp dp_state_host_lane[%zu] Unknown%u\n", i,
                   (unsigned)raw);
    }
}

int main(int argc, char **argv) {
    uint8_t buf[128];
    unsigned long rev;
    if (argc != 4 || ((strcmp(argv[1], "01h") != 0 &&
                      strcmp(argv[1], "11h") != 0)) ||
        (rev = strtoul(argv[2], NULL, 0)) > 255) {
        fprintf(stderr,
                "usage: %s <01h|11h> <rev-byte> <seed>\n", argv[0]);
        return 2;
    }
    fill_buffer(buf, argv[3]);
    if (strcmp(argv[1], "01h") == 0)
        run_01h((uint8_t)rev, buf);
    else
        run_11h((uint8_t)rev, buf);
    return 0;
}
