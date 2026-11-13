/* Field-catalog resolution over the generated per-family tables.
 * Mirrors openxcvr_core::field_catalog::resolve and openxcvr::catalog. */
#include "openxcvr/catalog.h"

#include <stdlib.h>
#include <string.h>

extern const openxcvr_family_catalog_t openxcvr_cmis_catalog;
extern const openxcvr_family_catalog_t openxcvr_sff8636_catalog;
extern const openxcvr_family_catalog_t openxcvr_sff8472_catalog;

const openxcvr_family_catalog_t *openxcvr_catalog_table(openxcvr_spec_kind_t family) {
    switch (family) {
    case OPENXCVR_SPEC_SFF8472:
        return &openxcvr_sff8472_catalog;
    case OPENXCVR_SPEC_SFF8636:
        return &openxcvr_sff8636_catalog;
    case OPENXCVR_SPEC_CMIS:
        return &openxcvr_cmis_catalog;
    default:
        return NULL;
    }
}

/* Same glob dialect as the Rust surface: '*' and '?' only. */
static int glob_match(const char *pattern, const char *name) {
    if (*pattern == '\0') {
        return *name == '\0';
    }
    if (*pattern == '*') {
        return glob_match(pattern + 1, name)
            || (*name != '\0' && glob_match(pattern, name + 1));
    }
    if (*pattern == '?') {
        return *name != '\0' && glob_match(pattern + 1, name + 1);
    }
    return *pattern == *name && glob_match(pattern + 1, name + 1);
}

static int row_applicable(const openxcvr_catalog_row_t *row, uint8_t major, uint8_t minor) {
    if (row->has_since && (major < row->since_major || (major == row->since_major && minor < row->since_minor))) {
        return 0;
    }
    if (row->has_until && (major > row->until_major || (major == row->until_major && minor >= row->until_minor))) {
        return 0;
    }
    return 1;
}

static int page_matches(const openxcvr_catalog_row_t *row, const char *page) {
    if (page == NULL) {
        return 1;
    }
    return strcmp(page, row->page_id) == 0 || strcmp(page, row->page_label) == 0;
}

/* Candidate ordering matches the Rust surface: page id, then offset. */
static int compare_candidates(const void *left, const void *right) {
    const openxcvr_catalog_row_t *a = *(const openxcvr_catalog_row_t *const *)left;
    const openxcvr_catalog_row_t *b = *(const openxcvr_catalog_row_t *const *)right;
    int by_page = strcmp(a->page_id, b->page_id);
    if (by_page != 0) {
        return by_page;
    }
    if (a->offset != b->offset) {
        return a->offset < b->offset ? -1 : 1;
    }
    return strcmp(a->name, b->name);
}

openxcvr_status openxcvr_catalog_resolve(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_field_query_t *query,
    openxcvr_resolved_field_t *out, size_t capacity, size_t *count
) {
    if (query == NULL || count == NULL || query->pattern == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    if ((out == NULL && capacity != 0)) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    const openxcvr_family_catalog_t *table = openxcvr_catalog_table(family);
    if (table == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    int supported = 0;
    for (size_t i = 0; i < table->supported_rev_count; ++i) {
        if (table->supported_revs[i].major == revision_major
            && table->supported_revs[i].minor == revision_minor) {
            supported = 1;
            break;
        }
    }
    if (!supported) {
        return OPENXCVR_ERR_UNSUPPORTED_REVISION;
    }

    const int is_glob = strchr(query->pattern, '*') != NULL || strchr(query->pattern, '?') != NULL;
    const openxcvr_catalog_row_t **matched =
        malloc(table->row_count * sizeof(*matched));
    if (matched == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    size_t matched_count = 0;
    for (size_t i = 0; i < table->row_count; ++i) {
        const openxcvr_catalog_row_t *row = &table->rows[i];
        if (!row_applicable(row, revision_major, revision_minor)) {
            continue;
        }
        if (!page_matches(row, query->page)) {
            continue;
        }
        const int name_matches = is_glob
            ? glob_match(query->pattern, row->name)
            : strcmp(query->pattern, row->name) == 0;
        if (name_matches) {
            matched[matched_count++] = row;
        }
    }
    if (matched_count == 0) {
        free(matched);
        return OPENXCVR_ERR_NO_MATCH;
    }
    if (!is_glob && matched_count > 1) {
        free(matched);
        return OPENXCVR_ERR_AMBIGUOUS;
    }
    qsort(matched, matched_count, sizeof(*matched), compare_candidates);

    /* Count the entries every matched row expands to. */
    size_t total = 0;
    for (size_t i = 0; i < matched_count; ++i) {
        const openxcvr_catalog_row_t *row = matched[i];
        if (!row->has_instances) {
            if (query->lane != OPENXCVR_LANE_UNSPECIFIED) {
                free(matched);
                return OPENXCVR_ERR_OUT_OF_RANGE;
            }
            total += 1;
        } else if (query->lane == OPENXCVR_LANE_UP_TO) {
            const uint16_t last = row->instances.last < query->lane_arg
                ? row->instances.last
                : query->lane_arg;
            if (row->instances.first <= last) {
                total += (size_t)(last - row->instances.first + 1);
            }
        } else if (query->lane == OPENXCVR_LANE_UNSPECIFIED) {
            total += 1;
        } else if (query->lane == OPENXCVR_LANE_ALL) {
            total += (size_t)(row->instances.last - row->instances.first + 1);
        } else {
            /* OPENXCVR_LANE_ONE: bounds-checked, exactly one entry. */
            if (query->lane_arg < row->instances.first || query->lane_arg > row->instances.last) {
                free(matched);
                return OPENXCVR_ERR_OUT_OF_RANGE;
            }
            total += 1;
        }
    }
    *count = total;
    if (out == NULL) {
        free(matched);
        return OPENXCVR_OK;
    }
    if (capacity < total) {
        free(matched);
        return OPENXCVR_ERR_SHORT_BUFFER;
    }

    size_t written = 0;
    for (size_t i = 0; i < matched_count; ++i) {
        const openxcvr_catalog_row_t *row = matched[i];
        uint16_t first = row->has_instances ? row->instances.first : 0;
        uint16_t last = row->has_instances ? row->instances.last : 0;
        if (!row->has_instances) {
            out[written++] = (openxcvr_resolved_field_t){
                row,
                {row->address, row->banked ? query->bank : 0, row->page, (uint8_t)row->offset, row->len},
                0,
                0,
                row->bit_offset,
                family,
                revision_major,
                revision_minor,
            };
            continue;
        }
        if (query->lane == OPENXCVR_LANE_UP_TO) {
            last = row->instances.last < query->lane_arg ? row->instances.last : query->lane_arg;
            if (row->instances.first > last) {
                continue;
            }
        } else if (query->lane == OPENXCVR_LANE_ONE) {
            first = query->lane_arg;
            last = query->lane_arg;
        }
        if (query->lane == OPENXCVR_LANE_UNSPECIFIED) {
            /* The field as a whole: one entry at the base instance. */
            out[written++] = (openxcvr_resolved_field_t){
                row,
                {row->address, row->banked ? query->bank : 0, row->page, (uint8_t)row->instances.offsets[0], row->len},
                0,
                0,
                row->instances.bit_offsets[0],
                family,
                revision_major,
                revision_minor,
            };
            continue;
        }
        for (uint32_t lane = first; lane <= last; ++lane) {
            size_t slot = lane - row->instances.first;
            if (slot >= row->instances.count) {
                free(matched);
                return OPENXCVR_ERR_INVALID_ARGUMENT;
            }
            out[written++] = (openxcvr_resolved_field_t){
                row,
                {row->address, row->banked ? query->bank : 0, row->page, (uint8_t)row->instances.offsets[slot], row->len},
                (uint16_t)lane,
                1,
                row->instances.bit_offsets[slot],
                family,
                revision_major,
                revision_minor,
            };
        }
    }
    free(matched);
    return OPENXCVR_OK;
}
