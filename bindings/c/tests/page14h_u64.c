/*
 * CMIS Page 14h u64le member: HostSideErrorCountLane1
 * (host_side_error_count_lane_1_4[0]) is flat bytes 192-199, file offset 64
 * of the 128-byte upper-page buffer. Wire bytes 80 00 00 00 00 00 00 01 are
 * little-endian (LSB at the lowest address), the same order as
 * OPENXCVR_FIELD_U64LE in observation.c.
 *
 * That wire image decodes to 0x0100000000000080 (72057594037928064). The
 * opposite byte order is 0x8000000000000001 (9223372036854775809). A swapped
 * order, a cast applied after a 32-bit shift, or a read that drops the high
 * bytes must not produce the little-endian value.
 */
#include "openxcvr/openxcvr.h"

#include <stdio.h>
#include <string.h>

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
    memset(buf, 0, sizeof buf);

    /* Wire order, lowest address first. */
    static const uint8_t wire[8] = {0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01};
    memcpy(buf + 64, wire, sizeof wire);

    openxcvr_cmis_page14h_diagnostics_results out;
    memset(&out, 0, sizeof out);
    openxcvr_status status =
        openxcvr_decode_cmis_page14h_diagnostics_results(buf, sizeof buf, &out);
    CHECK(status == OPENXCVR_OK, "decode failed: %s", openxcvr_status_string(status));
    if (status != OPENXCVR_OK) {
        return 1;
    }

    const uint64_t got = out.host_side_error_count_lane_1_4[0];
    const uint64_t expected = UINT64_C(0x0100000000000080);
    const uint64_t swapped = UINT64_C(0x8000000000000001);
    CHECK(got == expected,
          "host_side_error_count_lane_1_4[0] = 0x%016llx (%llu), expected "
          "0x0100000000000080 (72057594037928064)",
          (unsigned long long)got, (unsigned long long)got);
    CHECK(got == UINT64_C(72057594037928064),
          "decimal form of the little-endian value mismatched");
    CHECK(got != swapped,
          "decoded the big-endian reading 9223372036854775809");
    CHECK(expected != swapped, "oracle collapsed both byte orders");

    if (failures != 0) {
        fprintf(stderr, "%d failure(s)\n", failures);
        return 1;
    }
    return 0;
}
