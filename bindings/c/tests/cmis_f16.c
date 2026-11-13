/*
 * CMIS / SFF-8636 F16 conversion: exhaustive FNV-1a-64 digest over the
 * binary64 bit patterns of all 65,536 codes (must equal the exact
 * reference digest), a per-code strtod oracle and the golden bit
 * patterns, mirroring crates/openxcvr-core/tests/cmis_f16.rs.
 */
#include "openxcvr/observation.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint64_t bits_of(double v) {
    uint64_t bits;
    memcpy(&bits, &v, sizeof bits);
    return bits;
}

/* FNV-1a-64 over each result's bit pattern, emitted byte-by-byte in
 * little-endian order via shifts — the same byte stream as the exact
 * reference and the Rust digest test. */
static void test_exhaustive_digest(void) {
    uint64_t h = 0xcbf29ce484222325ULL;
    for (unsigned c = 0; c < 65536; c++) {
        uint64_t bits = bits_of(openxcvr_cmis_f16_to_double((uint16_t)c));
        for (int i = 0; i < 8; i++) {
            h ^= (bits >> (8 * i)) & 0xffu;
            h *= 0x100000001b3ULL;
        }
    }
    if (h != 0x98194d0508bcca80ULL) {
        fprintf(stderr,
                "FAIL digest: got %016llx, want 98194d0508bcca80\n",
                (unsigned long long)h);
        exit(1);
    }
}

/* Per-code oracle: the conversion must be bit-identical to
 * strtod("<m>e<e>") on this platform. Reports the first mismatch. */
static void test_strtod_oracle(void) {
    char buf[32];
    for (unsigned c = 0; c < 65536; c++) {
        int m = (int)(c & 0x7ffu);
        int e = (int)(c >> 11) - 24;
        snprintf(buf, sizeof buf, "%de%d", m, e);
        uint64_t got = bits_of(openxcvr_cmis_f16_to_double((uint16_t)c));
        uint64_t want = bits_of(strtod(buf, NULL));
        if (got != want) {
            fprintf(stderr,
                    "FAIL oracle: first mismatching code %04x: m=%d e=%d, "
                    "got %016llx, strtod(\"%s\") %016llx\n",
                    c, m, e, (unsigned long long)got, buf,
                    (unsigned long long)want);
            exit(1);
        }
    }
}

/* Golden binary64 bit patterns, shared with the Rust tests. */
static void test_goldens(void) {
    static const struct {
        uint16_t code;
        uint64_t bits;
    } goldens[] = {
        {0x0000, 0x0000000000000000ULL},
        {0xF800, 0x0000000000000000ULL},
        {0x07FF, 0x3ba35557a1555996ULL},
        {0x0801, 0x3b282db34012b251ULL},
        {0x1001, 0x3b5e392010175ee6ULL},
        {0x6001, 0x3d719799812dea11ULL},
        {0xC001, 0x3ff0000000000000ULL},
        {0xC00A, 0x4024000000000000ULL},
        {0xFFFF, 0x4213106da6000000ULL},
    };
    for (size_t i = 0; i < sizeof goldens / sizeof goldens[0]; i++) {
        uint64_t got = bits_of(openxcvr_cmis_f16_to_double(goldens[i].code));
        if (got != goldens[i].bits) {
            fprintf(stderr,
                    "FAIL golden %04x: got %016llx, want %016llx\n",
                    goldens[i].code, (unsigned long long)got,
                    (unsigned long long)goldens[i].bits);
            exit(1);
        }
    }
}

int main(void) {
    test_exhaustive_digest();
    test_strtod_oracle();
    test_goldens();
    return 0;
}
