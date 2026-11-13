#ifndef OPENXCVR_OPENXCVR_H
#define OPENXCVR_OPENXCVR_H

#include <stddef.h>
#include <stdint.h>

#include "openxcvr/status.h"
#include "openxcvr_generated.h"

#ifdef __cplusplus
extern "C" {
#endif

/* SFF-8024 byte-0 / byte-1 dispatch, matching openxcvr::unified::SpecKind. */
typedef enum openxcvr_spec_kind {
    OPENXCVR_SPEC_SFF8472 = 1,
    OPENXCVR_SPEC_SFF8636 = 2,
    OPENXCVR_SPEC_CMIS = 3
} openxcvr_spec_kind_t;

/*
 * Classify an SFF-8024 identifier (byte 0) and revision (byte 1).
 * Mirrors openxcvr::unified::dispatch_identifier. On refusal *out is
 * left untouched. A NULL out is OPENXCVR_ERR_INVALID_ARGUMENT.
 */
openxcvr_status openxcvr_classify_identifier(
    uint8_t identifier,
    uint8_t revision,
    openxcvr_spec_kind_t *out
);

/* Seven-bit I2C address; offsets are absolute within the 256-byte window. */
typedef struct openxcvr_range {
    uint8_t address;
    uint8_t bank;
    uint8_t page;
    uint8_t offset;
    uint16_t len;
} openxcvr_range;

typedef struct openxcvr_range_observation {
    openxcvr_range range;
    const uint8_t *bytes;
    size_t len;
    int present;
} openxcvr_range_observation;

typedef struct openxcvr_identity {
    char vendor_name[17];
    char vendor_pn[17];
    /* This ABI retains at most two characters, including for SFF-8472. */
    char vendor_rev[3];
    char vendor_sn[17];
    char date_code[9];
    uint8_t oui[3];
    openxcvr_spec_kind_t family;
    uint8_t revision;
} openxcvr_identity;

/*
 * Pure supplied-input APIs, not permission to read the returned footprint.
 * CMIS administrative identity supports revisions 0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54
 * (3.0, 4.0, 5.0, 5.1, 5.2, 5.3, 5.4).
 * SFF identity layouts are the existing schema layouts (8636 2.11, 8472 12.5a); revision is retained as metadata,
 * not interpreted as a CMIS-style BCD revision for those families.
 * required_ranges returns six field ranges, in name/PN/rev/SN/date/OUI order.
 * NULL out with capacity zero is a size query. On insufficient capacity,
 * *count is set but out is unchanged. Other errors leave outputs unchanged.
 */
openxcvr_status openxcvr_identity_required_ranges(
    openxcvr_spec_kind_t family, uint8_t revision,
    openxcvr_range *out, size_t capacity, size_t *count
);

/*
 * Accepts sparse fields, byte fragments or enclosing full pages. Missing or
 * absent required bytes return SHORT_BUFFER; invalid ranges, length mismatch,
 * overlapping present observations in the identity window or unsupported
 * family return INVALID_ARGUMENT. Unsupported CMIS revisions return
 * UNSUPPORTED_REVISION. Generated field accessors return INSUFFICIENT_INPUT
 * for fields outside their supplied span. Other windows are structurally
 * validated but ignored. All errors leave *out unchanged. No allocation or I/O.
 */
openxcvr_status openxcvr_decode_identity_ranges(
    openxcvr_spec_kind_t family, uint8_t revision,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_identity *out
);

typedef struct openxcvr_cmis_pages {
    uint8_t lower_raw[128];
    uint8_t page00h_raw[128];
    uint8_t page01h_raw[128];
    uint8_t page11h_raw[128];
    int has_page01h;
    int has_page11h;
    /*
     * VDM page set (CMIS § 8.27): descriptor pages 20h-23h, sample pages
     * 24h-27h, threshold quad pages 28h-2Bh, TC flag page 2Ch, TC mask page
     * 2Dh and the advertisement page 2Fh, in openxcvr_cmis_vdm_page_tags
     * order (2Eh is not part of the feature). Each slot carries a full
     * 128-byte page; has_vdm_page[i] marks the supplied ones. The `vdm`
     * JSON block is emitted only when every page of the set is supplied —
     * a partial set yields no block, never one fabricated from zero-filled
     * missing pages (fail closed).
     */
    uint8_t vdm_raw[15][128];
    int has_vdm_page[15];
    openxcvr_cmis_lower_memory lower;
    openxcvr_cmis_page00h_administrative page00h;
    openxcvr_cmis_page01h_advertising page01h;
    openxcvr_cmis_page11h_lane_status page11h;
} openxcvr_cmis_pages;

#define OPENXCVR_CMIS_VDM_PAGE_COUNT 15

/* VDM page tags ("20h".."2fh", 2Eh excluded) indexing vdm_raw slots. */
extern const char *const openxcvr_cmis_vdm_page_tags[OPENXCVR_CMIS_VDM_PAGE_COUNT];

/*
 * Supplies one VDM page by tag ("20h".."2fh"). buf must hold at least 128
 * bytes (SHORT_BUFFER otherwise); an unknown tag is INVALID_ARGUMENT. Only
 * marks the slot supplied — the `vdm` block still requires the complete set
 * (openxcvr_cmis_pages_vdm_complete).
 */
openxcvr_status openxcvr_cmis_pages_set_vdm(
    openxcvr_cmis_pages *pages,
    const char *tag,
    const uint8_t *buf,
    size_t len
);

/* Non-zero when every VDM page of the set is supplied. */
int openxcvr_cmis_pages_vdm_complete(const openxcvr_cmis_pages *pages);

openxcvr_status openxcvr_cmis_pages_load(
    const char *fixture_dir,
    const char *stem,
    openxcvr_cmis_pages *out
);

openxcvr_status openxcvr_decode_cmis_json(
    const openxcvr_cmis_pages *pages,
    char *buf,
    size_t len,
    size_t *written
);

/*
 * Revision-qualified CMIS Data Path State Machine terminal-state predicate,
 * mirroring openxcvr_cmis::adapter::dpsm::DpState::is_terminal_for_rev.
 *
 * Admitted catalog revisions (Lower memory byte 1): 0x30 (3.0), 0x40 (4.0),
 * 0x50 (5.0), 0x51 (5.1), 0x52 (5.2), 0x53 (5.3), 0x54 (5.4) — the same seven bytes the
 * generated DPSM name projection binds. Any other cmis_rev_byte returns
 * OPENXCVR_ERR_UNSUPPORTED_REVISION without modifying *is_terminal_out.
 *
 * raw_state is the 4-bit DataPathState nibble (0x00..=0x0f) taken straight
 * from the Page 11h dp_state_host_lane bytes. If raw_state > 0x0f or
 * is_terminal_out is NULL, returns OPENXCVR_ERR_INVALID_ARGUMENT without
 * modifying *is_terminal_out (argument errors precede revision refusal).
 *
 * Terminal sets — the same ones the Rust adapter implements:
 * - CMIS 3.0 (0x30): DP_ACTIVATED (0x04) only. The 3.0 machine has four
 *   states and no Tx parking state — the Tx controls never withhold
 *   DataPathActivated — and dp_init_pending does not exist. Reserved
 *   nibbles (0x00, 0x05..=0x0f) are never terminal, and tx_disabled /
 *   dp_init_pending do not make 0x07 terminal there (it is reserved at 3.0).
 * - CMIS 4.0 (0x40): DP_ACTIVATED, plus DP_INITIALIZED (0x07) while
 *   tx_disabled != 0 (DPInitialized is a steady state while DPDeactivateS
 *   stays TRUE). Page 11h byte 235 is Reserved in CMIS 4.0, so
 *   dp_init_pending is ignored at 0x40.
 * - CMIS 5.0-5.4 (0x50..=0x54): DP_ACTIVATED, plus DP_INITIALIZED while
 *   tx_disabled != 0 or under the host/adapter deferred-provisioning
 *   operational acceptance policy when dp_init_pending != 0.
 *
 * Every other state — DP_DEACTIVATED (0x01), DP_INIT, DP_DEINIT,
 * DP_TX_TURN_ON, DP_TX_TURN_OFF, reserved nibbles — is transient or
 * reserved at every revision and never terminal.
 */
openxcvr_status openxcvr_cmis_dpsm_is_terminal_for_rev(
    uint8_t cmis_rev_byte,
    uint8_t raw_state,
    int tx_disabled,
    int dp_init_pending,
    int *is_terminal_out
);

/*
 * Expected active terminal state after a DPSM activation walk for a given
 * CMIS revision and lane conditions, mirroring
 * openxcvr_cmis::adapter::dpsm::DpState::expected_active_terminal_for_rev.
 *
 * - CMIS 3.0: DP_ACTIVATED (regardless of tx_disabled — the 3.0 machine has
 *   no Tx parking state).
 * - CMIS 4.0: DP_INITIALIZED when Tx is held off, else DP_ACTIVATED.
 * - CMIS 5.0-5.4: DP_INITIALIZED when Tx is held off or init is pending,
 *   else DP_ACTIVATED.
 *
 * The returned state always satisfies openxcvr_cmis_dpsm_is_terminal_for_rev
 * under the same flags. Returns OPENXCVR_ERR_INVALID_ARGUMENT if state_out
 * is NULL, or OPENXCVR_ERR_UNSUPPORTED_REVISION if cmis_rev_byte is outside
 * {0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54} (leaving *state_out unmodified on
 * error).
 */
openxcvr_status openxcvr_cmis_dpsm_expected_active_terminal_for_rev(
    uint8_t cmis_rev_byte,
    int tx_disabled,
    int dp_init_pending,
    openxcvr_cmis_data_path_state *state_out
);

/* SFF-8636 (QSFP) aggregate decode input: the 128-byte lower memory page
 * and the 128-byte Page 00h upper administrative page, as raw bytes plus
 * the generated decoded structs. */
typedef struct openxcvr_sff8636_pages {
    uint8_t lower_raw[128];
    uint8_t page00h_raw[128];
    openxcvr_sff8636_lower_memory lower;
    openxcvr_sff8636_page00h_upper_administrative page00h;
} openxcvr_sff8636_pages;

openxcvr_status openxcvr_sff8636_pages_load(
    const char *fixture_dir,
    const char *stem,
    openxcvr_sff8636_pages *out
);

/* Aggregate JSON decoder for SFF-8636 (schema revision 2.11): mirrors
 * openxcvr_decode_cmis_json's status codes and two-pass buffer semantics
 * (buf NULL with len 0 sizes; *written includes the NUL terminator).
 * Values come from the generated structs and generated supplied readers;
 * only the checksums and the raw_bytes_page_00h_3_10 range read the raw
 * buffers. This reproduces the aggregate golden contract, not the
 * revision-qualified facade of. */
openxcvr_status openxcvr_decode_sff8636_json(
    const openxcvr_sff8636_pages *pages,
    char *buf,
    size_t len,
    size_t *written
);

/* SFF-8472 (SFP) aggregate decode input: the 256-byte A0h base memory and
 * A2h diagnostics pages, as raw bytes plus the generated decoded structs.
 * The loader zero-pads short files, like the Python _pad_page helper. */
typedef struct openxcvr_sff8472_pages {
    uint8_t a0h_raw[256];
    uint8_t a2h_raw[256];
    openxcvr_page_a0h_base_memory a0h;
    openxcvr_page_a2h_diagnostics_base a2h;
} openxcvr_sff8472_pages;

openxcvr_status openxcvr_sff8472_pages_load(
    const char *fixture_dir,
    const char *stem,
    openxcvr_sff8472_pages *out
);

/* Aggregate JSON decoder for SFF-8472 (schema revision 12.5a): mirrors
 * openxcvr_decode_cmis_json's status codes and two-pass buffer semantics
 * (buf NULL with len 0 sizes; *written includes the NUL terminator).
 * Values come from the generated structs and generated supplied readers
 * (A2h telemetry, status_control, wavelength); only the checksums and the
 * raw_bytes_a0h_3_10 range read the raw buffers. When A0h byte 92 bit 6
 * (digital_diagnostics_implemented) is clear, flags, monitoring and lanes
 * are emitted as null. This reproduces the aggregate golden contract, not
 * the revision-qualified facade of.
 *
 * Telemetry in this result is uncalibrated: SFF-8472 external calibration
 * constants are not applied. Check calibration.externally_calibrated;
 * calibrated values come from the Rust SFF-8472 adapter or the observation
 * APIs. */
openxcvr_status openxcvr_decode_sff8472_json(
    const openxcvr_sff8472_pages *pages,
    char *buf,
    size_t len,
    size_t *written
);

#ifdef __cplusplus
}
#endif

#endif
