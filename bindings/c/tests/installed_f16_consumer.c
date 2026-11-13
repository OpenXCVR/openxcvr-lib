/*
 * Installed-consumer F16 test (C): the F16 surface an installed openxcvr-c
 * prefix must provide, used exactly as an outside consumer would use it.
 * Built twice by the openxcvr_c_installed_f16_consumer test script from
 * the installed prefix only — statically against libopenxcvr_c.a with the
 * flags `pkg-config --static --libs openxcvr-c` names beyond the library
 * itself, and dynamically with `pkg-config --cflags --libs openxcvr-c` —
 * and run in both builds.
 *
 * (a) openxcvr_cmis_f16_to_double on the slice-1 golden codes, compared
 *     by bit pattern: the conversion contract is exact, not approximate.
 * (b) media_side_ber resolved through the installed catalog API and
 *     observed on a synthetic CMIS 5.1 lower page plus a synthetic Page
 *     14h window with selector 01h and matching acquisition evidence:
 *     Valid, Scaled, ratio, the expected value and the F16 code in raw.
 *     Synthetic bytes only — no fixture is read.
 */
#include "openxcvr/catalog.h"
#include "openxcvr/observation.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(cond) do { \
    if (!(cond)) { \
        fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
        exit(1); \
    } \
} while (0)

static uint64_t bits_of(double v) {
    uint64_t bits;
    memcpy(&bits, &v, sizeof bits);
    return bits;
}

static double double_of(uint64_t bits) {
    double value;
    memcpy(&value, &bits, sizeof value);
    return value;
}

/* The slice-1 golden codes and binary64 bit patterns, transcribed from
 * the design (not from any decoder run): 0x0000 and 0xF800 are both 0.0
 * (distinct codes, same value), 0xC001 is 1.0, 0xC00A is 10.0, 0xFFFF is
 * 2.047e10 — a value above 1 stays a ratio. */
static const uint16_t golden_codes[9] = {
    0x0000, 0xF800, 0x07FF, 0x0801, 0x1001, 0x6001, 0xC001, 0xC00A, 0xFFFF
};
static const uint64_t golden_bits[9] = {
    0x0000000000000000ull, 0x0000000000000000ull, 0x3ba35557a1555996ull,
    0x3b282db34012b251ull, 0x3b5e392010175ee6ull, 0x3d719799812dea11ull,
    0x3ff0000000000000ull, 0x4024000000000000ull, 0x4213106da6000000ull
};

/* (a) the conversion on the goldens, by bits. */
static void f16_goldens_by_bits(void) {
    for (size_t i = 0; i < sizeof golden_codes / sizeof golden_codes[0]; ++i) {
        const uint64_t got = bits_of(openxcvr_cmis_f16_to_double(golden_codes[i]));
        if (got != golden_bits[i]) {
            fprintf(stderr, "FAIL golden %04x: got %016llx, want %016llx\n",
                golden_codes[i], (unsigned long long)got,
                (unsigned long long)golden_bits[i]);
            exit(1);
        }
    }
}

/* The eight lane codes the synthetic Page 14h window carries (lanes 1..8)
 * and their expected binary64 bit patterns — goldens covering zero, one,
 * ten, a value above 1 and the smallest magnitudes. */
static const uint16_t lane_codes[8] = {
    0x0000, 0xC001, 0xFFFF, 0x6001, 0x0801, 0x07FF, 0xF800, 0xC00A
};
static const uint64_t lane_bits[8] = {
    0x0000000000000000ull, 0x3ff0000000000000ull, 0x4213106da6000000ull,
    0x3d719799812dea11ull, 0x3b282db34012b251ull, 0x3ba35557a1555996ull,
    0x0000000000000000ull, 0x4024000000000000ull
};

/* (b) media_side_ber through the installed catalog API: a synthetic CMIS
 * 5.1 lower page (byte 1 = 0x51) plus a synthetic Page 14h window
 * (selector 01h at offset 128, the lane codes in both BER views), with
 * matching selector-acquisition evidence covering bytes 192-255. */
static void media_side_ber_observation(void) {
    unsigned char lower[128];
    memset(lower, 0, sizeof lower);
    lower[1] = 0x51;

    unsigned char page14[128];
    memset(page14, 0, sizeof page14);
    page14[0] = 0x01;
    for (size_t lane = 0; lane < 8; ++lane) {
        const uint16_t code = lane_codes[lane];
        /* The supplied window starts at offset 128: host view 192-207,
         * media view 208-223. */
        page14[64 + 2 * lane] = (unsigned char)(code >> 8);
        page14[64 + 2 * lane + 1] = (unsigned char)(code & 0xff);
        page14[80 + 2 * lane] = (unsigned char)(code >> 8);
        page14[80 + 2 * lane + 1] = (unsigned char)(code & 0xff);
    }

    openxcvr_range_observation parts[2];
    memset(parts, 0, sizeof parts);
    parts[0].range = (openxcvr_range){0x50, 0, 0x00, 0, 128};
    parts[0].bytes = lower;
    parts[0].len = sizeof lower;
    parts[0].present = 1;
    parts[1].range = (openxcvr_range){0x50, 0, 0x14, 128, 128};
    parts[1].bytes = page14;
    parts[1].len = sizeof page14;
    parts[1].present = 1;

    /* Caller-asserted acquisition evidence for the selector-stable window
     * that carried the BER view (its data range covers bytes 192-255,
     * including media_side_ber's own 208-223). */
    openxcvr_selector_acquisition_t evidence[1];
    memset(evidence, 0, sizeof evidence);
    evidence[0].generation = 1;
    evidence[0].epoch = 2;
    evidence[0].address = 0x50;
    evidence[0].bank = 0;
    evidence[0].page = 0x14;
    evidence[0].selector_offset = 128;
    evidence[0].selector_value = 0x01;
    evidence[0].data_range = (openxcvr_range){0x50, 0, 0x14, 192, 64};
    const openxcvr_acquisition_context_t ctx = {1, evidence};

    openxcvr_field_query_t query = {NULL, OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    query.pattern = "media_side_ber";
    query.lane = OPENXCVR_LANE_ALL;
    query.page = "14h";
    openxcvr_resolved_field_t entries[8];
    size_t count = 99;
    CHECK(openxcvr_catalog_resolve(
        OPENXCVR_SPEC_CMIS, 5, 1, &query, entries, 8, &count) == OPENXCVR_OK);
    CHECK(count == 8);
    for (size_t i = 0; i < count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(
            &entries[i], parts, 2, NULL, &ctx, &obs) == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_VALID);
        CHECK(obs.has_value == 1);
        CHECK(obs.value.is_scaled == 1);
        CHECK(obs.value.unit == OPENXCVR_UNIT_RATIO);
        CHECK(obs.value.value == double_of(lane_bits[i]));
        /* The 16-bit F16 code rides along with the scaled value. */
        CHECK(obs.value.raw == lane_codes[i]);
    }
}

int main(void) {
    f16_goldens_by_bits();
    media_side_ber_observation();
    return 0;
}
