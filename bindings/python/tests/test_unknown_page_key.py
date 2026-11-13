"""Regression: an unknown canonical page-map key fails closed.

Consumer facade contract (bindings/README.md): a page map with an unknown
key is a language error naming the key, matching the Go/TS canonical
adapters (`rejectUnknownKeys` / `rejectUnknown`). Before the fix the Python
decoders silently accepted any extra key.
"""

from __future__ import annotations

import pytest

from openxcvr import (
    decode_cmis,
    decode_sff8472,
    decode_sff8472_lane_signal_flags,
    decode_sff8636,
    decode_sff8636_lane_signal_flags,
    evaluate_sff8472_thresholds,
    evaluate_sff8636_thresholds,
)


def _cmis_lower() -> bytes:
    page = bytearray(128)
    page[1] = 0x53  # CMIS 5.3, paged memory
    return bytes(page)


def _cmis_valid() -> dict[str, bytes]:
    return {"lower": _cmis_lower(), "00h": bytes(128)}


def _sff8636_valid() -> dict[str, bytes]:
    return {"lower": bytes(128), "00h": bytes(128)}


def _sff8472_valid() -> dict[str, bytes]:
    return {"a0h": bytes(256), "a2h": bytes(256)}


@pytest.mark.parametrize(
    "decode",
    [
        decode_cmis,
        decode_sff8636,
        decode_sff8472,
        decode_sff8636_lane_signal_flags,
        decode_sff8472_lane_signal_flags,
        evaluate_sff8636_thresholds,
        evaluate_sff8472_thresholds,
    ],
)
@pytest.mark.parametrize("bad_key", ["99h", "Page 11h", "0x11h"])
def test_unknown_page_key_fails_closed(decode, bad_key):
    pages = {
        "cmis": _cmis_valid(),
        "sff-8636": _sff8636_valid(),
        "sff-8472": _sff8472_valid(),
    }
    family = {
        decode_cmis: "cmis",
        decode_sff8636: "sff-8636",
        decode_sff8636_lane_signal_flags: "sff-8636",
        evaluate_sff8636_thresholds: "sff-8636",
        decode_sff8472: "sff-8472",
        decode_sff8472_lane_signal_flags: "sff-8472",
        evaluate_sff8472_thresholds: "sff-8472",
    }[decode]
    supplied = dict(pages[family])
    supplied[bad_key] = b"x"
    with pytest.raises(ValueError, match=bad_key):
        decode(supplied)


def test_cmis_valid_pages_still_decode():
    assert decode_cmis(_cmis_valid())["decode_status"] == "ok"


def test_cmis_known_optional_keys_still_decode():
    supplied = _cmis_valid()
    supplied["01h"] = bytes(128)
    supplied["11h"] = bytes(128)
    assert decode_cmis(supplied)["decode_status"] == "ok"


def test_sff8636_valid_pages_still_decode():
    assert decode_sff8636(_sff8636_valid())["decode_status"] == "ok"
    supplied = _sff8636_valid()
    supplied["03h"] = bytes(128)
    assert decode_sff8636(supplied)["decode_status"] == "ok"


def test_sff8472_valid_pages_still_decode():
    assert decode_sff8472(_sff8472_valid())["decode_status"] == "ok"
