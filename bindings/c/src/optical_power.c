/* Native counterpart of openxcvr_core::observation::optical_power_dbm.
 * Mathematical presentation only: no acquisition or protocol decoding. */
#include "openxcvr/observation.h"
#include <math.h>

openxcvr_status openxcvr_optical_power_dbm(
    const openxcvr_observation_t *source, openxcvr_optical_dbm_t *out
) {
    openxcvr_optical_dbm_t result = {0};
    double watts;
    if (!source || !out || out->size < sizeof result ||
        out->version != OPENXCVR_OPTICAL_DBM_VERSION)
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    result.size = out->size;
    result.version = out->version;
    result.source = source;
    result.state = source->state;
    result.kind = OPENXCVR_OPTICAL_DBM_UNAVAILABLE;
    if (source->state != OPENXCVR_OBS_VALID) {
        *out = result;
        return OPENXCVR_OK;
    }
    if (!source->has_value || !source->value.is_scaled ||
        source->value.unit != OPENXCVR_UNIT_WATT) {
        result.kind = OPENXCVR_OPTICAL_DBM_NOT_CONVERTIBLE;
        *out = result;
        return OPENXCVR_OK;
    }
    watts = source->value.value;
    if (!isfinite(watts) || watts < 0.0) {
        result.state = OPENXCVR_OBS_INVALID;
    } else if (watts == 0.0) {
        result.kind = OPENXCVR_OPTICAL_DBM_NO_POWER;
    } else {
        double dbm = 10.0 * (log10(watts) + 3.0);
        if (isfinite(dbm)) {
            result.kind = OPENXCVR_OPTICAL_DBM_VALUE;
            result.dbm = dbm;
        } else {
            result.state = OPENXCVR_OBS_INVALID;
        }
    }
    *out = result;
    return OPENXCVR_OK;
}
