"""Per-lane signal flags and threshold evaluation for the SFF facades.

Mirrors the Go facade tests and the Rust surface's own tests: the unified
LaneStatus vocabulary (openxcvr-core ``types::LaneStatus``) for the
per-lane signal flags, and the ``evaluate_thresholds`` pure decisions
against the checked-in fixtures. Expected values are an independent oracle
transcribed from the specs' bit tables and the fixture bytes, not from
generator output.
"""

from __future__ import annotations

import struct
from pathlib import Path

import pytest

from openxcvr import (
    decode_sff8472_lane_signal_flags,
    decode_sff8636_lane_signal_flags,
    evaluate_sff8472_thresholds,
    evaluate_sff8636_thresholds,
)

FIXTURES = Path(__file__).parents[2] / ".." / "tests" / "fixtures"
FIXTURES = FIXTURES.resolve()


def _read(path) -> bytes:
    with open(path, "rb") as f:
        return f.read()


def _within_bounds(evaluation: dict) -> bool:
    return not any(evaluation[k] for k in ("high_alarm", "low_alarm", "high_warning", "low_warning"))


# --- SFF-8636 lane signal flags -------------------------------------------


def test_sff8636_lane_signal_flags_fixture():
    lower = _read(f"{FIXTURES}/sff-8636/finisar-ftlc9551repm-100g-sr4.lower.bin")
    lanes = decode_sff8636_lane_signal_flags({"lower": lower})
    assert len(lanes) == 4
    bit = lambda b, n: bool((b >> n) & 1)  # noqa: E731
    for i, lane in enumerate(lanes):
        assert lane["index"] == i + 1
        assert lane["tx_los"] == bit(lower[3], 4 + i)  # Table 6-5, byte 3 hi nibble
        assert lane["rx_los"] == bit(lower[3], i)  # byte 3 lo nibble
        assert lane["tx_adaptive_eq_fault"] == bit(lower[4], 4 + i)  # byte 4 hi nibble
        assert lane["tx_fault"] == bit(lower[4], i)  # byte 4 lo nibble
        assert lane["tx_cdr_lol"] == bit(lower[5], 4 + i)  # byte 5 hi nibble
        assert lane["rx_cdr_lol"] == bit(lower[5], i)  # byte 5 lo nibble
        assert lane["tx_disable"] == bit(lower[86], i)  # byte 86
    # The fixture latches every LOS and CDR LOL bit and no fault bits.
    assert lanes[0]["tx_los"] is True and lanes[0]["rx_los"] is True
    assert lanes[0]["tx_adaptive_eq_fault"] is False


@pytest.mark.parametrize(
    ("rev", "modeled"),
    [
        (0x03, False),  # rev 1.3
        (0x05, False),  # rev 1.5
        (0x06, True),  # rev 2.0 — schema `since: "2.0"`
        (0x0A, True),  # rev 2.12
        (0x00, True),  # unadmitted byte → union view (Sff8636Module::new)
        (0x7F, True),
    ],
)
def test_sff8636_adaptive_eq_revision_window(rev: int, modeled: bool):
    lower = bytearray(128)
    lower[1] = rev
    lower[4] = 0xF0  # every Tx adaptive-EQ bit set
    lanes = decode_sff8636_lane_signal_flags({"lower": bytes(lower)})
    assert ("tx_adaptive_eq_fault" in lanes[0]) is modeled
    if modeled:
        assert lanes[0]["tx_adaptive_eq_fault"] is True


# --- SFF-8472 lane signal flags -------------------------------------------


def test_sff8472_lane_signal_flags_fixture():
    a0h = _read(f"{FIXTURES}/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin")
    a2h = _read(f"{FIXTURES}/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin")
    lanes = decode_sff8472_lane_signal_flags({"a0h": a0h, "a2h": a2h})
    assert len(lanes) == 1
    lane = lanes[0]
    assert lane["index"] == 1
    assert lane["rx_los"] == bool((a2h[110] >> 1) & 1)  # Table 9-16 b110.1
    assert lane["tx_fault"] == bool((a2h[110] >> 2) & 1)  # b110.2
    assert lane["tx_disable"] == bool((a2h[110] >> 7) & 1)  # b110.7
    assert a0h[94] == 0x03  # rev 10.2 — pre-12.5
    assert "tx_adaptive_eq_fault" not in lane
    # SFF-8472 defines no Tx LOS / CDR LOL flags.
    for key in ("tx_los", "tx_cdr_lol", "rx_cdr_lol"):
        assert key not in lane


@pytest.mark.parametrize(
    ("rev", "modeled"),
    [
        (0x01, False),  # rev 9.3
        (0x09, False),  # rev 12.3
        (0x0A, True),  # rev 12.5 — schema `since: "12.5"`
        (0x00, True),  # unadmitted byte → union view (Sff8472Module::new)
    ],
)
def test_sff8472_adaptive_eq_revision_window(rev: int, modeled: bool):
    a0h = bytearray(256)
    a0h[94] = rev
    a2h = bytearray(256)
    a2h[118] = 0x10  # A2h:118.4 Adaptive Input EQ Fail
    lanes = decode_sff8472_lane_signal_flags({"a0h": bytes(a0h), "a2h": bytes(a2h)})
    assert ("tx_adaptive_eq_fault" in lanes[0]) is modeled
    if modeled:
        assert lanes[0]["tx_adaptive_eq_fault"] is True


# --- SFF-8472 threshold evaluation ----------------------------------------


def test_evaluate_sff8472_thresholds_finisar():
    stem = "finisar-ftlx8571d3bcl-mup0wb0"
    result = evaluate_sff8472_thresholds(
        {"a0h": _read(f"{FIXTURES}/sff-8472/{stem}.a0h.bin"), "a2h": _read(f"{FIXTURES}/sff-8472/{stem}.a2h.bin")}
    )
    # temp 10.10 °C vs 78/-13/73/-8, vcc 3.3162 vs 3.7/2.9/3.6/3.0, bias
    # 7.176 vs 13.2/4.0/12.6/5.0, tx 0.5846 vs 1.0/0.2512/0.7943/0.3162.
    for name in ("temperature", "supply_voltage", "tx_bias", "tx_power"):
        assert _within_bounds(result[name]), name
    # rx 0.0 mW vs 1.0/0.01/0.7943/0.0158: genuine low alarm and warning.
    assert result["rx_power"]["low_alarm"] is True
    assert result["rx_power"]["low_warning"] is True
    assert result["rx_power"]["high_alarm"] is False
    assert result["rx_power"]["high_warning"] is False


def test_evaluate_sff8472_thresholds_zero_sets_stay_within_bounds():
    for stem in ("synthetic-extcal-sfp10g-sr", "synthetic-sfp28-25g-sr"):
        result = evaluate_sff8472_thresholds(
            {
                "a0h": _read(f"{FIXTURES}/sff-8472/{stem}.a0h.bin"),
                "a2h": _read(f"{FIXTURES}/sff-8472/{stem}.a2h.bin"),
            }
        )
        for name in ("temperature", "supply_voltage", "tx_bias", "tx_power", "rx_power"):
            assert _within_bounds(result[name]), (stem, name)


def test_evaluate_sff8472_thresholds_applies_external_calibration():
    a0h = bytearray(256)
    a0h[92] = 0x10  # externally calibrated
    a2h = bytearray(256)
    for i in range(4):  # linear terms: slope 512 (×2), offset 0
        struct.pack_into(">H", a2h, 76 + 4 * i, 512)
        struct.pack_into(">h", a2h, 78 + 4 * i, 0)
    struct.pack_into(">h", a2h, 96, 8192)  # temp raw: uncal 32 °C, cal 64 °C
    struct.pack_into(">h", a2h, 0, 10240)  # high alarm 40 °C
    result = evaluate_sff8472_thresholds({"a0h": bytes(a0h), "a2h": bytes(a2h)})
    assert result["temperature"]["high_alarm"] is True


def test_evaluate_sff8472_thresholds_refuses_uncalibrated_module():
    with pytest.raises(ValueError, match="calibration"):
        evaluate_sff8472_thresholds({"a0h": bytes(256), "a2h": bytes(256)})


# --- SFF-8636 threshold evaluation ----------------------------------------


def _sff8636_pages(stem: str) -> dict[str, bytes]:
    return {
        "lower": _read(f"{FIXTURES}/sff-8636/{stem}.lower.bin"),
        "00h": _read(f"{FIXTURES}/sff-8636/{stem}.page-00h.bin"),
        "03h": _read(f"{FIXTURES}/sff-8636/{stem}.page-03h.bin"),
    }


def test_evaluate_sff8636_thresholds_rev_detected_fixture():
    # Rev 2.5 (Lower:1 = 0x07): temp 19.14 °C and vcc 3.2861 V within
    # bounds; every lane monitor (0.0001 mW / 0 mA) below its low bounds.
    result = evaluate_sff8636_thresholds(_sff8636_pages("finisar-ftlc9551repm-100g-sr4"))
    assert _within_bounds(result["temperature"])
    assert _within_bounds(result["supply_voltage"])
    for group in ("rx_power", "tx_bias", "tx_power"):
        for lane in result[group]:
            assert lane["low_alarm"] is True
            assert lane["low_warning"] is True
            assert lane["high_alarm"] is False
            assert lane["high_warning"] is False


def test_evaluate_sff8636_thresholds_union_path_fixture():
    # Union view (Lower:1 = 0x00): lane 2 tx power 0.9152 mW crosses the
    # 0.7943 mW high-warning bound only; everything else within bounds.
    result = evaluate_sff8636_thresholds(_sff8636_pages("finisar-ftl410qe3c-40g-sr4"))
    for i, lane in enumerate(result["tx_power"]):
        if i == 1:
            assert lane["high_warning"] is True
            assert lane["high_alarm"] is False
            assert lane["low_alarm"] is False
        else:
            assert _within_bounds(lane), i
    for group in ("rx_power", "tx_bias"):
        for lane in result[group]:
            assert _within_bounds(lane)


def test_evaluate_sff8636_thresholds_zero_sets_stay_within_bounds():
    result = evaluate_sff8636_thresholds(_sff8636_pages("synthetic-qsfp-dac-40g-passive-cu"))
    assert _within_bounds(result["temperature"])
    assert _within_bounds(result["supply_voltage"])
    for group in ("rx_power", "tx_bias", "tx_power"):
        for lane in result[group]:
            assert _within_bounds(lane)


def test_evaluate_sff8636_thresholds_requires_page03h():
    pages = {"lower": _read(f"{FIXTURES}/sff-8636/synthetic-qsfp-dac-40g-passive-cu.lower.bin")}
    with pytest.raises(ValueError, match="page03h"):
        evaluate_sff8636_thresholds(pages)
    with pytest.raises(ValueError, match="page03h"):
        evaluate_sff8636_thresholds({**pages, "03h": bytes(127)})
