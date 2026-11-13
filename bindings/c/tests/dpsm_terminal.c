/* Revision-qualified CMIS DPSM terminal predicate and expected active
   terminal helper (GitHub Issue #1 Ask 2, C parity with the Rust adapter
   crates/openxcvr-cmis/src/adapter/dpsm.rs).

   Four contracts:

   1. The exhaustive 384-cell terminal matrix: six catalog revisions x
      sixteen raw nibbles x tx_disabled x dp_init_pending, every cell
      compared against an independent literal oracle transcribed from the
      per-revision state machines — never derived from the predicate under
      test.
   2. The exhaustive 24-cell expected-active-terminal matrix (six catalog
      revisions x both flags), each cell pinning the returned state against
      a literal oracle AND its coherence with
      openxcvr_cmis_dpsm_is_terminal_for_rev under the same flags.
   3. Fail-closed error returns with output sentinels preserved: NULL
      outputs, raw_state > 0x0f and non-catalog revisions.
   4. openxcvr_decode_cmis_json reads the generated decoded struct field
      pages.page01h.tx_bias_current_scaling_factor (not the raw Page 01h
      byte 160): codes 0/1/2 scale tx_bias_ma by 1x/2x/4x and reserved
      code 3 renders null, driven through the public entrypoint on a repo
      fixture mutated in memory only. */

#include "openxcvr/openxcvr.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, ...)                                                       \
    do {                                                                       \
        if (!(cond)) {                                                         \
            failures++;                                                        \
            fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__);               \
            fprintf(stderr, __VA_ARGS__);                                      \
            fprintf(stderr, "\n");                                             \
        }                                                                       \
    } while (0)

/* The six DPSM catalog revisions (packed-BCD Lower memory byte 1). */
static const uint8_t CATALOG[6] = {0x30, 0x40, 0x50, 0x51, 0x52, 0x53};

/* Non-catalog revision bytes: pre-CMIS, wrong family neighbors of the
   catalog (3.1, 4.1, 5.4) and the extremes. */
static const uint8_t REFUSED[6] = {0x00, 0x20, 0x31, 0x41, 0x55, 0xFF};

/* Independent literal oracle for the terminal predicate, transcribed from
   the per-revision state machines (CMIS 3.0 § 1.5.3 Figure 9 / Table 8 /
   Table 66; CMIS 4.0 Table 8-58 § 6.3.2.4; CMIS 5.0-5.3 Table 8-84
   § 6.3.3.6) and mirroring the Rust oracle in
   crates/openxcvr-cmis/tests/dpsm_terminal.rs:

   | rev      | tx_disabled | dp_init_pending | terminal raws |
   |----------|-------------|-----------------|---------------|
   | 3.0      | any         | any             | 0x04 only     |
   | 4.0      | false       | any             | 0x04 only     |
   | 4.0      | true        | any             | 0x04, 0x07    |
   | 5.0-5.3  | false       | false           | 0x04 only     |
   | 5.0-5.3  | true/pending| any             | 0x04, 0x07    |

   Raw 0x07 (DP_INITIALIZED) is a named state only from CMIS 4.0 on; at
   3.0 it is reserved (Unknown) and never terminal. Every other nibble —
   0x00 (Reserved0), 0x01 (Deactivated), 0x02 (Init), 0x03 (Deinit),
   0x05/0x06 (TxTurnOn/TxTurnOff, named from 4.0) and 0x08..=0x0f
   (reserved at every revision) — is never terminal anywhere. */
static int oracle_terminal(uint8_t rev, uint8_t raw, int tx_disabled, int dp_init_pending) {
    if (raw == 0x04u) {
        return 1; /* DP_ACTIVATED: terminal at every catalog revision. */
    }
    if (raw != 0x07u) {
        return 0; /* Everything else is transient or reserved. */
    }
    switch (rev) {
    case 0x30:
        return 0; /* Reserved at 3.0: never terminal, flags irrelevant. */
    case 0x40:
        return tx_disabled ? 1 : 0; /* Page 11h:235 Reserved: pending ignored. */
    default:
        return (tx_disabled || dp_init_pending) ? 1 : 0; /* 5.0-5.3. */
    }
}

/* Independent literal oracle for the expected active terminal: 3.0 has no
   Tx parking state (Activated regardless); 4.0 parks at Initialized only
   while Tx is held off; 5.0-5.3 also park while init is pending. */
static uint8_t oracle_expected(uint8_t rev, int tx_disabled, int dp_init_pending) {
    switch (rev) {
    case 0x30:
        return 0x04;
    case 0x40:
        return tx_disabled ? 0x07 : 0x04;
    default:
        return (tx_disabled || dp_init_pending) ? 0x07 : 0x04;
    }
}

static void run_terminal_matrix_384(void) {
    int cells = 0;
    for (size_t r = 0; r < sizeof(CATALOG); r++) {
        const uint8_t rev = CATALOG[r];
        for (unsigned raw = 0; raw <= 15u; raw++) {
            for (int tx = 0; tx <= 1; tx++) {
                for (int pend = 0; pend <= 1; pend++) {
                    int want = oracle_terminal(rev, (uint8_t)raw, tx, pend);
                    int got = ~want; /* Poisoned: a refusal must overwrite. */
                    openxcvr_status st = openxcvr_cmis_dpsm_is_terminal_for_rev(
                        rev, (uint8_t)raw, tx, pend, &got);
                    CHECK(st == OPENXCVR_OK,
                          "rev %#04x raw %#04x tx=%d pend=%d: status %d, want OK",
                          rev, raw, tx, pend, (int)st);
                    CHECK(got == want,
                          "rev %#04x raw %#04x tx=%d pend=%d: terminal %d, want %d",
                          rev, raw, tx, pend, got, want);
                    cells++;
                }
            }
        }
    }
    CHECK(cells == 384, "matrix cells %d, want 384 (6 revs x 16 raws x 2 x 2)", cells);
}

static void run_expected_terminal_matrix_24(void) {
    int cells = 0;
    for (size_t r = 0; r < sizeof(CATALOG); r++) {
        const uint8_t rev = CATALOG[r];
        for (int tx = 0; tx <= 1; tx++) {
            for (int pend = 0; pend <= 1; pend++) {
                const uint8_t want = oracle_expected(rev, tx, pend);
                openxcvr_cmis_data_path_state got =
                    (openxcvr_cmis_data_path_state)0xA5; /* Poisoned. */
                openxcvr_status st = openxcvr_cmis_dpsm_expected_active_terminal_for_rev(
                    rev, tx, pend, &got);
                CHECK(st == OPENXCVR_OK,
                      "rev %#04x tx=%d pend=%d: status %d, want OK",
                      rev, tx, pend, (int)st);
                CHECK((uint8_t)got == want,
                      "rev %#04x tx=%d pend=%d: expected terminal %#04x, want %#04x",
                      rev, tx, pend, (unsigned)got, (unsigned)want);
                /* Coherence: the expected state is itself terminal under
                   the same flags. */
                int terminal = 0;
                st = openxcvr_cmis_dpsm_is_terminal_for_rev(rev, (uint8_t)got, tx, pend, &terminal);
                CHECK(st == OPENXCVR_OK && terminal == 1,
                      "rev %#04x tx=%d pend=%d: expected terminal %#04x not terminal (status %d, flag %d)",
                      rev, tx, pend, (unsigned)got, (int)st, terminal);
                cells++;
            }
        }
    }
    CHECK(cells == 24, "coherence cells %d, want 24 (6 revs x 2 x 2)", cells);
}

static void run_error_and_sentinel(void) {
    /* NULL outputs: INVALID_ARGUMENT, nothing to preserve. */
    CHECK(openxcvr_cmis_dpsm_is_terminal_for_rev(0x53, 0x04, 0, 0, NULL)
              == OPENXCVR_ERR_INVALID_ARGUMENT,
          "is_terminal_for_rev NULL is_terminal_out: want INVALID_ARGUMENT");
    CHECK(openxcvr_cmis_dpsm_expected_active_terminal_for_rev(0x53, 0, 0, NULL)
              == OPENXCVR_ERR_INVALID_ARGUMENT,
          "expected_active_terminal_for_rev NULL state_out: want INVALID_ARGUMENT");

    /* raw_state > 0x0f: INVALID_ARGUMENT, sentinel untouched (both the
       0xdeadbeef poison and the small -99 off-range marker). */
    const uint8_t big_raws[3] = {0x10, 0x80, 0xFF};
    for (size_t i = 0; i < sizeof(big_raws); i++) {
        int sentinel = (int)0xdeadbeef;
        openxcvr_status st = openxcvr_cmis_dpsm_is_terminal_for_rev(
            0x53, big_raws[i], 0, 0, &sentinel);
        CHECK(st == OPENXCVR_ERR_INVALID_ARGUMENT,
              "raw %#04x: status %d, want INVALID_ARGUMENT", big_raws[i], (int)st);
        CHECK(sentinel == (int)0xdeadbeef,
              "raw %#04x: sentinel moved to %d", big_raws[i], sentinel);
        sentinel = -99;
        st = openxcvr_cmis_dpsm_is_terminal_for_rev(0x40, big_raws[i], 1, 1, &sentinel);
        CHECK(st == OPENXCVR_ERR_INVALID_ARGUMENT,
              "raw %#04x (rev 0x40): status %d, want INVALID_ARGUMENT", big_raws[i], (int)st);
        CHECK(sentinel == -99,
              "raw %#04x (rev 0x40): sentinel moved to %d", big_raws[i], sentinel);
    }

    /* Argument errors precede revision refusal (header contract): a raw
       above 0x0f is INVALID_ARGUMENT even at a non-catalog revision. */
    {
        int sentinel = (int)0xdeadbeef;
        openxcvr_status st = openxcvr_cmis_dpsm_is_terminal_for_rev(
            0x31, 0x80, 0, 0, &sentinel);
        CHECK(st == OPENXCVR_ERR_INVALID_ARGUMENT,
              "rev 0x31 raw 0x80: status %d, want INVALID_ARGUMENT (precedence)", (int)st);
        CHECK(sentinel == (int)0xdeadbeef,
              "rev 0x31 raw 0x80: sentinel moved to %d", sentinel);
    }

    /* Non-catalog revisions: UNSUPPORTED_REVISION from both entry points,
       output sentinels untouched. */
    for (size_t i = 0; i < sizeof(REFUSED); i++) {
        const uint8_t rev = REFUSED[i];
        int sentinel = (int)0xdeadbeef;
        openxcvr_status st = openxcvr_cmis_dpsm_is_terminal_for_rev(
            rev, 0x04, 0, 0, &sentinel);
        CHECK(st == OPENXCVR_ERR_UNSUPPORTED_REVISION,
              "rev %#04x: status %d, want UNSUPPORTED_REVISION", rev, (int)st);
        CHECK(sentinel == (int)0xdeadbeef,
              "rev %#04x: is_terminal sentinel moved to %d", rev, sentinel);
        sentinel = -99;
        st = openxcvr_cmis_dpsm_is_terminal_for_rev(rev, 0x01, 1, 1, &sentinel);
        CHECK(st == OPENXCVR_ERR_UNSUPPORTED_REVISION,
              "rev %#04x (raw 0x01): status %d, want UNSUPPORTED_REVISION", rev, (int)st);
        CHECK(sentinel == -99,
              "rev %#04x: is_terminal sentinel (-99) moved to %d", rev, sentinel);

        openxcvr_cmis_data_path_state state =
            (openxcvr_cmis_data_path_state)0xdeadbeef; /* Poisoned enum. */
        st = openxcvr_cmis_dpsm_expected_active_terminal_for_rev(rev, 0, 0, &state);
        CHECK(st == OPENXCVR_ERR_UNSUPPORTED_REVISION,
              "rev %#04x: expected-terminal status %d, want UNSUPPORTED_REVISION",
              rev, (int)st);
        CHECK(state == (openxcvr_cmis_data_path_state)0xdeadbeef,
              "rev %#04x: state sentinel moved to %u", rev, (unsigned)state);
    }
}

static int count_occurrences(const char *haystack, const char *needle) {
    int count = 0;
    const char *p = haystack;
    while ((p = strstr(p, needle)) != NULL) {
        count++;
        p += strlen(needle);
    }
    return count;
}

/* Two-pass decode of `pages` into a heap buffer; refuses on any status
   other than the sizing pass's SHORT_BUFFER and the writing pass's OK. */
static char *decode_json(const openxcvr_cmis_pages *pages) {
    size_t needed = 0;
    openxcvr_status status = openxcvr_decode_cmis_json(pages, NULL, 0, &needed);
    if (status != OPENXCVR_ERR_SHORT_BUFFER && status != OPENXCVR_OK) {
        fprintf(stderr, "size pass failed: %s\n", openxcvr_status_string(status));
        return NULL;
    }
    char *json = (char *)malloc(needed);
    if (json == NULL) {
        return NULL;
    }
    status = openxcvr_decode_cmis_json(pages, json, needed, &needed);
    if (status != OPENXCVR_OK) {
        fprintf(stderr, "decode failed: %s\n", openxcvr_status_string(status));
        free(json);
        return NULL;
    }
    return json;
}

/* The repo fixture fboss-cmis-4-0-qsfpdd-400g-dr4 decodes laser_bias_tx
   (Page 11h bytes 190-205, big-endian u16) as 0x8f55 (36693, lane 1),
   0x78fc (30972, lane 2), 0x97d7 (38871, lane 3), 0x84a0 (33952, lane 4)
   and zero for lanes 5-8; at 0.002 mA/bit that is 73.386 / 61.944 /
   77.742 / 67.904 / 0.000 mA before scaling. These literals are
   transcribed from the fixture bytes, not from a decoder run (they also
   match the shared golden JSON). */
static void run_bias_scaling_struct_field(void) {
    openxcvr_cmis_pages base;
    openxcvr_status status = openxcvr_cmis_pages_load(
        "tests/fixtures/cmis", "fboss-cmis-4-0-qsfpdd-400g-dr4", &base);
    CHECK(status == OPENXCVR_OK, "fixture load: %s", openxcvr_status_string(status));
    if (status != OPENXCVR_OK) {
        return;
    }
    /* The fixture's Page 01h byte 160 carries scaling code 0 (1x), so the
       in-memory struct-field mutation below is what drives each case. */
    CHECK(base.page01h.tx_bias_current_scaling_factor == 0,
          "fixture decodes bias scaling code %u, want 0",
          (unsigned)base.page01h.tx_bias_current_scaling_factor);

    for (unsigned code = 0; code <= 3u; code++) {
        openxcvr_cmis_pages pages = base;
        pages.page01h.tx_bias_current_scaling_factor = (uint8_t)code;
        char *json = decode_json(&pages);
        if (json == NULL) {
            CHECK(0, "code %u: decode failed", code);
            continue;
        }
        const int total = count_occurrences(json, "\"tx_bias_ma\":");
        CHECK(total == 8, "code %u: %d tx_bias_ma fields, want 8", code, total);
        if (code == 3u) {
            /* Reserved code 0b11 (CMIS 5.3 Table 8-52): undefined
               multiplier renders null per lane. */
            const int nulls = count_occurrences(json, "\"tx_bias_ma\":null,");
            CHECK(nulls == 8, "code 3: %d null tx_bias_ma, want 8", nulls);
        } else {
            static const char *const NONZERO[3][4] = {
                {"73.386", "61.944", "77.742", "67.904"},
                {"146.772", "123.888", "155.484", "135.808"},
                {"293.544", "247.776", "310.968", "271.616"},
            };
            for (int lane = 0; lane < 4; lane++) {
                char want[64];
                snprintf(want, sizeof(want), "\"tx_bias_ma\":%s,", NONZERO[code][lane]);
                CHECK(count_occurrences(json, want) == 1,
                      "code %u: lane %d %s not found exactly once", code, lane + 1, want);
            }
            CHECK(count_occurrences(json, "\"tx_bias_ma\":0.000,") == 4,
                  "code %u: lanes 5-8 not 0.000 four times", code);
            CHECK(count_occurrences(json, "\"tx_bias_ma\":null,") == 0,
                  "code %u: unexpected null tx_bias_ma", code);
        }
        free(json);
    }

    /* Raw-divergence witness: the decoder follows the decoded struct field
       and no longer re-shifts the raw Page 01h byte. Pin the raw byte to
       code 2 (0x17: bits 4-3 set) while the struct field says 1, and the
       output must scale 2x (146.772/123.888), not 4x. */
    {
        openxcvr_cmis_pages pages = base;
        pages.page01h.tx_bias_current_scaling_factor = 1;
        pages.page01h_raw[160 - 128] = 0x17; /* Raw bits 4-3 = 0b10 (4x). */
        char *json = decode_json(&pages);
        CHECK(json != NULL, "raw-divergence: decode failed");
        if (json != NULL) {
            CHECK(count_occurrences(json, "\"tx_bias_ma\":146.772,") == 1,
                  "raw-divergence: struct code 1 must scale lane 1 to 146.772");
            CHECK(count_occurrences(json, "\"tx_bias_ma\":123.888,") == 1,
                  "raw-divergence: struct code 1 must scale lane 2 to 123.888");
            CHECK(count_occurrences(json, "\"tx_bias_ma\":293.544,") == 0,
                  "raw-divergence: raw code 2 must not reach the output");
            CHECK(count_occurrences(json, "\"tx_bias_ma\":247.776,") == 0,
                  "raw-divergence: raw code 2 must not reach the output");
            free(json);
        }
    }
}

int main(void) {
    run_terminal_matrix_384();
    run_expected_terminal_matrix_24();
    run_error_and_sentinel();
    run_bias_scaling_struct_field();
    if (failures != 0) {
        fprintf(stderr, "dpsm terminal: %d failure(s)\n", failures);
        return 1;
    }
    puts("C dpsm terminal: 384-cell matrix, 24-cell coherence, fail-closed errors and Page 01h bias scaling correct");
    return 0;
}
