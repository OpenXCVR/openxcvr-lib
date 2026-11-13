#!/usr/bin/env python3
"""Emit the synthetic-cmis-5-4-qsfp-dd-vdm fixture .bin files and parity golden.

This script is the durable source of truth for the synthetic CMIS 5.4 VDM
fixture under tests/fixtures/cmis/, following the
tools/build-cmis-5-2-synthetic.py precedent: every byte is documented inline
with a primary-source citation (CMIS 5.4 § 8.27 / Table 8-177..8-186) so the
corpus remains auditable, and the golden pins the expected VDM decode
semantics derived from the authored bytes plus the spec tables — never by
running a decoder and capturing its output.

The fixture exists to close the VDM fixture gap demonstrated by the coverage
analysis: no `page-28h`-`page-2bh` (supervision threshold quads) or
`page-2dh` (supervision mask) slices existed anywhere in the corpus, and no
golden asserted any VDM value. This fixture carries the complete VDM page
set of a four-group module:

  - Page 2Fh advertisement: VDMSupport = 3 (groups 1-4), DutyCycleSupport
    and PowerSavingSupport advertised, MonitoringDutyCycle k = 5.
  - Pages 20h-23h descriptors: four-group coverage with U16 / S16 / F16
    observable carriers, exercising the polymorphic sample
    dispatch (page-24h-vdm-samples variant table).
  - Pages 24h-27h samples with hand-computed raw values.
  - Pages 28h-2Bh threshold quads (sets 1, 17, 33, 49) and the 2Ch/2Dh
    flag/mask nibble packing across group boundaries.

CMIS 5.4 source: `docs/stds/cmis/OIF-CMIS-05.4.pdf` (OIF, 2025).

Run:
    python3 tools/build-cmis-5-4-synthetic.py
"""

from __future__ import annotations

import json
import pathlib
import struct

FIXTURE_DIR = pathlib.Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "cmis"
STEM = "synthetic-cmis-5-4-qsfp-dd-vdm"
GOLDEN_DIR = pathlib.Path(__file__).resolve().parent.parent / "tests" / "golden" / "cmis"
GOLDEN_PATH = GOLDEN_DIR / f"{STEM}.json"

AUTHORED_AT = "2026-11-11"
AUTHORED_BY = "ras + engineer via tools/build-cmis-5-4-synthetic.py"


def pad_ascii(s: str, width: int) -> bytes:
    b = s.encode("ascii")
    if len(b) > width:
        b = b[:width]
    return b + b" " * (width - len(b))


def f16(m: int, s: int) -> bytes:
    """CMIS F16: 11-bit mantissa m and 5-bit exponent s, value m*10^(s-24)."""
    assert 0 <= m <= 0x7FF and 0 <= s <= 0x1F
    return bytes([(s << 3) | (m >> 8), m & 0xFF])


def f16_value(m: int, s: int) -> float:
    return m * (10.0 ** (s - 24))


def lower_memory() -> bytes:
    """Lower memory (128 bytes). CMIS 5.4 § 8.2 / Table 8-5.

      - byte 0: SFF8024Identifier 0x18 (QSFP-DD, SFF-8024 § 4.1).
      - byte 1: CmisRevision 0x54 (CMIS 5.4).
      - byte 2: paged memory model, MCI 400 kHz, intervention-free.
    All other bytes zero; the golden decodes them honestly (ModuleState
    000b = Reserved, monitors 0.0, flags clear), exactly like the 5.2
    synthetic fixture's lower memory.
    """
    buf = bytearray(128)
    buf[0] = 0x18
    buf[1] = 0x54
    buf[2] = 0x00
    return bytes(buf)


def page_00h() -> bytes:
    """Page 00h (Administrative). CMIS 5.4 § 8.3 / Tables 8-27..8-30.

    Synthetic vendor identity strings (space-padded ASCII) so the golden's
    identity block derives from authored bytes, plus the PageChecksum at
    page-relative byte 94 (low 8 bits of the arithmetic sum of bytes 0-93).
    Connector (page-relative 75) stays zero = unknown_or_unspecified.
    """
    buf = bytearray(128)
    buf[0] = 0x18  # SFF8024IdentifierCopy
    buf[1:17] = pad_ascii("CMIS_5_4_VDM_SYNTH", 16)
    buf[20:36] = pad_ascii("FIXTURE-SYNTH-V1", 16)
    buf[36:38] = pad_ascii("00", 2)
    buf[38:54] = pad_ascii("SYNTHETIC0000002", 16)
    buf[54:62] = pad_ascii("25111100", 8)
    buf[62:72] = pad_ascii("", 10)  # CLEICode, blank
    buf[94] = sum(buf[0:94]) & 0xFF
    return bytes(buf)


def page_02h() -> bytes:
    """Page 02h (Module and Lane Supervision Thresholds). CMIS 5.4 § 8.5.

    All zeros = no thresholds defined; not decoded by the golden.
    """
    return bytes(128)


def page_10h() -> bytes:
    """Banked Page 10h (Lane Control and Data Path Control). CMIS 5.4 § 8.13.

    All zeros = every control field at its default.
    """
    return bytes(128)


def page_11h() -> bytes:
    """Banked Page 11h (Lane Status and Data Path Status). CMIS 5.4 § 8.14.

    All zeros = every host lane's DPStateHostLane nibble is 0h = Reserved
    (Table 8-91 code set), which the golden renders as `Reserved0`, exactly
    like the CMIS 5.2 synthetic fixture.
    """
    return bytes(128)


def page_01h() -> bytes:
    """Page 01h (Advertising). CMIS 5.4 § 8.4 / Tables 8-46..8-62.

    Byte 142 (page-relative 14): bit 6 VDMPagesSupported = 1 (Pages 20h-2Fh
    supported, advertisement details in Page 2Fh, Table 8-47) and bits 1-0
    BanksSupported = 00b (bank 0, 8 lanes). Everything else zero.
    """
    buf = bytearray(128)
    buf[14] = 0x40
    return bytes(buf)


# ---------------------------------------------------------------------------
# VDM pages. CMIS 5.4 § 8.27, Tables 8-177..8-186 (pre-5.4 § 8.22).
# All pages are upper-memory slices: page-relative offsets 0..127 map to
# module bytes 128..255.
# ---------------------------------------------------------------------------

def vdm_support_byte() -> int:
    """2Fh:128. Bit 3 DutyCycleSupport, bit 2 PowerSavingSupport, bits 1-0
    VDMSupport (Table 8-186). 0b1111 = duty cycle + power saving supported,
    all four VDM groups."""
    return (1 << 3) | (1 << 2) | 0x3


def page_2fh() -> bytes:
    """Page 2Fh (VDM Advertisement and Dynamic Controls). Table 8-186.

      - 128: 0b1111 (see vdm_support_byte).
      - 129-130: FineIntervalLength = 100 (10.0 ms in 0.1 ms units).
      - 144: MonitoringDutyCycle k = 5 (bits 5-2), FreezeRequest /
             PowerSavingMode clear, bits 1-0 reserved.
      - 145: FreezeDone/UnfreezeDone clear (both handshake bits start 0).
    """
    buf = bytearray(128)
    buf[0] = vdm_support_byte()
    struct.pack_into(">H", buf, 1, 100)
    buf[16] = 5 << 2
    return bytes(buf)


def _descriptor(local_index: int, threshold_set_id: int, monitored_resource: int,
                observable_type: int) -> tuple[int, int]:
    """One 2-byte VDM instance descriptor (Table 8-179): even byte carries
    ThresholdSetIndex bits 7-4 and MonitoredObjectLocation bits 3-0, odd
    byte the observable type. `local_index` is 0-based (instance ID - 1)."""
    assert 0 <= local_index <= 63 and 0 <= threshold_set_id <= 15
    assert 0 <= monitored_resource <= 15
    even = (threshold_set_id << 4) | monitored_resource
    return (local_index * 2, even, local_index * 2 + 1, observable_type)


# (group page, local index 0-based, threshold set, monitored resource, type)
DESCRIPTORS = [
    (0x20, 0, 0, 0x0, 0x01),  # inst 1:  laser age, lane 1, U16
    (0x20, 1, 1, 0x1, 0x04),  # inst 2:  laser temperature deviation, lane 2, S16
    (0x20, 2, 2, 0x0, 0x09),  # inst 3:  pre-FEC BER min, media input, F16
    (0x20, 3, 3, 0xF, 0x4D),  # inst 4:  Vcc2p6 voltage monitor, module, U16
    (0x21, 0, 0, 0x1, 0x1A),  # inst 65: FERC total accumulated, host input, F16
    (0x22, 0, 0, 0x2, 0x54),  # inst 129: ELS input power, ELS/laser 3, S16
    (0x23, 62, 0, 0x3, 0x1B), # inst 255: SEWmax minimum sample, media input, U16
]

# (group page, local index 0-based, raw sample bytes)
SAMPLES = [
    (0x20, 0, struct.pack(">H", 100)),       # laser age 100%
    (0x20, 1, struct.pack(">h", 256)),       # +1.0 degC deviation (1/256 degC LSB)
    (0x20, 2, f16(250, 19)),                 # pre-FEC BER min 2.5e-3
    (0x20, 3, struct.pack(">H", 2600)),      # Vcc2p6 = 0.26 V in 100 uV units
    (0x21, 0, f16(1, 24)),                   # FERC total accumulated = 1.0
    (0x22, 0, struct.pack(">h", -50)),       # ELS input power -0.50 dBm
    (0x23, 62, struct.pack(">H", 7)),         # SEWmax min = 7
]

# Threshold quads (Tables 8-182): (page, set 0-based, hi_alarm, lo_alarm,
# hi_warn, lo_warn) - raw X16 carriers in the supervised value's storage
# type (5.4: same storage type as the supervised value; the golden pins the
# raw carriers).
THRESHOLD_SETS = [
    (0x28, 0, 0x1000, 0x0800, 0x0C00, 0x0900),
    (0x29, 0, 0x1111, 0x2222, 0x3333, 0x4444),
    (0x2A, 0, 0x00FF, 0x00AA, 0x00EE, 0x00BB),
    (0x2B, 0, 0x7FFF, 0x8000, 0x6000, 0xA000),
]

# TC flags (Table 8-183/8-184): (instance, high_alarm, low_alarm,
# high_warning, low_warning)
FLAGS = [
    (1, True, False, False, True),
    (2, True, False, False, False),
    (129, False, False, True, False),
    (255, False, True, False, False),
]

# TC masks (Table 8-185): same nibble layout, default all-masked; instance 2
# and 129 leave the alarm masks clear (interrupts enabled).
MASKS = [
    (1, True, True, True, True),
    (2, False, False, True, True),
    (129, False, True, True, True),
    (255, True, True, True, True),
]


def page_descriptors(group_page: int) -> bytes:
    buf = bytearray(128)
    for page, local, tsid, res, otype in DESCRIPTORS:
        if page != group_page:
            continue
        off, even, odd_off, odd = _descriptor(local, tsid, res, otype)
        buf[off] = even
        buf[odd_off] = odd
    return bytes(buf)


def page_samples(group_page: int) -> bytes:
    buf = bytearray(128)
    for page, local, raw in SAMPLES:
        if page != group_page:
            continue
        off = local * 2
        buf[off:off + 2] = raw
    return bytes(buf)


def page_thresholds(page: int) -> bytes:
    buf = bytearray(128)
    for tpage, set_idx, hi_a, lo_a, hi_w, lo_w in THRESHOLD_SETS:
        if tpage != page:
            continue
        base = set_idx * 8
        struct.pack_into(">HHHH", buf, base, hi_a, lo_a, hi_w, lo_w)
    return bytes(buf)


def _pack_nibbles(entries) -> bytes:
    """2Ch/2Dh nibble packing (Tables 8-184/8-185): instance i occupies
    nibble ((i-1) % 2) * 4 .. +3 of byte (i-1) // 2; bit 0 (or 4) is the
    high-alarm bit, 1 low alarm, 2 high warning, 3 low warning."""
    buf = bytearray(128)
    for inst, hi_a, lo_a, hi_w, lo_w in entries:
        byte = (inst - 1) // 2
        shift = ((inst - 1) % 2) * 4
        nibble = (hi_a << 0) | (lo_a << 1) | (hi_w << 2) | (lo_w << 3)
        buf[byte] |= nibble << shift
    return bytes(buf)


def page_2ch() -> bytes:
    return _pack_nibbles(FLAGS)


def page_2dh() -> bytes:
    return _pack_nibbles(MASKS)


# ---------------------------------------------------------------------------
# Golden - expected semantics derived from the authored bytes.
# ---------------------------------------------------------------------------

OBS_TYPE_STORAGE = { # variant table (page-24h-vdm-samples-1-64)
    0x01: "u16", 0x04: "i16", 0x09: "f16", 0x4D: "u16",
    0x1A: "f16", 0x54: "i16", 0x1B: "u16",
}

GROUP_BY_PAGE = {0x20: 0, 0x21: 1, 0x22: 2, 0x23: 3}


def _decoded_sample(otype: int, raw: bytes):
    storage = OBS_TYPE_STORAGE[otype]
    if storage == "u16":
        return {"storage": "u16", "value": struct.unpack(">H", raw)[0]}
    if storage == "i16":
        return {"storage": "i16", "value": struct.unpack(">h", raw)[0]}
    m = ((raw[0] & 0x07) << 8) | raw[1]
    s = raw[0] >> 3
    return {"storage": "f16", "value": f16_value(m, s)}


def build_golden() -> dict:
    vdm_observables = []
    for page, local, tsid, res, otype in DESCRIPTORS:
        sample = next(s for p, l, s in SAMPLES if p == page and l == local)
        inst = GROUP_BY_PAGE[page] * 64 + local + 1
        vdm_observables.append({
            "instance": inst,
            "group": GROUP_BY_PAGE[page] + 1,
            "threshold_set_id": tsid,
            "global_threshold_set_id": (GROUP_BY_PAGE[page] * 16) + tsid + 1,
            "monitored_resource": res,
            "observable_type": otype,
            "active": True,
            "sample_raw": f"0x{struct.unpack('>H', sample)[0]:04X}",
            "sample": _decoded_sample(otype, sample),
        })

    vdm_thresholds = []
    for tpage, set_idx, hi_a, lo_a, hi_w, lo_w in THRESHOLD_SETS:
        vdm_thresholds.append({
            "page": f"{tpage:02X}h",
            "set": set_idx + 1,
            "high_alarm": f"0x{hi_a:04X}",
            "low_alarm": f"0x{lo_a:04X}",
            "high_warning": f"0x{hi_w:04X}",
            "low_warning": f"0x{lo_w:04X}",
        })

    def nibble_entries(entries):
        out = []
        for inst, hi_a, lo_a, hi_w, lo_w in entries:
            out.append({
                "instance": inst,
                "high_alarm": hi_a,
                "low_alarm": lo_a,
                "high_warning": hi_w,
                "low_warning": lo_w,
            })
        return out

    p00 = page_00h()

    golden = {
        "_metadata": {
            "schema_version": "openxcvr.golden.v1",
            "fixture": STEM,
            "cmis_version": "5.4",
            "spec": "cmis-5.4",
            "spec_revision": "5.4",
            "synthetic": True,
            "monitoring_stale": True,
            "authored_at": AUTHORED_AT,
            "authored_by": AUTHORED_BY,
            "synthetic_note": (
                "Synthetic VDM parity golden, NOT a hardware oracle: every byte "
                "is hand-authored by tools/build-cmis-5-4-synthetic.py and the "
                "expected semantics derive from the CMIS 5.4 spec "
                "(OIF-CMIS-05.4.pdf, section 8.27, Tables 8-177..8-186) applied "
                "to those bytes, not from a decoder run. The `vdm` block is a "
                "fixture-scoped extension: the CmisDecodeResult OpenAPI schema "
                "admits additional properties, and no generated decoder output "
                "is compared against this block today - it pins the authored "
                "bytes' VDM semantics (descriptor layout, sample "
                "dispatch, threshold quad layout, TC flag/mask nibble packing, "
                "2Fh controls) for fixture audit and future harness use. The "
                "zeroed non-VDM pages carry no field-content semantics; the "
                "standard blocks decode them honestly (ModuleState Reserved0, "
                "0.0 monitors, no advertised Applications)."
            ),
        },
        "decode_status": "ok",
        "identity": {
            # CMIS 5.4 Page 00h bytes 129-144 (page-relative 1..16): a
            # 16-byte ASCII field, so the golden derives from the authored
            # bytes with the same trailing-space trim every decoder applies —
            # an authored literal longer than 16 bytes is field-width
            # truncated, never pinned untruncated.
            "vendor_name": p00[1:17].decode("ascii").rstrip(" "),
            "vendor_oui": "00:00:00",
            "vendor_pn": "FIXTURE-SYNTH-V1",
            "vendor_rev": "00",
            "vendor_sn": "SYNTHETIC0000002",
            "date_code": "251111",
        },
        "module": {
            "cmis_revision": "5.4",
            "identifier": {"name": "qsfp_dd", "raw": "0x18"},
            "connector": {"name": "unknown_or_unspecified", "raw": "0x00"},
            "memory_model": "paged",
            "module_state": "Reserved0",
            "module_type": None,
        },
        "monitoring": {"module_temperature_c": 0.0, "module_voltage_v": 0.0},
        "applications": None,
        "checksums": None,
        "lanes": [
            # rx_los / tx_fault: schema fields los_flag_rx (Table 8-88) and
            # failure_flag_tx (Table 8-87) read False on this all-zero
            # Page 11h (latched flags; § 8.10.3 default state is 0b).
            {"index": i, "dp_state": "Reserved0", "tx_power_mw": 0.0,
             "rx_power_mw": 0.0, "tx_bias_ma": 0.0, "rx_los": False,
             "tx_fault": False}
            for i in range(1, 9)
        ],
        "flags": {
            "interrupt_asserted": True,
            "data_not_ready": None,
            "temp_high_alarm": False,
            "temp_low_alarm": False,
            "supply_high_alarm": False,
            "supply_low_alarm": False,
            "lanes": [
                {k: False for k in (
                    "rx_power_high_alarm", "rx_power_high_warning",
                    "rx_power_low_alarm", "rx_power_low_warning",
                    "tx_bias_high_alarm", "tx_bias_high_warning",
                    "tx_bias_low_alarm", "tx_bias_low_warning",
                    "tx_power_high_alarm", "tx_power_high_warning",
                    "tx_power_low_alarm", "tx_power_low_warning")}
                for _ in range(8)
            ],
        },
        "vdm": {
            "support": {
                "vdm_support": 3,
                "vdm_support_name": "groups_1_4",
                "groups_advertised": 4,
                "duty_cycle_support": True,
                "power_saving_support": True,
                "fine_interval_length_0p1ms": 100,
                "freeze_request": False,
                "power_saving_mode": False,
                "monitoring_duty_cycle_k": 5,
                "freeze_done": False,
                "unfreeze_done": False,
            },
            "observables": vdm_observables,
            "threshold_quads": vdm_thresholds,
            "tc_flags": nibble_entries(FLAGS),
            "tc_masks": nibble_entries(MASKS),
        },
    }
    return golden


def main() -> int:
    pages = {
        "page-00h": page_00h(),
        "page-01h": page_01h(),
        "page-02h": page_02h(),
        "page-10h": page_10h(),
        "page-11h": page_11h(),
        "page-20h": page_descriptors(0x20),
        "page-21h": page_descriptors(0x21),
        "page-22h": page_descriptors(0x22),
        "page-23h": page_descriptors(0x23),
        "page-24h": page_samples(0x20),
        "page-25h": page_samples(0x21),
        "page-26h": page_samples(0x22),
        "page-27h": page_samples(0x23),
        "page-28h": page_thresholds(0x28),
        "page-29h": page_thresholds(0x29),
        "page-2ah": page_thresholds(0x2A),
        "page-2bh": page_thresholds(0x2B),
        "page-2ch": page_2ch(),
        "page-2dh": page_2dh(),
        "page-2fh": page_2fh(),
    }
    for name, data in pages.items():
        assert len(data) == 128, (name, len(data))
        (FIXTURE_DIR / f"{STEM}.{name}.bin").write_bytes(data)
    (FIXTURE_DIR / f"{STEM}.lower.bin").write_bytes(lower_memory())

    GOLDEN_PATH.write_text(json.dumps(build_golden(), indent=1) + "\n")
    print(f"wrote {len(pages) + 1} fixture slices and {GOLDEN_PATH}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
