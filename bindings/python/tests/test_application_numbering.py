"""Test CMIS application numbering across revisions and edge cases.

Verifies:
(a) For revisions 0x30, 0x40, 0x50, 0x51, 0x52, 0x53 with both pages:
    app_code == [1..15], and for 0x53 host_interface_id matches expected ZR literals.
(b) Page 01h not supplied: app_code == [1..8].
(c) Lower slot 3 HIF set to 0xFF (terminator): Lower yields [1, 2], Page 01h yields [9..15].
(d) Every Page 01h descriptor MIF set to 0: app_code == [1..8].
"""

from pathlib import Path
import pytest

import openxcvr


EXPECTED_HIF_0x53 = [81, 82, 79, 80, 77, 78, 75, 76, 81, 82, 79, 80, 77, 78, 75]
REVISIONS = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53]


def find_repo_root() -> Path:
    p = Path(__file__).resolve().parent
    for _ in range(6):
        if (p / "tests" / "golden").exists() or (p / "schemas").exists():
            return p
        p = p.parent
    return Path(__file__).resolve().parents[3]


def load_zr_buffers():
    root = find_repo_root()
    fixtures_dir = root / "tests" / "fixtures" / "cmis"
    prefix = fixtures_dir / "fboss-cmis-5-3-osfp-800g-zr"
    lower_path = prefix.with_suffix(".lower.bin")
    p00_path = prefix.with_suffix(".page-00h.bin")
    p01_path = prefix.with_suffix(".page-01h.bin")

    lower = bytearray(lower_path.read_bytes())
    p00 = bytearray(p00_path.read_bytes())
    p01 = bytearray(p01_path.read_bytes())
    return lower, p00, p01


def run_decode(lower: bytes, page00h: bytes, page01h: bytes | None = None):
    pages = {
        "lower": bytes(lower),
        "00h": bytes(page00h),
    }
    if page01h is not None:
        pages["01h"] = bytes(page01h)
    return openxcvr.decode_cmis(pages)


def get_app_codes(decoded):
    apps = decoded.get("applications") if isinstance(decoded, dict) else getattr(decoded, "applications", None)
    if apps is None:
        return []
    res = []
    for app in apps:
        if isinstance(app, dict):
            res.append(app.get("app_code"))
        else:
            res.append(getattr(app, "app_code", None))
    return res


def get_hifs(decoded):
    apps = decoded.get("applications") if isinstance(decoded, dict) else getattr(decoded, "applications", None)
    if apps is None:
        return []
    res = []
    for app in apps:
        if isinstance(app, dict):
            res.append(app.get("host_interface_id"))
        else:
            res.append(getattr(app, "host_interface_id", None))
    return res


@pytest.mark.parametrize("rev", REVISIONS)
def test_revisions_additive(rev):
    """(a) For each Lower:1 revision byte 0x30..0x53: app codes [1..15], and for 0x53 HIF matches."""
    lower, p00, p01 = load_zr_buffers()
    lower[1] = rev
    decoded = run_decode(lower, p00, p01)
    app_codes = get_app_codes(decoded)
    assert app_codes == list(range(1, 16))
    if rev == 0x53:
        hifs = get_hifs(decoded)
        assert hifs == EXPECTED_HIF_0x53


def test_page01h_not_supplied():
    """(b) Page 01h not supplied: app codes exactly [1..8]."""
    lower, p00, _ = load_zr_buffers()
    decoded = run_decode(lower, p00, None)
    app_codes = get_app_codes(decoded)
    assert app_codes == list(range(1, 9))


def test_lower_slot3_terminator():
    """(c) Lower slot 3 HIF set to 0xFF (terminator): Lower yields [1,2], Page 01h still yields [9..15].

    Preservation of existing facade behaviour, not normative spec rules (design-decision A3).
    """
    lower, p00, p01 = load_zr_buffers()
    # Lower slot 3 descriptor is at bytes 86 + 2*4 = 94. Byte 94 is HIF.
    lower[94] = 0xFF
    decoded = run_decode(lower, p00, p01)
    app_codes = get_app_codes(decoded)
    assert app_codes == [1, 2] + list(range(9, 16))


def test_page01h_all_mif_zero():
    """(d) Every Page 01h descriptor MIF set to 0: app codes [1..8] (MIF-0 skip preserved).

    Preservation of existing facade behaviour, not normative spec rules (design-decision A3).
    """
    lower, p00, p01 = load_zr_buffers()
    # Page 01h has 7 descriptors at buffer offsets 95-122.
    # Each descriptor is 4 bytes: [HIF, MIF, ...]. MIF is offset + 1.
    for i in range(7):
        mif_offset = 95 + i * 4 + 1
        p01[mif_offset] = 0
    decoded = run_decode(lower, p00, p01)
    app_codes = get_app_codes(decoded)
    assert app_codes == list(range(1, 9))
