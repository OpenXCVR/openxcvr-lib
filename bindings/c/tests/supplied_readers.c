/*
 * Additive generated supplied readers (design R2/R3 lane C-a): wire-value
 * readers for SFF scalar (U16Be/I16Be), whole-byte Bitmap and instanced
 * lane-nibble fields. Values are checked against fixture bytes; every
 * failure path must leave the output untouched.
 */
#include "openxcvr/openxcvr.h"
#include "pages/page_a0h_base_memory.h"
#include "pages/page_a2h_diagnostics_base.h"
#include "pages/sff8636_lower_memory.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, ...)                                                       \
    do {                                                                       \
        if (!(cond)) {                                                          \
            failures++;                                                        \
            fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__);               \
            fprintf(stderr, __VA_ARGS__);                                      \
            fprintf(stderr, "\n");                                             \
        }                                                                       \
    } while (0)

static int read_file(const char *path, uint8_t *out, size_t capacity, size_t *got) {
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return 1;
    }
    size_t n = fread(out, 1, capacity, file);
    int rc = fclose(file);
    if (rc != 0) {
        return 1;
    }
    *got = n;
    return 0;
}

static int run_fixture_values(void) {
    /* SFF-8472 A2h telemetry (variants fields: wire value, no scaling here). */
    uint8_t a2h[256];
    size_t a2h_len = 0;
    CHECK(read_file("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin",
                    a2h, sizeof(a2h), &a2h_len) == 0,
          "cannot read a2h fixture");
    CHECK(a2h_len == 256, "a2h fixture length %zu", a2h_len);

    int16_t temperature = 0;
    uint16_t supply_voltage = 0, tx_bias = 0, tx_power = 0, rx_power = 0;
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              a2h, 0, 256, &temperature) == OPENXCVR_OK, "temperature status");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_supply_voltage_supplied(
              a2h, 0, 256, &supply_voltage) == OPENXCVR_OK, "supply_voltage status");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_tx_bias_supplied(
              a2h, 0, 256, &tx_bias) == OPENXCVR_OK, "tx_bias status");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_tx_power_supplied(
              a2h, 0, 256, &tx_power) == OPENXCVR_OK, "tx_power status");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_rx_power_supplied(
              a2h, 0, 256, &rx_power) == OPENXCVR_OK, "rx_power status");
    /* The status_control wire-value reader was retired from the generated
     * surface by the sff revision-lineage regen; the status/control bits are
     * covered through the decoded struct, at the schema-owned positions
     * (rx_los_state = byte 110 bit 1, tx_fault_state = bit 2,
     * tx_disable_state = bit 7). This fixture's byte 110 is 0x12 = bits 4
     * and 1: rx_los set, tx_fault and tx_disable clear. */
    openxcvr_page_a2h_diagnostics_base a2d;
    CHECK(openxcvr_decode_page_a2h_diagnostics_base(a2h, 256, &a2d) == OPENXCVR_OK,
          "a2h full-decode status");

    CHECK(temperature == 2586, "temperature %d", temperature);
    CHECK(supply_voltage == 33162, "supply_voltage %u", (unsigned)supply_voltage);
    CHECK(tx_bias == 3588, "tx_bias %u", (unsigned)tx_bias);
    CHECK(tx_power == 5846, "tx_power %u", (unsigned)tx_power);
    CHECK(rx_power == 0, "rx_power %u", (unsigned)rx_power);
    CHECK(a2d.rx_los_state, "rx_los_state");
    CHECK(!a2d.tx_fault_state, "tx_fault_state");
    CHECK(!a2d.tx_disable_state, "tx_disable_state");

    /* SFF-8472 A0h wavelength (u16be with variants, from the 128-byte extcal
     * fixture presented as an exact two-byte supplied subrange at byte 60). */
    uint8_t a0h[256];
    size_t a0h_len = 0;
    CHECK(read_file("tests/fixtures/sff-8472/synthetic-extcal-sfp10g-sr.a0h.bin",
                    a0h, sizeof(a0h), &a0h_len) == 0,
          "cannot read a0h fixture");
    CHECK(a0h_len == 128, "a0h fixture length %zu", a0h_len);
    uint16_t wavelength = 0;
    CHECK(openxcvr_decode_page_a0h_base_memory_wavelength_or_cable_compliance_supplied(
              a0h + 60, 60, 2, &wavelength) == OPENXCVR_OK, "wavelength status");
    CHECK(wavelength == 0x352, "wavelength %u", (unsigned)wavelength);

    /* SFF-8636 lower memory: fixed scalars plus instanced lane nibbles. */
    uint8_t lower[128];
    size_t lower_len = 0;
    CHECK(read_file("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin",
                    lower, sizeof(lower), &lower_len) == 0,
          "cannot read lower fixture");
    CHECK(lower_len == 128, "lower fixture length %zu", lower_len);

    int16_t lower_temperature = 0;
    uint16_t lower_voltage = 0;
    CHECK(openxcvr_decode_sff8636_lower_memory_temperature_supplied(
              lower, 0, 128, &lower_temperature) == OPENXCVR_OK, "lower temperature status");
    CHECK(openxcvr_decode_sff8636_lower_memory_supply_voltage_supplied(
              lower, 0, 128, &lower_voltage) == OPENXCVR_OK, "lower voltage status");
    CHECK(lower_temperature == 11100, "lower temperature %d", lower_temperature);
    CHECK(lower_voltage == 32689, "lower voltage %u", (unsigned)lower_voltage);

    /* The instanced lane-nibble _supplied readers were retired from the
     * generated surface by the sff revision-lineage regen (the decoded
     * lower struct carries every lane bit as a bool). Their coverage now
     * flows through the aggregate SFF-8636 decoders and the sff-8636
     * goldens. */
    return 0;
}

static int run_negative_i16be(void) {
    uint8_t buf[4] = {0xFF, 0x70, 0xFF, 0xFF};
    int16_t value = 0;
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              buf, 96, 4, &value) == OPENXCVR_OK, "negative i16be status");
    CHECK(value == -144, "0xFF70 signed value %d", value);
    value = 0;
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_optional_laser_temperature_supplied(
              buf + 2, 106, 2, &value) == OPENXCVR_OK, "negative i16be #2 status");
    CHECK(value == -1, "0xFFFF signed value %d", value);
    return 0;
}

static int run_failure_paths(void) {
    uint8_t page[256];
    memset(page, 0xAB, sizeof(page));

    /* NULL buf or out. */
    int16_t temperature = 0x5A5A;
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              NULL, 0, 256, &temperature) == OPENXCVR_ERR_INVALID_ARGUMENT,
          "NULL buf status");
    CHECK(temperature == 0x5A5A, "output touched on NULL buf failure");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              page, 0, 256, NULL) == OPENXCVR_ERR_INVALID_ARGUMENT,
          "NULL out status");

    /* offset/len overflow: offset > 256, and len > 256 - offset. */
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              page, 257, 0, &temperature) == OPENXCVR_ERR_INVALID_ARGUMENT,
          "offset overflow status");
    CHECK(temperature == 0x5A5A, "output touched on offset overflow");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              page, 100, 157, &temperature) == OPENXCVR_ERR_INVALID_ARGUMENT,
          "len overflow status");
    CHECK(temperature == 0x5A5A, "output touched on len overflow");

    /* Truncated two-byte field: the window ends between the two bytes. */
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              page, 96, 1, &temperature) == OPENXCVR_ERR_INSUFFICIENT_INPUT,
          "truncated status");
    CHECK(temperature == 0x5A5A, "output touched on truncated field");
    CHECK(openxcvr_decode_page_a2h_diagnostics_base_temperature_supplied(
              page, 97, 2, &temperature) == OPENXCVR_ERR_INSUFFICIENT_INPUT,
          "window-before-field status");
    CHECK(temperature == 0x5A5A, "output touched on window-before-field");

    /* The instanced lane-nibble readers (and the status_control bitmap
     * reader) were retired from the generated surface by the sff
     * revision-lineage regen; their failure paths went with them. The
     * surviving scalar readers' failure paths are checked above. */
    return 0;
}

int main(void) {
    run_fixture_values();
    run_negative_i16be();
    run_failure_paths();
    if (failures != 0) {
        fprintf(stderr, "supplied readers: %d failure(s)\n", failures);
        return 1;
    }
    puts("C supplied readers: fixture values and failure paths correct");
    return 0;
}
