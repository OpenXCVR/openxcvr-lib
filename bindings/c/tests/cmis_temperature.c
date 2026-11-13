/* CMIS module temperature precision test:
 * Verifies that module_temperature_c is formatted with sufficient precision
 * to round-trip exact binary quotients (n / 256.0).
 *
 * Repro: lower[14..15] = 0x8001 -> -32767 / 256.0 = -127.99609375 C.
 * With %.10g it rounds to -127.9960938. With %.17g it round-trips to -127.99609375.
 */

#include "openxcvr/openxcvr.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    openxcvr_cmis_pages pages;
    openxcvr_status status = openxcvr_cmis_pages_load(
        "tests/fixtures/cmis", "fboss-cmis-4-0-osfp-2x400g-dr4", &pages
    );
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "failed to load CMIS fixture: %s\n", openxcvr_status_string(status));
        return 1;
    }

    /* Overwrite lower[14..15] with 0x8001 (-32767 / 256 = -127.99609375 C) */
    pages.lower_raw[14] = 0x80;
    pages.lower_raw[15] = 0x01;
    status = openxcvr_decode_cmis_lower_memory(pages.lower_raw, 128, &pages.lower);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "failed to redecode lower memory: %s\n", openxcvr_status_string(status));
        return 1;
    }

    size_t needed = 0;
    status = openxcvr_decode_cmis_json(&pages, NULL, 0, &needed);
    if (status != OPENXCVR_OK && status != OPENXCVR_ERR_SHORT_BUFFER) {
        fprintf(stderr, "size pass failed: %s\n", openxcvr_status_string(status));
        return 1;
    }

    char *buf = (char *)malloc(needed);
    if (!buf) {
        fprintf(stderr, "malloc failed\n");
        return 1;
    }

    status = openxcvr_decode_cmis_json(&pages, buf, needed, &needed);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "decode failed: %s\n", openxcvr_status_string(status));
        free(buf);
        return 1;
    }

    const char *key = "\"module_temperature_c\":";
    const char *p = strstr(buf, key);
    if (!p) {
        fprintf(stderr, "expected key %s not found in JSON: %s\n", key, buf);
        free(buf);
        return 1;
    }
    p += strlen(key);
    char *end = NULL;
    double val = strtod(p, &end);
    double expected = -127.99609375;

    if (val != expected) {
        fprintf(stderr, "FAIL: expected %.*s to parse to %.17g, got %.17g\n",
                (int)(end - p), p, expected, val);
        free(buf);
        return 1;
    }

    printf("PASS: module_temperature_c formatted as %.*s, parses to %.17g\n",
           (int)(end - p), p, val);
    free(buf);
    return 0;
}
