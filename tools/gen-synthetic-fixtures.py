#!/usr/bin/env python3
"""
Generate synthetic SFF-8472 and SFF-8636 test fixtures.

Writes .bin files, .md provenance files, and golden .json files for 6 fixtures
designed to exercise code paths not covered by the OOM bootstrap fixtures:
  SFF-8472: external-cal SFP+, passive-copper DAC, 25G SFP28
  SFF-8636: QSFP+ passive-copper DAC, QSFP28 BiDi 100G, QSFP28 AOC 100G-SR4

Usage: python3 tools/gen-synthetic-fixtures.py [--dry-run]
"""

import json
import struct
import pathlib
import datetime
import argparse

ROOT = pathlib.Path(__file__).parent.parent
FIXTURES = ROOT / "tests" / "fixtures"
GOLDEN = ROOT / "tests" / "golden"
TODAY = datetime.date.today().strftime("%y%m%d")
AUTHORED_BY = "ras + claude-sonnet-4-6 (gen-synthetic-fixtures.py)"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def csum(data: bytearray, start: int, end: int) -> int:
    """Low 8 bits of arithmetic sum of data[start:end+1]."""
    return sum(data[start : end + 1]) & 0xFF


def ascii16(s: str) -> bytes:
    return s.encode("ascii").ljust(16)[:16]


def ascii8(s: str) -> bytes:
    return s.encode("ascii").ljust(8)[:8]


def ascii4(s: str) -> bytes:
    return s.encode("ascii").ljust(4)[:4]


def ascii2(s: str) -> bytes:
    return s.encode("ascii").ljust(2)[:2]


def oui_bytes(oui_str: str) -> bytes:
    parts = oui_str.split(":")
    return bytes(int(x, 16) for x in parts)


def u16be(val: int) -> bytes:
    return struct.pack(">H", val)


def i16be(val: int) -> bytes:
    return struct.pack(">h", val)


def f32be(val: float) -> bytes:
    return struct.pack(">f", val)


# ---------------------------------------------------------------------------
# SFF-8472 A0h builder (128 bytes)
# ---------------------------------------------------------------------------

def build_sff8472_a0h(p: dict) -> bytearray:
    a = bytearray(128)
    a[0] = 0x03                            # Identifier: SFP/SFP+/SFP28
    a[1] = p.get("ext_id", 0x00)
    a[2] = p["connector"]
    for i, v in enumerate(p.get("compliance", [0] * 8)):
        a[3 + i] = v
    a[11] = p["encoding"]
    a[12] = p["bit_rate_100m"]             # nominal bit rate in 100 Mbps units
    a[13] = 0x00                           # rate identifier
    ll = p.get("link_length", {})
    a[14] = ll.get("smf_km", 0)
    a[15] = ll.get("smf_100m", 0)
    a[16] = ll.get("om2", 0)
    a[17] = ll.get("om1", 0)
    a[18] = ll.get("om4_or_copper_1m", 0)
    a[19] = ll.get("om3_byte", 0)          # multiplier + base in one byte
    a[20:36] = ascii16(p["vendor_name"])
    a[36] = p.get("ext_spec_compliance", 0)
    a[37:40] = oui_bytes(p.get("oui", "00:00:00"))
    a[40:56] = ascii16(p["vendor_pn"])
    a[56:60] = ascii4(p.get("vendor_rev", "    "))
    struct.pack_into(">H", a, 60, p.get("wavelength_nm", 0))
    a[63] = csum(a, 0, 62)                 # CC_BASE
    a[64] = p.get("options_64", 0)
    a[65] = p.get("options_65", 0)
    a[66] = p.get("signaling_rate_max", 0) # or bit-rate × 250 Mbps when [12]=0xFF
    a[67] = p.get("signaling_rate_min", 0)
    a[68:84] = ascii16(p["vendor_sn"])
    a[84:92] = ascii8(p.get("date_code", TODAY + "  "))
    a[92] = p["dmt"]                       # diagnostic monitoring type
    a[93] = p.get("enhanced_options", 0)
    a[94] = p.get("sff8472_compliance", 0)
    a[95] = csum(a, 64, 94)               # CC_EXT
    return a


# ---------------------------------------------------------------------------
# SFF-8472 A2h builder (256 bytes)
# ---------------------------------------------------------------------------

def build_sff8472_a2h(p: dict) -> bytearray:
    a = bytearray(256)

    # Thresholds bytes 0-55: all zeros (no alarm thresholds set)

    # External calibration constants bytes 56-91 (present only if ext-cal)
    if p.get("ext_cal", False):
        # Rx power polynomial coefficients, 5 IEEE-754 floats, degree 4 → 0
        # offset_expr = 72 - 4*degree → degree 0 at 72, degree 4 at 56
        for deg in range(5):
            offset = 72 - 4 * deg
            coeff = 1.0 if deg == 0 else 0.0  # degree-0 coefficient 1.0, i.e. constant RxPwr = 1.0 LSB; an identity would need the degree-1 coefficient
            a[offset : offset + 4] = f32be(coeff)
        # Linear terms: 8 × 2-byte entries at A2h:76-91
        # order: tx_bias_slope, tx_bias_offset, tx_power_slope, tx_power_offset,
        #        temp_slope, temp_offset, vcc_slope, vcc_offset
        for i in range(8):
            off = 76 + 2 * i
            val = 0x0100 if i % 2 == 0 else 0x0000  # slope=1.0 (8.8fp), offset=0
            a[off : off + 2] = u16be(val)

    # CC_DMI: byte 95, sum of bytes 0-94
    a[95] = csum(a, 0, 94)

    # Real-time monitors (bytes 96-105)
    mon = p.get("monitoring", {})
    temp = mon.get("temperature_c", 0.0)
    struct.pack_into(">h", a, 96, round(temp * 256))        # i16be, 1/256 °C
    vcc = mon.get("voltage_v", 0.0)
    struct.pack_into(">H", a, 98, round(vcc / 0.0001))      # u16be, 100 µV
    txbias = mon.get("tx_bias_ma", 0.0)
    struct.pack_into(">H", a, 100, round(txbias / 0.002))   # u16be, 2 µA
    txpwr = mon.get("tx_power_mw", 0.0)
    struct.pack_into(">H", a, 102, round(txpwr * 10000))    # u16be, 0.1 µW
    rxpwr = mon.get("rx_power_mw", 0.0)
    struct.pack_into(">H", a, 104, round(rxpwr * 10000))    # u16be, 0.1 µW

    # Alarm/warning flags at bytes 112-113, 116-117: all zeros (no alarms)
    return a


# ---------------------------------------------------------------------------
# SFF-8636 Lower Page builder (128 bytes)
# ---------------------------------------------------------------------------

def build_sff8636_lower(p: dict) -> bytearray:
    a = bytearray(128)
    a[0] = p["identifier"]                 # 0x0D = QSFP+, 0x11 = QSFP28
    a[1] = p.get("revision_compliance", 0x00)
    # Byte 2: status — bit 2 = flat_memory, bit 1 = interrupt_deasserted, bit 0 = data_not_ready
    a[2] = p.get("status_byte", 0b00000010)  # interrupt deasserted (module OK)
    # Bytes 3-21: interrupt/alarm flags — all zeros (no faults)
    # Temperature: bytes 22-23, i16be, 1/256 °C
    temp = p.get("temperature_c", 0.0)
    struct.pack_into(">h", a, 22, round(temp * 256))
    # Supply voltage: bytes 26-27, u16be, 100 µV
    vcc = p.get("voltage_v", 0.0)
    struct.pack_into(">H", a, 26, round(vcc / 0.0001))
    # Per-lane Rx power: bytes 34+2*(lane-1), u16be, 0.1 µW
    for i, rxpwr in enumerate(p.get("rx_power_mw", [0.0] * 4)):
        struct.pack_into(">H", a, 34 + 2 * i, round(rxpwr * 10000))
    # Per-lane Tx bias: bytes 42+2*(lane-1), u16be, 2 µA
    for i, bias in enumerate(p.get("tx_bias_ma", [0.0] * 4)):
        struct.pack_into(">H", a, 42 + 2 * i, round(bias / 0.002))
    # Per-lane Tx power: bytes 50+2*(lane-1), u16be, 0.1 µW
    for i, txpwr in enumerate(p.get("tx_power_mw", [0.0] * 4)):
        struct.pack_into(">H", a, 50 + 2 * i, round(txpwr * 10000))
    return a


# ---------------------------------------------------------------------------
# SFF-8636 Page 00h builder (128 bytes = flat offsets 128-255)
# Here all "offset" values are page-relative (file offset = flat - 128)
# ---------------------------------------------------------------------------

def build_sff8636_page00h(p: dict) -> bytearray:
    a = bytearray(128)
    a[0] = p["identifier"]                   # byte 0 (flat 128)
    a[1] = p.get("ext_id_power", 0x00)       # power class + CDR
    a[2] = p["connector"]                    # flat 130
    for i, v in enumerate(p.get("compliance", [0] * 8)):
        a[3 + i] = v                         # flat 131-138
    a[11] = p["encoding"]                    # flat 139
    a[12] = p["bit_rate_100m"]               # flat 140
    a[13] = 0x00                             # extended rate select version
    ll = p.get("link_length", {})
    a[14] = ll.get("smf_km", 0)             # flat 142
    a[15] = ll.get("om3_2m", 0)             # flat 143
    a[16] = ll.get("om2_1m", 0)             # flat 144
    a[17] = ll.get("om1_or_copper", 0)      # flat 145
    a[18] = ll.get("cable_or_om4", 0)       # flat 146
    # Device technology: bits 4-7, lower bits are control flags
    a[19] = (p.get("device_tech", 0) & 0x0F) << 4  # flat 147
    a[20:36] = ascii16(p["vendor_name"])     # flat 148-163
    # KNOWN-GAP (tracked follow-up): ext_spec_compliance is written here at offset 36
    # (flat 164), but the SFF-8636 Extended Specification Compliance byte is offset 64
    # (flat 192) per schemas/sff-8636 — which is where decode_sff8636 / bindings read it
    # (see make_sff8636_golden: page00h[64]). Net effect: ext_spec_byte_192 always decodes
    # null for these synthetic fixtures, so they don't exercise the extended-compliance
    # path. NOT moved here on purpose: relocating the write changes the committed fixture
    # bytes + CC_EXT and, for the non-0x02 fixtures (0x18 CWDM4, 0x1C 25G-SR), breaks
    # parity because the generator's hex fallback name diverges from the decoders'
    # "unknown"/"" fallback. Proper fix = move write to offset 64 AND align the ext-spec
    # name maps across generator + all four bindings; done as its own change, not the flood fix.
    a[36] = p.get("ext_spec_compliance", 0)  # flat 164 (see KNOWN-GAP above)
    a[37:40] = oui_bytes(p.get("oui", "00:00:00"))  # flat 165-167
    a[40:56] = ascii16(p["vendor_pn"])       # flat 168-183
    a[56:58] = ascii2(p.get("vendor_rev", "  "))     # flat 184-185
    struct.pack_into(">H", a, 58, p.get("wavelength_raw", 0))   # flat 186-187
    # Bytes 58-62 cover optional wavelength tolerance, FC compliance, cable attenuation
    a[63] = csum(a, 0, 62)                   # CC_BASE (page-relative bytes 0-62)
    a[68:84] = ascii16(p["vendor_sn"])       # flat 196-211
    a[84:92] = ascii8(p.get("date_code", TODAY + "  "))  # flat 212-219
    a[95] = csum(a, 64, 94)                  # CC_EXT (page-relative bytes 64-94)
    return a


# ---------------------------------------------------------------------------
# Golden JSON helpers
# ---------------------------------------------------------------------------

def exact(val: float, decimals: int) -> float:
    """Format to exact LSB precision without float noise."""
    return round(val, decimals)


def make_sff8472_golden(stem: str, p: dict, a0h: bytearray, a2h: bytearray) -> dict:
    """Build golden JSON from decoded byte values."""
    dmt = a0h[92]
    ddm = bool(dmt & 0x40)
    int_cal = bool(dmt & 0x20)
    ext_cal = bool(dmt & 0x10)
    rx_avg = bool(dmt & 0x08)

    # Identity from A0h
    vendor_name = a0h[20:36].decode("ascii").rstrip(" ")
    vendor_oui = ":".join(f"{b:02X}" for b in a0h[37:40])
    vendor_pn = a0h[40:56].decode("ascii").rstrip(" ")
    vendor_sn = a0h[68:84].decode("ascii").rstrip(" ")
    date_code = a0h[84:90].decode("ascii")  # first 6 bytes = YYMMDD

    # Nominal bit rate
    br_raw = a0h[12]
    if br_raw == 0xFF:
        nominal_gbps = exact(a0h[66] * 0.25, 4)
    else:
        nominal_gbps = exact(br_raw * 0.1, 4)

    # Compliance: raw bytes + wavelength/reach
    wl_nm = struct.unpack_from(">H", a0h, 60)[0]
    compliance = {
        "raw_bytes_a0h_3_10": [f"0x{v:02X}" for v in a0h[3:11]],
        "reach": {
            "om1_m": a0h[17] * 10,
            "om2_m": a0h[16] * 10,
            # SFF-8472 byte 19 is a flat OM3 length in units of 10 m (NOT a
            # base+multiplier split — that schema reading is falsified by the
            # finisar real-capture golden). Mirrors bindings/*/sff8472 (byte * 10).
            "om3_m": a0h[19] * 10,
            # byte 18 = OM4 length / copper-or-DAC length in units of 1 m (raw,
            # no ×10). Mirrors bindings/*/sff8472 (length_om4_or_copper_1m).
            "om4_m": a0h[18],
            "smf_km": a0h[14],
        },
        "wavelength_nm": wl_nm,
    }

    # Checksums
    cc_base_stored = a0h[63]
    cc_base_computed = csum(a0h, 0, 62)
    cc_ext_stored = a0h[95]
    cc_ext_computed = csum(a0h, 64, 94)
    cc_dmi_stored = a2h[95]
    cc_dmi_computed = csum(a2h, 0, 94)

    # Monitoring (only when DDM implemented)
    monitoring = None
    lanes = None
    if ddm:
        temp_raw = struct.unpack_from(">h", a2h, 96)[0]
        vcc_raw = struct.unpack_from(">H", a2h, 98)[0]
        monitoring = {
            "module_temperature_c": exact(temp_raw / 256, 7),
            "module_voltage_v": exact(vcc_raw * 0.0001, 4),
        }
        txbias_raw = struct.unpack_from(">H", a2h, 100)[0]
        txpwr_raw = struct.unpack_from(">H", a2h, 102)[0]
        rxpwr_raw = struct.unpack_from(">H", a2h, 104)[0]
        lanes = [
            {
                "index": 1,
                "rx_los": False,
                "rx_power_mw": exact(rxpwr_raw * 0.0001, 4),
                "tx_bias_ma": exact(txbias_raw * 0.002, 3),
                "tx_disable": False,
                "tx_fault": False,
                "tx_power_mw": exact(txpwr_raw * 0.0001, 4),
            }
        ]

    # Flags (only when DDM implemented, use byte 112-113 from A2h)
    flags = None
    if ddm:
        alarm_byte = a2h[112]
        warn_byte = a2h[116]
        flags = {
            "alarm_summary": {
                "rx_power_high_alarm": bool(alarm_byte & 0x01),
                "rx_power_low_alarm": bool(alarm_byte & 0x02),
                "temp_high_alarm": bool(alarm_byte & 0x80),
                "temp_low_alarm": bool(alarm_byte & 0x40),
                "tx_bias_high_alarm": bool(alarm_byte & 0x08),
                "tx_bias_low_alarm": bool(alarm_byte & 0x04),
                "tx_power_high_alarm": bool(alarm_byte & 0x20),
                "tx_power_low_alarm": bool(alarm_byte & 0x10),
                "vcc_high_alarm": bool(a2h[113] & 0x40),
                "vcc_low_alarm": bool(a2h[113] & 0x20),
            },
            "warning_summary": {
                "rx_power_high_warning": bool(warn_byte & 0x01),
                "rx_power_low_warning": bool(warn_byte & 0x02),
                "temp_high_warning": bool(warn_byte & 0x80),
                "temp_low_warning": bool(warn_byte & 0x40),
                "tx_bias_high_warning": bool(warn_byte & 0x08),
                "tx_bias_low_warning": bool(warn_byte & 0x04),
                "tx_power_high_warning": bool(warn_byte & 0x20),
                "tx_power_low_warning": bool(warn_byte & 0x10),
                "vcc_high_warning": bool(a2h[117] & 0x40),
                "vcc_low_warning": bool(a2h[117] & 0x20),
            },
        }

    # Identifier name from enum
    id_name_map = {0x03: "sfp_sff8472"}
    conn_name_map = {0x07: "lc", 0x21: "copper_pigtail", 0x22: "optical_pigtail", 0x23: "mpo_1x12_no_pins"}
    enc_name_map = {0x06: "sixty_four_b_sixty_six_b_or_legacy_manchester", 0x01: "eight_b_ten_b_or_fc", 0x05: "sonet_scrambled_or_legacy_64b66b"}

    # module_type: the decoder labels ONLY the 10GBASE-SR SFP+ case; everything else
    # (incl. 25G, passive copper) decodes to null. The trigger is transceiver_compliance
    # bit 3 (schema page-a0h-base-memory-part-01.yaml: offset_expr "3 + bit/8",
    # bit_offset_expr "7 - bit%8" → bit 3 = byte a0h[3], bit 4 = mask 0x10, the
    # 10GBASE-SR compliance code). Mirrors bindings/*/sff8472 decode_sff8472:
    # `transceiver_compliance[3]`. The fixture-definition marketing string is NOT used.
    module_type = "10GBASE-SR SFP+" if (a0h[3] & 0x10) else None

    return {
        "_metadata": {
            "authored_at": str(datetime.date.today()),
            "authored_by": AUTHORED_BY,
            "cmis_version": None,
            "fixture": stem,
            "monitoring_stale": p.get("monitoring_stale", False),
            "schema_version": "openxcvr.golden.v1",
            "spec": "sff-8472",
            "spec_revision": "12.5a",
            "vendor_strings_synthetic": True,
        },
        "checksums": {
            "cc_base": {"computed": f"0x{cc_base_computed:02X}", "stored": f"0x{cc_base_stored:02X}", "valid": cc_base_computed == cc_base_stored},
            "cc_dmi": {"computed": f"0x{cc_dmi_computed:02X}", "stored": f"0x{cc_dmi_stored:02X}", "valid": cc_dmi_computed == cc_dmi_stored},
            "cc_ext": {"computed": f"0x{cc_ext_computed:02X}", "stored": f"0x{cc_ext_stored:02X}", "valid": cc_ext_computed == cc_ext_stored},
        },
        "compliance": compliance,
        "decode_status": "ok",
        "flags": flags,
        "identity": {
            "date_code": date_code,
            "vendor_name": vendor_name,
            "vendor_oui": vendor_oui,
            "vendor_pn": vendor_pn,
            "vendor_sn": vendor_sn,
        },
        "lanes": lanes,
        "module": {
            "calibration": {
                "ddm_implemented": ddm,
                "externally_calibrated": ext_cal,
                "internally_calibrated": int_cal,
                "rx_power_kind": "average" if rx_avg else "oma",
            },
            "cmis_revision": None,
            "connector": {"name": conn_name_map.get(a0h[2], f"0x{a0h[2]:02X}"), "raw": f"0x{a0h[2]:02X}"},
            "encoding": {"name": enc_name_map.get(a0h[11], f"0x{a0h[11]:02X}"), "raw": f"0x{a0h[11]:02X}"},
            "identifier": {"name": id_name_map.get(a0h[0], f"0x{a0h[0]:02X}"), "raw": f"0x{a0h[0]:02X}"},
            "memory_model": None,
            "module_state": None,
            "module_type": module_type,
            "nominal_bit_rate_gbps": nominal_gbps,
        },
        "monitoring": monitoring,
    }


def make_sff8636_golden(stem: str, p: dict, lower: bytearray, page00h: bytearray) -> dict:
    """Build golden JSON for SFF-8636 fixture."""
    # Identity from page 00h (page-relative offsets = file offsets)
    vendor_name = page00h[20:36].decode("ascii").rstrip(" ")
    vendor_oui = ":".join(f"{b:02X}" for b in page00h[37:40])
    vendor_pn = page00h[40:56].decode("ascii").rstrip(" ")
    vendor_rev = page00h[56:58].decode("ascii").rstrip(" ")
    vendor_sn = page00h[68:84].decode("ascii").rstrip(" ")
    date_code = page00h[84:90].decode("ascii")

    # Bit rate
    br_raw = page00h[12]
    nominal_gbps = exact(br_raw * 0.1, 4)

    # Identifier and connector names
    id_name_map = {0x0D: "qsfp_plus_sff8636", 0x11: "qsfp28_sff8636"}
    conn_name_map = {0x0C: "mpo_1x12", 0x07: "lc", 0x21: "copper_pigtail", 0x01: "sc"}
    enc_name_map = {0x05: "sonet_scrambled_or_legacy_64b66b", 0x06: "sixty_four_b_sixty_six_b_or_legacy_manchester"}
    id_byte = lower[0]
    conn_byte = page00h[2]
    enc_byte = page00h[11]

    # Compliance bits from page00h bytes 3-10 (page-relative)
    bits_set = []
    compliance_bit_names = {
        (3, 7): "compliance_extended",
        (3, 6): "compliance_10gbase_lrm",
        (3, 5): "compliance_10gbase_lr",
        (3, 4): "compliance_10gbase_sr",
        (3, 3): "compliance_40gbase_cr4",
        (3, 2): "compliance_40gbase_sr4",
        (3, 1): "compliance_40gbase_lr4",
        (3, 0): "compliance_40g_active_cable",
    }
    for (byte_off, bit), name in compliance_bit_names.items():
        if page00h[byte_off] & (1 << bit):
            bits_set.append(name)

    # Device technology: bits 4-7 of page00h[19]. Names are the authoritative
    # SFF-8636 v2.11 § 6.3.13 Table 6-20 labels (schemas/sff-8636/enums/
    # device-technology.yaml = enum Sff8636DeviceTechnology), which the bindings
    # decode via .Label(). Keep this in lockstep with that enum.
    dev_tech_raw = (page00h[19] >> 4) & 0x0F
    dev_tech_name_map = {
        0x0: "nm850_vcsel", 0x1: "nm1310_vcsel", 0x2: "nm1550_vcsel", 0x3: "nm1310_fp",
        0x4: "nm1310_dfb", 0x5: "nm1550_dfb", 0x6: "nm1310_eml", 0x7: "nm1550_eml",
        0x8: "other_or_undefined", 0x9: "nm1490_dfb", 0xA: "copper_unequalized",
        0xB: "copper_passive_equalized", 0xC: "copper_near_far_limiting_active_equalizers",
        0xD: "copper_far_limiting_active_equalizers", 0xE: "copper_near_limiting_active_equalizers",
        0xF: "copper_linear_active_equalizers",
    }

    # ext_spec_compliance (SFF-8636 flat byte 192 = upper-page-00h offset 64) and
    # module_type both mirror bindings/*/sff8636. ext_spec is null when zero, else
    # {name, raw}. module_type is a label the decoder synthesizes ONLY for
    # recognized SR4 variants (ext_spec 0x02 → 100GBASE-SR4; the 40GBASE-SR4
    # compliance bit page00h byte 3 bit 2 → 40GBASE-SR4); everything else decodes
    # to null — the fixture-definition marketing string is NOT a decoded field.
    ext_spec_raw = page00h[64]
    ext_spec_name_map = {0x02: "base_100g_sr4_or_25g_sr"}
    ext_spec_field = (
        {"name": ext_spec_name_map.get(ext_spec_raw, f"0x{ext_spec_raw:02X}"), "raw": f"0x{ext_spec_raw:02X}"}
        if ext_spec_raw else None
    )
    if ext_spec_raw == 0x02:
        module_type = "100GBASE-SR4 QSFP28"
    elif page00h[3] & (1 << 2):
        module_type = "40GBASE-SR4 QSFP+"
    else:
        module_type = None

    # Checksums
    cc_base_stored = page00h[63]
    cc_base_computed = csum(page00h, 0, 62)
    cc_ext_stored = page00h[95]
    cc_ext_computed = csum(page00h, 64, 94)

    # Monitoring (module-level from lower page)
    temp_raw = struct.unpack_from(">h", lower, 22)[0]
    vcc_raw = struct.unpack_from(">H", lower, 26)[0]
    monitoring = {
        "module_temperature_c": exact(temp_raw / 256, 7),
        "module_voltage_v": exact(vcc_raw * 0.0001, 4),
    }

    # Per-lane monitoring
    active_lanes = p.get("active_lanes", 4)
    lanes = []
    for lane in range(1, active_lanes + 1):
        i = lane - 1
        rx_raw = struct.unpack_from(">H", lower, 34 + 2 * i)[0]
        bias_raw = struct.unpack_from(">H", lower, 42 + 2 * i)[0]
        tx_raw = struct.unpack_from(">H", lower, 50 + 2 * i)[0]
        lanes.append({
            "index": lane,
            "rx_power_mw": exact(rx_raw * 0.0001, 4),
            "tx_bias_ma": exact(bias_raw * 0.002, 3),
            "tx_power_mw": exact(tx_raw * 0.0001, 4),
        })

    # Flags from lower page bytes 3-7 (interrupt/alarm flags; all zeros for us)
    flags = {
        "data_not_ready": bool(lower[2] & 0x01),
        "interrupt_asserted": not bool(lower[2] & 0x02),
    }

    return {
        "_metadata": {
            "authored_at": str(datetime.date.today()),
            "authored_by": AUTHORED_BY,
            "cmis_version": None,
            "fixture": stem,
            "monitoring_stale": p.get("monitoring_stale", False),
            "schema_version": "openxcvr.golden.v1",
            "spec": "sff-8636",
            "spec_revision": "2.11",
            "vendor_strings_synthetic": True,
        },
        "checksums": {
            "cc_base": {"computed": f"0x{cc_base_computed:02X}", "stored": f"0x{cc_base_stored:02X}", "valid": cc_base_computed == cc_base_stored},
            "cc_ext": {"computed": f"0x{cc_ext_computed:02X}", "stored": f"0x{cc_ext_stored:02X}", "valid": cc_ext_computed == cc_ext_stored},
        },
        "compliance": {
            "bits_set": bits_set,
            "device_technology": {"name": dev_tech_name_map.get(dev_tech_raw, f"0x{dev_tech_raw:X}"), "raw": f"0x{dev_tech_raw:X}"},
            "ext_spec_byte_192": ext_spec_field,
            "raw_bytes_page_00h_3_10": [f"0x{v:02X}" for v in page00h[3:11]],
            "reach": {
                "om1_m": page00h[17],
                "om2_m": page00h[16],
                "om3_2m_units": page00h[15],
                "om4_or_copper_m": page00h[18],
                "smf_km": page00h[14],
            },
        },
        "decode_status": "ok",
        "flags": flags,
        "identity": {
            "date_code": date_code,
            "vendor_name": vendor_name,
            "vendor_oui": vendor_oui,
            "vendor_pn": vendor_pn,
            "vendor_rev": vendor_rev,
            "vendor_sn": vendor_sn,
        },
        "lanes": lanes,
        "module": {
            "cmis_revision": None,
            "connector": {"name": conn_name_map.get(conn_byte, f"0x{conn_byte:02X}"), "raw": f"0x{conn_byte:02X}"},
            "encoding": {"name": enc_name_map.get(enc_byte, f"0x{enc_byte:02X}"), "raw": f"0x{enc_byte:02X}"},
            "extended_identifier": f"0x{page00h[1]:02X}",
            "identifier": {"name": id_name_map.get(id_byte, f"0x{id_byte:02X}"), "raw": f"0x{id_byte:02X}"},
            "memory_model": "paged",
            "module_state": None,
            "module_type": module_type,
            "nominal_bit_rate_gbps": nominal_gbps,
        },
        "monitoring": monitoring,
    }


# ---------------------------------------------------------------------------
# Provenance MD builder
# ---------------------------------------------------------------------------

def make_sff8472_md(stem: str, p: dict, a0h: bytearray) -> str:
    wl = struct.unpack_from(">H", a0h, 60)[0]
    dmt = a0h[92]
    return f"""# Fixture: {stem}

**Source:** Synthetic — hand-generated by `tools/gen-synthetic-fixtures.py`
**Spec:** SFF-8472 v12.5a
**Module type:** {p['module_type']}

## Identity

| Field | Value |
|---|---|
| Vendor name | `{a0h[20:36].decode('ascii').rstrip()}` |
| Vendor OUI | `{':'.join(f'{b:02X}' for b in a0h[37:40])}` |
| Vendor PN | `{a0h[40:56].decode('ascii').rstrip()}` |
| Vendor S/N | `{a0h[68:84].decode('ascii').rstrip()}` |
| Date code | `{a0h[84:90].decode('ascii')}` (YYMMDD) |

## Key schema interactions

{p['md_notes']}

## Diagnostic Monitoring Type (A0h:92 = 0x{dmt:02X})

| Bit | Name | Value |
|---|---|---|
| 6 | DDM implemented | {'1' if dmt & 0x40 else '0'} |
| 5 | Internally calibrated | {'1' if dmt & 0x20 else '0'} |
| 4 | Externally calibrated | {'1' if dmt & 0x10 else '0'} |
| 3 | Rx power: average | {'1' if dmt & 0x08 else '0'} |

## Compliance (A0h:3-10)

`{[f'0x{v:02X}' for v in a0h[3:11]]}` — Wavelength: {wl} nm

## Checksums

| Name | Covers | Stored | Valid |
|---|---|---|---|
| CC_BASE | A0h:0–62 | 0x{a0h[63]:02X} | ✓ |
| CC_EXT | A0h:64–94 | 0x{a0h[95]:02X} | ✓ |
"""


def make_sff8636_md(stem: str, p: dict, lower: bytearray, page00h: bytearray) -> str:
    return f"""# Fixture: {stem}

**Source:** Synthetic — hand-generated by `tools/gen-synthetic-fixtures.py`
**Spec:** SFF-8636 v2.11
**Module type:** {p['module_type']}

## Identity

| Field | Value |
|---|---|
| Vendor name | `{page00h[20:36].decode('ascii').rstrip()}` |
| Vendor OUI | `{':'.join(f'{b:02X}' for b in page00h[37:40])}` |
| Vendor PN | `{page00h[40:56].decode('ascii').rstrip()}` |
| Vendor S/N | `{page00h[68:84].decode('ascii').rstrip()}` |
| Date code | `{page00h[84:90].decode('ascii')}` (YYMMDD) |
| Identifier | 0x{lower[0]:02X} |

## Key schema interactions

{p['md_notes']}

## Compliance (Page 00h:3-10)

`{[f'0x{v:02X}' for v in page00h[3:11]]}`

## Checksums

| Name | Covers | Stored | Valid |
|---|---|---|---|
| CC_BASE | Page 00h:0–62 (flat 128–190) | 0x{page00h[63]:02X} | ✓ |
| CC_EXT | Page 00h:64–94 (flat 192–222) | 0x{page00h[95]:02X} | ✓ |
"""


# ---------------------------------------------------------------------------
# Fixture definitions
# ---------------------------------------------------------------------------

# SFF-8472 fixture 1: external calibration 10GBASE-SR SFP+
# external calibration path; identity constants (slope=1.0, offset=0)
FIXTURE_SFF8472_EXTCAL = {
    "stem": "synthetic-extcal-sfp10g-sr",
    "vendor_name": "OPENXCVR      ",    # 14 chars, padded to 16
    "vendor_pn": "SYN-EXTCAL-10GSR",
    "vendor_sn": "SYNTH000001     ",
    "vendor_rev": "A1  ",
    "oui": "00:00:00",
    "connector": 0x07,                  # LC
    "compliance": [0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],  # 10GBASE-SR
    "encoding": 0x06,                   # 64B/66B
    "bit_rate_100m": 0x67,              # 103 = 10.3 Gbps
    "wavelength_nm": 850,
    "link_length": {"om2": 8, "om1": 3, "om3_byte": 0x32},  # 80m OM2, 30m OM1, 100m OM3
    "dmt": 0x58,                        # DDM=1 (bit6), ExtCal=1 (bit4), RxAvgPwr=1 (bit3)
    "ext_cal": True,
    "sff8472_compliance": 0x04,         # implements rev 10.4 diagnostics
    "module_type": "10GBASE-SR SFP+ (external calibration)",
    "monitoring": {"temperature_c": 25.0, "voltage_v": 3.3, "tx_bias_ma": 5.0,
                   "tx_power_mw": 0.5, "rx_power_mw": 0.3},
    "md_notes": "Exercises the **external calibration** branch of calibration variants. "
                "Identity calibration constants (slope=0x0100 in U8.8 format, offset=0x0000) yield "
                "decoded values identical to internal calibration, confirming the formula path is "
                "exercised even though the numerical result is the same. This is the only fixture "
                "in the SFF-8472 corpus where A0h:92 bit 4 (externally_calibrated) is set and "
                "A2h:56-91 carry non-zero calibration constant bytes.",
}

# SFF-8472 fixture 2: passive copper DAC, no DDM
FIXTURE_SFF8472_DAC = {
    "stem": "synthetic-dac-sfp10g-passive-cu",
    "vendor_name": "OPENXCVR      ",
    "vendor_pn": "SYN-DAC-10G-1M  ",
    "vendor_sn": "SYNTH000002     ",
    "vendor_rev": "B1  ",
    "oui": "00:00:00",
    "connector": 0x21,                  # Copper Pigtail
    "compliance": [0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00],  # byte 8 bit 3 = Passive cable
    "encoding": 0x06,                   # 64B/66B
    "bit_rate_100m": 0x67,              # 103 = 10.3 Gbps
    "wavelength_nm": 0,                 # N/A for copper
    "link_length": {"om4_or_copper_1m": 1},  # 1m copper cable
    "dmt": 0x00,                        # No DDM, no calibration
    "ext_cal": False,
    "sff8472_compliance": 0x00,
    "module_type": "10G Passive Copper DAC 1m",
    "monitoring": {},                   # No monitoring (DDM not implemented)
    "monitoring_stale": True,
    "md_notes": "Passive copper direct-attach cable (DAC) — DDM not implemented (A0h:92 = 0x00, "
                "digital_diagnostics_implemented = 0). Exercises the no-DDM decode branch where "
                "monitoring and lane data are absent. Connector type 0x21 (Copper Pigtail) and "
                "compliance byte 8 bit 3 (passive_cable) distinguish this from optical modules. "
                "A2h bytes are all zeros; CC_DMI covers the zero-initialized threshold region.",
}

# SFF-8472 fixture 3: 25G SFP28 — bit-rate overflow path
FIXTURE_SFF8472_SFP28 = {
    "stem": "synthetic-sfp28-25g-sr",
    "vendor_name": "OPENXCVR      ",
    "vendor_pn": "SYN-SFP28-25GSR ",
    "vendor_sn": "SYNTH000003     ",
    "vendor_rev": "C1  ",
    "oui": "00:00:00",
    "connector": 0x07,                  # LC
    "compliance": [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],  # bit7 = extended compliance present
    "ext_spec_compliance": 0x1C,        # 25GBASE-SR (SFF-8024 4.x Table 4-2)
    "encoding": 0x06,                   # 64B/66B
    "bit_rate_100m": 0xFF,              # overflow marker → see byte 66
    "signaling_rate_max": 103,          # 103 × 250 Mbit/s = 25.75 Gbps
    "wavelength_nm": 850,
    "link_length": {"om3_byte": 0x46},  # 70m OM3 at 25G (multiplier=times_10m, base=7 → 70m)
    "dmt": 0x68,                        # DDM=1 (bit6), IntCal=1 (bit5), RxAvgPwr=1 (bit3)
    "ext_cal": False,
    "sff8472_compliance": 0x04,
    "module_type": "25GBASE-SR SFP28",
    "monitoring": {"temperature_c": 30.0, "voltage_v": 3.3, "tx_bias_ma": 5.5,
                   "tx_power_mw": 0.6, "rx_power_mw": 0.4},
    "md_notes": "25G SFP28 optical module. Exercises the **bit-rate overflow** path: A0h:12 = 0xFF "
                "(overflow marker per SFF-8472 v12.5a § 7.3) and A0h:66 = 103 (signaling_rate_max, "
                "103 × 250 Mbit/s = 25.75 Gbps). Extended compliance at A0h:36 = 0x1C identifies "
                "25GBASE-SR (SFF-8024 Table 4-2). CC_EXT covers byte 64-94 including byte 66. "
                "Internal calibration (same path as the Finisar FTLX8571D3BCL corpus fixtures) "
                "at 25G rate — confirms the bit-rate decode formula at A0h:12 = 0xFF.",
}

# SFF-8636 fixture 1: QSFP+ passive copper DAC
FIXTURE_SFF8636_DAC = {
    "stem": "synthetic-qsfp-dac-40g-passive-cu",
    "identifier": 0x0D,                 # QSFP+ SFF-8636
    "vendor_name": "OPENXCVR      ",
    "vendor_pn": "SYN-QSFP-DAC-3M ",
    "vendor_sn": "SYNTH000004     ",
    "vendor_rev": "  ",
    "oui": "00:00:00",
    "connector": 0x21,                  # Copper Pigtail
    "compliance": [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    "encoding": 0x06,                   # 64B/66B
    "bit_rate_100m": 0x67,              # 103 = 10.3 Gbps (per lane)
    "link_length": {"om4_or_copper_m": 3},  # 3m copper DAC
    "device_tech": 0xA,                 # 0xA = copper_unequalized
    "wavelength_raw": 0,
    "temperature_c": 0.0,
    "voltage_v": 0.0,
    "rx_power_mw": [0.0, 0.0, 0.0, 0.0],
    "tx_bias_ma": [0.0, 0.0, 0.0, 0.0],
    "tx_power_mw": [0.0, 0.0, 0.0, 0.0],
    "monitoring_stale": True,
    "active_lanes": 4,
    "module_type": "40G Passive Copper DAC 3m QSFP+",
    "md_notes": "Passive copper QSFP+ DAC — no optical monitoring. Exercises the copper-cable "
                "decode branch: device_technology = 0xA (copper_unequalized, bits 4-7 of page "
                "00h:19 = flat 147). All per-lane and module monitoring is zero (monitoring_stale=true). "
                "Connector type 0x21 (Copper Pigtail). 3m copper DAC length in page00h:18 (flat 146).",
}

# SFF-8636 fixture 2: QSFP28 BiDi 100G
FIXTURE_SFF8636_BIDI = {
    "stem": "synthetic-qsfp28-bidi-100g-cwdm4",
    "identifier": 0x11,                 # QSFP28
    "vendor_name": "OPENXCVR      ",
    "vendor_pn": "SYN-QSFP28-BIDI ",
    "vendor_sn": "SYNTH000005     ",
    "vendor_rev": "  ",
    "oui": "00:00:00",
    "connector": 0x07,                  # LC
    "compliance": [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],  # extended compliance set
    "ext_spec_compliance": 0x18,        # 100G-CWDM4 (SFF-8024 Table 4-2)
    "encoding": 0x06,                   # 64B/66B
    "bit_rate_100m": 0x67,              # 103 = 10.3 Gbps (per lane)
    "link_length": {"smf_km": 2},       # 2km SMF
    "device_tech": 0x4,                 # 0x4 = nm1550_dfb (CWDM4 uses 4 wavelengths)
    "wavelength_raw": 0x4268,           # 17000 = 850nm equivalent field; BiDi has dual WL
    "ext_id_power": 0x00,
    "temperature_c": 32.0,
    "voltage_v": 3.3,
    "rx_power_mw": [0.5, 0.5, 0.5, 0.5],
    "tx_bias_ma": [30.0, 30.0, 30.0, 30.0],
    "tx_power_mw": [0.4, 0.4, 0.4, 0.4],
    "monitoring_stale": False,
    "active_lanes": 4,
    "module_type": "100G CWDM4 QSFP28 (BiDi-style 2km SMF)",
    "md_notes": "QSFP28 CWDM4 100G module representing a BiDi-architecture optical transceiver. "
                "Extended compliance byte 0x18 (100G-CWDM4 per SFF-8024 Table 4-2) and extended "
                "identifier dispatch (compliance byte 3 bit 7). Device technology 0x4 (nm1550_dfb) "
                "distinguishes from the 850nm NRZ VCSEL corpus. 4-lane monitoring at 25Gbps/lane "
                "with realistic bias current for EML transmitters (~30mA).",
}

# SFF-8636 fixture 3: QSFP28 AOC 100GBASE-SR4
FIXTURE_SFF8636_AOC = {
    "stem": "synthetic-qsfp28-aoc-100g-sr4",
    "identifier": 0x11,                 # QSFP28
    "vendor_name": "OPENXCVR      ",
    "vendor_pn": "SYN-QSFP28-AOC5 ",
    "vendor_sn": "SYNTH000006     ",
    "vendor_rev": "  ",
    "oui": "00:00:00",
    "connector": 0x01,                  # Subsystem/vendor-specific active cable
    "compliance": [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],  # no single standard bit
    "ext_spec_compliance": 0x00,
    "encoding": 0x06,                   # 64B/66B
    "bit_rate_100m": 0x67,              # 103 = 10.3 Gbps per lane
    "link_length": {"cable_or_om4": 5},  # 5m AOC
    "device_tech": 0x0,                 # nm850_vcsel
    "wavelength_raw": 0x4268,           # 850nm
    "ext_id_power": 0x00,
    "temperature_c": 35.0,
    "voltage_v": 3.3,
    "rx_power_mw": [0.6, 0.6, 0.6, 0.6],
    "tx_bias_ma": [6.0, 6.0, 6.0, 6.0],
    "tx_power_mw": [0.6, 0.6, 0.6, 0.6],
    "monitoring_stale": False,
    "active_lanes": 4,
    "module_type": "100GBASE-SR4 QSFP28 AOC 5m",
    "md_notes": "Active Optical Cable (AOC) QSFP28 100GBASE-SR4, 5m. Exercises the AOC connector "
                "path: connector type 0x01 (subsystem connector) and length in page00h:18 (cable_or_om4, "
                "flat 146) = 5 (5m). AOC distinguishes from optical transceiver by connector type; "
                "the monitoring data exercises the full 4-lane telemetry path at 850nm VCSEL parameters "
                "typical of active cable optics (low bias ~6mA vs EML ~30mA in BiDi fixture).",
}


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def write_file(path: pathlib.Path, data: bytes, dry_run: bool) -> None:
    if dry_run:
        print(f"  [dry-run] would write {path} ({len(data)} bytes)")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    print(f"  wrote {path} ({len(data)} bytes)")


def generate_sff8472(fix: dict, dry_run: bool) -> None:
    stem = fix["stem"]
    print(f"\n=== SFF-8472: {stem} ===")
    a0h = build_sff8472_a0h(fix)
    a2h = build_sff8472_a2h(fix)
    golden = make_sff8472_golden(stem, fix, a0h, a2h)
    md = make_sff8472_md(stem, fix, a0h)

    write_file(FIXTURES / "sff-8472" / f"{stem}.a0h.bin", bytes(a0h), dry_run)
    write_file(FIXTURES / "sff-8472" / f"{stem}.a2h.bin", bytes(a2h), dry_run)
    write_file(FIXTURES / "sff-8472" / f"{stem}.md", md.encode(), dry_run)
    write_file(GOLDEN / "sff-8472" / f"{stem}.json",
               json.dumps(golden, indent=2, sort_keys=True).encode() + b"\n", dry_run)


def generate_sff8636(fix: dict, dry_run: bool) -> None:
    stem = fix["stem"]
    print(f"\n=== SFF-8636: {stem} ===")
    lower = build_sff8636_lower(fix)
    page00h = build_sff8636_page00h(fix)
    page01h = bytearray(128)             # deprecated stub, all zeros
    page02h = bytearray(128)             # user EEPROM, all zeros
    page03h = bytearray(128)             # thresholds, all zeros

    golden = make_sff8636_golden(stem, fix, lower, page00h)
    md = make_sff8636_md(stem, fix, lower, page00h)

    write_file(FIXTURES / "sff-8636" / f"{stem}.lower.bin", bytes(lower), dry_run)
    write_file(FIXTURES / "sff-8636" / f"{stem}.page-00h.bin", bytes(page00h), dry_run)
    write_file(FIXTURES / "sff-8636" / f"{stem}.page-01h.bin", bytes(page01h), dry_run)
    write_file(FIXTURES / "sff-8636" / f"{stem}.page-02h.bin", bytes(page02h), dry_run)
    write_file(FIXTURES / "sff-8636" / f"{stem}.page-03h.bin", bytes(page03h), dry_run)
    write_file(FIXTURES / "sff-8636" / f"{stem}.md", md.encode(), dry_run)
    write_file(GOLDEN / "sff-8636" / f"{stem}.json",
               json.dumps(golden, indent=2, sort_keys=True).encode() + b"\n", dry_run)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate synthetic openxcvr test fixtures")
    parser.add_argument("--dry-run", action="store_true", help="Print what would be written without writing")
    args = parser.parse_args()

    generate_sff8472(FIXTURE_SFF8472_EXTCAL, args.dry_run)
    generate_sff8472(FIXTURE_SFF8472_DAC, args.dry_run)
    generate_sff8472(FIXTURE_SFF8472_SFP28, args.dry_run)
    generate_sff8636(FIXTURE_SFF8636_DAC, args.dry_run)
    generate_sff8636(FIXTURE_SFF8636_BIDI, args.dry_run)
    generate_sff8636(FIXTURE_SFF8636_AOC, args.dry_run)

    print("\nDone.")


if __name__ == "__main__":
    main()
