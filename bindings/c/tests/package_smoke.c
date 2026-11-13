/*
 * No-hardware consumer smoke for a *packaged* C SDK.
 *
 * Not wired into the CTest suite: the CI artifact job
 * (.github/workflows/artifacts.yml) compiles and links this file against an
 * unpacked release package (headers + libraries via the package's own
 * openxcvr-c.pc) and runs it over the golden dispatch rows, so a package that
 * does not compile, link or classify fails the artifact job instead of
 * shipping.
 *
 * Usage: package_smoke <dispatch_table.txt>
 * The table is the same golden fixture CTest's openxcvr_c_identifier_parity
 * consumes: 256 identifiers x 7 revisions, "ID REV KIND" per line.
 */
#include "openxcvr/openxcvr.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Row order is fixed: identifier 0..255, each with these revisions in turn
 * (the same census openxcvr_c_identifier_parity enforces). */
#define ROW_COUNT (256 * 7)
static const uint8_t REVISIONS[7] = {0x00, 0x10, 0x30, 0x3F, 0x40, 0x50, 0xFF};

static int kind_from_name(const char *name, openxcvr_spec_kind_t *out) {
    if (strcmp(name, "SFF8472") == 0) {
        *out = OPENXCVR_SPEC_SFF8472;
        return 0;
    }
    if (strcmp(name, "SFF8636") == 0) {
        *out = OPENXCVR_SPEC_SFF8636;
        return 0;
    }
    if (strcmp(name, "CMIS") == 0) {
        *out = OPENXCVR_SPEC_CMIS;
        return 0;
    }
    return -1;
}

static int parse_byte(const char *text, uint8_t *out) {
    char *end = NULL;
    unsigned long value;

    if (text == NULL || !isxdigit((unsigned char)text[0]) ||
        !isxdigit((unsigned char)text[1]) || text[2] != '\0') {
        return -1;
    }
    value = strtoul(text, &end, 16);
    if (end == NULL || *end != '\0' || value > 0xFF) {
        return -1;
    }
    *out = (uint8_t)value;
    return 0;
}

int main(int argc, char **argv) {
    FILE *table = NULL;
    char line[128];
    unsigned long index = 0;

    if (argc != 2) {
        fprintf(stderr, "usage: %s <dispatch_table.txt>\n", argv[0]);
        return 2;
    }
    table = fopen(argv[1], "r");
    if (table == NULL) {
        fprintf(stderr, "cannot open %s\n", argv[1]);
        return 2;
    }

    while (fgets(line, sizeof(line), table) != NULL) {
        char id_text[8], rev_text[8], kind_text[32];
        uint8_t identifier, revision;
        openxcvr_spec_kind_t expected, got;
        openxcvr_status status;

        if (line[0] == '\n' || line[0] == '\r' || line[0] == '\0') {
            continue;
        }
        if (sscanf(line, "%7s %7s %31s", id_text, rev_text, kind_text) != 3) {
            fprintf(stderr, "row %lu: malformed '%s'\n", index, line);
            fclose(table);
            return 1;
        }
        if (parse_byte(id_text, &identifier) != 0 ||
            parse_byte(rev_text, &revision) != 0) {
            fprintf(stderr, "row %lu: bad identifier or revision '%s'\n", index, line);
            fclose(table);
            return 1;
        }
        if (identifier != (uint8_t)(index / 7UL) ||
            revision != REVISIONS[index % 7UL]) {
            fprintf(stderr, "row %lu: out of census order '%s'\n", index, line);
            fclose(table);
            return 1;
        }

        if (strcmp(kind_text, "UNKNOWN") == 0) {
            got = OPENXCVR_SPEC_SFF8472; /* sentinel: must stay untouched */
            status = openxcvr_classify_identifier(identifier, revision, &got);
            if (status != OPENXCVR_ERR_INVALID_ARGUMENT) {
                fprintf(stderr, "row %lu: %s expected refusal, got %s\n", index, line,
                    openxcvr_status_string(status));
                fclose(table);
                return 1;
            }
            if (got != OPENXCVR_SPEC_SFF8472) {
                fprintf(stderr, "row %lu: *out written on refusal\n", index);
                fclose(table);
                return 1;
            }
        } else {
            if (kind_from_name(kind_text, &expected) != 0) {
                fprintf(stderr, "row %lu: unknown kind '%s'\n", index, kind_text);
                fclose(table);
                return 1;
            }
            status = openxcvr_classify_identifier(identifier, revision, &got);
            if (status != OPENXCVR_OK) {
                fprintf(stderr, "row %lu: %s expected %s, got %s\n", index, line,
                    kind_text, openxcvr_status_string(status));
                fclose(table);
                return 1;
            }
            if (got != expected) {
                fprintf(stderr, "row %lu: %s classified as %d\n", index, line, (int)got);
                fclose(table);
                return 1;
            }
        }
        index++;
    }
    fclose(table);

    if (index != (unsigned long)ROW_COUNT) {
        fprintf(stderr, "expected %d rows, got %lu\n", ROW_COUNT, index);
        return 1;
    }
    if (openxcvr_classify_identifier(0x03, 0x00, NULL) != OPENXCVR_ERR_INVALID_ARGUMENT) {
        fprintf(stderr, "NULL out accepted\n");
        return 1;
    }
    printf("package_smoke: %lu golden dispatch rows classified OK\n", index);
    return 0;
}
