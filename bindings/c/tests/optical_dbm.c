#include "openxcvr/observation.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void shared_vectors(const char *path) {
    FILE *f = fopen(path, "r");
    char line[512];
    unsigned count = 0;
    assert(f);
    while (fgets(line, sizeof line, f)) {
        char name[80], watts[80];
        int state, input_kind, kind, result_state;
        double expected;
        openxcvr_observation_t source = {0}, before;
        openxcvr_optical_dbm_t result = {0};
        uint64_t bits;
        if (line[0] == '#') continue;
        assert(sscanf(line, "%79s %d %d %79s %d %d %lf", name, &state,
                      &input_kind, watts, &kind, &result_state, &expected) == 7);
        source.state = (openxcvr_observation_state_t)state;
        source.has_value = input_kind != 3;
        source.value.is_scaled = input_kind < 2;
        source.value.unit = input_kind == 1 ? OPENXCVR_UNIT_VOLT : OPENXCVR_UNIT_WATT;
        source.value.value = strtod(watts, NULL);
        source.value.raw = 42;
        source.field = "optical_power_rx";
        source.has_lane = 1;
        source.lane = 3;
        source.note = "retained";
        memcpy(&before, &source, sizeof source);
        result.size = sizeof result;
        result.version = OPENXCVR_OPTICAL_DBM_VERSION;
        assert(openxcvr_optical_power_dbm(&source, &result) == OPENXCVR_OK);
        assert(result.source == &source);
        assert(memcmp(&source, &before, sizeof source) == 0);
        assert(result.size == sizeof result && result.version == 1);
        assert((int)result.state == result_state);
        assert((int)result.kind == kind);
        assert(isfinite(result.dbm));
        if (kind == 0) assert(fabs(result.dbm - expected) <= 1e-10);
        else assert(result.dbm == 0.0);
        memcpy(&bits, &result.dbm, sizeof bits);
        printf("PARITY %s %d %d %016llx\n", name, result_state, kind,
               (unsigned long long)bits);
        ++count;
    }
    assert(feof(f) && count == 26);
    assert(fclose(f) == 0);
}

static void arguments(void) {
    openxcvr_observation_t source = {0};
    openxcvr_optical_dbm_t out = {0}, before;
    assert(openxcvr_optical_power_dbm(&source, NULL) == OPENXCVR_ERR_INVALID_ARGUMENT);
    out.size = sizeof out;
    out.version = OPENXCVR_OPTICAL_DBM_VERSION;
    memcpy(&before, &out, sizeof out);
    assert(openxcvr_optical_power_dbm(NULL, &out) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(memcmp(&out, &before, sizeof out) == 0);
    out.size--;
    memcpy(&before, &out, sizeof out);
    assert(openxcvr_optical_power_dbm(&source, &out) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(memcmp(&out, &before, sizeof out) == 0);
    out.size = sizeof out;
    out.version++;
    memcpy(&before, &out, sizeof out);
    assert(openxcvr_optical_power_dbm(&source, &out) == OPENXCVR_ERR_INVALID_ARGUMENT);
    assert(memcmp(&out, &before, sizeof out) == 0);
}

static void states_units_and_extended_storage(void) {
    openxcvr_observation_t source = {0};
    struct { openxcvr_optical_dbm_t v1; unsigned char tail[16]; } extended;
    openxcvr_optical_dbm_t *out = &extended.v1;
    int state, shape, unit;
    memset(&extended, 0xa5, sizeof extended);
    out->size = sizeof extended;
    out->version = OPENXCVR_OPTICAL_DBM_VERSION;
    for (state = 1; state <= 7; ++state) {
        for (shape = 0; shape < 3; ++shape) {
            source.state = (openxcvr_observation_state_t)state;
            source.has_value = shape != 0;
            source.value.is_scaled = shape == 2;
            source.value.unit = OPENXCVR_UNIT_WATT;
            source.value.value = 0.001;
            assert(openxcvr_optical_power_dbm(&source, out) == OPENXCVR_OK);
            assert(out->state == source.state);
            assert(out->kind == OPENXCVR_OPTICAL_DBM_UNAVAILABLE);
            assert(out->dbm == 0.0);
        }
    }
    source.state = OPENXCVR_OBS_VALID;
    for (unit = OPENXCVR_UNIT_CELSIUS; unit <= OPENXCVR_UNIT_RATIO; ++unit) {
        if (unit == OPENXCVR_UNIT_WATT) continue;
        source.value.unit = (openxcvr_unit_t)unit;
        assert(openxcvr_optical_power_dbm(&source, out) == OPENXCVR_OK);
        assert(out->state == OPENXCVR_OBS_VALID);
        assert(out->kind == OPENXCVR_OPTICAL_DBM_NOT_CONVERTIBLE);
        assert(out->dbm == 0.0);
    }
    assert(out->size == sizeof extended && out->version == 1);
    for (unit = 0; unit < 16; ++unit) assert(extended.tail[unit] == 0xa5);
}

int main(int argc, char **argv) {
    assert(argc == 2);
    shared_vectors(argv[1]);
    arguments();
    states_units_and_extended_storage();
    return 0;
}
