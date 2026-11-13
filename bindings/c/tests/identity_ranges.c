#include "openxcvr/openxcvr.h"
#include <assert.h>
#include <string.h>

static void exercise(openxcvr_spec_kind_t family, uint8_t revision) {
    openxcvr_range fields[6];
    openxcvr_range_observation observations[6];
    openxcvr_identity identity, sentinel;
    size_t count = 99;
    uint8_t page[256];
    memset(page, ' ', sizeof(page));
    memset(&sentinel, 0xa5, sizeof(sentinel));
    assert(openxcvr_identity_required_ranges(family, revision, NULL, 0, &count) == OPENXCVR_OK);
    assert(count == 6);
    memset(fields, 0xa5, sizeof(fields));
    openxcvr_range old[6];
    memcpy(old, fields, sizeof(old));
    assert(openxcvr_identity_required_ranges(family, revision, fields, 5, &count) == OPENXCVR_ERR_SHORT_BUFFER);
    assert(memcmp(fields, old, sizeof(old)) == 0);
    assert(openxcvr_identity_required_ranges(family, revision, fields, 6, &count) == OPENXCVR_OK);
    const uint8_t cmis_offsets[] = {129, 148, 164, 166, 182, 145};
    const uint8_t sff_offsets[] = {20, 40, 56, 68, 84, 37};
    for (size_t i = 0; i < count; ++i) {
        unsigned expected = family == OPENXCVR_SPEC_CMIS ? cmis_offsets[i] : sff_offsets[i];
        if (family == OPENXCVR_SPEC_SFF8636) expected += 128;
        assert(fields[i].offset == expected && fields[i].address == 0x50);
        assert(fields[i].bank == 0 && fields[i].page == 0);
        observations[i] = (openxcvr_range_observation){fields[i], page + fields[i].offset, fields[i].len, 1};
    }
    memcpy(page + fields[0].offset, "SYNTHETIC", 9);
    memcpy(page + fields[1].offset, "PART-123", 8);
    memcpy(page + fields[2].offset, family == OPENXCVR_SPEC_SFF8472 ? "R123" : "R1", fields[2].len);
    memcpy(page + fields[3].offset, "SERIAL-123", 10);
    memcpy(page + fields[4].offset, "261006  ", 8);
    memcpy(page + fields[5].offset, "\x01\x02\x03", 3);
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_OK);
    assert(strcmp(identity.vendor_name, "SYNTHETIC") == 0);
    assert(strcmp(identity.vendor_pn, "PART-123") == 0);
    assert(strcmp(identity.vendor_rev, "R1") == 0);
    assert(strcmp(identity.vendor_sn, "SERIAL-123") == 0);
    assert(strcmp(identity.date_code, "261006") == 0);
    assert(memcmp(identity.oui, "\x01\x02\x03", 3) == 0);
    assert(identity.family == family && identity.revision == revision);

    openxcvr_range_observation full = {{0x50, 0, 0, 0, 256}, page, 256, 1};
    assert(openxcvr_decode_identity_ranges(family, revision, &full, 1, &identity) == OPENXCVR_OK);
    /* Fragmented supplied bytes, including a field split across observations. */
    openxcvr_range_observation fragments[256];
    for (size_t i = 0; i < 256; ++i) fragments[i] = (openxcvr_range_observation){{0x50, 0, 0, (uint8_t)i, 1}, page + i, 1, 1};
    assert(openxcvr_decode_identity_ranges(family, revision, fragments, 256, &identity) == OPENXCVR_OK);

    /* A supplied zero OUI succeeds; an absent zero byte still refuses. */
    page[fields[5].offset] = 0;
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_OK);
    assert(identity.oui[0] == 0);
    identity = sentinel;
    observations[0].present = 0;
    observations[0].bytes = NULL;
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_ERR_SHORT_BUFFER);
    assert(memcmp(&identity, &sentinel, sizeof(identity)) == 0);
    observations[0].present = 1;
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_ERR_INVALID_ARGUMENT);
    observations[0].bytes = page + fields[0].offset;
    observations[0].len--;
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_ERR_INVALID_ARGUMENT);
    observations[0].len++;
    observations[0].range.bank = 1;
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_ERR_SHORT_BUFFER);
    observations[0].range.bank = 0;
    observations[1] = observations[0];
    assert(openxcvr_decode_identity_ranges(family, revision, observations, count, &identity) == OPENXCVR_ERR_INVALID_ARGUMENT);
    full.range.offset = 255;
    full.range.len = 2;
    assert(openxcvr_decode_identity_ranges(family, revision, &full, 1, &identity) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(memcmp(&identity, &sentinel, sizeof(identity)) == 0);
}

static void supplied_span(void) {
    uint8_t name[16];
    memset(name, ' ', sizeof(name));
    memcpy(name, "EXAMPLE", 7);
    char decoded[17];
    char sentinel[17];
    memset(sentinel, 0xa5, sizeof(sentinel));
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_name_supplied(name, 129, 16, decoded) == OPENXCVR_OK);
    assert(strcmp(decoded, "EXAMPLE") == 0);
    memcpy(decoded, sentinel, sizeof(decoded));
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_name_supplied(name, 129, 15, decoded) == OPENXCVR_ERR_INSUFFICIENT_INPUT);
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_name_supplied(name, 130, 16, decoded) == OPENXCVR_ERR_INSUFFICIENT_INPUT);
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_pn_supplied(name, 129, 16, decoded) == OPENXCVR_ERR_INSUFFICIENT_INPUT);
    assert(memcmp(decoded, sentinel, sizeof(decoded)) == 0);
    uint8_t zeros[3] = {0}, oui[3] = {0xa5, 0xa5, 0xa5};
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_oui_supplied(zeros, 145, 0, oui) == OPENXCVR_ERR_INSUFFICIENT_INPUT);
    assert(oui[0] == 0xa5);
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_oui_supplied(zeros, 145, 3, oui) == OPENXCVR_OK);
    assert(memcmp(oui, zeros, 3) == 0);
    assert(openxcvr_decode_cmis_page00h_administrative_vendor_oui_supplied(zeros, 255, 3, oui) == OPENXCVR_ERR_INVALID_ARGUMENT);
}

static void exercise_independent_witness(uint8_t revision) {
    openxcvr_range fields[6];
    openxcvr_range_observation observations[6];
    openxcvr_identity identity, sentinel;
    size_t count = 0;
    uint8_t page[256];
    memset(page, ' ', sizeof(page));
    memset(&sentinel, 0x5a, sizeof(sentinel));

    assert(openxcvr_identity_required_ranges(OPENXCVR_SPEC_CMIS, revision, fields, 6, &count) == OPENXCVR_OK);
    assert(count == 6);
    for (size_t i = 0; i < count; ++i) {
        observations[i] = (openxcvr_range_observation){fields[i], page + fields[i].offset, fields[i].len, 1};
    }
    memcpy(page + 129, "QUALIFY-LAB     ", 16);
    memcpy(page + 145, "\x12\xa5\x7c", 3);
    memcpy(page + 148, "PART-42         ", 16);
    memcpy(page + 164, "B7", 2);
    memcpy(page + 166, "SERIAL-09       ", 16);
    memcpy(page + 182, "240229L7", 8);

    identity = sentinel;
    assert(openxcvr_decode_identity_ranges(OPENXCVR_SPEC_CMIS, revision, observations, count, &identity) == OPENXCVR_OK);
    assert(strcmp(identity.vendor_name, "QUALIFY-LAB") == 0);
    assert(memcmp(identity.oui, "\x12\xa5\x7c", 3) == 0);
    assert(strcmp(identity.vendor_pn, "PART-42") == 0);
    assert(strcmp(identity.vendor_rev, "B7") == 0);
    assert(strcmp(identity.vendor_sn, "SERIAL-09") == 0);
    assert(strcmp(identity.date_code, "240229L7") == 0);
    assert(identity.family == OPENXCVR_SPEC_CMIS);
    assert(identity.revision == revision);
}

int main(void) {
    supplied_span();
    const uint8_t cmis_revs[] = {0x30, 0x40, 0x50, 0x51, 0x52, 0x53};
    for (size_t i = 0; i < sizeof(cmis_revs); ++i) {
        exercise(OPENXCVR_SPEC_CMIS, cmis_revs[i]);
        exercise_independent_witness(cmis_revs[i]);
    }
    exercise(OPENXCVR_SPEC_SFF8636, 8);
    exercise(OPENXCVR_SPEC_SFF8472, 0);
    openxcvr_identity identity;
    openxcvr_range ranges[6];
    size_t count = 99;
    const uint8_t bad_revs[] = {0, 0x31, 0x41, 0x55, 0x5f, 0x60, 0xff};
    for (size_t i = 0; i < sizeof(bad_revs); ++i) {
        assert(openxcvr_identity_required_ranges(OPENXCVR_SPEC_CMIS, bad_revs[i], ranges, 6, &count) == OPENXCVR_ERR_UNSUPPORTED_REVISION);
        assert(count == 99);
        assert(openxcvr_decode_identity_ranges(OPENXCVR_SPEC_CMIS, bad_revs[i], NULL, 0, &identity) == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    }
    assert(openxcvr_identity_required_ranges((openxcvr_spec_kind_t)99, 0, ranges, 6, &count) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(openxcvr_identity_required_ranges(OPENXCVR_SPEC_CMIS, 0x53, NULL, 1, &count) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(openxcvr_identity_required_ranges(OPENXCVR_SPEC_CMIS, 0x53, ranges, 6, NULL) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(openxcvr_decode_identity_ranges(OPENXCVR_SPEC_CMIS, 0x53, NULL, 1, &identity) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(openxcvr_decode_identity_ranges(OPENXCVR_SPEC_CMIS, 0x53, NULL, 0, &identity) == OPENXCVR_ERR_SHORT_BUFFER);
    assert(openxcvr_decode_identity_ranges(OPENXCVR_SPEC_CMIS, 0x53, NULL, 0, NULL) == OPENXCVR_ERR_INVALID_ARGUMENT);
    return 0;
}
