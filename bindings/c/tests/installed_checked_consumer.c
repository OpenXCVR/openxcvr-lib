/* Installed-prefix consumer of both revision-checked CMIS page APIs.
 * Compiled and run twice by tests/installed_checked_consumer.py: once
 * against the installed static archive named by path, once through
 * pkg-config against the installed shared library. Everything is included
 * through the installed umbrella header exactly as a downstream consumer
 * would. */
#include "openxcvr/openxcvr.h"

#include <stdio.h>
#include <string.h>

static int failures;

#define CHECK(cond)                                                     \
    do {                                                                \
        if (!(cond)) {                                                  \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
            failures++;                                                 \
        }                                                               \
    } while (0)

/* Both pages, all six catalog revisions, on zero and 0xFF inputs; plus the
 * error contract through the installed headers. */
static void both_pages_all_revisions(void) {
    static const uint8_t revs[] = {0x30, 0x40, 0x50, 0x51, 0x52, 0x53};
    for (int pattern = 0; pattern <= 1; ++pattern) {
        uint8_t buf[128];
        memset(buf, pattern ? 0xFF : 0, sizeof(buf));
        for (size_t i = 0; i < sizeof(revs); ++i) {
            uint8_t rev = revs[i];
            openxcvr_cmis_page01h_advertising_checked a;
            openxcvr_cmis_page11h_lane_status_checked s;
            memset(&a, 0, sizeof(a));
            memset(&s, 0, sizeof(s));
            CHECK(openxcvr_decode_cmis_page01h_advertising_for_revision(
                rev, buf, sizeof(buf), &a) == OPENXCVR_OK);
            CHECK(openxcvr_decode_cmis_page11h_lane_status_for_revision(
                rev, buf, sizeof(buf), &s) == OPENXCVR_OK);
            CHECK(a.cmis_rev_byte == rev);
            CHECK(s.cmis_rev_byte == rev);
            /* Applicability landmarks, independent of the generator's own
             * tests: presence, not values, decides applicability. */
            CHECK(a.present.module_hardware_major_revision);
            CHECK(a.present.page_checksum);
            CHECK(a.present.module_inactive_firmware_major_revision ==
                  (rev >= 0x40));
            CHECK(a.present.e_pps_supported == (rev >= 0x50));
            CHECK(a.present.network_path_pages_supported == (rev >= 0x51));
            CHECK(a.present.bank_broadcast_supported == (rev >= 0x52));
            CHECK(a.present.password_entry_supported == (rev >= 0x53));
            CHECK(s.present.dp_state_host_lane);
            CHECK(s.present.optical_power_tx);
            CHECK(s.present.output_status_rx == (rev >= 0x50));
            CHECK(s.present.dp_init_pending_lane == (rev >= 0x50));
            /* Present-zero is valid input; absent stays zero even for 0xFF. */
            CHECK(a.values.module_hardware_major_revision ==
                  (pattern ? 255 : 0));
            CHECK(a.values.password_entry_supported ==
                  ((rev >= 0x53 && pattern) ? 3 : 0));
            for (size_t lane = 0; lane < 8; ++lane) {
                CHECK(s.values.dp_state_host_lane[lane] == (pattern ? 15 : 0));
                CHECK(s.values.optical_power_tx[lane] ==
                      (pattern ? 65535 : 0));
                CHECK(s.values.dp_init_pending_lane[lane] ==
                      ((rev >= 0x50 && pattern) ? 1 : 0));
            }
            /* Raw enum storage is unremapped; interpretation stays with the
             * existing helpers also reached through the installed headers. */
            openxcvr_cmis_data_path_state_projection project =
                openxcvr_cmis_data_path_state_for_revision(rev);
            CHECK(project != NULL);
            if (pattern) {
                CHECK(project(3) != NULL && strcmp(project(3), "DpDeinit") == 0);
                CHECK(project(4) != NULL &&
                      strcmp(project(4), "DpActivated") == 0);
                CHECK(project(15) == NULL);
            }
        }
    }
}

static void error_contract(void) {
    uint8_t buf[128] = {0};
    openxcvr_cmis_page01h_advertising_checked a;
    openxcvr_cmis_page11h_lane_status_checked s;
    memset(&a, 0xA5, sizeof(a));
    memset(&s, 0xA5, sizeof(s));
    openxcvr_cmis_page01h_advertising_checked a_sentinel = a;
    openxcvr_cmis_page11h_lane_status_checked s_sentinel = s;
    /* Error precedence: NULL, then unsupported revision, then short input;
     * failures never touch caller output. */
    CHECK(openxcvr_decode_cmis_page01h_advertising_for_revision(
        0x53, NULL, sizeof(buf), &a) == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(openxcvr_decode_cmis_page01h_advertising_for_revision(
        0x53, buf, sizeof(buf), NULL) == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(memcmp(&a, &a_sentinel, sizeof(a)) == 0);
    CHECK(openxcvr_decode_cmis_page11h_lane_status_for_revision(
        0x55, buf, sizeof(buf), &s) == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    CHECK(memcmp(&s, &s_sentinel, sizeof(s)) == 0);
    CHECK(openxcvr_decode_cmis_page11h_lane_status_for_revision(
        0x53, buf, 127, &s) == OPENXCVR_ERR_SHORT_BUFFER);
    CHECK(memcmp(&s, &s_sentinel, sizeof(s)) == 0);
    /* Longer inputs are accepted like the raw decoders accept them. */
    CHECK(openxcvr_decode_cmis_page01h_advertising_for_revision(
        0x53, buf, 129, &a) == OPENXCVR_OK);
    CHECK(openxcvr_decode_cmis_page11h_lane_status_for_revision(
        0x53, buf, 128, &s) == OPENXCVR_OK);
}

int main(void) {
    both_pages_all_revisions();
    error_contract();
    if (failures) {
        fprintf(stderr, "installed checked consumer: %d failures\n", failures);
        return 1;
    }
    puts("installed checked consumer: both pages, both link modes, all six "
         "revisions, error contract passed");
    return 0;
}
