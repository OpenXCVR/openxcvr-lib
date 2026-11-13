"""Round-trip tests (encode-round-trip Wave 2, Python): set_<field> is the
exact inverse of decode; writing decoded values back is byte-identical."""

from __future__ import annotations

from pathlib import Path

from openxcvr._generated.cmis_page00h_administrative import (
    CmisPage00hAdministrative as Admin,
)

REPO_ROOT = Path(__file__).parents[3]
FIXTURE = (
    REPO_ROOT
    / "tests"
    / "fixtures"
    / "cmis"
    / "fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin"
)


def _orig() -> bytearray:
    return bytearray(FIXTURE.read_bytes()[:128])


def test_u8_setter_is_decode_inverse():
    buf = bytearray(128)
    Admin.set_max_power(buf, 42)
    assert Admin.decode(bytes(buf)).max_power == 42


def test_ascii_setter_is_decode_inverse():
    buf = bytearray(128)
    Admin.set_vendor_name(buf, "ACME OPTICS")
    assert Admin.decode(bytes(buf)).vendor_name == "ACME OPTICS"


def test_writing_decoded_values_back_is_identity():
    orig = _orig()
    d = Admin.decode(bytes(orig))
    copy = bytearray(orig)
    Admin.set_max_power(copy, d.max_power)
    Admin.set_vendor_name(copy, d.vendor_name)
    Admin.set_vendor_oui(copy, d.vendor_oui)
    Admin.set_vendor_pn(copy, d.vendor_pn)
    assert copy == orig, "writing decoded values back must be a no-op"


def test_bit_field_setter_preserves_neighbors():
    buf = bytearray(128)
    buf[82] = 0xFF
    Admin.set_media_lane_unsupported_lane_1(buf, False)
    assert buf[82] == 0xFE
    Admin.set_media_lane_unsupported_lane_2(buf, False)
    assert buf[82] == 0xFC
