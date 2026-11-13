/* CMIS 4.0 Tables 8-12/13/39 and CMIS 5.3 Tables 8-20/22/23/59.
 * Synthetic fixtures, no module I/O. Also used by the Rust equivalence test. */
#include "openxcvr/application_records.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void fixture(uint8_t rev, uint8_t media, uint8_t *lower, uint8_t *upper) {
    size_t i;
    memset(lower, 0, 128); memset(upper, 0, 128);
    lower[1] = rev; lower[85] = media;
    for (i = 0; i < 8; ++i) {
        const uint8_t d[] = {0x1a, 1, 0x44, 0x11};
        memcpy(lower + 86 + i * 4, d, 4);
    }
    for (i = 0; i < 7; ++i) {
        const uint8_t d[] = {0x1a, 1, 0x22, 0x55};
        memcpy(upper + 95 + i * 4, d, 4);
    }
}

int main(int argc, char **argv) {
    uint8_t lower[128], upper[128];
    const openxcvr_application_result *out = NULL;
    openxcvr_application_request r = {0};
    size_t i;
    r.size = sizeof(r); r.version = OPENXCVR_APPLICATION_RECORDS_VERSION;
    r.revision_major = 4; r.revision_minor = 0;
    r.lower = lower; r.lower_len = 128; r.page01 = upper; r.page01_len = 128;
    if (argc == 4) {
        /* Dump is compared field-for-field with the Rust API. The scenario
         * index varies media table, sentinel, missing/short data, unknowns,
         * implicit/reserved counts, revision mismatch and unsupported rev. */
        unsigned rev = (unsigned)strtoul(argv[1], NULL, 0);
        unsigned media = (unsigned)strtoul(argv[2], NULL, 0);
        unsigned scenario = (unsigned)strtoul(argv[3], NULL, 0);
        fixture((uint8_t)rev, (uint8_t)media, lower, upper);
        r.revision_major = (uint8_t)(rev >> 4); r.revision_minor = (uint8_t)(rev & 15);
        switch (scenario) {
        case 1: lower[114] = 255; break;
        case 2: upper[95] = 255; break;
        case 3: r.page01 = NULL; r.page01_len = 0; break;
        case 4: r.page01_len = 122; break;
        case 5: lower[86] = 254; lower[87] = 254; break;
        case 6: lower[86] = 0; lower[87] = 0; break;
        case 7: lower[88] = 9; lower[89] = 255; break;
        case 8: lower[1] = 0; break;
        case 9: r.lower = NULL; r.lower_len = 0; break;
        case 10: r.lower_len = 117; break;
        case 11: upper[119] = 255; break;
        default: break;
        }
        assert(openxcvr_application_records(&r, &out) == OPENXCVR_OK);
        for (i = 0; i < out->count; ++i) {
            const openxcvr_application_record *p = &out->records[i];
            printf("%u|%u|%u|%u|%u|%u|%u|%u|%u|%u|%u|%u|%u|%s|%s\n",
                p->slot, p->state, p->has_application, p->host_interface_code,
                p->media_interface_code, p->host_lane_count, p->media_lane_count,
                p->host_lane_assignment_options, p->host_meaning_state,
                p->media_meaning_state, p->host_lane_count_state, p->media_lane_count_state,
                p->unsupported_media_type, p->host_meaning ? p->host_meaning : "-", p->media_meaning ? p->media_meaning : "-");
        }
        openxcvr_application_records_free(out);
        return 0;
    }
    fixture(0x40, 1, lower, upper);
    assert(openxcvr_application_records(&r, &out) == OPENXCVR_OK);
    assert(out && out->size == sizeof(*out) && out->version == 1 && out->count == 15);
    assert(out->revision_major == 4 && out->revision_minor == 0);
    for (i = 0; i < 15; ++i) {
        assert(out->records[i].slot == i + 1);
        assert(out->records[i].state == OPENXCVR_APPLICATION_PRESENT);
        assert(!strcmp(out->records[i].host_meaning, "Base100gCr4"));
        assert(!strcmp(out->records[i].media_meaning, "Base10gSw"));
    }
    /* Ownership is independent of input buffers. */
    memset(lower, 255, 128); memset(upper, 255, 128);
    assert(out->records[14].host_lane_count == 2);
    assert(!strcmp(out->records[14].host_meaning, "Base100gCr4"));
    openxcvr_application_records_free(out);
    openxcvr_application_records_free(NULL);
    assert(openxcvr_application_records(NULL, &out) == OPENXCVR_ERR_INVALID_ARGUMENT && !out);
    assert(openxcvr_application_records(&r, NULL) == OPENXCVR_ERR_INVALID_ARGUMENT);
    r.size = 0;
    assert(openxcvr_application_records(&r, &out) == OPENXCVR_ERR_INVALID_ARGUMENT && !out);
    r.size = sizeof(r); r.version = 2;
    assert(openxcvr_application_records(&r, &out) == OPENXCVR_ERR_INVALID_ARGUMENT && !out);
    r.version = 1; r.lower_len = 129;
    assert(openxcvr_application_records(&r, &out) == OPENXCVR_ERR_INVALID_ARGUMENT && !out);
    r.lower_len = 128; r.lower = NULL;
    assert(openxcvr_application_records(&r, &out) == OPENXCVR_ERR_INVALID_ARGUMENT && !out);
    r.lower = lower; r.page01_len = (size_t)-1;
    assert(openxcvr_application_records(&r, &out) == OPENXCVR_ERR_INVALID_ARGUMENT && !out);
    puts("application records: ABI, bounds and ownership passed");
    return 0;
}
