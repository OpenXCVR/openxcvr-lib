#include "openxcvr/openxcvr.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Row order is fixed: identifier 0..255, each with these revisions in turn. */
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

static int check_line(const char *line, unsigned long lineno, unsigned long index) {
    char id_text[3];
    char rev_text[3];
    char kind_text[16];
    uint8_t identifier;
    uint8_t revision;
    openxcvr_spec_kind_t expected;
    openxcvr_spec_kind_t sentinel = (openxcvr_spec_kind_t)0x5A;
    openxcvr_spec_kind_t got = sentinel;
    openxcvr_status status;
    int fields;
    char extra;

    fields = sscanf(line, "%2s %2s %15s %c", id_text, rev_text, kind_text, &extra);
    if (fields != 3) {
        fprintf(stderr, "line %lu: expected 'II RR KIND', got '%s'\n", lineno, line);
        return 1;
    }
    if (parse_byte(id_text, &identifier) != 0 || parse_byte(rev_text, &revision) != 0) {
        fprintf(stderr, "line %lu: bad identifier or revision '%s'\n", lineno, line);
        return 1;
    }
    if (index >= 256UL * 7UL || identifier != (uint8_t)(index / 7UL) ||
        revision != REVISIONS[index % 7UL]) {
        fprintf(stderr, "line %lu: row %lu out of order: '%s'\n", lineno, index, line);
        return 1;
    }

    status = openxcvr_classify_identifier(identifier, revision, &got);
    if (strcmp(kind_text, "UNKNOWN") == 0) {
        if (status != OPENXCVR_ERR_INVALID_ARGUMENT) {
            fprintf(
                stderr,
                "line %lu: %s expected refusal, got %s\n",
                lineno,
                line,
                openxcvr_status_string(status)
            );
            return 1;
        }
        if (got != sentinel) {
            fprintf(stderr, "line %lu: *out was written on refusal\n", lineno);
            return 1;
        }
        return 0;
    }

    if (kind_from_name(kind_text, &expected) != 0) {
        fprintf(stderr, "line %lu: unknown kind '%s'\n", lineno, kind_text);
        return 1;
    }
    if (status != OPENXCVR_OK) {
        fprintf(
            stderr,
            "line %lu: %s expected %s, got %s\n",
            lineno,
            line,
            kind_text,
            openxcvr_status_string(status)
        );
        return 1;
    }
    if (got != expected) {
        fprintf(stderr, "line %lu: %s classified as %d\n", lineno, line, (int)got);
        return 1;
    }
    return 0;
}

static int check_null_out(void) {
    openxcvr_status status = openxcvr_classify_identifier(0x03, 0x00, NULL);
    if (status != OPENXCVR_ERR_INVALID_ARGUMENT) {
        fprintf(stderr, "NULL out returned %s\n", openxcvr_status_string(status));
        return 1;
    }
    return 0;
}

/* Direct expected values for the CMIS 3.0 revision byte, independent of the
   shared table. CMIS 3.0 reports its major revision as 0x30 (BCD-like nibble
   layout; the Rust Rev::from_major_byte_cmis maps 0x30 to Rev::CMIS_3_0).
   QSFP-DD (0x18) and OSFP (0x19) modules reporting 0x30 are CMIS family;
   every other sub-0x40 byte stays on the legacy pre-CMIS SFF-8636 map. */
static int check_cmis_3_0_expected(void) {
    static const struct {
        uint8_t identifier;
        uint8_t revision;
        int refused;
        openxcvr_spec_kind_t kind;
    } cases[] = {
        {0x18, 0x30, 0, OPENXCVR_SPEC_CMIS},
        {0x19, 0x30, 0, OPENXCVR_SPEC_CMIS},
        {0x18, 0x31, 0, OPENXCVR_SPEC_SFF8636},
        {0x19, 0x2F, 0, OPENXCVR_SPEC_SFF8636},
        {0x18, 0x3F, 0, OPENXCVR_SPEC_SFF8636},
        {0x18, 0x40, 0, OPENXCVR_SPEC_CMIS},
        {0x18, 0x00, 0, OPENXCVR_SPEC_SFF8636},
        {0x19, 0x10, 0, OPENXCVR_SPEC_SFF8636},
        {0x1B, 0x30, 0, OPENXCVR_SPEC_CMIS},
        {0x1A, 0x30, 1, 0},
        {0xFF, 0x30, 1, 0},
    };
    size_t i;
    for (i = 0; i < sizeof(cases) / sizeof(cases[0]); ++i) {
        openxcvr_spec_kind_t sentinel = (openxcvr_spec_kind_t)0x5A;
        openxcvr_spec_kind_t kind = sentinel;
        openxcvr_status status =
            openxcvr_classify_identifier(cases[i].identifier, cases[i].revision, &kind);
        if (cases[i].refused) {
            if (status != OPENXCVR_ERR_INVALID_ARGUMENT || kind != sentinel) {
                fprintf(
                    stderr,
                    "(%02X,%02X) expected refusal, got %s kind %d\n",
                    cases[i].identifier,
                    cases[i].revision,
                    openxcvr_status_string(status),
                    (int)kind
                );
                return 1;
            }
        } else if (status != OPENXCVR_OK || kind != cases[i].kind) {
            fprintf(
                stderr,
                "(%02X,%02X) expected kind %d, got %s kind %d\n",
                cases[i].identifier,
                cases[i].revision,
                (int)cases[i].kind,
                openxcvr_status_string(status),
                (int)kind
            );
            return 1;
        }
    }
    return 0;
}

/* Trunk snapshot of the classifier at 42efcae, embedded verbatim as the
   old-vs-new baseline. */
static openxcvr_status trunk_classify(
    uint8_t identifier,
    uint8_t revision,
    openxcvr_spec_kind_t *out
) {
    switch (identifier) {
    case 0x02:
    case 0x03:
        *out = OPENXCVR_SPEC_SFF8472;
        return OPENXCVR_OK;
    case 0x0C:
    case 0x0D:
    case 0x11:
    case 0x17:
        *out = OPENXCVR_SPEC_SFF8636;
        return OPENXCVR_OK;
    case 0x18:
    case 0x19:
        *out = revision >= 0x40 ? OPENXCVR_SPEC_CMIS : OPENXCVR_SPEC_SFF8636;
        return OPENXCVR_OK;
    case 0x1B:
    case 0x1E:
    case 0x1F:
    case 0x20:
    case 0x21:
    case 0x22:
    case 0x23:
    case 0x24:
    case 0x25:
        *out = OPENXCVR_SPEC_CMIS;
        return OPENXCVR_OK;
    default:
        return OPENXCVR_ERR_INVALID_ARGUMENT;
    }
}

/* Every one of the 65536 (identifier, revision) pairs must keep its trunk
   outcome except exactly (0x18,0x30) and (0x19,0x30), which move
   SFF-8636 -> CMIS. The expected families come from this embedded snapshot
   and the CMIS 3.0 revision contract, never from the shared table. */
static int check_exhaustive_compat(void) {
    unsigned long deltas = 0;
    unsigned identifier;
    unsigned revision;
    for (identifier = 0; identifier <= 255; ++identifier) {
        for (revision = 0; revision <= 255; ++revision) {
            openxcvr_spec_kind_t old_kind = (openxcvr_spec_kind_t)0x5A;
            openxcvr_spec_kind_t new_kind = (openxcvr_spec_kind_t)0x5A;
            openxcvr_status old_status =
                trunk_classify((uint8_t)identifier, (uint8_t)revision, &old_kind);
            openxcvr_status new_status = openxcvr_classify_identifier(
                (uint8_t)identifier, (uint8_t)revision, &new_kind);
            if (old_status == new_status && old_kind == new_kind) {
                continue;
            }
            if (old_status != OPENXCVR_OK || new_status != OPENXCVR_OK ||
                old_kind != OPENXCVR_SPEC_SFF8636 || new_kind != OPENXCVR_SPEC_CMIS ||
                !((identifier == 0x18 || identifier == 0x19) && revision == 0x30)) {
                fprintf(
                    stderr,
                    "unexpected delta at (%02X,%02X)\n",
                    identifier,
                    revision
                );
                return 1;
            }
            deltas++;
        }
    }
    if (deltas != 2) {
        fprintf(stderr, "expected exactly 2 deltas, saw %lu\n", deltas);
        return 1;
    }
    return 0;
}

int main(int argc, char **argv) {
    FILE *file;
    char line[128];
    unsigned long lineno = 0;
    unsigned long checked = 0;

    if (argc != 2) {
        fprintf(stderr, "usage: %s dispatch_table.txt\n", argv[0]);
        return 2;
    }
    file = fopen(argv[1], "r");
    if (file == NULL) {
        perror(argv[1]);
        return 1;
    }

    if (check_null_out() != 0) {
        fclose(file);
        return 1;
    }
    if (check_cmis_3_0_expected() != 0) {
        fclose(file);
        return 1;
    }
    if (check_exhaustive_compat() != 0) {
        fclose(file);
        return 1;
    }

    while (fgets(line, sizeof(line), file) != NULL) {
        size_t len;
        lineno++;
        len = strlen(line);
        while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r')) {
            line[--len] = '\0';
        }
        if (len == 0) {
            continue;
        }
        if (check_line(line, lineno, checked) != 0) {
            fclose(file);
            return 1;
        }
        checked++;
    }
    if (ferror(file) != 0) {
        perror(argv[1]);
        fclose(file);
        return 1;
    }
    fclose(file);

    /* identifier 0..255 × seven revisions */
    if (checked != 256UL * 7UL) {
        fprintf(stderr, "expected 1792 rows, checked %lu\n", checked);
        return 1;
    }
    printf("identifier parity: %lu rows matched\n", checked);
    return 0;
}
