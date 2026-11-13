/* Public-normalizer revision-context tests for the CMIS DPSM labels,
   driven through the ACTUAL public entrypoint openxcvr_decode_cmis_json
   (not a private formatting helper).

   Uses the independently authored reserved fixture
   (synthetic-cmis-3-0-qsfp-dd-dp-reserved) loaded from the repo corpus,
   then mutated IN MEMORY (Lower:1 context byte; Page 11h state bytes) — no
   corpus file is modified. Expected labels and non-label snapshot values
   are literals transcribed from CMIS 3.0 Table 66 and the fixture's
   authored bytes, never from a decoder run. */

#include "openxcvr/openxcvr.h"
#include "pages/cmis_lower_memory.h"
#include "pages/cmis_page11h_lane_status.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Every major-3/4/5 context byte outside the literal seven-byte catalog
   (0x30, 0x40, 0x50..0x54): 0x31..0x3F, 0x41..0x4F, 0x55..0x5F —
   41 unsupported contexts, explicitly including 0x31/0x41/0x55. */
static const unsigned char UNSUPPORTED_CONTEXTS[41] = {
    0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C,
    0x3D, 0x3E, 0x3F,
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C,
    0x4D, 0x4E, 0x4F,
    0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F,
};

static int count_occurrences(const char *haystack, const char *needle) {
    int count = 0;
    const char *p = haystack;
    while ((p = strstr(p, needle)) != NULL) {
        count++;
        p += strlen(needle);
    }
    return count;
}

/* Two-pass decode of `pages` into a heap buffer; aborts the test on any
   status other than OK (admission errors surface as a failed assert). */
static char *decode_json(const openxcvr_cmis_pages *pages, size_t *len) {
    size_t needed = 0;
    openxcvr_status status =
        openxcvr_decode_cmis_json(pages, NULL, 0, &needed);
    if (status != OPENXCVR_ERR_SHORT_BUFFER && status != OPENXCVR_OK) {
        fprintf(stderr, "size pass failed: %s\n", openxcvr_status_string(status));
        return NULL;
    }
    char *json = (char *)malloc(needed);
    if (json == NULL) {
        return NULL;
    }
    status = openxcvr_decode_cmis_json(pages, json, needed, &needed);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "decode failed: %s\n", openxcvr_status_string(status));
        free(json);
        return NULL;
    }
    *len = needed;
    return json;
}

int main(void) {
    openxcvr_cmis_pages base;
    openxcvr_status status = openxcvr_cmis_pages_load(
        "tests/fixtures/cmis", "synthetic-cmis-3-0-qsfp-dd-dp-reserved", &base
    );
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "reserved fixture load failed: %s\n", openxcvr_status_string(status));
        return 1;
    }

    /* Baseline snapshot markers from the independently authored fixture
       bytes (Table 66 controls are checked below; these are the non-label
       snapshot values that must NOT move with the context byte). */
    static const char *const SNAPSHOT_MARKERS[] = {
        "\"decode_status\":\"ok\"",
        "\"vendor_name\":\"CMIS_3_0_SYNTH\"",
        "\"vendor_pn\":\"FIXTURE-30-SYNTH\"",
        "\"memory_model\":\"paged\"",
        "\"module_state\":\"Reserved0\"",
        "\"connector\":{\"name\":\"unknown_or_unspecified\"",
        "\"identifier\":{\"name\":\"qsfp_dd\"",
        "\"interrupt_asserted\":true",
    };

    for (size_t c = 0; c < sizeof(UNSUPPORTED_CONTEXTS); c++) {
        unsigned char ctx = UNSUPPORTED_CONTEXTS[c];
        for (unsigned raw = 0; raw <= 15; raw++) {
            openxcvr_cmis_pages mut = base;
            mut.lower_raw[1] = ctx;
            if (openxcvr_decode_cmis_lower_memory(mut.lower_raw, 128, &mut.lower) != OPENXCVR_OK) {
                fprintf(stderr, "ctx %#04x: lower re-decode failed\n", ctx);
                return 1;
            }
            unsigned char state = (unsigned char)(raw | (raw << 4));
            for (int i = 0; i < 4; i++) {
                mut.page11h_raw[i] = state;
            }
            if (openxcvr_decode_cmis_page11h_lane_status(mut.page11h_raw, 128, &mut.page11h) != OPENXCVR_OK) {
                fprintf(stderr, "ctx %#04x raw %u: page 11h re-decode failed\n", ctx, raw);
                return 1;
            }

            size_t len = 0;
            char *json = decode_json(&mut, &len);
            if (json == NULL) {
                fprintf(stderr, "ctx %#04x raw %u: decode failed\n", ctx, raw);
                return 1;
            }

            /* Exactly eight dp_state labels, all Unknown<raw>. */
            char want[32];
            snprintf(want, sizeof(want), "\"dp_state\":\"Unknown%u\"", raw);
            int total = count_occurrences(json, "\"dp_state\":\"");
            int hits = count_occurrences(json, want);
            if (total != 8 || hits != 8) {
                fprintf(stderr, "ctx %#04x raw %u: dp_state labels: %d total, %d x %s\n",
                        ctx, raw, total, hits, want);
                free(json);
                return 1;
            }

            /* Non-label snapshot unchanged; device revision text follows the
               mutated context byte. */
            for (size_t m = 0; m < sizeof(SNAPSHOT_MARKERS) / sizeof(SNAPSHOT_MARKERS[0]); m++) {
                if (strstr(json, SNAPSHOT_MARKERS[m]) == NULL) {
                    fprintf(stderr, "ctx %#04x raw %u: snapshot marker missing: %s\n",
                            ctx, raw, SNAPSHOT_MARKERS[m]);
                    free(json);
                    return 1;
                }
            }
            char rev[32];
            snprintf(rev, sizeof(rev), "\"cmis_revision\":\"%u.%u\"", ctx >> 4, ctx & 0xF);
            if (strstr(json, rev) == NULL) {
                fprintf(stderr, "ctx %#04x: revision text missing: %s\n", ctx, rev);
                free(json);
                return 1;
            }
            free(json);
        }
    }

    /* Missing Page 11h on a paged module keeps the existing null blocks. */
    {
        openxcvr_cmis_pages mut = base;
        mut.has_page11h = 0;
        size_t len = 0;
        char *json = decode_json(&mut, &len);
        if (json == NULL) {
            return 1;
        }
        if (strstr(json, "\"flags\":null,\"lanes\":null") == NULL
            || strstr(json, "\"dp_state\"") != NULL) {
            fprintf(stderr, "missing page 11h: null lane/flag blocks changed\n");
            free(json);
            return 1;
        }
        free(json);
    }

    /* Flat-memory module: null blocks even with Page 11h bytes supplied. */
    {
        openxcvr_cmis_pages mut = base;
        mut.lower_raw[2] |= 0x80;
        if (openxcvr_decode_cmis_lower_memory(mut.lower_raw, 128, &mut.lower) != OPENXCVR_OK) {
            return 1;
        }
        size_t len = 0;
        char *json = decode_json(&mut, &len);
        if (json == NULL) {
            return 1;
        }
        if (strstr(json, "\"flags\":null,\"lanes\":null") == NULL) {
            fprintf(stderr, "flat memory: null lane/flag blocks changed\n");
            free(json);
            return 1;
        }
        free(json);
    }

    /* Page 11h present without Page 01h keeps the existing explicit
       prerequisite error. */
    {
        openxcvr_cmis_pages mut = base;
        mut.has_page01h = 0;
        size_t needed = 0;
        openxcvr_status s = openxcvr_decode_cmis_json(&mut, NULL, 0, &needed);
        if (s != OPENXCVR_ERR_INVALID_ARGUMENT) {
            fprintf(stderr, "prerequisite error changed: %s\n", openxcvr_status_string(s));
            return 1;
        }
    }

    printf("openxcvr_c_dpsm_context: %zu contexts x 16 raws + missing/flat/prereq OK\n",
           sizeof(UNSUPPORTED_CONTEXTS));
    return 0;
}
