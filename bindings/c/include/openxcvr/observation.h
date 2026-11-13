#ifndef OPENXCVR_OBSERVATION_H
#define OPENXCVR_OBSERVATION_H

#include <stddef.h>
#include <stdint.h>

#include "openxcvr/catalog.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Basic observations from supplied bytes: module temperature, supply
 * voltage and per-lane TX bias, TX power and RX power. Typed values with
 * units and explicit validity states; no I/O, no allocation. Mirrors
 * openxcvr::observation. */

#define OPENXCVR_OBSERVATION_MAX_LANES 8

typedef enum openxcvr_observation_state {
    OPENXCVR_OBS_VALID = 0,
    OPENXCVR_OBS_NOT_SUPPLIED,
    OPENXCVR_OBS_UNCALIBRATED,
    OPENXCVR_OBS_UNSUPPORTED_REVISION,
    OPENXCVR_OBS_CLEAR_ON_READ_REFUSED,
    OPENXCVR_OBS_INVALID,
    OPENXCVR_OBS_UNSUPPORTED_FIELD
} openxcvr_observation_state_t;

typedef struct openxcvr_field_value {
    /* 1 = scaled { value, unit }; 0 = raw code in `raw`. For an
     * F16-scaled value (openxcvr_cmis_f16_to_double) `raw` holds the
     * 16-bit F16 code; unchanged for the other scaled kinds. */
    int is_scaled;
    double value;
    openxcvr_unit_t unit;
    uint64_t raw;
} openxcvr_field_value_t;

typedef struct openxcvr_observation {
    openxcvr_observation_state_t state;
    int has_value;
    openxcvr_field_value_t value;
    /* Static caveat string, or NULL. */
    const char *note;
    const char *field;
    int has_lane;
    uint16_t lane;
} openxcvr_observation_t;

typedef struct openxcvr_lane_observations {
    uint16_t lane;
    openxcvr_observation_t tx_bias;
    openxcvr_observation_t tx_power;
    openxcvr_observation_t rx_power;
} openxcvr_lane_observations_t;

typedef struct openxcvr_basic_observations {
    /* The decoder's schema revision (byte-proven for CMIS). */
    uint8_t revision_major;
    uint8_t revision_minor;
    /* The revision compliance the snapshot reports — SFF-8636 page 00h
     * byte 1, SFF-8472 A0h byte 94 — with the schema's meaning for the
     * code. Absent (0) for CMIS (its revision byte is proven into
     * revision_major/minor above) and when the compliance byte is not
     * supplied. */
    int has_reported_compliance;
    uint8_t reported_compliance_code;
    const char *reported_compliance_meaning;
    openxcvr_observation_t temperature;
    openxcvr_observation_t supply_voltage;
    openxcvr_lane_observations_t lanes[OPENXCVR_OBSERVATION_MAX_LANES];
    uint16_t lane_count;
} openxcvr_basic_observations_t;

typedef struct openxcvr_observe_options {
    /* Authorize decoding clear-on-read fields; the default (0) refuses. */
    int authorize_clear_on_read;
} openxcvr_observe_options_t;

/* ─── Optical power display conversion (native C, no Cargo) ──────── */

#define OPENXCVR_OPTICAL_DBM_VERSION 1

typedef enum openxcvr_optical_dbm_kind {
    OPENXCVR_OPTICAL_DBM_VALUE = 0,
    OPENXCVR_OPTICAL_DBM_UNAVAILABLE = 1,
    OPENXCVR_OPTICAL_DBM_NOT_CONVERTIBLE = 2,
    OPENXCVR_OPTICAL_DBM_NO_POWER = 3
} openxcvr_optical_dbm_kind_t;

/* New, independently size/version-tagged result; no existing layout changes. */
typedef struct openxcvr_optical_dbm {
    size_t size;
    uint32_t version;
    /* Borrowed unchanged input, including field/lane/W value/note. Must
     * remain alive and unchanged while the consumer uses this pointer. */
    const openxcvr_observation_t *source;
    openxcvr_observation_state_t state;
    openxcvr_optical_dbm_kind_t kind;
    /* Finite dBm only for VALUE; 0.0 otherwise is unused storage, NOT a
     * measurement or floor. Consumers MUST inspect kind before use. */
    double dbm;
} openxcvr_optical_dbm_t;

/* Pure, allocation-free conversion of a VALID, scaled Watt observation:
 * 10*log10(W*1000), evaluated as 10*(log10(W)+3) to avoid overflow.
 * Every non-VALID state passes through as UNAVAILABLE before inspecting
 * the value. VALID raw/absent/non-Watt values are NOT_CONVERTIBLE.
 * Zero W (either sign) is NO_POWER, not -inf or a floor. Negative or
 * non-finite W is INVALID/UNAVAILABLE. Positive finite W is converted
 * without clamping, including subnormals and values below 1 uW.
 * No field-name inference; caller selects an optical observation whose
 * already-calibrated Watt value is consumed as-is. Only the derived
 * dbm is guaranteed finite; the source is not sanitized or changed.
 *
 * Caller initializes out->size >= sizeof(openxcvr_optical_dbm_t) and
 * out->version = OPENXCVR_OPTICAL_DBM_VERSION (1). out must point to at
 * least readable size/version, and size must cover writable v1 storage.
 * NULL source/out or bad size/version returns INVALID_ARGUMENT, leaving
 * out unchanged. On success size/version are retained, the v1 fields
 * are published together, and any trailing caller storage is untouched.
 * source and out storage must be disjoint. No allocation/free ownership
 * transfers. Semantic no-value outcomes return OPENXCVR_OK.
 */
openxcvr_status openxcvr_optical_power_dbm(
    const openxcvr_observation_t *source, openxcvr_optical_dbm_t *out
);

/* ─── CMIS F16 numeric conversion ─────────────────────────────────── */

/*
 * Convert a CMIS / SFF-8636 F16 diagnostic-value code to binary64. F16
 * is a decimal format — value = mantissa * 10^(scaled_exponent - 24)
 * with mantissa = bits 10..0 (0..=2047) and scaled_exponent =
 * bits 15..11 (0..=31) — NOT IEEE 754 binary16: it is unsigned and has
 * no NaN, infinity or sentinel encodings. The 32 codes with mantissa 0
 * (0x0000 and 0xF800 among them) all convert to 0.0; the codes
 * themselves stay distinct wherever the caller keeps them.
 *
 * The result is the binary64 nearest mantissa * 10^(scaled_exponent -
 * 24), round-to-nearest with ties-to-even. Precondition: ordinary IEEE
 * binary64 arithmetic — no excess precision (FLT_EVAL_METHOD 0, not
 * x87 80-bit evaluation) and no fast-math transformations; the
 * conversion uses fma() and exact power-of-ten tables. No allocation.
 */
double openxcvr_cmis_f16_to_double(uint16_t code);

/* ─── Selector-acquisition evidence (Stage 1 B2) ─────────────────── */

/*
 * An acquisition owner's assertion that one selector-controlled data
 * window was read under a settled, selector-stable acquisition: for that
 * generation/epoch/bank, the selector value was in force, writes were
 * serialized with the read, and the data_range read began after the
 * revision's content-switch bound (CMIS 4.0: within the advertised tNACK
 * after the selector write; CMIS 5.0+: after a tDDCS guard period, at
 * most 10 ms - CMIS 5.3 Table 10-5).
 *
 * Trusted caller evidence, not library-measured timing: the C range
 * observations carry no generation/epoch stamps to cross-check, so the
 * generation and epoch provenance of this assertion is CALLER-ASSERTED.
 * The library performs no write, wait or timing check; it only matches
 * the assertion against the supplied bytes and the row's expectation.
 */
typedef struct openxcvr_selector_acquisition {
    uint64_t generation;
    uint64_t epoch;
    /* Page-window address of the selector. */
    uint8_t address;
    /* Bank the acquisition read (for banked pages). */
    uint8_t bank;
    /* Page selector byte of the window. */
    uint8_t page;
    /* Window offset of the selector byte (128 for Page 14h). */
    uint16_t selector_offset;
    /* The selector value the acquisition held in force. */
    uint8_t selector_value;
    /* The data window the acquisition read (must cover the observed
     * row's own bytes for the assertion to apply). */
    openxcvr_range data_range;
} openxcvr_selector_acquisition_t;

/*
 * Acquisition evidence supplied by the caller: zero or more selector
 * acquisitions. Decoding a content-multiplexed row (for example a Page
 * 14h diagnostics field) with no matching evidence yields
 * NOT_SUPPLIED. The context-free entry points behave exactly as with an
 * empty context.
 */
typedef struct openxcvr_acquisition_context {
    size_t count;
    const openxcvr_selector_acquisition_t *acquisitions;
} openxcvr_acquisition_context_t;

/*
 * Decode the basic observation set from supplied ranges. For CMIS the
 * requested revision is proven against the snapshot's revision byte
 * (lower memory byte 1): a disagreement is INVALID_ARGUMENT, an
 * undecodable byte is UNSUPPORTED_REVISION, a missing byte is
 * INSUFFICIENT_INPUT. For SFF-8636 and SFF-8472 the snapshot's
 * revision-compliance byte (page 00h byte 1 / A0h byte 94) must establish
 * applicability to the schema revision — SFF-8636 0Ah (Rev 2.12) and
 * SFF-8472 0Ah (Rev 12.5); every other code (00h unspecified, earlier
 * revisions, reserved) yields UNSUPPORTED_REVISION observations with no
 * value, and the result still reports the code and its meaning. Invalid
 * ranges (zero length, offset+len > 256, length mismatch, overlapping
 * present observations in one window) are INVALID_ARGUMENT. options may
 * be NULL for the defaults. Per-field outcomes are states, not errors:
 * absent pages are NOT_SUPPLIED, missing external-calibration constants
 * are UNCALIBRATED; no decode error is converted to zero.
 */
openxcvr_status openxcvr_observe_basic(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    openxcvr_basic_observations_t *out
);

/*
 * Decode one resolved field (from openxcvr_catalog_resolve) from the
 * supplied ranges. Clear-on-read fields are CLEAR_ON_READ_REFUSED unless
 * options authorizes them. The same family rules as
 * openxcvr_observe_basic apply, so the generic field path cannot bypass
 * them: SFF-8636 / SFF-8472 decodes are admitted only where the
 * snapshot's revision-compliance byte establishes applicability to the
 * schema revision (otherwise UNSUPPORTED_REVISION with no value), and a
 * CMIS laser-bias field applies the page-01h
 * TxBiasCurrentScalingFactor advertisement (an absent advertisement
 * yields no scaled value - NOT_SUPPLIED; a reserved code is INVALID).
 * Stage 1 admission applies before any value: the snapshot's reported
 * CMIS revision (lower memory byte 1; absent - NOT_SUPPLIED, different or
 * unknown - UNSUPPORTED_REVISION), presence prerequisites (NOT_SUPPLIED
 * when the prerequisite bytes are absent, INVALID on mismatch,
 * UNSUPPORTED_FIELD for unsupported conditions), and - for
 * selector-multiplexed rows such as Page 14h diagnostics - acquisition
 * evidence. This entry point behaves exactly as with an empty context.
 */
openxcvr_status openxcvr_observe_field(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    openxcvr_observation_t *out
);

/*
 * openxcvr_observe_field with caller-supplied acquisition evidence (see
 * openxcvr_selector_acquisition_t for what the caller asserts and the
 * library checks). A NULL ctx is the empty context. A non-NULL ctx with
 * count > 0 and acquisitions == NULL is OPENXCVR_ERR_INVALID_ARGUMENT.
 */
openxcvr_status openxcvr_observe_field_ctx(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    const openxcvr_acquisition_context_t *ctx,
    openxcvr_observation_t *out
);

/* ─── Basic observation read plan ──────────────────────────────────── */

typedef struct openxcvr_planned_read {
    openxcvr_range range;
    /* 1 when a missing range is an expected NOT_SUPPLIED decode outcome
     * rather than an acquisition failure: exactly the upper-page ranges
     * (page != 0), which flat-memory modules do not implement (CMIS
     * memory_model, lower byte 2 bit 7, 1b = "Flat memory (Page 00h
     * supported only)"; SFF-8636 flat_memory, byte 2 bit 2). */
    int optional;
    /* The field these bytes serve: the catalog row's effective name for
     * monitor, gate and constant ranges, and the discriminator's schema
     * field path for discriminator ranges. */
    const char *purpose;
} openxcvr_planned_read_t;

/*
 * The fixed per-family prelude of the basic-observation read plan: the
 * exact ranges whose bytes decide everything else the plan needs — the
 * CMIS lower-memory revision byte, the SFF-8636 lower-page / SFF-8472
 * A0h revision-compliance byte, and the SFF-8472 external-calibration
 * discriminator byte. Every location is a catalog row's (the same rows
 * openxcvr_observe_basic uses); there is no second offset table. NULL
 * out with capacity zero is a size query; on insufficient capacity
 * *count is set but out is unchanged. Unsupported family is
 * INVALID_ARGUMENT; unsupported revision is UNSUPPORTED_REVISION.
 */
openxcvr_status openxcvr_basic_read_prelude(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    openxcvr_planned_read_t *out, size_t capacity, size_t *count
);

/*
 * The remaining ranges of the basic-observation read plan, given the
 * prelude bytes: the five monitors' exact instance spans, the CMIS
 * TxBiasCurrentScalingFactor advertisement when a monitor's scaling is
 * qualified by it, and the SFF-8472 external-calibration constants only
 * when the discriminator byte says external calibration. Refuses like
 * openxcvr_observe_basic's gates: INSUFFICIENT_INPUT when a prelude byte
 * is missing, UNSUPPORTED_REVISION when the reported revision or
 * compliance does not admit a decode at the requested revision,
 * INVALID_ARGUMENT for a CMIS revision-byte mismatch, an unsupported
 * family or an invalid view — and in every refusal no monitor byte is
 * planned. Same size-query and SHORT_BUFFER contract as
 * openxcvr_basic_read_prelude; other errors leave outputs unchanged.
 */
openxcvr_status openxcvr_basic_read_remaining(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_planned_read_t *out, size_t capacity, size_t *out_count
);

/* ─── Field read plan: two-round admission (Stage 1b) ───────────────── */

/* Arbitrary-field two-round planning. No I/O and no allocation.
 * Bounds mirror Rust FIELD_PLAN_DEFAULT_LIMITS. The range bound applies
 * AFTER coalescing; refusals and requirements are each bounded by entries.
 *
 * Caller preconditions (both rounds): the caller-owned output arrays and
 * count objects must be mutually non-overlapping and disjoint from the
 * inputs (entries, view, catalog metadata), and every count pointer must
 * identify a distinct object; the inputs must remain stable throughout
 * the call. Returned strings are borrowed — generated catalog strings
 * live for the catalog's lifetime, and strings in hand-built row metadata
 * must outlive the result's use. No allocation or free ownership is
 * transferred.
 *
 * Stack use: both openxcvr_field_read_prelude and
 * openxcvr_field_read_remaining work in bounded per-call automatic
 * storage rather than allocating. Measured frames are about 12.8 KB
 * (prelude) and 56.4 KB (remaining) with gcc 15.2 on x86_64, varying
 * by compiler, target and build flags; caller-owned output arrays are
 * additional. The functions are intended for ordinary user-space
 * threads and are not qualified for kernel, interrupt or small
 * fixed-stack execution.
 */
#define OPENXCVR_FIELD_PLAN_MAX_ENTRIES 256
#define OPENXCVR_FIELD_PLAN_MAX_RANGES 256

/* Semantic counterparts of Rust FieldPlanRefusalReason, not read-effect
 * bitmask values and not a Rust binary-layout contract. */
typedef enum openxcvr_field_plan_refusal_reason {
    OPENXCVR_FIELD_PLAN_CLEAR_ON_READ = 0,
    OPENXCVR_FIELD_PLAN_ACKNOWLEDGE_ON_READ = 1,
    OPENXCVR_FIELD_PLAN_WRITE_ONLY = 2
} openxcvr_field_plan_refusal_reason_t;

typedef struct openxcvr_field_plan_refusal {
    const char *field;
    int has_lane;
    uint16_t lane;
    openxcvr_field_plan_refusal_reason_t reason;
} openxcvr_field_plan_refusal_t;

typedef enum openxcvr_field_admission_reason {
    OPENXCVR_FIELD_ADMISSION_PRELUDE_MISSING = 0,
    OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_REVISION = 1,
    OPENXCVR_FIELD_ADMISSION_COMPLIANCE_REFUSED = 2,
    OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED = 3,
    OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED = 4,
    OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_PRESENCE = 5,
    OPENXCVR_FIELD_ADMISSION_EFFECT = 6,
    OPENXCVR_FIELD_ADMISSION_PREREQUISITE_UNDECODABLE = 7
} openxcvr_field_admission_reason_t;

typedef struct openxcvr_field_admission_refusal {
    const char *field;
    int has_lane;
    uint16_t lane;
    openxcvr_field_admission_reason_t reason;
    /* Only the member selected by reason is meaningful. Strings are
     * borrowed from catalog metadata; the planner allocates no strings. */
    union {
        openxcvr_range prelude_missing;
        struct {
            openxcvr_spec_kind_t family;
            uint8_t major;
            uint8_t minor;
        } unsupported_revision;
        uint8_t compliance_code;
        struct {
            const char *prerequisite;
            uint64_t expected;
            uint64_t actual;
        } prerequisite_not_satisfied;
        struct {
            const char *selector;
            uint64_t expected;
            uint64_t actual;
        } view_not_selected;
        const char *unsupported_presence_expr;
        openxcvr_field_plan_refusal_reason_t effect;
        struct {
            const char *prerequisite;
        } prerequisite_undecodable;
    } detail;
} openxcvr_field_admission_refusal_t;

/* A requirement for later decode evidence, NOT acquisition evidence,
 * acquisition authorization, or a selector-write/wait instruction. */
typedef struct openxcvr_acquisition_requirement {
    const char *field;
    int has_lane;
    uint16_t lane;
    openxcvr_range selector_range;
    uint64_t selector_value;
    openxcvr_range data_range;
} openxcvr_acquisition_requirement_t;

/* Round 1: only the family revision/compliance gate and Equals
 * prerequisite ranges, effect-checked and coalesced. No acquisition
 * requirements are produced in this round, so it has two outputs.
 *
 * IMPORTANT: prerequisite reads may access an upper page BEFORE revision
 * proof. The guarantee is that remaining contributes no dependent range
 * for a refused entry, not that prelude avoids every unsupported page.
 *
 * Each output has an independent size-query form: NULL with capacity 0.
 * All count pointers are required. The complete result is computed before
 * publication. On SHORT_BUFFER all counts are set and every output array
 * is unchanged. Other errors leave arrays AND counts unchanged. See the
 * accompanying contract for validation and mixed-query semantics.
 */
openxcvr_status openxcvr_field_read_prelude(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_resolved_field_t *entries, size_t entry_count,
    openxcvr_planned_read_t *reads, size_t read_capacity, size_t *read_count,
    openxcvr_field_plan_refusal_t *refusals,
    size_t refusal_capacity, size_t *refusal_count
);

/* Round 2: self-contained admission from supplied prelude bytes; callers
 * need not filter entries using a previous prelude result. Returns data
 * and remaining decode dependencies only for admitted entries, plus typed
 * refusals and required selector-acquisition evidence coordinates.
 * Gate and prerequisite bytes are not added again as admission reads.
 *
 * ranges must be non-NULL, including when range_count is zero, matching
 * the existing C remaining/observation APIs. An empty view therefore uses
 * a non-NULL pointer with range_count 0. No acquisition context is needed.
 *
 * Each output independently supports NULL with capacity 0 as a query.
 * All THREE count pointers are required. On SHORT_BUFFER all three counts
 * are set and all three arrays remain unchanged. Other errors leave all
 * arrays and counts unchanged. Supplied-byte admission refusals are result
 * records, not function-level INSUFFICIENT_INPUT or UNSUPPORTED_REVISION.
 *
 * R3-A2, stated exactly: remaining adds no dependent contribution for a
 * refused entry; shared physical bytes another admitted entry needs may
 * remain planned through that entry. Neither a flat superset nor
 * supplied-data admission proves acquisition ordering, and returned
 * requirements are not selector-write permission, settled-acquisition
 * evidence, or library-verified timing/serialization — the acquisition
 * owner must provide the later decode context.
 */
openxcvr_status openxcvr_field_read_remaining(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_resolved_field_t *entries, size_t entry_count,
    const openxcvr_range_observation *ranges, size_t range_count,
    openxcvr_planned_read_t *reads, size_t read_capacity, size_t *read_count,
    openxcvr_field_admission_refusal_t *refusals,
    size_t refusal_capacity, size_t *refusal_count,
    openxcvr_acquisition_requirement_t *acquisition_required,
    size_t acquisition_capacity, size_t *acquisition_count
);

#ifdef __cplusplus
}
#endif

#endif
