/*
 * Stage 1 supplied-data admission parity (C): Page 14h selector-content
 * gating and CMIS reported-revision admission through the generic
 * openxcvr_observe_field path. Mirrors crates/openxcvr/tests/
 * page14h_admission.rs.
 *
 * Byte sources, labelled: "credo capture" — the real capture fixture
 * tests/fixtures/cmis/fboss-cmis-5-1-osfp-credo-800g-aec (loaded from the
 * repo root; this test proves admission behavior, not acquisition
 * provenance); "synthetic" — bytes built here, never claimed to be a
 * capture.
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

static unsigned char lower_buf[128];
static unsigned char page14_buf[128];
static size_t lower_len = 0;
static size_t page14_len = 0;

static void load_fixture(const char *rel, unsigned char *buf, size_t *len) {
    FILE *f = fopen(rel, "rb");
    CHECK(f != NULL);
    *len = fread(buf, 1, 128, f);
    CHECK(*len == 128);
    fclose(f);
}

typedef struct {
    openxcvr_range_observation obs[8];
    size_t count;
} parts_t;

static void add_part(
    parts_t *parts, uint8_t address, uint8_t bank, uint8_t page, uint8_t offset,
    const uint8_t *bytes, size_t len
) {
    openxcvr_range_observation *o = &parts->obs[parts->count++];
    o->range = (openxcvr_range){address, bank, page, offset, (uint16_t)len};
    o->bytes = bytes;
    o->len = len;
    o->present = 1;
}

/* Resolve one Page 14h data field (all lanes) at an arbitrary revision. */
static size_t resolve_page14h_at(
    const char *name, uint8_t major, uint8_t minor,
    openxcvr_resolved_field_t *out, size_t capacity
) {
    openxcvr_field_query_t query = {NULL, OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    query.pattern = name;
    query.lane = OPENXCVR_LANE_ALL;
    query.page = "14h";
    size_t count = 0;
    const openxcvr_status status =
        openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, major, minor, &query, out, capacity, &count);
    CHECK(status == OPENXCVR_OK);
    return count;
}

/* Resolve one Page 14h data field (all lanes) at CMIS 5.1. */
static size_t resolve_page14h(
    const char *name, openxcvr_resolved_field_t *out, size_t capacity
) {
    return resolve_page14h_at(name, 5, 1, out, capacity);
}

/* ─── credo capture: selector 06h, no acquisition evidence ─────────────── */

static void credo_without_evidence(void) {
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
    add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
    /* The capture's own facts: 5.1 revision byte and SNR-view selector. */
    CHECK(lower_buf[1] == 0x51);
    CHECK(page14_buf[0] == 0x06);

    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);
    for (size_t i = 0; i < count; ++i) {
        openxcvr_observation_t obs;
        const openxcvr_status status =
            openxcvr_observe_field(&entries[i], parts.obs, parts.count, NULL, &obs);
        CHECK(status == OPENXCVR_OK);
        /* Missing selector-acquisition evidence comes first: NOT_SUPPLIED,
         * never a Valid value out of a view the selector did not select. */
        CHECK(obs.state == OPENXCVR_OBS_NOT_SUPPLIED);
        CHECK(obs.has_value == 0);
    }

    /* The SNR rows the selector does select are NotSupplied for the same
     * reason (no evidence), and Valid only with matching evidence — that
     * half is exercised with the acquisition context in this suite. */
    const size_t snr_count = resolve_page14h("host_side_snr", entries, 8);
    CHECK(snr_count == 8);
    for (size_t i = 0; i < snr_count; ++i) {
        openxcvr_observation_t obs;
        const openxcvr_status status =
            openxcvr_observe_field(&entries[i], parts.obs, parts.count, NULL, &obs);
        CHECK(status == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_NOT_SUPPLIED);
        CHECK(obs.has_value == 0);
    }
}

/* ─── revision-byte admission ───────────────────────────────────────────── */

static void revision_byte_admission(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);

    /* Page 14h supplied, lower memory absent: NOT_SUPPLIED, no value. */
    {
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
        for (size_t i = 0; i < count; ++i) {
            openxcvr_observation_t obs;
            CHECK(openxcvr_observe_field(&entries[i], parts.obs, parts.count, NULL, &obs)
                == OPENXCVR_OK);
            CHECK(obs.state == OPENXCVR_OBS_NOT_SUPPLIED);
            CHECK(obs.has_value == 0);
        }
    }

    /* Revision byte 0x30 (reports 3.0) with 5.1 entries:
     * UNSUPPORTED_REVISION, no value. */
    {
        unsigned char lower30[128];
        memcpy(lower30, lower_buf, sizeof(lower30));
        lower30[1] = 0x30;
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, lower30, sizeof(lower30));
        add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
        for (size_t i = 0; i < count; ++i) {
            openxcvr_observation_t obs;
            CHECK(openxcvr_observe_field(&entries[i], parts.obs, parts.count, NULL, &obs)
                == OPENXCVR_OK);
            CHECK(obs.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
            CHECK(obs.has_value == 0);
        }
    }

    /* Unknown revision byte 0xff: UNSUPPORTED_REVISION, no value. */
    {
        unsigned char lowerff[128];
        memcpy(lowerff, lower_buf, sizeof(lowerff));
        lowerff[1] = 0xff;
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, lowerff, sizeof(lowerff));
        add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
        for (size_t i = 0; i < count; ++i) {
            openxcvr_observation_t obs;
            CHECK(openxcvr_observe_field(&entries[i], parts.obs, parts.count, NULL, &obs)
                == OPENXCVR_OK);
            CHECK(obs.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
            CHECK(obs.has_value == 0);
        }
    }
}

/* ─── Page 14h floor: resolution at CMIS 3.0 refuses ───────────────────── */

static void resolution_refuses_at_cmis_3_0(void) {
    openxcvr_field_query_t query = {NULL, OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    query.pattern = "media_side_ber";
    query.lane = OPENXCVR_LANE_ALL;
    query.page = "14h";
    openxcvr_resolved_field_t out[8];
    size_t count = 99;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 3, 0, &query, out, 8, &count)
        == OPENXCVR_ERR_NO_MATCH);
}

/* ─── selector-acquisition evidence (context-taking path) ───────────────── */

static openxcvr_selector_acquisition_t page14h_acquisition(
    uint8_t selector_value, uint8_t data_offset, uint16_t data_len
) {
    openxcvr_selector_acquisition_t acquisition;
    memset(&acquisition, 0, sizeof(acquisition));
    acquisition.generation = 1;
    acquisition.epoch = 2;
    acquisition.address = 0x50;
    acquisition.bank = 0;
    acquisition.page = 0x14;
    acquisition.selector_offset = 128;
    acquisition.selector_value = selector_value;
    acquisition.data_range =
        (openxcvr_range){0x50, 0, 0x14, data_offset, data_len};
    return acquisition;
}

/* Credo capture, selector 06h, evidence matching the snapshot and covering
 * bytes 192-255: the SNR rows the selector selects decode Valid with the
 * capture's lane-1 value (8.97265625 dB); the BER rows are Invalid. */
static void credo_with_matching_evidence(void) {
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
    add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
    openxcvr_selector_acquisition_t evidence[1] = {page14h_acquisition(0x06, 192, 64)};
    const openxcvr_acquisition_context_t ctx = {1, evidence};

    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("host_side_snr", entries, 8);
    CHECK(count == 8);
    for (size_t i = 0; i < count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_VALID);
        CHECK(obs.has_value == 1 && obs.value.is_scaled == 1);
        CHECK(obs.value.unit == OPENXCVR_UNIT_DECIBEL);
    }
    /* Lane 1: 0x08f9 little-endian in 1/256 dB = 8.97265625. */
    {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[0], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_OK);
        CHECK(obs.value.value > 8.9726562 && obs.value.value < 8.9726563);
    }

    const size_t ber_count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(ber_count == 8);
    for (size_t i = 0; i < ber_count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_INVALID);
        CHECK(obs.has_value == 0);
    }
}

/* Evidence claiming 01h on a snapshot whose selector byte is 06h:
 * contradiction - Invalid, no value. */
static void credo_with_contradictory_evidence(void) {
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
    add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
    openxcvr_selector_acquisition_t evidence[1] = {page14h_acquisition(0x01, 192, 64)};
    const openxcvr_acquisition_context_t ctx = {1, evidence};

    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("host_side_snr", entries, 8);
    for (size_t i = 0; i < count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_INVALID);
        CHECK(obs.has_value == 0);
        CHECK(obs.note != NULL && strcmp(obs.note, "selector evidence contradicts snapshot") == 0);
    }
}

/* Synthetic bytes (the credo page-14h window with the selector forced to
 * 01h - labelled synthetic: these are the capture's SNR-view bytes forced
 * through the BER view to exercise admission, not BER measurements): BER
 * rows decode Valid F16 ratio values with matching evidence; non-selecting
 * selector values (00h, 07h, 16h, C0h) are Invalid with matching evidence. */
static void synthetic_selector_evidence(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);

    unsigned char page14_01[128];
    memcpy(page14_01, page14_buf, sizeof(page14_01));
    page14_01[0] = 0x01;
    {
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
        add_part(&parts, 0x50, 0, 0x14, 128, page14_01, sizeof(page14_01));
        openxcvr_selector_acquisition_t evidence[1] = {page14h_acquisition(0x01, 192, 64)};
        const openxcvr_acquisition_context_t ctx = {1, evidence};
        const uint64_t codes[8] = {63752, 47624, 7944, 35080, 63496, 521, 18697, 47624};
        for (size_t i = 0; i < count; ++i) {
            openxcvr_observation_t obs;
            CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
                == OPENXCVR_OK);
            CHECK(obs.state == OPENXCVR_OBS_VALID);
            CHECK(obs.has_value == 1 && obs.value.is_scaled == 1);
            CHECK(obs.value.unit == OPENXCVR_UNIT_RATIO);
            CHECK(obs.value.value == openxcvr_cmis_f16_to_double((uint16_t)codes[i]));
            /* The 16-bit F16 code rides along with the scaled value. */
            CHECK(obs.value.raw == codes[i]);
        }
    }

    const uint8_t non_selecting[4] = {0x00, 0x07, 0x16, 0xc0};
    for (size_t sel = 0; sel < 4; ++sel) {
        unsigned char page14_sel[128];
        memcpy(page14_sel, page14_buf, sizeof(page14_sel));
        page14_sel[0] = non_selecting[sel];
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
        add_part(&parts, 0x50, 0, 0x14, 128, page14_sel, sizeof(page14_sel));
        openxcvr_selector_acquisition_t evidence[1] = {
            page14h_acquisition(non_selecting[sel], 192, 64)
        };
        const openxcvr_acquisition_context_t ctx = {1, evidence};
        for (size_t i = 0; i < count; ++i) {
            openxcvr_observation_t obs;
            CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
                == OPENXCVR_OK);
            CHECK(obs.state == OPENXCVR_OBS_INVALID);
            CHECK(obs.has_value == 0);
        }
    }
}

/* The evidence's data range must cover the row's own bytes: an assertion
 * covering only bytes 192-207 admits host_side_ber and leaves
 * media_side_ber (208-223) NotSupplied. */
static void evidence_range_containment(void) {
    unsigned char page14_01[128];
    memcpy(page14_01, page14_buf, sizeof(page14_01));
    page14_01[0] = 0x01;
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
    add_part(&parts, 0x50, 0, 0x14, 128, page14_01, sizeof(page14_01));
    openxcvr_selector_acquisition_t evidence[1] = {page14h_acquisition(0x01, 192, 16)};
    const openxcvr_acquisition_context_t ctx = {1, evidence};

    openxcvr_resolved_field_t entries[8];
    const size_t host_count = resolve_page14h("host_side_ber", entries, 8);
    for (size_t i = 0; i < host_count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_VALID);
    }
    const size_t media_count = resolve_page14h("media_side_ber", entries, 8);
    for (size_t i = 0; i < media_count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_NOT_SUPPLIED);
        CHECK(obs.has_value == 0);
    }
}

/* ─── malformed hand-built entry (synthetic) ───────────────────────── */

/* A 16-bit prerequisite whose declared length is one byte: reachable only
 * through a hand-built entry, never the generated catalog (16-bit kinds
 * always carry len 2). The decode must refuse — INVALID with the
 * decode-refusal note — rather than read past the one supplied byte. */
static void short_sixteen_bit_prerequisite_refuses(void) {
    /* Exactly the declared one byte: a two-byte read would go past it. */
    uint8_t selector_byte[1] = {0x06};
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
    add_part(&parts, 0x50, 0, 0x14, 128, selector_byte, sizeof(selector_byte));

    openxcvr_catalog_row_t row = {0};
    row.name = "synthetic_gated";
    row.presence_kind = OPENXCVR_PRESENCE_EQUALS;
    row.presence_equals.field = "selector";
    row.presence_equals.loc.address = 0x50;
    row.presence_equals.loc.page = 0x14;
    row.presence_equals.loc.offset = 128;
    row.presence_equals.loc.len = 1;
    row.presence_equals.kind = OPENXCVR_FIELD_U16BE;
    row.presence_equals.bit_width = 16;
    row.presence_equals.expected = 0xdead;

    openxcvr_resolved_field_t entry = {0};
    entry.row = &row;
    entry.range = (openxcvr_range){0x50, 0, 0x14, 192, 2};
    entry.family = OPENXCVR_SPEC_CMIS;
    entry.revision_major = 5;
    entry.revision_minor = 1;

    openxcvr_observation_t obs;
    CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, NULL, &obs) == OPENXCVR_OK);
    CHECK(obs.state == OPENXCVR_OBS_INVALID);
    CHECK(obs.has_value == 0);
    CHECK(obs.note != NULL && strcmp(obs.note, "presence prerequisite could not be decoded") == 0);
}

/* A non-NULL context that claims one acquisition but supplies none is
 * invalid input — refused as INVALID_ARGUMENT, never dereferenced. */
static void context_without_acquisitions_is_rejected(void) {
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, lower_buf, lower_len);
    add_part(&parts, 0x50, 0, 0x14, 128, page14_buf, page14_len);
    const openxcvr_acquisition_context_t ctx = {1, NULL};

    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);
    for (size_t i = 0; i < count; ++i) {
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field_ctx(&entries[i], parts.obs, parts.count, NULL, &ctx, &obs)
            == OPENXCVR_ERR_INVALID_ARGUMENT);
    }
}

/* ─── Stage 2: interpreted F16 BER values (ratio) ─────────────── */

/* The eight lane codes the synthetic BER views carry (lanes 1..8) and the
 * expected binary64 bit patterns - the slice-1 goldens, transcribed
 * independently of any decoder run (0x0000 and 0xF800 are 0.0, 0xC001 is
 * 1.0, 0xC00A is 10.0, 0xFFFF is 2.047e10 - a value above 1 stays a
 * ratio, never a clamped BER). Synthetic pages only. */
static const uint16_t f16_lane_codes[8] = {
    0x0000, 0xC001, 0xFFFF, 0x6001, 0x0801, 0x07FF, 0xF800, 0xC00A
};
static const uint64_t f16_lane_bits[8] = {
    0x0000000000000000ull, 0x3FF0000000000000ull, 0x4213106da6000000ull,
    0x3d719799812dea11ull, 0x3b282db34012b251ull, 0x3ba35557a1555996ull,
    0x0000000000000000ull, 0x4024000000000000ull
};

/* (field, selecting selector): ungated rows select on 01h, gated rows on
 * 11h - the selectors the schemas' present_when names. */
static const struct {
    const char *name;
    uint8_t selector;
} ber_fields[4] = {
    {"host_side_ber", 0x01},
    {"media_side_ber", 0x01},
    {"gated_host_side_ber", 0x11},
    {"gated_media_side_ber", 0x11},
};

static unsigned char synth_lower[128];
static unsigned char synth_page14[128];

/* Synthetic lower page (bytes 0-127) whose byte 1 names the revision. */
static void build_synthetic_lower(uint8_t revision_byte) {
    memset(synth_lower, 0, sizeof(synth_lower));
    synth_lower[1] = revision_byte;
}

/* Synthetic Page 14h window (bytes 128-255): the selector at byte 128 and
 * the eight lane codes in both BER views (host 192-207, media 208-223 -
 * the ungated and gated rows share the offsets; the selector decides
 * which view the bytes carry). */
static void build_synthetic_page14(uint8_t selector) {
    memset(synth_page14, 0, sizeof(synth_page14));
    synth_page14[0] = selector;
    for (size_t lane = 0; lane < 8; ++lane) {
        const uint16_t code = f16_lane_codes[lane];
        /* The supplied window starts at offset 128. */
        synth_page14[64 + 2 * lane] = (uint8_t)(code >> 8);
        synth_page14[64 + 2 * lane + 1] = (uint8_t)(code & 0xff);
        synth_page14[80 + 2 * lane] = (uint8_t)(code >> 8);
        synth_page14[80 + 2 * lane + 1] = (uint8_t)(code & 0xff);
    }
}

/* All four BER fields, at every revision Page 14h exists for, decode
 * Valid scaled ratio values with the slice-1 golden bits; the 16-bit F16
 * code rides along in value.raw. */
static void f16_ber_values_all_revisions(void) {
    static const uint8_t revs[5][2] = {{4, 0}, {5, 0}, {5, 1}, {5, 2}, {5, 3}};
    for (size_t r = 0; r < 5; ++r) {
        build_synthetic_lower((uint8_t)((revs[r][0] << 4) | revs[r][1]));
        for (size_t f = 0; f < 4; ++f) {
            build_synthetic_page14(ber_fields[f].selector);
            parts_t parts = {0};
            add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
            add_part(&parts, 0x50, 0, 0x14, 128, synth_page14, sizeof(synth_page14));
            openxcvr_selector_acquisition_t evidence[1] = {
                page14h_acquisition(ber_fields[f].selector, 192, 64)
            };
            const openxcvr_acquisition_context_t ctx = {1, evidence};

            openxcvr_resolved_field_t entries[8];
            const size_t count =
                resolve_page14h_at(ber_fields[f].name, revs[r][0], revs[r][1], entries, 8);
            CHECK(count == 8);
            for (size_t i = 0; i < count; ++i) {
                openxcvr_observation_t obs;
                CHECK(openxcvr_observe_field_ctx(
                    &entries[i], parts.obs, parts.count, NULL, &ctx, &obs) == OPENXCVR_OK);
                CHECK(obs.state == OPENXCVR_OBS_VALID);
                CHECK(obs.has_value == 1 && obs.value.is_scaled == 1);
                CHECK(obs.value.unit == OPENXCVR_UNIT_RATIO);
                union {
                    uint64_t bits;
                    double value;
                } expected;
                expected.bits = f16_lane_bits[i];
                CHECK(obs.value.value == expected.value);
                CHECK(obs.value.raw == f16_lane_codes[i]);
            }
        }
    }
}

/* A selector that selects the other BER view (11h for ungated rows, 01h
 * for gated rows) or the SNR view (06h) is Invalid with matching
 * evidence: no value, never a reinterpreted view. */
static void f16_wrong_selector_is_invalid(void) {
    build_synthetic_lower(0x51);
    static const uint8_t wrong[3] = {0x11, 0x01, 0x06};
    for (size_t f = 0; f < 4; ++f) {
        openxcvr_resolved_field_t entries[8];
        const size_t count = resolve_page14h(ber_fields[f].name, entries, 8);
        CHECK(count == 8);
        for (size_t s = 0; s < 3; ++s) {
            if (wrong[s] == ber_fields[f].selector) {
                continue;
            }
            build_synthetic_page14(wrong[s]);
            parts_t parts = {0};
            add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
            add_part(&parts, 0x50, 0, 0x14, 128, synth_page14, sizeof(synth_page14));
            openxcvr_selector_acquisition_t evidence[1] = {page14h_acquisition(wrong[s], 192, 64)};
            const openxcvr_acquisition_context_t ctx = {1, evidence};
            for (size_t i = 0; i < count; ++i) {
                openxcvr_observation_t obs;
                CHECK(openxcvr_observe_field_ctx(
                    &entries[i], parts.obs, parts.count, NULL, &ctx, &obs) == OPENXCVR_OK);
                CHECK(obs.state == OPENXCVR_OBS_INVALID);
                CHECK(obs.has_value == 0);
            }
        }
    }
}

/* A snapshot whose revision byte reports CMIS 3.0 (Page 14h does not
 * exist there) is UNSUPPORTED_REVISION, for every BER field. */
static void f16_reported_revision_3_0_is_unsupported(void) {
    build_synthetic_lower(0x30);
    for (size_t f = 0; f < 4; ++f) {
        build_synthetic_page14(ber_fields[f].selector);
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
        add_part(&parts, 0x50, 0, 0x14, 128, synth_page14, sizeof(synth_page14));
        openxcvr_selector_acquisition_t evidence[1] = {
            page14h_acquisition(ber_fields[f].selector, 192, 64)
        };
        const openxcvr_acquisition_context_t ctx = {1, evidence};

        openxcvr_resolved_field_t entries[8];
        const size_t count = resolve_page14h(ber_fields[f].name, entries, 8);
        CHECK(count == 8);
        for (size_t i = 0; i < count; ++i) {
            openxcvr_observation_t obs;
            CHECK(openxcvr_observe_field_ctx(
                &entries[i], parts.obs, parts.count, NULL, &ctx, &obs) == OPENXCVR_OK);
            CHECK(obs.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
            CHECK(obs.has_value == 0);
        }
    }
}

/* Hand-built rows (synthetic): F16 scaling on a row that is not F16BE, or
 * whose declared length is not two bytes, is UNSUPPORTED_FIELD with no
 * value - the mismatch is stated, never a narrowed reinterpretation. */
static void f16_hand_built_row_mismatch_is_unsupported(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);
    /* The generated row must carry the F16 ratio scaling this test breaks. */
    CHECK(entries[0].row->scaling.kind == OPENXCVR_SCALING_F16);
    CHECK(entries[0].row->scaling.scaled.unit == OPENXCVR_UNIT_RATIO);

    build_synthetic_lower(0x51);

    /* A non-F16BE kind (U16BE) with the F16 scaling kept. */
    {
        openxcvr_catalog_row_t row = *entries[0].row;
        row.kind = OPENXCVR_FIELD_U16BE;
        row.presence_kind = OPENXCVR_PRESENCE_UNCONDITIONAL;
        openxcvr_resolved_field_t entry = entries[0];
        entry.row = &row;
        const uint8_t ber[2] = {0xC0, 0x01};
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
        add_part(&parts, 0x50, 0, 0x14, 208, ber, sizeof(ber));
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, NULL, &obs) == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_UNSUPPORTED_FIELD);
        CHECK(obs.has_value == 0);
        CHECK(obs.note != NULL
            && strcmp(obs.note, "f16 scaling requires a two-byte f16be row") == 0);
    }

    /* A one-byte declared length: the two-byte F16 extraction refuses
     * before any value exists. */
    {
        openxcvr_catalog_row_t row = *entries[0].row;
        row.presence_kind = OPENXCVR_PRESENCE_UNCONDITIONAL;
        row.len = 1;
        openxcvr_resolved_field_t entry = entries[0];
        entry.row = &row;
        entry.range.len = 1;
        const uint8_t ber[1] = {0xC0};
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
        add_part(&parts, 0x50, 0, 0x14, 208, ber, sizeof(ber));
        openxcvr_observation_t obs;
        CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, NULL, &obs) == OPENXCVR_OK);
        CHECK(obs.state == OPENXCVR_OBS_UNSUPPORTED_FIELD);
        CHECK(obs.has_value == 0);
    }
}

/* A three-byte declared length with the F16BE kind kept: the two-byte
 * extraction succeeds on the first two of the three supplied bytes, so
 * decode reaches the F16 arm and its declared-length half refuses the
 * row shape - UNSUPPORTED_FIELD with the note, no value. */
static void f16_three_byte_row_reaches_the_length_guard(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);
    /* The generated row must carry the F16 ratio scaling this test keeps. */
    CHECK(entries[0].row->scaling.kind == OPENXCVR_SCALING_F16);
    CHECK(entries[0].row->scaling.scaled.unit == OPENXCVR_UNIT_RATIO);

    build_synthetic_lower(0x51);

    openxcvr_catalog_row_t row = *entries[0].row;
    row.presence_kind = OPENXCVR_PRESENCE_UNCONDITIONAL;
    row.len = 3;
    openxcvr_resolved_field_t entry = entries[0];
    entry.row = &row;
    entry.range.len = 3;
    const uint8_t ber[3] = {0xC0, 0x01, 0x00};
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
    add_part(&parts, 0x50, 0, 0x14, 208, ber, sizeof(ber));
    openxcvr_observation_t obs;
    CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, NULL, &obs) == OPENXCVR_OK);
    CHECK(obs.state == OPENXCVR_OBS_UNSUPPORTED_FIELD);
    CHECK(obs.has_value == 0);
    CHECK(obs.note != NULL
        && strcmp(obs.note, "f16 scaling requires a two-byte f16be row") == 0);
}

/* The planners still plan a BER entry's bytes: both C planner rounds
 * return the row's bytes with no refusal - F16 scaling adds no decode
 * dependency. */
static void f16_planners_still_plan_ber_rows(void) {
    openxcvr_resolved_field_t entries[8];
    const size_t count = resolve_page14h("media_side_ber", entries, 8);
    CHECK(count == 8);

    /* Round 1 (prelude): no refusals; the selector prerequisite byte is
     * planned. */
    {
        openxcvr_planned_read_t reads[64];
        size_t read_count = 0;
        openxcvr_field_plan_refusal_t refusals[8];
        size_t refusal_count = 0;
        CHECK(openxcvr_field_read_prelude(OPENXCVR_SPEC_CMIS, 5, 1, entries, count, reads, 64,
            &read_count, refusals, 8, &refusal_count) == OPENXCVR_OK);
        CHECK(refusal_count == 0);
        CHECK(read_count > 0);
        int selector_planned = 0;
        for (size_t i = 0; i < read_count; ++i) {
            if (reads[i].range.page == 0x14 && reads[i].range.offset <= 128
                && reads[i].range.offset + reads[i].range.len >= 129) {
                selector_planned = 1;
            }
        }
        CHECK(selector_planned);
    }

    /* Round 2 (remaining), with the prelude bytes supplied: the row's own
     * bytes are planned, no refusals (an acquisition requirement is not a
     * refusal). */
    {
        build_synthetic_lower(0x51);
        build_synthetic_page14(0x01);
        parts_t parts = {0};
        add_part(&parts, 0x50, 0, 0x00, 0, synth_lower, sizeof(synth_lower));
        add_part(&parts, 0x50, 0, 0x14, 128, synth_page14, sizeof(synth_page14));
        openxcvr_planned_read_t reads[64];
        size_t read_count = 0;
        openxcvr_field_admission_refusal_t refusals[8];
        size_t refusal_count = 0;
        openxcvr_acquisition_requirement_t required[8];
        size_t required_count = 0;
        CHECK(openxcvr_field_read_remaining(OPENXCVR_SPEC_CMIS, 5, 1, entries, count,
            parts.obs, parts.count, reads, 64, &read_count, refusals, 8, &refusal_count, required,
            8, &required_count) == OPENXCVR_OK);
        CHECK(refusal_count == 0);
        int data_planned = 0;
        for (size_t i = 0; i < read_count; ++i) {
            if (reads[i].range.page == 0x14 && reads[i].range.offset <= 208
                && reads[i].range.offset + reads[i].range.len >= 224) {
                data_planned = 1;
            }
        }
        CHECK(data_planned);
    }
}

int main(void) {
    load_fixture("tests/fixtures/cmis/fboss-cmis-5-1-osfp-credo-800g-aec.lower.bin",
        lower_buf, &lower_len);
    load_fixture("tests/fixtures/cmis/fboss-cmis-5-1-osfp-credo-800g-aec.page-14h.bin",
        page14_buf, &page14_len);

    credo_without_evidence();
    revision_byte_admission();
    resolution_refuses_at_cmis_3_0();
    credo_with_matching_evidence();
    credo_with_contradictory_evidence();
    synthetic_selector_evidence();
    evidence_range_containment();
    short_sixteen_bit_prerequisite_refuses();
    context_without_acquisitions_is_rejected();
    f16_ber_values_all_revisions();
    f16_wrong_selector_is_invalid();
    f16_reported_revision_3_0_is_unsupported();
    f16_hand_built_row_mismatch_is_unsupported();
    f16_three_byte_row_reaches_the_length_guard();
    f16_planners_still_plan_ber_rows();
    return 0;
}
