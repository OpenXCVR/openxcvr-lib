#include "openxcvr/openxcvr.h"
#include "pages/cmis_page00h_administrative.h"

_Static_assert(
    sizeof(struct openxcvr_cmis_page00h_administrative) == 101,
    "openxcvr_cmis_page00h_administrative size pin (condition C4)"
);
#include "pages/cmis_page01h_advertising.h"
#include "pages/cmis_page11h_lane_status.h"
#include "pages/page_a0h_base_memory.h"
#include "pages/sff8636_lower_memory.h"
#include "pages/sff8636_page00h_upper_administrative.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <process.h>
#else
#include <unistd.h>
#endif

static int run_self_test(void) {
    size_t written = 123;
    openxcvr_status status = openxcvr_decode_cmis_json(NULL, NULL, 0, &written);
    if (status != OPENXCVR_ERR_INVALID_ARGUMENT) {
        fprintf(stderr, "expected invalid argument, got %s\n", openxcvr_status_string(status));
        return 1;
    }
    if (written != 0) {
        fprintf(stderr, "expected written=0, got %lu\n", (unsigned long)written);
        return 1;
    }
    if (strcmp(openxcvr_status_string(OPENXCVR_OK), "ok") != 0) {
        fprintf(stderr, "unexpected status string\n");
        return 1;
    }
    puts("C parity scaffold: self-test passed");
    return 0;
}

static int write_text_file(const char *path, const char *data) {
    FILE *file = fopen(path, "wb");
    if (file == NULL) {
        perror(path);
        return 1;
    }
    if (fwrite(data, 1, strlen(data), file) != strlen(data)) {
        perror(path);
        fclose(file);
        return 1;
    }
    if (fclose(file) != 0) {
        perror(path);
        return 1;
    }
    return 0;
}

/* The Python driver discovers goldens using metadata from this table;
 * SDK loading, sizing and decoding remain in C. */
typedef struct {
    const char *label;
    const char *golden_dir;
    const char *fixture_dir;
    openxcvr_status (*load)(const char *dir, const char *stem, void *out);
    openxcvr_status (*decode)(const void *pages, char *buf, size_t len, size_t *written);
    size_t pages_size;
} family_t;

static openxcvr_status load_cmis(const char *dir, const char *stem, void *out) {
    return openxcvr_cmis_pages_load(dir, stem, out);
}

static openxcvr_status decode_cmis(const void *pages, char *buf, size_t len, size_t *written) {
    return openxcvr_decode_cmis_json((const openxcvr_cmis_pages *)pages, buf, len, written);
}

static openxcvr_status load_sff8636(const char *dir, const char *stem, void *out) {
    return openxcvr_sff8636_pages_load(dir, stem, out);
}

static openxcvr_status decode_sff8636(const void *pages, char *buf, size_t len, size_t *written) {
    return openxcvr_decode_sff8636_json((const openxcvr_sff8636_pages *)pages, buf, len, written);
}

static openxcvr_status load_sff8472(const char *dir, const char *stem, void *out) {
    return openxcvr_sff8472_pages_load(dir, stem, out);
}

static openxcvr_status decode_sff8472(const void *pages, char *buf, size_t len, size_t *written) {
    return openxcvr_decode_sff8472_json((const openxcvr_sff8472_pages *)pages, buf, len, written);
}

static const family_t FAMILIES[] = {
    { "cmis", "tests/golden/cmis", "tests/fixtures/cmis", load_cmis, decode_cmis, sizeof(openxcvr_cmis_pages) },
    { "sff-8636", "tests/golden/sff-8636", "tests/fixtures/sff-8636", load_sff8636, decode_sff8636, sizeof(openxcvr_sff8636_pages) },
    { "sff-8472", "tests/golden/sff-8472", "tests/fixtures/sff-8472", load_sff8472, decode_sff8472, sizeof(openxcvr_sff8472_pages) },
};

static int run_one_fixture(const family_t *family, const char *fixture_dir,
                           const char *stem, const char *actual_path) {
    openxcvr_status status;
    void *pages = malloc(family->pages_size);
    if (pages == NULL) {
        fprintf(stderr, "%s: malloc failed\n", stem);
        return 1;
    }
    status = family->load(fixture_dir, stem, pages);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "%s: load failed: %s\n", stem, openxcvr_status_string(status));
        free(pages);
        return 1;
    }

    size_t needed = 0;
    status = family->decode(pages, NULL, 0, &needed);
    if (status != OPENXCVR_ERR_SHORT_BUFFER && status != OPENXCVR_OK) {
        fprintf(stderr, "%s: size pass failed: %s\n", stem, openxcvr_status_string(status));
        free(pages);
        return 1;
    }

    char *json = (char *)malloc(needed);
    if (json == NULL) {
        fprintf(stderr, "%s: malloc failed\n", stem);
        free(pages);
        return 1;
    }
    status = family->decode(pages, json, needed, &needed);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "%s: decode failed: %s\n", stem, openxcvr_status_string(status));
        free(json);
        free(pages);
        return 1;
    }

    int failed = write_text_file(actual_path, json);
    free(json);
    free(pages);
    return failed;
}

/*
 * Regression test for issue #195: instanced (per-lane) fields that carry a
 * per-instance `bit_offset_expr` must decode each lane from its own bit slice,
 * not collapse every lane to the same static shift.
 *
 * CMIS Page 11h packs two lanes' data-path state into one byte (a nibble each):
 * `dp_state_host_lane` has offset_expr "128 + (lane - 1) / 2" and
 * bit_offset_expr "((lane - 1) % 2) * 4", so even lanes read the low nibble
 * (>> 0) and odd lanes the high nibble (>> 4). The per-lane status flags
 * (e.g. dp_state_changed_flag) pack all 8 lanes into one byte via
 * bit_offset_expr "lane - 1".
 *
 * The real fixtures use start byte 0x11 0x11 ... so both nibbles equal 1 and
 * the bug is invisible. This synthetic buffer uses distinct nibbles per lane
 * so the wrong-shift bug is observable: buggy code yields {1,1,3,3,5,5,7,7}
 * for dp_state_host_lane; correct code yields {1,2,3,4,5,6,7,8}.
 */
static int run_lane_decode_test(void) {
    uint8_t buf[128];
    memset(buf, 0, sizeof(buf));

    /* dp_state_host_lane bytes: low nibble = even lane, high nibble = odd lane. */
    buf[0] = 0x21; /* lane 1 -> 1, lane 2 -> 2 */
    buf[1] = 0x43; /* lane 3 -> 3, lane 4 -> 4 */
    buf[2] = 0x65; /* lane 5 -> 5, lane 6 -> 6 */
    buf[3] = 0x87; /* lane 7 -> 7, lane 8 -> 8 */

    /* dp_state_changed_flag is byte file-offset 6, bit = lane - 1.
       0x55 = 0b01010101 -> bits 0,2,4,6 set. */
    buf[6] = 0x55;

    openxcvr_cmis_page11h_lane_status out;
    openxcvr_status status =
        openxcvr_decode_cmis_page11h_lane_status(buf, sizeof(buf), &out);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "lane decode: status %s\n", openxcvr_status_string(status));
        return 1;
    }

    int failed = 0;
    for (int lane = 0; lane < 8; lane++) {
        uint8_t expected = (uint8_t)(lane + 1);
        if (out.dp_state_host_lane[lane] != expected) {
            fprintf(
                stderr,
                "lane decode: dp_state_host_lane[%d] = %u, expected %u\n",
                lane,
                (unsigned)out.dp_state_host_lane[lane],
                (unsigned)expected
            );
            failed = 1;
        }
    }

    for (int lane = 0; lane < 8; lane++) {
        bool expected = (lane % 2) == 0; /* bits 0,2,4,6 set */
        if (out.dp_state_changed_flag[lane] != expected) {
            fprintf(
                stderr,
                "lane decode: dp_state_changed_flag[%d] = %d, expected %d\n",
                lane,
                (int)out.dp_state_changed_flag[lane],
                (int)expected
            );
            failed = 1;
        }
    }

    if (failed) {
        return 1;
    }
    puts("C lane decode (#195): per-lane bit offsets correct");
    return 0;
}

/* Pin the per-lane status latches (rx_los ← LOSFlagRx, Table 8-88; tx_fault ←
 * FailureFlagTx, Table 8-87) on the one fixture whose Page 11h carries real
 * latched values, independently of the deep golden compare:
 * fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched on lanes 1/3 and
 * tx_fault on lanes 2/4, everything else clear (same pin as the go/python
 * focused latch tests). */
static int run_lane_latch_test(void) {
    openxcvr_cmis_pages pages;
    openxcvr_status status = openxcvr_cmis_pages_load(
        "tests/fixtures/cmis", "fboss-cmis-4-0-qsfp112-200g-fr4", &pages);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "lane latches: load failed: %s\n", openxcvr_status_string(status));
        return 1;
    }

    const openxcvr_cmis_page11h_lane_status *p = &pages.page11h;
    int failed = 0;
    for (int lane = 0; lane < 8; lane++) {
        bool want_los = (lane == 0 || lane == 2);
        bool want_fault = (lane == 1 || lane == 3);
        if (p->los_flag_rx[lane] != want_los) {
            fprintf(stderr, "lane latches: los_flag_rx[%d] = %d, expected %d\n",
                    lane + 1, (int)p->los_flag_rx[lane], (int)want_los);
            failed = 1;
        }
        if (p->failure_flag_tx[lane] != want_fault) {
            fprintf(stderr, "lane latches: failure_flag_tx[%d] = %d, expected %d\n",
                    lane + 1, (int)p->failure_flag_tx[lane], (int)want_fault);
            failed = 1;
        }
    }
    if (failed) {
        return 1;
    }
    puts("C lane latches: rx_los lanes 1/3, tx_fault lanes 2/4 pinned");
    return 0;
}

/*
 * Behavioral test for checksum recompute functions (sum8).
 *
 * For each schema that carries a sum8 checksum field, we build a synthetic
 * buf filled with a known pattern (0x01 per covered byte), call the generated
 * recompute function, and verify that the checksum byte now equals the wrapping
 * 8-bit sum of the covered region.  The test is self-contained (no fixture
 * files needed) and exercises the generated C for every spec that has checksum
 * metadata:
 *
 *   - CMIS page 00h: single checksum, covers buf[0..93], written to buf[94].
 *   - SFF-8636 page 00h: two checksums, CC_BASE buf[0..62]→buf[63];
 *                                       CC_EXT  buf[64..94]→buf[95].
 *   - SFF-8472 A0h: two checksums (same layout as SFF-8636 above).
 *
 * We zero buf[] first so the checksum byte starts at 0, fill only the covered
 * region with 0x01, then verify buf[chk] == (uint8_t)(count_of_covered_bytes).
 */
static int run_checksum_recompute_test(void) {
    int failed = 0;

    /* --- CMIS page 00h --- */
    /* covers buf[0..93] (94 bytes of 0x01) → expected checksum = 94 = 0x5e */
    {
        uint8_t buf[128];
        memset(buf, 0, sizeof(buf));
        for (size_t i = 0; i < 94; i++) buf[i] = 0x01;
        openxcvr_cmis_page00h_administrative_recompute_checksums(buf);
        if (buf[94] != 0x5e) {
            fprintf(stderr,
                    "checksum recompute: CMIS page00h buf[94] = 0x%02x, expected 0x5e\n",
                    (unsigned)buf[94]);
            failed = 1;
        }
    }

    /* --- SFF-8636 page 00h --- */
    /* CC_BASE covers buf[0..62] (63 bytes) → expected 0x3f at buf[63]         */
    /* CC_EXT  covers buf[64..94] (31 bytes) → expected 0x1f at buf[95]        */
    {
        uint8_t buf[128];
        memset(buf, 0, sizeof(buf));
        for (size_t i = 0; i < 63; i++) buf[i] = 0x01;
        for (size_t i = 64; i < 95; i++) buf[i] = 0x01;
        openxcvr_sff8636_page00h_upper_administrative_recompute_checksums(buf);
        if (buf[63] != 0x3f) {
            fprintf(stderr,
                    "checksum recompute: SFF-8636 page00h CC_BASE buf[63] = 0x%02x, expected 0x3f\n",
                    (unsigned)buf[63]);
            failed = 1;
        }
        if (buf[95] != 0x1f) {
            fprintf(stderr,
                    "checksum recompute: SFF-8636 page00h CC_EXT buf[95] = 0x%02x, expected 0x1f\n",
                    (unsigned)buf[95]);
            failed = 1;
        }
    }

    /* --- SFF-8472 A0h --- */
    /* CC_BASE covers buf[0..62] (63 bytes) → expected 0x3f at buf[63]         */
    /* CC_EXT  covers buf[64..94] (31 bytes) → expected 0x1f at buf[95]        */
    /* (same layout as SFF-8636 — base=0 because size_bytes=256)               */
    {
        uint8_t buf[256];
        memset(buf, 0, sizeof(buf));
        for (size_t i = 0; i < 63; i++) buf[i] = 0x01;
        for (size_t i = 64; i < 95; i++) buf[i] = 0x01;
        openxcvr_page_a0h_base_memory_recompute_checksums(buf);
        if (buf[63] != 0x3f) {
            fprintf(stderr,
                    "checksum recompute: SFF-8472 A0h CC_BASE buf[63] = 0x%02x, expected 0x3f\n",
                    (unsigned)buf[63]);
            failed = 1;
        }
        if (buf[95] != 0x1f) {
            fprintf(stderr,
                    "checksum recompute: SFF-8472 A0h CC_EXT buf[95] = 0x%02x, expected 0x1f\n",
                    (unsigned)buf[95]);
            failed = 1;
        }
    }

    /* --- CMIS page 01h --- */
    /* Covers buf[2..126] (125 bytes) → checksum written to buf[127].           *
     * buf[0] and buf[1] are set to 0xff as a sentinel: they are NOT covered by *
     * the sum and must not contribute to the result.  If the implementation    *
     * incorrectly starts the sum at buf[0], the checksum would be              *
     * 0xff + 0xff + 125 = 0x7b, not the correct 0x7d.                         */
    {
        uint8_t buf[128];
        memset(buf, 0, sizeof(buf));
        buf[0] = 0xff;
        buf[1] = 0xff;
        for (size_t i = 2; i <= 126; i++) buf[i] = 0x01;
        openxcvr_cmis_page01h_advertising_recompute_checksums(buf);
        if (buf[127] != 0x7d) {
            fprintf(stderr,
                    "checksum recompute: CMIS page01h buf[127] = 0x%02x, expected 0x7d\n",
                    (unsigned)buf[127]);
            failed = 1;
        }
    }

    if (!failed) {
        puts("C checksum recompute: all sum8 regions correct");
    }
    return failed ? 1 : 0;
}

/* R2 alarm-only witness (design-r3.md): on a copy of the finisar SFF-8636
 * fixture's lower page with byte 3 cleared (LOS flags all clear) and the
 * interrupt-deasserted bit still set, only the lane alarm/warning nibbles
 * (bytes 9-14) can drive interrupt_asserted. With bytes 9-14 nonzero it must
 * be true; with them also zero, false. Both expectations were confirmed
 * against the Python facade on the same bytes
 * (impl-c/r2-witness-python.log: True / False). */
static int run_alarm_only_witness(void) {
    openxcvr_sff8636_pages pages;
    openxcvr_status status = openxcvr_sff8636_pages_load(
        "tests/fixtures/sff-8636", "finisar-ftl410qe3c-40g-sr4", &pages);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "alarm witness: load failed: %s\n", openxcvr_status_string(status));
        return 1;
    }

    struct case_desc {
        const char *label;
        int nonzero_nibbles;
        const char *expected;
    } cases[] = {
        { "byte3 cleared, bytes 9-14 = 0x55", 1, "true" },
        { "byte3 cleared, bytes 9-14 = 0x00", 0, "false" },
    };

    for (size_t c = 0; c < sizeof(cases) / sizeof(cases[0]); c++) {
        openxcvr_sff8636_pages mutated = pages;
        mutated.lower_raw[3] = 0x00;
        for (int i = 9; i <= 14; i++) {
            mutated.lower_raw[i] = cases[c].nonzero_nibbles ? 0x55 : 0x00;
        }
        status = openxcvr_decode_sff8636_lower_memory(mutated.lower_raw, 128, &mutated.lower);
        if (status != OPENXCVR_OK) {
            fprintf(stderr, "alarm witness: re-decode failed: %s\n", openxcvr_status_string(status));
            return 1;
        }

        size_t needed = 0;
        status = openxcvr_decode_sff8636_json(&mutated, NULL, 0, &needed);
        if (status != OPENXCVR_ERR_SHORT_BUFFER && status != OPENXCVR_OK) {
            fprintf(stderr, "alarm witness: size pass failed: %s\n", openxcvr_status_string(status));
            return 1;
        }
        char *json = (char *)malloc(needed);
        if (json == NULL) {
            fprintf(stderr, "alarm witness: malloc failed\n");
            return 1;
        }
        status = openxcvr_decode_sff8636_json(&mutated, json, needed, &needed);
        if (status != OPENXCVR_OK) {
            fprintf(stderr, "alarm witness: decode failed: %s\n", openxcvr_status_string(status));
            free(json);
            return 1;
        }

        char expected_token[64];
        snprintf(expected_token, sizeof(expected_token), "\"interrupt_asserted\":%s", cases[c].expected);
        int ok = strstr(json, expected_token) != NULL;
        if (!ok) {
            fprintf(stderr, "alarm witness (%s): %s not found in:\n%s\n",
                    cases[c].label, expected_token, json);
        }
        free(json);
        if (!ok) {
            return 1;
        }
        printf("C alarm-only witness: %s -> interrupt_asserted %s\n", cases[c].label, cases[c].expected);
    }
    return 0;
}

/* Review follow-up A2: a read error on an existing page file must fail the
 * SFF-8472 loader instead of silently decoding the zeroed buffer as "ok".
 * On Linux a directory can be fopen'd for reading but every fread on it
 * fails (EISDIR), reaching ferror. Windows may instead fail fopen;
 * both must return ERR_IO. Python prepares isolated directory/short-file
 * cases. A short file stays zero-padded and OK (Python _pad_page). */
static int run_page_read_error(const char *fixture_dir) {
    int failed = 0;
    openxcvr_sff8472_pages pages;
    openxcvr_status status = openxcvr_sff8472_pages_load(
        fixture_dir, "a0h-err", &pages);
    if (status != OPENXCVR_ERR_IO) {
        fprintf(stderr, "page read error: a0h directory: expected ERR_IO, got %s\n",
                openxcvr_status_string(status));
        failed = 1;
    }
    status = openxcvr_sff8472_pages_load(
        fixture_dir, "a2h-err", &pages);
    if (status != OPENXCVR_ERR_IO) {
        fprintf(stderr, "page read error: a2h directory: expected ERR_IO, got %s\n",
                openxcvr_status_string(status));
        failed = 1;
    }

    /* Short files (100 bytes) load zero-padded and OK. */
    status = openxcvr_sff8472_pages_load(
        fixture_dir, "short", &pages);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "page read error: short pages: expected OK, got %s\n",
                openxcvr_status_string(status));
        failed = 1;
    } else if (pages.a0h_raw[100] != 0 || pages.a0h_raw[255] != 0 ||
               pages.a2h_raw[100] != 0 || pages.a2h_raw[255] != 0) {
        fprintf(stderr, "page read error: short pages not zero-padded past 100\n");
        failed = 1;
    }

    if (!failed) {
        puts("C SFF-8472 loader: read error fails, short page pads OK");
    }
    return failed ? 1 : 0;
}

/* No-args remains the full suite. CMake selects the Python interpreter;
 * no shell, PATH-dependent python3, or fixed temporary directory is used. */
static int launch_driver(const char *executable) {
    const char *args[] = {
        OPENXCVR_PARITY_PYTHON, OPENXCVR_PARITY_DRIVER,
        executable, OPENXCVR_PARITY_ROOT, NULL
    };
#ifdef _WIN32
    /* The CRT spawn API concatenates argv without quoting. Quote each path
     * using the Windows CRT rules, including backslashes before quotes/end. */
    char *quoted[sizeof(args) / sizeof(args[0])] = { NULL };
    for (size_t i = 0; args[i] != NULL; i++) {
        quoted[i] = malloc(2 * strlen(args[i]) + 3);
        if (quoted[i] == NULL) {
            for (size_t j = 0; j < i; j++) free(quoted[j]);
            fprintf(stderr, "parity launcher: malloc failed\n");
            return 1;
        }
        char *out = quoted[i];
        *out++ = '"';
        const char *in = args[i];
        while (*in) {
            size_t slashes = 0;
            while (*in == '\\') { slashes++; in++; }
            size_t count = (*in == '"' || *in == '\0') ? 2 * slashes : slashes;
            while (count--) *out++ = '\\';
            if (*in == '"') *out++ = '\\';
            if (*in) *out++ = *in++;
        }
        *out++ = '"';
        *out = '\0';
    }
    intptr_t rc = _spawnv(_P_WAIT, args[0], (const char *const *)quoted);
    for (size_t i = 0; quoted[i] != NULL; i++) free(quoted[i]);
    if (rc != -1) return (int)rc;
#else
    execv(args[0], (char *const *)args);
#endif
    perror("parity Python driver");
    return 1;
}

int main(int argc, char **argv) {
    if (argc == 1) return launch_driver(argv[0]);
    if (argc == 2 && strcmp(argv[1], "--self-test") == 0) {
        return run_self_test();
    }
    /* Internal argv-only protocol used by parity.py. Family metadata has a
     * single owner above; paths/stems come from the driver's discovery. */
    if (argc == 2 && strcmp(argv[1], "--families") == 0) {
        for (size_t i = 0; i < sizeof(FAMILIES) / sizeof(FAMILIES[0]); i++) {
            printf("%s\t%s\t%s\n", FAMILIES[i].label,
                   FAMILIES[i].golden_dir, FAMILIES[i].fixture_dir);
        }
        return 0;
    }
    if (argc == 2 && strcmp(argv[1], "--witnesses") == 0) {
        if (run_lane_decode_test() != 0) return 1;
        if (run_lane_latch_test() != 0) return 1;
        if (run_checksum_recompute_test() != 0) return 1;
        return run_alarm_only_witness();
    }
    if (argc == 3 && strcmp(argv[1], "--read-error") == 0) {
        return run_page_read_error(argv[2]);
    }
    if (argc == 6 && strcmp(argv[1], "--fixture") == 0) {
        for (size_t i = 0; i < sizeof(FAMILIES) / sizeof(FAMILIES[0]); i++) {
            if (strcmp(argv[2], FAMILIES[i].label) == 0) {
                return run_one_fixture(&FAMILIES[i], argv[3], argv[4], argv[5]);
            }
        }
    }
    fprintf(stderr, "invalid parity invocation\n");
    return 2;
}
