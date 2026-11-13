/*
 * Implicit sub-byte width regression (CMIS Page 01h advertising):
 * media_lane_count_app is a u4 with implicit width (no recorded bit_width);
 * its decode must mask to the wire-type nibble width, not the byte-wide
 * legacy default. Repro: raw byte 0xA5 with high bits set must decode as
 * media lane count 5, not 165, in all seven application slots.
 *
 * host_lane_count_app (explicit bit_width 4, bit_offset 4) guards the
 * explicit-width path: its high nibble 0xA decodes as 10.
 */
#include "openxcvr/openxcvr.h"

#include <stdio.h>

static int failures = 0;

#define CHECK(cond, ...)                                                       \
    do {                                                                       \
        if (!(cond)) {                                                         \
            failures++;                                                        \
            fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__);                \
            fprintf(stderr, __VA_ARGS__);                                       \
            fprintf(stderr, "\n");                                             \
        }                                                                      \
    } while (0)

int main(void) {
    uint8_t buf[128];
    for (size_t i = 0; i < sizeof(buf); i++) {
        buf[i] = 0x00;
    }

    /* CMIS 5.3 8.4.15: MediaLaneCountApp<app> at flat 225 + 4*(app-1),
     * bits 3:0; HostLaneCountApp<app> at the same flat byte, bits 7:4.
     * Flat 225..249 maps to page buffer bytes 97..121. High bits set:
     * 0xA5 = host nibble 0xA, media nibble 0x5. */
    static const size_t lane_bytes[7] = {97, 101, 105, 109, 113, 117, 121};
    for (size_t i = 0; i < 7; i++) {
        buf[lane_bytes[i]] = 0xA5;
    }

    openxcvr_cmis_page01h_advertising out;
    openxcvr_status status =
        openxcvr_decode_cmis_page01h_advertising(buf, sizeof(buf), &out);
    CHECK(status == OPENXCVR_OK, "decode failed: %s",
          openxcvr_status_string(status));
    if (status != OPENXCVR_OK) {
        return 1;
    }

    for (size_t i = 0; i < 7; i++) {
        CHECK(out.media_lane_count_app[i] == 5,
              "media_lane_count_app[%zu] = %u (expected 5; raw 0xA5 leaked "
              "high bits through a byte-wide mask)",
              i, (unsigned)out.media_lane_count_app[i]);
        CHECK(out.host_lane_count_app[i] == 10,
              "host_lane_count_app[%zu] = %u (expected 10)",
              i, (unsigned)out.host_lane_count_app[i]);
    }

    if (failures != 0) {
        fprintf(stderr, "%d failure(s)\n", failures);
        return 1;
    }
    return 0;
}
