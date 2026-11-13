// C++ linkage smoke for both revision-checked CMIS page APIs: a C++
// translation unit includes the umbrella header (whose generated revision
// declarations carry extern "C" guards) and calls both decode functions.
// The library itself stays C-only; C++ is enabled only for this test target.
#include "openxcvr/openxcvr.h"

#include <cstdio>
#include <cstring>

int main() {
    unsigned char buf[128];
    std::memset(buf, 0xFF, sizeof(buf));

    openxcvr_cmis_page01h_advertising_checked a{};
    if (openxcvr_decode_cmis_page01h_advertising_for_revision(
            0x53, buf, sizeof(buf), &a) != OPENXCVR_OK) {
        std::fprintf(stderr, "01h decode failed from C++\n");
        return 1;
    }
    if (a.cmis_rev_byte != 0x53 || !a.present.password_entry_supported ||
        a.values.password_entry_supported != 3) {
        std::fprintf(stderr, "01h C++ results wrong\n");
        return 1;
    }

    openxcvr_cmis_page11h_lane_status_checked s{};
    if (openxcvr_decode_cmis_page11h_lane_status_for_revision(
            0x30, buf, sizeof(buf), &s) != OPENXCVR_OK) {
        std::fprintf(stderr, "11h decode failed from C++\n");
        return 1;
    }
    if (!s.present.dp_state_host_lane || s.values.dp_state_host_lane[0] != 15 ||
        s.present.dp_init_pending_lane) {
        std::fprintf(stderr, "11h C++ results wrong\n");
        return 1;
    }
    std::puts("C++ linkage: both checked page APIs called through installed-style umbrella headers");
    return 0;
}
