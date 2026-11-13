#!/usr/bin/env python3
"""Emit the synthetic-cmis-3-0-qsfp-dd fixture .bin files and parity golden.

Companion to `tools/build-cmis-5-2-synthetic.py`. Same design pattern:
durable byte-by-byte source of truth with inline CMIS 3.0 spec
citations.

The fixture drives decoder-dispatch testing (MajorRevision-byte routes
correctly to the CMIS 3.0 branch) and carries a limited synthetic parity
golden (`expected_golden` below): the golden pins the structural decode of
the authored bytes — identity strings, the zero-value module/monitoring
paths, the all-Reserved Page 11h data-path states, and the empty
application advertisement. It is NOT a hardware oracle and NOT a
field-content test: no byte comes from a real module, and the zeroed
pages deliberately carry no field-content semantics (see
`tests/fixtures/cmis/synthetic-cmis-3-0-qsfp-dd.md`).

CMIS 3.0 source: `docs/stds/cmis/CMIS3p0_Third_Party_Spec.pdf`
(QSFP-DD MSA, "Common Management Specification Rev 3.0", 2018-09-18).

The golden's expected semantics are derived from the authored bytes plus
the CMIS 3.0 spec tables cited inline — never by running a decoder and
capturing its output. Fields decode under the 3.0 revision's own tables;
where CMIS 3.0 and the canonical CMIS 5.3 schema namespace (`schemas/cmis/`,
the namespace every generated decoder is emitted from) agree in offsets
and encodings, the golden metadata states that agreement explicitly
instead of silently assuming it.

Run:
    python3 tools/build-cmis-3-0-synthetic.py
"""

from __future__ import annotations

import json
import pathlib
import sys

FIXTURE_DIR = pathlib.Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "cmis"
STEM = "synthetic-cmis-3-0-qsfp-dd"
GOLDEN_DIR_ROOT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "golden"
GOLDEN_PATH = GOLDEN_DIR_ROOT / "cmis" / f"{STEM}.json"

# Reserved-DPSM variant: same module bytes as the baseline fixture, with a
# Page 11h whose Data Path State nibbles exercise CMIS 3.0 Table 66's
# reserved 5h-7h encodings alongside the 0h/3h/4h controls (see
# `page_11h_reserved`). Independent of every production decoder: the labels
# transcribe Table 66 exactly as the baseline builder does.
RESERVED_STEM = "synthetic-cmis-3-0-qsfp-dd-dp-reserved"
RESERVED_GOLDEN_PATH = GOLDEN_DIR_ROOT / "cmis" / f"{RESERVED_STEM}.json"
# Pinned actual authoring date of the reserved variant — the golden must
# regenerate byte-for-byte, so it is never taken from the clock at run time.
RESERVED_AUTHORED_AT = "2026-10-22"
AUTHORED_BY_RESERVED = "ras + builder agent via tools/build-cmis-3-0-synthetic.py"
# CMIS 3.0 Table 66 lane vector (lanes 1..8): 0h Reserved, 5h Reserved,
# 6h Reserved, 7h Reserved, 3h Deinit, 4h Activated, 0h Reserved, 3h Deinit.
RESERVED_LANE_VECTOR = (0x0, 0x5, 0x6, 0x7, 0x3, 0x4, 0x0, 0x3)


def pad_ascii(s: str, width: int) -> bytes:
    """Left-aligned ASCII, space-padded to `width`. Truncates if too long."""
    b = s.encode("ascii")
    if len(b) > width:
        b = b[:width]
    return b + b" " * (width - len(b))


def lower_memory() -> bytes:
    """Lower memory (128 bytes). CMIS 3.0 § 1.7.2 / Table 18.

    Critical bytes for dispatch test:
      - byte 0: Identifier (SFF-8024 module type) — 0x18 (QSFP-DD), per
                CMIS 3.0 § 1.7.2.1 Table 18.
      - byte 1: Revision Compliance Identifier — 0x30 (upper nibble=3,
                lower=0 → CMIS rev 3.0). § 1.7.2.1 Table 18.
      - byte 2: bit 7 = Flat_mem (0 = paged, 1 = flat). Set to 0 so the
                fixture exercises the paged-memory decoder path.
                bit 6 = CLEI present (note: this position was renamed
                "SteppedConfigOnly" in CMIS 5.x — our value 0 is
                benign in both interpretations).
                bits 3-2 = TWI Maximum Speed (00 = 400kHz). § 1.7.2.1
                Table 18.
    All other bytes 0x00 (synthetic placeholder). The golden decodes
    those zero bytes honestly rather than inventing meaning: ModuleState
    000b = Reserved (Tables 18/19), Interrupt bit 0 = asserted (Table 18:
    0b=Interrupt asserted), monitors 0.0 (Table 22), module flags clear
    (Table 21).
    """
    buf = bytearray(128)
    buf[0] = 0x18  # QSFP-DD identifier
    buf[1] = 0x30  # CMIS 3.0 revision (upper=3, lower=0)
    buf[2] = 0x00  # paged memory + no CLEI + 400kHz TWI
    return bytes(buf)


def page_00h() -> bytes:
    """Upper Page 00h (Administrative). CMIS 3.0 § 1.7.3.

    Identity-string layout matches CMIS 5.2 byte-for-byte
    (verified against § 1.7.3.2 - § 1.7.3.7):
      - 0   (flat 128): Identifier copy = 0x18 (mirror of Lower:0).
      - 1-16 (129-144): VendorName (16-byte ASCII, space-padded).
      - 17-19 (145-147): VendorOUI (3 bytes; 00 00 00 = synthetic).
      - 20-35 (148-163): VendorPN (16-byte ASCII).
      - 36-37 (164-165): VendorRev (2-byte ASCII).
      - 38-53 (166-181): VendorSN (16-byte ASCII).
      - 54-61 (182-189): DateCode (8-byte ASCII; YYMMDD + 2 lot chars).
      - 62-71 (190-199): CLEICode (10-byte ASCII; spaces = synthetic).
      - 72-93 (200-221): zeros (power, connector, attenuation, etc.).
      - 94   (222): Page Checksum = low-order 8 bits of sum of
                    page-relative bytes 0-93 (CMIS 3.0 § 1.7.3.15:
                    "low order 8 bits of the sum of the contents of all
                    the bytes from byte 128 to byte 221, inclusive").
      - 95-127 (223-255): Custom Info (non-volatile) — zeros per
                          § 1.7.3.16 ("contents of this area are not
                          defined by this specification").

    Vendor strings carry the marker `CMIS_3_0_SYNTH` and `FIXTURE-30`
    in human-readable form so a casual hex-dump inspection identifies
    this as a synthetic fixture, not a real-vendor capture.
    """
    buf = bytearray(128)
    buf[0] = 0x18  # Identifier copy

    buf[1:17] = pad_ascii("CMIS_3_0_SYNTH", 16)         # VendorName
    # buf[17:20] = b"\x00\x00\x00"                       # VendorOUI (already 0)
    buf[20:36] = pad_ascii("FIXTURE-30-SYNTH", 16)      # VendorPN
    buf[36:38] = pad_ascii("00", 2)                     # VendorRev
    buf[38:54] = pad_ascii("SYNTHETIC0000003", 16)      # VendorSN ("3" suffix = CMIS 3.0)
    buf[54:62] = pad_ascii("18091800", 8)               # DateCode (CMIS 3.0 publish 2018-09-18 + 2 lot chars)
    buf[62:72] = pad_ascii("", 10)                      # CLEICode (10 spaces — Lower:2[6] CLEI-present=0)
    # buf[72:94]: zeros
    # Page Checksum at offset 94, computed below
    # buf[95:128]: zeros (Custom)

    # Page Checksum: low-order 8 bits of arithmetic sum of bytes 0-93
    # (= flat 128-221) per CMIS 3.0 § 1.7.3.15.
    buf[94] = sum(buf[0:94]) & 0xFF
    return bytes(buf)


def page_01h() -> bytes:
    """Upper Page 01h (Application Advertising). CMIS 3.0 § 1.7.4.

    All zeros. Note: CMIS 3.0 Page 01h advertising layout differs from
    4.0+ in several repurposed bits (per the archived
    `2026-05-06-cmis-3-and-5-2-fixtures-design.md` background). The two
    regions the golden reads from this page are stable across revisions:
    the additional Application Descriptor slots at bytes 223-250
    (page-relative 95-122, Table 48; 5.2 Table 8-53 is byte-identical)
    and the Tx-bias multiplier at byte 160 bits 4-3 (Table 45; 5.2
    Table 8-47 encodes the same 00b=x1/01b=x2/10b=x4/11b=reserved). All
    descriptor slots are zero (MIF=0x00), so nothing is advertised and
    the multiplier decodes as x1. A future field-content test for CMIS
    3.0 specifics would still need the 3.0-specific bit layout authored
    here for the other bytes.
    """
    return bytes(128)


def page_02h() -> bytes:
    """Upper Page 02h (Module-Level + Lane-Specific Monitor Thresholds).
    CMIS 3.0 § 1.7.5. All zeros for synthetic dispatch testing.
    """
    return bytes(128)


def page_10h() -> bytes:
    """Upper Page 10h (Lane Control). CMIS 3.0 § 1.7.6.

    All zeros. Note on the CMIS 3.0 DPSM (Data Path State Machine): its
    Page 11h "Data Path State Indicator" encodings (Table 66) are NOT
    re-encoded relative to 4.0+ — 0h-4h (Reserved, Deactivated, Init,
    Deinit, Activated) are stable across CMIS 3.0 / 4.0 (Table 8-58) /
    5.2 (Table 8-77) / 5.3; CMIS 4.0 merely defined the 5h-7h codes
    (TxTurnOn/TxTurnOff/Initialized) that 3.0 leaves Reserved. Page 10h's
    control fields key into that state machine; synthetic-zero is
    acceptable for dispatch testing since the test does not read this
    page.
    """
    return bytes(128)


def page_11h() -> bytes:
    """Upper Page 11h (Lane Status + DPSM). CMIS 3.0 § 1.7.7.

    All zeros = every host lane's Data Path State Indicator nibble is 0h,
    which CMIS 3.0 § 1.7.7.1 Table 66 defines as **Reserved** — NOT
    DataPathDeactivated (that is 1h). The golden therefore renders every
    lane `dp_state: "Reserved0"` (the corpus PascalCase rendering of the
    schema enum name `reserved_0`). All lane flags are cleared and all
    lane monitors read 0 (Table 67/68/69 flags, Table 70 monitors).
    """
    return bytes(128)


def page_11h_reserved() -> bytes:
    """Reserved-DPSM variant of Upper Page 11h. CMIS 3.0 § 1.7.7.

    The first four bytes pack `RESERVED_LANE_VECTOR` per Table 65 (odd
    lanes in bits 3-0, even lanes in bits 7-4): 0h Reserved, 5h Reserved,
    6h Reserved, 7h Reserved, 3h Deinit, 4h Activated, 0h Reserved, 3h
    Deinit -> `50 76 43 30`. CMIS 3.0 Table 66 leaves 5h-7h Reserved (the
    DPTxTurnOn/DPTxTurnOff/DPInitialized meanings only exist from CMIS 4.0
    Table 8-58 on), so the golden renders them Unknown5/Unknown6/Unknown7
    while the 0h/3h/4h controls render Reserved0/DpDeinit/DpActivated.
    The remaining 124 bytes are zero: lane flags (Tables 67-69) and lane
    monitors (Table 70) carry no content, exactly like the baseline.
    """
    packed = [
        RESERVED_LANE_VECTOR[2 * i] | (RESERVED_LANE_VECTOR[2 * i + 1] << 4)
        for i in range(4)
    ]
    buf = bytearray(128)
    buf[0:4] = bytes(packed)
    assert bytes(packed) == bytes.fromhex("50764330"), (
        f"reserved lane vector mispacked: {bytes(packed).hex()}"
    )
    return bytes(buf)


# ============================================================================
# Expected-decode tables — the CMIS 3.0 revision's OWN encodings
# ============================================================================
# Every table below is transcribed from the CMIS 3.0 spec PDF
# (CMIS3p0_Third_Party_Spec.pdf) so the golden is authored from the named
# revision, never from CMIS 4.0+/5.x semantics.

# Data Path State Indicator — Page 11h bytes 128-131, one nibble per host
# lane (odd lanes bits 3-0, even lanes bits 7-4; Table 65).
# CMIS 3.0 § 1.7.7.1 Table 66:
#   0h Reserved; 1h DataPathDeactivated; 2h DataPathInit; 3h DataPathDeinit;
#   4h DataPathActivated; 5h-Fh Reserved.
# Names render in the cross-binding PascalCase convention the golden corpus
# uses ("Reserved0" precedent: fboss-cmis-4-0-qsfp112-200g-fr4.json, the
# schema enum name `reserved_0`). 5h-7h are Reserved in CMIS 3.0 — the
# DPTxTurnOn/DPTxTurnOff/DPInitialized meanings only exist from CMIS 4.0 on —
# so they render Unknown<n> instead of borrowing 4.0+ semantics.
DP_STATE_NAMES = {
    0x0: "Reserved0",
    0x1: "DpDeactivated",
    0x2: "DpInit",
    0x3: "DpDeinit",
    0x4: "DpActivated",
}

# Module State — Lower memory byte 3 bits 3-1. CMIS 3.0 § 1.7.2.1
# Tables 18/19:
#   000b Reserved; 001b ModuleLowPwr; 010b ModulePwrUp; 011b ModuleReady;
#   100b ModulePwrDn; 101b "Fault state"; 110b/111b Reserved.
# 101b renders with the corpus-wide "ModuleFault" label (CMIS 4.0+ renamed
# 3.0's "Fault state"; every binding renders the PascalCase ModuleFault).
MODULE_STATE_NAMES = {
    0b000: "Reserved0",
    0b001: "ModuleLowPwr",
    0b010: "ModulePwrUp",
    0b011: "ModuleReady",
    0b100: "ModulePwrDn",
    0b101: "ModuleFault",
}

# SFF-8024 identifier / connector names, taken from the locked schema enums
# (schemas/sff-8024/identifier-values.yaml "qsfp_dd", connector-types.yaml
# "unknown_or_unspecified") so the golden's `*.name` values are
# schema-approved, not author-invented. The builder only ever writes
# identifier 0x18 (QSFP-DD, SFF-8024 Table 4-1) and connector 0x00
# (Page 00h:203 = page-relative 75, zero-filled = "Unknown or unspecified",
# SFF-8024 Table 4-3 / CMIS 3.0 § 1.7.3.11 Table 32); any other byte is a
# builder-authoring error and must fail loudly, not render an invented name.
IDENTIFIER_NAMES = {0x18: "qsfp_dd"}
CONNECTOR_NAMES = {0x00: "unknown_or_unspecified"}

# Fixed authorship stamp: the golden must regenerate byte-for-byte, so the
# stamp is pinned here rather than taken from the clock at run time.
AUTHORED_AT = "2026-10-18"
AUTHORED_BY = "ras + claude-opus-4-8 via tools/build-cmis-3-0-synthetic.py"

# Baseline synthetic note — verbatim text of the all-zero Page 11h fixture's
# golden classification. Kept as a module constant so the reserved variant
# can sit beside it without either note being edited through the other.
_BASELINE_SYNTHETIC_NOTE = (
    "Limited synthetic parity golden, NOT a hardware oracle: every "
    "byte is hand-authored by tools/build-cmis-3-0-synthetic.py and "
    "the expected semantics derive from the CMIS 3.0 spec "
    "(CMIS3p0_Third_Party_Spec.pdf) applied to those bytes, not from "
    "a decoder run. Device revision is CMIS 3.0 (Lower:1=0x30); the "
    "canonical schema namespace every generated decoder is emitted "
    "from is cmis-5.3 (schemas/cmis) — no cmis-3.0 namespace exists. "
    "Every field this fixture exercises was verified to agree in "
    "offsets and encodings between CMIS 3.0 and CMIS 5.2/5.3: rev "
    "byte and Flat_mem "
    "(Table 18), ModuleState 000b=Reserved and Interrupt bit "
    "(Tables 18/19), module flags byte 9 (Table 21), temp/voltage "
    "monitors bytes 14-17 (Table 22), AppSel 1-8 descriptors at "
    "Lower 86-117 (Table 26), Data Path State 0h=Reserved with "
    "1h=Deactivated (Tables 65/66), lane monitors and the Page 01h:160 "
    "bias multiplier (Tables 70/45). Zeroed pages carry no "
    "field-content semantics: dp_state Reserved0 on every lane, "
    "no advertised Applications, 0.0 monitors. Nonconformance "
    "disclosure: the all-zero Application advertisement is an "
    "intentional nonconforming fixture — CMIS 3.0 § 1.7.2.7 "
    "requires every module to advertise at least one Application "
    "and requires FFh in the Host Electrical Code of the first "
    "unused Table 26 entry, and this fixture does neither. The "
    "authored bytes are preserved as-is; the golden's "
    "applications:null and decode_status:\"ok\" are the structural "
    "decode of those bytes — \"ok\" is decode-success, not "
    "conformance. Do not use as a "
    "field-round-trip or vendor-identity oracle."
)

# Reserved-variant synthetic note: same classification discipline, naming
# the CMIS 3.0 Table 66 reserved 5h-7h nibbles and the 0h/3h/4h controls the
# lane vector exercises. The lane labels are transcribed from Table 66
# (0h Reserved, 3h Deinit, 4h Activated, 5h-7h Reserved — the 4.0+ Table
# 8-58 TxTurnOn/TxTurnOff/Initialized meanings do NOT apply at 3.0), never
# taken from a decoder or generated projection.
_RESERVED_SYNTHETIC_NOTE = (
    "Limited synthetic parity golden, NOT a hardware oracle: every "
    "byte is hand-authored by tools/build-cmis-3-0-synthetic.py and "
    "the expected semantics derive from the CMIS 3.0 spec "
    "(CMIS3p0_Third_Party_Spec.pdf) applied to those bytes, not from "
    "a decoder run. Device revision is CMIS 3.0 (Lower:1=0x30); the "
    "canonical schema namespace every generated decoder is emitted "
    "from is cmis-5.3 (schemas/cmis) — no cmis-3.0 namespace exists. "
    "This variant exercises CMIS 3.0 § 1.7.7.1 Table 66 reserved "
    "Data Path State encodings: the Page 11h lane vector "
    "(lanes 1..8) is 0h, 5h, 6h, 7h, 3h, 4h, 0h, 3h — packed per "
    "Table 65 as 50 76 43 30 — so 5h/6h/7h are Reserved in 3.0 and "
    "render Unknown5/Unknown6/Unknown7 (the DPTxTurnOn/DPTxTurnOff/"
    "DPInitialized meanings only exist from CMIS 4.0 Table 8-58 on), "
    "while the controls render 0h Reserved (Reserved0), 3h Deinit "
    "(DpDeinit) and 4h Activated (DpActivated). Fields verified to "
    "agree in offsets and encodings between CMIS 3.0 and CMIS 5.2/5.3 "
    "are as the baseline fixture: rev byte and Flat_mem (Table 18), "
    "ModuleState 000b=Reserved and Interrupt bit (Tables 18/19), "
    "module flags byte 9 (Table 21), temp/voltage monitors bytes "
    "14-17 (Table 22), AppSel 1-8 descriptors at Lower 86-117 "
    "(Table 26), Data Path State packing and Table 66 meanings "
    "(Tables 65/66), lane monitors and the Page 01h:160 bias "
    "multiplier (Tables 70/45). The remaining Page 11h bytes are "
    "zero: lane flags (Tables 67-69) and lane monitors (Table 70) "
    "carry no content. Nonconformance disclosure: the all-zero "
    "Application advertisement is an intentional nonconforming "
    "fixture — CMIS 3.0 § 1.7.2.7 requires every module to advertise "
    "at least one Application and requires FFh in the Host Electrical "
    "Code of the first unused Table 26 entry, and this fixture does "
    "neither. The authored bytes are preserved as-is; the golden's "
    "applications:null and decode_status:\"ok\" are the structural "
    "decode of those bytes — \"ok\" is decode-success, not "
    "conformance. Do not use as a field-round-trip or vendor-identity "
    "oracle."
)

# Per-lane flag keys, in the golden's alphabetical layout (CMIS Page 11h
# lane flags: OpticalPower*Rx -> rx_power, LaserBias*Tx -> tx_bias,
# OpticalPower*Tx -> tx_power; LaserBias is Tx-only so there are no rx_bias
# flags).
_LANE_FLAG_KEYS = (
    "rx_power_high_alarm",
    "rx_power_low_alarm",
    "rx_power_high_warning",
    "rx_power_low_warning",
    "tx_bias_high_alarm",
    "tx_bias_low_alarm",
    "tx_bias_high_warning",
    "tx_bias_low_warning",
    "tx_power_high_alarm",
    "tx_power_low_alarm",
    "tx_power_high_warning",
    "tx_power_low_warning",
)


def _trim_ascii_sp(buf: bytes) -> str:
    """ASCII-decode, trimming trailing spaces only.

    Corpus convention (see the C/Go/PHP/Python aggregators): trailing 0x20
    padding is trimmed; NUL bytes would be preserved. The synthetic pages
    are space-padded ASCII, so only the space trim applies.
    """
    return buf.decode("ascii").rstrip(" ")


def _dp_state_name(nibble: int) -> str:
    try:
        return DP_STATE_NAMES[nibble]
    except KeyError:
        # Reserved per CMIS 3.0 Table 66 (5h-Fh; also 110b/111b-style gaps
        # can never occur in a 4-bit nibble). Render Unknown<n>, the corpus
        # rendering for reserved nibbles (see the dp-reserved golden's
        # "Unknown8" precedent) — never a 4.0+ state name.
        return f"Unknown{nibble}"


def _module_state_name(code: int) -> str:
    try:
        return MODULE_STATE_NAMES[code]
    except KeyError:
        return f"Unknown{code}"  # 110b/111b Reserved (Table 19)


def _app_entries(lower: bytes, page01h: bytes) -> list[dict]:
    """Advertised Application descriptors, per CMIS 3.0's own layout.

    AppSel codes 1-8: Lower memory bytes 86-117, 4 bytes per descriptor
    (HostElectricalInterfaceCode, ModuleMediaInterfaceCode, lane counts,
    HostLaneAssignmentOptions) — § 1.7.2.3 Table 26.
    AppSel codes 9-15: Upper Page 01h bytes 223-250 = page-relative 95-122,
    same 4-byte shape — § 1.7.4 Table 48.

    § 1.7.4: "All modules shall advertise at least one Application. The
    first unused entry in Table 26 shall have FFh in the Host Electrical Code
    field to indicate the end of the list of supported Applications."
    A slot whose MediaInterfaceID is 0x00 advertises no host-media
    combination and is skipped (corpus convention, matching the audit rule
    and every binding's parser); slot numbering is preserved so app_code
    stays the AppSel code.
    """
    entries: list[dict] = []
    tables = (
        ("lower", lower, 86, 8, 0),       # AppSel 1-8 (Table 26)
        ("page01h", page01h, 95, 7, 8),   # AppSel 9-15 (Table 48)
    )
    for _src, buf, start, count, base in tables:
        for i in range(count):
            off = start + 4 * i
            hif, mif, lanes, hlao = buf[off], buf[off + 1], buf[off + 2], buf[off + 3]
            if hif == 0xFF:
                break  # End of the list of supported Applications
            if mif == 0x00:
                continue  # Unadvertised slot; position preserved
            entries.append(
                {
                    "app_code": base + i + 1,
                    "host_interface_id": hif,
                    "host_interface_name": _lookup_fail_closed("host", hif),
                    "host_lane_count": lanes >> 4,
                    "host_lane_assignment_options": f"0x{hlao:02X}",
                    "media_interface_id": mif,
                    "media_interface_name": _lookup_fail_closed("media", mif),
                    "media_lane_count": lanes & 0xF,
                }
            )
    return entries


def _lookup_fail_closed(kind: str, code: int) -> str:
    """Interface-ID names are not needed for this fixture's golden.

    The synthetic advertisement is empty (all descriptor slots skipped), so
    no host/media interface name is ever rendered. If a future edit
    populates a descriptor, this raises instead of inventing a name: the
    name must come from the SFF-8024 schema enums (host-ei-ids.yaml and the
    media tables selected by Lower:85 ModuleTypeEncoding, Table 25), which
    this builder does not carry.
    """
    raise NotImplementedError(
        f"{kind} interface id 0x{code:02X} named lookups are not carried by "
        "this builder; extend it from schemas/sff-8024/ before populating "
        "application descriptors"
    )


def expected_golden(
    lower: bytes, p00: bytes, p01: bytes, p11: bytes, *, reserved_variant: bool = False
) -> dict:
    """Expected decode of the authored bytes, from CMIS 3.0 tables only.

    Every field below cites the CMIS 3.0 table it decodes from; the value is
    computed from the bytes this script authors, NOT captured from a
    decoder. Where a zero byte decodes to a reserved/unknown code, the
    golden says so explicitly (Reserved0 / unknown_or_unspecified) instead
    of guessing a meaning.

    With ``reserved_variant=True`` the metadata carries the reserved
    variant's own stem, pinned authorship date and synthetic note; the
    default (keyword omitted or False) reproduces the original baseline
    golden byte-for-byte.
    """
    stem = RESERVED_STEM if reserved_variant else STEM
    authored_at = RESERVED_AUTHORED_AT if reserved_variant else AUTHORED_AT
    authored_by = AUTHORED_BY_RESERVED if reserved_variant else AUTHORED_BY
    synthetic_note = _RESERVED_SYNTHETIC_NOTE if reserved_variant else _BASELINE_SYNTHETIC_NOTE
    # --- module-level scalars (Lower memory, Table 18) ---
    major, minor = lower[1] >> 4, lower[1] & 0xF            # Rev byte, Table 18
    memory_model = "flat" if lower[2] & 0x80 else "paged"  # Flat_mem, Table 18
    module_state = _module_state_name((lower[3] >> 1) & 0x7)  # Table 19
    # Byte 3 bit 0 "Interrupt": 0b=asserted, 1b=not asserted (Table 18).
    interrupt_asserted = (lower[3] & 0x01) == 0

    # --- monitors (Table 22, Module-Level Monitors) ---
    # Bytes 14-15: signed 2's complement in 1/256 degree C.
    temp_raw = int.from_bytes(lower[14:16], "big", signed=True)
    module_temperature_c = temp_raw / 256.0
    # Bytes 16-17: 3.3 V supply in 100 uV increments.
    vcc_raw = int.from_bytes(lower[16:18], "big")
    module_voltage_v = vcc_raw * 100 / 1_000_000

    # --- module flags (Table 21, byte 9 latched flags) ---
    flags = {
        "data_not_ready": None,  # CMIS has no such bit; cross-spec shape parity
        "interrupt_asserted": interrupt_asserted,
        "temp_high_alarm": bool(lower[9] & 0x01),
        "temp_low_alarm": bool(lower[9] & 0x02),
        "supply_high_alarm": bool(lower[9] & 0x10),
        "supply_low_alarm": bool(lower[9] & 0x20),
        "lanes": [],
    }

    # --- per-lane status + flags (Page 11h, Tables 65-70) ---
    lanes = []
    for i in range(8):
        # Tx Bias current measurement and threshold multiplier, Page 01h
        # byte 160 bits 4-3 (Table 45): 00b x1, 01b x2, 10b x4, 11b reserved.
        bias_code = (p01[160 - 128] >> 3) & 0x03
        if bias_code == 0x03:
            tx_bias_ma = None  # reserved multiplier is undefined
        else:
            # Lane monitors (Table 70), flat bytes 170-185 -> page-relative
            # 42-57 in this 128-byte buffer: Tx<i> Bias is a big-endian U16
            # at [42+2i:44+2i], in 2 uA increments times the multiplier.
            bias_raw = int.from_bytes(p11[42 + 2 * i : 44 + 2 * i], "big")
            tx_bias_ma = round(bias_raw * 0.002 * (1 << bias_code), 3)
        # Lane monitors (Table 70), flat bytes 154-201 -> page-relative
        # 26-73: Tx<i> Power at [26+2i:28+2i], Rx<i> Power at [58+2i:60+2i],
        # each a big-endian U16 in 0.1 uW (1e-4 mW) increments.
        tx_power_raw = int.from_bytes(p11[26 + 2 * i : 28 + 2 * i], "big")
        rx_power_raw = int.from_bytes(p11[58 + 2 * i : 60 + 2 * i], "big")
        # Data Path State nibble, Tables 65/66: byte 128+⌊lane/2⌋ packs two
        # lanes — odd lanes (1,3,5,7) in bits 3-0, even lanes (2,4,6,8) in
        # bits 7-4. 0-based lane i: byte i//2, shift (i%2)*4.
        nibble = (p11[i // 2] >> ((i % 2) * 4)) & 0xF
        # Lane status flags — schema fields failure_flag_tx (flat byte 135
        # = page-relative 7) and los_flag_rx (flat byte 147 = page-relative
        # 19), one bit per lane, bit lane-1 (§ 8.10.3 semantics): the
        # per-lane Tx-failure and Rx-loss-of-signal latches surfaced as
        # tx_fault / rx_los in the cross-binding lane contract. Both are
        # inside the flag region the guard below covers, so an all-zero
        # page reads False and a future non-zero flag byte fails closed
        # until its offsets are transcribed here.
        tx_fault = bool((p11[7] >> i) & 1)
        rx_los = bool((p11[19] >> i) & 1)
        lanes.append(
            {
                "dp_state": _dp_state_name(nibble),
                "index": i + 1,
                "rx_los": rx_los,
                "rx_power_mw": round(rx_power_raw * 0.0001, 4),
                "tx_bias_ma": tx_bias_ma,
                "tx_fault": tx_fault,
                "tx_power_mw": round(tx_power_raw * 0.0001, 4),
            }
        )
        # Lane-specific flags, Tables 68/69 (latched; § 1.7.7.2: "For normal
        # operation and default state, the bits in this field have the value
        # of 0b"). This fixture authors an all-zero Page 11h, so every flag
        # is False; a future non-zero page must transcribe the Tables 67-69
        # per-byte flag offsets here before this golden can carry real flag
        # content (the builder fails closed below rather than guessing).
        flags["lanes"].append({key: False for key in _LANE_FLAG_KEYS})
    # Unsupported-flags guard: Tables 67-69 place the lane flags at flat
    # bytes 134-152 = page-relative 6-24 of this 128-byte Page 11h buffer;
    # flat byte 153 (page-relative 25) is Reserved per Table 70, not a
    # flag byte, and the monitors start at flat byte 154 (page-relative 26).
    if any(p11[6:25]):
        raise NotImplementedError(
            "non-zero Page 11h flag region (bytes 134-152): transcribe CMIS 3.0 "
            "Tables 67-69 flag offsets in tools/build-cmis-3-0-synthetic.py "
            "before authoring flag content"
        )

    apps = _app_entries(lower, p01)

    return {
        "_metadata": {
            "authored_at": authored_at,
            "authored_by": authored_by,
            "cmis_version": f"{major}.{minor}",
            "fixture": stem,
            "monitoring_stale": True,  # zeroed monitors, not live captures
            "schema_version": "openxcvr.golden.v1",
            "spec": "cmis-5.3",  # canonical schema namespace (see synthetic_note)
            "spec_revision": "5.3",
            "synthetic": True,
            "synthetic_note": synthetic_note,
            "vendor_strings_synthetic": True,
        },
        "applications": apps if apps else None,
        "checksums": None,  # CMIS has no identity checksums in the golden shape
        "decode_status": "ok",
        "flags": flags,
        "identity": {
            "date_code": _trim_ascii_sp(p00[54:56])
            + _trim_ascii_sp(p00[56:58])
            + _trim_ascii_sp(p00[58:60]),
            "vendor_name": _trim_ascii_sp(p00[1:17]),
            "vendor_oui": f"{p00[17]:02X}:{p00[18]:02X}:{p00[19]:02X}",
            "vendor_pn": _trim_ascii_sp(p00[20:36]),
            "vendor_rev": _trim_ascii_sp(p00[36:38]),
            "vendor_sn": _trim_ascii_sp(p00[38:54]),
        },
        "lanes": lanes,
        "module": {
            "cmis_revision": f"{major}.{minor}",
            "connector": {
                "name": CONNECTOR_NAMES[p00[75]],
                "raw": f"0x{p00[75]:02X}",
            },
            "identifier": {
                "name": IDENTIFIER_NAMES[lower[0]],
                "raw": f"0x{lower[0]:02X}",
            },
            "memory_model": memory_model,
            "module_state": module_state,
            "module_type": None,  # vendor-PN-derived; synthetic PN carries no type
        },
        "monitoring": {
            "module_temperature_c": module_temperature_c,
            "module_voltage_v": module_voltage_v,
        },
    }


def main() -> int:
    if not FIXTURE_DIR.is_dir():
        print(f"error: fixture dir not found: {FIXTURE_DIR}", file=sys.stderr)
        return 2

    pages = {
        "lower":    lower_memory(),
        "page-00h": page_00h(),
        "page-01h": page_01h(),
        "page-02h": page_02h(),
        "page-10h": page_10h(),
        "page-11h": page_11h(),
    }
    # The reserved-DPSM variant shares every authored page with the baseline
    # except Page 11h (see `page_11h_reserved`).
    reserved_pages = {**pages, "page-11h": page_11h_reserved()}

    variants = [
        (STEM, pages, GOLDEN_PATH),
        (RESERVED_STEM, reserved_pages, RESERVED_GOLDEN_PATH),
    ]
    for stem, variant_pages, golden_path in variants:
        for tag, buf in variant_pages.items():
            assert len(buf) == 128, f"{stem}: page {tag} has {len(buf)} bytes, expected 128"
            out_path = FIXTURE_DIR / f"{stem}.{tag}.bin"
            out_path.write_bytes(buf)
            print(
                f"wrote {out_path.relative_to(FIXTURE_DIR.parent.parent.parent)} "
                f"({len(buf)} bytes)"
            )

        # Limited synthetic parity golden (see module docstring): expected
        # semantics computed from the authored bytes + CMIS 3.0 tables, written
        # in the corpus golden layout (alphabetical keys, 2-space indent,
        # ASCII-escaped, trailing newline) so regeneration is byte-stable.
        golden = expected_golden(
            variant_pages["lower"],
            variant_pages["page-00h"],
            variant_pages["page-01h"],
            variant_pages["page-11h"],
            reserved_variant=(stem == RESERVED_STEM),
        )
        golden_path.parent.mkdir(parents=True, exist_ok=True)
        golden_path.write_text(
            json.dumps(golden, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(f"wrote {golden_path.relative_to(GOLDEN_DIR_ROOT)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
