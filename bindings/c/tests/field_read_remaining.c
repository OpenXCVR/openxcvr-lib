/*
 * Field read remaining, Stage 1b (C): the openxcvr_field_read_remaining
 * contract — self-contained admission of entries from supplied prelude
 * bytes, with data reads, typed refusals and selector-acquisition
 * requirements for admitted entries — exercised on catalog-resolved
 * entries for CMIS and SFF-8472.
 *
 * No fixtures are opened: admission runs on synthetic observation bytes
 * and hand-built rows (rule 8 malformed metadata, fragmented
 * prerequisites, declared external constants) only.
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

/* True when one planned read covers a range entirely (ruling 6: an
 * admitted entry's full range, not a mere overlap). */
static int read_covers(const openxcvr_planned_read_t *r, const openxcvr_range *e) {
    if (r->range.address != e->address || r->range.bank != e->bank
        || r->range.page != e->page) {
        return 0;
    }
    return r->range.offset <= e->offset
        && (uint32_t)r->range.offset + r->range.len
            >= (uint32_t)e->offset + e->len;
}

/* An SNR lane entry (first of the all-lanes resolve) with its Equals
 * presence prerequisite moved onto the CMIS lower clear-on-read byte 8
 * (0x50 page 0 offset 8 len 1). */
static void effectful_entry(
    openxcvr_resolved_field_t *entry_out, openxcvr_catalog_row_t *row_out
) {
    openxcvr_resolved_field_t resolved[8];
    const size_t count = resolve_page14h("media_side_snr", resolved, 8);
    CHECK(count >= 1);
    *row_out = *resolved[0].row;
    row_out->presence_kind = OPENXCVR_PRESENCE_EQUALS;
    row_out->presence_equals.loc.address = 0x50;
    row_out->presence_equals.loc.page = 0;
    row_out->presence_equals.loc.offset = 8;
    row_out->presence_equals.loc.len = 1;
    *entry_out = resolved[0];
    entry_out->row = row_out;
}

/* A supplied-byte view: up to eight one-byte observations with backing
 * storage kept alive beside them. */
typedef struct { openxcvr_range_observation obs[8]; uint8_t bytes[8][1]; size_t count; } view_t;
static void view_byte(view_t *v, uint8_t address, uint8_t page, uint8_t offset, uint8_t value) {
    openxcvr_range_observation *o = &v->obs[v->count];
    v->bytes[v->count][0] = value;
    o->range = (openxcvr_range){address, 0, page, offset, 1};
    o->bytes = v->bytes[v->count]; o->len = 1; o->present = 1; v->count++;
}

/* True when a range is exactly the given window. */
static int range_eq(const openxcvr_range *r, uint8_t address, uint8_t bank,
                    uint8_t page, uint16_t offset, uint16_t len) {
    return r->address == address && r->bank == bank && r->page == page &&
           r->offset == offset && r->len == len;
}

/* (a) Sixteen Page 14h entries under a view holding exactly the prelude
 * bytes (revision byte 0x51, Page 14h selector 0x06): the eight SNR
 * lanes are admitted with selector-acquisition requirements, the eight
 * BER lanes are refused VIEW_NOT_SELECTED with the row's expected value
 * and the observed selector as actual, every planned read covers some
 * SNR entry, and no planned read is a prelude byte. */
static void snr_view_prelude_bytes_only(void) {
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    const size_t ber = resolve_page14h("media_side_ber", entries + snr, 8);
    CHECK(snr + ber == 16);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0x14, 128, 0x06);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 8);
    for (size_t i = 0; i < refusal_count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED);
        CHECK(strcmp(refusals[i].field, entries[snr + i].row->name) == 0);
        CHECK(refusals[i].has_lane == entries[snr + i].has_lane);
        CHECK(refusals[i].lane == entries[snr + i].lane);
        CHECK(refusals[i].detail.view_not_selected.expected ==
              entries[snr + i].row->presence_equals.expected);
        CHECK(refusals[i].detail.view_not_selected.actual == 6);
    }
    CHECK(acq_count == 8);
    for (size_t i = 0; i < acq_count; i++) {
        CHECK(strcmp(requirements[i].field, entries[i].row->name) == 0);
        CHECK(requirements[i].has_lane == entries[i].has_lane);
        CHECK(requirements[i].lane == entries[i].lane);
        CHECK(requirements[i].selector_value == 6);
        CHECK(range_eq(&requirements[i].selector_range, 0x50, 0, 0x14, 128, 1));
        /* Ruling 1: the data range is the content-switch window with the
         * prerequisite's bank policy, not the entry's own field range. */
        CHECK(range_eq(&requirements[i].data_range, 0x50,
                       entries[i].row->presence_equals.banked
                           ? entries[i].range.bank
                           : 0,
                       0x14, 192, 64));
    }
    for (size_t i = 0; i < read_count; i++) {
        CHECK(!read_is(&reads[i], 0x50, 0, 0, 1, 1));
        CHECK(!read_is(&reads[i], 0x50, 0, 0x14, 128, 1));
        int covered = 0;
        for (size_t j = 0; j < snr; j++) {
            if (ranges_overlap(&reads[i].range, &entries[j].range)) {
                covered = 1;
            }
        }
        CHECK(covered);
    }
    /* Ruling 6: every admitted entry's full range is covered by some
     * planned read, not merely overlapped. */
    for (size_t j = 0; j < snr; j++) {
        int covered = 0;
        for (size_t i = 0; i < read_count; i++) {
            if (read_covers(&reads[i], &entries[j].range)) {
                covered = 1;
            }
        }
        CHECK(covered);
    }
}

/* (b) The same entries under the BER view (selector 0x01): the BER
 * lanes are admitted with selector value 1 and the SNR lanes are
 * refused VIEW_NOT_SELECTED with actual 1. */
static void ber_view_prelude_bytes_only(void) {
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    const size_t ber = resolve_page14h("media_side_ber", entries + snr, 8);
    CHECK(snr + ber == 16);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0x14, 128, 0x01);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 8);
    for (size_t i = 0; i < refusal_count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED);
        CHECK(strcmp(refusals[i].field, entries[i].row->name) == 0);
        CHECK(refusals[i].has_lane == entries[i].has_lane);
        CHECK(refusals[i].lane == entries[i].lane);
        CHECK(refusals[i].detail.view_not_selected.expected ==
              entries[i].row->presence_equals.expected);
        CHECK(refusals[i].detail.view_not_selected.actual == 1);
    }
    CHECK(acq_count == 8);
    for (size_t i = 0; i < acq_count; i++) {
        CHECK(strcmp(requirements[i].field, entries[snr + i].row->name) == 0);
        CHECK(requirements[i].has_lane == entries[snr + i].has_lane);
        CHECK(requirements[i].lane == entries[snr + i].lane);
        CHECK(requirements[i].selector_value == 1);
        CHECK(range_eq(&requirements[i].selector_range, 0x50, 0, 0x14, 128, 1));
    }
    for (size_t i = 0; i < read_count; i++) {
        CHECK(!read_is(&reads[i], 0x50, 0, 0, 1, 1));
        CHECK(!read_is(&reads[i], 0x50, 0, 0x14, 128, 1));
        int covered = 0;
        for (size_t j = 0; j < ber; j++) {
            if (ranges_overlap(&reads[i].range, &entries[snr + j].range)) {
                covered = 1;
            }
        }
        CHECK(covered);
    }
    /* Ruling 6: every admitted entry's full range is covered by some
     * planned read, not merely overlapped. */
    for (size_t j = 0; j < ber; j++) {
        int covered = 0;
        for (size_t i = 0; i < read_count; i++) {
            if (read_covers(&reads[i], &entries[snr + j].range)) {
                covered = 1;
            }
        }
        CHECK(covered);
    }
}

/* (c) BER-only entries under the SNR view: every BER lane is refused
 * VIEW_NOT_SELECTED and nothing is planned or required. */
static void ber_alone_under_snr_view(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t ber = resolve_page14h("media_side_ber", entries, 8);
    CHECK(ber == 8);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0x14, 128, 0x06);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 8);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
    for (size_t i = 0; i < refusal_count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED);
        CHECK(strcmp(refusals[i].field, entries[i].row->name) == 0);
        CHECK(refusals[i].detail.view_not_selected.actual == 6);
    }
}

/* (d) Selector values that select neither family (0x00, 0x07, 0x16,
 * 0xC0): one SNR lane entry and one BER lane entry are both refused
 * VIEW_NOT_SELECTED and nothing is planned or required. */
static void non_selecting_selectors(void) {
    openxcvr_resolved_field_t snr[8];
    openxcvr_resolved_field_t ber[8];
    CHECK(resolve_page14h("media_side_snr", snr, 8) >= 1);
    CHECK(resolve_page14h("media_side_ber", ber, 8) >= 1);
    static const uint8_t selectors[] = {0x00, 0x07, 0x16, 0xC0};
    for (size_t s = 0; s < sizeof selectors / sizeof selectors[0]; s++) {
        openxcvr_resolved_field_t entries[2];
        entries[0] = snr[0];
        entries[1] = ber[0];
        view_t view;
        view.count = 0;
        view_byte(&view, 0x50, 0, 1, 0x51);
        view_byte(&view, 0x50, 0x14, 128, selectors[s]);
        openxcvr_planned_read_t reads[64];
        openxcvr_field_admission_refusal_t refusals[64];
        openxcvr_acquisition_requirement_t requirements[64];
        size_t read_count = 0;
        size_t refusal_count = 0;
        size_t acq_count = 0;
        const openxcvr_status status = openxcvr_field_read_remaining(
            OPENXCVR_SPEC_CMIS, 5, 1, entries, 2,
            view.obs, view.count,
            reads, 64, &read_count, refusals, 64, &refusal_count,
            requirements, 64, &acq_count);
        CHECK(status == OPENXCVR_OK);
        CHECK(refusal_count == 2);
        CHECK(read_count == 0);
        CHECK(acq_count == 0);
        CHECK(refusals[0].reason == OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED);
        CHECK(strcmp(refusals[0].field, snr[0].row->name) == 0);
        CHECK(refusals[0].has_lane == snr[0].has_lane);
        CHECK(refusals[0].lane == snr[0].lane);
        CHECK(refusals[0].detail.view_not_selected.actual == selectors[s]);
        CHECK(refusals[1].reason == OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED);
        CHECK(strcmp(refusals[1].field, ber[0].row->name) == 0);
        CHECK(refusals[1].has_lane == ber[0].has_lane);
        CHECK(refusals[1].lane == ber[0].lane);
        CHECK(refusals[1].detail.view_not_selected.actual == selectors[s]);
    }
}

/* (e) Revision gate over supplied bytes: a view whose revision byte
 * reports CMIS 3.0 refuses every entry UNSUPPORTED_REVISION naming the
 * observed revision; a view with only the selector byte refuses every
 * entry PRELUDE_MISSING naming the revision-byte range; an empty
 * non-NULL view does the same. Refusals are result records, so the
 * status stays OK and nothing is planned or required. */
static void revision_refusals(void) {
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    const size_t ber = resolve_page14h("media_side_ber", entries + snr, 8);
    CHECK(snr + ber == 16);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    /* The module reports CMIS 3.0 against the requested 5.1. */
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x30);
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 16);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
    for (size_t i = 0; i < refusal_count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_REVISION);
        CHECK(strcmp(refusals[i].field, entries[i].row->name) == 0);
        CHECK(refusals[i].detail.unsupported_revision.family ==
              OPENXCVR_SPEC_CMIS);
        CHECK(refusals[i].detail.unsupported_revision.major == 3);
        CHECK(refusals[i].detail.unsupported_revision.minor == 0);
    }

    /* Only the selector byte: the revision byte is the missing prelude
     * byte named by every refusal. */
    view.count = 0;
    view_byte(&view, 0x50, 0x14, 128, 0x06);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 16);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
    for (size_t i = 0; i < refusal_count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_PRELUDE_MISSING);
        CHECK(range_eq(&refusals[i].detail.prelude_missing, 0x50, 0, 0, 1, 1));
    }

    /* An empty view (non-NULL observations, count zero): same. */
    view.count = 0;
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 16);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
    for (size_t i = 0; i < refusal_count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_PRELUDE_MISSING);
        CHECK(range_eq(&refusals[i].detail.prelude_missing, 0x50, 0, 0, 1, 1));
    }
}

/* (f) SFF-8472 optional_laser_temperature at 12.5: with the external
 * calibration bit clear and the compliance byte 0x0a the entry is
 * admitted and its range is read; with the calibration bit set it is
 * refused PREREQUISITE_NOT_SATISFIED with actual 1 and no read touches
 * its range. */
static void sff8472_conditional(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_SFF8472, 12, 5, "optional_laser_temperature",
        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base", entries, 4);
    CHECK(count >= 1);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 92, 0x00);
    view_byte(&view, 0x50, 0, 94, 0x0a);
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, entries, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    int covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        if (ranges_overlap(&reads[i].range, &entries[0].range)) {
            covered = 1;
        }
    }
    CHECK(covered);

    view.count = 0;
    view_byte(&view, 0x50, 0, 92, 0x10);
    view_byte(&view, 0x50, 0, 94, 0x0a);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, entries, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason ==
          OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED);
    CHECK(strcmp(refusals[0].field, entries[0].row->name) == 0);
    CHECK(refusals[0].has_lane == entries[0].has_lane);
    CHECK(refusals[0].lane == entries[0].lane);
    CHECK(strcmp(refusals[0].detail.prerequisite_not_satisfied.prerequisite,
                 entries[0].row->presence_equals.field) == 0);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.expected ==
          entries[0].row->presence_equals.expected);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.actual == 1);
    for (size_t i = 0; i < read_count; i++) {
        CHECK(!ranges_overlap(&reads[i].range, &entries[0].range));
    }
    CHECK(read_count == 0);
}

/* (g) An unconditional monitor (temp_mon_value) needs only the revision
 * byte: no refusals, no acquisition requirements, and its range is
 * read. */
static void unconditional_monitor(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, entries, 4);
    CHECK(count >= 1);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    int covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        if (ranges_overlap(&reads[i].range, &entries[0].range)) {
            covered = 1;
        }
    }
    CHECK(covered);
}

/* (h) An Equals prerequisite on a clear-on-read byte is refused EFFECT
 * whether or not the view supplies that byte: the effect check filters
 * on the prerequisite's location, not on its presence in the view. */
static void effectful_prerequisite_unfiltered(void) {
    openxcvr_resolved_field_t entry;
    openxcvr_catalog_row_t row;
    effectful_entry(&entry, &row);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    /* The clear-on-read byte supplied with its expected value. */
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 8, (uint8_t)row.presence_equals.expected);
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_ADMISSION_EFFECT);
    CHECK(refusals[0].detail.effect == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
    CHECK(strcmp(refusals[0].field, entry.row->name) == 0);
    CHECK(refusals[0].has_lane == entry.has_lane);
    CHECK(refusals[0].lane == entry.lane);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);

    /* The clear-on-read byte absent from the view. */
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_ADMISSION_EFFECT);
    CHECK(refusals[0].detail.effect == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* (i) A non-scalar (BYTES) prerequisite kind cannot be decoded into the
 * expected scalar: the entry is refused PREREQUISITE_UNDECODABLE naming
 * the prerequisite, and nothing is planned or required. */
static void undecodable_prerequisite(void) {
    openxcvr_resolved_field_t resolved[8];
    const size_t count = resolve_page14h("media_side_snr", resolved, 8);
    CHECK(count >= 1);
    openxcvr_catalog_row_t row = *resolved[0].row;
    row.presence_equals.kind = OPENXCVR_FIELD_BYTES;
    row.presence_equals.expected = UINT64_MAX;
    openxcvr_resolved_field_t entry = resolved[0];
    entry.row = &row;
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0x14, 128, 0x06);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason ==
          OPENXCVR_FIELD_ADMISSION_PREREQUISITE_UNDECODABLE);
    CHECK(refusals[0].detail.prerequisite_undecodable.prerequisite != NULL);
    CHECK(strcmp(refusals[0].field, entry.row->name) == 0);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* (j) An UNSUPPORTED presence kind with an expression string: the entry
 * is refused UNSUPPORTED_PRESENCE and the refusal borrows exactly that
 * expression pointer. */
static void unsupported_presence(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, entries, 4);
    CHECK(count >= 1);
    openxcvr_catalog_row_t row = *entries[0].row;
    const char *const expr = "synthetic_expr >= 1";
    row.presence_kind = OPENXCVR_PRESENCE_UNSUPPORTED;
    row.presence_expr = expr;
    openxcvr_resolved_field_t entry = entries[0];
    entry.row = &row;
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_PRESENCE);
    CHECK(refusals[0].detail.unsupported_presence_expr == expr);
    CHECK(strcmp(refusals[0].field, entry.row->name) == 0);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* (k) Refusals follow entry order: a refused BER lane, an admitted
 * unconditional monitor, the effectful entry, an unsupported row, and
 * an admitted SNR lane produce exactly three refusals in that order,
 * one acquisition requirement for the SNR lane, and reads covering both
 * admitted entries. */
static void refusal_order(void) {
    openxcvr_resolved_field_t ber[8];
    CHECK(resolve_page14h("media_side_ber", ber, 8) >= 1);
    openxcvr_resolved_field_t mon[4];
    CHECK(resolve_field(OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
                        OPENXCVR_LANE_UNSPECIFIED, NULL, mon, 4) >= 1);
    openxcvr_resolved_field_t effect_entry;
    openxcvr_catalog_row_t effect_row;
    effectful_entry(&effect_entry, &effect_row);
    openxcvr_catalog_row_t unsup_row = *mon[0].row;
    unsup_row.presence_kind = OPENXCVR_PRESENCE_UNSUPPORTED;
    unsup_row.presence_expr = "synthetic_expr >= 1";
    openxcvr_resolved_field_t unsup_entry = mon[0];
    unsup_entry.row = &unsup_row;
    openxcvr_resolved_field_t snr[8];
    CHECK(resolve_page14h("media_side_snr", snr, 8) >= 1);

    openxcvr_resolved_field_t entries[5];
    entries[0] = ber[0];
    entries[1] = mon[0];
    entries[2] = effect_entry;
    entries[3] = unsup_entry;
    entries[4] = snr[0];

    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0x14, 128, 0x06);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 5,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 3);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED);
    CHECK(strcmp(refusals[0].field, ber[0].row->name) == 0);
    CHECK(refusals[0].detail.view_not_selected.actual == 6);
    CHECK(refusals[1].reason == OPENXCVR_FIELD_ADMISSION_EFFECT);
    CHECK(strcmp(refusals[1].field, effect_entry.row->name) == 0);
    CHECK(refusals[2].reason == OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_PRESENCE);
    CHECK(strcmp(refusals[2].field, unsup_entry.row->name) == 0);
    CHECK(acq_count == 1);
    CHECK(strcmp(requirements[0].field, snr[0].row->name) == 0);
    CHECK(requirements[0].has_lane == snr[0].has_lane);
    CHECK(requirements[0].lane == snr[0].lane);
    CHECK(requirements[0].selector_value == 6);
    int mon_covered = 0;
    int snr_covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        if (ranges_overlap(&reads[i].range, &mon[0].range)) {
            mon_covered = 1;
        }
        if (ranges_overlap(&reads[i].range, &snr[0].range)) {
            snr_covered = 1;
        }
    }
    CHECK(mon_covered);
    CHECK(snr_covered);
}

/* (l) Transactional contract on the case (a) query: the three-output
 * size query; SHORT_BUFFER on any one output publishes all three counts
 * and touches no array; required pointers; and the non-NULL ranges
 * rule. */
static void transactional(void) {
    openxcvr_resolved_field_t entries[16];
    const size_t snr = resolve_page14h("media_side_snr", entries, 8);
    const size_t ber = resolve_page14h("media_side_ber", entries + snr, 8);
    CHECK(snr + ber == 16);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0x14, 128, 0x06);

    /* All three outputs NULL with capacity zero: a size query. */
    size_t read_count = 777;
    size_t refusal_count = 777;
    size_t acq_count = 777;
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        NULL, 0, &read_count, NULL, 0, &refusal_count,
        NULL, 0, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 8);
    CHECK(acq_count == 8);
    CHECK(read_count > 0);
    const size_t n = read_count;

    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    memset(reads, 0xA5, sizeof reads);
    memset(refusals, 0xA5, sizeof refusals);
    memset(requirements, 0xA5, sizeof requirements);
    unsigned char reads_canary[sizeof reads];
    unsigned char refusals_canary[sizeof refusals];
    unsigned char requirements_canary[sizeof requirements];
    memcpy(reads_canary, reads, sizeof reads);
    memcpy(refusals_canary, refusals, sizeof refusals);
    memcpy(requirements_canary, requirements, sizeof requirements);

    /* read capacity n-1: SHORT_BUFFER publishes the counts and leaves
     * all three canary arrays untouched. */
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, n - 1, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_SHORT_BUFFER);
    CHECK(read_count == n);
    CHECK(refusal_count == 8);
    CHECK(acq_count == 8);
    CHECK(memcmp(reads, reads_canary, sizeof reads) == 0);
    CHECK(memcmp(refusals, refusals_canary, sizeof refusals) == 0);
    CHECK(memcmp(requirements, requirements_canary,
                 sizeof requirements) == 0);

    /* refusal capacity 7: same. */
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 7, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_SHORT_BUFFER);
    CHECK(read_count == n);
    CHECK(refusal_count == 8);
    CHECK(acq_count == 8);
    CHECK(memcmp(reads, reads_canary, sizeof reads) == 0);
    CHECK(memcmp(refusals, refusals_canary, sizeof refusals) == 0);
    CHECK(memcmp(requirements, requirements_canary,
                 sizeof requirements) == 0);

    /* acquisition capacity 7: same. */
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 7, &acq_count);
    CHECK(status == OPENXCVR_ERR_SHORT_BUFFER);
    CHECK(read_count == n);
    CHECK(refusal_count == 8);
    CHECK(acq_count == 8);
    CHECK(memcmp(reads, reads_canary, sizeof reads) == 0);
    CHECK(memcmp(refusals, refusals_canary, sizeof refusals) == 0);
    CHECK(memcmp(requirements, requirements_canary,
                 sizeof requirements) == 0);

    /* reads NULL with capacity 4 is invalid, counts untouched. */
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        NULL, 4, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* refusals NULL with capacity 4: same. */
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, NULL, 4, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* requirements NULL with capacity 4: same. */
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        NULL, 4, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* All three count pointers are required. */
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, NULL, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
    read_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, NULL,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(acq_count == 777);
    read_count = 777;
    refusal_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, NULL);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);

    /* ranges NULL is invalid even with range_count zero. */
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, snr + ber,
        NULL, 0,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
}

/* (m) Validation: the requested-revision gate applies with no entries;
 * an entry resolved at another revision than the request is invalid;
 * 257 entries exceed OPENXCVR_FIELD_PLAN_MAX_ENTRIES with sentinels and
 * canaries unchanged; zero entries with a valid view is an empty OK
 * query. */
static void validation(void) {
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    openxcvr_planned_read_t reads[64];
    openxcvr_field_admission_refusal_t refusals[64];
    openxcvr_acquisition_requirement_t requirements[64];
    size_t read_count = 777;
    size_t refusal_count = 777;
    size_t acq_count = 777;

    /* CMIS 5.5 is not a supported revision, even with no entries. */
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 5, NULL, 0,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* An SNR entry resolved at CMIS 5.3 passed with 5.1. */
    openxcvr_resolved_field_t stale[8];
    const size_t stale_count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 3, "media_side_snr",
        OPENXCVR_LANE_ALL, "14h", stale, 8);
    CHECK(stale_count >= 1);
    view_t sel_view;
    sel_view.count = 0;
    view_byte(&sel_view, 0x50, 0, 1, 0x51);
    view_byte(&sel_view, 0x50, 0x14, 128, 0x06);
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, stale, 1,
        sel_view.obs, sel_view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* 257 entries exceed the entry bound; nothing is published. */
    openxcvr_resolved_field_t one[4];
    CHECK(resolve_field(OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
                        OPENXCVR_LANE_UNSPECIFIED, NULL, one, 4) >= 1);
    openxcvr_resolved_field_t many[257];
    for (size_t i = 0; i < 257; i++) {
        many[i] = one[0];
    }
    memset(reads, 0xA5, sizeof reads);
    memset(refusals, 0xA5, sizeof refusals);
    memset(requirements, 0xA5, sizeof requirements);
    unsigned char reads_canary[sizeof reads];
    unsigned char refusals_canary[sizeof refusals];
    unsigned char requirements_canary[sizeof requirements];
    memcpy(reads_canary, reads, sizeof reads);
    memcpy(refusals_canary, refusals, sizeof refusals);
    memcpy(requirements_canary, requirements, sizeof requirements);
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, many, 257,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_ERR_OUT_OF_RANGE);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
    CHECK(memcmp(reads, reads_canary, sizeof reads) == 0);
    CHECK(memcmp(refusals, refusals_canary, sizeof refusals) == 0);
    CHECK(memcmp(requirements, requirements_canary,
                 sizeof requirements) == 0);

    /* Zero entries with a valid view: an empty query. */
    read_count = 777;
    refusal_count = 777;
    acq_count = 777;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, NULL, 0,
        view.obs, view.count,
        reads, 64, &read_count, refusals, 64, &refusal_count,
        requirements, 64, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(read_count == 0);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
}

/* (n) Rule 8 malformed entries through remaining, counts untouched:
 * an unknown prerequisite field-kind tag is plan-level INVALID_ARGUMENT
 * — the ruling 3 boundary, since the known non-scalar kind of case (i)
 * is instead an admission refusal — a zero-length or out-of-window
 * linear constant location is malformed (ruling 2), a banked entry
 * range on a non-banked row is inconsistent, an entry range disagreeing
 * with the row's own footprint is invalid geometry, and a discriminator
 * with a NULL field string or an out-of-window location is malformed. */
static void malformed_metadata(void) {
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 777;
    size_t refusal_count = 777;
    size_t acq_count = 777;
    openxcvr_status status;

    /* Unknown prerequisite field-kind tag (contrast case (i)). */
    openxcvr_resolved_field_t snr[8];
    CHECK(resolve_page14h("media_side_snr", snr, 8) >= 1);
    openxcvr_catalog_row_t snr_row = *snr[0].row;
    snr_row.presence_equals.kind = (openxcvr_field_kind_t)99;
    openxcvr_resolved_field_t snr_entry = snr[0];
    snr_entry.row = &snr_row;
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &snr_entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* SFF-8472 externally-calibrated monitors carry the declared
     * constants ruling 2 validates. */
    openxcvr_resolved_field_t temp[4];
    CHECK(resolve_field(OPENXCVR_SPEC_SFF8472, 12, 5, "temperature",
                        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base",
                        temp, 4) >= 1);
    view_t a2;
    a2.count = 0;
    view_byte(&a2, 0x50, 0, 94, 0x0a);

    /* Zero-length linear slope location (ruling 2). */
    openxcvr_catalog_row_t temp_row = *temp[0].row;
    temp_row.scaling.linear.slope.len = 0;
    openxcvr_resolved_field_t temp_entry = temp[0];
    temp_entry.row = &temp_row;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, &temp_entry, 1,
        a2.obs, a2.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* Out-of-window linear offset constant (ruling 2). */
    temp_row = *temp[0].row;
    temp_row.scaling.linear.offset.offset = 255;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, &temp_entry, 1,
        a2.obs, a2.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* A banked entry range on a non-banked row. */
    openxcvr_resolved_field_t mon[4];
    CHECK(resolve_field(OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
                        OPENXCVR_LANE_UNSPECIFIED, NULL, mon, 4) >= 1);
    openxcvr_resolved_field_t banked = mon[0];
    banked.range.bank = 3;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &banked, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* An entry range disagreeing with the row's own footprint. */
    openxcvr_resolved_field_t moved = mon[0];
    moved.range.offset = 20;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &moved, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* A discriminator with a NULL field string. */
    openxcvr_catalog_row_t disc_row = *mon[0].row;
    disc_row.has_discriminator = 1;
    disc_row.discriminator.field = NULL;
    disc_row.discriminator.loc.address = 0x50;
    disc_row.discriminator.loc.page = 0;
    disc_row.discriminator.loc.offset = 92;
    disc_row.discriminator.loc.len = 1;
    disc_row.discriminator.bit = 4;
    openxcvr_resolved_field_t disc_entry = mon[0];
    disc_entry.row = &disc_row;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &disc_entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* A discriminator location out of window. */
    disc_row.discriminator.field = "synthetic_discriminator";
    disc_row.discriminator.loc.offset = 250;
    disc_row.discriminator.loc.len = 10;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &disc_entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
}

/* (o) Rule 9 malformed views: a zero-length observation range, a range
 * ending past the 256-byte window, an address beyond the seven-bit I2C
 * range, a present observation with no bytes, a present length mismatch
 * and two overlapping present observations in one window are each
 * INVALID_ARGUMENT before publication, with the counts untouched. */
static void malformed_views(void) {
    openxcvr_resolved_field_t entries[4];
    CHECK(resolve_field(OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
                        OPENXCVR_LANE_UNSPECIFIED, NULL, entries, 4) >= 1);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 777;
    size_t refusal_count = 777;
    size_t acq_count = 777;
    uint8_t backing[2] = {0x51, 0x51};

    /* Zero-length range. */
    openxcvr_range_observation bad;
    memset(&bad, 0, sizeof bad);
    bad.range = (openxcvr_range){0x50, 0, 0, 1, 0};
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        &bad, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* Range ending past the window. */
    bad.range = (openxcvr_range){0x50, 0, 0, 250, 10};
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        &bad, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* Address beyond the seven-bit I2C range. */
    bad.range = (openxcvr_range){0x80, 0, 0, 1, 1};
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        &bad, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* Present observation with no bytes. */
    bad.range = (openxcvr_range){0x50, 0, 0, 1, 1};
    bad.present = 1;
    bad.bytes = NULL;
    bad.len = 1;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        &bad, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* Present length mismatch. */
    bad.bytes = backing;
    bad.len = 2;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        &bad, 1,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* Two overlapping present observations in one window. */
    openxcvr_range_observation pair[2];
    memset(pair, 0, sizeof pair);
    pair[0].range = (openxcvr_range){0x50, 0, 0, 1, 2};
    pair[0].bytes = backing;
    pair[0].len = 2;
    pair[0].present = 1;
    pair[1].range = (openxcvr_range){0x50, 0, 0, 2, 1};
    pair[1].bytes = backing;
    pair[1].len = 1;
    pair[1].present = 1;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, 1,
        pair, 2,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
}

/* (p) Ruling 2 / rule 15: an admitted SFF-8472 externally-calibrated
 * monitor (temperature at 12.5, unconditional presence) plans its own
 * range, its variant discriminator and its declared external linear
 * constants from metadata — never conditionally on the discriminator
 * byte (contrast basic_read_remaining's discriminator-driven selection,
 * which remaining must not reuse). The two touching constant ranges
 * coalesce; purposes follow the contributor rule — the discriminator
 * read carries the discriminator's schema field path, constants and own
 * bytes the row's name — and every read is a page-0 read, so none is
 * optional. */
static void linear_constants_from_metadata(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_SFF8472, 12, 5, "temperature",
        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base", entries, 4);
    CHECK(count >= 1);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 94, 0x0a);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, entries, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    CHECK(read_count == 3);
    CHECK(read_is(&reads[0], 0x50, 0, 0, 92, 1));
    CHECK(strcmp(reads[0].purpose, entries[0].row->discriminator.field) == 0);
    CHECK(read_is(&reads[1], 0x51, 0, 0, 84, 4));
    CHECK(strcmp(reads[1].purpose, entries[0].row->name) == 0);
    CHECK(read_is(&reads[2], 0x51, 0, 0, 96, 2));
    CHECK(strcmp(reads[2].purpose, entries[0].row->name) == 0);
    CHECK(reads[0].optional == 0);
    CHECK(reads[1].optional == 0);
    CHECK(reads[2].optional == 0);
}

/* (q) Rule 9 fragmented prerequisites: a two-byte U16BE prerequisite is
 * supplied by two adjacent one-byte observations and decoded across
 * them; a missing fragment is PRELUDE_MISSING naming the whole
 * prerequisite range, and an unequal assembled code is
 * PREREQUISITE_NOT_SATISFIED with the assembled actual. The prerequisite
 * range itself is never planned as an admission read. */
static void fragmented_prerequisite(void) {
    openxcvr_resolved_field_t resolved[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, resolved, 4);
    CHECK(count >= 1);
    openxcvr_catalog_row_t row = *resolved[0].row;
    row.presence_kind = OPENXCVR_PRESENCE_EQUALS;
    row.presence_equals.field = "synthetic_u16_gate";
    row.presence_equals.loc.address = 0x50;
    row.presence_equals.loc.page = 0;
    row.presence_equals.loc.offset = 200;
    row.presence_equals.loc.len = 2;
    row.presence_equals.banked = 0;
    row.presence_equals.kind = OPENXCVR_FIELD_U16BE;
    row.presence_equals.bit_offset = 0;
    row.presence_equals.bit_width = 16;
    row.presence_equals.expected = 0x1234;
    row.presence_equals.has_content_switch = 0;
    openxcvr_resolved_field_t entry = resolved[0];
    entry.row = &row;
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    /* Both fragments present: the prerequisite is decoded across them and
     * the entry is admitted. */
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 200, 0x12);
    view_byte(&view, 0x50, 0, 201, 0x34);
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    int covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        const openxcvr_range fragments = {0x50, 0, 0, 200, 2};
        CHECK(!ranges_overlap(&reads[i].range, &fragments));
        if (read_covers(&reads[i], &entry.range)) {
            covered = 1;
        }
    }
    CHECK(covered);

    /* One fragment missing: PRELUDE_MISSING names the whole prerequisite
     * range. */
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 200, 0x12);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason == OPENXCVR_FIELD_ADMISSION_PRELUDE_MISSING);
    CHECK(range_eq(&refusals[0].detail.prelude_missing, 0x50, 0, 0, 200, 2));
    CHECK(strcmp(refusals[0].field, entry.row->name) == 0);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);

    /* Both fragments present, unequal code: the assembled actual is
     * reported. */
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 200, 0x12);
    view_byte(&view, 0x50, 0, 201, 0x35);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason ==
          OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED);
    CHECK(strcmp(refusals[0].detail.prerequisite_not_satisfied.prerequisite,
                 "synthetic_u16_gate") == 0);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.expected == 0x1234);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.actual == 0x1235);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* (r) C-B1/C-A4: the seven-bit address bound applies to the row itself
 * and to every prerequisite, content-switch, discriminator and external
 * constant location. Each hand-built row moves exactly one location
 * kind to address 0x80 (in-window otherwise) and is rejected plan-level
 * with INVALID_ARGUMENT before publication: counts and output arrays
 * untouched. */
static void seven_bit_address_bound(void) {
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    memset(reads, 0xA5, sizeof reads);
    memset(refusals, 0xA5, sizeof refusals);
    memset(requirements, 0xA5, sizeof requirements);
    unsigned char reads_canary[sizeof reads];
    unsigned char refusals_canary[sizeof refusals];
    unsigned char requirements_canary[sizeof requirements];
    memcpy(reads_canary, reads, sizeof reads);
    memcpy(refusals_canary, refusals, sizeof refusals);
    memcpy(requirements_canary, requirements, sizeof requirements);
    size_t read_count = 777;
    size_t refusal_count = 777;
    size_t acq_count = 777;
    openxcvr_status status;

    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);

    /* The row's own address (the entry range follows it for
     * consistency, so only the address bound can refuse). */
    openxcvr_resolved_field_t snr[8];
    CHECK(resolve_page14h("media_side_snr", snr, 8) >= 1);
    openxcvr_catalog_row_t row = *snr[0].row;
    row.address = 0x80;
    openxcvr_resolved_field_t entry = snr[0];
    entry.row = &row;
    entry.range.address = 0x80;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
    CHECK(memcmp(reads, reads_canary, sizeof reads) == 0);
    CHECK(memcmp(refusals, refusals_canary, sizeof refusals) == 0);
    CHECK(memcmp(requirements, requirements_canary,
                 sizeof requirements) == 0);

    /* The Equals prerequisite location. */
    row = *snr[0].row;
    row.presence_equals.loc.address = 0x80;
    entry = snr[0];
    entry.row = &row;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* The content-switch data location. */
    row = *snr[0].row;
    row.presence_equals.content_switch.data.address = 0x80;
    entry = snr[0];
    entry.row = &row;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

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
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &disc_entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* SFF-8472 declared external linear constants (ruling 2). */
    openxcvr_resolved_field_t temp[4];
    CHECK(resolve_field(OPENXCVR_SPEC_SFF8472, 12, 5, "temperature",
                        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base",
                        temp, 4) >= 1);
    view_t a2;
    a2.count = 0;
    view_byte(&a2, 0x50, 0, 94, 0x0a);
    openxcvr_catalog_row_t temp_row = *temp[0].row;
    temp_row.scaling.linear.slope.address = 0x80;
    openxcvr_resolved_field_t temp_entry = temp[0];
    temp_entry.row = &temp_row;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, &temp_entry, 1,
        a2.obs, a2.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);

    /* SFF-8472 declared polynomial coefficients (ruling 2). */
    openxcvr_resolved_field_t rxp[4];
    CHECK(resolve_field(OPENXCVR_SPEC_SFF8472, 12, 5, "rx_power",
                        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base",
                        rxp, 4) >= 1);
    openxcvr_catalog_row_t rxp_row = *rxp[0].row;
    rxp_row.scaling.poly.coefficients[2].address = 0x80;
    openxcvr_resolved_field_t rxp_entry = rxp[0];
    rxp_entry.row = &rxp_row;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, &rxp_entry, 1,
        a2.obs, a2.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(read_count == 777);
    CHECK(refusal_count == 777);
    CHECK(acq_count == 777);
}

/* (s) P-B3: an applicable effect row's footprint is the union of its
 * own span and every instance offset span. CMIS 5.1
 * tuning_complete_flag_tx is clear-on-read with instances at Page 12h
 * bytes 231..238 while its row span covers byte 231 only: every lane
 * (including lanes 2..8, whose bytes sit outside the row span) must be
 * refused EFFECT / CLEAR_ON_READ and nothing may be planned. */
static void instanced_effect_footprint(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "tuning_complete_flag_tx",
        OPENXCVR_LANE_ALL, NULL, entries, 8);
    CHECK(count == 8);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, count,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == count);
    for (size_t i = 0; i < count; i++) {
        CHECK(refusals[i].reason == OPENXCVR_FIELD_ADMISSION_EFFECT);
        CHECK(refusals[i].detail.effect == OPENXCVR_FIELD_PLAN_CLEAR_ON_READ);
        CHECK(strcmp(refusals[i].field, entries[i].row->name) == 0);
        CHECK(refusals[i].has_lane == entries[i].has_lane);
        CHECK(refusals[i].lane == entries[i].lane);
    }
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* A hand-built Equals prerequisite of a chosen scalar kind at one
 * lower-memory location, on an otherwise valid unconditional row. */
static void scalar_prereq_entry(
    openxcvr_field_kind_t kind, uint8_t bit_offset, uint16_t bit_width,
    uint64_t expected, openxcvr_resolved_field_t *entry_out,
    openxcvr_catalog_row_t *row_out
) {
    openxcvr_resolved_field_t resolved[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "temp_mon_value",
        OPENXCVR_LANE_UNSPECIFIED, NULL, resolved, 4);
    CHECK(count >= 1);
    *row_out = *resolved[0].row;
    row_out->presence_kind = OPENXCVR_PRESENCE_EQUALS;
    row_out->presence_equals.field = "synthetic_scalar_gate";
    row_out->presence_equals.loc.address = 0x50;
    row_out->presence_equals.loc.page = 0;
    row_out->presence_equals.loc.offset = 100;
    row_out->presence_equals.loc.len = 8;
    row_out->presence_equals.banked = 0;
    row_out->presence_equals.kind = kind;
    row_out->presence_equals.bit_offset = bit_offset;
    row_out->presence_equals.bit_width = bit_width;
    row_out->presence_equals.expected = expected;
    row_out->presence_equals.has_content_switch = 0;
    *entry_out = resolved[0];
    entry_out->row = row_out;
}

/* (t) C-A1: an ENUM_REF prerequisite with len != 2 is extracted exactly
 * as Rust raw_bits_at extracts it — (byte >> bit_offset) & mask with
 * width = min(bit_width, 8) clamped to 1..8 — never as the raw byte.
 * With bit_offset 3, bit_width 4 and byte 0x51 the code is
 * (0x51 >> 3) & 0x0f = 10, so an expectation of the raw byte 0x51 is
 * refused with actual 10, and an expectation of 10 admits. */
static void enum_ref_shifted_prerequisite(void) {
    openxcvr_resolved_field_t entry;
    openxcvr_catalog_row_t row;
    scalar_prereq_entry(OPENXCVR_FIELD_ENUM_REF, 3, 4, 0x51,
                        &entry, &row);
    row.presence_equals.loc.len = 1;
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 100, 0x51);
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason ==
          OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED);
    CHECK(strcmp(refusals[0].detail.prerequisite_not_satisfied.prerequisite,
                 "synthetic_scalar_gate") == 0);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.expected == 0x51);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.actual == 10);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);

    /* The shifted code satisfies the expectation: the entry is
     * admitted and its own range is planned. */
    row.presence_equals.expected = 10;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 100, 0x51);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    int covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        const openxcvr_range gate = {0x50, 0, 0, 100, 1};
        CHECK(!ranges_overlap(&reads[i].range, &gate));
        if (read_covers(&reads[i], &entry.range)) {
            covered = 1;
        }
    }
    CHECK(covered);
}

/* (u) C-A2 (ruling 3 corrected): wide-integer prerequisites decode
 * exactly as Rust raw_bits_at decodes them. A U32BE prerequisite
 * supplied by four adjacent one-byte fragments decodes big-endian across
 * them (admitting on 0x11223344 and refusing with the assembled actual
 * on a one-byte disagreement). */
static void u32be_prerequisite_fragmented(void) {
    openxcvr_resolved_field_t entry;
    openxcvr_catalog_row_t row;
    scalar_prereq_entry(OPENXCVR_FIELD_U32BE, 0, 32, 0x11223344,
                        &entry, &row);
    row.presence_equals.loc.len = 4;
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 100, 0x11);
    view_byte(&view, 0x50, 0, 101, 0x22);
    view_byte(&view, 0x50, 0, 102, 0x33);
    view_byte(&view, 0x50, 0, 103, 0x44);
    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    int covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        const openxcvr_range gate = {0x50, 0, 0, 100, 4};
        CHECK(!ranges_overlap(&reads[i].range, &gate));
        if (read_covers(&reads[i], &entry.range)) {
            covered = 1;
        }
    }
    CHECK(covered);

    /* One byte off: the assembled big-endian actual is reported. */
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    view_byte(&view, 0x50, 0, 100, 0x11);
    view_byte(&view, 0x50, 0, 101, 0x22);
    view_byte(&view, 0x50, 0, 102, 0x33);
    view_byte(&view, 0x50, 0, 103, 0x45);
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason ==
          OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.expected == 0x11223344);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.actual == 0x11223345);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* (v) C-A2 (ruling 3 corrected): a U64LE prerequisite decodes eight
 * bytes little-endian: bytes 01..08 admit on 0x0807060504030201 (the
 * big-endian order 0x0102030405060708 is refused with that little-
 * endian actual, pinning the byte order). */
static void u64le_prerequisite(void) {
    openxcvr_resolved_field_t entry;
    openxcvr_catalog_row_t row;
    scalar_prereq_entry(OPENXCVR_FIELD_U64LE, 0, 64,
                        0x0807060504030201ull, &entry, &row);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;

    uint8_t rev = 0x51;
    uint8_t backing[8] = {0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08};
    openxcvr_range_observation obs[2];
    memset(obs, 0, sizeof obs);
    obs[0].range = (openxcvr_range){0x50, 0, 0, 1, 1};
    obs[0].bytes = &rev;
    obs[0].len = 1;
    obs[0].present = 1;
    obs[1].range = (openxcvr_range){0x50, 0, 0, 100, 8};
    obs[1].bytes = backing;
    obs[1].len = 8;
    obs[1].present = 1;

    openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        obs, 2,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    int covered = 0;
    for (size_t i = 0; i < read_count; i++) {
        const openxcvr_range gate = {0x50, 0, 0, 100, 8};
        CHECK(!ranges_overlap(&reads[i].range, &gate));
        if (read_covers(&reads[i], &entry.range)) {
            covered = 1;
        }
    }
    CHECK(covered);

    /* The big-endian reading of the same bytes is not the code. */
    row.presence_equals.expected = 0x0102030405060708ull;
    read_count = 0;
    refusal_count = 0;
    acq_count = 0;
    status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, &entry, 1,
        obs, 2,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 1);
    CHECK(refusals[0].reason ==
          OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED);
    CHECK(refusals[0].detail.prerequisite_not_satisfied.actual
          == 0x0807060504030201ull);
    CHECK(read_count == 0);
    CHECK(acq_count == 0);
}

/* (w) R-B2 mirrored in C: an SFF-8472 compliance byte that does not
 * admit a decode at the requested revision refuses every entry with
 * COMPLIANCE_REFUSED carrying that byte — 09h names Rev 12.4 against
 * the requested 12.5, 00h names no revision — and nothing is planned
 * or required. */
static void compliance_refused(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_SFF8472, 12, 5, "optional_laser_temperature",
        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base", entries, 4);
    CHECK(count >= 1);
    static const uint8_t codes[] = {0x09, 0x00};
    for (size_t c = 0; c < sizeof codes / sizeof codes[0]; c++) {
        view_t view;
        view.count = 0;
        view_byte(&view, 0x50, 0, 94, codes[c]);
        openxcvr_planned_read_t reads[8];
        openxcvr_field_admission_refusal_t refusals[8];
        openxcvr_acquisition_requirement_t requirements[8];
        size_t read_count = 0;
        size_t refusal_count = 0;
        size_t acq_count = 0;
        const openxcvr_status status = openxcvr_field_read_remaining(
            OPENXCVR_SPEC_SFF8472, 12, 5, entries, 1,
            view.obs, view.count,
            reads, 8, &read_count, refusals, 8, &refusal_count,
            requirements, 8, &acq_count);
        CHECK(status == OPENXCVR_OK);
        CHECK(refusal_count == 1);
        CHECK(refusals[0].reason ==
              OPENXCVR_FIELD_ADMISSION_COMPLIANCE_REFUSED);
        CHECK(refusals[0].detail.compliance_code == codes[c]);
        CHECK(strcmp(refusals[0].field, entries[0].row->name) == 0);
        CHECK(refusals[0].has_lane == entries[0].has_lane);
        CHECK(refusals[0].lane == entries[0].lane);
        CHECK(read_count == 0);
        CHECK(acq_count == 0);
    }
}

/* (x) C-A6: an admitted CMIS laser-bias entry (whose scaling is
 * qualified by the TxBiasCurrentScalingFactor advertisement) plans the
 * Page 01h byte-160 advertisement beside its own lane spans: the eight
 * adjacent two-byte lane spans coalesce into one read and the
 * advertisement is its own upper-page (optional) read. */
static void bias_advertisement_planned(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_CMIS, 5, 1, "laser_bias_tx",
        OPENXCVR_LANE_ALL, NULL, entries, 8);
    CHECK(count == 8);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x51);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 1, entries, count,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    CHECK(read_count == 2);
    /* Page 1 sorts before page 17: the advertisement read comes first. */
    CHECK(read_is(&reads[0], 0x50, 0, 1, 160, 1));
    CHECK(strcmp(reads[0].purpose, "tx_bias_current_scaling_factor") == 0);
    CHECK(reads[0].optional == 1);
    CHECK(read_is(&reads[1], 0x50, 0, 17, 170, 16));
    CHECK(strcmp(reads[1].purpose, "laser_bias_tx") == 0);
    CHECK(reads[1].optional == 1);
}

/* (y) C-A6: an admitted SFF-8472 polynomial-scaled entry plans its five
 * declared coefficients from metadata (ruling 2): the five adjacent
 * four-byte coefficient ranges coalesce into one 16-byte read beside
 * the discriminator and the entry's own range. */
static void polynomial_coefficients_planned(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_SFF8472, 12, 5, "rx_power",
        OPENXCVR_LANE_UNSPECIFIED, "page_a2h_diagnostics_base", entries, 4);
    CHECK(count >= 1);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 94, 0x0a);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, entries, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    CHECK(read_count == 3);
    CHECK(read_is(&reads[0], 0x50, 0, 0, 92, 1));
    CHECK(strcmp(reads[0].purpose,
                 "page_a0h_base_memory.externally_calibrated") == 0);
    CHECK(reads[0].optional == 0);
    CHECK(read_is(&reads[1], 0x51, 0, 0, 56, 20));
    CHECK(strcmp(reads[1].purpose, "rx_power") == 0);
    CHECK(reads[1].optional == 0);
    CHECK(read_is(&reads[2], 0x51, 0, 0, 104, 2));
    CHECK(strcmp(reads[2].purpose, "rx_power") == 0);
    CHECK(reads[2].optional == 0);
}

/* (z) C-A6: an SFF-8636 entry through remaining — the third family —
 * with its compliance gate supplied (0Ah admits Rev 2.12): the
 * temperature entry is admitted, its own range is planned and nothing
 * else is. */
static void sff8636_remaining(void) {
    openxcvr_resolved_field_t entries[4];
    const size_t count = resolve_field(
        OPENXCVR_SPEC_SFF8636, 2, 12, "temperature",
        OPENXCVR_LANE_UNSPECIFIED, NULL, entries, 4);
    CHECK(count >= 1);
    view_t view;
    view.count = 0;
    view_byte(&view, 0x50, 0, 1, 0x0a);
    openxcvr_planned_read_t reads[8];
    openxcvr_field_admission_refusal_t refusals[8];
    openxcvr_acquisition_requirement_t requirements[8];
    size_t read_count = 0;
    size_t refusal_count = 0;
    size_t acq_count = 0;
    const openxcvr_status status = openxcvr_field_read_remaining(
        OPENXCVR_SPEC_SFF8636, 2, 12, entries, 1,
        view.obs, view.count,
        reads, 8, &read_count, refusals, 8, &refusal_count,
        requirements, 8, &acq_count);
    CHECK(status == OPENXCVR_OK);
    CHECK(refusal_count == 0);
    CHECK(acq_count == 0);
    CHECK(read_count == 1);
    CHECK(read_is(&reads[0], 0x50, 0, 0, 22, 2));
    CHECK(strcmp(reads[0].purpose, "temperature") == 0);
    CHECK(reads[0].optional == 0);
}

int main(void) {
    snr_view_prelude_bytes_only();
    ber_view_prelude_bytes_only();
    ber_alone_under_snr_view();
    non_selecting_selectors();
    revision_refusals();
    sff8472_conditional();
    unconditional_monitor();
    effectful_prerequisite_unfiltered();
    undecodable_prerequisite();
    unsupported_presence();
    refusal_order();
    transactional();
    validation();
    malformed_metadata();
    malformed_views();
    linear_constants_from_metadata();
    fragmented_prerequisite();
    seven_bit_address_bound();
    instanced_effect_footprint();
    enum_ref_shifted_prerequisite();
    u32be_prerequisite_fragmented();
    u64le_prerequisite();
    compliance_refused();
    bias_advertisement_planned();
    polynomial_coefficients_planned();
    sff8636_remaining();
    printf("OK\n");
    return 0;
}
