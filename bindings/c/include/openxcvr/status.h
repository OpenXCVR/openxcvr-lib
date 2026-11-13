#ifndef OPENXCVR_STATUS_H
#define OPENXCVR_STATUS_H

#ifdef __cplusplus
extern "C" {
#endif

typedef enum openxcvr_status {
    OPENXCVR_OK = 0,
    OPENXCVR_ERR_INVALID_ARGUMENT = 1,
    OPENXCVR_ERR_SHORT_BUFFER = 2,
    OPENXCVR_ERR_IO = 3,
    OPENXCVR_ERR_INSUFFICIENT_INPUT = 4,
    OPENXCVR_ERR_UNSUPPORTED_REVISION = 5,
    /* Catalog-resolution refusals (P-OBS basic/catalog). */
    OPENXCVR_ERR_NO_MATCH = 6,
    OPENXCVR_ERR_AMBIGUOUS = 7,
    OPENXCVR_ERR_OUT_OF_RANGE = 8
} openxcvr_status;

const char *openxcvr_status_string(openxcvr_status status);

#ifdef __cplusplus
}
#endif

#endif
