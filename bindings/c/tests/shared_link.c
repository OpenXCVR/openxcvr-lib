#include "openxcvr/openxcvr.h"

/*
 * Linked into a shared object against libopenxcvr_c. A non-PIC static
 * archive fails this link on x86-64 (-fPIC relocation against a local
 * symbol). The call is real so the linker cannot drop the reference.
 */
openxcvr_status openxcvr_shared_link_classify(uint8_t identifier, uint8_t revision, openxcvr_spec_kind_t *out) {
    return openxcvr_classify_identifier(identifier, revision, out);
}
