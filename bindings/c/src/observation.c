/* Basic observation decoding from supplied bytes. Mirrors
 * openxcvr_core::observation::observe_field and openxcvr::observation. */
#include "openxcvr/observation.h"

#include <math.h>
#include <string.h>

/* openxcvr_cmis_f16_to_double promises the binary64 nearest
 * m * 10^(s-24) under round-to-nearest, ties-to-even. That promise holds
 * only under ordinary binary64 evaluation: fast-math transformations and
 * excess precision (FLT_EVAL_METHOD != 0, e.g. x87 80-bit evaluation)
 * can change the rounded result, so the library refuses to build in
 * those modes rather than silently break the conversion contract. */
#if defined(__FAST_MATH__)
#error "openxcvr observation requires ordinary binary64 arithmetic: __FAST_MATH__ (fast-math) can change the F16 conversion's correctly rounded binary64 result (openxcvr_cmis_f16_to_double)"
#endif
#include <float.h>
#if defined(FLT_EVAL_METHOD) && FLT_EVAL_METHOD != 0
#error "openxcvr observation requires ordinary binary64 arithmetic: FLT_EVAL_METHOD != 0 (excess precision) can change the F16 conversion's correctly rounded binary64 result (openxcvr_cmis_f16_to_double)"
#endif

extern const openxcvr_family_catalog_t openxcvr_cmis_catalog;
extern const openxcvr_family_catalog_t openxcvr_sff8636_catalog;
extern const openxcvr_family_catalog_t openxcvr_sff8472_catalog;

/* ─── CMIS F16 numeric conversion ────────────────────────────────────── */

/* Exact decimal powers of ten as binary64: 10^k for k in 0..=22 is
 * exactly representable, so m * P10[e] and m / P10[-e] are a single
 * correctly rounded operation each. */
static const double F16_P10[23] = {
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11,
    1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
};

double openxcvr_cmis_f16_to_double(uint16_t code) {
    double m = (double)(code & 0x7ffu);
    int e = (int)(code >> 11) - 24;
    if (e >= 0) {
        return m * F16_P10[e];
    }
    if (e >= -22) {
        return m / F16_P10[-e];
    }
    /* e in -23..=-24: 10^e is not exactly representable. hi is the
     * rounded value and lo the exact error (2^23 resp. 2^24); one fma
     * and a correction term yield the correctly rounded quotient. */
    double hi = (e == -23) ? 1e23 : 1e24;
    double lo = (e == -23) ? 8388608.0 : 16777216.0;
    double q = m / hi;
    double r = fma(-q, hi, m) - q * lo;
    return q + r / hi;
}

/* ─── Revision compliance reported by SFF snapshots ──────────────────────
 *
 * The SFF families carry no plain revision number: the module reports a
 * compliance code meaning "includes functionality described in revision X
 * of the standard". The tables mirror the schema enums
 * Sff8636RevisionCompliance and Sff8472ComplianceCode (meanings verbatim),
 * which cite SFF-8636 Rev 2.12 § 6.2.2 Table 6-4 (PDF p. 33) and SFF-8472
 * Rev 12.5a § 8.11 Table 8-8 (PDF p. 35); they must move with those enums.
 * `reports` is the single family revision the code names; codes naming
 * several revisions, SFF-8436 codes, unspecified and reserved codes name
 * none. A decode is admitted only where the reported code names exactly
 * the schema revision the catalog decodes (SFF-8636 0Ah = Rev 2.12;
 * SFF-8472 0Ah = Rev 12.5). */

typedef struct {
    uint8_t code;
    const char *meaning;
    int has_reports;
    uint8_t reports_major;
    uint8_t reports_minor;
} compliance_code_t;

static const compliance_code_t SFF8636_COMPLIANCE[] = {
    {0x00, "Revision not specified; not used for SFF-8636 rev 2.5 or higher", 0, 0, 0},
    {0x01, "SFF-8436 Rev 4.8 or earlier", 0, 0, 0},
    {0x02, "SFF-8436 Rev 4.8 or earlier except this byte and bytes 186-189 follow SFF-8636", 0, 0, 0},
    {0x03, "SFF-8636 Rev 1.3 or earlier", 1, 1, 3},
    {0x04, "SFF-8636 Rev 1.4", 1, 1, 4},
    {0x05, "SFF-8636 Rev 1.5", 1, 1, 5},
    {0x06, "SFF-8636 Rev 2.0", 1, 2, 0},
    /* 07h and 08h each name several revisions (2.5-2.7, 2.8-2.10): none of
     * them is the single revision the schema decodes. */
    {0x07, "SFF-8636 Rev 2.5, 2.6, and 2.7", 0, 0, 0},
    {0x08, "SFF-8636 Rev 2.8, 2.9, and 2.10", 0, 0, 0},
    /* 09h names Rev 2.11: a named code in the schema enum, but not a
     * revision this schema decodes. */
    {0x09, "SFF-8636 Rev 2.11", 1, 2, 11},
    /* 0Ah names Rev 2.12, the revision this schema decodes. */
    {0x0a, "SFF-8636 Rev 2.12", 1, 2, 12},
};

static const compliance_code_t SFF8472_COMPLIANCE[] = {
    {0x00, "SFF-8472 compliance not specified.", 0, 0, 0},
    {0x01, "Complies with SFF-8472 Rev 9.3.", 1, 9, 3},
    {0x02, "Complies with SFF-8472 Rev 9.5.", 1, 9, 5},
    {0x03, "Complies with SFF-8472 Rev 10.2.", 1, 10, 2},
    {0x04, "Complies with SFF-8472 Rev 10.4.", 1, 10, 4},
    {0x05, "Complies with SFF-8472 Rev 11.0.", 1, 11, 0},
    {0x06, "Complies with SFF-8472 Rev 11.3.", 1, 11, 3},
    {0x07, "Complies with SFF-8472 Rev 11.4.", 1, 11, 4},
    {0x08, "Complies with SFF-8472 Rev 12.3.", 1, 12, 3},
    {0x09, "Complies with SFF-8472 Rev 12.4.", 1, 12, 4},
    {0x0a, "Complies with SFF-8472 Rev 12.5.", 1, 12, 5},
};

/* Codes above the last named entry are the enums' reserved range. */
static const char RESERVED_MEANING[] = "Reserved";

static const compliance_code_t *compliance_table(
    openxcvr_spec_kind_t family, size_t *count
) {
    switch (family) {
    case OPENXCVR_SPEC_SFF8636:
        *count = sizeof(SFF8636_COMPLIANCE) / sizeof(SFF8636_COMPLIANCE[0]);
        return SFF8636_COMPLIANCE;
    case OPENXCVR_SPEC_SFF8472:
        *count = sizeof(SFF8472_COMPLIANCE) / sizeof(SFF8472_COMPLIANCE[0]);
        return SFF8472_COMPLIANCE;
    default:
        *count = 0;
        return NULL;
    }
}

/* The meaning a reported code carries; reserved codes fall into the
 * reserved range. */
static const char *compliance_meaning(openxcvr_spec_kind_t family, uint8_t code) {
    size_t count = 0;
    const compliance_code_t *table = compliance_table(family, &count);
    if (table != NULL) {
        for (size_t i = 0; i < count; ++i) {
            if (table[i].code == code) {
                return table[i].meaning;
            }
        }
    }
    return RESERVED_MEANING;
}

/* Whether a reported code establishes applicability to the (schema)
 * revision: it must name exactly that revision. */
static int compliance_admits(
    openxcvr_spec_kind_t family, uint8_t code, uint8_t major, uint8_t minor
) {
    size_t count = 0;
    const compliance_code_t *table = compliance_table(family, &count);
    if (table == NULL) {
        return 0;
    }
    for (size_t i = 0; i < count; ++i) {
        if (table[i].code == code) {
            return table[i].has_reports
                && table[i].reports_major == major
                && table[i].reports_minor == minor;
        }
    }
    return 0;
}

/* Where each SFF family's compliance byte lives (page id, field name); the
 * catalog supplies the location. */
static int compliance_field(
    openxcvr_spec_kind_t family, const char **page, const char **name
) {
    switch (family) {
    case OPENXCVR_SPEC_SFF8636:
        *page = "Sff8636LowerMemory";
        *name = "revision_compliance";
        return 1;
    case OPENXCVR_SPEC_SFF8472:
        *page = "page_a0h_base_memory";
        *name = "sff8472_compliance";
        return 1;
    default:
        return 0;
    }
}

static const char COMPLIANCE_BYTE_NOT_SUPPLIED[] =
    "revision compliance byte not supplied; module revision undetermined";

/* ─── supplied-byte view ────────────────────────────────────────────────── */

/* Mirrors Rust Range::validate: a seven-bit I2C address, a nonzero
 * length and an end within the 256-byte window (rule 9). */
static int range_valid(openxcvr_range range) {
    return range.address <= 0x7f && range.len != 0
        && (uint32_t)range.offset + range.len <= 256;
}

static int ranges_overlap(openxcvr_range a, openxcvr_range b) {
    return a.address == b.address && a.bank == b.bank && a.page == b.page
        && (uint32_t)a.offset < (uint32_t)b.offset + b.len
        && (uint32_t)b.offset < (uint32_t)a.offset + a.len;
}

/* Structural validation, mirroring the Rust Snapshot::validate refusals
 * that the identity surface also enforces. */
static openxcvr_status validate_view(const openxcvr_range_observation *ranges, size_t count) {
    for (size_t i = 0; i < count; ++i) {
        if (!range_valid(ranges[i].range)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        if (ranges[i].present
            && (ranges[i].bytes == NULL || ranges[i].len != ranges[i].range.len)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        for (size_t j = 0; j < i; ++j) {
            if (ranges[i].present && ranges[j].present
                && ranges_overlap(ranges[i].range, ranges[j].range)) {
                return OPENXCVR_ERR_INVALID_ARGUMENT;
            }
        }
    }
    return OPENXCVR_OK;
}

/* Bytes at a location, or NULL when not supplied. */
static const uint8_t *supplied_at(
    const openxcvr_range_observation *ranges, size_t count, openxcvr_range range
) {
    for (size_t i = 0; i < count; ++i) {
        const openxcvr_range seen = ranges[i].range;
        if (!ranges[i].present || seen.address != range.address || seen.bank != range.bank
            || seen.page != range.page) {
            continue;
        }
        if (seen.offset <= range.offset
            && (uint32_t)seen.offset + seen.len >= (uint32_t)range.offset + range.len) {
            return ranges[i].bytes + (range.offset - seen.offset);
        }
    }
    return NULL;
}

/* ─── row applicability (same semantics as catalog.c) ──────────────────── */

static int row_applicable(const openxcvr_catalog_row_t *row, uint8_t major, uint8_t minor) {
    if (row->has_since
        && (major < row->since_major
            || (major == row->since_major && minor < row->since_minor))) {
        return 0;
    }
    if (row->has_until
        && (major > row->until_major
            || (major == row->until_major && minor >= row->until_minor))) {
        return 0;
    }
    return 1;
}

/* ─── single-field decode ───────────────────────────────────────────────── */

static void set_state(
    openxcvr_observation_t *out, const openxcvr_resolved_field_t *entry,
    openxcvr_observation_state_t state
) {
    memset(out, 0, sizeof(*out));
    out->state = state;
    out->field = entry->row->name;
    out->has_lane = entry->has_lane;
    out->lane = entry->lane;
}

static uint16_t read_u16be(const uint8_t *bytes) {
    return (uint16_t)(((uint16_t)bytes[0] << 8) | (uint16_t)bytes[1]);
}

static uint16_t read_u16le(const uint8_t *bytes) {
    return (uint16_t)((uint16_t)bytes[0] | ((uint16_t)bytes[1] << 8));
}

static int32_t read_i16be(const uint8_t *bytes) {
    return (int32_t)(int16_t)read_u16be(bytes);
}

static uint32_t read_u32be(const uint8_t *bytes) {
    return ((uint32_t)bytes[0] << 24) | ((uint32_t)bytes[1] << 16)
        | ((uint32_t)bytes[2] << 8) | (uint32_t)bytes[3];
}

static float read_f32be(const uint8_t *bytes) {
    const uint32_t raw = read_u32be(bytes);
    float value;
    memcpy(&value, &raw, sizeof(value));
    return value;
}

static double scaled_lsb(const openxcvr_scaled_lsb_t *lsb) {
    return (double)lsb->num / (double)lsb->den;
}

/* Raw bit pattern of the field at its location. */
static int extract_raw(
    const openxcvr_resolved_field_t *entry, const uint8_t *bytes, uint16_t len, uint64_t *raw
) {
    switch (entry->row->kind) {
    case OPENXCVR_FIELD_BOOL:
    case OPENXCVR_FIELD_U2:
    case OPENXCVR_FIELD_U3:
    case OPENXCVR_FIELD_U4:
    case OPENXCVR_FIELD_U5:
    case OPENXCVR_FIELD_U6:
    case OPENXCVR_FIELD_U7:
    case OPENXCVR_FIELD_U8:
    case OPENXCVR_FIELD_I8:
        if (len < 1) {
            return 0;
        }
        {
            uint32_t width = entry->row->bit_width > 8 ? 8 : entry->row->bit_width;
            if (width == 0) {
                width = 1;
            }
            const uint64_t mask = (1ull << width) - 1;
            *raw = ((uint64_t)bytes[0] >> entry->bit_offset) & mask;
        }
        return 1;
    case OPENXCVR_FIELD_ENUM_REF:
        if (entry->row->len == 2) {
            if (len < 2) {
                return 0;
            }
            *raw = read_u16be(bytes);
        } else {
            if (len < 1) {
                return 0;
            }
            *raw = bytes[0];
        }
        return 1;
    case OPENXCVR_FIELD_U16BE:
    case OPENXCVR_FIELD_F16BE:
    case OPENXCVR_FIELD_X16BE:
    case OPENXCVR_FIELD_I16BE:
        if (len < 2) {
            return 0;
        }
        *raw = read_u16be(bytes);
        return 1;
    case OPENXCVR_FIELD_U16LE:
        if (len < 2) {
            return 0;
        }
        *raw = read_u16le(bytes);
        return 1;
    case OPENXCVR_FIELD_U32BE:
    case OPENXCVR_FIELD_I32BE:
        if (len < 4) {
            return 0;
        }
        *raw = read_u32be(bytes);
        return 1;
    case OPENXCVR_FIELD_U64LE:
        if (len < 8) {
            return 0;
        }
        {
            uint64_t value = 0;
            for (size_t i = 0; i < 8; ++i) {
                value |= (uint64_t)bytes[i] << (8 * i);
            }
            *raw = value;
        }
        return 1;
    case OPENXCVR_FIELD_U64BE:
        if (len < 8) {
            return 0;
        }
        {
            uint64_t value = 0;
            for (size_t i = 0; i < 8; ++i) {
                value = (value << 8) | (uint64_t)bytes[i];
            }
            *raw = value;
        }
        return 1;
    default:
        return 0;
    }
}

static double sign_extend(double raw, uint16_t bits) {
    uint32_t width = bits == 0 ? 1 : bits;
    if (width >= 64) {
        return raw;
    }
    const double sign = (double)(1ull << (width - 1));
    if (raw >= sign) {
        return raw - 2.0 * sign;
    }
    return raw;
}

static double raw_magnitude(
    const openxcvr_resolved_field_t *entry, uint64_t raw,
    const openxcvr_scaled_lsb_t *lsb
) {
    if (!lsb->is_signed) {
        return (double)raw;
    }
    return sign_extend((double)raw, entry->row->bit_width);
}

/* External-calibration discriminator: 1 when the external encoding
 * applies, 0 when internal, -1 when the byte is not supplied. */
static int discriminator_is_external(
    const openxcvr_resolved_field_t *entry, const openxcvr_range_observation *ranges, size_t count
) {
    if (!entry->row->has_discriminator) {
        return 0;
    }
    const openxcvr_field_loc_t *loc = &entry->row->discriminator.loc;
    const openxcvr_range range = {loc->address, 0, loc->page, (uint8_t)loc->offset, 1};
    const uint8_t *bytes = supplied_at(ranges, count, range);
    if (bytes == NULL) {
        return -1;
    }
    return (*bytes & (1u << entry->row->discriminator.bit)) != 0;
}

/* 1 with slope and offset set, 0 when the constants are not supplied. */
static int external_linear_bytes(
    const openxcvr_resolved_field_t *entry, const openxcvr_range_observation *ranges, size_t count,
    double *slope, double *offset
) {
    const openxcvr_external_linear_t *external = &entry->row->scaling.linear;
    const openxcvr_range slope_range = {
        external->slope.address, 0, external->slope.page, (uint8_t)external->slope.offset, 2
    };
    const openxcvr_range offset_range = {
        external->offset.address, 0, external->offset.page, (uint8_t)external->offset.offset, 2
    };
    const uint8_t *slope_bytes = supplied_at(ranges, count, slope_range);
    const uint8_t *offset_bytes = supplied_at(ranges, count, offset_range);
    if (slope_bytes == NULL || offset_bytes == NULL) {
        return 0;
    }
    *slope = (double)read_u16be(slope_bytes) / 256.0;
    *offset = (double)read_i16be(offset_bytes);
    return 1;
}

/* 1 with *coefficients set (degree 0..4), 0 when not supplied. */
static int external_poly_bytes(
    const openxcvr_resolved_field_t *entry, const openxcvr_range_observation *ranges, size_t count,
    double *coefficients
) {
    const openxcvr_external_poly_t *external = &entry->row->scaling.poly;
    for (size_t i = 0; i < 5; ++i) {
        const openxcvr_range range = {
            external->coefficients[i].address, 0, external->coefficients[i].page,
            (uint8_t)external->coefficients[i].offset, 4
        };
        const uint8_t *bytes = supplied_at(ranges, count, range);
        if (bytes == NULL) {
            return 0;
        }
        coefficients[i] = (double)read_f32be(bytes);
    }
    return 1;
}

static const openxcvr_catalog_row_t *find_row(
    const openxcvr_family_catalog_t *table, const char *page, const char *name,
    uint8_t major, uint8_t minor
);

/* The SFF revision gate for one resolved field: 1 (with *out set) when the
 * snapshot's compliance byte does not admit a decode at the entry's
 * revision, 0 when it does (or the family has no gate). */
static int sff_compliance_refusal(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_observation_t *out
) {
    const char *page = NULL;
    const char *name = NULL;
    if (!compliance_field(entry->family, &page, &name)) {
        return 0;
    }
    const openxcvr_family_catalog_t *table = openxcvr_catalog_table(entry->family);
    const openxcvr_catalog_row_t *row =
        find_row(table, page, name, entry->revision_major, entry->revision_minor);
    set_state(out, entry, OPENXCVR_OBS_UNSUPPORTED_REVISION);
    if (row == NULL) {
        /* No compliance row is a catalog defect: fail closed. */
        out->note = "revision compliance field not in catalog";
        return 1;
    }
    const openxcvr_range range = {row->address, 0, row->page, (uint8_t)row->offset, row->len};
    const uint8_t *bytes = supplied_at(ranges, count, range);
    if (bytes == NULL) {
        out->note = COMPLIANCE_BYTE_NOT_SUPPLIED;
        return 1;
    }
    if (compliance_admits(entry->family, bytes[0], entry->revision_major, entry->revision_minor)) {
        return 0;
    }
    out->note = compliance_meaning(entry->family, bytes[0]);
    return 1;
}

/* Whether a CMIS revision byte names a revision the family knows
 * (mirrors openxcvr_core Rev::from_major_byte_cmis). */
static int cmis_revision_known(uint8_t byte) {
    const uint8_t major = byte >> 4;
    const uint8_t minor = byte & 0x0f;
    return (major == 3 && minor == 0) || (major == 4 && minor == 0)
        || (major == 5 && minor <= 3);
}

/* Whether the byte names exactly the requested revision. */
static int cmis_revision_admits(uint8_t byte, uint8_t major, uint8_t minor) {
    return cmis_revision_known(byte) && (byte >> 4) == major && (byte & 0x0f) == minor;
}

/* ─── CMIS laser-bias scale advertisement ────────────────────────────────
 *
 * Every CMIS revision the schemas cover (3.0 through 5.3) defines the same
 * advertisement (page 01h byte 160 bits 4:3: 00b x1, 01b x2, 10b x4, 11b
 * reserved — CMIS 3.0 Table 45, CMIS 4.0 Table 8-42, CMIS 5.0/5.1
 * Table 8-55, CMIS 5.2 Table 8-56, CMIS 5.3 Table 8-63) and scales the
 * LaserBiasTx monitor by it ("unsigned integer in 2 uA increments, times
 * the multiplier"). No covered revision defines a fixed bias scale without
 * this advertisement. */

static const char BIAS_ADVERTISEMENT_NOTE[] =
    "TxBiasCurrentScalingFactor advertisement not supplied; laser bias scale undetermined";
static const char BIAS_RESERVED_NOTE[] = "reserved TxBiasCurrentScalingFactor code";

/* Whether a row's scaling is qualified by the TxBiasCurrentScalingFactor
 * advertisement (the schema's lsb_units caveat, generated onto the row). */
static int row_scaling_needs_bias_advertisement(const openxcvr_catalog_row_t *row) {
    const openxcvr_scaling_t *scaling = &row->scaling;
    return scaling->kind == OPENXCVR_SCALING_SCALED
        && scaling->scaled.note != NULL
        && strstr(scaling->scaled.note, "TxBiasCurrentScalingFactor") != NULL;
}

static int scaling_needs_bias_advertisement(const openxcvr_resolved_field_t *entry) {
    return row_scaling_needs_bias_advertisement(entry->row);
}

/* Apply the advertisement to one valid observation: x1/x2/x4 scale the
 * value, a missing advertisement yields no scaled value (the
 * not-supplied representation), a reserved code is invalid. */
static void apply_bias_advertisement(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_observation_t *observation
) {
    if (!scaling_needs_bias_advertisement(entry)
        || observation->state != OPENXCVR_OBS_VALID) {
        return;
    }
    const openxcvr_catalog_row_t *row = find_row(
        &openxcvr_cmis_catalog, "CmisPage01hAdvertising",
        "tx_bias_current_scaling_factor", entry->revision_major, entry->revision_minor
    );
    const uint8_t *bytes = NULL;
    if (row != NULL) {
        const openxcvr_range range = {row->address, 0, row->page, (uint8_t)row->offset, row->len};
        bytes = supplied_at(ranges, count, range);
    }
    if (bytes == NULL) {
        /* No advertisement (or no row): the scale is undetermined — the
         * value can be off by up to 4x — so no scaled value is produced. */
        observation->state = OPENXCVR_OBS_NOT_SUPPLIED;
        observation->has_value = 0;
        observation->note = BIAS_ADVERTISEMENT_NOTE;
        return;
    }
    double factor = 0.0;
    switch ((*bytes >> 3) & 0x03) {
    case 0:
        factor = 1.0;
        break;
    case 1:
        factor = 2.0;
        break;
    case 2:
        factor = 4.0;
        break;
    default:
        observation->state = OPENXCVR_OBS_INVALID;
        observation->has_value = 0;
        observation->note = BIAS_RESERVED_NOTE;
        return;
    }
    if (observation->has_value && observation->value.is_scaled) {
        observation->value.value *= factor;
    }
    observation->note = NULL;
}

/* ─── Stage 1 admission (supplied data) ──────────────────────────────────
 *
 * Before any value is produced: the snapshot's reported CMIS revision
 * (lower memory byte 1, found through the catalog like the Rust
 * RevisionProbe), presence prerequisites (the row's presence metadata),
 * and - for prerequisites carrying content-switch metadata - a matching
 * acquisition assertion. Mirrors openxcvr_core::observation. */

static const char NOTE_SELECTOR_EVIDENCE_MISSING[] =
    "selector acquisition evidence not supplied";
static const char NOTE_SELECTOR_EVIDENCE_CONTRADICTS[] =
    "selector evidence contradicts snapshot";
static const char NOTE_VIEW_NOT_SELECTED[] = "view not selected by selector";
static const char NOTE_PREREQ_NOT_SATISFIED[] = "presence prerequisite not satisfied";
static const char NOTE_REVISION_MISSING[] =
    "CMIS revision byte not supplied; revision undetermined";
static const char NOTE_REVISION_OTHER[] =
    "revision byte reports a different revision than requested";
static const char NOTE_REVISION_UNKNOWN[] = "revision byte reports an unknown revision";

/* The static note every F16-scaling row-shape mismatch carries (the Rust
 * core shares the wording). */
static const char NOTE_F16_SCALING_MISMATCH[] =
    "f16 scaling requires a two-byte f16be row";

/* The prerequisite's bank: the referenced page's bank policy, applied at
 * the entry's bank exactly as the Rust core does. */
static openxcvr_range prereq_range(
    const openxcvr_resolved_field_t *entry, const openxcvr_prerequisite_equals_t *prereq
) {
    const uint8_t bank = prereq->banked ? entry->range.bank : 0;
    return (openxcvr_range){
        prereq->loc.address, bank, prereq->loc.page, (uint8_t)prereq->loc.offset,
        prereq->loc.len
    };
}

/* Raw code at the prerequisite's location shape; mirrors the Rust
 * raw_bits_at extraction exactly (rule 14, ruling 3 corrected): a
 * sub-byte scalar or a non-two-byte ENUM_REF reads
 * (byte >> bit_offset) & mask with width = min(bit_width, 8) clamped
 * to 1..8; a two-byte ENUM_REF and the 16-bit kinds read their fixed
 * big- or little-endian code; U32BE/I32BE read four bytes big-endian,
 * U64LE eight bytes little-endian and U64BE eight bytes big-endian.
 * Only ASCII, BYTES, BITMAP and
 * COMPOSITE prerequisites are undecodable. A declared length shorter
 * than the kind's read is refused, never read past. */
static int prereq_raw(
    const openxcvr_prerequisite_equals_t *prereq, const uint8_t *bytes, uint64_t *raw
) {
    const uint16_t len = prereq->loc.len;
    switch (prereq->kind) {
    case OPENXCVR_FIELD_BOOL:
    case OPENXCVR_FIELD_U2:
    case OPENXCVR_FIELD_U3:
    case OPENXCVR_FIELD_U4:
    case OPENXCVR_FIELD_U5:
    case OPENXCVR_FIELD_U6:
    case OPENXCVR_FIELD_U7:
    case OPENXCVR_FIELD_U8:
    case OPENXCVR_FIELD_I8: {
        if (len < 1) {
            return 0;
        }
        uint32_t width = prereq->bit_width > 8 ? 8 : prereq->bit_width;
        if (width == 0) {
            width = 1;
        }
        const uint64_t mask = (1ull << width) - 1;
        *raw = ((uint64_t)bytes[0] >> prereq->bit_offset) & mask;
        return 1;
    }
    case OPENXCVR_FIELD_ENUM_REF:
        /* Two-byte enum refs carry a big-endian code (SFF-8472 format_id
         * style); the supplied pointer covers the declared length. A
         * non-two-byte enum ref is a sub-byte scalar exactly as Rust
         * raw_bits_at reads it: shifted and masked, never the raw byte. */
        if (len == 2) {
            *raw = read_u16be(bytes);
            return 1;
        }
        if (len < 1) {
            return 0;
        }
        {
            uint32_t width = prereq->bit_width > 8 ? 8 : prereq->bit_width;
            if (width == 0) {
                width = 1;
            }
            const uint64_t mask = (1ull << width) - 1;
            *raw = ((uint64_t)bytes[0] >> prereq->bit_offset) & mask;
        }
        return 1;
    case OPENXCVR_FIELD_U16BE:
    case OPENXCVR_FIELD_F16BE:
    case OPENXCVR_FIELD_X16BE:
    case OPENXCVR_FIELD_I16BE:
    case OPENXCVR_FIELD_U16LE:
        /* The declared length must cover the two-byte read; a short
         * location (a malformed hand-built entry — the generator always
         * emits len 2 for 16-bit kinds) is refused, never read past. */
        if (len < 2) {
            return 0;
        }
        *raw = (prereq->kind == OPENXCVR_FIELD_U16LE) ? read_u16le(bytes) : read_u16be(bytes);
        return 1;
    case OPENXCVR_FIELD_U32BE:
    case OPENXCVR_FIELD_I32BE:
        if (len < 4) {
            return 0;
        }
        *raw = read_u32be(bytes);
        return 1;
    case OPENXCVR_FIELD_U64LE:
        if (len < 8) {
            return 0;
        }
        {
            uint64_t value = 0;
            for (size_t i = 0; i < 8; ++i) {
                value |= (uint64_t)bytes[i] << (8 * i);
            }
            *raw = value;
        }
        return 1;
    case OPENXCVR_FIELD_U64BE:
        if (len < 8) {
            return 0;
        }
        {
            uint64_t value = 0;
            for (size_t i = 0; i < 8; ++i) {
                value = (value << 8) | (uint64_t)bytes[i];
            }
            *raw = value;
        }
        return 1;
    default:
        return 0;
    }
}

/* Whether one acquisition assertion covers the entry: same selector
 * coordinates and a data range covering the row's own bytes. The C view
 * carries no generation/epoch stamps; the assertion's stamps are
 * caller-asserted and matched against nothing here. */
static int evidence_covers(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_prerequisite_equals_t *prereq,
    const openxcvr_selector_acquisition_t *evidence
) {
    const openxcvr_range selector = prereq_range(entry, prereq);
    const openxcvr_range row = entry->range;
    return evidence->address == selector.address
        && evidence->bank == selector.bank
        && evidence->page == selector.page
        && (uint32_t)evidence->selector_offset == (uint32_t)selector.offset
        && evidence->data_range.address == row.address
        && evidence->data_range.bank == row.bank
        && evidence->data_range.page == row.page
        && (uint32_t)evidence->data_range.offset <= (uint32_t)row.offset
        && (uint32_t)evidence->data_range.offset + evidence->data_range.len
        >= (uint32_t)row.offset + row.len;
}

/* Reported-revision admission: the CMIS revision byte must be supplied and
 * name exactly the entry's revision. 1 (with *out set) on refusal. */
static int cmis_revision_refusal(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_observation_t *out
) {
    if (entry->family != OPENXCVR_SPEC_CMIS) {
        return 0;
    }
    const openxcvr_catalog_row_t *row =
        find_row(&openxcvr_cmis_catalog, "CmisLowerMemory", "cmis_revision",
                 entry->revision_major, entry->revision_minor);
    if (row == NULL) {
        /* No revision row is a catalog defect: fail closed. */
        set_state(out, entry, OPENXCVR_OBS_INVALID);
        out->note = "CMIS revision field not in catalog";
        return 1;
    }
    const openxcvr_range range = {row->address, 0, row->page, (uint8_t)row->offset, row->len};
    const uint8_t *bytes = supplied_at(ranges, count, range);
    if (bytes == NULL) {
        set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
        out->note = NOTE_REVISION_MISSING;
        return 1;
    }
    if (!cmis_revision_admits(bytes[0], entry->revision_major, entry->revision_minor)) {
        set_state(out, entry, OPENXCVR_OBS_UNSUPPORTED_REVISION);
        out->note = cmis_revision_known(bytes[0]) ? NOTE_REVISION_OTHER : NOTE_REVISION_UNKNOWN;
        return 1;
    }
    return 0;
}

/* Presence admission. 1 (with *out set) on refusal. */
static int presence_refusal(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_acquisition_context_t *ctx,
    openxcvr_observation_t *out
) {
    const openxcvr_catalog_row_t *row = entry->row;
    switch (row->presence_kind) {
    case OPENXCVR_PRESENCE_UNCONDITIONAL:
        return 0;
    case OPENXCVR_PRESENCE_UNSUPPORTED:
        set_state(out, entry, OPENXCVR_OBS_UNSUPPORTED_FIELD);
        out->note = row->presence_expr;
        return 1;
    case OPENXCVR_PRESENCE_EQUALS:
        break;
    }
    const openxcvr_prerequisite_equals_t *prereq = &row->presence_equals;
    if (prereq->has_content_switch) {
        /* Content-multiplexed row: missing evidence comes first, before
         * the selector value is compared (the all-rows-NotSupplied
         * contract for a context-free decode). */
        size_t matching = 0;
        size_t contradictions = 0;
        if (ctx != NULL) {
            for (size_t i = 0; i < ctx->count; ++i) {
                if (!evidence_covers(entry, prereq, &ctx->acquisitions[i])) {
                    continue;
                }
                matching++;
            }
        }
        if (matching == 0) {
            set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
            out->note = NOTE_SELECTOR_EVIDENCE_MISSING;
            return 1;
        }
        /* Cross-check every matching assertion against the snapshot's own
         * selector byte; a disagreeing assertion is a contradiction,
         * never ignored in favour of another record. */
        const openxcvr_range selector = prereq_range(entry, prereq);
        const uint8_t *bytes = supplied_at(ranges, count, selector);
        if (bytes == NULL) {
            set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
            return 1;
        }
        uint64_t raw = 0;
        uint8_t snapshot_value = 0;
        if (!prereq_raw(prereq, bytes, &raw)) {
            set_state(out, entry, OPENXCVR_OBS_INVALID);
            out->note = "selector byte could not be decoded";
            return 1;
        }
        snapshot_value = (uint8_t)raw;
        if (ctx != NULL) {
            for (size_t i = 0; i < ctx->count; ++i) {
                if (evidence_covers(entry, prereq, &ctx->acquisitions[i])
                    && ctx->acquisitions[i].selector_value != snapshot_value) {
                    contradictions++;
                }
            }
        }
        if (contradictions > 0) {
            set_state(out, entry, OPENXCVR_OBS_INVALID);
            out->note = NOTE_SELECTOR_EVIDENCE_CONTRADICTS;
            return 1;
        }
        /* The snapshot selector byte (asserted stable) must select this
         * row's view: 00h, reserved, custom and other standard views are
         * all Invalid with no value. */
        if ((uint64_t)snapshot_value != prereq->expected) {
            set_state(out, entry, OPENXCVR_OBS_INVALID);
            out->note = NOTE_VIEW_NOT_SELECTED;
            return 1;
        }
        return 0;
    }
    /* Plain prerequisite: the referenced bytes decide, no evidence. */
    const openxcvr_range prereq_loc = prereq_range(entry, prereq);
    const uint8_t *bytes = supplied_at(ranges, count, prereq_loc);
    if (bytes == NULL) {
        set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
        return 1;
    }
    uint64_t raw = 0;
    if (!prereq_raw(prereq, bytes, &raw)) {
        set_state(out, entry, OPENXCVR_OBS_INVALID);
        out->note = "presence prerequisite could not be decoded";
        return 1;
    }
    if (raw != prereq->expected) {
        set_state(out, entry, OPENXCVR_OBS_INVALID);
        out->note = NOTE_PREREQ_NOT_SATISFIED;
        return 1;
    }
    return 0;
}

/* Family-agnostic single-field decode with acquisition context (the
 * previous body of openxcvr_observe_field plus Stage 1 admission). */
static openxcvr_status decode_field_entry(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    const openxcvr_acquisition_context_t *ctx,
    openxcvr_observation_t *out
) {
    if (cmis_revision_refusal(entry, ranges, count, out)) {
        return OPENXCVR_OK;
    }
    if (presence_refusal(entry, ranges, count, ctx, out)) {
        return OPENXCVR_OK;
    }
    const int authorize = options != NULL && options->authorize_clear_on_read != 0;
    if ((entry->row->read_effect_mask & OPENXCVR_READ_EFFECT_CLEAR_ON_READ) != 0 && !authorize) {
        set_state(out, entry, OPENXCVR_OBS_CLEAR_ON_READ_REFUSED);
        return OPENXCVR_OK;
    }
    const uint8_t *bytes = supplied_at(ranges, count, entry->range);
    if (bytes == NULL) {
        set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
        return OPENXCVR_OK;
    }
    uint64_t raw = 0;
    if (!extract_raw(entry, bytes, entry->range.len, &raw)) {
        set_state(out, entry, OPENXCVR_OBS_UNSUPPORTED_FIELD);
        return OPENXCVR_OK;
    }
    const openxcvr_scaling_t *scaling = &entry->row->scaling;
    switch (scaling->kind) {
    case OPENXCVR_SCALING_NONE:
        set_state(out, entry, OPENXCVR_OBS_VALID);
        out->has_value = 1;
        out->value.is_scaled = 0;
        out->value.raw = raw;
        return OPENXCVR_OK;
    case OPENXCVR_SCALING_SCALED:
        set_state(out, entry, OPENXCVR_OBS_VALID);
        out->has_value = 1;
        out->value.is_scaled = 1;
        out->value.value =
            raw_magnitude(entry, raw, &scaling->scaled) * scaled_lsb(&scaling->scaled);
        out->value.unit = scaling->scaled.unit;
        out->note = scaling->scaled.note;
        return OPENXCVR_OK;
    case OPENXCVR_SCALING_LINEAR: {
        const int external = discriminator_is_external(entry, ranges, count);
        if (external < 0) {
            set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
            return OPENXCVR_OK;
        }
        if (!external) {
            set_state(out, entry, OPENXCVR_OBS_VALID);
            out->has_value = 1;
            out->value.is_scaled = 1;
            out->value.value =
                raw_magnitude(entry, raw, &scaling->scaled) * scaled_lsb(&scaling->scaled);
            out->value.unit = scaling->scaled.unit;
            out->note = scaling->scaled.note;
            return OPENXCVR_OK;
        }
        double slope = 0.0;
        double offset = 0.0;
        if (!external_linear_bytes(entry, ranges, count, &slope, &offset)) {
            set_state(out, entry, OPENXCVR_OBS_UNCALIBRATED);
            return OPENXCVR_OK;
        }
        /* Calibrated result stays in internal LSB units, then scales. */
        const double calibrated =
            slope * raw_magnitude(entry, raw, &scaling->scaled) + offset;
        set_state(out, entry, OPENXCVR_OBS_VALID);
        out->has_value = 1;
        out->value.is_scaled = 1;
        out->value.value = calibrated * scaled_lsb(&scaling->scaled);
        out->value.unit = scaling->scaled.unit;
        return OPENXCVR_OK;
    }
    case OPENXCVR_SCALING_POLYNOMIAL: {
        const int external = discriminator_is_external(entry, ranges, count);
        if (external < 0) {
            set_state(out, entry, OPENXCVR_OBS_NOT_SUPPLIED);
            return OPENXCVR_OK;
        }
        if (!external) {
            set_state(out, entry, OPENXCVR_OBS_VALID);
            out->has_value = 1;
            out->value.is_scaled = 1;
            out->value.value =
                raw_magnitude(entry, raw, &scaling->scaled) * scaled_lsb(&scaling->scaled);
            out->value.unit = scaling->scaled.unit;
            out->note = scaling->scaled.note;
            return OPENXCVR_OK;
        }
        double coefficients[5] = {0.0, 0.0, 0.0, 0.0, 0.0};
        if (!external_poly_bytes(entry, ranges, count, coefficients)) {
            set_state(out, entry, OPENXCVR_OBS_UNCALIBRATED);
            return OPENXCVR_OK;
        }
        /* coefficients[i] is the degree-i term; the result is in internal
         * LSB units. */
        const double r = (double)raw;
        const double poly = coefficients[4] * r * r * r * r + coefficients[3] * r * r * r
            + coefficients[2] * r * r + coefficients[1] * r + coefficients[0];
        set_state(out, entry, OPENXCVR_OBS_VALID);
        out->has_value = 1;
        out->value.is_scaled = 1;
        out->value.value = poly * scaled_lsb(&scaling->scaled);
        out->value.unit = scaling->scaled.unit;
        return OPENXCVR_OK;
    }
    case OPENXCVR_SCALING_F16:
        /* F16 scaling is meaningful only on a two-byte f16be row whose
         * raw code is the 16-bit F16 code - never a narrowed
         * reinterpretation of an arbitrary u64. Any other shape is a
         * row/catalog mismatch stated as UNSUPPORTED_FIELD. The 16-bit
         * code rides along in value.raw (see observation.h). */
        /* Parity: this arm checks the resolved range length where Rust
         * checks the row length; they agree for every resolver-produced
         * entry (catalog resolve sets range.len = row->len; the planner
         * rejects a mismatch). */
        if (entry->row->kind != OPENXCVR_FIELD_F16BE || entry->range.len != 2 || raw > 0xFFFF) {
            set_state(out, entry, OPENXCVR_OBS_UNSUPPORTED_FIELD);
            out->note = NOTE_F16_SCALING_MISMATCH;
            return OPENXCVR_OK;
        }
        set_state(out, entry, OPENXCVR_OBS_VALID);
        out->has_value = 1;
        out->value.is_scaled = 1;
        out->value.value = openxcvr_cmis_f16_to_double((uint16_t)raw);
        out->value.unit = scaling->scaled.unit;
        out->value.raw = raw;
        return OPENXCVR_OK;
    case OPENXCVR_SCALING_OTHER:
    default:
        set_state(out, entry, OPENXCVR_OBS_UNSUPPORTED_FIELD);
        out->note = scaling->other_kind;
        return OPENXCVR_OK;
    }
}

openxcvr_status openxcvr_observe_field(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    openxcvr_observation_t *out
) {
    if (entry == NULL || entry->row == NULL || ranges == NULL || out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    openxcvr_status status = validate_view(ranges, count);
    if (status != OPENXCVR_OK) {
        return status;
    }
    /* The same family rules as openxcvr_observe_basic, so the generic
     * field path cannot bypass them. */
    if (sff_compliance_refusal(entry, ranges, count, out)) {
        return OPENXCVR_OK;
    }
    status = decode_field_entry(entry, ranges, count, options, NULL, out);
    if (status != OPENXCVR_OK) {
        return status;
    }
    apply_bias_advertisement(entry, ranges, count, out);
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_observe_field_ctx(
    const openxcvr_resolved_field_t *entry,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    const openxcvr_acquisition_context_t *ctx,
    openxcvr_observation_t *out
) {
    if (entry == NULL || entry->row == NULL || ranges == NULL || out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    /* A context that claims acquisitions but supplies none would be
     * dereferenced during presence admission: refuse it here. */
    if (ctx != NULL && ctx->count > 0 && ctx->acquisitions == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    openxcvr_status status = validate_view(ranges, count);
    if (status != OPENXCVR_OK) {
        return status;
    }
    if (sff_compliance_refusal(entry, ranges, count, out)) {
        return OPENXCVR_OK;
    }
    status = decode_field_entry(entry, ranges, count, options, ctx, out);
    if (status != OPENXCVR_OK) {
        return status;
    }
    apply_bias_advertisement(entry, ranges, count, out);
    return OPENXCVR_OK;
}

/* ─── basic observation set ─────────────────────────────────────────────── */

struct monitor_names {
    const char *temperature_page, *temperature;
    const char *supply_page, *supply;
    const char *bias_page, *bias;
    const char *tx_page, *tx;
    const char *rx_page, *rx;
};

static const struct monitor_names CMIS_MONITORS = {
    "CmisLowerMemory", "temp_mon_value",
    "CmisLowerMemory", "vcc_mon_voltage",
    "CmisPage11hLaneStatus", "laser_bias_tx",
    "CmisPage11hLaneStatus", "optical_power_tx",
    "CmisPage11hLaneStatus", "optical_power_rx",
};

static const struct monitor_names SFF8636_MONITORS = {
    "Sff8636LowerMemory", "temperature",
    "Sff8636LowerMemory", "supply_voltage",
    "Sff8636LowerMemory", "tx_bias_lane",
    "Sff8636LowerMemory", "tx_power_lane",
    "Sff8636LowerMemory", "rx_power_lane",
};

static const struct monitor_names SFF8472_MONITORS = {
    "page_a2h_diagnostics_base", "temperature",
    "page_a2h_diagnostics_base", "supply_voltage",
    "page_a2h_diagnostics_base", "tx_bias",
    "page_a2h_diagnostics_base", "tx_power",
    "page_a2h_diagnostics_base", "rx_power",
};

static const struct monitor_names *monitors_for(openxcvr_spec_kind_t family) {
    switch (family) {
    case OPENXCVR_SPEC_CMIS:
        return &CMIS_MONITORS;
    case OPENXCVR_SPEC_SFF8636:
        return &SFF8636_MONITORS;
    case OPENXCVR_SPEC_SFF8472:
        return &SFF8472_MONITORS;
    default:
        return NULL;
    }
}

/* Exact (page, name) row lookup in one family table at one revision. */
static const openxcvr_catalog_row_t *find_row(
    const openxcvr_family_catalog_t *table, const char *page, const char *name,
    uint8_t major, uint8_t minor
) {
    for (size_t i = 0; i < table->row_count; ++i) {
        const openxcvr_catalog_row_t *row = &table->rows[i];
        if (!row_applicable(row, major, minor)) {
            continue;
        }
        if (strcmp(row->page_id, page) == 0 && strcmp(row->name, name) == 0) {
            return row;
        }
    }
    return NULL;
}

/* Resolve one monitor (optionally one lane) through the public resolver so
 * the catalog stays the single location source. */
static openxcvr_status resolve_monitor(
    openxcvr_spec_kind_t family, const openxcvr_family_catalog_t *table, const char *page,
    const char *name, uint8_t major, uint8_t minor, openxcvr_lane_selection_t lane,
    uint16_t lane_arg, openxcvr_resolved_field_t *out
) {
    const openxcvr_catalog_row_t *row = find_row(table, page, name, major, minor);
    if (row == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    const openxcvr_field_query_t query = {name, lane, lane_arg, page, 0};
    size_t count = 0;
    const openxcvr_status status =
        openxcvr_catalog_resolve(family, major, minor, &query, out, 1, &count);
    if (status != OPENXCVR_OK || count != 1) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    return OPENXCVR_OK;
}

/* CMIS: prove the requested revision against the snapshot's byte 1. */
static openxcvr_status verify_cmis_revision(
    uint8_t major, uint8_t minor, const openxcvr_range_observation *ranges, size_t count
) {
    const openxcvr_catalog_row_t *row =
        find_row(&openxcvr_cmis_catalog, "CmisLowerMemory", "cmis_revision", major, minor);
    if (row == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    const openxcvr_range range = {row->address, 0, row->page, (uint8_t)row->offset, row->len};
    const uint8_t *bytes = supplied_at(ranges, count, range);
    if (bytes == NULL) {
        return OPENXCVR_ERR_INSUFFICIENT_INPUT;
    }
    const uint8_t detected_major = bytes[0] >> 4;
    const uint8_t detected_minor = bytes[0] & 0x0f;
    const int known = (detected_major == 3 && detected_minor == 0)
        || (detected_major == 4 && detected_minor == 0)
        || (detected_major == 5 && detected_minor <= 3);
    if (!known) {
        return OPENXCVR_ERR_UNSUPPORTED_REVISION;
    }
    if (detected_major != major || detected_minor != minor) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_observe_basic(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_range_observation *ranges, size_t count,
    const openxcvr_observe_options_t *options,
    openxcvr_basic_observations_t *out
) {
    if (ranges == NULL || out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    const openxcvr_family_catalog_t *table = openxcvr_catalog_table(family);
    const struct monitor_names *monitors = monitors_for(family);
    if (table == NULL || monitors == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    openxcvr_status status = validate_view(ranges, count);
    if (status != OPENXCVR_OK) {
        return status;
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
    if (family == OPENXCVR_SPEC_CMIS) {
        status = verify_cmis_revision(revision_major, revision_minor, ranges, count);
        if (status != OPENXCVR_OK) {
            return status;
        }
    }

    openxcvr_resolved_field_t temperature_entry, supply_entry, bias_entry, tx_entry, rx_entry;
    status = resolve_monitor(family, table, monitors->temperature_page, monitors->temperature,
        revision_major, revision_minor, OPENXCVR_LANE_UNSPECIFIED, 0, &temperature_entry);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = resolve_monitor(family, table, monitors->supply_page, monitors->supply,
        revision_major, revision_minor, OPENXCVR_LANE_UNSPECIFIED, 0, &supply_entry);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = resolve_monitor(family, table, monitors->bias_page, monitors->bias,
        revision_major, revision_minor, OPENXCVR_LANE_UNSPECIFIED, 0, &bias_entry);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = resolve_monitor(family, table, monitors->tx_page, monitors->tx,
        revision_major, revision_minor, OPENXCVR_LANE_UNSPECIFIED, 0, &tx_entry);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = resolve_monitor(family, table, monitors->rx_page, monitors->rx,
        revision_major, revision_minor, OPENXCVR_LANE_UNSPECIFIED, 0, &rx_entry);
    if (status != OPENXCVR_OK) {
        return status;
    }

    memset(out, 0, sizeof(*out));
    out->revision_major = revision_major;
    out->revision_minor = revision_minor;

    /* What the module reports, for the result: SFF-8636 page 00h byte 1 /
     * SFF-8472 A0h byte 94 (CMIS has no compliance code). The per-field
     * gate inside openxcvr_observe_field decides the decode outcomes. */
    {
        const char *page = NULL;
        const char *name = NULL;
        if (compliance_field(family, &page, &name)) {
            const openxcvr_catalog_row_t *row =
                find_row(table, page, name, revision_major, revision_minor);
            if (row != NULL) {
                const openxcvr_range range = {
                    row->address, 0, row->page, (uint8_t)row->offset, row->len
                };
                const uint8_t *bytes = supplied_at(ranges, count, range);
                if (bytes != NULL) {
                    out->has_reported_compliance = 1;
                    out->reported_compliance_code = bytes[0];
                    out->reported_compliance_meaning =
                        compliance_meaning(family, bytes[0]);
                }
            }
        }
    }

    status = openxcvr_observe_field(&temperature_entry, ranges, count, options, &out->temperature);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_observe_field(&supply_entry, ranges, count, options, &out->supply_voltage);
    if (status != OPENXCVR_OK) {
        return status;
    }

    const openxcvr_catalog_row_t *bias_row = bias_entry.row;
    uint16_t lane_first = 1, lane_last = 1;
    int per_lane = 0;
    if (bias_row->has_instances) {
        per_lane = 1;
        lane_first = bias_row->instances.first;
        lane_last = bias_row->instances.last;
    }
    if ((size_t)(lane_last - lane_first + 1) > OPENXCVR_OBSERVATION_MAX_LANES) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    for (uint32_t lane = lane_first; lane <= lane_last; ++lane) {
        openxcvr_resolved_field_t lane_bias, lane_tx, lane_rx;
        const openxcvr_lane_selection_t selection =
            per_lane ? OPENXCVR_LANE_ONE : OPENXCVR_LANE_UNSPECIFIED;
        status = resolve_monitor(family, table, monitors->bias_page, monitors->bias,
            revision_major, revision_minor, selection, (uint16_t)lane, &lane_bias);
        if (status != OPENXCVR_OK) {
            return status;
        }
        status = resolve_monitor(family, table, monitors->tx_page, monitors->tx,
            revision_major, revision_minor, selection, (uint16_t)lane, &lane_tx);
        if (status != OPENXCVR_OK) {
            return status;
        }
        status = resolve_monitor(family, table, monitors->rx_page, monitors->rx,
            revision_major, revision_minor, selection, (uint16_t)lane, &lane_rx);
        if (status != OPENXCVR_OK) {
            return status;
        }
        openxcvr_lane_observations_t *slot = &out->lanes[out->lane_count++];
        slot->lane = (uint16_t)(per_lane ? lane : 1);
        status = openxcvr_observe_field(&lane_bias, ranges, count, options, &slot->tx_bias);
        if (status != OPENXCVR_OK) {
            return status;
        }
        status = openxcvr_observe_field(&lane_tx, ranges, count, options, &slot->tx_power);
        if (status != OPENXCVR_OK) {
            return status;
        }
        status = openxcvr_observe_field(&lane_rx, ranges, count, options, &slot->rx_power);
        if (status != OPENXCVR_OK) {
            return status;
        }
    }
    return OPENXCVR_OK;
}

/* ─── Basic observation read plan ──────────────────────────────────────
 *
 * Mirrors openxcvr::observation::{basic_read_prelude, basic_read_remaining}:
 * which exact bytes a caller must acquire so openxcvr_observe_basic decodes
 * everything a full snapshot would. Every location comes from the same
 * catalog rows the decoder resolves (find_row over the generated tables);
 * there is no second offset table. Two stages, because what the decode
 * reads depends on bytes only known after reading: a fixed per-family
 * prelude (revision/compliance gate byte, plus the SFF-8472 calibration
 * discriminator byte), then the remaining ranges given those bytes. */

/* 5 monitors + at most 5 constants per monitor + the CMIS advertisement. */
#define PLAN_MAX_READS 32

static openxcvr_planned_read_t make_planned(
    openxcvr_range range, const char *purpose
) {
    openxcvr_planned_read_t read;
    memset(&read, 0, sizeof(read));
    read.range = range;
    /* A range in an upper page (page != 0) is optional: flat-memory
     * modules implement only page 00h (CMIS memory_model, lower byte 2
     * bit 7; SFF-8636 flat_memory, byte 2 bit 2). */
    read.optional = range.page != 0;
    read.purpose = purpose;
    return read;
}

static int plan_push(
    openxcvr_planned_read_t *reads, size_t *count,
    openxcvr_range range, const char *purpose
) {
    if (*count >= PLAN_MAX_READS) {
        /* A generated-table shape beyond the planned bound: refuse rather
         * than write past it. */
        return 0;
    }
    reads[(*count)++] = make_planned(range, purpose);
    return 1;
}

/* The row's own range at bank 0 (the decoder's monitor queries use bank 0). */
static openxcvr_range row_range_bank0(const openxcvr_catalog_row_t *row) {
    return (openxcvr_range){row->address, 0, row->page, (uint8_t)row->offset, row->len};
}

/* The enclosing span of the row's instance dimension: the exact bytes
 * every instance occupies (a plain field is its own span). */
static openxcvr_range row_span(const openxcvr_catalog_row_t *row) {
    openxcvr_range range = row_range_bank0(row);
    if (row->has_instances && row->instances.count > 0) {
        const uint16_t first = row->instances.offsets[0];
        const uint16_t last =
            (uint16_t)(row->instances.offsets[row->instances.count - 1] + row->len);
        range.offset = (uint8_t)first;
        range.len = (uint16_t)(last - first);
    }
    return range;
}

static openxcvr_range loc_range(const openxcvr_field_loc_t *loc) {
    return (openxcvr_range){loc->address, 0, loc->page, (uint8_t)loc->offset, loc->len};
}

static int same_range(openxcvr_range a, openxcvr_range b) {
    return a.address == b.address && a.bank == b.bank && a.page == b.page
        && a.offset == b.offset && a.len == b.len;
}

/* Family/revision gate both plan entry points share. */
static openxcvr_status plan_table(
    openxcvr_spec_kind_t family, uint8_t major, uint8_t minor,
    const openxcvr_family_catalog_t **table, const struct monitor_names **monitors
) {
    *table = openxcvr_catalog_table(family);
    *monitors = monitors_for(family);
    if (*table == NULL || *monitors == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    int supported = 0;
    for (size_t i = 0; i < (*table)->supported_rev_count; ++i) {
        if ((*table)->supported_revs[i].major == major
            && (*table)->supported_revs[i].minor == minor) {
            supported = 1;
            break;
        }
    }
    if (!supported) {
        return OPENXCVR_ERR_UNSUPPORTED_REVISION;
    }
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_basic_read_prelude(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    openxcvr_planned_read_t *out, size_t capacity, size_t *count
) {
    if (count == NULL || (out == NULL && capacity != 0)) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    const openxcvr_family_catalog_t *table = NULL;
    const struct monitor_names *monitors = NULL;
    openxcvr_status status =
        plan_table(family, revision_major, revision_minor, &table, &monitors);
    if (status != OPENXCVR_OK) {
        return status;
    }
    openxcvr_planned_read_t reads[PLAN_MAX_READS];
    size_t n = 0;
    /* The revision/compliance gate row. */
    switch (family) {
    case OPENXCVR_SPEC_CMIS: {
        const openxcvr_catalog_row_t *row =
            find_row(table, "CmisLowerMemory", "cmis_revision", revision_major, revision_minor);
        if (row == NULL || !plan_push(reads, &n, row_range_bank0(row), row->name)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        break;
    }
    case OPENXCVR_SPEC_SFF8636:
    case OPENXCVR_SPEC_SFF8472: {
        const char *page = NULL;
        const char *name = NULL;
        if (!compliance_field(family, &page, &name)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        const openxcvr_catalog_row_t *row =
            find_row(table, page, name, revision_major, revision_minor);
        if (row == NULL || !plan_push(reads, &n, row_range_bank0(row), row->name)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        break;
    }
    default:
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    /* The monitor rows' external-calibration discriminators, unique, in
     * monitor order (SFF-8472's five rows share one location). */
    const struct {
        const char *page;
        const char *name;
    } five[5] = {
        {monitors->temperature_page, monitors->temperature},
        {monitors->supply_page, monitors->supply},
        {monitors->bias_page, monitors->bias},
        {monitors->tx_page, monitors->tx},
        {monitors->rx_page, monitors->rx},
    };
    for (size_t i = 0; i < 5; ++i) {
        const openxcvr_catalog_row_t *row =
            find_row(table, five[i].page, five[i].name, revision_major, revision_minor);
        if (row == NULL || !row->has_discriminator) {
            continue;
        }
        const openxcvr_range range = loc_range(&row->discriminator.loc);
        int seen = 0;
        for (size_t j = 0; j < n; ++j) {
            if (same_range(reads[j].range, range)) {
                seen = 1;
                break;
            }
        }
        if (!seen
            && !plan_push(reads, &n, range, row->discriminator.field)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
    }
    *count = n;
    if (out == NULL) {
        return OPENXCVR_OK;
    }
    if (capacity < n) {
        return OPENXCVR_ERR_SHORT_BUFFER;
    }
    memcpy(out, reads, n * sizeof(reads[0]));
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_basic_read_remaining(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_planned_read_t *out, size_t capacity, size_t *out_count
) {
    if (ranges == NULL || out_count == NULL || (out == NULL && capacity != 0)) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    const openxcvr_family_catalog_t *table = NULL;
    const struct monitor_names *monitors = NULL;
    openxcvr_status status =
        plan_table(family, revision_major, revision_minor, &table, &monitors);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = validate_view(ranges, count);
    if (status != OPENXCVR_OK) {
        return status;
    }
    /* The same gates openxcvr_observe_basic applies. */
    switch (family) {
    case OPENXCVR_SPEC_CMIS: {
        const openxcvr_catalog_row_t *row =
            find_row(table, "CmisLowerMemory", "cmis_revision", revision_major, revision_minor);
        if (row == NULL) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        const uint8_t *bytes = supplied_at(ranges, count, row_range_bank0(row));
        if (bytes == NULL) {
            return OPENXCVR_ERR_INSUFFICIENT_INPUT;
        }
        const uint8_t detected_major = bytes[0] >> 4;
        const uint8_t detected_minor = bytes[0] & 0x0f;
        const int known = (detected_major == 3 && detected_minor == 0)
            || (detected_major == 4 && detected_minor == 0)
            || (detected_major == 5 && detected_minor <= 3);
        if (!known) {
            return OPENXCVR_ERR_UNSUPPORTED_REVISION;
        }
        if (detected_major != revision_major || detected_minor != revision_minor) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        break;
    }
    case OPENXCVR_SPEC_SFF8636:
    case OPENXCVR_SPEC_SFF8472: {
        const char *page = NULL;
        const char *name = NULL;
        if (!compliance_field(family, &page, &name)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        const openxcvr_catalog_row_t *row =
            find_row(table, page, name, revision_major, revision_minor);
        if (row == NULL) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        const uint8_t *bytes = supplied_at(ranges, count, row_range_bank0(row));
        if (bytes == NULL) {
            return OPENXCVR_ERR_INSUFFICIENT_INPUT;
        }
        if (!compliance_admits(family, bytes[0], revision_major, revision_minor)) {
            /* The module's reported compliance does not admit a decode at
             * the schema revision: no monitor byte is planned. The code
             * and its meaning stay retrievable through
             * openxcvr_observe_basic on the prelude bytes. */
            return OPENXCVR_ERR_UNSUPPORTED_REVISION;
        }
        break;
    }
    default:
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    /* The monitors' spans, each field followed by its external-calibration
     * constants when its discriminator byte says the external variant. */
    openxcvr_planned_read_t reads[PLAN_MAX_READS];
    size_t n = 0;
    const struct {
        const char *page;
        const char *name;
    } five[5] = {
        {monitors->temperature_page, monitors->temperature},
        {monitors->supply_page, monitors->supply},
        {monitors->bias_page, monitors->bias},
        {monitors->tx_page, monitors->tx},
        {monitors->rx_page, monitors->rx},
    };
    for (size_t i = 0; i < 5; ++i) {
        const openxcvr_catalog_row_t *row =
            find_row(table, five[i].page, five[i].name, revision_major, revision_minor);
        if (row == NULL || !plan_push(reads, &n, row_span(row), row->name)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        if (!row->has_discriminator) {
            continue;
        }
        const openxcvr_range discriminator = loc_range(&row->discriminator.loc);
        const uint8_t *bytes = supplied_at(ranges, count, discriminator);
        if (bytes == NULL) {
            return OPENXCVR_ERR_INSUFFICIENT_INPUT;
        }
        if ((*bytes & (1u << row->discriminator.bit)) == 0) {
            continue;
        }
        if (row->scaling.kind == OPENXCVR_SCALING_LINEAR) {
            if (!plan_push(
                    reads, &n, loc_range(&row->scaling.linear.slope), row->name)
                || !plan_push(
                    reads, &n, loc_range(&row->scaling.linear.offset), row->name)) {
                return OPENXCVR_ERR_INVALID_ARGUMENT;
            }
        } else if (row->scaling.kind == OPENXCVR_SCALING_POLYNOMIAL) {
            for (size_t slot = 0; slot < 5; ++slot) {
                if (!plan_push(
                        reads, &n,
                        loc_range(&row->scaling.poly.coefficients[slot]), row->name)) {
                    return OPENXCVR_ERR_INVALID_ARGUMENT;
                }
            }
        }
    }
    /* CMIS: the TxBiasCurrentScalingFactor advertisement, when a planned
     * monitor's scaling is qualified by it (the same test
     * apply_bias_advertisement uses). */
    if (family == OPENXCVR_SPEC_CMIS) {
        const openxcvr_catalog_row_t *bias =
            find_row(table, monitors->bias_page, monitors->bias, revision_major, revision_minor);
        if (bias == NULL) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        if (row_scaling_needs_bias_advertisement(bias)) {
            const openxcvr_catalog_row_t *advertisement = find_row(
                &openxcvr_cmis_catalog, "CmisPage01hAdvertising",
                "tx_bias_current_scaling_factor", revision_major, revision_minor
            );
            if (advertisement == NULL
                || !plan_push(
                    reads, &n, row_range_bank0(advertisement), advertisement->name)) {
                return OPENXCVR_ERR_INVALID_ARGUMENT;
            }
        }
    }
    *out_count = n;
    if (out == NULL) {
        return OPENXCVR_OK;
    }
    if (capacity < n) {
        return OPENXCVR_ERR_SHORT_BUFFER;
    }
    memcpy(out, reads, n * sizeof(reads[0]));
    return OPENXCVR_OK;
}

/* Rule 8 helpers: a metadata span is valid exactly when its uint16_t
 * offset and length are in-window — validated BEFORE any conversion to
 * the uint8_t range offset — and a location is valid when its address
 * is within the seven-bit I2C bound (openxcvr.h:32, the same bound
 * range_valid enforces on views and Rust Range::validate enforces on
 * every planned range) and its span is. */
static int plan_meta_range_valid(uint16_t offset, uint16_t len) {
    return len != 0 && (uint32_t)offset + (uint32_t)len <= 256;
}

static int plan_loc_valid(const openxcvr_field_loc_t *loc) {
    return loc->address <= 0x7f && plan_meta_range_valid(loc->offset, loc->len);
}

/* C-B1/C-A4: a catalog row's own planned range (the family gate, the
 * bias advertisement) must satisfy the same two bounds before it is
 * planned or read, so a defective generated row fails closed with
 * INVALID_ARGUMENT instead of truncating into an out-of-contract
 * range — matching Rust gate_range.validate() and
 * advertisement_range.validate(). */
static int plan_row_range_valid(const openxcvr_catalog_row_t *row) {
    return row->address <= 0x7f && plan_meta_range_valid(row->offset, row->len);
}

/* A value inside a generated enum's declared tag range (a plain signed
 * comparison would admit negative casts). */
static int plan_field_kind_known(openxcvr_field_kind_t kind) {
    return (unsigned)kind <= (unsigned)OPENXCVR_FIELD_ENUM_REF;
}

static int plan_presence_kind_known(openxcvr_presence_kind_t kind) {
    return (unsigned)kind <= (unsigned)OPENXCVR_PRESENCE_UNSUPPORTED;
}

static int plan_scaling_kind_known(openxcvr_scaling_kind_t kind) {
    return (unsigned)kind <= (unsigned)OPENXCVR_SCALING_F16;
}

/* Whether a prerequisite kind's extraction shifts within one byte
 * (rule 8: the shift bound is validated before the shift happens). */
static int prereq_shifts_within_byte(openxcvr_field_kind_t kind) {
    switch (kind) {
    case OPENXCVR_FIELD_BOOL:
    case OPENXCVR_FIELD_U2:
    case OPENXCVR_FIELD_U3:
    case OPENXCVR_FIELD_U4:
    case OPENXCVR_FIELD_U5:
    case OPENXCVR_FIELD_U6:
    case OPENXCVR_FIELD_U7:
    case OPENXCVR_FIELD_U8:
    case OPENXCVR_FIELD_I8:
    case OPENXCVR_FIELD_ENUM_REF:
        return 1;
    default:
        return 0;
    }
}

/* Rule 8 structural validation of one entry against its row metadata:
 * known enum tags (field kind, presence kind, prerequisite field kind,
 * scaling kind), required metadata strings the plan's outputs borrow
 * (the row name, the prerequisite field, the unsupported expression, the
 * discriminator field), the row's own seven-bit address, in-window
 * footprint and applicability at the requested revision, an entry range
 * consistent with the row (address, page, length, bank policy and the
 * selected instance), the seven-bit address and in-window span of every
 * prerequisite, discriminator, content-switch and declared external
 * constant location (ruling 2: a LINEAR row always
 * carries slope and offset locations, a POLYNOMIAL row five
 * coefficients), and extraction shift bounds. Structurally valid
 * hand-built rows need no generated-table membership; a known
 * non-scalar prerequisite kind is NOT malformed and refuses at
 * admission instead (ruling 3). */
static int plan_row_valid(const openxcvr_resolved_field_t *entry) {
    const openxcvr_catalog_row_t *row = entry->row;

    if (row->name == NULL
        || !plan_field_kind_known(row->kind)
        || !plan_presence_kind_known(row->presence_kind)
        || !plan_scaling_kind_known(row->scaling.kind)) {
        return 0;
    }
    if (!row_applicable(row, entry->revision_major, entry->revision_minor)) {
        return 0;
    }
    if (row->address > 0x7f || !plan_meta_range_valid(row->offset, row->len)) {
        return 0;
    }

    /* The resolved range must agree with the row's own footprint:
     * address, page, length, bank policy and the selected instance
     * (rule 8's invalid ranges and selected-instance geometry). */
    if (entry->range.address != row->address || entry->range.page != row->page
        || entry->range.len != row->len
        || (!row->banked && entry->range.bank != 0)
        || (uint32_t)entry->range.offset + entry->range.len > 256) {
        return 0;
    }
    if (row->has_instances) {
        const openxcvr_instance_dimension_t *dim = &row->instances;
        if (dim->count == 0 || dim->offsets == NULL || dim->bit_offsets == NULL) {
            return 0;
        }
        if (entry->has_lane) {
            if (entry->lane < dim->first || entry->lane > dim->last) {
                return 0;
            }
            const size_t slot = (size_t)(entry->lane - dim->first);
            if (slot >= dim->count
                || (uint32_t)entry->range.offset
                    != (uint32_t)dim->offsets[slot]) {
                return 0;
            }
        } else if ((uint32_t)entry->range.offset != (uint32_t)dim->offsets[0]) {
            return 0;
        }
    } else if ((uint32_t)entry->range.offset != (uint32_t)row->offset) {
        return 0;
    }

    switch (row->presence_kind) {
    case OPENXCVR_PRESENCE_EQUALS: {
        const openxcvr_prerequisite_equals_t *prereq = &row->presence_equals;
        if (prereq->field == NULL || !plan_field_kind_known(prereq->kind)
            || !plan_loc_valid(&prereq->loc)) {
            return 0;
        }
        if (prereq_shifts_within_byte(prereq->kind) && prereq->bit_offset > 7) {
            return 0;
        }
        if (prereq->has_content_switch
            && !plan_loc_valid(&prereq->content_switch.data)) {
            return 0;
        }
        break;
    }
    case OPENXCVR_PRESENCE_UNSUPPORTED:
        if (row->presence_expr == NULL) {
            return 0;
        }
        break;
    default:
        break;
    }

    if (row->has_discriminator
        && (row->discriminator.field == NULL
            || !plan_loc_valid(&row->discriminator.loc))) {
        return 0;
    }
    switch (row->scaling.kind) {
    case OPENXCVR_SCALING_LINEAR:
        if (!plan_loc_valid(&row->scaling.linear.slope)
            || !plan_loc_valid(&row->scaling.linear.offset)) {
            return 0;
        }
        break;
    case OPENXCVR_SCALING_POLYNOMIAL:
        for (size_t i = 0; i < 5; ++i) {
            if (!plan_loc_valid(&row->scaling.poly.coefficients[i])) {
                return 0;
            }
        }
        break;
    default:
        break;
    }
    return 1;
}

/* Stage 1b shared entry validation, mirroring Rust validate_field_entries
 * where the C contract settles it: entries non-NULL exactly when the count
 * is positive, every row present, every entry resolved at exactly the
 * requested family and revision, and every entry structurally valid
 * against its row metadata (rule 8: unknown enum tags, required NULL
 * metadata strings, invalid ranges and geometry, inconsistent
 * address/page/length/bank policy and rows inapplicable at the requested
 * revision are plan-level INVALID_ARGUMENT; a known non-scalar
 * prerequisite kind stays structurally valid and refuses at admission
 * instead — ruling 3). Family/revision admission (the catalog table
 * lookup) stays with plan_table, as in Rust; the entry-count bound is
 * checked by both entry points before this traversal (ruling 4). */
static openxcvr_status validate_field_entries(
    const openxcvr_resolved_field_t *entries, size_t entry_count,
    openxcvr_spec_kind_t family, uint8_t revision_major, uint8_t revision_minor
) {
    if (entries == NULL) {
        return entry_count == 0 ? OPENXCVR_OK : OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    for (size_t i = 0; i < entry_count; ++i) {
        const openxcvr_resolved_field_t *entry = &entries[i];
        if (entry->row == NULL || entry->family != family
            || entry->revision_major != revision_major
            || entry->revision_minor != revision_minor
            || !plan_row_valid(entry)) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
    }
    return OPENXCVR_OK;
}

/* P-B3: an applicable row's effect footprint is the union of its own
 * span and every instance offset span (each instance occupies
 * row-len bytes at its own offset) — per-lane flags carry their read
 * effect at every lane's byte, not only at the row's declared offset,
 * so a range outside the row span but inside an instance span is
 * still effect-checked. */
static int row_effect_overlaps(const openxcvr_catalog_row_t *row, openxcvr_range range) {
    if ((uint32_t)row->offset < (uint32_t)range.offset + range.len
        && (uint32_t)range.offset < (uint32_t)row->offset + row->len) {
        return 1;
    }
    if (row->has_instances && row->instances.offsets != NULL) {
        for (size_t i = 0; i < row->instances.count; ++i) {
            const uint16_t off = row->instances.offsets[i];
            if ((uint32_t)off < (uint32_t)range.offset + range.len
                && (uint32_t)range.offset < (uint32_t)off + row->len) {
                return 1;
            }
        }
    }
    return 0;
}

/* The restrictive read effect any applicable catalog row overlapping the
 * range carries, as Rust restrictive_effect classifies it: rows are matched
 * by (address, page) and byte overlap against the row's effect footprint
 * (own span union instance spans, P-B3), deliberately not by bank (a banked
 * page's effect rows apply to the same offsets on every bank, so the check
 * is conservative, never permissive), and the first matching row in table
 * order wins with per-row precedence write-only before clear-on-read
 * before acknowledge-on-read. Sets *reason and returns 1 when restrictive. */
static int field_restrictive_effect(
    const openxcvr_family_catalog_t *table, uint8_t major, uint8_t minor,
    openxcvr_range range, openxcvr_field_plan_refusal_reason_t *reason
) {
    for (size_t i = 0; i < table->row_count; ++i) {
        const openxcvr_catalog_row_t *row = &table->rows[i];
        if (!row_applicable(row, major, minor)) {
            continue;
        }
        if (row->address != range.address || row->page != range.page) {
            continue;
        }
        if (!row_effect_overlaps(row, range)) {
            continue;
        }
        if ((row->read_effect_mask & OPENXCVR_READ_EFFECT_WRITE_ONLY) != 0) {
            *reason = OPENXCVR_FIELD_PLAN_WRITE_ONLY;
            return 1;
        }
        if ((row->read_effect_mask & OPENXCVR_READ_EFFECT_CLEAR_ON_READ) != 0) {
            *reason = OPENXCVR_FIELD_PLAN_CLEAR_ON_READ;
            return 1;
        }
        if ((row->read_effect_mask & OPENXCVR_READ_EFFECT_ACK_STARTUP) != 0) {
            *reason = OPENXCVR_FIELD_PLAN_ACKNOWLEDGE_ON_READ;
            return 1;
        }
    }
    return 0;
}

/* Whether *a sorts strictly before *b by unsigned (address, page, bank,
 * offset, len) — the coalescing sort key; equal keys compare 0 so a stable
 * sort preserves contributor insertion order. */
static int plan_read_before(
    const openxcvr_planned_read_t *a, const openxcvr_planned_read_t *b
) {
    if (a->range.address != b->range.address) {
        return a->range.address < b->range.address;
    }
    if (a->range.page != b->range.page) {
        return a->range.page < b->range.page;
    }
    if (a->range.bank != b->range.bank) {
        return a->range.bank < b->range.bank;
    }
    if (a->range.offset != b->range.offset) {
        return a->range.offset < b->range.offset;
    }
    return a->range.len < b->range.len;
}

/* Coalesce planned reads in place, exactly as Rust coalesce_reads: a
 * stable sort by (address, page, bank, offset, len), then a merge of
 * touching or overlapping ranges within one address/bank/page selection
 * (never across a gap), each merged read keeping its first contributor's
 * purpose. The output bound applies only after complete coalescing:
 * more than OPENXCVR_FIELD_PLAN_MAX_RANGES final reads is OUT_OF_RANGE,
 * not SHORT_BUFFER. On success *count is the final read count. */
static openxcvr_status coalesce_planned_reads(
    openxcvr_planned_read_t *reads, size_t *count
) {
    for (size_t i = 1; i < *count; ++i) {
        const openxcvr_planned_read_t key = reads[i];
        size_t j = i;
        while (j > 0 && plan_read_before(&key, &reads[j - 1])) {
            reads[j] = reads[j - 1];
            --j;
        }
        reads[j] = key;
    }
    size_t out = 0;
    for (size_t i = 0; i < *count; ++i) {
        const openxcvr_range range = reads[i].range;
        if (out > 0) {
            openxcvr_planned_read_t *last = &reads[out - 1];
            if (last->range.address == range.address && last->range.bank == range.bank
                && last->range.page == range.page
                && (uint32_t)range.offset
                    <= (uint32_t)last->range.offset + last->range.len) {
                const uint32_t last_end =
                    (uint32_t)last->range.offset + last->range.len;
                const uint32_t end = (uint32_t)range.offset + range.len;
                const uint32_t merged_end = last_end > end ? last_end : end;
                last->range.len =
                    (uint16_t)(merged_end - (uint32_t)last->range.offset);
                continue;
            }
        }
        reads[out++] = reads[i];
    }
    *count = out;
    if (out > OPENXCVR_FIELD_PLAN_MAX_RANGES) {
        return OPENXCVR_ERR_OUT_OF_RANGE;
    }
    return OPENXCVR_OK;
}

/* The family gate row both field-plan rounds read: the CMIS lower-memory
 * revision row, or the SFF compliance row — the same rows the basic plan
 * and the decoders resolve, never a second offset table. NULL is a
 * catalog defect (the callers fail closed). */
static const openxcvr_catalog_row_t *plan_family_gate(
    openxcvr_spec_kind_t family, const openxcvr_family_catalog_t *table,
    uint8_t major, uint8_t minor
) {
    switch (family) {
    case OPENXCVR_SPEC_CMIS:
        return find_row(
            table, "CmisLowerMemory", "cmis_revision", major, minor);
    case OPENXCVR_SPEC_SFF8636:
    case OPENXCVR_SPEC_SFF8472: {
        const char *page = NULL;
        const char *name = NULL;
        if (!compliance_field(family, &page, &name)) {
            return NULL;
        }
        return find_row(table, page, name, major, minor);
    }
    default:
        return NULL;
    }
}

/* Stage 1b C entry points, declared in observation.h: the prelude round
 * (gate plus Equals prerequisite ranges, effect-checked and coalesced)
 * and the remaining round (self-contained admission from the supplied
 * prelude bytes, with typed refusals, per-admitted-entry reads and
 * selector-acquisition requirements). */
openxcvr_status openxcvr_field_read_prelude(
    openxcvr_spec_kind_t family,
    uint8_t revision_major, uint8_t revision_minor,
    const openxcvr_resolved_field_t *entries, size_t entry_count,
    openxcvr_planned_read_t *reads, size_t read_capacity, size_t *read_count,
    openxcvr_field_plan_refusal_t *refusals,
    size_t refusal_capacity, size_t *refusal_count
) {
    const openxcvr_family_catalog_t *table = NULL;
    const struct monitor_names *monitors = NULL;
    openxcvr_status status =
        plan_table(family, revision_major, revision_minor, &table, &monitors);
    if (status != OPENXCVR_OK) {
        return status;
    }
    if (entries == NULL && entry_count != 0) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    /* Ruling 4: the entry-count bound precedes any traversal of the
     * entries (rule 3: oversize is OUT_OF_RANGE, never trimmed). */
    if (entry_count > OPENXCVR_FIELD_PLAN_MAX_ENTRIES) {
        return OPENXCVR_ERR_OUT_OF_RANGE;
    }
    if ((reads == NULL && read_capacity != 0)
        || (refusals == NULL && refusal_capacity != 0) || read_count == NULL
        || refusal_count == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    status = validate_field_entries(
        entries, entry_count, family, revision_major, revision_minor);
    if (status != OPENXCVR_OK) {
        return status;
    }

    /* Per-call fixed scratch, no allocation: entry_count is bounded above,
     * so candidates are at most entry_count + 1 (one prerequisite per
     * surviving entry plus the shared gate) and refusals at most one per
     * entry. Refusals are recorded per entry (rule 18) and published in
     * input-entry order across both passes, exactly as remaining
     * publishes its per-entry outcomes. */
    openxcvr_planned_read_t planned[OPENXCVR_FIELD_PLAN_MAX_ENTRIES + 1];
    openxcvr_field_plan_refusal_t entry_refusal[OPENXCVR_FIELD_PLAN_MAX_ENTRIES];
    unsigned char entry_refused[OPENXCVR_FIELD_PLAN_MAX_ENTRIES];
    size_t planned_count = 0;

    /* Pass 1 — each entry's Equals presence prerequisite (bank per the
     * prerequisite's own policy): an effect-tainted prerequisite refuses
     * its entry with no range; a clean one is planned. Unconditional and
     * Unsupported entries add nothing here. */
    for (size_t i = 0; i < entry_count; ++i) {
        const openxcvr_resolved_field_t *entry = &entries[i];
        entry_refused[i] = 0;
        if (entry->row->presence_kind != OPENXCVR_PRESENCE_EQUALS) {
            continue;
        }
        const openxcvr_prerequisite_equals_t *prereq = &entry->row->presence_equals;
        const openxcvr_range range = prereq_range(entry, prereq);
        openxcvr_field_plan_refusal_reason_t reason;
        if (field_restrictive_effect(
                table, revision_major, revision_minor, range, &reason)) {
            entry_refusal[i].field = entry->row->name;
            entry_refusal[i].has_lane = entry->has_lane;
            entry_refusal[i].lane = entry->lane;
            entry_refusal[i].reason = reason;
            entry_refused[i] = 1;
        } else {
            planned[planned_count++] = make_planned(range, prereq->field);
        }
    }

    /* Pass 2 — the family gate byte (CMIS lower-memory revision row; the
     * SFF compliance row), planned only while an entry survives; an
     * effectful gate refuses every surviving entry and removes their
     * contributions. */
    int any_survivor = 0;
    for (size_t i = 0; i < entry_count; ++i) {
        if (!entry_refused[i]) {
            any_survivor = 1;
            break;
        }
    }
    if (any_survivor) {
        const openxcvr_catalog_row_t *gate =
            plan_family_gate(family, table, revision_major, revision_minor);
        if (gate == NULL) {
            /* No gate row is a catalog defect: fail closed. */
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        if (!plan_row_range_valid(gate)) {
            /* An out-of-contract gate range is a catalog defect: fail
             * closed, exactly as Rust gate_range.validate() does. */
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        const openxcvr_range gate_range = row_range_bank0(gate);
        openxcvr_field_plan_refusal_reason_t reason;
        if (field_restrictive_effect(
                table, revision_major, revision_minor, gate_range, &reason)) {
            for (size_t i = 0; i < entry_count; ++i) {
                if (entry_refused[i]) {
                    continue;
                }
                entry_refusal[i].field = entries[i].row->name;
                entry_refusal[i].has_lane = entries[i].has_lane;
                entry_refusal[i].lane = entries[i].lane;
                entry_refusal[i].reason = reason;
                entry_refused[i] = 1;
            }
            planned_count = 0;
        } else {
            planned[planned_count++] = make_planned(gate_range, gate->name);
        }
    }

    status = coalesce_planned_reads(planned, &planned_count);
    if (status != OPENXCVR_OK) {
        return status;
    }
    size_t refusal_total = 0;
    for (size_t i = 0; i < entry_count; ++i) {
        if (entry_refused[i]) {
            refusal_total++;
        }
    }

    /* Transactional publication: the complete result exists before any
     * caller-visible write; a short non-NULL destination sets both counts
     * and writes no array, and a NULL array with capacity zero queries. */
    if ((reads != NULL && read_capacity < planned_count)
        || (refusals != NULL && refusal_capacity < refusal_total)) {
        *read_count = planned_count;
        *refusal_count = refusal_total;
        return OPENXCVR_ERR_SHORT_BUFFER;
    }
    if (reads != NULL) {
        memcpy(reads, planned, planned_count * sizeof(reads[0]));
    }
    if (refusals != NULL) {
        size_t written = 0;
        for (size_t i = 0; i < entry_count; ++i) {
            if (entry_refused[i]) {
                refusals[written++] = entry_refusal[i];
            }
        }
    }
    *read_count = planned_count;
    *refusal_count = refusal_total;
    return OPENXCVR_OK;
}

/* Bytes at a range assembled from whatever present observations supply
 * them — adjacent fragments jointly supply a multi-byte range, exactly
 * as Rust supplied_bytes assembles it (rule 9); supplied_at's single
 * covering observation is not enough here. Fills out[0 .. min(len, cap))
 * and returns 1 only when every byte of the range is present; any
 * missing byte returns 0. The view is already validated, so present
 * observations never overlap within one window and each byte has one
 * source. */
static int supplied_fragmented_bytes(
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_range range, uint8_t *out, size_t cap
) {
    const size_t fill = range.len < cap ? (size_t)range.len : cap;
    for (size_t k = 0; k < range.len; ++k) {
        const uint32_t at = (uint32_t)range.offset + (uint32_t)k;
        const uint8_t *byte = NULL;
        for (size_t i = 0; i < count; ++i) {
            const openxcvr_range seen = ranges[i].range;
            if (!ranges[i].present || seen.address != range.address
                || seen.bank != range.bank || seen.page != range.page) {
                continue;
            }
            if ((uint32_t)seen.offset <= at
                && at < (uint32_t)seen.offset + seen.len) {
                byte = ranges[i].bytes + (size_t)(at - seen.offset);
                break;
            }
        }
        if (byte == NULL) {
            return 0;
        }
        if (k < fill) {
            out[k] = *byte;
        }
    }
    return 1;
}

/* Push one raw read candidate into the fixed per-call scratch. Rule 19's
 * bound covers every contributor by construction; the guard fails
 * closed rather than writing past the scratch. */
static int plan_candidate_push(
    openxcvr_planned_read_t *planned, size_t *count, size_t cap,
    openxcvr_range range, const char *purpose
) {
    if (*count >= cap) {
        return 0;
    }
    planned[(*count)++] = make_planned(range, purpose);
    return 1;
}

/* A refusal record for one entry; the caller fills the reason's detail. */
static openxcvr_field_admission_refusal_t make_admission_refusal(
    const openxcvr_resolved_field_t *entry,
    openxcvr_field_admission_reason_t reason
) {
    openxcvr_field_admission_refusal_t refusal;
    memset(&refusal, 0, sizeof(refusal));
    refusal.field = entry->row->name;
    refusal.has_lane = entry->has_lane;
    refusal.lane = entry->lane;
    refusal.reason = reason;
    return refusal;
}

/* A metadata location's range at one bank; the uint16 offset is
 * validated (rule 8) before this conversion to the uint8 range offset. */
static openxcvr_range loc_range_bank(const openxcvr_field_loc_t *loc, uint8_t bank) {
    return (openxcvr_range){
        loc->address, bank, loc->page, (uint8_t)loc->offset, loc->len
    };
}

/* One admitted entry's remaining decode dependencies (rule 15), in Rust
 * order (rule 17): the variant discriminator, then the declared external
 * linear constants or polynomial coefficients — planned from metadata
 * for every admitted row of those kinds, never conditionally on a
 * discriminator byte (ruling 2). At most six ranges: one discriminator
 * plus five coefficients. */
static size_t entry_remaining_dependencies(
    const openxcvr_catalog_row_t *row, uint8_t bank,
    openxcvr_range needs[6], const char *purposes[6]
) {
    size_t count = 0;
    if (row->has_discriminator) {
        needs[count] = loc_range_bank(&row->discriminator.loc, bank);
        purposes[count++] = row->discriminator.field;
    }
    switch (row->scaling.kind) {
    case OPENXCVR_SCALING_LINEAR:
        needs[count] = loc_range_bank(&row->scaling.linear.slope, bank);
        purposes[count++] = row->name;
        needs[count] = loc_range_bank(&row->scaling.linear.offset, bank);
        purposes[count++] = row->name;
        break;
    case OPENXCVR_SCALING_POLYNOMIAL:
        for (size_t c = 0; c < 5; ++c) {
            needs[count] = loc_range_bank(&row->scaling.poly.coefficients[c], bank);
            purposes[count++] = row->name;
        }
        break;
    default:
        break;
    }
    return count;
}

/* Rule 19 raw-candidate bound for the remaining round: own range plus
 * discriminator plus at most five polynomial coefficients per entry, plus
 * the one shared advertisement. */
#define FIELD_REMAINING_MAX_CANDIDATES (7 * OPENXCVR_FIELD_PLAN_MAX_ENTRIES + 1)

/* Per-entry outcome scratch. A refused entry never carries a requirement
 * (one exists only while the entry is admitted, and refusing it drops the
 * requirement — including the shared-advertisement refusals), so the two
 * records share one slot: the lean per-entry storage rule 19 bounds. */
typedef union entry_outcome {
    openxcvr_field_admission_refusal_t refusal;
    openxcvr_acquisition_requirement_t requirement;
} entry_outcome_t;

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
) {
    const openxcvr_family_catalog_t *table = NULL;
    const struct monitor_names *monitors = NULL;
    openxcvr_status status =
        plan_table(family, revision_major, revision_minor, &table, &monitors);
    if (status != OPENXCVR_OK) {
        return status;
    }
    if (entries == NULL && entry_count != 0) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    /* Ruling 4: the entry-count bound precedes any traversal of the
     * entries (rule 3: oversize is OUT_OF_RANGE, never trimmed). */
    if (entry_count > OPENXCVR_FIELD_PLAN_MAX_ENTRIES) {
        return OPENXCVR_ERR_OUT_OF_RANGE;
    }
    if ((reads == NULL && read_capacity != 0)
        || (refusals == NULL && refusal_capacity != 0)
        || (acquisition_required == NULL && acquisition_capacity != 0)
        || read_count == NULL || refusal_count == NULL
        || acquisition_count == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    status = validate_field_entries(
        entries, entry_count, family, revision_major, revision_minor);
    if (status != OPENXCVR_OK) {
        return status;
    }
    /* Rule 9: the view is required (non-NULL even when empty, matching
     * basic_read_remaining and observe_field_ctx) and fully validated —
     * including windows no entry reads — before anything is published. */
    if (ranges == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    status = validate_view(ranges, range_count);
    if (status != OPENXCVR_OK) {
        return status;
    }

    /* Per-call fixed automatic scratch (rule 19): raw candidates
     * coalesced in place, no duplicate candidate arrays; per-entry outcome
     * storage and admission state, each bounded by the entry bound. No
     * dynamic allocation, VLA or global mutable buffer. */
    openxcvr_planned_read_t planned[FIELD_REMAINING_MAX_CANDIDATES];
    entry_outcome_t outcome[OPENXCVR_FIELD_PLAN_MAX_ENTRIES];
    unsigned char entry_refused[OPENXCVR_FIELD_PLAN_MAX_ENTRIES];
    unsigned char entry_required[OPENXCVR_FIELD_PLAN_MAX_ENTRIES];
    size_t planned_count = 0;

    for (size_t i = 0; i < entry_count; ++i) {
        entry_refused[i] = 0;
        entry_required[i] = 0;
    }

    /* Rule 12, the gate first: the family gate byte decides for every
     * entry before any per-entry rule — a restrictive effect on its range,
     * a missing byte, a CMIS byte 1 naming another or unknown revision,
     * or an SFF compliance code that does not admit a decode at the schema
     * revision. These are per-entry refusals in a result record (rule 13),
     * never the function-level statuses basic_read_remaining returns. */
    if (entry_count != 0) {
        const openxcvr_catalog_row_t *gate =
            plan_family_gate(family, table, revision_major, revision_minor);
        if (gate == NULL) {
            /* No gate row is a catalog defect: fail closed. */
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        if (!plan_row_range_valid(gate)) {
            /* An out-of-contract gate range is a catalog defect: fail
             * closed, exactly as Rust gate_range.validate() does. */
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        const openxcvr_range gate_range = row_range_bank0(gate);
        openxcvr_field_plan_refusal_reason_t reason;
        if (field_restrictive_effect(
                table, revision_major, revision_minor, gate_range, &reason)) {
            for (size_t i = 0; i < entry_count; ++i) {
                outcome[i].refusal = make_admission_refusal(
                    &entries[i], OPENXCVR_FIELD_ADMISSION_EFFECT);
                outcome[i].refusal.detail.effect = reason;
                entry_refused[i] = 1;
            }
        } else {
            uint8_t gate_bytes[8];
            if (!supplied_fragmented_bytes(
                    ranges, range_count, gate_range, gate_bytes,
                    sizeof gate_bytes)) {
                for (size_t i = 0; i < entry_count; ++i) {
                    outcome[i].refusal = make_admission_refusal(
                        &entries[i], OPENXCVR_FIELD_ADMISSION_PRELUDE_MISSING);
                    outcome[i].refusal.detail.prelude_missing = gate_range;
                    entry_refused[i] = 1;
                }
            } else {
                const uint8_t gate_byte = gate_bytes[0];
                const int admits = family == OPENXCVR_SPEC_CMIS
                    ? cmis_revision_admits(
                          gate_byte, revision_major, revision_minor)
                    : compliance_admits(
                          family, gate_byte, revision_major, revision_minor);
                if (!admits) {
                    for (size_t i = 0; i < entry_count; ++i) {
                        outcome[i].refusal = make_admission_refusal(
                            &entries[i],
                            family == OPENXCVR_SPEC_CMIS
                                ? OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_REVISION
                                : OPENXCVR_FIELD_ADMISSION_COMPLIANCE_REFUSED);
                        if (family == OPENXCVR_SPEC_CMIS) {
                            /* Rule 13: the byte still names what it reports,
                             * known or not. */
                            outcome[i].refusal.detail.unsupported_revision.family =
                                OPENXCVR_SPEC_CMIS;
                            outcome[i].refusal.detail.unsupported_revision.major =
                                (uint8_t)(gate_byte >> 4);
                            outcome[i].refusal.detail.unsupported_revision.minor =
                                (uint8_t)(gate_byte & 0x0f);
                        } else {
                            outcome[i].refusal.detail.compliance_code = gate_byte;
                        }
                        entry_refused[i] = 1;
                    }
                }
            }
        }
    }

    /* The shared CMIS TxBiasCurrentScalingFactor advertisement (rule 12,
     * shared dependency effect), resolved lazily on the first admitted
     * dependent — a refused decode needs no advertisement — and reused for
     * every later one. State: 0 unresolved, 1 clean, 2 effectful. */
    const openxcvr_catalog_row_t *advertisement = NULL;
    openxcvr_field_plan_refusal_reason_t advertisement_reason;
    int advertisement_state = 0;

    /* Rules 2-4 of the rule 12 precedence, per entry in input order; the
     * first matching rule wins. */
    for (size_t i = 0; i < entry_count; ++i) {
        if (entry_refused[i]) {
            continue;
        }
        const openxcvr_resolved_field_t *entry = &entries[i];
        const openxcvr_catalog_row_t *row = entry->row;

        /* UnsupportedPresence. */
        if (row->presence_kind == OPENXCVR_PRESENCE_UNSUPPORTED) {
            outcome[i].refusal = make_admission_refusal(
                entry, OPENXCVR_FIELD_ADMISSION_UNSUPPORTED_PRESENCE);
            outcome[i].refusal.detail.unsupported_presence_expr =
                row->presence_expr;
            entry_refused[i] = 1;
            continue;
        }

        /* The Equals prerequisite the prelude round acquired: its range is
         * effect-checked before its bytes are read, its raw code is
         * extracted exactly as the core decoder extracts it (rule 14), and
         * failed extraction is a typed undecodable refusal, never a
         * fabricated actual. */
        if (row->presence_kind == OPENXCVR_PRESENCE_EQUALS) {
            const openxcvr_prerequisite_equals_t *prereq = &row->presence_equals;
            const openxcvr_range prereq_loc = prereq_range(entry, prereq);
            openxcvr_field_plan_refusal_reason_t reason;
            if (field_restrictive_effect(
                    table, revision_major, revision_minor, prereq_loc, &reason)) {
                outcome[i].refusal =
                    make_admission_refusal(entry, OPENXCVR_FIELD_ADMISSION_EFFECT);
                outcome[i].refusal.detail.effect = reason;
                entry_refused[i] = 1;
                continue;
            }
            uint8_t prereq_bytes[8];
            if (!supplied_fragmented_bytes(
                    ranges, range_count, prereq_loc, prereq_bytes,
                    sizeof prereq_bytes)) {
                outcome[i].refusal = make_admission_refusal(
                    entry, OPENXCVR_FIELD_ADMISSION_PRELUDE_MISSING);
                outcome[i].refusal.detail.prelude_missing = prereq_loc;
                entry_refused[i] = 1;
                continue;
            }
            uint64_t raw = 0;
            if (!prereq_raw(prereq, prereq_bytes, &raw)) {
                outcome[i].refusal = make_admission_refusal(
                    entry, OPENXCVR_FIELD_ADMISSION_PREREQUISITE_UNDECODABLE);
                outcome[i].refusal.detail.prerequisite_undecodable.prerequisite =
                    prereq->field;
                entry_refused[i] = 1;
                continue;
            }
            if (raw != prereq->expected) {
                outcome[i].refusal = make_admission_refusal(
                    entry,
                    prereq->has_content_switch
                        ? OPENXCVR_FIELD_ADMISSION_VIEW_NOT_SELECTED
                        : OPENXCVR_FIELD_ADMISSION_PREREQUISITE_NOT_SATISFIED);
                if (prereq->has_content_switch) {
                    outcome[i].refusal.detail.view_not_selected.selector =
                        prereq->field;
                    outcome[i].refusal.detail.view_not_selected.expected =
                        prereq->expected;
                    outcome[i].refusal.detail.view_not_selected.actual = raw;
                } else {
                    outcome[i].refusal.detail.prerequisite_not_satisfied.prerequisite =
                        prereq->field;
                    outcome[i].refusal.detail.prerequisite_not_satisfied.expected =
                        prereq->expected;
                    outcome[i].refusal.detail.prerequisite_not_satisfied.actual = raw;
                }
                entry_refused[i] = 1;
                continue;
            }
        }

        /* Own-data effect, then the remaining decode dependencies, each
         * effect-checked: a tainted one refuses the entry and drops
         * everything it contributed in this iteration. */
        const size_t entry_start = planned_count;
        openxcvr_field_plan_refusal_reason_t reason;
        if (field_restrictive_effect(
                table, revision_major, revision_minor, entry->range, &reason)) {
            outcome[i].refusal =
                make_admission_refusal(entry, OPENXCVR_FIELD_ADMISSION_EFFECT);
            outcome[i].refusal.detail.effect = reason;
            entry_refused[i] = 1;
            continue;
        }
        if (!plan_candidate_push(
                planned, &planned_count, FIELD_REMAINING_MAX_CANDIDATES,
                entry->range, row->name)) {
            return OPENXCVR_ERR_OUT_OF_RANGE;
        }
        openxcvr_range needs[6];
        const char *purposes[6];
        const size_t need_count =
            entry_remaining_dependencies(row, entry->range.bank, needs, purposes);
        int effect_refused = 0;
        for (size_t d = 0; d < need_count; ++d) {
            if (field_restrictive_effect(
                    table, revision_major, revision_minor, needs[d], &reason)) {
                effect_refused = 1;
                break;
            }
            if (!plan_candidate_push(
                    planned, &planned_count, FIELD_REMAINING_MAX_CANDIDATES,
                    needs[d], purposes[d])) {
                return OPENXCVR_ERR_OUT_OF_RANGE;
            }
        }
        if (effect_refused) {
            outcome[i].refusal =
                make_admission_refusal(entry, OPENXCVR_FIELD_ADMISSION_EFFECT);
            outcome[i].refusal.detail.effect = reason;
            entry_refused[i] = 1;
            planned_count = entry_start;
            continue;
        }

        /* The shared advertisement, for admitted laser-bias-scaled entries
         * only (rule 12's shared-dependency effect, at its entry position
         * in the refusal order — rule 18). */
        if (family == OPENXCVR_SPEC_CMIS
            && row_scaling_needs_bias_advertisement(row)) {
            if (advertisement_state == 0) {
                advertisement = find_row(
                    &openxcvr_cmis_catalog, "CmisPage01hAdvertising",
                    "tx_bias_current_scaling_factor", revision_major,
                    revision_minor);
                if (advertisement == NULL) {
                    /* No advertisement row is a catalog defect: fail closed. */
                    return OPENXCVR_ERR_INVALID_ARGUMENT;
                }
                if (!plan_row_range_valid(advertisement)) {
                    /* An out-of-contract advertisement range is a catalog
                     * defect: fail closed, exactly as Rust
                     * advertisement_range.validate() does. */
                    return OPENXCVR_ERR_INVALID_ARGUMENT;
                }
                advertisement_state = field_restrictive_effect(
                    table, revision_major, revision_minor,
                    row_range_bank0(advertisement), &advertisement_reason)
                    ? 2
                    : 1;
            }
            if (advertisement_state == 2) {
                outcome[i].refusal = make_admission_refusal(
                    entry, OPENXCVR_FIELD_ADMISSION_EFFECT);
                outcome[i].refusal.detail.effect = advertisement_reason;
                entry_refused[i] = 1;
                planned_count = entry_start;
                continue;
            }
        }

        /* Admitted (rule 18): a content-multiplexed entry additionally
         * requires selector-acquisition evidence with these coordinates —
         * a requirement, not authorization. */
        if (row->presence_kind == OPENXCVR_PRESENCE_EQUALS
            && row->presence_equals.has_content_switch) {
            const openxcvr_prerequisite_equals_t *prereq =
                &row->presence_equals;
            const uint8_t bank = prereq->banked ? entry->range.bank : 0;
            memset(&outcome[i].requirement, 0, sizeof(outcome[i].requirement));
            outcome[i].requirement.field = row->name;
            outcome[i].requirement.has_lane = entry->has_lane;
            outcome[i].requirement.lane = entry->lane;
            outcome[i].requirement.selector_range = prereq_range(entry, prereq);
            outcome[i].requirement.selector_value = prereq->expected;
            outcome[i].requirement.data_range =
                loc_range_bank(&prereq->content_switch.data, bank);
            entry_required[i] = 1;
        }
    }

    /* The one shared advertisement, after every per-entry contribution
     * (rule 17 insertion order), when a dependent stayed admitted. */
    if (advertisement_state == 1
        && !plan_candidate_push(
               planned, &planned_count, FIELD_REMAINING_MAX_CANDIDATES,
               row_range_bank0(advertisement), advertisement->name)) {
        return OPENXCVR_ERR_OUT_OF_RANGE;
    }

    /* Coalesce in place; the 256-range output bound applies only after
     * complete coalescing, and more final reads is intrinsic OUT_OF_RANGE,
     * not SHORT_BUFFER (rule 7) — with no output touched. */
    status = coalesce_planned_reads(planned, &planned_count);
    if (status != OPENXCVR_OK) {
        return status;
    }

    size_t refusal_total = 0;
    size_t requirement_total = 0;
    for (size_t i = 0; i < entry_count; ++i) {
        if (entry_refused[i]) {
            refusal_total++;
        } else if (entry_required[i]) {
            requirement_total++;
        }
    }

    /* Rules 5-7, transactional publication over the three outputs: the
     * complete result exists before any caller-visible write; on any short
     * non-NULL destination every count is set and no array is written; a
     * NULL array with capacity zero queries that output independently. */
    if ((reads != NULL && read_capacity < planned_count)
        || (refusals != NULL && refusal_capacity < refusal_total)
        || (acquisition_required != NULL
            && acquisition_capacity < requirement_total)) {
        *read_count = planned_count;
        *refusal_count = refusal_total;
        *acquisition_count = requirement_total;
        return OPENXCVR_ERR_SHORT_BUFFER;
    }
    if (reads != NULL) {
        memcpy(reads, planned, planned_count * sizeof(reads[0]));
    }
    if (refusals != NULL) {
        size_t written = 0;
        for (size_t i = 0; i < entry_count; ++i) {
            if (entry_refused[i]) {
                refusals[written++] = outcome[i].refusal;
            }
        }
    }
    if (acquisition_required != NULL) {
        size_t written = 0;
        for (size_t i = 0; i < entry_count; ++i) {
            if (entry_required[i]) {
                acquisition_required[written++] = outcome[i].requirement;
            }
        }
    }
    *read_count = planned_count;
    *refusal_count = refusal_total;
    *acquisition_count = requirement_total;
    return OPENXCVR_OK;
}
