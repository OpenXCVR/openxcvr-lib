#!/usr/bin/env python3
"""Emit the synthetic-cmis-5-2-qsfp-dd fixture .bin files and parity golden.

This script is the durable source of truth for the synthetic CMIS 5.2
fixture under tests/fixtures/cmis/. Each byte's value is documented
inline with a primary-source citation (CMIS 5.2 § 8.x) so the corpus
remains auditable.

The fixture drives decoder-dispatch testing (MajorRevision-byte routes
correctly to the CMIS 5.2 branch) and carries a limited synthetic parity
golden (`expected_golden` below): the golden pins the structural decode of
the authored bytes — identity strings, the zero-value module/monitoring
paths, the all-Reserved Page 11h data-path states, and the empty
application advertisement. It is NOT a hardware oracle and NOT a
field-content test: no byte comes from a real module, and the zeroed
pages deliberately carry no field-content semantics (see
`tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.md`).

CMIS 5.2 source: `docs/stds/cmis/OIF-CMIS-05.2.pdf` (OIF, 2022-04-27).

The golden's expected semantics are derived from the authored bytes plus
the CMIS 5.2 spec tables cited inline — never by running a decoder and
capturing its output. Fields decode under the 5.2 revision's own tables;
where CMIS 5.2 and the canonical CMIS 5.3 schema namespace (`schemas/cmis/`,
the namespace every generated decoder is emitted from) agree in offsets
and encodings, the golden metadata states that agreement explicitly
instead of silently assuming it.

Run:
    python3 tools/build-cmis-5-2-synthetic.py
"""

from __future__ import annotations

import json
import pathlib
import sys

FIXTURE_DIR = pathlib.Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "cmis"
STEM = "synthetic-cmis-5-2-qsfp-dd"
GOLDEN_DIR_ROOT = pathlib.Path(__file__).resolve().parent.parent / "tests" / "golden"
GOLDEN_PATH = GOLDEN_DIR_ROOT / "cmis" / f"{STEM}.json"


def pad_ascii(s: str, width: int) -> bytes:
    """Left-aligned ASCII, space-padded to `width`. Truncates if too long."""
    b = s.encode("ascii")
    if len(b) > width:
        b = b[:width]
    return b + b" " * (width - len(b))


def lower_memory() -> bytes:
    """Lower memory (128 bytes, addresses 0-127). CMIS 5.2 § 8.2.

    Critical bytes for dispatch test:
      - byte 0: SFF8024Identifier — 0x18 (QSFP-DD), per SFF-8024 § 4.1.
      - byte 1: CmisRevision — 0x52 (upper nibble=5 major, lower=2 minor),
                per CMIS 5.2 § 8.2.1 Table 8-5.
      - byte 2: bit 7 = MemoryModel (0 = paged, 1 = flat). Set to 0
                so the fixture exercises the paged-memory decoder path.
                Bits 3-2 = MciMaxSpeed (00 = 400kHz). Other bits 0.
    All other bytes 0x00 (synthetic placeholder). The golden decodes
    those zero bytes honestly rather than inventing meaning: ModuleState
    000b = Reserved (Tables 8-6/8-7), InterruptDeasserted 0b = asserted
    (Table 8-6), monitors 0.0 (Table 8-10), module flags clear (Table 8-9).
    """
    buf = bytearray(128)
    buf[0] = 0x18  # QSFP-DD identifier
    buf[1] = 0x52  # CMIS 5.2 revision
    buf[2] = 0x00  # paged + intervention-free + 400kHz MCI
    return bytes(buf)


def page_00h() -> bytes:
    """Page 00h (Administrative). CMIS 5.2 § 8.3 Table 8-23.

    Page-relative offsets (flat = page-relative + 128):
      - 0   (128): SFF8024IdentifierCopy = 0x18 (mirror of Lower:0).
      - 1-16 (129-144): VendorName (16-byte ASCII, space-padded).
      - 17-19 (145-147): VendorOUI (3 bytes; 00 00 00 = synthetic).
      - 20-35 (148-163): VendorPN (16-byte ASCII).
      - 36-37 (164-165): VendorRev (2-byte ASCII).
      - 38-53 (166-181): VendorSN (16-byte ASCII).
      - 54-61 (182-189): DateCode (8-byte ASCII; YYMMDDLC per SFF-8472
                          convention — CMIS doesn't fully redefine, so
                          use SFF-8024-style YYMMDD + 2 lot chars).
      - 62-71 (190-199): CLEICode (10-byte ASCII; spaces = synthetic).
      - 72-93 (200-221): zeros (power, connector, attenuation,
                          media-lane info, etc. — synthetic).
      - 94   (222): PageChecksum = low-order 8 bits of sum of
                    page-relative bytes 0-93 (CMIS 5.2 § 8.3.11).
      - 95-127 (223-255): Custom[33] — zeros.

    Vendor strings carry the marker `CMIS_5_2_SYNTH` and `FIXTURE-SYNTH`
    in human-readable form so a casual hex-dump inspection identifies
    this as a synthetic fixture, not a real-vendor capture.
    """
    buf = bytearray(128)
    buf[0] = 0x18  # SFF8024IdentifierCopy

    # Vendor identity strings (synthetic markers)
    buf[1:17] = pad_ascii("CMIS_5_2_SYNTH", 16)         # VendorName
    # buf[17:20] = b"\x00\x00\x00"                       # VendorOUI (already 0)
    buf[20:36] = pad_ascii("FIXTURE-SYNTH-V1", 16)      # VendorPN
    buf[36:38] = pad_ascii("00", 2)                     # VendorRev
    buf[38:54] = pad_ascii("SYNTHETIC0000001", 16)      # VendorSN
    buf[54:62] = pad_ascii("22042700", 8)               # DateCode (CMIS 5.2 publish date 2022-04-27 + 2 lot chars)
    buf[62:72] = pad_ascii("", 10)                      # CLEICode (10 spaces)
    # buf[72:94]: zeros (already)
    # PageChecksum at offset 94, computed below
    # buf[95:128]: zeros (Custom[33])

    # PageChecksum: low-order 8 bits of arithmetic sum of bytes 0-93
    # (= flat 128-221) per CMIS 5.2 § 8.3.11.
    buf[94] = sum(buf[0:94]) & 0xFF
    return bytes(buf)


def page_01h() -> bytes:
    """Page 01h (Advertising). CMIS 5.2 § 8.4.

    All zeros = "module advertises no Applications and no optional
    features." Nonconformance disclosure: CMIS 5.2 § 8.2.11 requires
    every module to advertise at least one Application and requires FFh
    in the HostInterfaceID of the first unused Application Descriptor;
    this all-zero page does neither, so the fixture is intentionally
    nonconforming (the dispatch test only routes by Lower:1, CmisRevision,
    and the golden decodes the zero bytes honestly). Every additional
    Application Descriptor slot at bytes 223-250 (page-relative 95-122,
    Table 8-53) is unadvertised (MIF=0x00), and the
    TxBiasCurrentScalingFactor at byte 160 bits 4-3 (Table 8-47)
    decodes as 00b = multiply x1. Synthetic-zero satisfies the
    structural requirement that Page 01h be loadable as 128 bytes of
    paged memory.
    """
    return bytes(128)


def page_02h() -> bytes:
    """Page 02h (Module and Lane Thresholds). CMIS 5.2 § 8.5.

    All zeros = "no thresholds defined" (high-alarm/low-alarm/
    high-warning/low-warning bytes are all 0). A real fixture would
    have plausible threshold ranges; synthetic-zero is acceptable for
    dispatch testing since the test does not read this page.
    """
    return bytes(128)


def page_10h() -> bytes:
    """Banked Page 10h (Lane Control and Data Path Control). CMIS 5.2 § 8.9.

    All zeros = every control field at its default: DPDeinit bits clear
    (§ 8.9.1 Table 8-61 — no de-initialization requested; data paths
    auto-initialize once the module reaches ModuleReady), no output
    muting, no polarity inversion, squelch off. Synthetic-zero is
    acceptable for dispatch testing.
    """
    return bytes(128)


def page_11h() -> bytes:
    """Banked Page 11h (Lane Status and Data Path Status). CMIS 5.2 § 8.10.

    All zeros = every host lane's DPStateHostLane nibble is 0h, which
    CMIS 5.2 § 8.10.1 Table 8-77 defines as **Reserved** — NOT
    DPDeactivated (that is 1h, "DPDeactivated (or unused lane)"). The
    golden therefore renders every lane `dp_state: "Reserved0"` (the
    corpus PascalCase rendering of the schema enum name `reserved_0`).
    All lane flags are cleared (Tables 8-79/8-80/8-81) and all lane
    monitors read 0 (Table 8-82).
    """
    return bytes(128)


# ============================================================================
# Expected-decode tables — the CMIS 5.2 revision's OWN encodings
# ============================================================================
# Every table below is transcribed from the CMIS 5.2 spec PDF
# (OIF-CMIS-05.2.pdf) so the golden is authored from the named revision,
# never from CMIS 5.3-only semantics (the two agree everywhere this
# fixture exercises; the note in the golden metadata says so explicitly).

# DPStateHostLane — Page 11h bytes 128-131, one nibble per host lane (odd
# lanes bits 3-0, even lanes bits 7-4; Table 8-76).
# CMIS 5.2 § 8.10.1 Table 8-77:
#   0h Reserved; 1h DPDeactivated (or unused lane); 2h DPInit;
#   3h DPDeinit; 4h DPActivated; 5h DPTxTurnOn; 6h DPTxTurnOff;
#   7h DPInitialized; 8h-Fh Reserved.
# Names render in the cross-binding PascalCase convention the golden corpus
# uses ("Reserved0" precedent: fboss-cmis-4-0-qsfp112-200g-fr4.json, the
# schema enum name `reserved_0`). 8h-Fh render Unknown<n> (the dp-reserved
# golden's "Unknown8" precedent).
DP_STATE_NAMES = {
    0x0: "Reserved0",
    0x1: "DpDeactivated",
    0x2: "DpInit",
    0x3: "DpDeinit",
    0x4: "DpActivated",
    0x5: "DpTxTurnOn",
    0x6: "DpTxTurnOff",
    0x7: "DpInitialized",
}

# ModuleState — Lower memory byte 3 bits 3-1. CMIS 5.2 § 8.2.2
# Tables 8-6/8-7:
#   000b Reserved; 001b ModuleLowPwr; 010b ModulePwrUp; 011b ModuleReady;
#   100b ModulePwrDn; 101b ModuleFault; 110b/111b Reserved.
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
# SFF-8024 Table 4-3 / CMIS 5.2 § 8.3.5 Table 8-30); any other byte is a
# builder-authoring error and must fail loudly, not render an invented name.
IDENTIFIER_NAMES = {0x18: "qsfp_dd"}
CONNECTOR_NAMES = {0x00: "unknown_or_unspecified"}

# Fixed authorship stamp: the golden must regenerate byte-for-byte, so the
# stamp is pinned here rather than taken from the clock at run time.
AUTHORED_AT = "2026-10-18"
AUTHORED_BY = "ras + claude-opus-4-8 via tools/build-cmis-5-2-synthetic.py"

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
        # Reserved per CMIS 5.2 Table 8-77 (8h-Fh). Render Unknown<n>, the
        # corpus rendering for reserved nibbles (see the dp-reserved
        # golden's "Unknown8" precedent).
        return f"Unknown{nibble}"


def _module_state_name(code: int) -> str:
    try:
        return MODULE_STATE_NAMES[code]
    except KeyError:
        return f"Unknown{code}"  # 110b/111b Reserved (Table 8-7)


def _app_entries(lower: bytes, page01h: bytes) -> list[dict]:
    """Advertised Application descriptors, per CMIS 5.2's own layout.

    AppSel codes 1-8: Lower memory bytes 86-117, 4 bytes per descriptor
    (HostInterfaceIDApp<i>, MediaInterfaceIDApp<i>, lane counts,
    HostLaneAssignmentOptionsApp<i>) — § 8.2.11 Table 8-20.
    AppSel codes 9-15: Page 01h bytes 223-250 = page-relative 95-122, same
    4-byte shape — § 8.4.14 Table 8-53.

    § 8.4.14: "The HostInterfaceID field of the first unused descriptor in
    Table 8-53 shall have a value of FFh, indicating the end of the list of
    Application Descriptors." A slot whose MediaInterfaceID is 0x00
    advertises no host-media combination and is skipped (corpus convention,
    matching the audit rule and every binding's parser); slot numbering is
    preserved so app_code stays the AppSel code.
    """
    entries: list[dict] = []
    tables = (
        ("lower", lower, 86, 8, 0),       # AppSel 1-8 (Table 8-20)
        ("page01h", page01h, 95, 7, 8),   # AppSel 9-15 (Table 8-53)
    )
    for _src, buf, start, count, base in tables:
        for i in range(count):
            off = start + 4 * i
            hif, mif, lanes, hlao = buf[off], buf[off + 1], buf[off + 2], buf[off + 3]
            if hif == 0xFF:
                break  # End of the list of Application Descriptors
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
    media tables selected by Lower:85 MediaType, Table 8-14), which this
    builder does not carry.
    """
    raise NotImplementedError(
        f"{kind} interface id 0x{code:02X} named lookups are not carried by "
        "this builder; extend it from schemas/sff-8024/ before populating "
        "application descriptors"
    )


def expected_golden(lower: bytes, p00: bytes, p01: bytes, p11: bytes) -> dict:
    """Expected decode of the authored bytes, from CMIS 5.2 tables only.

    Every field below cites the CMIS 5.2 table it decodes from; the value is
    computed from the bytes this script authors, NOT captured from a
    decoder. Where a zero byte decodes to a reserved/unknown code, the
    golden says so explicitly (Reserved0 / unknown_or_unspecified) instead
    of guessing a meaning.
    """
    # --- module-level scalars (Lower memory, Table 8-5) ---
    major, minor = lower[1] >> 4, lower[1] & 0xF              # CmisRevision
    memory_model = "flat" if lower[2] & 0x80 else "paged"    # MemoryModel
    module_state = _module_state_name((lower[3] >> 1) & 0x7)  # Tables 8-6/8-7
    # Byte 3 bit 0 InterruptDeasserted: 0b=Interrupt asserted,
    # 1b=Interrupt not asserted (default) — Table 8-6.
    interrupt_asserted = (lower[3] & 0x01) == 0

    # --- monitors (§ 8.2.5 Table 8-10, Module-Level Monitor Values) ---
    # Bytes 14-15 TempMonValue: S16 in 1/256 degree Celsius increments.
    temp_raw = int.from_bytes(lower[14:16], "big", signed=True)
    module_temperature_c = temp_raw / 256.0
    # Bytes 16-17 VccMonVoltage: U16 in 100 uV increments.
    vcc_raw = int.from_bytes(lower[16:18], "big")
    module_voltage_v = vcc_raw * 100 / 1_000_000

    # --- module flags (§ 8.2.4 Table 8-9, byte 9 latched flags) ---
    flags = {
        "data_not_ready": None,  # CMIS has no such bit; cross-spec shape parity
        "interrupt_asserted": interrupt_asserted,
        "temp_high_alarm": bool(lower[9] & 0x01),
        "temp_low_alarm": bool(lower[9] & 0x02),
        "supply_high_alarm": bool(lower[9] & 0x10),
        "supply_low_alarm": bool(lower[9] & 0x20),
        "lanes": [],
    }

    # --- per-lane status + flags (Page 11h, Tables 8-76..8-82) ---
    lanes = []
    for i in range(8):
        # TxBiasCurrentScalingFactor, Page 01h byte 160 bits 4-3 (§ 8.4.9
        # Table 8-47): 00b x1, 01b x2, 10b x4, 11b reserved.
        bias_code = (p01[160 - 128] >> 3) & 0x03
        if bias_code == 0x03:
            tx_bias_ma = None  # reserved multiplier is undefined
        else:
            # Lane monitors (§ 8.10.4 Table 8-82), flat bytes 170-185 ->
            # page-relative 42-57 in this 128-byte buffer: LaserBiasTx<i> is
            # a big-endian U16 at [42+2i:44+2i], in 2 uA increments times
            # the multiplier.
            bias_raw = int.from_bytes(p11[42 + 2 * i : 44 + 2 * i], "big")
            tx_bias_ma = round(bias_raw * 0.002 * (1 << bias_code), 3)
        # Lane monitors (§ 8.10.4 Table 8-82): OpticalPowerTx bytes 154-169,
        # LaserBiasTx bytes 170-185, OpticalPowerRx bytes 186-201; powers in
        # 0.1 uW increments, bias in 2 uA increments times the multiplier.
        # Flat -> page-relative in this 128-byte buffer: Tx at [26+2i:28+2i],
        # Rx at [58+2i:60+2i], each a big-endian U16.
        tx_power_raw = int.from_bytes(p11[26 + 2 * i : 28 + 2 * i], "big")
        rx_power_raw = int.from_bytes(p11[58 + 2 * i : 60 + 2 * i], "big")
        # DPStateHostLane nibble, Tables 8-76/8-77: byte 128+⌊lane/2⌋ packs
        # two lanes — odd lanes (1,3,5,7) in bits 3-0, even lanes (2,4,6,8)
        # in bits 7-4. 0-based lane i: byte i//2, shift (i%2)*4.
        nibble = (p11[i // 2] >> ((i % 2) * 4)) & 0xF
        # Lane status flags — schema fields failure_flag_tx (Table 8-87,
        # flat byte 135 = page-relative 7) and los_flag_rx (Table 8-88,
        # flat byte 147 = page-relative 19), one bit per lane, bit lane-1:
        # the per-lane Tx-failure and Rx-loss-of-signal latches surfaced as
        # tx_fault / rx_los in the cross-binding lane contract. Both are
        # inside the flag region the fail-closed guard below covers, so an
        # all-zero page reads False and a future non-zero flag byte fails
        # closed until its offsets are transcribed here.
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
        # Lane-specific flags, Tables 8-79/8-80/8-81 (latched; § 8.10.3:
        # cleared in default state). This fixture authors an all-zero
        # Page 11h, so every flag is False; a future non-zero page must
        # transcribe the per-byte flag offsets here before this golden can
        # carry real flag content (the builder fails closed below rather
        # than guessing).
        flags["lanes"].append({key: False for key in _LANE_FLAG_KEYS})
    # Unsupported-flags guard: Table 8-75 places the Lane-specific Flags at
    # flat bytes 134-153 = page-relative 6-25 of this 128-byte Page 11h
    # buffer; the monitors start at flat byte 154 (page-relative 26).
    if any(p11[6:26]):
        raise NotImplementedError(
            "non-zero Page 11h flag region (bytes 134-153): transcribe CMIS 5.2 "
            "Tables 8-79..8-81 flag offsets in tools/build-cmis-5-2-synthetic.py "
            "before authoring flag content"
        )

    apps = _app_entries(lower, p01)

    return {
        "_metadata": {
            "authored_at": AUTHORED_AT,
            "authored_by": AUTHORED_BY,
            "cmis_version": f"{major}.{minor}",
            "fixture": STEM,
            "monitoring_stale": True,  # zeroed monitors, not live captures
            "schema_version": "openxcvr.golden.v1",
            "spec": "cmis-5.3",  # canonical schema namespace (see synthetic_note)
            "spec_revision": "5.3",
            "synthetic": True,
            "synthetic_note": (
                "Limited synthetic parity golden, NOT a hardware oracle: every "
                "byte is hand-authored by tools/build-cmis-5-2-synthetic.py and "
                "the expected semantics derive from the CMIS 5.2 spec "
                "(OIF-CMIS-05.2.pdf) applied to those bytes, not from a decoder "
                "run. Device revision is CMIS 5.2 (Lower:1=0x52); the canonical "
                "schema namespace every generated decoder is emitted from is "
                "cmis-5.3 (schemas/cmis). Every field this fixture exercises "
                "was verified to agree in offsets and encodings between CMIS "
                "5.2 and CMIS 5.3: rev byte and MemoryModel (Table 8-5), "
                "ModuleState 000b=Reserved and InterruptDeasserted bit "
                "(Tables 8-6/8-7), module flags byte 9 (Table 8-9), temp/voltage "
                "monitors bytes 14-17 (Table 8-10), AppSel 1-8 descriptors at "
                "Lower 86-117 (Table 8-20) and AppSel 9-15 at Page 01h:223-250 "
                "(Table 8-53), Data Path State 0h=Reserved with 1h=DPDeactivated "
                "(Tables 8-76/8-77), lane monitors and the Page 01h:160 bias "
                "multiplier (Tables 8-82/8-47). Zeroed pages carry no "
                "field-content semantics: dp_state Reserved0 on every lane, no "
                "advertised Applications, 0.0 monitors. Nonconformance "
                "disclosure: the all-zero Application advertisement is an "
                "intentional nonconforming fixture — CMIS 5.2 § 8.2.11 "
                "requires every module to advertise at least one Application "
                "and requires FFh in the HostInterfaceID of the first unused "
                "Application Descriptor, and this fixture does neither. The "
                "authored bytes are preserved as-is; the golden's "
                "applications:null and decode_status:\"ok\" are the structural "
                "decode of those bytes — \"ok\" is decode-success, not "
                "conformance. Do not use as a "
                "field-round-trip or vendor-identity oracle."
            ),
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

    for tag, buf in pages.items():
        assert len(buf) == 128, f"page {tag} has {len(buf)} bytes, expected 128"
        out_path = FIXTURE_DIR / f"{STEM}.{tag}.bin"
        out_path.write_bytes(buf)
        print(f"wrote {out_path.relative_to(FIXTURE_DIR.parent.parent.parent)} ({len(buf)} bytes)")

    # Limited synthetic parity golden (see module docstring): expected
    # semantics computed from the authored bytes + CMIS 5.2 tables, written
    # in the corpus golden layout (alphabetical keys, 2-space indent,
    # ASCII-escaped, trailing newline) so regeneration is byte-stable.
    golden = expected_golden(
        pages["lower"], pages["page-00h"], pages["page-01h"], pages["page-11h"]
    )
    GOLDEN_PATH.parent.mkdir(parents=True, exist_ok=True)
    GOLDEN_PATH.write_text(
        json.dumps(golden, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"wrote {GOLDEN_PATH.relative_to(GOLDEN_DIR_ROOT)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
