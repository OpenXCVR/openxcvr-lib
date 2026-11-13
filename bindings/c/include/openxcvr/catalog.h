#ifndef OPENXCVR_CATALOG_H
#define OPENXCVR_CATALOG_H

#include <stddef.h>
#include <stdint.h>

#include "openxcvr/openxcvr.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Field-catalog resolution over the generated per-family tables
 * (openxcvr_generated.h's catalog/ headers define the row type).
 * Mirrors openxcvr::catalog. Revision-aware: rows carry since/until spans,
 * so schema renames and aliases resolve as plain name lookups.
 */

typedef enum openxcvr_lane_selection {
    OPENXCVR_LANE_UNSPECIFIED = 0,
    OPENXCVR_LANE_ALL = 1,
    OPENXCVR_LANE_ONE = 2,
    OPENXCVR_LANE_UP_TO = 3
} openxcvr_lane_selection_t;

typedef struct openxcvr_field_query {
    /* Exact name, schema alias, or glob with '*' and '?'. */
    const char *pattern;
    openxcvr_lane_selection_t lane;
    /* Instance index for ONE; instance limit for UP_TO. */
    uint16_t lane_arg;
    /* Optional page filter: page schema id or page label; may be NULL. */
    const char *page;
    uint8_t bank;
} openxcvr_field_query_t;

typedef struct openxcvr_resolved_field {
    const openxcvr_catalog_row_t *row;
    openxcvr_range range;
    /* Instance index when a lane/instance was selected; has_lane is 0
     * otherwise. */
    uint16_t lane;
    int has_lane;
    uint8_t bit_offset;
    /* The family and revision the entry was resolved at (mirrors the
     * Rust FieldEntry's rev); family rules such as the SFF
     * revision-compliance gate read them. */
    openxcvr_spec_kind_t family;
    uint8_t revision_major;
    uint8_t revision_minor;
} openxcvr_resolved_field_t;

/*
 * Resolve fields by exact name, schema alias or glob at one revision
 * (major.minor; the family selects the table). Exact names matching several
 * rows return OPENXCVR_ERR_AMBIGUOUS (qualify with the query's page
 * filter); zero matches return OPENXCVR_ERR_NO_MATCH; a lane index outside
 * the field's dimension returns OPENXCVR_ERR_OUT_OF_RANGE; an unsupported
 * family/revision returns OPENXCVR_ERR_UNSUPPORTED_REVISION. NULL query or
 * count is INVALID_ARGUMENT. NULL out with capacity zero is a size query;
 * on insufficient capacity *count is set but out is unchanged.
 */
openxcvr_status openxcvr_catalog_resolve(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_field_query_t *query,
    openxcvr_resolved_field_t *out, size_t capacity, size_t *count
);

/* The generated catalog table for a family, or NULL for unknown families. */
const openxcvr_family_catalog_t *openxcvr_catalog_table(openxcvr_spec_kind_t family);

#ifdef __cplusplus
}
#endif

#endif
