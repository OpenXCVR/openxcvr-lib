#ifndef OPENXCVR_APPLICATION_RECORDS_H
#define OPENXCVR_APPLICATION_RECORDS_H

#include <stddef.h>
#include <stdint.h>
#include "openxcvr/status.h"

#ifdef __cplusplus
extern "C" {
#endif

#define OPENXCVR_APPLICATION_RECORDS_VERSION 1u

typedef enum openxcvr_application_slot_state {
    OPENXCVR_APPLICATION_PRESENT = 0,
    OPENXCVR_APPLICATION_END_OF_LIST = 1,
    OPENXCVR_APPLICATION_ABSENT = 2,
    OPENXCVR_APPLICATION_UNAVAILABLE = 3,
    OPENXCVR_APPLICATION_SHORT_DATA = 4,
    OPENXCVR_APPLICATION_UNSUPPORTED_REVISION = 5,
    OPENXCVR_APPLICATION_REVISION_MISMATCH = 6
} openxcvr_application_slot_state;

typedef enum openxcvr_interface_meaning_state {
    OPENXCVR_INTERFACE_KNOWN = 0,
    OPENXCVR_INTERFACE_UNDEFINED = 1,
    OPENXCVR_INTERFACE_UNKNOWN_CODE = 2,
    OPENXCVR_INTERFACE_UNSUPPORTED_MEDIA_TYPE = 3,
    OPENXCVR_INTERFACE_UNAVAILABLE = 4
} openxcvr_interface_meaning_state;

typedef enum openxcvr_application_lane_count_state {
    OPENXCVR_APPLICATION_LANES_IMPLICIT = 0,
    OPENXCVR_APPLICATION_LANES_EXPLICIT = 1,
    OPENXCVR_APPLICATION_LANES_RESERVED = 2,
    OPENXCVR_APPLICATION_LANES_UNAVAILABLE = 3
} openxcvr_application_lane_count_state;

typedef struct openxcvr_application_request {
    size_t size;
    uint32_t version;
    uint8_t revision_major;
    uint8_t revision_minor;
    /* Lower memory and Page 01h UPPER bytes (physical offsets 128..255).
     * NULL/0 means unavailable. A non-NULL short buffer is SHORT_DATA.
     * Each length is bounded to 128; no padding, transport or selector I/O. */
    const uint8_t *lower;
    size_t lower_len;
    const uint8_t *page01;
    size_t page01_len;
} openxcvr_application_request;

typedef struct openxcvr_application_record {
    uint8_t slot; /* AppSel 1..15, never a physical lane number */
    uint8_t has_application;
    uint8_t host_interface_code;
    uint8_t media_interface_code;
    uint8_t host_lane_count; /* raw nibble: zero is implicit, 9..15 reserved */
    uint8_t media_lane_count;
    uint8_t host_lane_assignment_options; /* permitted first-host-lane bitmap */
    uint8_t unsupported_media_type; /* raw byte 85 when media_meaning_state is 3 */
    uint32_t state;
    uint32_t host_meaning_state;
    uint32_t media_meaning_state;
    uint32_t host_lane_count_state;
    uint32_t media_lane_count_state;
    /* Generated Rust SFF-8024 enum spellings, owned by the result; NULL if
     * unknown/unsupported/unavailable. Undefined has spelling "Undefined". */
    const char *host_meaning;
    const char *media_meaning;
} openxcvr_application_record;

typedef struct openxcvr_application_result {
    size_t size;
    uint32_t version;
    uint8_t revision_major;
    uint8_t revision_minor;
    size_t count; /* exactly 15, including unavailable and absent slots */
    const openxcvr_application_record *records;
} openxcvr_application_result;

/* Request must point to at least size/version, and size must cover the full
 * v1 struct. Version must be 1. All supplied pointers must remain readable for
 * their counted lengths until return. Length >128, NULL/nonzero pairs, bad
 * size/version and NULL out are INVALID_ARGUMENT. On any error *out is NULL.
 * Success publishes a fully owned immutable result; inputs can immediately be
 * released. Unsupported revisions/mismatch are per-slot states, not empty
 * success. Missing Page 01h remains unavailable even after a lower sentinel.
 * CMIS 4.0 and 5.0..5.3 only; no fallback to newest revision.
 * Calls the shared Rust implementation, not a C copy of protocol semantics. */
openxcvr_status openxcvr_application_records(
    const openxcvr_application_request *request,
    const openxcvr_application_result **out);

/* NULL is a no-op. Otherwise pass the original result pointer exactly once;
 * every array/string is invalid after this call. Never free members yourself. */
void openxcvr_application_records_free(const openxcvr_application_result *result);

/* Additive sparse API (optional rich component only), version 1. These are
 * decoder dependencies, NOT read-effect/selector/acquisition permission. */
typedef struct openxcvr_application_footprint_request {
    size_t size;
    uint32_t version;
    uint8_t revision_major;
    uint8_t revision_minor;
} openxcvr_application_footprint_request;

typedef struct openxcvr_application_range {
    uint8_t address; /* seven-bit I2C address */
    uint8_t bank;
    uint8_t page;
    uint8_t offset; /* physical offset, including 128..255 for upper memory */
    uint16_t len;
} openxcvr_application_range;

typedef struct openxcvr_application_footprint_result {
    size_t size;
    uint32_t version;
    uint8_t revision_major;
    uint8_t revision_minor;
    size_t count;
    const openxcvr_application_range *ranges;
} openxcvr_application_footprint_result;

/* Sorted, disjoint, coalesced generated field spans. Unsupported revision
 * returns UNSUPPORTED_REVISION; missing schema metadata returns NO_MATCH.
 * On any error *out is NULL. Result owns its array until the matching free. */
openxcvr_status openxcvr_application_records_footprint(
    const openxcvr_application_footprint_request *request,
    const openxcvr_application_footprint_result **out);
void openxcvr_application_records_footprint_free(
    const openxcvr_application_footprint_result *result);

typedef struct openxcvr_application_supplied_range {
    openxcvr_application_range range;
    const uint8_t *bytes;
    size_t len; /* must equal range.len, 1..128, non-NULL bytes */
} openxcvr_application_supplied_range;

typedef struct openxcvr_application_sparse_request {
    size_t size;
    uint32_t version;
    uint8_t revision_major;
    uint8_t revision_minor;
    const openxcvr_application_supplied_range *ranges;
    size_t count; /* 0..256; NULL allowed only with zero count */
} openxcvr_application_sparse_request;

/* Each new request uses the same size/version/readability contract as the
 * original request. Full v1 size and version 1 required. NULL out is invalid.
 * Only address 0x50, bank 0, page 0 lower (end<=128) or page 1 upper
 * (offset>=128, end<=256). No zero lengths, mismatched lengths or overlaps,
 * even if values agree. Fragments may be unordered/adjacent. Invalid input
 * returns INVALID_ARGUMENT and publishes no result. No input is retained.
 * Missing lower fragments -> UNAVAILABLE; incomplete common lower dependency
 * (revision/media type) -> SHORT_DATA in all slots. Missing slot fields ->
 * SHORT_DATA for that slot. No Page01 fragments -> UNAVAILABLE for 9..15 even
 * after a lower FFh. Only a fully supplied sentinel slot terminates the list;
 * missing slots never become ABSENT. Mismatch/unsupported are per-slot states.
 * Free this result with the EXISTING openxcvr_application_records_free. */
openxcvr_status openxcvr_application_records_from_ranges(
    const openxcvr_application_sparse_request *request,
    const openxcvr_application_result **out);

#ifdef __cplusplus
}
#endif
#endif
