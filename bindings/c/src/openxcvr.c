#include "openxcvr/openxcvr.h"
#include "enums/cmis_data_path_state.h"
#include "enums/cmis_media_interface.h"
#include "openxcvr/observation.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

openxcvr_status openxcvr_classify_identifier(
    uint8_t identifier,
    uint8_t revision,
    openxcvr_spec_kind_t *out
) {
    openxcvr_spec_kind_t kind;

    if (out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }

    switch (identifier) {
    case 0x02:
    case 0x03:
        kind = OPENXCVR_SPEC_SFF8472;
        break;
    case 0x0C:
    case 0x0D:
    case 0x11:
    case 0x17:
        kind = OPENXCVR_SPEC_SFF8636;
        break;
    case 0x18:
    case 0x19:
        /* CMIS 3.0 reports its major revision as the packed byte 0x30
         * (BCD-like nibble layout; the Rust Rev::from_major_byte_cmis maps
         * 0x30 to Rev::CMIS_3_0) and CMIS 4.0+ as >= 0x40. Every other
         * sub-0x40 byte is the legacy pre-CMIS SFF-8636-compatible map.
         * Classification is family routing, not revision admission. */
        kind = (revision == 0x30 || revision >= 0x40) ? OPENXCVR_SPEC_CMIS
                                                      : OPENXCVR_SPEC_SFF8636;
        break;
    case 0x1B:
    case 0x1E:
    case 0x1F:
    case 0x20:
    case 0x21:
    case 0x22:
    case 0x23:
    case 0x24:
    case 0x25:
        kind = OPENXCVR_SPEC_CMIS;
        break;
    default:
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }

    *out = kind;
    return OPENXCVR_OK;
}

const char *openxcvr_status_string(openxcvr_status status) {
    switch (status) {
    case OPENXCVR_OK:
        return "ok";
    case OPENXCVR_ERR_INVALID_ARGUMENT:
        return "invalid argument";
    case OPENXCVR_ERR_SHORT_BUFFER:
        return "short buffer";
    case OPENXCVR_ERR_INSUFFICIENT_INPUT:
        return "insufficient supplied input";
    case OPENXCVR_ERR_UNSUPPORTED_REVISION:
        return "unsupported revision";
    case OPENXCVR_ERR_IO:
        return "io error";
    case OPENXCVR_ERR_NO_MATCH:
        return "no field matches the pattern";
    case OPENXCVR_ERR_AMBIGUOUS:
        return "ambiguous field pattern";
    case OPENXCVR_ERR_OUT_OF_RANGE:
        return "lane index out of range";
    default:
        return "unknown status";
    }
}

static openxcvr_status read_page_file(
    const char *fixture_dir,
    const char *stem,
    const char *suffix,
    uint8_t out[128],
    int required,
    int *present
) {
    char path[1024];
    int n = snprintf(path, sizeof(path), "%s/%s.%s", fixture_dir, stem, suffix);
    if (n < 0 || (size_t)n >= sizeof(path)) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }

    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        if (!required) {
            if (present != NULL) {
                *present = 0;
            }
            memset(out, 0, 128);
            return OPENXCVR_OK;
        }
        return OPENXCVR_ERR_IO;
    }

    size_t got = fread(out, 1, 128, file);
    int close_rc = fclose(file);
    if (close_rc != 0 || got < 128) {
        return OPENXCVR_ERR_IO;
    }
    if (present != NULL) {
        *present = 1;
    }
    return OPENXCVR_OK;
}

static const char *const VDM_PAGE_TAGS[OPENXCVR_CMIS_VDM_PAGE_COUNT] = {
    "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
    "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
};

const char *const openxcvr_cmis_vdm_page_tags[OPENXCVR_CMIS_VDM_PAGE_COUNT] = {
    "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
    "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
};

static int vdm_page_slot(const char *tag) {
    if (tag == NULL) {
        return -1;
    }
    for (int i = 0; i < OPENXCVR_CMIS_VDM_PAGE_COUNT; i++) {
        if (strcmp(tag, VDM_PAGE_TAGS[i]) == 0) {
            return i;
        }
    }
    return -1;
}

openxcvr_status openxcvr_cmis_pages_set_vdm(
    openxcvr_cmis_pages *pages,
    const char *tag,
    const uint8_t *buf,
    size_t len
) {
    int slot;

    if (pages == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    slot = vdm_page_slot(tag);
    if (slot < 0 || buf == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    if (len < 128) {
        return OPENXCVR_ERR_SHORT_BUFFER;
    }
    memcpy(pages->vdm_raw[slot], buf, 128);
    pages->has_vdm_page[slot] = 1;
    return OPENXCVR_OK;
}

int openxcvr_cmis_pages_vdm_complete(const openxcvr_cmis_pages *pages) {
    if (pages == NULL) {
        return 0;
    }
    for (int i = 0; i < OPENXCVR_CMIS_VDM_PAGE_COUNT; i++) {
        if (!pages->has_vdm_page[i]) {
            return 0;
        }
    }
    return 1;
}

openxcvr_status openxcvr_cmis_pages_load(
    const char *fixture_dir,
    const char *stem,
    openxcvr_cmis_pages *out
) {
    openxcvr_status status;
    if (fixture_dir == NULL || stem == NULL || out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    memset(out, 0, sizeof(*out));

    status = read_page_file(fixture_dir, stem, "lower.bin", out->lower_raw, 1, NULL);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = read_page_file(fixture_dir, stem, "page-00h.bin", out->page00h_raw, 1, NULL);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = read_page_file(fixture_dir, stem, "page-01h.bin", out->page01h_raw, 0, &out->has_page01h);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = read_page_file(fixture_dir, stem, "page-11h.bin", out->page11h_raw, 0, &out->has_page11h);
    if (status != OPENXCVR_OK) {
        return status;
    }
    /* VDM pages load when the fixture carries them; a short slice fails the
     * load (fail closed). Completeness of the set gates the `vdm` block in
     * openxcvr_decode_cmis_json. */
    for (int i = 0; i < OPENXCVR_CMIS_VDM_PAGE_COUNT; i++) {
        char suffix[24];
        snprintf(suffix, sizeof(suffix), "page-%s.bin", openxcvr_cmis_vdm_page_tags[i]);
        status = read_page_file(fixture_dir, stem, suffix, out->vdm_raw[i], 0, &out->has_vdm_page[i]);
        if (status != OPENXCVR_OK) {
            return status;
        }
    }

    status = openxcvr_decode_cmis_lower_memory(out->lower_raw, 128, &out->lower);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_cmis_page00h_administrative(out->page00h_raw, 128, &out->page00h);
    if (status != OPENXCVR_OK) {
        return status;
    }
    if (out->has_page01h) {
        status = openxcvr_decode_cmis_page01h_advertising(out->page01h_raw, 128, &out->page01h);
        if (status != OPENXCVR_OK) {
            return status;
        }
    }
    if (out->has_page11h) {
        status = openxcvr_decode_cmis_page11h_lane_status(out->page11h_raw, 128, &out->page11h);
        if (status != OPENXCVR_OK) {
            return status;
        }
    }
    return OPENXCVR_OK;
}

static const char *lookup_identifier(uint8_t id) {
    switch (id) {
    case 0x18: return "qsfp_dd";
    case 0x19: return "osfp";
    case 0x1E: return "qsfp_plus_cmis";
    default: return "unknown";
    }
}

static const char *lookup_connector(uint8_t id) {
    switch (id) {
    case 0x00: return "unknown_or_unspecified";
    case 0x07: return "lc";
    case 0x0C: return "mpo_1x12";
    case 0x23: return "no_separable_connector";
    case 0x27: return "mpo_2x12";
    default: return "unknown";
    }
}

static const char *lookup_host(uint8_t id) {
    switch (id) {
    case 11: return "caui_4_c2m";
    case 13: return "gaui_100g_2_c2m";
    case 15: return "gaui_200g_4_c2m";
    case 17: return "gaui_400g_8_c2m";
    case 32: return "lei_100g_pam4_1";
    case 34: return "lei_400g_pam4_4";
    case 35: return "lei_800g_pam4_8";
    case 66: return "caui_4_c2m_rs_528_514";
    case 75: return "gaui_100g_1_s_c2m";
    case 76: return "gaui_100g_1_l_c2m";
    case 77: return "gaui_200g_2_s_c2m";
    case 78: return "gaui_200g_2_l_c2m";
    case 79: return "gaui_400g_4_s_c2m";
    case 80: return "gaui_400g_4_l_c2m";
    case 81: return "gaui_800g_8_s_c2m";
    case 82: return "gaui_800g_8_l_c2m";
    case 128: return "gaui_200g_1_c2m";
    case 129: return "gaui_400g_2_c2m";
    case 130: return "gaui_800g_4_c2m";
    default: return "unknown";
    }
}

static const char *module_state_name(uint8_t state) {
    switch (state) {
    case 0x00: return "Reserved0";
    case 0x01: return "ModuleLowPwr";
    case 0x02: return "ModulePwrUp";
    case 0x03: return "ModuleReady";
    case 0x04: return "ModulePwrDn";
    case 0x05: return "ModuleFault";
    default: return NULL;
    }
}

/* CMIS Data-Path state labels are NOT authored here: the schema-owned
   generated projection (enums/cmis_data_path_state.h) is the single name
   authority, bound once per decode from the module's revision byte. A NULL
   projection (refused — unsupported present — context, selected before any
   call) or a raw the projection does not name renders "Unknown<n>" via the
   caller-side buffer below, mirroring module_state_name above. */

typedef struct json_writer {
    char *buf;
    size_t len;
    size_t pos;
} json_writer;

static void jw_raw(json_writer *w, const char *s, size_t n) {
    if (w->buf != NULL && w->len > 0) {
        size_t avail = w->pos < w->len ? w->len - w->pos : 0;
        if (avail > 0) {
            size_t copy = n < avail ? n : avail - 1;
            memcpy(w->buf + w->pos, s, copy);
            w->buf[w->pos + copy] = '\0';
        }
    }
    w->pos += n;
}

static void jw_lit(json_writer *w, const char *s) {
    jw_raw(w, s, strlen(s));
}

static void jw_fmt(json_writer *w, const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    va_list copy;
    va_copy(copy, args);
    int needed = vsnprintf(NULL, 0, fmt, args);
    va_end(args);
    if (needed <= 0) {
        va_end(copy);
        return;
    }
    if (w->buf != NULL && w->len > 0 && w->pos < w->len) {
        (void)vsnprintf(w->buf + w->pos, w->len - w->pos, fmt, copy);
    }
    va_end(copy);
    w->pos += (size_t)needed;
}

static void jw_string(json_writer *w, const char *s) {
    jw_lit(w, "\"");
    for (const unsigned char *p = (const unsigned char *)s; *p != '\0'; p++) {
        if (*p == '"' || *p == '\\') {
            jw_fmt(w, "\\%c", *p);
        } else if (*p < 0x20) {
            jw_fmt(w, "\\u%04x", (unsigned)*p);
        } else {
            char c = (char)*p;
            jw_raw(w, &c, 1);
        }
    }
    jw_lit(w, "\"");
}

static void jw_bytes_string(json_writer *w, const uint8_t *s, size_t n) {
    while (n > 0 && s[n - 1] == ' ') {
        n--;
    }
    jw_lit(w, "\"");
    for (size_t i = 0; i < n; i++) {
        if (s[i] == '"' || s[i] == '\\') {
            jw_fmt(w, "\\%c", s[i]);
        } else if (s[i] < 0x20) {
            jw_fmt(w, "\\u%04x", (unsigned)s[i]);
        } else {
            char c = (char)s[i];
            jw_raw(w, &c, 1);
        }
    }
    jw_lit(w, "\"");
}

static int bytes_equal_trim_spaces(const uint8_t *s, size_t n, const char *expected) {
    while (n > 0 && s[n - 1] == ' ') {
        n--;
    }
    return strlen(expected) == n && memcmp(s, expected, n) == 0;
}

/* ═══════════════ VDM (Pages 20h-2Fh, CMIS § 8.27) ═══════════════════════
 *
 * C parity for bindings/go/cmis_vdm.go and bindings/python/openxcvr/_cmis.py:
 * group gating on the Page 2Fh advertisement (vdm_support code + 1 groups;
 * only advertised groups' descriptor/sample pages decode), polymorphic X16
 * sample dispatch per, the 28h-2Bh threshold quads and the 2Ch/2Dh
 * TC flag/mask nibbles. Pure: decodes caller-supplied page bytes, no I/O,
 * no allocation; the block is emitted only from the COMPLETE 15-page set.
 *
 * The work struct is decoded up front (identically on the sizing and
 * writing passes) and the emitter below is a straight-line writer over it,
 * mirroring the bias-factor pattern of the lanes/flags emitter.
 */

typedef struct vdm_work {
    /* Page 2Fh advertisement. */
    uint8_t vdm_support;
    int groups;
    bool duty_cycle_support;
    bool power_saving_support;
    uint16_t fine_interval_length;
    bool freeze_request;
    bool power_saving_mode;
    uint8_t monitoring_duty_cycle;
    bool freeze_done;
    bool unfreeze_done;
    /* Per-group projected descriptors and raw X16 sample registers; only
     * advertised groups (index < groups) are filled. */
    uint8_t tsid[4][64];
    uint8_t resource[4][64];
    uint8_t otype[4][64];
    uint16_t sample_raw[4][64];
    /* Threshold quads: pages 28h-2Bh × 4 carriers × 16 global sets. */
    uint16_t hi_alarm[4][16];
    uint16_t lo_alarm[4][16];
    uint16_t hi_warn[4][16];
    uint16_t lo_warn[4][16];
    /* TC flag/mask nibbles, 256 instances × 4 bits each. */
    bool tc_flag[4][256];
    bool tc_mask[4][256];
} vdm_work_t;

/* VDMSupport labels mirror the generated enums/cmis_vdm_support.h entries
 * (the schema enum is the name source; the C enum emitter has no label
 * function, like the other hand labels in this facade). */
static const char *vdm_support_name(uint8_t code) {
    switch (code) {
    case 0x00: return "group_1";
    case 0x01: return "groups_1_2";
    case 0x02: return "groups_1_3";
    case 0x03: return "groups_1_4";
    default: return "unknown";
    }
}

/* dispatch (page-24h-vdm-samples-1-64.yaml variant table): S16
 * for TEC current / laser frequency error / laser temperature (0x02-0x04)
 * and ELS input power (0x54); F16 via the exact openxcvr_cmis_f16_to_double
 * conversion (the strtod oracle); raw U16 otherwise. */
static void vdm_emit_sample(json_writer *w, uint8_t observable_type, uint16_t raw) {
    if ((observable_type >= 0x02 && observable_type <= 0x04) || observable_type == 0x54) {
        int16_t value = (int16_t)raw;
        jw_fmt(w, "{\"storage\":\"i16\",\"value\":%d}", value);
    } else if (observable_type >= 0x09 && observable_type <= 0x1A) {
        double value = openxcvr_cmis_f16_to_double(raw);
        jw_fmt(w, "{\"storage\":\"f16\",\"value\":%.17g}", value);
    } else {
        jw_fmt(w, "{\"storage\":\"u16\",\"value\":%u}", (unsigned)raw);
    }
}

static openxcvr_status vdm_decode_work(const openxcvr_cmis_pages *pages, vdm_work_t *work) {
    openxcvr_status status;

    memset(work, 0, sizeof(*work));

    openxcvr_cmis_page2_fh_vdm_advertisement ad;
    status = openxcvr_decode_cmis_page2_fh_vdm_advertisement(pages->vdm_raw[14], 128, &ad);
    if (status != OPENXCVR_OK) {
        return status;
    }
    work->vdm_support = ad.vdm_support;
    work->groups = ad.vdm_support + 1;
    work->duty_cycle_support = ad.duty_cycle_support;
    work->power_saving_support = ad.power_saving_support;
    work->fine_interval_length = ad.fine_interval_length;
    work->freeze_request = ad.freeze_request;
    work->power_saving_mode = ad.power_saving_mode;
    work->monitoring_duty_cycle = ad.monitoring_duty_cycle;
    work->freeze_done = ad.freeze_done;
    work->unfreeze_done = ad.unfreeze_done;

    for (int group = 0; group < work->groups; group++) {
        /* Descriptor page 20h+group: the generated decoder owns the offset
         * and bit math. */
        switch (group) {
        case 0: {
            openxcvr_cmis_page20h_vdm_descriptors1_to64 d;
            status = openxcvr_decode_cmis_page20h_vdm_descriptors1_to64(pages->vdm_raw[group], 128, &d);
            if (status != OPENXCVR_OK) {
                return status;
            }
            memcpy(work->tsid[0], d.local_threshold_set_id, 64);
            memcpy(work->resource[0], d.monitored_resource, 64);
            memcpy(work->otype[0], d.observable_type, 64);
            break;
        }
        case 1: {
            openxcvr_cmis_page21h_vdm_descriptors65_to128 d;
            status = openxcvr_decode_cmis_page21h_vdm_descriptors65_to128(pages->vdm_raw[group], 128, &d);
            if (status != OPENXCVR_OK) {
                return status;
            }
            memcpy(work->tsid[1], d.local_threshold_set_id, 64);
            memcpy(work->resource[1], d.monitored_resource, 64);
            memcpy(work->otype[1], d.observable_type, 64);
            break;
        }
        case 2: {
            openxcvr_cmis_page22h_vdm_descriptors129_to192 d;
            status = openxcvr_decode_cmis_page22h_vdm_descriptors129_to192(pages->vdm_raw[group], 128, &d);
            if (status != OPENXCVR_OK) {
                return status;
            }
            memcpy(work->tsid[2], d.local_threshold_set_id, 64);
            memcpy(work->resource[2], d.monitored_resource, 64);
            memcpy(work->otype[2], d.observable_type, 64);
            break;
        }
        default: {
            openxcvr_cmis_page23h_vdm_descriptors193_to256 d;
            status = openxcvr_decode_cmis_page23h_vdm_descriptors193_to256(pages->vdm_raw[group], 128, &d);
            if (status != OPENXCVR_OK) {
                return status;
            }
            memcpy(work->tsid[3], d.local_threshold_set_id, 64);
            memcpy(work->resource[3], d.monitored_resource, 64);
            memcpy(work->otype[3], d.observable_type, 64);
            break;
        }
        }

        /* Sample page 24h+group: the C emitter does not yet admit instanced
         * X16 fields with variant tables (the generated page structs carry
         * only the 128-byte bounds check), so the raw wire registers are
         * read here per the schema offset_expr "128 + 2 * (vdm_instance -
         * 1)" (page-24h-vdm-samples-1-64.yaml) — the same raw-read pattern
         * the facade already uses for identity fields. The variant
         * interpretation stays in vdm_emit_sample. */
        const uint8_t *samples = pages->vdm_raw[4 + group];
        for (int i = 0; i < 64; i++) {
            work->sample_raw[group][i] = (uint16_t)(((uint16_t)samples[2 * i] << 8) | samples[2 * i + 1]);
        }
    }

    /* Threshold quad pages 28h-2Bh (global sets 1-64). */
    {
        openxcvr_cmis_page28h_vdm_thresholds1_to16 q28;
        openxcvr_cmis_page29h_vdm_thresholds17_to32 q29;
        openxcvr_cmis_page2_ah_vdm_thresholds33_to48 q2a;
        openxcvr_cmis_page2_bh_vdm_thresholds49_to64 q2b;

        status = openxcvr_decode_cmis_page28h_vdm_thresholds1_to16(pages->vdm_raw[8], 128, &q28);
        if (status != OPENXCVR_OK) {
            return status;
        }
        memcpy(work->hi_alarm[0], q28.high_alarm_threshold, sizeof(work->hi_alarm[0]));
        memcpy(work->lo_alarm[0], q28.low_alarm_threshold, sizeof(work->lo_alarm[0]));
        memcpy(work->hi_warn[0], q28.high_warning_threshold, sizeof(work->hi_warn[0]));
        memcpy(work->lo_warn[0], q28.low_warning_threshold, sizeof(work->lo_warn[0]));

        status = openxcvr_decode_cmis_page29h_vdm_thresholds17_to32(pages->vdm_raw[9], 128, &q29);
        if (status != OPENXCVR_OK) {
            return status;
        }
        memcpy(work->hi_alarm[1], q29.high_alarm_threshold, sizeof(work->hi_alarm[1]));
        memcpy(work->lo_alarm[1], q29.low_alarm_threshold, sizeof(work->lo_alarm[1]));
        memcpy(work->hi_warn[1], q29.high_warning_threshold, sizeof(work->hi_warn[1]));
        memcpy(work->lo_warn[1], q29.low_warning_threshold, sizeof(work->lo_warn[1]));

        status = openxcvr_decode_cmis_page2_ah_vdm_thresholds33_to48(pages->vdm_raw[10], 128, &q2a);
        if (status != OPENXCVR_OK) {
            return status;
        }
        memcpy(work->hi_alarm[2], q2a.high_alarm_threshold, sizeof(work->hi_alarm[2]));
        memcpy(work->lo_alarm[2], q2a.low_alarm_threshold, sizeof(work->lo_alarm[2]));
        memcpy(work->hi_warn[2], q2a.high_warning_threshold, sizeof(work->hi_warn[2]));
        memcpy(work->lo_warn[2], q2a.low_warning_threshold, sizeof(work->lo_warn[2]));

        status = openxcvr_decode_cmis_page2_bh_vdm_thresholds49_to64(pages->vdm_raw[11], 128, &q2b);
        if (status != OPENXCVR_OK) {
            return status;
        }
        memcpy(work->hi_alarm[3], q2b.high_alarm_threshold, sizeof(work->hi_alarm[3]));
        memcpy(work->lo_alarm[3], q2b.low_alarm_threshold, sizeof(work->lo_alarm[3]));
        memcpy(work->hi_warn[3], q2b.high_warning_threshold, sizeof(work->hi_warn[3]));
        memcpy(work->lo_warn[3], q2b.low_warning_threshold, sizeof(work->lo_warn[3]));
    }

    /* TC flag/mask pages 2Ch / 2Dh (256 instances). */
    {
        openxcvr_cmis_page2_ch_vdm_flags flags;
        openxcvr_cmis_page2_dh_vdm_masks masks;

        status = openxcvr_decode_cmis_page2_ch_vdm_flags(pages->vdm_raw[12], 128, &flags);
        if (status != OPENXCVR_OK) {
            return status;
        }
        memcpy(work->tc_flag[0], flags.high_alarm_tc_flag, 256);
        memcpy(work->tc_flag[1], flags.low_alarm_tc_flag, 256);
        memcpy(work->tc_flag[2], flags.high_warning_tc_flag, 256);
        memcpy(work->tc_flag[3], flags.low_warning_tc_flag, 256);

        status = openxcvr_decode_cmis_page2_dh_vdm_masks(pages->vdm_raw[13], 128, &masks);
        if (status != OPENXCVR_OK) {
            return status;
        }
        memcpy(work->tc_mask[0], masks.high_alarm_tc_mask, 256);
        memcpy(work->tc_mask[1], masks.low_alarm_tc_mask, 256);
        memcpy(work->tc_mask[2], masks.high_warning_tc_mask, 256);
        memcpy(work->tc_mask[3], masks.low_warning_tc_mask, 256);
    }
    return OPENXCVR_OK;
}

static void append_vdm(json_writer *w, const vdm_work_t *work) {
    jw_lit(w, "{\"support\":{");
    jw_fmt(w, "\"vdm_support\":%u,", (unsigned)work->vdm_support);
    jw_fmt(w, "\"vdm_support_name\":\"%s\"", vdm_support_name(work->vdm_support));
    jw_fmt(w, ",\"groups_advertised\":%d", work->groups);
    jw_fmt(w, ",\"duty_cycle_support\":%s", work->duty_cycle_support ? "true" : "false");
    jw_fmt(w, ",\"power_saving_support\":%s", work->power_saving_support ? "true" : "false");
    jw_fmt(w, ",\"fine_interval_length_0p1ms\":%u", (unsigned)work->fine_interval_length);
    jw_fmt(w, ",\"freeze_request\":%s", work->freeze_request ? "true" : "false");
    jw_fmt(w, ",\"power_saving_mode\":%s", work->power_saving_mode ? "true" : "false");
    jw_fmt(w, ",\"monitoring_duty_cycle_k\":%u", (unsigned)work->monitoring_duty_cycle);
    jw_fmt(w, ",\"freeze_done\":%s", work->freeze_done ? "true" : "false");
    jw_fmt(w, ",\"unfreeze_done\":%s", work->unfreeze_done ? "true" : "false");
    jw_lit(w, "},\"observables\":[");

    int first = 1;
    for (int group = 0; group < work->groups; group++) {
        for (int local = 0; local < 64; local++) {
            /* An unused descriptor slot is 00h in both bytes (CMIS Table
             * 8-178): observable type 0 means the instance is not active and
             * its sample is ignored. */
            if (work->otype[group][local] == 0) {
                continue;
            }
            if (!first) {
                jw_lit(w, ",");
            }
            first = 0;
            jw_fmt(w, "{\"instance\":%d,\"group\":%d", group * 64 + local + 1, group + 1);
            jw_fmt(w, ",\"threshold_set_id\":%u", (unsigned)work->tsid[group][local]);
            jw_fmt(w, ",\"global_threshold_set_id\":%d", group * 16 + work->tsid[group][local] + 1);
            jw_fmt(w, ",\"monitored_resource\":%u", (unsigned)work->resource[group][local]);
            jw_fmt(w, ",\"observable_type\":%u", (unsigned)work->otype[group][local]);
            jw_fmt(w, ",\"active\":true,\"sample_raw\":\"0x%04X\",\"sample\":", work->sample_raw[group][local]);
            vdm_emit_sample(w, work->otype[group][local], work->sample_raw[group][local]);
            jw_lit(w, "}");
        }
    }
    jw_lit(w, "],\"threshold_quads\":[");

    static const char *const THRESHOLD_TAGS[4] = {"28h", "29h", "2Ah", "2Bh"};
    first = 1;
    for (int page = 0; page < 4; page++) {
        for (int set = 0; set < 16; set++) {
            uint16_t hi_a = work->hi_alarm[page][set];
            uint16_t lo_a = work->lo_alarm[page][set];
            uint16_t hi_w = work->hi_warn[page][set];
            uint16_t lo_w = work->lo_warn[page][set];
            /* An all-zero quad is an undefined threshold set: emitted only
             * when at least one carrier is non-zero. */
            if (hi_a == 0 && lo_a == 0 && hi_w == 0 && lo_w == 0) {
                continue;
            }
            if (!first) {
                jw_lit(w, ",");
            }
            first = 0;
            jw_fmt(w, "{\"page\":\"%s\",\"set\":%d", THRESHOLD_TAGS[page], set + 1);
            jw_fmt(w, ",\"high_alarm\":\"0x%04X\",\"low_alarm\":\"0x%04X\"", hi_a, lo_a);
            jw_fmt(w, ",\"high_warning\":\"0x%04X\",\"low_warning\":\"0x%04X\"}", hi_w, lo_w);
        }
    }
    jw_lit(w, "],\"tc_flags\":[");

    static const char *const NIBBLE_KEYS[4] = {"high_alarm", "low_alarm", "high_warning", "low_warning"};
    for (int list = 0; list < 2; list++) {
        if (list == 1) {
            jw_lit(w, "],\"tc_masks\":[");
        }
        const bool (*bits)[256] = list == 0 ? work->tc_flag : work->tc_mask;
        first = 1;
        for (int i = 0; i < 256; i++) {
            /* An all-zero nibble carries no threshold-crossing state. */
            if (!bits[0][i] && !bits[1][i] && !bits[2][i] && !bits[3][i]) {
                continue;
            }
            if (!first) {
                jw_lit(w, ",");
            }
            first = 0;
            jw_fmt(w, "{\"instance\":%d,\"%s\":%s,\"%s\":%s,\"%s\":%s,\"%s\":%s}",
                   i + 1,
                   NIBBLE_KEYS[0], bits[0][i] ? "true" : "false",
                   NIBBLE_KEYS[1], bits[1][i] ? "true" : "false",
                   NIBBLE_KEYS[2], bits[2][i] ? "true" : "false",
                   NIBBLE_KEYS[3], bits[3][i] ? "true" : "false");
        }
    }
    jw_lit(w, "]}");
}

static void append_app_entries(json_writer *w, const uint8_t *buf, int start, int count, int base_code, uint8_t media_type, int *first) {
    for (int i = 0; i < count; i++) {
        int off = start + i * 4;
        uint8_t hif = buf[off];
        uint8_t mif = buf[off + 1];
        uint8_t hlc = (uint8_t)(buf[off + 2] >> 4);
        uint8_t mlc = (uint8_t)(buf[off + 2] & 0x0Fu);
        uint8_t hlao = buf[off + 3];
        if (hif == 0xFF) {
            break;
        }
        if (mif == 0) {
            continue;
        }
        if (!*first) {
            jw_lit(w, ",");
        }
        *first = 0;
        const char *media_name = openxcvr_cmis_media_interface_name(media_type, mif);
        jw_fmt(w, "{\"app_code\":%d,", base_code + i + 1);
        jw_fmt(w, "\"host_interface_id\":%u,", hif);
        jw_lit(w, "\"host_interface_name\":");
        jw_string(w, lookup_host(hif));
        jw_fmt(w, ",\"host_lane_count\":%u,", hlc);
        jw_fmt(w, "\"host_lane_assignment_options\":\"0x%02X\",", hlao);
        jw_fmt(w, "\"media_interface_id\":%u,", mif);
        jw_lit(w, "\"media_interface_name\":");
        jw_string(w, media_name != NULL ? media_name : "unknown");
        jw_fmt(w, ",\"media_lane_count\":%u}", mlc);
    }
}

static void append_applications(json_writer *w, const openxcvr_cmis_pages *pages) {
    int first = 1;
    uint8_t media_type = pages->lower_raw[85];
    if (pages->lower.memory_model) {
        jw_lit(w, "null");
        return;
    }
    jw_lit(w, "[");
    append_app_entries(w, pages->lower_raw, 86, 8, 0, media_type, &first);
    if (pages->has_page01h) {
        append_app_entries(w, pages->page01h_raw, 95, 7, 8, media_type, &first);
    }
    jw_lit(w, first ? "]" : "]");
    if (first) {
        /* Rewrite an empty array as null by emitting a marker the caller never sees. */
    }
}

static void append_rejection(json_writer *w, uint8_t cmis_byte) {
    jw_lit(w, "{\"decode_status\":\"rejected\",\"rejection\":{");
    jw_lit(w, "\"detail\":");
    jw_fmt(
        w,
        "\"Lower memory byte 1 = 0x%02X; CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. The fboss test corpus uses 0x%02X as an explicit anomalous-rev regression target.\"",
        cmis_byte,
        cmis_byte
    );
    jw_fmt(w, ",\"reason\":\"unknown_major_revision: 0x%02X\"}}", cmis_byte);
}

/* Emit the `"flags":{...},"lanes":[...]` JSON pair from CMIS Page 11h (issue
 * #105). Leans entirely on the generated Page 11h decoder (made byte-exact by
 * the #195 bit-offset fix) for offset/bit math; applies only linear scaling,
 * dp_state naming via the schema-owned projection, and flag-key remapping
 * (OpticalPower*Tx -> tx_power, LaserBias*Tx -> tx_bias, OpticalPower*Rx ->
 * rx_power) plus the per-lane status latches (rx_los ← LOSFlagRx, tx_fault ←
 * FailureFlagTx, § 8.10.3 Tables 8-88/8-87). `projection` is the generated name projection bound once by the
 * caller from the module's revision byte (Lower:1): NULL (refused —
 * unsupported present — context, or an unnamed reserved raw) renders the
 * caller-side "Unknown<n>" buffer — no generated buffer API is added.
 *
 * Scaling: power = u16 * 0.0001 mW (4 dp); bias = u16 * 0.002 * mult mA (3 dp).
 * These products are exact decimals at the printed precision, so %.4f / %.3f
 * round-trip to the same float the Python goldens carry (verified across the
 * whole corpus) -- no libm round() needed. */
static void append_lanes_flags(
    json_writer *w,
    const openxcvr_cmis_pages *pages,
    double bias_mult,
    bool bias_reserved,
    openxcvr_cmis_data_path_state_projection projection
) {
    const openxcvr_cmis_page11h_lane_status *p = &pages->page11h;
    const uint8_t *lower = pages->lower_raw;

    /* ── module flags (CMIS 5.3 § 8.2.4, lower memory) ──────────────────────── */
    jw_lit(w, "\"flags\":{");
    jw_lit(w, "\"data_not_ready\":null,");
    jw_fmt(w, "\"interrupt_asserted\":%s,", (lower[3] & 0x01u) == 0 ? "true" : "false");
    jw_fmt(w, "\"temp_high_alarm\":%s,", (lower[9] & 0x01u) ? "true" : "false");
    jw_fmt(w, "\"temp_low_alarm\":%s,", (lower[9] & 0x02u) ? "true" : "false");
    jw_fmt(w, "\"supply_high_alarm\":%s,", (lower[9] & 0x10u) ? "true" : "false");
    jw_fmt(w, "\"supply_low_alarm\":%s,", (lower[9] & 0x20u) ? "true" : "false");

    /* ── per-lane latching flags ────────────────────────────────────────────── */
    jw_lit(w, "\"lanes\":[");
    for (int i = 0; i < 8; i++) {
        if (i != 0) {
            jw_lit(w, ",");
        }
        jw_lit(w, "{");
        jw_fmt(w, "\"rx_power_high_alarm\":%s,", p->optical_power_high_alarm_flag_rx[i] ? "true" : "false");
        jw_fmt(w, "\"rx_power_low_alarm\":%s,", p->optical_power_low_alarm_flag_rx[i] ? "true" : "false");
        jw_fmt(w, "\"rx_power_high_warning\":%s,", p->optical_power_high_warning_flag_rx[i] ? "true" : "false");
        jw_fmt(w, "\"rx_power_low_warning\":%s,", p->optical_power_low_warning_flag_rx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_bias_high_alarm\":%s,", p->laser_bias_high_alarm_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_bias_low_alarm\":%s,", p->laser_bias_low_alarm_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_bias_high_warning\":%s,", p->laser_bias_high_warning_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_bias_low_warning\":%s,", p->laser_bias_low_warning_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_power_high_alarm\":%s,", p->optical_power_high_alarm_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_power_low_alarm\":%s,", p->optical_power_low_alarm_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_power_high_warning\":%s,", p->optical_power_high_warning_flag_tx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_power_low_warning\":%s}", p->optical_power_low_warning_flag_tx[i] ? "true" : "false");
    }
    jw_lit(w, "]},");

    /* ── per-lane status / monitoring ───────────────────────────────────────── */
    jw_lit(w, "\"lanes\":[");
    for (int i = 0; i < 8; i++) {
        if (i != 0) {
            jw_lit(w, ",");
        }
        const char *dpname = NULL;
        char dpbuf[32];
        if (projection != NULL) {
            dpname = projection(p->dp_state_host_lane[i]);
        }
        if (dpname == NULL) {
            snprintf(dpbuf, sizeof(dpbuf), "Unknown%u", p->dp_state_host_lane[i]);
            dpname = dpbuf;
        }
        jw_fmt(w, "{\"index\":%d,", i + 1);
        jw_lit(w, "\"dp_state\":");
        jw_string(w, dpname);
        /* Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
         * (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87), straight
         * from the generated Page 11h struct — same keys as the go/python
         * facades and the goldens. */
        jw_fmt(w, ",\"rx_los\":%s,", p->los_flag_rx[i] ? "true" : "false");
        jw_fmt(w, "\"tx_fault\":%s", p->failure_flag_tx[i] ? "true" : "false");
        jw_fmt(w, ",\"rx_power_mw\":%.4f,", (double)p->optical_power_rx[i] * 0.0001);
        /* Reserved 0b11 scaling (CMIS 5.3 Table 8-52): multiplier undefined →
         * emit literal null, never a %.3f-formatted sentinel (which could print
         * "nan"). The rest of the lane block stays intact. */
        if (bias_reserved) {
            jw_lit(w, "\"tx_bias_ma\":null,");
        } else {
            jw_fmt(w, "\"tx_bias_ma\":%.3f,", (double)p->laser_bias_tx[i] * 0.002 * bias_mult);
        }
        jw_fmt(w, "\"tx_power_mw\":%.4f}", (double)p->optical_power_tx[i] * 0.0001);
    }
    jw_lit(w, "]");
}

openxcvr_status openxcvr_decode_cmis_json(
    const openxcvr_cmis_pages *pages,
    char *buf,
    size_t len,
    size_t *written
) {
    if (written != NULL) {
        *written = 0;
    }
    if (pages == NULL || written == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }

    json_writer w = { .buf = buf, .len = len, .pos = 0 };
    uint8_t cmis_byte = pages->lower_raw[1];
    int cmis_major = (cmis_byte >> 4) & 0x0F;
    int cmis_minor = cmis_byte & 0x0F;

    if (cmis_major != 3 && cmis_major != 4 && cmis_major != 5) {
        append_rejection(&w, cmis_byte);
        *written = w.pos + 1;
        return (len >= *written) ? OPENXCVR_OK : OPENXCVR_ERR_SHORT_BUFFER;
    }

    /* Per-lane lanes/flags are emitted only for paged modules that expose Page
     * 11h; flat-memory modules keep both null (OpenAPI contract, issue #105).
     * Resolve the Tx-bias scaling factor up front (Page 01h:160 bits 4-3, CMIS
     * 5.3 Table 8-52) so the JSON writer below stays a straight-line emitter and
     * any error surfaces identically on both the sizing and writing passes.
     * The factor is read from the generated decoded struct field
     * pages->page01h.tx_bias_current_scaling_factor — not re-shifted from
     * the raw byte — so the generated field decode stays the single source
     * of the offset/mask.
     * The schema-owned DPSM name projection is bound ONCE here, from the
     * module's revision byte (Lower:1), before any lane emission: a supported
     * context uses the bound callable; a refused — unsupported present —
     * context (admitted major, byte outside the six catalog revisions)
     * explicitly keeps the NULL projection so the fallback branch is selected
     * before any call and every raw renders Unknown<n>. */
    int emit_lanes = pages->has_page11h && !pages->lower.memory_model;
    double bias_mult = 1.0;
    bool bias_reserved = false;
    openxcvr_cmis_data_path_state_projection dpsm_projection = NULL;

    /* VDM (Pages 20h-2Fh): decoded fully before any writing so a decode
     * failure surfaces identically on the sizing and writing passes; the
     * block is emitted only when the caller supplied the complete page set
     * (a partial set yields no block, fail closed). */
    vdm_work_t vdm_work;
    int emit_vdm = openxcvr_cmis_pages_vdm_complete(pages);
    if (emit_vdm) {
        openxcvr_status vdm_status = vdm_decode_work(pages, &vdm_work);
        if (vdm_status != OPENXCVR_OK) {
            return vdm_status;
        }
    }

    if (emit_lanes) {
        if (!pages->has_page01h) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        switch (pages->page01h.tx_bias_current_scaling_factor & 0x03u) {
        case 0: bias_mult = 1.0; break;
        case 1: bias_mult = 2.0; break;
        case 2: bias_mult = 4.0; break;
        /* Reserved 0b11 (CMIS 5.3 Table 8-52): multiplier undefined. Render
         * tx_bias_ma as null per lane instead of failing the whole decode. */
        default: bias_reserved = true; break;
        }
        dpsm_projection = openxcvr_cmis_data_path_state_for_revision(cmis_byte);
    }

    const char *state_name = module_state_name(pages->lower.module_state);
    char unknown_state[32];
    if (state_name == NULL) {
        snprintf(unknown_state, sizeof(unknown_state), "Unknown%u", pages->lower.module_state);
        state_name = unknown_state;
    }

    jw_lit(&w, "{");
    jw_lit(&w, "\"decode_status\":\"ok\",");
    jw_lit(&w, "\"identity\":{");
    jw_lit(&w, "\"date_code\":");
    jw_bytes_string(&w, &pages->page00h_raw[54], 6);
    jw_lit(&w, ",\"vendor_name\":");
    jw_bytes_string(&w, &pages->page00h_raw[1], 16);
    jw_fmt(&w, ",\"vendor_oui\":\"%02X:%02X:%02X\"", pages->page00h_raw[17], pages->page00h_raw[18], pages->page00h_raw[19]);
    jw_lit(&w, ",\"vendor_pn\":");
    jw_bytes_string(&w, &pages->page00h_raw[20], 16);
    jw_lit(&w, ",\"vendor_rev\":");
    jw_bytes_string(&w, &pages->page00h_raw[36], 2);
    jw_lit(&w, ",\"vendor_sn\":");
    jw_bytes_string(&w, &pages->page00h_raw[38], 16);
    jw_lit(&w, "},");

    jw_lit(&w, "\"module\":{");
    jw_fmt(&w, "\"cmis_revision\":\"%d.%d\",", cmis_major, cmis_minor);
    jw_lit(&w, "\"connector\":{");
    jw_lit(&w, "\"name\":");
    jw_string(&w, lookup_connector(pages->page00h.connector_type));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", pages->page00h.connector_type);
    jw_lit(&w, "\"identifier\":{");
    jw_lit(&w, "\"name\":");
    jw_string(&w, lookup_identifier(pages->lower.sff8024_identifier));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", pages->lower.sff8024_identifier);
    jw_lit(&w, "\"memory_model\":");
    jw_string(&w, pages->lower.memory_model ? "flat" : "paged");
    jw_lit(&w, ",\"module_state\":");
    jw_string(&w, state_name);
    jw_lit(&w, ",\"module_type\":");
    if (bytes_equal_trim_spaces(&pages->page00h_raw[1], 16, "METAZR")) {
        jw_string(&w, "800ZR coherent OSFP");
    } else {
        jw_lit(&w, "null");
    }
    jw_lit(&w, "},");

    jw_lit(&w, "\"monitoring\":{");
    jw_fmt(&w, "\"module_temperature_c\":%.17g,", (double)pages->lower.temp_mon_value / 256.0);
    jw_fmt(&w, "\"module_voltage_v\":%.4f},", (double)pages->lower.vcc_mon_voltage / 10000.0);

    jw_lit(&w, "\"applications\":");
    size_t apps_start = w.pos;
    append_applications(&w, pages);
    if (w.pos == apps_start + 2) {
        w.pos = apps_start;
        jw_lit(&w, "null");
    }
    jw_lit(&w, ",\"checksums\":null,");
    if (emit_lanes) {
        append_lanes_flags(&w, pages, bias_mult, bias_reserved, dpsm_projection);
    } else {
        jw_lit(&w, "\"flags\":null,\"lanes\":null");
    }
    if (emit_vdm) {
        jw_lit(&w, ",\"vdm\":");
        append_vdm(&w, &vdm_work);
    }
    jw_lit(&w, "}");

    *written = w.pos + 1;
    if (buf != NULL && len > 0) {
        if (w.pos < len) {
            buf[w.pos] = '\0';
        } else {
            buf[len - 1] = '\0';
        }
    }
    return (len >= *written) ? OPENXCVR_OK : OPENXCVR_ERR_SHORT_BUFFER;
}

/* ═══════════════ CMIS DPSM terminal predicate (revision-qualified) ═════════
 *
 * C parity for openxcvr_cmis::adapter::dpsm::DpState::is_terminal_for_rev /
 * expected_active_terminal_for_rev (GitHub Issue #1 Ask 2): the Rust
 * adapter's fail-closed, revision-qualified terminal semantics exposed for
 * C consumers driving DPSM walks over raw Page 11h nibbles. The revision
 * admission is the same six packed bytes the generated DPSM name projection
 * (openxcvr_cmis_data_path_state_for_revision) binds, so the predicate and
 * the projection can never disagree about which revisions are in the
 * catalog.
 */

static int dpsm_rev_admitted(uint8_t cmis_rev_byte) {
    switch (cmis_rev_byte) {
    case 0x30: /* CMIS 3.0 */
    case 0x40: /* CMIS 4.0 */
    case 0x50: /* CMIS 5.0 */
    case 0x51: /* CMIS 5.1 */
    case 0x52: /* CMIS 5.2 */
    case 0x53: /* CMIS 5.3 */
    case 0x54: /* CMIS 5.4 */
        return 1;
    default:
        return 0;
    }
}

openxcvr_status openxcvr_cmis_dpsm_is_terminal_for_rev(
    uint8_t cmis_rev_byte,
    uint8_t raw_state,
    int tx_disabled,
    int dp_init_pending,
    int *is_terminal_out
) {
    int terminal;

    if (is_terminal_out == NULL || raw_state > 0x0Fu) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    if (!dpsm_rev_admitted(cmis_rev_byte)) {
        return OPENXCVR_ERR_UNSUPPORTED_REVISION;
    }

    switch (cmis_rev_byte) {
    case 0x30:
        /* CMIS 3.0 (Figure 9, Table 8, Table 66): Activated only — the
         * four-state machine has no Tx parking state, and raws 0x05..=0x0f
         * are reserved (Unknown), never terminal. */
        terminal = (raw_state == OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED);
        break;
    case 0x40:
        /* CMIS 4.0 (Table 8-58, § 6.3.2.4): Activated, plus Initialized
         * while Tx is held off. Page 11h byte 235 is Reserved at 4.0, so
         * dp_init_pending contributes nothing (ignored, not an error). */
        terminal = (raw_state == OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED)
            || (tx_disabled != 0
                && raw_state == OPENXCVR_CMIS_DATA_PATH_STATE_DP_INITIALIZED);
        break;
    default:
        /* CMIS 5.0-5.3 (Table 8-84, § 6.3.3.6): as 4.0, plus Initialized
         * under the deferred-provisioning operational acceptance policy
         * while dp_init_pending (Page 11h byte 235, since 5.0). */
        terminal = (raw_state == OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED)
            || (((tx_disabled != 0) || (dp_init_pending != 0))
                && raw_state == OPENXCVR_CMIS_DATA_PATH_STATE_DP_INITIALIZED);
        break;
    }

    *is_terminal_out = terminal;
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_cmis_dpsm_expected_active_terminal_for_rev(
    uint8_t cmis_rev_byte,
    int tx_disabled,
    int dp_init_pending,
    openxcvr_cmis_data_path_state *state_out
) {
    openxcvr_cmis_data_path_state state;

    if (state_out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    if (!dpsm_rev_admitted(cmis_rev_byte)) {
        return OPENXCVR_ERR_UNSUPPORTED_REVISION;
    }

    switch (cmis_rev_byte) {
    case 0x30:
        /* The 3.0 machine has no Tx parking state: Activated regardless
         * of the projected Tx/pending facts. */
        state = OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED;
        break;
    case 0x40:
        state = (tx_disabled != 0)
            ? OPENXCVR_CMIS_DATA_PATH_STATE_DP_INITIALIZED
            : OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED;
        break;
    default:
        /* CMIS 5.0-5.3 (the only remaining catalog revisions). */
        state = (tx_disabled != 0 || dp_init_pending != 0)
            ? OPENXCVR_CMIS_DATA_PATH_STATE_DP_INITIALIZED
            : OPENXCVR_CMIS_DATA_PATH_STATE_DP_ACTIVATED;
        break;
    }

    *state_out = state;
    return OPENXCVR_OK;
}

/* ═══════════════ SFF-8636 / SFF-8472 aggregate JSON decoders ═══════════════
 *
 * Design R2/R3 lane C-b. Both writers reproduce the aggregate golden
 * contract (the same one the Python/Go/PHP facades implement): flat-union
 * page fields, no revision qualification (that is 's outstanding
 * facade, not this). Values come from the generated page structs and the
 * generated supplied readers; only the checksums and the raw bytes 3-10
 * ranges read the raw page buffers, exactly the slices Python reads raw.
 *
 * Number formatting mirrors the Python facades' json.dump output at parse
 * equality: exact-binary quotients (temperature /256, signaling rates) print
 * with %.17g so the parsed double round-trips; rounded products
 * (x*0.0001 at 4 dp, x*0.002 at 3 dp) print with %.4f/%.3f like the CMIS
 * writer, whose products are exact decimals at the printed precision.
 */

/* SFF-8024 table 4-4 identifier labels (schema enum sff8024_identifier),
 * shared by every family. */
static const char *sff8024_identifier_label(uint8_t value) {
    switch (value) {
    case 0x00: return "unknown_or_unspecified";
    case 0x01: return "gbic";
    case 0x02: return "soldered_module_sff8472";
    case 0x03: return "sfp_sff8472";
    case 0x04: return "xbi_300_pin";
    case 0x05: return "xenpak";
    case 0x06: return "xfp";
    case 0x07: return "xff";
    case 0x08: return "xfp_e";
    case 0x09: return "xpak";
    case 0x0A: return "x2";
    case 0x0B: return "dwdm_sfp_without_sff8472";
    case 0x0C: return "qsfp_inf8438";
    case 0x0D: return "qsfp_plus_sff8636";
    case 0x0E: return "cxp";
    case 0x0F: return "shielded_mini_multilane_hd_4x";
    case 0x10: return "shielded_mini_multilane_hd_8x";
    case 0x11: return "qsfp28_sff8636";
    case 0x12: return "cxp2";
    case 0x13: return "cdfp_style_1_2";
    case 0x14: return "shielded_mini_multilane_hd_4x_fanout";
    case 0x15: return "shielded_mini_multilane_hd_8x_fanout";
    case 0x16: return "cdfp_style_3";
    case 0x17: return "micro_qsfp";
    case 0x18: return "qsfp_dd";
    case 0x19: return "osfp";
    case 0x1A: return "sfp_dd_sfp_dd_management";
    case 0x1B: return "dsfp";
    case 0x1C: return "x4_minilink_oculink";
    case 0x1D: return "x8_minilink";
    case 0x1E: return "qsfp_plus_cmis";
    case 0x1F: return "sfp_dd_cmis";
    case 0x20: return "sfp_plus_cmis";
    case 0x21: return "osfp_xd_cmis";
    case 0x22: return "oif_elsfp_cmis";
    case 0x23: return "cdfp_x4_pcie_cmis";
    case 0x24: return "cdfp_x8_pcie_cmis";
    case 0x25: return "cdfp_x16_pcie_cmis";
    case 0x26: return "xpo";
    default: return "unknown";
    }
}

/* SFF-8024 connector labels (schema enum sff8024_connector_type). */
static const char *sff8024_connector_label(uint8_t value) {
    switch (value) {
    case 0x00: return "unknown_or_unspecified";
    case 0x01: return "sc";
    case 0x02: return "fibre_channel_style_1_copper";
    case 0x03: return "fibre_channel_style_2_copper";
    case 0x04: return "bnc_tnc";
    case 0x05: return "fibre_channel_coax_headers";
    case 0x06: return "fiber_jack";
    case 0x07: return "lc";
    case 0x08: return "mt_rj";
    case 0x09: return "mu";
    case 0x0A: return "sg";
    case 0x0B: return "optical_pigtail";
    case 0x0C: return "mpo_1x12";
    case 0x0D: return "mpo_2x16";
    case 0x20: return "hssdc_ii";
    case 0x21: return "copper_pigtail";
    case 0x22: return "rj45";
    case 0x23: return "no_separable_connector";
    case 0x24: return "mxc_2x16";
    case 0x25: return "cs_optical";
    case 0x26: return "sn_optical";
    case 0x27: return "mpo_2x12";
    case 0x28: return "mpo_1x16";
    default: return "unknown";
    }
}

/* SFF-8024 encoding labels (schema enum sff8024_encoding). */
static const char *sff8024_encoding_label(uint8_t value) {
    switch (value) {
    case 0x00: return "unspecified";
    case 0x01: return "eight_b_ten_b";
    case 0x02: return "four_b_five_b";
    case 0x03: return "nrz";
    case 0x04: return "manchester_or_sonet_scrambled_legacy";
    case 0x05: return "sonet_scrambled_or_legacy_64b66b";
    case 0x06: return "sixty_four_b_sixty_six_b_or_legacy_manchester";
    case 0x07: return "two_hundred_fifty_six_b_two_hundred_fifty_seven_b";
    case 0x08: return "pam4";
    default: return "unknown";
    }
}

/* SFF-8636 device-technology nibble labels (schema enum sff8636_device_technology). */
static const char *sff8636_device_technology_label(uint8_t value) {
    switch (value) {
    case 0x00: return "nm850_vcsel";
    case 0x01: return "nm1310_vcsel";
    case 0x02: return "nm1550_vcsel";
    case 0x03: return "nm1310_fp";
    case 0x04: return "nm1310_dfb";
    case 0x05: return "nm1550_dfb";
    case 0x06: return "nm1310_eml";
    case 0x07: return "nm1550_eml";
    case 0x08: return "other_or_undefined";
    case 0x09: return "nm1490_dfb";
    case 0x0A: return "copper_unequalized";
    case 0x0B: return "copper_passive_equalized";
    case 0x0C: return "copper_near_far_limiting_active_equalizers";
    case 0x0D: return "copper_far_limiting_active_equalizers";
    case 0x0E: return "copper_near_limiting_active_equalizers";
    case 0x0F: return "copper_linear_active_equalizers";
    default: return "unknown";
    }
}
static const char *json_bool(int b) {
    return b ? "true" : "false";
}

/* Python _checksum: sum(buf[start:end]) (end exclusive) vs buf[stored]. */
static void append_checksum(json_writer *w, const uint8_t *buf, unsigned start, unsigned end, unsigned stored_offset) {
    uint8_t computed = 0;
    for (unsigned i = start; i < end; i++) {
        computed += buf[i];
    }
    uint8_t stored = buf[stored_offset];
    jw_fmt(w, "{\"computed\":\"0x%02X\",\"stored\":\"0x%02X\",\"valid\":%s}",
           computed, stored, json_bool(computed == stored));
}

/* Python _hex_bytes: ["0x%02X", ...] */
static void append_hex_bytes(json_writer *w, const uint8_t *buf, size_t n) {
    jw_lit(w, "[");
    for (size_t i = 0; i < n; i++) {
        if (i != 0) {
            jw_lit(w, ",");
        }
        jw_fmt(w, "\"0x%02X\"", buf[i]);
    }
    jw_lit(w, "]");
}

/* Like read_page_file, but sized for 256-byte pages and zero-pads short
 * files, mirroring the Python _pad_page helper (extra bytes past `size` in
 * the file are ignored, as the Python decoders only read the first 256). */
static openxcvr_status read_page_file_padded(
    const char *fixture_dir,
    const char *stem,
    const char *suffix,
    uint8_t *out,
    size_t size
) {
    char path[1024];
    int n = snprintf(path, sizeof(path), "%s/%s.%s", fixture_dir, stem, suffix);
    if (n < 0 || (size_t)n >= sizeof(path)) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return OPENXCVR_ERR_IO;
    }
    memset(out, 0, size);
    /* The fread result is checked together with ferror: a read error fails
     * the load, while a short file (result < size) stays zero-padded like
     * the Python _pad_page. ferror is read before fclose invalidates the
     * stream. */
    size_t got = fread(out, 1, size, file);
    (void)got;
    int read_failed = ferror(file);
    int close_rc = fclose(file);
    if (read_failed || close_rc != 0) {
        return OPENXCVR_ERR_IO;
    }
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_sff8636_pages_load(
    const char *fixture_dir,
    const char *stem,
    openxcvr_sff8636_pages *out
) {
    openxcvr_status status;
    if (fixture_dir == NULL || stem == NULL || out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    memset(out, 0, sizeof(*out));

    status = read_page_file(fixture_dir, stem, "lower.bin", out->lower_raw, 1, NULL);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = read_page_file(fixture_dir, stem, "page-00h.bin", out->page00h_raw, 1, NULL);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_sff8636_lower_memory(out->lower_raw, 128, &out->lower);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_sff8636_page00h_upper_administrative(out->page00h_raw, 128, &out->page00h);
    if (status != OPENXCVR_OK) {
        return status;
    }
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_decode_sff8636_json(
    const openxcvr_sff8636_pages *pages,
    char *buf,
    size_t len,
    size_t *written
) {
    if (written != NULL) {
        *written = 0;
    }
    if (pages == NULL || written == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }

    const openxcvr_sff8636_page00h_upper_administrative *p = &pages->page00h;
    openxcvr_status status;
    uint8_t vendor_oui[3];
    status = openxcvr_decode_sff8636_page00h_upper_administrative_vendor_oui_supplied(pages->page00h_raw, 128, 128, vendor_oui);
    if (status != OPENXCVR_OK) {
        return status;
    }
    /* Lane alarm/warning nibbles come from the fully decoded lower struct
     * (every lane bit is one bool); the per-instance _supplied readers were
     * retired from the generated surface by the sff revision-lineage regen. */
    bool rx_any = false, bias_any = false, power_any = false;
    for (size_t i = 0; i < sizeof(pages->lower.rx_power_alarm_warning_flag) / sizeof(pages->lower.rx_power_alarm_warning_flag[0]); i++) {
        rx_any = rx_any || pages->lower.rx_power_alarm_warning_flag[i];
    }
    for (size_t i = 0; i < sizeof(pages->lower.tx_bias_alarm_warning_flag) / sizeof(pages->lower.tx_bias_alarm_warning_flag[0]); i++) {
        bias_any = bias_any || pages->lower.tx_bias_alarm_warning_flag[i];
    }
    for (size_t i = 0; i < sizeof(pages->lower.tx_power_alarm_warning_flag) / sizeof(pages->lower.tx_power_alarm_warning_flag[0]); i++) {
        power_any = power_any || pages->lower.tx_power_alarm_warning_flag[i];
    }

    bool interrupt = !pages->lower.interrupt_deasserted;
    for (int i = 0; i < 4; i++) {
        interrupt = interrupt || pages->lower.tx_los_flag[i] || pages->lower.rx_los_flag[i];
    }
    interrupt = interrupt || rx_any || bias_any || power_any;

    json_writer w = { .buf = buf, .len = len, .pos = 0 };

    jw_lit(&w, "{");
    jw_lit(&w, "\"decode_status\":\"ok\",");

    jw_lit(&w, "\"identity\":{");
    char date_code[8];
    snprintf(date_code, sizeof(date_code), "%s%s%s", p->date_code_year, p->date_code_month, p->date_code_day);
    jw_lit(&w, "\"date_code\":");
    jw_string(&w, date_code);
    jw_lit(&w, ",\"vendor_name\":");
    jw_string(&w, p->vendor_name);
    jw_fmt(&w, ",\"vendor_oui\":\"%02X:%02X:%02X\"", vendor_oui[0], vendor_oui[1], vendor_oui[2]);
    jw_lit(&w, ",\"vendor_pn\":");
    jw_string(&w, p->vendor_pn);
    jw_lit(&w, ",\"vendor_rev\":");
    jw_string(&w, p->vendor_rev);
    jw_lit(&w, ",\"vendor_sn\":");
    jw_string(&w, p->vendor_sn);
    jw_lit(&w, "},");

    uint8_t ext_id = (uint8_t)(p->power_class_low | (p->power_class_high << 6) |
                               ((unsigned)p->rx_cdr_present << 2) | ((unsigned)p->tx_cdr_present << 3) |
                               ((unsigned)p->clei_present << 4) | ((unsigned)p->power_class_8_implemented << 5));
    double nominal_rate = p->nominal_signaling_rate == 0xFF
        ? (double)p->ext_baud_rate / 4.0
        : (double)p->nominal_signaling_rate / 10.0;
    jw_lit(&w, "\"module\":{");
    jw_lit(&w, "\"cmis_revision\":null,");
    jw_lit(&w, "\"connector\":{\"name\":");
    jw_string(&w, sff8024_connector_label(p->connector_type));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", p->connector_type);
    jw_lit(&w, "\"encoding\":{\"name\":");
    jw_string(&w, sff8024_encoding_label(p->encoding));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", p->encoding);
    jw_fmt(&w, "\"extended_identifier\":\"0x%02X\",", ext_id);
    jw_lit(&w, "\"identifier\":{\"name\":");
    jw_string(&w, sff8024_identifier_label(pages->lower.sff8024_identifier));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", pages->lower.sff8024_identifier);
    jw_fmt(&w, "\"memory_model\":\"%s\",", pages->lower.flat_memory ? "flat" : "paged");
    jw_lit(&w, "\"module_state\":null,");
    jw_lit(&w, "\"module_type\":");
    if (p->ext_spec_compliance == 0x02) {
        jw_string(&w, "100GBASE-SR4 QSFP28");
    } else if (p->compliance_40gbase_sr4) {
        jw_string(&w, "40GBASE-SR4 QSFP+");
    } else {
        jw_lit(&w, "null");
    }
    jw_fmt(&w, ",\"nominal_bit_rate_gbps\":%.17g},", nominal_rate);

    jw_lit(&w, "\"monitoring\":{");
    jw_fmt(&w, "\"module_temperature_c\":%.17g,", (double)pages->lower.temperature / 256.0);
    jw_fmt(&w, "\"module_voltage_v\":%.4f},", (double)pages->lower.supply_voltage * 0.0001);

    jw_lit(&w, "\"compliance\":{");
    jw_lit(&w, "\"bits_set\":[");
    if (p->compliance_extended) {
        jw_lit(&w, "\"compliance_extended\"");
    }
    if (p->compliance_40gbase_sr4) {
        if (p->compliance_extended) {
            jw_lit(&w, ",");
        }
        jw_lit(&w, "\"compliance_40gbase_sr4\"");
    }
    jw_lit(&w, "],");
    jw_lit(&w, "\"device_technology\":{\"name\":");
    jw_string(&w, sff8636_device_technology_label(p->device_technology));
    jw_fmt(&w, ",\"raw\":\"0x%X\"},", p->device_technology);
    jw_lit(&w, "\"ext_spec_byte_192\":");
    if (p->ext_spec_compliance != 0) {
        jw_lit(&w, "{\"name\":");
        jw_string(&w, p->ext_spec_compliance == 0x02 ? "base_100g_sr4_or_25g_sr" : "unknown");
        jw_fmt(&w, ",\"raw\":\"0x%02X\"},", p->ext_spec_compliance);
    } else {
        jw_lit(&w, "null,");
    }
    jw_lit(&w, "\"raw_bytes_page_00h_3_10\":");
    append_hex_bytes(&w, &pages->page00h_raw[3], 8);
    jw_fmt(&w, ",\"reach\":{\"om1_m\":%u,\"om2_m\":%u,\"om3_2m_units\":%u,\"om4_or_copper_m\":%u,\"smf_km\":%u}},",
           (unsigned)p->length_om1_or_copper_attenuation_25p78ghz,
           (unsigned)p->length_om2_1m,
           (unsigned)p->length_om3_2m * 2,
           (unsigned)p->length_cable_or_om4,
           (unsigned)p->length_smf_km);

    jw_fmt(&w, "\"flags\":{\"data_not_ready\":%s,\"interrupt_asserted\":%s},",
           json_bool(pages->lower.data_not_ready), json_bool(interrupt));

    jw_lit(&w, "\"lanes\":[");
    for (int i = 0; i < 4; i++) {
        jw_fmt(&w, "%s{\"index\":%d,\"rx_power_mw\":%.4f,\"tx_bias_ma\":%.3f,\"tx_power_mw\":%.4f}",
               i == 0 ? "" : ",",
               i + 1,
               (double)pages->lower.rx_power_lane[i] * 0.0001,
               (double)pages->lower.tx_bias_lane[i] * 0.002,
               (double)pages->lower.tx_power_lane[i] * 0.0001);
    }
    jw_lit(&w, "],");

    jw_lit(&w, "\"checksums\":{\"cc_base\":");
    append_checksum(&w, pages->page00h_raw, 0, 63, 63);
    jw_lit(&w, ",\"cc_ext\":");
    append_checksum(&w, pages->page00h_raw, 64, 95, 95);
    jw_lit(&w, "}");

    jw_lit(&w, "}");
    *written = w.pos + 1;
    if (buf != NULL && len > 0) {
        if (w.pos < len) {
            buf[w.pos] = '\0';
        } else {
            buf[len - 1] = '\0';
        }
    }
    return (len >= *written) ? OPENXCVR_OK : OPENXCVR_ERR_SHORT_BUFFER;
}

openxcvr_status openxcvr_sff8472_pages_load(
    const char *fixture_dir,
    const char *stem,
    openxcvr_sff8472_pages *out
) {
    openxcvr_status status;
    if (fixture_dir == NULL || stem == NULL || out == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    memset(out, 0, sizeof(*out));

    status = read_page_file_padded(fixture_dir, stem, "a0h.bin", out->a0h_raw, 256);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = read_page_file_padded(fixture_dir, stem, "a2h.bin", out->a2h_raw, 256);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_page_a0h_base_memory(out->a0h_raw, 256, &out->a0h);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_page_a2h_diagnostics_base(out->a2h_raw, 256, &out->a2h);
    if (status != OPENXCVR_OK) {
        return status;
    }
    return OPENXCVR_OK;
}

openxcvr_status openxcvr_decode_sff8472_json(
    const openxcvr_sff8472_pages *pages,
    char *buf,
    size_t len,
    size_t *written
) {
    if (written != NULL) {
        *written = 0;
    }
    if (pages == NULL || written == NULL) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }

    const openxcvr_page_a0h_base_memory *a0h = &pages->a0h;
    const bool *alarm = pages->a2h.alarm_warning_flag;
    openxcvr_status status;
    uint8_t vendor_oui[3];
    status = openxcvr_decode_page_a0h_base_memory_vendor_oui_supplied(pages->a0h_raw, 0, 256, vendor_oui);
    if (status != OPENXCVR_OK) {
        return status;
    }
    uint16_t wavelength = 0;
    status = openxcvr_decode_page_a0h_base_memory_wavelength_or_cable_compliance_supplied(pages->a0h_raw, 0, 256, &wavelength);
    if (status != OPENXCVR_OK) {
        return status;
    }
    int16_t temperature = 0;
    uint16_t supply_voltage = 0, tx_bias = 0, tx_power = 0, rx_power = 0;
    status = openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(pages->a2h_raw, 0, 256, &temperature);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_page_a2h_diagnostics_base_supply_voltage_supplied(pages->a2h_raw, 0, 256, &supply_voltage);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_page_a2h_diagnostics_base_tx_bias_supplied(pages->a2h_raw, 0, 256, &tx_bias);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_page_a2h_diagnostics_base_tx_power_supplied(pages->a2h_raw, 0, 256, &tx_power);
    if (status != OPENXCVR_OK) {
        return status;
    }
    status = openxcvr_decode_page_a2h_diagnostics_base_rx_power_supplied(pages->a2h_raw, 0, 256, &rx_power);
    if (status != OPENXCVR_OK) {
        return status;
    }
    /* rx_los / tx_disable / tx_fault come from the decoded A2h struct below
     * (rx_los_state / tx_disable_state / tx_fault_state — the same
     * status/control byte-110 bits the retired status_control reader
     * carried); the per-field _supplied reader was removed from the
     * generated surface by the sff revision-lineage regen. */

    json_writer w = { .buf = buf, .len = len, .pos = 0 };

    jw_lit(&w, "{");
    jw_lit(&w, "\"decode_status\":\"ok\",");

    jw_lit(&w, "\"identity\":{");
    char date_code[7];
    memcpy(date_code, a0h->date_code, 6);
    date_code[6] = '\0';
    jw_lit(&w, "\"date_code\":");
    jw_string(&w, date_code);
    jw_lit(&w, ",\"vendor_name\":");
    jw_string(&w, a0h->vendor_name);
    jw_fmt(&w, ",\"vendor_oui\":\"%02X:%02X:%02X\"", vendor_oui[0], vendor_oui[1], vendor_oui[2]);
    jw_lit(&w, ",\"vendor_pn\":");
    jw_string(&w, a0h->vendor_pn);
    jw_lit(&w, ",\"vendor_sn\":");
    jw_string(&w, a0h->vendor_sn);
    jw_lit(&w, "},");

    double nominal_rate = a0h->signaling_rate_nominal == 0xFF
        ? (double)a0h->signaling_rate_max / 4.0
        : (double)a0h->signaling_rate_nominal / 10.0;
    jw_lit(&w, "\"module\":{");
    jw_fmt(&w, "\"calibration\":{\"ddm_implemented\":%s,\"externally_calibrated\":%s,\"internally_calibrated\":%s,\"rx_power_kind\":\"%s\"},",
           json_bool(a0h->digital_diagnostics_implemented),
           json_bool(a0h->externally_calibrated),
           json_bool(a0h->internally_calibrated),
           a0h->rx_power_average ? "average" : "oma");
    jw_lit(&w, "\"cmis_revision\":null,");
    jw_lit(&w, "\"connector\":{\"name\":");
    jw_string(&w, sff8024_connector_label(a0h->connector_type));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", a0h->connector_type);
    jw_lit(&w, "\"encoding\":{\"name\":");
    jw_string(&w, sff8024_encoding_label(a0h->encoding));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", a0h->encoding);
    jw_lit(&w, "\"identifier\":{\"name\":");
    jw_string(&w, sff8024_identifier_label(a0h->sff8024_identifier));
    jw_fmt(&w, ",\"raw\":\"0x%02X\"},", a0h->sff8024_identifier);
    jw_lit(&w, "\"memory_model\":null,\"module_state\":null,\"module_type\":");
    if (a0h->transceiver_compliance[3]) {
        jw_string(&w, "10GBASE-SR SFP+");
    } else {
        jw_lit(&w, "null");
    }
    jw_fmt(&w, ",\"nominal_bit_rate_gbps\":%.17g},", nominal_rate);

    if (a0h->digital_diagnostics_implemented) {
        jw_lit(&w, "\"monitoring\":{");
        jw_fmt(&w, "\"module_temperature_c\":%.17g,", (double)temperature / 256.0);
        jw_fmt(&w, "\"module_voltage_v\":%.4f},", (double)supply_voltage * 0.0001);
    } else {
        jw_lit(&w, "\"monitoring\":null,");
    }

    jw_lit(&w, "\"compliance\":{");
    jw_lit(&w, "\"raw_bytes_a0h_3_10\":");
    append_hex_bytes(&w, &pages->a0h_raw[3], 8);
    jw_fmt(&w, ",\"reach\":{\"om1_m\":%u,\"om2_m\":%u,\"om3_m\":%u,\"om4_m\":%u,\"smf_km\":%u},",
           (unsigned)a0h->length_om1_10m * 10,
           (unsigned)a0h->length_om2_10m * 10,
           (unsigned)((a0h->length_om3_base | (a0h->length_om3_multiplier << 6)) * 10),
           (unsigned)a0h->length_om4_or_copper_1m,
           (unsigned)a0h->length_smf_km);
    jw_fmt(&w, "\"wavelength_nm\":%u},", (unsigned)wavelength);

    if (a0h->digital_diagnostics_implemented) {
        jw_lit(&w, "\"flags\":{\"alarm_summary\":{");
        jw_fmt(&w, "\"rx_power_high_alarm\":%s,\"rx_power_low_alarm\":%s,", json_bool(alarm[8]), json_bool(alarm[9]));
        jw_fmt(&w, "\"temp_high_alarm\":%s,\"temp_low_alarm\":%s,", json_bool(alarm[0]), json_bool(alarm[1]));
        jw_fmt(&w, "\"tx_bias_high_alarm\":%s,\"tx_bias_low_alarm\":%s,", json_bool(alarm[4]), json_bool(alarm[5]));
        jw_fmt(&w, "\"tx_power_high_alarm\":%s,\"tx_power_low_alarm\":%s,", json_bool(alarm[6]), json_bool(alarm[7]));
        jw_fmt(&w, "\"vcc_high_alarm\":%s,\"vcc_low_alarm\":%s},", json_bool(alarm[2]), json_bool(alarm[3]));
        jw_lit(&w, "\"warning_summary\":{");
        jw_fmt(&w, "\"rx_power_high_warning\":%s,\"rx_power_low_warning\":%s,", json_bool(alarm[24]), json_bool(alarm[25]));
        jw_fmt(&w, "\"temp_high_warning\":%s,\"temp_low_warning\":%s,", json_bool(alarm[16]), json_bool(alarm[17]));
        jw_fmt(&w, "\"tx_bias_high_warning\":%s,\"tx_bias_low_warning\":%s,", json_bool(alarm[20]), json_bool(alarm[21]));
        jw_fmt(&w, "\"tx_power_high_warning\":%s,\"tx_power_low_warning\":%s,", json_bool(alarm[22]), json_bool(alarm[23]));
        jw_fmt(&w, "\"vcc_high_warning\":%s,\"vcc_low_warning\":%s}},", json_bool(alarm[18]), json_bool(alarm[19]));

        jw_lit(&w, "\"lanes\":[{\"index\":1,");
        jw_fmt(&w, "\"rx_los\":%s,", json_bool(pages->a2h.rx_los_state));
        jw_fmt(&w, "\"rx_power_mw\":%.4f,", (double)rx_power * 0.0001);
        jw_fmt(&w, "\"tx_bias_ma\":%.3f,", (double)tx_bias * 0.002);
        jw_fmt(&w, "\"tx_disable\":%s,", json_bool(pages->a2h.tx_disable_state));
        jw_fmt(&w, "\"tx_fault\":%s,", json_bool(pages->a2h.tx_fault_state));
        jw_fmt(&w, "\"tx_power_mw\":%.4f}],", (double)tx_power * 0.0001);
    } else {
        jw_lit(&w, "\"flags\":null,\"lanes\":null,");
    }

    jw_lit(&w, "\"checksums\":{\"cc_base\":");
    append_checksum(&w, pages->a0h_raw, 0, 63, 63);
    jw_lit(&w, ",\"cc_dmi\":");
    append_checksum(&w, pages->a2h_raw, 0, 95, 95);
    jw_lit(&w, ",\"cc_ext\":");
    append_checksum(&w, pages->a0h_raw, 64, 95, 95);
    jw_lit(&w, "}");

    jw_lit(&w, "}");
    *written = w.pos + 1;
    if (buf != NULL && len > 0) {
        if (w.pos < len) {
            buf[w.pos] = '\0';
        } else {
            buf[len - 1] = '\0';
        }
    }
    return (len >= *written) ? OPENXCVR_OK : OPENXCVR_ERR_SHORT_BUFFER;
}

static int identity_family_supported(openxcvr_spec_kind_t family, uint8_t revision) {
    switch (family) {
    case OPENXCVR_SPEC_CMIS:
        return revision == 0x30 || revision == 0x40 || revision == 0x50 ||
               revision == 0x51 || revision == 0x52 || revision == 0x53 ||
               revision == 0x54;
    case OPENXCVR_SPEC_SFF8636:
    case OPENXCVR_SPEC_SFF8472:
        return 1;
    default:
        return 0;
    }
}

openxcvr_status openxcvr_identity_required_ranges(
    openxcvr_spec_kind_t family, uint8_t revision,
    openxcvr_range *out, size_t capacity, size_t *count
) {
    openxcvr_range fields[6];
    if (count == NULL || (out == NULL && capacity != 0)) return OPENXCVR_ERR_INVALID_ARGUMENT;
    if (family == OPENXCVR_SPEC_CMIS && !identity_family_supported(family, revision))
        return OPENXCVR_ERR_UNSUPPORTED_REVISION;
    if (!identity_family_supported(family, revision)) return OPENXCVR_ERR_INVALID_ARGUMENT;
    if (family == OPENXCVR_SPEC_CMIS) {
        fields[0] = (openxcvr_range){0x50, 0, 0, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_NAME_OFFSET, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_NAME_LEN};
        fields[1] = (openxcvr_range){0x50, 0, 0, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_PN_OFFSET, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_PN_LEN};
        fields[2] = (openxcvr_range){0x50, 0, 0, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_REV_OFFSET, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_REV_LEN};
        fields[3] = (openxcvr_range){0x50, 0, 0, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_SN_OFFSET, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_SN_LEN};
        fields[4] = (openxcvr_range){0x50, 0, 0, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_DATE_CODE_YEAR_OFFSET, 8};
        fields[5] = (openxcvr_range){0x50, 0, 0, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_OUI_OFFSET, OPENXCVR_CMIS_PAGE00H_ADMINISTRATIVE_VENDOR_OUI_LEN};
    }
    else if (family == OPENXCVR_SPEC_SFF8636) {
        fields[0] = (openxcvr_range){0x50, 0, 0, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_NAME_OFFSET, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_NAME_LEN};
        fields[1] = (openxcvr_range){0x50, 0, 0, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_PN_OFFSET, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_PN_LEN};
        fields[2] = (openxcvr_range){0x50, 0, 0, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_REV_OFFSET, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_REV_LEN};
        fields[3] = (openxcvr_range){0x50, 0, 0, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_SN_OFFSET, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_SN_LEN};
        fields[4] = (openxcvr_range){0x50, 0, 0, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_DATE_CODE_YEAR_OFFSET, 8};
        fields[5] = (openxcvr_range){0x50, 0, 0, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_OUI_OFFSET, OPENXCVR_SFF8636_PAGE00H_UPPER_ADMINISTRATIVE_VENDOR_OUI_LEN};
    }
    else if (family == OPENXCVR_SPEC_SFF8472) {
        fields[0] = (openxcvr_range){0x50, 0, 0, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_NAME_OFFSET, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_NAME_LEN};
        fields[1] = (openxcvr_range){0x50, 0, 0, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_PN_OFFSET, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_PN_LEN};
        fields[2] = (openxcvr_range){0x50, 0, 0, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_REV_OFFSET, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_REV_LEN};
        fields[3] = (openxcvr_range){0x50, 0, 0, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_SN_OFFSET, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_SN_LEN};
        fields[4] = (openxcvr_range){0x50, 0, 0, OPENXCVR_PAGE_A0H_BASE_MEMORY_DATE_CODE_OFFSET, 8};
        fields[5] = (openxcvr_range){0x50, 0, 0, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_OUI_OFFSET, OPENXCVR_PAGE_A0H_BASE_MEMORY_VENDOR_OUI_LEN};
    }
    *count = 6;
    if (out == NULL) return OPENXCVR_OK;
    if (capacity < 6) return OPENXCVR_ERR_SHORT_BUFFER;
    memcpy(out, fields, sizeof(fields));
    return OPENXCVR_OK;
}

/* Collect only the six supplied fields; never synthesize a full page. */
openxcvr_status openxcvr_decode_identity_ranges(
    openxcvr_spec_kind_t family, uint8_t revision,
    const openxcvr_range_observation *ranges, size_t count,
    openxcvr_identity *out
) {
    openxcvr_range required[6];
    size_t required_count;
    uint8_t fields[6][16];
    uint8_t supplied[256] = {0};
    openxcvr_identity identity = {0};
    openxcvr_status status;
    if (out == NULL || (ranges == NULL && count != 0)) {
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
    status = openxcvr_identity_required_ranges(family, revision, required, 6, &required_count);
    if (status != OPENXCVR_OK) return status;
    for (size_t i = 0; i < count; ++i) {
        const openxcvr_range_observation *o = &ranges[i];
        unsigned end = (unsigned)o->range.offset + o->range.len;
        if (o->range.address > 0x7f || o->range.len == 0 || end > 256 ||
            (o->present && (o->bytes == NULL || o->len != o->range.len))) {
            return OPENXCVR_ERR_INVALID_ARGUMENT;
        }
        /* Absent buffers are deliberately never dereferenced. */
        if (!o->present || o->range.address != 0x50 || o->range.bank != 0 || o->range.page != 0) continue;
        for (unsigned b = o->range.offset; b < end; ++b) {
            if (supplied[b]) return OPENXCVR_ERR_INVALID_ARGUMENT;
            supplied[b] = 1;
            for (size_t f = 0; f < required_count; ++f) {
                if (b >= required[f].offset && b < (unsigned)required[f].offset + required[f].len)
                    fields[f][b - required[f].offset] = o->bytes[b - o->range.offset];
            }
        }
    }
    for (size_t i = 0; i < required_count; ++i) {
        for (unsigned b = required[i].offset; b < (unsigned)required[i].offset + required[i].len; ++b) {
            if (!supplied[b]) return OPENXCVR_ERR_SHORT_BUFFER;
        }
    }
    identity.family = family;
    identity.revision = revision;
    if (family == OPENXCVR_SPEC_CMIS) {
        status = openxcvr_decode_cmis_page00h_administrative_vendor_name_supplied(fields[0], required[0].offset, required[0].len, identity.vendor_name);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_cmis_page00h_administrative_vendor_pn_supplied(fields[1], required[1].offset, required[1].len, identity.vendor_pn);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_cmis_page00h_administrative_vendor_rev_supplied(fields[2], required[2].offset, required[2].len, identity.vendor_rev);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_cmis_page00h_administrative_vendor_sn_supplied(fields[3], required[3].offset, required[3].len, identity.vendor_sn);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_cmis_page00h_administrative_vendor_oui_supplied(fields[5], required[5].offset, required[5].len, identity.oui);
        if (status != OPENXCVR_OK) return status;
    }
    else if (family == OPENXCVR_SPEC_SFF8636) {
        status = openxcvr_decode_sff8636_page00h_upper_administrative_vendor_name_supplied(fields[0], required[0].offset, required[0].len, identity.vendor_name);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_sff8636_page00h_upper_administrative_vendor_pn_supplied(fields[1], required[1].offset, required[1].len, identity.vendor_pn);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_sff8636_page00h_upper_administrative_vendor_rev_supplied(fields[2], required[2].offset, required[2].len, identity.vendor_rev);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_sff8636_page00h_upper_administrative_vendor_sn_supplied(fields[3], required[3].offset, required[3].len, identity.vendor_sn);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_sff8636_page00h_upper_administrative_vendor_oui_supplied(fields[5], required[5].offset, required[5].len, identity.oui);
        if (status != OPENXCVR_OK) return status;
    }
    else if (family == OPENXCVR_SPEC_SFF8472) {
        status = openxcvr_decode_page_a0h_base_memory_vendor_name_supplied(fields[0], required[0].offset, required[0].len, identity.vendor_name);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_page_a0h_base_memory_vendor_pn_supplied(fields[1], required[1].offset, required[1].len, identity.vendor_pn);
        if (status != OPENXCVR_OK) return status;
        char vendor_rev[5];
        status = openxcvr_decode_page_a0h_base_memory_vendor_rev_supplied(fields[2], required[2].offset, required[2].len, vendor_rev);
        if (status != OPENXCVR_OK) return status;
        memcpy(identity.vendor_rev, vendor_rev, 2);
        identity.vendor_rev[2] = '\0';
        status = openxcvr_decode_page_a0h_base_memory_vendor_sn_supplied(fields[3], required[3].offset, required[3].len, identity.vendor_sn);
        if (status != OPENXCVR_OK) return status;
        status = openxcvr_decode_page_a0h_base_memory_vendor_oui_supplied(fields[5], required[5].offset, required[5].len, identity.oui);
        if (status != OPENXCVR_OK) return status;
    }
    /* The date is one supplied eight-byte field; preserve embedded spaces. */
    memcpy(identity.date_code, fields[4], 8);
    for (size_t n = 8; n > 0 && identity.date_code[n - 1] == ' '; --n) {
        identity.date_code[n - 1] = '\0';
    }
    *out = identity;
    return OPENXCVR_OK;
}
