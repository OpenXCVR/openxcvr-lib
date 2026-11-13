/* Additive checked Page 01h/11h contract. Literal revision landmarks here
 * are independent of the generator's schema-derived exhaustive member test.
 * Presence means applicability, never enum validity. */
#include "openxcvr/openxcvr.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>

static int accepted(unsigned rev) {
    return rev == 0x30 || rev == 0x40 || rev == 0x50 ||
           rev == 0x51 || rev == 0x52 || rev == 0x53 || rev == 0x54;
}

/* Bytewise sentinel comparisons are only used on failures, when no output
 * store is permitted (including padding). Both output types are exercised. */
#define ADMISSION(suffix)                                                       \
    do {                                                                        \
        openxcvr_##suffix##_checked out, sentinel;                               \
        memset(&sentinel, 0xA5, sizeof(sentinel));                               \
        for (unsigned r = 0; r < 256; ++r) {                                    \
            for (size_t len = 127; len <= 129; ++len) {                         \
                openxcvr_status expected = !accepted(r)                         \
                    ? OPENXCVR_ERR_UNSUPPORTED_REVISION                         \
                    : len < 128 ? OPENXCVR_ERR_SHORT_BUFFER : OPENXCVR_OK;       \
                memcpy(&out, &sentinel, sizeof(out));                            \
                assert(openxcvr_decode_##suffix##_for_revision(                 \
                    (uint8_t)r, buf, len, &out) == expected);                    \
                if (expected != OPENXCVR_OK)                                    \
                    assert(memcmp(&out, &sentinel, sizeof(out)) == 0);           \
                else                                                            \
                    assert(out.cmis_rev_byte == r);                             \
                memcpy(&out, &sentinel, sizeof(out));                            \
                assert(openxcvr_decode_##suffix##_for_revision(                 \
                    (uint8_t)r, NULL, len, &out) ==                              \
                    OPENXCVR_ERR_INVALID_ARGUMENT);                             \
                assert(memcmp(&out, &sentinel, sizeof(out)) == 0);               \
                assert(openxcvr_decode_##suffix##_for_revision(                 \
                    (uint8_t)r, buf, len, NULL) ==                               \
                    OPENXCVR_ERR_INVALID_ARGUMENT);                             \
                assert(openxcvr_decode_##suffix##_for_revision(                 \
                    (uint8_t)r, NULL, len, NULL) ==                              \
                    OPENXCVR_ERR_INVALID_ARGUMENT);                             \
            }                                                                   \
        }                                                                       \
    } while (0)

static void admission(void) {
    uint8_t buf[129] = {0};
    ADMISSION(cmis_page01h_advertising);
    ADMISSION(cmis_page11h_lane_status);
}

static void landmarks(void) {
    const uint8_t revs[] = {0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54};
    uint8_t buf[128];
    for (unsigned pattern = 0; pattern < 2; ++pattern) {
        memset(buf, pattern ? 0xFF : 0, sizeof(buf));
        for (size_t r = 0; r < sizeof(revs); ++r) {
            uint8_t rev = revs[r];
            openxcvr_cmis_page01h_advertising_checked a;
            openxcvr_cmis_page11h_lane_status_checked s;
            memset(&a, 0xA5, sizeof(a));
            memset(&s, 0xA5, sizeof(s));
            assert(openxcvr_decode_cmis_page01h_advertising_for_revision(
                rev, buf, sizeof(buf), &a) == OPENXCVR_OK);
            assert(openxcvr_decode_cmis_page11h_lane_status_for_revision(
                rev, buf, sizeof(buf), &s) == OPENXCVR_OK);
#define A(member, since, nonzero)                                                \
            assert(a.present.member == (rev >= since));                         \
            assert(a.values.member ==                                          \
                ((rev >= since && pattern) ? (nonzero) : 0))
            A(module_inactive_firmware_major_revision, 0x40, 255);
            A(module_inactive_firmware_minor_revision, 0x40, 255);
            A(cdb_instances_supported, 0x40, 3);
            A(e_pps_supported, 0x50, 1);
            A(timing_page15h_supported, 0x50, 1);
            A(network_path_pages_supported, 0x51, 1);
            A(bank_broadcast_supported, 0x52, 1);
            A(password_entry_supported, 0x53, 3);
            A(full_page_read_supported, 0x53, 3);
#undef A
            /* Zero input still marks present fields present; nonzero input
             * never leaks absent values. Arrays have one presence flag. */
            assert(a.present.host_interface_id_app);
            assert(a.present.media_lane_count_app);
            for (size_t i = 0; i < 7; ++i) {
                assert(a.values.host_interface_id_app[i] == (pattern ? 255 : 0));
                assert(a.values.media_lane_count_app[i] == (pattern ? 15 : 0));
            }
            assert(s.present.dp_state_host_lane);
            assert(s.present.optical_power_tx);
#define S(member)                                                               \
            assert(s.present.member == (rev >= 0x50));                          \
            for (size_t i = 0; i < 8; ++i)                                      \
                assert(s.values.member[i] == (rev >= 0x50 && pattern))
            S(output_status_rx);
            S(output_status_tx);
            S(output_status_changed_flag_rx);
            S(dp_init_pending_lane);
#undef S
            for (size_t i = 0; i < 8; ++i) {
                assert(s.values.dp_state_host_lane[i] == (pattern ? 15 : 0));
                assert(s.values.optical_power_tx[i] == (pattern ? 65535 : 0));
            }
        }
    }
}

/* In-place overlap is allowed: every read must finish before *out is
 * published. Character access to the object representation is valid C. */
static void overlap(void) {
    openxcvr_cmis_page01h_advertising_checked a;
    openxcvr_cmis_page11h_lane_status_checked s;
    assert(sizeof(a) >= 128 && sizeof(s) >= 128);
    memset(&a, 0xFF, sizeof(a));
    assert(openxcvr_decode_cmis_page01h_advertising_for_revision(
        0x53, (const uint8_t *)&a, 128, &a) == OPENXCVR_OK);
    assert(a.cmis_rev_byte == 0x53);
    assert(a.values.module_inactive_firmware_major_revision == 255);
    assert(a.values.page_checksum == 255);
    for (size_t i = 0; i < 7; ++i) {
        assert(a.values.host_interface_id_app[i] == 255);
        assert(a.values.media_lane_count_app[i] == 15);
    }
    memset(&s, 0xFF, sizeof(s));
    assert(openxcvr_decode_cmis_page11h_lane_status_for_revision(
        0x30, (const uint8_t *)&s, 128, &s) == OPENXCVR_OK);
    assert(s.cmis_rev_byte == 0x30);
    assert(!s.present.dp_init_pending_lane);
    for (size_t i = 0; i < 8; ++i) {
        assert(s.values.dp_state_host_lane[i] == 15);
        assert(s.values.media_lane_to_fiber_mapping_rx[i] == 15);
        assert(!s.values.dp_init_pending_lane[i]);
    }
}

static void dpsm(void) {
    const uint8_t revs[] = {0x30, 0x53};
    /* Test all raw nibbles, including 0, 3, 4, 5, 6, 7 and high reserved
     * values. No 3.0 remapping: 3 Deinit and 4 Activated at both revisions. */
    const char *names[] = {"Reserved0", "DpDeactivated", "DpInit", "DpDeinit",
        "DpActivated", "DpTxTurnOn", "DpTxTurnOff", "DpInitialized"};
    for (size_t r = 0; r < sizeof(revs); ++r) {
        uint8_t rev = revs[r];
        openxcvr_cmis_data_path_state_projection project =
            openxcvr_cmis_data_path_state_for_revision(rev);
        assert(project != NULL);
        for (uint8_t raw = 0; raw < 16; ++raw) {
            uint8_t buf[128] = {0};
            openxcvr_cmis_page11h_lane_status_checked out;
            memset(buf, raw | (raw << 4), 4);
            assert(openxcvr_decode_cmis_page11h_lane_status_for_revision(
                rev, buf, sizeof(buf), &out) == OPENXCVR_OK);
            assert(out.present.dp_state_host_lane);
            for (size_t i = 0; i < 8; ++i) {
                uint8_t value = out.values.dp_state_host_lane[i];
                assert(value == raw);
                if (raw <= (rev == 0x30 ? 4 : 7))
                    assert(strcmp(project(value), names[raw]) == 0);
                else
                    assert(project(value) == NULL);
                for (int tx = 0; tx <= 1; ++tx) {
                    for (int pending = 0; pending <= 1; ++pending) {
                        int terminal = -1;
                        assert(openxcvr_cmis_dpsm_is_terminal_for_rev(
                            rev, value, tx, pending, &terminal) == OPENXCVR_OK);
                        assert(terminal == (raw == 4 ||
                            (rev == 0x53 && raw == 7 && (tx || pending))));
                    }
                }
            }
        }
    }
}

int main(void) {
    admission();
    landmarks();
    overlap();
    dpsm();
    puts("checked pages: both pages x 256 revision bytes; atomic errors, landmarks, arrays, overlap, raw DPSM passed");
    return 0;
}
