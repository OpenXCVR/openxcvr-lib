/*
 * Field read plan prelude, Stage 1b (C): the openxcvr_field_read_prelude
 * contract — family revision/compliance gate reads plus Equals
 * prerequisite ranges, effect-checked and coalesced — exercised on
 * catalog-resolved entries for CMIS and SFF-8472.
 *
 * No fixtures are opened: the prelude plans from catalog metadata only,
 * and hand-built rows exercise the shared rule 8 validation without
 * needing generated-table membership.
 */
#include "openxcvr/catalog.h"
#include "openxcvr/observation.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(cond) do { \
    if (!(cond)) { \
        fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
        exit(1); \
    } \
} while (0)

/* Resolve one Page 14h data field (all lanes) at CMIS 5.1. */
static size_t resolve_page14h(
    const char *name, openxcvr_resolved_field_t *out, size_t capacity
) {
    openxcvr_field_query_t query = {NULL, OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    query.pattern = name;
    query.lane = OPENXCVR_LANE_ALL;
    query.page = "14h";
    size_t count = 0;
    const openxcvr_status status =
        openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 1, &query, out, capacity, &count);
    CHECK(status == OPENXCVR_OK);
    return count;
}

/* Resolve one field by exact name at one revision and lane selection. */
static size_t resolve_field(
    openxcvr_spec_kind_t family, uint8_t major, uint8_t minor,
    const char *name, openxcvr_lane_selection_t lane, const char *page,
    openxcvr_resolved_field_t *out, size_t capacity
) {
    openxcvr_field_query_t query = {NULL, OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    query.pattern = name;
    query.lane = lane;
    query.page = page;
    size_t count = 0;
    const openxcvr_status status =
        openxcvr_catalog_resolve(family, major, minor, &query, out, capacity, &count);
    CHECK(status == OPENXCVR_OK);
    return count;
}

/* True when a planned read is exactly the given range. */
static int read_is(
    const openxcvr_planned_read_t *r, uint8_t address, uint8_t bank,
    uint8_t page, uint16_t offset, uint16_t len
) {
    return r->range.address == address && r->range.bank == bank &&
           r->range.page == page && r->range.offset == offset &&
           r->range.len == len;
}

/* True when two ranges cover a common byte: same address, bank and page
 * with overlapping offset spans. */
static int ranges_overlap(const openxcvr_range *a, const openxcvr_range *b) {
    if (a->address != b->address || a->bank != b->bank || a->page != b->page) {
        return 0;
    }
    return a->offset < b->offset + b->len && b->offset < a->offset + a->len;
}

/* (a) Two page-14h fields over all lanes: the prelude plans exactly the
 * CMIS revision byte (0x50 bank 0 page 0 offset 1) and the Page 14h
 * selector byte (0x50 bank 0 page 0x14 offset 128), in either order,
 * and refuses nothing. */
static void cmis_page14h_prelude(void) {
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    const size_t ber = resolve_page14h("media_side_ber", entries + snr, 8);
    CHECK(snr + ber == 16);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_plan_refusal_t refusals[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    const openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        reads, 8, &read_count, refusals, 8, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(read_count == 2);
    CHECK(refusal_count == 0);
    CHECK((read_is(&reads[0], 0x50, 0, 0, 1, 1) &&
           read_is(&reads[1], 0x50, 0, 0x14, 128, 1)) ||
          (read_is(&reads[0], 0x50, 0, 0x14, 128, 1) &&
           read_is(&reads[1], 0x50, 0, 0, 1, 1)));
}

/* (b) SFF-8472 optional_laser_temperature at 12.5: the prelude plans the
 * external-calibration byte (offset 92) and the revision-compliance byte
 * (offset 94), both 0x50 page 0 len 1, in either order, and no read
 * overlaps the field's own entry range. */
static void sff8472_prelude(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_SFF8472, 12, 5, "optional_laser_temperature",
        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base", entries, 4);
    CHECK(count >= 1);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_plan_refusal_t refusals[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    const openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_SFF8472, 12, 5, entries, count,
        reads, 8, &read_count, refusals, 8, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(read_count == 2);
    CHECK((read_is(&reads[0], 0x50, 0, 0, 92, 1) &&
           read_is(&reads[1], 0x50, 0, 0, 94, 1)) ||
          (read_is(&reads[0], 0x50, 0, 0, 94, 1) &&
           read_is(&reads[1], 0x50, 0, 0, 92, 1)));
    for (size_t i = 0; i < read_count; i++) {
        CHECK(!ranges_overlap(&reads[i].range, &entries[0].range));
    }
}

/* (c) An unconditional field (temp_mon_value, first resolved entry): the
 * prelude plans only the revision byte. */
static void unconditional_prelude(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, entries, 4);
    CHECK(count >= 1);
    openxcvr_planned_read_t reads[4];
    openxcvr_field_plan_refusal_t refusals[4];
    size_t read_count = 0;
    size_t refusal_count = 0;
    const openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        reads, 4, &read_count, refusals, 4, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(read_count == 1);
    CHECK(read_is(&reads[0], 0x50, 0, 0, 1, 1));
}

/* (d) Empty input and validation: zero entries is a valid query with
 * both counts zero; the family and revision gates apply with no entries.
 * Other errors leave counts unchanged. */
static void empty_and_validation(void) {
    size_t read_count = 777;
    size_t refusal_count = 777;
    openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, NULL, 0,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(read_count == 0);
    CHECK(refusal_count == 0);

    /* CMIS 5.5 is not a supported revision. */
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 5, NULL, 0,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* An unknown family value is an invalid argument. */
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        (openxcvr_spec_kind_t)99, 5, 1, NULL, 0,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
}

/* A generalized effectful-entry builder: one resolved entry whose Equals
 * presence prerequisite is moved onto a clear-on-read lower-memory byte
 * (page 0, the given offset, len 1). */
static void effectful_entry_at(
    const openxcvr_resolved_field_t *base, uint16_t offset,
    openxcvr_resolved_field_t *entry_out, openxcvr_catalog_row_t *row_out
) {
    *row_out = *base->row;
    row_out->presence_kind = OPENXCVR_PRESENCE_EQUALS;
    row_out->presence_equals.loc.address = 0x50;
    row_out->presence_equals.loc.page = 0;
    row_out->presence_equals.loc.offset = offset;
    row_out->presence_equals.loc.len = 1;
    *entry_out = *base;
    entry_out->row = row_out;
}

/* The case (e) entry: media_side_snr lane 0 with its Equals presence
 * prerequisite moved onto the CMIS lower clear-on-read byte 8
 * (0x50 page 0 offset 8 len 1). */
static void effectful_entry(
    openxcvr_resolved_field_t *entry_out, openxcvr_catalog_row_t *row_out
) {
    openxcvr_resolved_field_t resolved[8];
    const size_t count = resolve_page14h("media_side_snr", resolved, 8);
    CHECK(count >= 1);
    effectful_entry_at(&resolved[0], 8, entry_out, row_out);
}

/* (e) An Equals presence prerequisite on a clear-on-read byte is refused
 * for that entry with OPENXCVR_FIELD_PLAN_CLEAR_ON_READ, and no planned
 * read touches the byte. */
static void effectful_prerequisite(void) {
    openxcvr_resolved_field_t entry;
    openxcvr_catalog_row_t row;
    effectful_entry(&entry, &row);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_plan_refusal_t refusals[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    const openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
    CHECK(refusals[0].field != NULL);
    CHECK(strcmp(refusals[0].field, entry.row->name) == 0);
    CHECK(refusals[0].has_lane == entry.has_lane);
    CHECK(refusals[0].lane == entry.lane);
    const openxcvr_range clear_on_read_byte = {0x50, 0, 0, 8, 1};
    for (size_t i = 0; i < read_count; i++) {
        CHECK(!ranges_overlap(&reads[i].range, &clear_on_read_byte));
    }
}

/* (f) Bounds: entry_count at OPENXCVR_FIELD_PLAN_MAX_ENTRIES is admitted;
 * one entry over fails with OUT_OF_RANGE, leaving the pre-set counts and
 * the canary-filled reads array unchanged. */
static void bounds(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, entries, 4);
    CHECK(count >= 1);
    openxcvr_resolved_field_t many[257];
    for (size_t i = 0; i < 257; i++) {
        many[i] = entries[0];
    }
    openxcvr_planned_read_t reads[4];
    openxcvr_field_plan_refusal_t refusals[4];
    size_t read_count = 777;
    size_t refusal_count = 777;
    openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, many, OPENXCVR_FIELD_PLAN_MAX_ENTRIES,
        reads, 4, &read_count, refusals, 4, &refusal_count);
    CHECK(status == OPENXCVR_OK);

    unsigned char canary[sizeof reads];
    memset(reads, 0xAB, sizeof reads);
    memcpy(canary, reads, sizeof reads);
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, many, 257,
        reads, 4, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_OUT_OF_RANGE);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(memcmp(reads, canary, sizeof reads) == 0);
}

/* (g) Transactional contract: NULL-with-zero size queries, SHORT_BUFFER
 * publishing counts but not arrays, and required pointers. */
static void transactional(void) {
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    const size_t ber = resolve_page14h("media_side_ber", entries + snr, 8);
    CHECK(snr + ber == 16);
    const size_t entry_count = snr + ber;

    /* Both outputs NULL with capacity zero: a size query. */
    size_t read_count = 777;
    size_t refusal_count = 777;
    openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, entry_count,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(read_count == 2);
    CHECK(refusal_count == 0);

    /* read_capacity 1: SHORT_BUFFER sets the count and leaves the canary
     * read untouched. */
    openxcvr_planned_read_t one[1];
    memset(one, 0xAB, sizeof one);
    unsigned char one_canary[sizeof one];
    memcpy(one_canary, one, sizeof one);
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, entry_count,
        one, 1, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_SHORT_BUFFER);
    CHECK(read_count == 2);
    CHECK(memcmp(one, one_canary, sizeof one) == 0);

    /* The case (e) entry with refusal_capacity 0 on a non-NULL array:
     * SHORT_BUFFER sets refusal_count and leaves the reads canary
     * untouched. */
    openxcvr_resolved_field_t entry;
    openxcvr_catalog_row_t row;
    effectful_entry(&entry, &row);
    openxcvr_planned_read_t reads[4];
    memset(reads, 0xAB, sizeof reads);
    unsigned char reads_canary[sizeof reads];
    memcpy(reads_canary, reads, sizeof reads);
    openxcvr_field_plan_refusal_t refusals[1];
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        reads, 4, &read_count, refusals, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_SHORT_BUFFER);
    CHECK(refusal_count == 1);
    CHECK(memcmp(reads, reads_canary, sizeof reads) == 0);

    /* reads NULL with non-zero capacity is invalid. */
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, entry_count,
        NULL, 4, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* The read count pointer is required. */
    openxcvr_planned_read_t out_reads[4];
    openxcvr_field_plan_refusal_t out_refusals[4];
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, entry_count,
        out_reads, 4, NULL, out_refusals, 4, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(refusal_count == 777);
}

/* (h) Malformed entries: a NULL entries array with a non-zero count, an
 * entry whose resolved revision disagrees with the requested one, and an
 * entry with no row, are each INVALID_ARGUMENT with counts untouched. */
static void malformed(void) {
    /* entries NULL with entry_count 1. */
    size_t read_count = 777;
    size_t refusal_count = 777;
    openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, NULL, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* An entry resolved at CMIS 5.3 passed with revision 5.1. */
    openxcvr_resolved_field_t stale[8];
    const size_t stale_count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 3, "media_side_snr",
        OPENXCVR_LANE_ALL, "14h", stale, 8);
    CHECK(stale_count >= 1);
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, stale, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* A local entry copy with row = NULL. */
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    CHECK(snr >= 1);
    openxcvr_resolved_field_t rowless = entries[0];
    rowless.row = NULL;
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &rowless, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
}

/* (i) Rule 8 malformed metadata, each plan-level INVALID_ARGUMENT with
 * counts untouched (ruling 3: unknown enum tags are malformed, never
 * admission refusals): an unknown presence-kind tag, an unknown
 * prerequisite field-kind tag, a zero-length row, an out-of-window row
 * offset, a prerequisite location out of window, a prerequisite shift
 * beyond one byte, a missing prerequisite field string and a row
 * inapplicable at the requested revision. */
static void malformed_metadata(void) {
    openxcvr_resolved_field_t resolved[8];
    const size_t count = resolve_page14h("media_side_snr", resolved, 8);
    CHECK(count >= 1);
    openxcvr_catalog_row_t row;
    openxcvr_resolved_field_t entry;
    size_t read_count = 777;
    size_t refusal_count = 777;
    openxcvr_status status;

    /* Unknown presence-kind tag. */
    row = *resolved[0].row;
    row.presence_kind = (openxcvr_presence_kind_t)7;
    entry = resolved[0];
    entry.row = &row;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Unknown prerequisite field-kind tag. */
    row = *resolved[0].row;
    row.presence_equals.kind = (openxcvr_field_kind_t)99;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Zero-length row. */
    row = *resolved[0].row;
    row.len = 0;
    entry = resolved[0];
    entry.row = &row;
    entry.range.len = 0;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Out-of-window row offset. */
    row = *resolved[0].row;
    row.offset = 255;
    entry = resolved[0];
    entry.row = &row;
    entry.range.offset = 255;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Prerequisite location out of window. */
    row = *resolved[0].row;
    row.presence_equals.loc.offset = 240;
    row.presence_equals.loc.len = 20;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Prerequisite shift beyond one byte (rule 8: shift bounds are
     * validated before any extraction shifts). */
    row = *resolved[0].row;
    row.presence_equals.bit_offset = 9;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Missing prerequisite field string. */
    row = *resolved[0].row;
    row.presence_equals.field = NULL;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* Row inapplicable at the requested revision. */
    row = *resolved[0].row;
    row.has_until = 1;
    row.until_major = 5;
    row.until_minor = 1;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
}

/* (j) C-B1/C-A4: the seven-bit address bound applies to the row itself
 * and to every prerequisite, content-switch, discriminator and external
 * constant location, matching Rust Range::validate at every planner
 * range site. Each hand-built row moves exactly one location kind to
 * address 0x80 (in-window otherwise) and is rejected plan-level with
 * INVALID_ARGUMENT, counts untouched. */
static void seven_bit_address_bound(void) {
    size_t read_count = 777;
    size_t refusal_count = 777;
    openxcvr_status status;

    /* The row's own address (the entry range follows it for
     * consistency, so only the address bound can refuse). */
    openxcvr_resolved_field_t snr[8];
    CHECK(resolve_page14h("media_side_snr", snr, 8) >= 1);
    openxcvr_catalog_row_t row = *snr[0].row;
    row.address = 0x80;
    openxcvr_resolved_field_t entry = snr[0];
    entry.row = &row;
    entry.range.address = 0x80;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* The Equals prerequisite location. */
    row = *snr[0].row;
    row.presence_equals.loc.address = 0x80;
    entry = snr[0];
    entry.row = &row;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* The content-switch data location. */
    row = *snr[0].row;
    row.presence_equals.content_switch.data.address = 0x80;
    entry = snr[0];
    entry.row = &row;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* A variant-discriminator location. */
    openxcvr_resolved_field_t mon[4];
    CHECK(resolve_field(OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
                        OPENXCVR_LANE_UNSPECIFIED, NULL, mon, 4) >= 1);
    openxcvr_catalog_row_t disc_row = *mon[0].row;
    disc_row.has_discriminator = 1;
    disc_row.discriminator.field = "synthetic_discriminator";
    disc_row.discriminator.loc.address = 0x80;
    disc_row.discriminator.loc.page = 0;
    disc_row.discriminator.loc.offset = 92;
    disc_row.discriminator.loc.len = 1;
    disc_row.discriminator.bit = 4;
    openxcvr_resolved_field_t disc_entry = mon[0];
    disc_entry.row = &disc_row;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &disc_entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* SFF-8472 declared external linear constants (ruling 2). */
    openxcvr_resolved_field_t temp[4];
    CHECK(resolve_field(OPENXCVR_SPEC_SFF8472, 12, 5, "temperature",
                        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base",
                        temp, 4) >= 1);
    openxcvr_catalog_row_t temp_row = *temp[0].row;
    temp_row.scaling.linear.slope.address = 0x80;
    openxcvr_resolved_field_t temp_entry = temp[0];
    temp_entry.row = &temp_row;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_SFF8472, 12, 5, &temp_entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* SFF-8472 declared polynomial coefficients (ruling 2). */
    openxcvr_resolved_field_t rxp[4];
    CHECK(resolve_field(OPENXCVR_SPEC_SFF8472, 12, 5, "rx_power",
                        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base",
                        rxp, 4) >= 1);
    openxcvr_catalog_row_t rxp_row = *rxp[0].row;
    rxp_row.scaling.poly.coefficients[2].address = 0x80;
    openxcvr_resolved_field_t rxp_entry = rxp[0];
    rxp_entry.row = &rxp_row;
    status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_SFF8472, 12, 5, &rxp_entry, 1,
        NULL, 0, &read_count, NULL, 0, &refusal_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
}

/* (k) P-B3: an instanced effect row's footprint is the union of its own
 * span and every instance offset span. tuning_complete_flag_tx (CMIS
 * 5.1, Page 12h, clear-on-read, instances at bytes 231..238) puts lanes
 * 2..8 outside the row's own span (byte 231 only); a synthetic Equals
 * prerequisite pointing at byte 233 of Page 12h must be refused
 * CLEAR_ON_READ, not planned. */
static void page12h_instance_effect_prerequisite(void) {
    openxcvr_resolved_field_t resolved[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, resolved, 4);
    CHECK(count >= 1);
    openxcvr_catalog_row_t row = *resolved[0].row;
    row.presence_kind = OPENXCVR_PRESENCE_EQUALS;
    row.presence_equals.field = "synthetic_tuning_complete_flag";
    row.presence_equals.loc.address = 0x50;
    row.presence_equals.loc.page = 18;
    row.presence_equals.loc.offset = 233;
    row.presence_equals.loc.len = 1;
    row.presence_equals.banked = 0;
    row.presence_equals.kind = OPENXCVR_FIELD_U8;
    row.presence_equals.bit_offset = 0;
    row.presence_equals.bit_width = 8;
    row.presence_equals.expected = 1;
    row.presence_equals.has_content_switch = 0;
    openxcvr_resolved_field_t entry = resolved[0];
    entry.row = &row;
    openxcvr_planned_read_t reads[8];
    openxcvr_field_plan_refusal_t refusals[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    const openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
    CHECK(strcmp(refusals[0].field, entry.row->name) == 0);
    CHECK(refusals[0].has_lane == entry.has_lane);
    CHECK(refusals[0].lane == entry.lane);
    /* The refused entry is no survivor, so no gate is planned either. */
    CHECK(read_count == 0);
}

/* (l) Rule 18 / C-A7: prelude refusals are published in input-entry
 * order across both passes. The interleaved case (an entry refused by
 * the shared gate in pass 2 sitting before an entry refused by its own
 * prerequisite in pass 1) needs an effectful family-gate byte, and no
 * generated catalog row carries a restrictive read effect on any
 * family's gate byte (CMIS/SFF-8636 page 0 byte 1, SFF-8472 A0h byte
 * 94) even after the P-B3 instance-footprint fix; hand-built entries
 * are not catalog rows, so no seam reaches it without editing the
 * generated tables. The reachable order is pinned here: refusals from
 * pass 1 keep input-entry order when interleaved with surviving
 * entries. */
static void refusal_order_input_entries(void) {
    openxcvr_resolved_field_t snr[8];
    const size_t snr_count = resolve_page14h("media_side_snr", snr, 8);
    CHECK(snr_count >= 2);
    openxcvr_resolved_field_t eff_b;
    openxcvr_catalog_row_t row_b;
    effectful_entry_at(&snr[1], 8, &eff_b, &row_b);
    openxcvr_resolved_field_t eff_d;
    openxcvr_catalog_row_t row_d;
    effectful_entry_at(&snr[2], 9, &eff_d, &row_d);

    /* A clean, B effectful, C clean, D effectful: the refusals are
     * [B, D] — input-entry order — and the reads are the two identical
     * selector prerequisites (coalesced) plus the gate. */
    openxcvr_resolved_field_t entries[4];
    entries[0] = snr[0];
    entries[1] = eff_b;
    entries[2] = snr[7];
    entries[3] = eff_d;
    openxcvr_planned_read_t reads[8];
    openxcvr_field_plan_refusal_t refusals[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    const openxcvr_status status = openxcvr_field_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 4,
        reads, 8, &read_count, refusals, 8, &refusal_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 2);
    CHECK(strcmp(refusals[0].field, eff_b.row->name) == 0);
    CHECK(refusals[0].has_lane == eff_b.has_lane);
    CHECK(refusals[0].lane == eff_b.lane);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
    CHECK(strcmp(refusals[1].field, eff_d.row->name) == 0);
    CHECK(refusals[1].has_lane == eff_d.has_lane);
    CHECK(refusals[1].lane == eff_d.lane);
    CHECK(refusals[1].reason == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
    CHECK(read_count == 2);
    CHECK(read_is(&reads[0], 0x50, 0, 0, 1, 1)
          || read_is(&reads[1], 0x50, 0, 0, 1, 1));
    CHECK(read_is(&reads[0], 0x50, 0, 0x14, 128, 1)
          || read_is(&reads[1], 0x50, 0, 0x14, 128, 1));
}

int main(void) {
    cmis_page14h_prelude();
    sff8472_prelude();
    unconditional_prelude();
    empty_and_validation();
    effectful_prerequisite();
    bounds();
    transactional();
    malformed();
    malformed_metadata();
    seven_bit_address_bound();
    page12h_instance_effect_prerequisite();
    refusal_order_input_entries();
    printf("OK\n");
    return 0;
}
