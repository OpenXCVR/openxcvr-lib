#include "openxcvr/openxcvr.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const int EXPECTED_HIF_0x53[15] = {
    81, 82, 79, 80, 77, 78, 75, 76,
    81, 82, 79, 80, 77, 78, 75
};

static const uint8_t REVISIONS[6] = {
    0x30, 0x40, 0x50, 0x51, 0x52, 0x53
};

static char *decode_json(const openxcvr_cmis_pages *pages) {
    size_t needed = 0;
    openxcvr_status status = openxcvr_decode_cmis_json(pages, NULL, 0, &needed);
    if (status != OPENXCVR_ERR_SHORT_BUFFER && status != OPENXCVR_OK) {
        return NULL;
    }
    char *buf = (char *)malloc(needed);
    if (!buf) return NULL;
    status = openxcvr_decode_cmis_json(pages, buf, needed, &needed);
    if (status != OPENXCVR_OK) {
        free(buf);
        return NULL;
    }
    return buf;
}

static int extract_applications(const char *json, int *out_codes, int *out_hifs, int max_entries) {
    const char *apps_pos = strstr(json, "\"applications\":[");
    if (!apps_pos) {
        return 0;
    }
    apps_pos += strlen("\"applications\":[");
    const char *end_array = strchr(apps_pos, ']');
    if (!end_array) {
        return 0;
    }

    int count = 0;
    const char *cur = apps_pos;
    while (cur < end_array) {
        const char *code_pos = strstr(cur, "\"app_code\":");
        if (!code_pos || code_pos > end_array) break;
        code_pos += strlen("\"app_code\":");
        int code = atoi(code_pos);

        int hif = -1;
        const char *hif_pos = strstr(code_pos, "\"host_interface_id\":");
        if (hif_pos && hif_pos < end_array) {
            hif_pos += strlen("\"host_interface_id\":");
            hif = atoi(hif_pos);
        }

        if (count < max_entries) {
            if (out_codes) out_codes[count] = code;
            if (out_hifs) out_hifs[count] = hif;
            count++;
        }
        const char *next_entry = strchr(code_pos, '}');
        if (!next_entry || next_entry > end_array) break;
        cur = next_entry + 1;
    }
    return count;
}

int main(void) {
    int failed = 0;
    openxcvr_cmis_pages base_zr;
    openxcvr_status status = openxcvr_cmis_pages_load("tests/fixtures/cmis", "fboss-cmis-5-3-osfp-800g-zr", &base_zr);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "Failed to load ZR fixture: %s\n", openxcvr_status_string(status));
        return 1;
    }

    /* (a) For each Lower:1 revision byte 0x30, 0x40, 0x50, 0x51, 0x52, 0x53 with both pages:
     * app codes exactly [1..15], and for 0x53 each entry's host_interface_id equals literals. */
    for (size_t i = 0; i < sizeof(REVISIONS) / sizeof(REVISIONS[0]); i++) {
        uint8_t rev = REVISIONS[i];
        openxcvr_cmis_pages pages = base_zr;
        pages.lower_raw[1] = rev;

        char *json = decode_json(&pages);
        if (!json) {
            fprintf(stderr, "FAIL (a) rev 0x%02X: decode failed\n", rev);
            failed++;
            continue;
        }

        int codes[32];
        int hifs[32];
        int count = extract_applications(json, codes, hifs, 32);
        free(json);

        if (count != 15) {
            fprintf(stderr, "FAIL (a) rev 0x%02X: got %d apps, expected 15\n", rev, count);
            failed++;
            continue;
        }

        int codes_ok = 1;
        for (int k = 0; k < 15; k++) {
            if (codes[k] != k + 1) {
                codes_ok = 0;
                break;
            }
        }
        if (!codes_ok) {
            fprintf(stderr, "FAIL (a) rev 0x%02X: app codes mismatch\n", rev);
            failed++;
            continue;
        }

        if (rev == 0x53) {
            int hif_ok = 1;
            for (int k = 0; k < 15; k++) {
                if (hifs[k] != EXPECTED_HIF_0x53[k]) {
                    hif_ok = 0;
                    fprintf(stderr, "FAIL (a) rev 0x53: hif[%d] = %d, expected %d\n",
                            k, hifs[k], EXPECTED_HIF_0x53[k]);
                }
            }
            if (!hif_ok) {
                failed++;
            }
        }
    }

    /* (b) Page 01h not supplied: app codes exactly [1..8]. */
    {
        openxcvr_cmis_pages pages = base_zr;
        pages.has_page01h = 0;
        pages.has_page11h = 0;
        char *json = decode_json(&pages);
        if (!json) {
            fprintf(stderr, "FAIL (b): decode failed\n");
            failed++;
        } else {
            int codes[32];
            int count = extract_applications(json, codes, NULL, 32);
            free(json);
            if (count != 8) {
                fprintf(stderr, "FAIL (b): got %d apps, expected 8\n", count);
                failed++;
            } else {
                for (int k = 0; k < 8; k++) {
                    if (codes[k] != k + 1) {
                        fprintf(stderr, "FAIL (b): code[%d] = %d, expected %d\n", k, codes[k], k + 1);
                        failed++;
                        break;
                    }
                }
            }
        }
    }

    /* (c) Lower slot 3 HIF set to 0xFF (terminator): Lower yields [1,2], Page 01h still yields [9..15].
     * Preservation of existing facade behaviour, not normative spec rules (design-decision A3). */
    {
        openxcvr_cmis_pages pages = base_zr;
        /* Lower slot 3 descriptor is at bytes 86 + 2*4 = 94. Byte 94 is HIF. */
        pages.lower_raw[94] = 0xFF;
        char *json = decode_json(&pages);
        if (!json) {
            fprintf(stderr, "FAIL (c): decode failed\n");
            failed++;
        } else {
            int codes[32];
            int count = extract_applications(json, codes, NULL, 32);
            free(json);
            int expected_codes[9] = {1, 2, 9, 10, 11, 12, 13, 14, 15};
            if (count != 9) {
                fprintf(stderr, "FAIL (c): got %d apps, expected 9\n", count);
                failed++;
            } else {
                for (int k = 0; k < 9; k++) {
                    if (codes[k] != expected_codes[k]) {
                        fprintf(stderr, "FAIL (c): code[%d] = %d, expected %d\n", k, codes[k], expected_codes[k]);
                        failed++;
                        break;
                    }
                }
            }
        }
    }

    /* (d) Every Page 01h descriptor MIF set to 0: app codes [1..8] (MIF-0 skip preserved).
     * Preservation of existing facade behaviour, not normative spec rules (design-decision A3). */
    {
        openxcvr_cmis_pages pages = base_zr;
        /* Page 01h has 7 descriptors at buffer offsets 95-122.
         * Each descriptor is 4 bytes: [HIF, MIF, ...]. MIF is offset + 1. */
        for (int i = 0; i < 7; i++) {
            pages.page01h_raw[95 + i * 4 + 1] = 0;
        }
        char *json = decode_json(&pages);
        if (!json) {
            fprintf(stderr, "FAIL (d): decode failed\n");
            failed++;
        } else {
            int codes[32];
            int count = extract_applications(json, codes, NULL, 32);
            free(json);
            if (count != 8) {
                fprintf(stderr, "FAIL (d): got %d apps, expected 8\n", count);
                failed++;
            } else {
                for (int k = 0; k < 8; k++) {
                    if (codes[k] != k + 1) {
                        fprintf(stderr, "FAIL (d): code[%d] = %d, expected %d\n", k, codes[k], k + 1);
                        failed++;
                        break;
                    }
                }
            }
        }
    }

    if (failed == 0) {
        printf("PASS: openxcvr_c_application_numbering\n");
        return 0;
    }
    fprintf(stderr, "FAILED: %d tests failed\n", failed);
    return 1;
}
