#include "openxcvr/application_records.h"

/* The bridge owns decoding, strings and allocation. Do not duplicate its
 * protocol logic in C. Keep public symbol names independent of Rust internals. */
extern int32_t openxcvr_rich_application_records_impl(
    const openxcvr_application_request *, const openxcvr_application_result **);
extern void openxcvr_rich_application_records_free_impl(
    const openxcvr_application_result *);

openxcvr_status openxcvr_application_records(
    const openxcvr_application_request *request,
    const openxcvr_application_result **out) {
    return (openxcvr_status)openxcvr_rich_application_records_impl(request, out);
}

void openxcvr_application_records_free(const openxcvr_application_result *result) {
    openxcvr_rich_application_records_free_impl(result);
}

extern int32_t openxcvr_rich_application_records_footprint_impl(
    const openxcvr_application_footprint_request *, const openxcvr_application_footprint_result **);
extern void openxcvr_rich_application_records_footprint_free_impl(
    const openxcvr_application_footprint_result *);
extern int32_t openxcvr_rich_application_records_from_ranges_impl(
    const openxcvr_application_sparse_request *, const openxcvr_application_result **);

openxcvr_status openxcvr_application_records_footprint(
    const openxcvr_application_footprint_request *request,
    const openxcvr_application_footprint_result **out) {
    return (openxcvr_status)openxcvr_rich_application_records_footprint_impl(request, out);
}
void openxcvr_application_records_footprint_free(const openxcvr_application_footprint_result *result) {
    openxcvr_rich_application_records_footprint_free_impl(result);
}
openxcvr_status openxcvr_application_records_from_ranges(
    const openxcvr_application_sparse_request *request,
    const openxcvr_application_result **out) {
    return (openxcvr_status)openxcvr_rich_application_records_from_ranges_impl(request, out);
}
