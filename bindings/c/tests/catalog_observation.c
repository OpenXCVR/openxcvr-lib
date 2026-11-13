/*
 * C parity for the catalog and basic-observation surfaces. The snapshots and
 * expected values mirror crates/openxcvr/tests/obs_basic_catalog.rs exactly:
 * the C binding must produce the same typed values, units and validity states
 * for the same supplied bytes. Synthetic bytes only; no fixtures, no I/O.
 */
#include "openxcvr/catalog.h"
#include "openxcvr/observation.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(cond) do { \
    if (!(cond)) { \
        fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
        exit(1); \
    } \
} while (0)

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

static void put_u16(uint8_t *out, uint16_t v) {
    out[0] = (uint8_t)(v >> 8);
    out[1] = (uint8_t)v;
}

static void put_i16(uint8_t *out, int16_t v) {
    put_u16(out, (uint16_t)v);
}

/* ─── Catalog resolution parity ─────────────────────────────────────────── */

static void catalog_resolution(void) {
    openxcvr_resolved_field_t out[16];
    size_t count = 99;

    /* Exact name at CMIS 5.3. */
    openxcvr_field_query_t query = {NULL, OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    query.pattern = "temp_mon_value";
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(out[0].row != NULL);
    CHECK(strcmp(out[0].row->name, "temp_mon_value") == 0);
    CHECK(out[0].range.address == 0x50 && out[0].range.bank == 0 && out[0].range.page == 0);
    CHECK(out[0].range.offset == 14 && out[0].range.len == 2);
    CHECK(out[0].bit_offset == 0);
    CHECK(out[0].row->kind == OPENXCVR_FIELD_I16BE);
    CHECK(out[0].row->access == OPENXCVR_ACCESS_RO);
    CHECK(out[0].row->scaling.kind == OPENXCVR_SCALING_SCALED);
    CHECK(out[0].row->scaling.scaled.is_signed == 1);
    CHECK(out[0].row->scaling.scaled.num == 1 && out[0].row->scaling.scaled.den == 256);
    CHECK(out[0].row->scaling.scaled.unit == OPENXCVR_UNIT_CELSIUS);
    /* Plain RO telemetry declares no read effects (an empty mask). */
    CHECK((out[0].row->read_effect_mask & OPENXCVR_READ_EFFECT_CLEAR_ON_READ) == 0);

    /* Schema alias only resolves in its revision span. */
    query.pattern = "twi_max_speed_cmis4";
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 4, 0, &query, out, 16, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(strcmp(out[0].row->name, "twi_max_speed_cmis4") == 0);
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count)
        == OPENXCVR_ERR_NO_MATCH);

    /* Glob expansion and unmatched glob. Size-query first: the pattern
     * expands across every optical-power field and lane dimension. */
    query.pattern = "optical_power_*";
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, NULL, 0, &count) == OPENXCVR_OK);
    CHECK(count >= 2);
    {
        openxcvr_resolved_field_t *glob_out = malloc(count * sizeof(*glob_out));
        CHECK(glob_out != NULL);
        CHECK(openxcvr_catalog_resolve(
            OPENXCVR_SPEC_CMIS, 5, 3, &query, glob_out, count, &count) == OPENXCVR_OK);
        int has_tx = 0, has_rx = 0;
        for (size_t i = 0; i < count; ++i) {
            if (strcmp(glob_out[i].row->name, "optical_power_tx") == 0) has_tx = 1;
            if (strcmp(glob_out[i].row->name, "optical_power_rx") == 0) has_rx = 1;
        }
        CHECK(has_tx && has_rx);
        free(glob_out);
    }
    query.pattern = "no_such_*";
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count)
        == OPENXCVR_ERR_NO_MATCH);

    /* Ambiguous exact name. */
    query.pattern = "host_interface_id_app";
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count)
        == OPENXCVR_ERR_AMBIGUOUS);

    /* Lanes: all, one, out of bounds, trimmed. */
    query.pattern = "optical_power_tx";
    query.lane = OPENXCVR_LANE_ALL;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count) == OPENXCVR_OK);
    CHECK(count == 8);
    CHECK(out[7].has_lane == 1 && out[7].lane == 8);
    CHECK(out[7].range.offset == 168 && out[7].range.len == 2);
    CHECK(out[7].range.page == 0x11);
    query.lane = OPENXCVR_LANE_ONE;
    query.lane_arg = 9;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count)
        == OPENXCVR_ERR_OUT_OF_RANGE);
    query.lane = OPENXCVR_LANE_UP_TO;
    query.lane_arg = 2;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 16, &count) == OPENXCVR_OK);
    CHECK(count == 2);
    CHECK(out[1].lane == 2 && out[1].range.offset == 156);

    /* Unsupported revision and family. */
    query.lane = OPENXCVR_LANE_UNSPECIFIED;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 5, &query, out, 16, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_SFF8472, 5, 3, &query, out, 16, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);

    /* SFF families resolve their own fields. */
    query.pattern = "rx_power";
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_SFF8472, 12, 5, &query, out, 16, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(out[0].range.address == 0x51 && out[0].range.offset == 104 && out[0].range.len == 2);

    /* Size query semantics. */
    query.pattern = "temp_mon_value";
    count = 99;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, NULL, 0, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 0, &count)
        == OPENXCVR_ERR_SHORT_BUFFER);

    /* Table accessor. */
    CHECK(openxcvr_catalog_table(OPENXCVR_SPEC_CMIS) != NULL);
    CHECK(openxcvr_catalog_table(OPENXCVR_SPEC_SFF8636) != NULL);
    CHECK(openxcvr_catalog_table(OPENXCVR_SPEC_SFF8472) != NULL);
    CHECK(openxcvr_catalog_table((openxcvr_spec_kind_t)99) == NULL);
    CHECK(openxcvr_catalog_table(OPENXCVR_SPEC_CMIS)->row_count > 500);
}

/* ─── CMIS basic observations parity ────────────────────────────────────── */

static void cmis_observations(void) {
    uint8_t lower[18];
    memset(lower, 0, sizeof(lower));
    lower[1] = 0x53;
    put_u16(&lower[14], 0x1000);   /* 16.0 degC */
    put_u16(&lower[16], 33000);    /* 3.3 V */
    uint8_t page11[128];
    memset(page11, 0, sizeof(page11));
    for (uint16_t lane = 0; lane < 8; lane++) {
        put_u16(&page11[154 - 128 + 2 * lane], (uint16_t)(1000 + lane));
        put_u16(&page11[170 - 128 + 2 * lane], (uint16_t)(2500 + 10 * lane));
        put_u16(&page11[186 - 128 + 2 * lane], (uint16_t)(3000 + lane));
    }
    uint8_t page01_160 = 0x00;     /* multiplier x1 */
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    add_part(&parts, 0x50, 0, 0x11, 128, page11, sizeof(page11));
    add_part(&parts, 0x50, 0, 0x01, 160, &page01_160, 1);

    openxcvr_basic_observations_t observed;
    memset(&observed, 0xa5, sizeof(observed));
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.revision_major == 5 && observed.revision_minor == 3);
    CHECK(observed.temperature.state == OPENXCVR_OBS_VALID);
    CHECK(observed.temperature.has_value == 1);
    CHECK(observed.temperature.value.is_scaled == 1);
    CHECK(fabs(observed.temperature.value.value - 16.0) < 1e-9);
    CHECK(observed.temperature.value.unit == OPENXCVR_UNIT_CELSIUS);
    CHECK(strcmp(observed.temperature.field, "temp_mon_value") == 0);
    CHECK(observed.supply_voltage.state == OPENXCVR_OBS_VALID);
    CHECK(fabs(observed.supply_voltage.value.value - 3.3) < 1e-9);
    CHECK(observed.supply_voltage.value.unit == OPENXCVR_UNIT_VOLT);
    CHECK(observed.lane_count == 8);
    /* CMIS reports no compliance code: its revision byte is proven into
     * revision_major/minor and reported_compliance stays absent. */
    CHECK(observed.has_reported_compliance == 0);
    for (uint16_t lane = 0; lane < 8; lane++) {
        const openxcvr_lane_observations_t *l = &observed.lanes[lane];
        CHECK(l->lane == lane + 1);
        CHECK(l->tx_bias.state == OPENXCVR_OBS_VALID);
        CHECK(fabs(l->tx_bias.value.value - (2500.0 + 10.0 * lane) * 2.0e-6) < 1e-12);
        CHECK(l->tx_bias.value.unit == OPENXCVR_UNIT_AMPERE);
        CHECK(l->tx_bias.note == NULL);
        CHECK(fabs(l->tx_power.value.value - (1000.0 + lane) * 1.0e-7) < 1e-12);
        CHECK(l->tx_power.value.unit == OPENXCVR_UNIT_WATT);
        CHECK(fabs(l->rx_power.value.value - (3000.0 + lane) * 1.0e-7) < 1e-12);
        CHECK(l->rx_power.value.unit == OPENXCVR_UNIT_WATT);
    }
    CHECK(strcmp(observed.lanes[0].tx_bias.field, "laser_bias_tx") == 0);

    /* Second revision: 4.0 decodes identically, reporting its own revision. */
    lower[1] = 0x40;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 4, 0, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.revision_major == 4 && observed.revision_minor == 0);
    CHECK(fabs(observed.temperature.value.value - 16.0) < 1e-9);

    /* Bias multiplier advertisement x2. */
    lower[1] = 0x53;
    page01_160 = 0x08;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(fabs(observed.lanes[0].tx_bias.value.value - 2500.0 * 2.0e-6 * 2.0) < 1e-12);

    /* Bias multiplier advertisement x4 (10b). */
    page01_160 = 0x10;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(fabs(observed.lanes[0].tx_bias.value.value - 2500.0 * 2.0e-6 * 4.0) < 1e-12);
    CHECK(observed.lanes[0].tx_bias.note == NULL);

    /* Reserved multiplier code 11b: invalid, no value. */
    page01_160 = 0x18;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.lanes[0].tx_bias.state == OPENXCVR_OBS_INVALID);
    CHECK(observed.lanes[0].tx_bias.has_value == 0);

    /* Advertisement not supplied: no scaled value (not-supplied state),
     * never a base-LSB guess that can be off by 4x. */
    parts.count = 2;   /* lower + page 11h only */
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.lanes[0].tx_bias.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observed.lanes[0].tx_bias.has_value == 0);
    CHECK(observed.lanes[0].tx_bias.note != NULL);

    /* Absent optional page 11h: lane monitors not supplied, module monitors valid. */
    parts.count = 1;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.temperature.state == OPENXCVR_OBS_VALID);
    for (uint16_t lane = 0; lane < observed.lane_count; lane++) {
        CHECK(observed.lanes[lane].tx_bias.state == OPENXCVR_OBS_NOT_SUPPLIED);
        CHECK(observed.lanes[lane].tx_bias.has_value == 0);
        CHECK(observed.lanes[lane].tx_power.state == OPENXCVR_OBS_NOT_SUPPLIED);
        CHECK(observed.lanes[lane].rx_power.state == OPENXCVR_OBS_NOT_SUPPLIED);
    }

    /* Unsupported revision byte. */
    lower[1] = 0x55;
    parts.count = 3;
    page01_160 = 0x00;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    /* Revision byte disagreeing with the requested revision. */
    lower[1] = 0x40;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, &observed)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    /* Missing revision byte: bytes 2.. supplied, byte 1 absent. */
    {
        openxcvr_range_observation no_byte[1] = {
            {{0x50, 0, 0, 2, 16}, lower + 2, 16, 1},
        };
        CHECK(openxcvr_observe_basic(
            OPENXCVR_SPEC_CMIS, 5, 3, no_byte, 1, NULL, &observed)
            == OPENXCVR_ERR_INSUFFICIENT_INPUT);
    }
}

/* ─── SFF-8636 basic observations parity ────────────────────────────────── */

static void sff8636_observations(void) {
    uint8_t lower[128];
    memset(lower, 0, sizeof(lower));
    lower[1] = 0x0a;               /* Rev 2.12: the schema's own code */
    put_i16(&lower[22], -768);     /* -3.0 degC */
    put_u16(&lower[26], 33000);    /* 3.3 V */
    for (uint16_t lane = 0; lane < 4; lane++) {
        put_u16(&lower[34 + 2 * lane], (uint16_t)(3000 + lane));
        put_u16(&lower[42 + 2 * lane], (uint16_t)(2500 + 10 * lane));
        put_u16(&lower[50 + 2 * lane], (uint16_t)(1000 + lane));
    }
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0, 0, lower, sizeof(lower));

    openxcvr_basic_observations_t observed;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_SFF8636, 2, 12, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.revision_major == 2 && observed.revision_minor == 12);
    CHECK(observed.temperature.state == OPENXCVR_OBS_VALID);
    CHECK(fabs(observed.temperature.value.value - (-3.0)) < 1e-9);
    CHECK(fabs(observed.supply_voltage.value.value - 3.3) < 1e-9);
    CHECK(observed.lane_count == 4);
    CHECK(fabs(observed.lanes[0].rx_power.value.value - 3000.0 * 1.0e-7) < 1e-12);
    CHECK(fabs(observed.lanes[0].tx_bias.value.value - 2500.0 * 2.0e-6) < 1e-12);
    CHECK(fabs(observed.lanes[0].tx_power.value.value - 1000.0 * 1.0e-7) < 1e-12);
    /* The result carries the reported compliance and schema revision. */
    CHECK(observed.has_reported_compliance == 1);
    CHECK(observed.reported_compliance_code == 0x0a);
    CHECK(strcmp(observed.reported_compliance_meaning, "SFF-8636 Rev 2.12") == 0);

    /* Compliance gate: only 0Ah (Rev 2.12, SFF-8636 Rev 2.12 § 6.2.2
     * Table 6-4) admits. 08h/07h name earlier revisions, 00h is
     * unspecified, 09h names Rev 2.11 (not this schema's revision), and
     * 0Bh..FFh are reserved: every observation is an explicit
     * unsupported-revision outcome with no value. */
    const uint8_t refused_codes[] = {0x08, 0x07, 0x00, 0x09, 0x0b, 0xff};
    for (size_t i = 0; i < sizeof(refused_codes) / sizeof(refused_codes[0]); i++) {
        lower[1] = refused_codes[i];
        CHECK(openxcvr_observe_basic(
            OPENXCVR_SPEC_SFF8636, 2, 12, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
        CHECK(observed.revision_major == 2 && observed.revision_minor == 12);
        CHECK(observed.has_reported_compliance == 1);
        CHECK(observed.reported_compliance_code == refused_codes[i]);
        CHECK(observed.reported_compliance_meaning != NULL);
        CHECK(observed.temperature.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.temperature.has_value == 0);
        CHECK(observed.supply_voltage.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.supply_voltage.has_value == 0);
        CHECK(observed.lane_count == 4);
        for (uint16_t lane = 0; lane < observed.lane_count; lane++) {
            CHECK(observed.lanes[lane].tx_bias.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
            CHECK(observed.lanes[lane].tx_bias.has_value == 0);
            CHECK(observed.lanes[lane].tx_power.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
            CHECK(observed.lanes[lane].tx_power.has_value == 0);
            CHECK(observed.lanes[lane].rx_power.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
            CHECK(observed.lanes[lane].rx_power.has_value == 0);
        }
    }

    /* Missing compliance byte (lower page supplied from byte 2): the
     * revision is undetermined — refused with no value. */
    {
        openxcvr_range_observation no_byte[1] = {
            {{0x50, 0, 0, 2, 126}, lower + 2, 126, 1},
        };
        CHECK(openxcvr_observe_basic(
            OPENXCVR_SPEC_SFF8636, 2, 12, no_byte, 1, NULL, &observed) == OPENXCVR_OK);
        CHECK(observed.has_reported_compliance == 0);
        CHECK(observed.temperature.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.temperature.has_value == 0);
        CHECK(observed.temperature.note != NULL);
        CHECK(observed.lanes[0].tx_bias.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.lanes[0].tx_bias.has_value == 0);
    }

    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_SFF8636, 2, 10, parts.obs, parts.count, NULL, &observed)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);
}

/* ─── SFF-8472 calibration parity ───────────────────────────────────────── */

static void sff8472_observations(void) {
    uint8_t a0[96];
    memset(a0, 0, sizeof(a0));
    /* Without constants the A2h supply starts at byte 92, so bytes 56-91
     * (the calibration constants) are genuinely absent. */
    uint8_t a2_full[120];
    uint8_t a2_tail[28];
    memset(a2_full, 0, sizeof(a2_full));
    memset(a2_tail, 0, sizeof(a2_tail));
    put_i16(&a2_full[96], -768);   /* -3.0 degC internal */
    put_u16(&a2_full[98], 33000);  /* 3.3 V */
    put_u16(&a2_full[100], 2500);  /* 5 mA */
    put_u16(&a2_full[102], 1000);  /* 0.1 mW */
    put_u16(&a2_full[104], 3000);  /* 0.3 mW */
    memcpy(a2_tail, a2_full + 92, sizeof(a2_tail));

    /* Internal calibration: A0h:92 bit 5; byte 94 = 0Ah (Rev 12.5
     * compliance, SFF-8472 Rev 12.5a § 8.11 Table 8-8). */
    a0[92] = 0x40;
    a0[94] = 0x0a;
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0, 0, a0, sizeof(a0));
    add_part(&parts, 0x51, 0, 0, 0, a2_full, sizeof(a2_full));
    openxcvr_basic_observations_t observed;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_SFF8472, 12, 5, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.revision_major == 12 && observed.revision_minor == 5);
    CHECK(observed.temperature.state == OPENXCVR_OBS_VALID);
    CHECK(fabs(observed.temperature.value.value - (-3.0)) < 1e-9);
    CHECK(fabs(observed.supply_voltage.value.value - 3.3) < 1e-9);
    CHECK(observed.lane_count == 1);
    CHECK(observed.lanes[0].lane == 1);
    CHECK(fabs(observed.lanes[0].tx_bias.value.value - 5.0e-3) < 1e-12);
    CHECK(fabs(observed.lanes[0].tx_power.value.value - 1.0e-4) < 1e-12);
    CHECK(fabs(observed.lanes[0].rx_power.value.value - 3.0e-4) < 1e-12);
    CHECK(observed.has_reported_compliance == 1);
    CHECK(observed.reported_compliance_code == 0x0a);
    CHECK(strcmp(observed.reported_compliance_meaning, "Complies with SFF-8472 Rev 12.5.") == 0);

    /* External calibration with constants. */
    a0[92] = 0x10;
    parts.obs[1] = (openxcvr_range_observation){
        {0x51, 0, 0, 0, 120}, a2_full, 120, 1
    };
    put_u16(&a2_full[76], 0x0100);      /* tx_bias slope 1.0 */
    put_i16(&a2_full[78], 0);
    put_u16(&a2_full[80], 0x0100);      /* tx_power slope 1.0 */
    put_i16(&a2_full[82], 0);
    put_u16(&a2_full[84], 0x0200);      /* temperature slope 2.0 */
    put_i16(&a2_full[86], -256);        /* temperature offset */
    put_u16(&a2_full[88], 0x0100);      /* vcc slope 1.0 */
    put_i16(&a2_full[90], 0);
    put_u16(&a2_full[68], 0x3f80);      /* ~1.0f degree-1 coefficient */
    a2_full[70] = 0x00;
    a2_full[71] = 0x00;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_SFF8472, 12, 5, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(fabs(observed.temperature.value.value - (-7.0)) < 1e-6);
    CHECK(fabs(observed.supply_voltage.value.value - 3.3) < 1e-6);
    CHECK(fabs(observed.lanes[0].tx_bias.value.value - 5.0e-3) < 1e-9);
    CHECK(fabs(observed.lanes[0].tx_power.value.value - 1.0e-4) < 1e-9);
    CHECK(fabs(observed.lanes[0].rx_power.value.value - 3.0e-4) < 1e-9);

    /* External calibration with missing constants: uncalibrated, never raw. */
    a0[92] = 0x10;
    parts.obs[1] = (openxcvr_range_observation){
        {0x51, 0, 0, 92, 28}, a2_tail, 28, 1
    };
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_SFF8472, 12, 5, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.temperature.state == OPENXCVR_OBS_UNCALIBRATED);
    CHECK(observed.temperature.has_value == 0);
    CHECK(observed.supply_voltage.state == OPENXCVR_OBS_UNCALIBRATED);
    CHECK(observed.lanes[0].tx_bias.state == OPENXCVR_OBS_UNCALIBRATED);
    CHECK(observed.lanes[0].tx_power.state == OPENXCVR_OBS_UNCALIBRATED);
    CHECK(observed.lanes[0].rx_power.state == OPENXCVR_OBS_UNCALIBRATED);

    /* Missing A2h page: not supplied (compliance still admits). */
    parts.count = 1;
    a0[92] = 0x40;
    a0[94] = 0x0a;
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_SFF8472, 12, 5, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
    CHECK(observed.temperature.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observed.lanes[0].rx_power.state == OPENXCVR_OBS_NOT_SUPPLIED);

    /* Compliance gate: only 0Ah (Rev 12.5) admits. 09h/08h name earlier
     * revisions, 00h is unspecified, 0Bh..FFh are reserved. */
    parts.count = 2;
    parts.obs[1] = (openxcvr_range_observation){
        {0x51, 0, 0, 92, 28}, a2_tail, 28, 1
    };
    const uint8_t refused[] = {0x09, 0x08, 0x00, 0x0b, 0xff};
    for (size_t i = 0; i < sizeof(refused) / sizeof(refused[0]); i++) {
        a0[94] = refused[i];
        CHECK(openxcvr_observe_basic(
            OPENXCVR_SPEC_SFF8472, 12, 5, parts.obs, parts.count, NULL, &observed) == OPENXCVR_OK);
        CHECK(observed.revision_major == 12 && observed.revision_minor == 5);
        CHECK(observed.has_reported_compliance == 1);
        CHECK(observed.reported_compliance_code == refused[i]);
        CHECK(observed.reported_compliance_meaning != NULL);
        CHECK(observed.temperature.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.temperature.has_value == 0);
        CHECK(observed.supply_voltage.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.supply_voltage.has_value == 0);
        CHECK(observed.lanes[0].tx_bias.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.lanes[0].tx_bias.has_value == 0);
        CHECK(observed.lanes[0].tx_power.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.lanes[0].rx_power.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
    }

    /* Missing compliance byte (A0h supplied only through byte 93): the
     * revision is undetermined — refused with no value. */
    {
        uint8_t a0_short[94];
        memset(a0_short, 0, sizeof(a0_short));
        a0_short[92] = 0x40;
        openxcvr_range_observation short_view[2] = {
            {{0x50, 0, 0, 0, 94}, a0_short, 94, 1},
            {{0x51, 0, 0, 92, 28}, a2_tail, 28, 1},
        };
        CHECK(openxcvr_observe_basic(
            OPENXCVR_SPEC_SFF8472, 12, 5, short_view, 2, NULL, &observed) == OPENXCVR_OK);
        CHECK(observed.has_reported_compliance == 0);
        CHECK(observed.temperature.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.temperature.has_value == 0);
        CHECK(observed.temperature.note != NULL);
        CHECK(observed.lanes[0].rx_power.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
        CHECK(observed.lanes[0].rx_power.has_value == 0);
    }
}

/* ─── Clear-on-read gate parity ─────────────────────────────────────────── */

static void clear_on_read_gate(void) {
    openxcvr_resolved_field_t entry;
    size_t count = 0;
    openxcvr_field_query_t query = {"temp_mon_high_alarm_flag", OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, &entry, 1, &count) == OPENXCVR_OK);
    CHECK((entry.row->read_effect_mask & OPENXCVR_READ_EFFECT_CLEAR_ON_READ) != 0);

    uint8_t lower[18];
    memset(lower, 0, sizeof(lower));
    lower[1] = 0x53;
    lower[9] = 0x01;   /* high-alarm flag set (byte 9 bit 0) */
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0, 0, lower, sizeof(lower));

    openxcvr_observation_t observation;
    memset(&observation, 0xa5, sizeof(observation));
    openxcvr_observe_options_t defaults = {0};
    CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_CLEAR_ON_READ_REFUSED);
    CHECK(observation.has_value == 0);

    openxcvr_observe_options_t authorized = {1};
    CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, &authorized, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_VALID);
    CHECK(observation.has_value == 1);
    CHECK(observation.value.is_scaled == 0);
    CHECK(observation.value.raw == 1);
}

/* ─── Revision gate and bias rule on the field path ─────────────────────── */

static void field_path_rules(void) {
    openxcvr_resolved_field_t entry;
    size_t count = 0;
    openxcvr_observation_t observation;
    openxcvr_observe_options_t defaults = {0};

    /* SFF-8636: the generic field path refuses when the compliance byte
     * (page 00h byte 1) does not establish the schema revision. */
    openxcvr_field_query_t query = {"temperature", OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_SFF8636, 2, 12, &query, &entry, 1, &count)
        == OPENXCVR_OK);
    CHECK(count == 1);

    uint8_t lower[128];
    memset(lower, 0, sizeof(lower));
    put_i16(&lower[22], -768);     /* -3.0 degC */
    lower[1] = 0x08;               /* Rev 2.8-2.10: refused */
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
    CHECK(observation.has_value == 0);
    CHECK(observation.note != NULL);
    CHECK(strcmp(observation.note, "SFF-8636 Rev 2.8, 2.9, and 2.10") == 0);

    lower[1] = 0x0a;               /* Rev 2.12: admitted */
    CHECK(openxcvr_observe_field(&entry, parts.obs, parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_VALID);
    CHECK(observation.has_value == 1);
    CHECK(fabs(observation.value.value - (-3.0)) < 1e-9);

    /* SFF-8472: A0h byte 94 gates the same way. */
    openxcvr_field_query_t rx_query = {"rx_power", OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_SFF8472, 12, 5, &rx_query, &entry, 1, &count)
        == OPENXCVR_OK);
    CHECK(count == 1);
    uint8_t a0[96];
    memset(a0, 0, sizeof(a0));
    a0[92] = 0x40;
    uint8_t a2_tail[28];
    memset(a2_tail, 0, sizeof(a2_tail));
    put_u16(&a2_tail[104 - 92], 3000);   /* 0.3 mW */
    parts_t parts8472 = {0};
    add_part(&parts8472, 0x50, 0, 0, 0, a0, sizeof(a0));
    add_part(&parts8472, 0x51, 0, 0, 92, a2_tail, sizeof(a2_tail));

    a0[94] = 0x00;                /* unspecified: refused */
    CHECK(openxcvr_observe_field(&entry, parts8472.obs, parts8472.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_UNSUPPORTED_REVISION);
    CHECK(observation.has_value == 0);

    a0[94] = 0x0a;                /* Rev 12.5: admitted */
    CHECK(openxcvr_observe_field(&entry, parts8472.obs, parts8472.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_VALID);
    CHECK(observation.has_value == 1);
    CHECK(fabs(observation.value.value - 3.0e-4) < 1e-12);

    /* CMIS: the generic field path applies the TxBiasCurrentScalingFactor
     * advertisement rule (page 01h:160 bits 4:3). */
    openxcvr_field_query_t bias_query = {
        "laser_bias_tx", OPENXCVR_LANE_ONE, 1, "CmisPage11hLaneStatus", 0
    };
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &bias_query, &entry, 1, &count)
        == OPENXCVR_OK);
    CHECK(count == 1);

    uint8_t cmis_lower[18];
    memset(cmis_lower, 0, sizeof(cmis_lower));
    cmis_lower[1] = 0x53;
    uint8_t page11[128];
    memset(page11, 0, sizeof(page11));
    put_u16(&page11[170 - 128], 2500);
    uint8_t page01_160 = 0x08;     /* x2 */
    parts_t cmis_parts = {0};
    add_part(&cmis_parts, 0x50, 0, 0, 0, cmis_lower, sizeof(cmis_lower));
    add_part(&cmis_parts, 0x50, 0, 0x11, 128, page11, sizeof(page11));
    add_part(&cmis_parts, 0x50, 0, 0x01, 160, &page01_160, 1);

    CHECK(openxcvr_observe_field(&entry, cmis_parts.obs, cmis_parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_VALID);
    CHECK(fabs(observation.value.value - 2500.0 * 2.0e-6 * 2.0) < 1e-12);
    CHECK(observation.note == NULL);

    page01_160 = 0x10;             /* x4 */
    CHECK(openxcvr_observe_field(&entry, cmis_parts.obs, cmis_parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(fabs(observation.value.value - 2500.0 * 2.0e-6 * 4.0) < 1e-12);

    cmis_parts.count = 2;          /* advertisement missing: no scaled value */
    CHECK(openxcvr_observe_field(&entry, cmis_parts.obs, cmis_parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observation.has_value == 0);
    CHECK(observation.note != NULL);

    cmis_parts.count = 3;
    page01_160 = 0x18;             /* reserved: invalid */
    CHECK(openxcvr_observe_field(&entry, cmis_parts.obs, cmis_parts.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_INVALID);
    CHECK(observation.has_value == 0);
}

/* ─── Status strings ────────────────────────────────────────────────────── */

static void status_strings(void) {
    /* openxcvr_status_string covers the catalog-refusal codes too. */
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_OK), "ok") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_INVALID_ARGUMENT), "invalid argument") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_SHORT_BUFFER), "short buffer") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_INSUFFICIENT_INPUT), "insufficient supplied input") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_UNSUPPORTED_REVISION), "unsupported revision") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_IO), "io error") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_NO_MATCH), "no field matches the pattern") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_AMBIGUOUS), "ambiguous field pattern") == 0);
    CHECK(strcmp(openxcvr_status_string(OPENXCVR_ERR_OUT_OF_RANGE), "lane index out of range") == 0);
}

/* ─── Argument validation ───────────────────────────────────────────────── */

static void argument_validation(void) {
    openxcvr_resolved_field_t out[2];
    size_t count = 0;
    openxcvr_field_query_t query = {"temp_mon_value", OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, NULL, out, 2, &count)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, out, 2, NULL)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(openxcvr_catalog_resolve((openxcvr_spec_kind_t)99, 5, 3, &query, out, 2, &count)
        == OPENXCVR_ERR_INVALID_ARGUMENT);

    openxcvr_basic_observations_t observed;
    parts_t parts = {0};
    uint8_t lower[2] = {0x1b, 0x53};
    add_part(&parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, parts.obs, parts.count, NULL, NULL)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(openxcvr_observe_basic(
        (openxcvr_spec_kind_t)99, 5, 3, parts.obs, parts.count, NULL, &observed)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    /* Overlapping present observations in the same window are refused. */
    parts_t overlap = {0};
    uint8_t bytes[18];
    memset(bytes, 0, sizeof(bytes));
    bytes[1] = 0x53;
    add_part(&overlap, 0x50, 0, 0, 0, bytes, sizeof(bytes));
    add_part(&overlap, 0x50, 0, 0, 4, bytes, 4);
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, overlap.obs, overlap.count, NULL, &observed)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    /* Zero-length range is structurally invalid. */
    parts_t empty = {0};
    add_part(&empty, 0x50, 0, 0, 0, bytes, 0);
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, empty.obs, empty.count, NULL, &observed)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
}

/* ─── Basic observation read plan parity ─────────────────────────── */

/* Mirrors crates/openxcvr/tests/obs_basic_read_plan.rs: the same inputs
 * must produce the same planned ranges, optional flags and purposes on
 * both surfaces. */
static void basic_read_plan(void) {
    openxcvr_planned_read_t reads[24];
    size_t count = 99;

    /* CMIS prelude: the revision byte, size query first. */
    CHECK(openxcvr_basic_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 3, NULL, 0, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(openxcvr_basic_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 3, reads, 24, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(reads[0].range.address == 0x50 && reads[0].range.bank == 0);
    CHECK(reads[0].range.page == 0 && reads[0].range.offset == 1);
    CHECK(reads[0].range.len == 1);
    CHECK(reads[0].optional == 0);
    CHECK(strcmp(reads[0].purpose, "cmis_revision") == 0);
    /* Every supported CMIS revision names the same prelude. */
    for (int slot = 0; slot < 6; ++slot) {
        const uint8_t major = (uint8_t[]){3, 4, 5, 5, 5, 5}[slot];
        const uint8_t minor = (uint8_t[]){0, 0, 0, 1, 2, 3}[slot];
        CHECK(openxcvr_basic_read_prelude(
            OPENXCVR_SPEC_CMIS, major, minor, reads, 24, &count) == OPENXCVR_OK);
        CHECK(count == 1);
        CHECK(reads[0].range.offset == 1 && reads[0].range.len == 1);
    }
    /* Unsupported revision. */
    CHECK(openxcvr_basic_read_prelude(
        OPENXCVR_SPEC_CMIS, 6, 0, reads, 24, &count) == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    CHECK(openxcvr_basic_read_prelude(
        (openxcvr_spec_kind_t)99, 5, 3, reads, 24, &count)
        == OPENXCVR_ERR_INVALID_ARGUMENT);

    /* CMIS remaining: with the prelude byte supplied, the monitors plus
     * the bias advertisement; bias before tx, optional pages marked. */
    uint8_t rev53 = 0x53;
    parts_t cmis_prelude = {0};
    add_part(&cmis_prelude, 0x50, 0, 0, 1, &rev53, 1);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, cmis_prelude.obs, cmis_prelude.count, NULL, 0, &count)
        == OPENXCVR_OK);
    CHECK(count == 6);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, cmis_prelude.obs, cmis_prelude.count, reads, 24, &count)
        == OPENXCVR_OK);
    CHECK(count == 6);
    CHECK(reads[0].range.offset == 14 && reads[0].range.len == 2);
    CHECK(reads[0].range.page == 0 && reads[0].optional == 0);
    CHECK(strcmp(reads[0].purpose, "temp_mon_value") == 0);
    CHECK(reads[1].range.offset == 16 && reads[1].range.len == 2);
    CHECK(strcmp(reads[1].purpose, "vcc_mon_voltage") == 0);
    CHECK(reads[2].range.page == 0x11 && reads[2].range.bank == 0);
    CHECK(reads[2].range.offset == 170 && reads[2].range.len == 16);
    CHECK(reads[2].optional == 1);
    CHECK(strcmp(reads[2].purpose, "laser_bias_tx") == 0);
    CHECK(reads[3].range.page == 0x11 && reads[3].range.offset == 154);
    CHECK(reads[3].range.len == 16 && reads[3].optional == 1);
    CHECK(strcmp(reads[3].purpose, "optical_power_tx") == 0);
    CHECK(reads[4].range.page == 0x11 && reads[4].range.offset == 186);
    CHECK(reads[4].range.len == 16 && reads[4].optional == 1);
    CHECK(strcmp(reads[4].purpose, "optical_power_rx") == 0);
    CHECK(reads[5].range.page == 0x01 && reads[5].range.offset == 160);
    CHECK(reads[5].range.len == 1 && reads[5].optional == 1);
    CHECK(strcmp(reads[5].purpose, "tx_bias_current_scaling_factor") == 0);
    /* No planned byte is a clear-on-read byte (lower-page flags 8..=11). */
    for (size_t i = 0; i < count; ++i) {
        CHECK(reads[i].range.page != 0
            || reads[i].range.offset + reads[i].range.len <= 8
            || reads[i].range.offset >= 12);
    }
    /* Short buffer: *count is set, out unchanged. */
    {
        openxcvr_planned_read_t one[1];
        memset(one, 0, sizeof(one));
        count = 99;
        CHECK(openxcvr_basic_read_remaining(
            OPENXCVR_SPEC_CMIS, 5, 3, cmis_prelude.obs, cmis_prelude.count, one, 1, &count)
            == OPENXCVR_ERR_SHORT_BUFFER);
        CHECK(count == 6);
        CHECK(one[0].purpose == NULL);   /* unchanged */
    }

    /* The same gates as openxcvr_observe_basic. */
    uint8_t rev55 = 0x55;
    parts_t future = {0};
    add_part(&future, 0x50, 0, 0, 1, &rev55, 1);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, future.obs, future.count, reads, 24, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    uint8_t rev40 = 0x40;
    parts_t mismatch = {0};
    add_part(&mismatch, 0x50, 0, 0, 1, &rev40, 1);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, mismatch.obs, mismatch.count, reads, 24, &count)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    parts_t no_prelude = {0};
    add_part(&no_prelude, 0x50, 0, 0, 2, &rev53, 1);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, no_prelude.obs, no_prelude.count, reads, 24, &count)
        == OPENXCVR_ERR_INSUFFICIENT_INPUT);

    /* SFF-8636: prelude = compliance byte; remaining = five lower-page
     * monitor spans, none optional. */
    CHECK(openxcvr_basic_read_prelude(
        OPENXCVR_SPEC_SFF8636, 2, 12, reads, 24, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(reads[0].range.offset == 1 && reads[0].range.len == 1);
    CHECK(strcmp(reads[0].purpose, "revision_compliance") == 0);
    uint8_t lower8636[128];
    memset(lower8636, 0, sizeof(lower8636));
    lower8636[1] = 0x0a;
    parts_t sff8636_parts = {0};
    add_part(&sff8636_parts, 0x50, 0, 0, 0, lower8636, sizeof(lower8636));
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_SFF8636, 2, 12, sff8636_parts.obs, sff8636_parts.count, reads, 24, &count)
        == OPENXCVR_OK);
    CHECK(count == 5);
    CHECK(reads[0].range.offset == 22 && reads[0].range.len == 2);
    CHECK(strcmp(reads[0].purpose, "temperature") == 0);
    CHECK(reads[1].range.offset == 26 && reads[1].range.len == 2);
    CHECK(strcmp(reads[1].purpose, "supply_voltage") == 0);
    CHECK(reads[2].range.offset == 42 && reads[2].range.len == 8);
    CHECK(strcmp(reads[2].purpose, "tx_bias_lane") == 0);
    CHECK(reads[3].range.offset == 50 && reads[3].range.len == 8);
    CHECK(strcmp(reads[3].purpose, "tx_power_lane") == 0);
    CHECK(reads[4].range.offset == 34 && reads[4].range.len == 8);
    CHECK(strcmp(reads[4].purpose, "rx_power_lane") == 0);
    for (size_t i = 0; i < count; ++i) {
        CHECK(reads[i].range.page == 0 && reads[i].optional == 0);
    }
    /* Refused compliance: no monitor byte planned. */
    lower8636[1] = 0x08;
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_SFF8636, 2, 12, sff8636_parts.obs, sff8636_parts.count, reads, 24, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);
    lower8636[1] = 0xff;
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_SFF8636, 2, 12, sff8636_parts.obs, sff8636_parts.count, reads, 24, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);

    /* SFF-8472: prelude = compliance byte then discriminator byte. */
    CHECK(openxcvr_basic_read_prelude(
        OPENXCVR_SPEC_SFF8472, 12, 5, reads, 24, &count) == OPENXCVR_OK);
    CHECK(count == 2);
    CHECK(reads[0].range.offset == 94 && reads[0].range.len == 1);
    CHECK(strcmp(reads[0].purpose, "sff8472_compliance") == 0);
    CHECK(reads[1].range.offset == 92 && reads[1].range.len == 1);
    CHECK(strcmp(reads[1].purpose, "page_a0h_base_memory.externally_calibrated") == 0);

    /* Internal calibration: the five A2h monitor fields only. */
    uint8_t a0[96];
    memset(a0, 0, sizeof(a0));
    a0[92] = 0x40;
    a0[94] = 0x0a;
    uint8_t a2[120];
    memset(a2, 0, sizeof(a2));
    parts_t sff8472_parts = {0};
    add_part(&sff8472_parts, 0x50, 0, 0, 0, a0, sizeof(a0));
    add_part(&sff8472_parts, 0x51, 0, 0, 0, a2, sizeof(a2));
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, sff8472_parts.obs, sff8472_parts.count, reads, 24, &count)
        == OPENXCVR_OK);
    CHECK(count == 5);
    CHECK(reads[0].range.address == 0x51 && reads[0].range.offset == 96);
    CHECK(reads[0].range.len == 2 && reads[0].optional == 0);
    CHECK(strcmp(reads[0].purpose, "temperature") == 0);
    CHECK(reads[4].range.offset == 104 && strcmp(reads[4].purpose, "rx_power") == 0);

    /* External calibration: each field followed by its constants. */
    a0[92] = 0x10;
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, sff8472_parts.obs, sff8472_parts.count, reads, 24, &count)
        == OPENXCVR_OK);
    CHECK(count == 18);
    CHECK(reads[1].range.offset == 84 && reads[1].range.len == 2);
    CHECK(strcmp(reads[1].purpose, "temperature") == 0);
    CHECK(reads[4].range.offset == 88 && strcmp(reads[4].purpose, "supply_voltage") == 0);
    CHECK(reads[6].range.offset == 100 && reads[7].range.offset == 76);
    CHECK(strcmp(reads[7].purpose, "tx_bias") == 0);
    CHECK(reads[13].range.offset == 72 && reads[13].range.len == 4);
    CHECK(strcmp(reads[13].purpose, "rx_power") == 0);
    CHECK(reads[17].range.offset == 56 && reads[17].range.len == 4);
    /* Discriminator byte missing: INSUFFICIENT_INPUT naming nothing. */
    {
        uint8_t a0_partial[96];
        memset(a0_partial, 0, sizeof(a0_partial));
        a0_partial[94] = 0x0a;
        parts_t partial = {0};
        add_part(&partial, 0x50, 0, 0, 0, a0_partial, sizeof(a0_partial));
        add_part(&partial, 0x51, 0, 0, 0, a2, sizeof(a2));
        /* a0_partial covers byte 92 with zeros: not the missing case. The
         * missing-discriminator case needs a view that stops before 92
         * while supplying 94 — two disjoint ranges. */
        parts_t split = {0};
        add_part(&split, 0x50, 0, 0, 0, a0_partial, 92);          /* 0..91 */
        add_part(&split, 0x50, 0, 0, 94, a0_partial + 94, 2);      /* 94..95 */
        add_part(&split, 0x51, 0, 0, 0, a2, sizeof(a2));
        CHECK(openxcvr_basic_read_remaining(
            OPENXCVR_SPEC_SFF8472, 12, 5, split.obs, split.count, reads, 24, &count)
            == OPENXCVR_ERR_INSUFFICIENT_INPUT);
        (void)partial;
    }
    /* Refused compliance: no monitor byte planned. */
    a0[92] = 0x40;
    a0[94] = 0x09;
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_SFF8472, 12, 5, sff8472_parts.obs, sff8472_parts.count, reads, 24, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);

    /* Argument validation. */
    CHECK(openxcvr_basic_read_prelude(
        OPENXCVR_SPEC_CMIS, 5, 3, reads, 24, NULL) == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, cmis_prelude.obs, cmis_prelude.count, reads, 24, NULL)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, NULL, 0, reads, 24, &count)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
}

/* ─── CMIS U16LE field decoding parity ─────────────────────────────────── */

static void cmis_u16le_observation(void) {
    /* CMIS 5.5 remains rejected for catalog resolution. */
    openxcvr_field_query_t query = {"host_side_snr", OPENXCVR_LANE_ONE, 1, "CmisPage14hDiagnosticsResults", 0};
    openxcvr_resolved_field_t entry;
    size_t count = 0;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 5, &query, &entry, 1, &count)
        == OPENXCVR_ERR_UNSUPPORTED_REVISION);

    /* Resolve host_side_snr at CMIS 5.3 to derive field range and properties. */
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, &entry, 1, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(entry.row != NULL);
    CHECK(strcmp(entry.row->name, "host_side_snr") == 0);
    CHECK(entry.row->kind == OPENXCVR_FIELD_U16LE);
    CHECK(entry.range.len == 2);
    CHECK(entry.has_lane == 1 && entry.lane == 1);

    /* Supply revision proof (CMIS 5.3 lower memory byte 1 = 0x53) plus exact field bytes.
     * Use asymmetric bytes {0x34, 0x12} so little-endian vs big-endian decoding is distinguishable:
     * LE: (0x12 << 8) | 0x34 = 0x1234 = 4660 (in 1/256 dB = 18.203125 dB)
     * BE: (0x34 << 8) | 0x12 = 0x3412 = 13330 (in 1/256 dB = 52.0703125 dB) */
    uint8_t lower[2] = {0x00, 0x53};
    const uint8_t snr_bytes[2] = {0x34, 0x12};
    /* Stage 1 admission: the SNR rows are selector-multiplexed, so the
     * snapshot needs the DiagnosticsSelector byte (06h = SNR view) and the
     * decode a matching acquisition assertion covering bytes 192-255. */
    const uint8_t selector_byte[1] = {0x06};
    parts_t parts = {0};
    add_part(&parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    add_part(&parts, 0x50, 0, 0x14, 128, selector_byte, sizeof(selector_byte));
    add_part(&parts, entry.range.address, entry.range.bank, entry.range.page, entry.range.offset, snr_bytes, entry.range.len);
    openxcvr_selector_acquisition_t snr_evidence = {0};
    snr_evidence.generation = 1;
    snr_evidence.epoch = 2;
    snr_evidence.address = 0x50;
    snr_evidence.page = 0x14;
    snr_evidence.selector_offset = 128;
    snr_evidence.selector_value = 0x06;
    snr_evidence.data_range = (openxcvr_range){0x50, 0, 0x14, 192, 64};
    const openxcvr_acquisition_context_t snr_ctx = {1, &snr_evidence};

    openxcvr_observation_t observation;
    memset(&observation, 0xa5, sizeof(observation));
    openxcvr_observe_options_t defaults = {0};
    CHECK(openxcvr_observe_field_ctx(&entry, parts.obs, parts.count, &defaults, &snr_ctx, &observation) == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_VALID);
    CHECK(observation.has_value == 1);
    CHECK(observation.value.is_scaled == 1);
    CHECK(observation.value.unit == OPENXCVR_UNIT_DECIBEL);

    const double expected_db = 4660.0 / 256.0;
    if (fabs(observation.value.value - expected_db) >= 1e-9) {
        fprintf(stderr,
            "FAIL u16le decoding numeric swap: expected %.6f dB (raw 0x1234), got %.6f dB\n",
            expected_db, observation.value.value);
    }
    CHECK(fabs(observation.value.value - expected_db) < 1e-9);

    /* Also verify media_side_snr resolves to U16LE and decodes identically. */
    openxcvr_field_query_t media_query = {"media_side_snr", OPENXCVR_LANE_ONE, 1, "CmisPage14hDiagnosticsResults", 0};
    openxcvr_resolved_field_t media_entry;
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &media_query, &media_entry, 1, &count) == OPENXCVR_OK);
    CHECK(count == 1);
    CHECK(media_entry.row != NULL);
    CHECK(strcmp(media_entry.row->name, "media_side_snr") == 0);
    CHECK(media_entry.row->kind == OPENXCVR_FIELD_U16LE);
    CHECK(media_entry.range.len == 2);

    parts_t media_parts = {0};
    add_part(&media_parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    add_part(&media_parts, 0x50, 0, 0x14, 128, selector_byte, sizeof(selector_byte));
    add_part(&media_parts, media_entry.range.address, media_entry.range.bank, media_entry.range.page, media_entry.range.offset, snr_bytes, media_entry.range.len);

    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field_ctx(&media_entry, media_parts.obs, media_parts.count, &defaults, &snr_ctx, &observation) == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_VALID);
    CHECK(observation.has_value == 1);
    CHECK(observation.value.is_scaled == 1);
    CHECK(observation.value.unit == OPENXCVR_UNIT_DECIBEL);
    CHECK(fabs(observation.value.value - expected_db) < 1e-9);

    /* Refusal tests: missing or short ranges must not produce a spurious value. */
    parts_t missing_parts = {0};
    add_part(&missing_parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field(&entry, missing_parts.obs, missing_parts.count, &defaults, &observation) == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observation.has_value == 0);

    parts_t short_parts = {0};
    add_part(&short_parts, 0x50, 0, 0, 0, lower, sizeof(lower));
    add_part(&short_parts, entry.range.address, entry.range.bank, entry.range.page, entry.range.offset, snr_bytes, 1);
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field(&entry, short_parts.obs, short_parts.count, &defaults, &observation) == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observation.has_value == 0);
}

/* ─── Seven-bit I2C address bound ───────────────────────────── */

/* The shared range validator rejects addresses above 0x7f, the
 * seven-bit I2C contract documented at openxcvr.h and matched by Rust
 * Range::validate; the refusal happens before any output is written.
 * Pinned through every entry point that shares the validator:
 * observe_field, observe_field_ctx, observe_basic and
 * basic_read_remaining (basic_read_prelude takes no view and never
 * calls it). */
static void address_bound_regression(void) {
    openxcvr_resolved_field_t entry;
    size_t count = 0;
    openxcvr_field_query_t query = {"temp_mon_value", OPENXCVR_LANE_UNSPECIFIED, 0, NULL, 0};
    CHECK(openxcvr_catalog_resolve(OPENXCVR_SPEC_CMIS, 5, 3, &query, &entry, 1, &count)
        == OPENXCVR_OK);

    /* Synthetic lower-memory byte 1 (CMIS 5.3) at an address above the
     * seven-bit bound, structurally consistent otherwise, so only the
     * address rule can refuse it. */
    uint8_t rev53 = 0x53;
    parts_t invalid = {0};
    add_part(&invalid, 0x80, 0, 0, 1, &rev53, 1);

    openxcvr_planned_read_t reads[24];
    openxcvr_basic_observations_t observed;
    openxcvr_observation_t observation;
    openxcvr_observe_options_t defaults = {0};

    count = 777;
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, invalid.obs, invalid.count, reads, 24, &count)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    CHECK(count == 777);
    memset(&observed, 0xa5, sizeof(observed));
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, invalid.obs, invalid.count, NULL, &observed)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field(&entry, invalid.obs, invalid.count, &defaults, &observation)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field_ctx(&entry, invalid.obs, invalid.count,
        &defaults, NULL, &observation)
        == OPENXCVR_ERR_INVALID_ARGUMENT);
    {
        unsigned char untouched[sizeof(observation)];
        memset(untouched, 0xa5, sizeof(untouched));
        CHECK(memcmp(&observation, untouched, sizeof(observation)) == 0);
    }

    /* The identical view at the in-bounds module address 0x50 is
     * accepted by the same entry points. */
    parts_t valid = {0};
    add_part(&valid, 0x50, 0, 0, 1, &rev53, 1);
    count = 777;
    CHECK(openxcvr_basic_read_remaining(
        OPENXCVR_SPEC_CMIS, 5, 3, valid.obs, valid.count, reads, 24, &count)
        == OPENXCVR_OK);
    CHECK(count == 6);
    memset(&observed, 0xa5, sizeof(observed));
    CHECK(openxcvr_observe_basic(
        OPENXCVR_SPEC_CMIS, 5, 3, valid.obs, valid.count, NULL, &observed)
        == OPENXCVR_OK);
    CHECK(observed.revision_major == 5 && observed.revision_minor == 3);
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field(&entry, valid.obs, valid.count, &defaults, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observation.has_value == 0);
    memset(&observation, 0xa5, sizeof(observation));
    CHECK(openxcvr_observe_field_ctx(&entry, valid.obs, valid.count,
        &defaults, NULL, &observation)
        == OPENXCVR_OK);
    CHECK(observation.state == OPENXCVR_OBS_NOT_SUPPLIED);
    CHECK(observation.has_value == 0);
}

int main(void) {
    catalog_resolution();
    cmis_observations();
    sff8636_observations();
    sff8472_observations();
    clear_on_read_gate();
    field_path_rules();
    cmis_u16le_observation();
    status_strings();
    argument_validation();
    basic_read_plan();
    address_bound_regression();
    puts("catalog/observation C parity: all checks passed");
    return 0;
}
