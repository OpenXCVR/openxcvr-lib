"""Consumer example: a synthetic page dict, no hardware, fed to the public facade.

The bytes are constructed here. They are not a vendor capture. Lane `index`
in the facade JSON is 1-based.
"""

from __future__ import annotations

import pytest

from openxcvr import (
    decode_sff8472_lane_signal_flags,
    decode_sff8636_lane_signal_flags,
    evaluate_sff8636_thresholds,
)


def synthetic_sff8636_lower() -> bytes:
    page = bytearray(128)
    page[1] = 0x07  # compliance 2.5: adaptive-EQ is modeled
    page[3] = 0x11  # lane 1 Rx LOS (bit 0) and Tx LOS (bit 4)
    page[86] = 0x01  # lane 1 Tx disable
    # Case temperature 2.0 C at 1/256 C per LSB (big-endian int16 at byte 22).
    page[22] = 0x02
    page[23] = 0x00
    return bytes(page)


def synthetic_sff8636_page03h() -> bytes:
    page = bytearray(128)
    # Temperature set, severity order high alarm, low alarm, high warning, low warning.
    # 1.0 C, -1.0 C, 0.5 C, -0.5 C.
    page[0:8] = bytes([0x01, 0x00, 0xFF, 0x00, 0x00, 0x80, 0xFF, 0x80])
    return bytes(page)


def test_synthetic_sff8636_lane_flags_and_thresholds():
    pages = {
        "lower": synthetic_sff8636_lower(),
        "00h": bytes(128),
        "03h": synthetic_sff8636_page03h(),
    }
    lanes = decode_sff8636_lane_signal_flags(pages)
    assert len(lanes) == 4
    assert lanes[0]["index"] == 1
    assert lanes[0]["rx_los"] is True
    assert lanes[0]["tx_los"] is True
    assert lanes[0]["tx_disable"] is True
    assert lanes[0]["tx_fault"] is False
    assert lanes[0]["tx_adaptive_eq_fault"] is False
    assert lanes[1]["rx_los"] is False

    evaluated = evaluate_sff8636_thresholds(pages)
    assert evaluated["temperature"] == {
        "high_alarm": True,
        "low_alarm": False,
        "high_warning": True,
        "low_warning": False,
    }
    assert evaluated["supply_voltage"] == {
        "high_alarm": False,
        "low_alarm": False,
        "high_warning": False,
        "low_warning": False,
    }


def test_missing_threshold_page_is_an_error_not_a_zero_page():
    with pytest.raises(ValueError, match="page03h"):
        evaluate_sff8636_thresholds({"lower": synthetic_sff8636_lower()})


def test_synthetic_sff8472_lane_flags_omit_unmodeled_adaptive_eq():
    early = bytearray(256)
    early[92] = 0x20  # internally calibrated
    early[94] = 0x01  # rev before 12.5
    a2h = bytearray(256)
    a2h[110] = 0x02  # Rx LOS
    a2h[118] = 0x10  # adaptive-EQ bit, not modeled at this revision
    lanes = decode_sff8472_lane_signal_flags({"a0h": bytes(early), "a2h": bytes(a2h)})
    assert lanes == [
        {"index": 1, "rx_los": True, "tx_disable": False, "tx_fault": False}
    ]
    assert "tx_adaptive_eq_fault" not in lanes[0]
    assert "tx_los" not in lanes[0]
