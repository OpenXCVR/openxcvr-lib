/* Native tests for the generated revision-bound CMIS DPSM name projection
   (openxcvr_cmis_data_path_state_for_revision, schema-owned).

   The expected tables below are an independent literal oracle transcribed
   from the spec tables (CMIS 3.0 § 1.7.7.1 Table 66; CMIS 4.0 Table 8-58;
   CMIS 5.3 § 8.10.1 / Table 8-84), not values derived from a decoder, the
   generated table, a raw adapter or runtime case conversion. NULL means "no
   name for this raw at this revision" -- the caller renders its own
   Unknown<n> fallback. */

#include "enums/cmis_data_path_state.h"
#include "pages/cmis_page11h_lane_status.h"

#include <assert.h>
#include <stddef.h>
#include <string.h>

/* Expected name per raw nibble at CMIS 3.0 (Table 66: raws 5-7 reserved). */
static const char *const V3_0[16] = {
    "Reserved0", "DpDeactivated", "DpInit", "DpDeinit", "DpActivated",
    NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
};

/* Expected name per raw nibble at every catalog revision from 4.0 on. */
static const char *const V4_0_PLUS[16] = {
    "Reserved0", "DpDeactivated", "DpInit", "DpDeinit", "DpActivated",
    "DpTxTurnOn", "DpTxTurnOff", "DpInitialized",
    NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
};

/* The six known CMIS context bytes (packed-BCD major/minor). */
static const uint8_t CATALOG_BYTES[7] = {0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54};

static const char *oracle(unsigned rev, unsigned raw) {
    const char *const *table = (rev == 0x30u) ? V3_0 : V4_0_PLUS;
    return table[raw];
}

static int is_catalog(unsigned byte) {
    for (size_t i = 0; i < sizeof(CATALOG_BYTES); i++) {
        if (CATALOG_BYTES[i] == byte) {
            return 1;
        }
    }
    return 0;
}

int main(void) {
    /* Factory support domain: exactly the six catalog bytes, no second
       whitelist anywhere else in the loop. */
    for (unsigned byte = 0; byte <= 255u; byte++) {
        openxcvr_cmis_data_path_state_projection project =
            openxcvr_cmis_data_path_state_for_revision((uint8_t)byte);
        assert((project != NULL) == is_catalog(byte));
    }

    /* 112 exact-string cases: every catalog revision x every raw nibble. */
    {
        int cases = 0;
        for (size_t i = 0; i < sizeof(CATALOG_BYTES); i++) {
            const unsigned rev = CATALOG_BYTES[i];
            openxcvr_cmis_data_path_state_projection project =
                openxcvr_cmis_data_path_state_for_revision((uint8_t)rev);
            assert(project != NULL);
            for (unsigned raw = 0; raw < 16u; raw++) {
                const char *want = oracle(rev, raw);
                const char *got = project((uint8_t)raw);
                if (want == NULL) {
                    assert(got == NULL);
                } else {
                    assert(got != NULL);
                    assert(strcmp(got, want) == 0);
                }
                cases++;
            }
        }
        assert(cases == 112);
    }

    /* Unsupported revision contexts refuse (NULL): 3.1, 4.1, 5.4, 0x00, 0xFF. */
    {
        const unsigned refused[] = {0x31u, 0x41u, 0x55u, 0x00u, 0xFFu};
        for (size_t i = 0; i < sizeof(refused) / sizeof(refused[0]); i++) {
            assert(openxcvr_cmis_data_path_state_for_revision((uint8_t)refused[i]) == NULL);
        }
    }

    /* Unnamed raws: reserved range 8-15 at every revision, plus 16 and 255. */
    {
        const unsigned raws[] = {8u, 9u, 10u, 11u, 12u, 13u, 14u, 15u, 16u, 255u};
        for (size_t i = 0; i < sizeof(CATALOG_BYTES); i++) {
            openxcvr_cmis_data_path_state_projection project =
                openxcvr_cmis_data_path_state_for_revision(CATALOG_BYTES[i]);
            assert(project != NULL);
            for (size_t r = 0; r < sizeof(raws) / sizeof(raws[0]); r++) {
                assert(project((uint8_t)raws[r]) == NULL);
            }
        }
    }

    /* Two handles for different revisions interleave without shared state. */
    {
        openxcvr_cmis_data_path_state_projection v3 =
            openxcvr_cmis_data_path_state_for_revision(0x30);
        openxcvr_cmis_data_path_state_projection v5 =
            openxcvr_cmis_data_path_state_for_revision(0x53);
        assert(v3 != NULL && v5 != NULL);
        for (unsigned raw = 0; raw < 16u; raw++) {
            const char *want3 = oracle(0x30, raw);
            const char *want5 = oracle(0x53, raw);
            const char *got3 = v3((uint8_t)raw);
            const char *got5 = v5((uint8_t)raw);
            assert((got3 == NULL) == (want3 == NULL));
            if (want3 != NULL) {
                assert(strcmp(got3, want3) == 0);
            }
            assert((got5 == NULL) == (want5 == NULL));
            if (want5 != NULL) {
                assert(strcmp(got5, want5) == 0);
            }
            /* Interleave again: same answers, no cross-handle state. */
            const char *again3 = v3((uint8_t)raw);
            assert((again3 == NULL) == (want3 == NULL));
            if (want3 != NULL) {
                assert(strcmp(again3, want3) == 0);
            }
        }
        assert(v3(0x05) == NULL);
        assert(v5(0x05) != NULL && strcmp(v5(0x05), "DpTxTurnOn") == 0);
    }

    /* Existing revision-blind flat controls: generated constants keep their
       raw values. */
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_RESERVED_0 == 0x00);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_DEACTIVATED == 0x01);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_INIT == 0x02);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_DEINIT == 0x03);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED == 0x04);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_TX_TURN_ON == 0x05);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_TX_TURN_OFF == 0x06);
    assert(OPENXCVR_CMIS_DATA_PATH_STATE_DP_INITIALIZED == 0x07);

    /* Existing flat Page 11h nibble extraction is unchanged: two nibbles per
       byte, low lane first, and the projection names those flat nibbles the
       same way at every lane position. */
    {
        uint8_t buf[128];
        memset(buf, 0, sizeof(buf));
        buf[0] = 0x51u; /* lane 1 = 1, lane 2 = 5 */
        buf[1] = 0x42u; /* lane 3 = 2, lane 4 = 4 */
        buf[2] = 0xF0u; /* lane 5 = 0, lane 6 = 15 */
        buf[3] = 0x83u; /* lane 7 = 3, lane 8 = 8 */
        openxcvr_cmis_page11h_lane_status status;
        assert(openxcvr_decode_cmis_page11h_lane_status(buf, sizeof(buf), &status) == OPENXCVR_OK);
        const uint8_t want[8] = {1, 5, 2, 4, 0, 15, 3, 8};
        openxcvr_cmis_data_path_state_projection project =
            openxcvr_cmis_data_path_state_for_revision(0x53);
        assert(project != NULL);
        const char *const want_names[8] = {
            "DpDeactivated", "DpTxTurnOn", "DpInit", "DpActivated",
            "Reserved0", NULL, "DpDeinit", NULL,
        };
        for (int lane = 0; lane < 8; lane++) {
            assert(status.dp_state_host_lane[lane] == want[lane]);
            const char *name = project(status.dp_state_host_lane[lane]);
            assert((name == NULL) == (want_names[lane] == NULL));
            if (name != NULL) {
                assert(strcmp(name, want_names[lane]) == 0);
            }
        }
    }

    return 0;
}
