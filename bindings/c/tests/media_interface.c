#include "openxcvr/openxcvr.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct test_case {
    uint8_t media_type;
    uint8_t mif;
    const char *expected;
};

static const struct test_case CASES[] = {
    {0x01, 0x01, "base_10g_sw"},
    {0x01, 0x1C, "fc_128g_mm"},
    {0x01, 0xC0, "vendor_specific"},
    {0x02, 0x01, "base_10g_lw"},
    {0x02, 0x1C, "base_400g_dr4"},
    {0x02, 0x68, "flexo_8e_dpo_16qam_foic8e_8_dpo"},
    {0x02, 0xC0, "vendor_specific"},
    {0x02, 0xC1, "vendor_specific"},
    {0x02, 0xF7, "vendor_specific_f7"},
    {0x03, 0x01, "copper_cable"},
    {0x03, 0x1C, "reserved_02_be"},
    {0x03, 0xC0, "linear_active_copper_loopback_module"},
    {0x03, 0xC1, "vendor_specific"},
    {0x04, 0x01, "active_cable_ber_1e_12"},
    {0x04, 0x1C, "reserved_05_be"},
    {0x04, 0xF7, "vendor_specific"},
    {0x05, 0x01, "base_1000_t"},
    {0x05, 0xC1, "vendor_specific"},
    {0x00, 0x1C, "unknown"},
    {0x06, 0x1C, "unknown"},
    {0x40, 0x1C, "unknown"},
    {0x90, 0x1C, "unknown"},
    {0xFF, 0x1C, "unknown"},
};

static int get_media_interface_name(const char *json, int app_code, char *out, size_t out_len) {
    char target[64];
    snprintf(target, sizeof(target), "\"app_code\":%d", app_code);
    const char *pos = strstr(json, target);
    if (!pos) return 0;
    const char *end_entry = strchr(pos, '}');
    if (!end_entry) return 0;
    const char *key = "\"media_interface_name\":\"";
    const char *name_pos = strstr(pos, key);
    if (!name_pos || name_pos > end_entry) return 0;
    name_pos += strlen(key);
    const char *quote = strchr(name_pos, '"');
    if (!quote || quote > end_entry) return 0;
    size_t len = (size_t)(quote - name_pos);
    if (len >= out_len) return 0;
    memcpy(out, name_pos, len);
    out[len] = '\0';
    return 1;
}

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

int main(void) {
    int failed = 0;
    size_t num_cases = sizeof(CASES) / sizeof(CASES[0]);

    /* Test 1: Lower caller (fboss-cmis-4-0-qsfpdd-400g-dr4) */
    openxcvr_cmis_pages base_dr4;
    openxcvr_status status = openxcvr_cmis_pages_load("tests/fixtures/cmis", "fboss-cmis-4-0-qsfpdd-400g-dr4", &base_dr4);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "Failed to load DR4 fixture: %s\n", openxcvr_status_string(status));
        return 1;
    }

    for (size_t i = 0; i < num_cases; i++) {
        openxcvr_cmis_pages pages = base_dr4;
        pages.lower_raw[85] = CASES[i].media_type;
        pages.lower_raw[87] = CASES[i].mif;

        char *json = decode_json(&pages);
        if (!json) {
            fprintf(stderr, "FAIL lower 0x%02X 0x%02X: decode failed\n", CASES[i].media_type, CASES[i].mif);
            failed++;
            continue;
        }

        char actual[128] = {0};
        if (!get_media_interface_name(json, 1, actual, sizeof(actual))) {
            fprintf(stderr, "FAIL lower 0x%02X 0x%02X: app_code 1 not found in json\n", CASES[i].media_type, CASES[i].mif);
            failed++;
        } else if (strcmp(actual, CASES[i].expected) != 0) {
            fprintf(stderr, "FAIL lower 0x%02X 0x%02X: got %s, expected %s\n", CASES[i].media_type, CASES[i].mif, actual, CASES[i].expected);
            failed++;
        } else {
            printf("PASS lower 0x%02X 0x%02X: %s\n", CASES[i].media_type, CASES[i].mif, actual);
        }
        free(json);
    }

    /* Test 2: Page 01h caller (fboss-cmis-4-0-osfp-2x400g-dr4) */
    openxcvr_cmis_pages base_osfp;
    status = openxcvr_cmis_pages_load("tests/fixtures/cmis", "fboss-cmis-4-0-osfp-2x400g-dr4", &base_osfp);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "Failed to load OSFP fixture: %s\n", openxcvr_status_string(status));
        return 1;
    }

    for (size_t i = 0; i < num_cases; i++) {
        openxcvr_cmis_pages pages = base_osfp;
        pages.lower_raw[85] = CASES[i].media_type;
        pages.page01h_raw[96] = CASES[i].mif;

        char *json = decode_json(&pages);
        if (!json) {
            fprintf(stderr, "FAIL page01 0x%02X 0x%02X: decode failed\n", CASES[i].media_type, CASES[i].mif);
            failed++;
            continue;
        }

        char actual[128] = {0};
        if (!get_media_interface_name(json, 9, actual, sizeof(actual))) {
            fprintf(stderr, "FAIL page01 0x%02X 0x%02X: app_code 9 not found in json\n", CASES[i].media_type, CASES[i].mif);
            failed++;
        } else if (strcmp(actual, CASES[i].expected) != 0) {
            fprintf(stderr, "FAIL page01 0x%02X 0x%02X: got %s, expected %s\n", CASES[i].media_type, CASES[i].mif, actual, CASES[i].expected);
            failed++;
        } else {
            printf("PASS page01 0x%02X 0x%02X: %s\n", CASES[i].media_type, CASES[i].mif, actual);
        }
        free(json);
    }

    printf("\nC media interface tests: %d failures\n", failed);
    return failed > 0 ? 1 : 0;
}
