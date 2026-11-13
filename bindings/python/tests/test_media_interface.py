from pathlib import Path
import pytest
import openxcvr

REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURE_DIR = REPO_ROOT / "tests" / "fixtures" / "cmis"

DR4_STEM = "fboss-cmis-4-0-qsfpdd-400g-dr4"
OSFP_STEM = "fboss-cmis-4-0-osfp-2x400g-dr4"

TEST_CASES = [
    (0x01, 0x01, "base_10g_sw"),
    (0x01, 0x1C, "fc_128g_mm"),
    (0x01, 0xC0, "vendor_specific"),
    (0x02, 0x01, "base_10g_lw"),
    (0x02, 0x1C, "base_400g_dr4"),
    (0x02, 0x68, "flexo_8e_dpo_16qam_foic8e_8_dpo"),
    (0x02, 0xC0, "vendor_specific"),
    (0x02, 0xC1, "vendor_specific"),
    (0x02, 0xF7, "vendor_specific_f7"),
    (0x03, 0x01, "copper_cable"),
    (0x03, 0x1C, "reserved_02_be"),
    (0x03, 0xC0, "linear_active_copper_loopback_module"),
    (0x03, 0xC1, "vendor_specific"),
    (0x04, 0x01, "active_cable_ber_1e_12"),
    (0x04, 0x1C, "reserved_05_be"),
    (0x04, 0xF7, "vendor_specific"),
    (0x05, 0x01, "base_1000_t"),
    (0x05, 0xC1, "vendor_specific"),
    (0x00, 0x1C, "unknown"),
    (0x06, 0x1C, "unknown"),
    (0x40, 0x1C, "unknown"),
    (0x90, 0x1C, "unknown"),
    (0xFF, 0x1C, "unknown"),
]

def load_pages(stem: str) -> dict[str, bytes]:
    lower_path = FIXTURE_DIR / f"{stem}.lower.bin"
    page00h_path = FIXTURE_DIR / f"{stem}.page-00h.bin"
    pages: dict[str, bytes] = {
        "lower": lower_path.read_bytes(),
        "00h": page00h_path.read_bytes() if page00h_path.exists() else bytes(128),
    }
    p01h = FIXTURE_DIR / f"{stem}.page-01h.bin"
    if p01h.exists():
        pages["01h"] = p01h.read_bytes()
    p11h = FIXTURE_DIR / f"{stem}.page-11h.bin"
    if p11h.exists():
        pages["11h"] = p11h.read_bytes()
    return pages

@pytest.mark.parametrize("media_type,mif,expected", TEST_CASES)
def test_media_interface_lower_caller(media_type, mif, expected):
    pages = load_pages(DR4_STEM)
    lower = bytearray(pages["lower"])
    lower[85] = media_type
    lower[87] = mif
    pages["lower"] = bytes(lower)

    result = openxcvr.decode_cmis(pages)
    apps = result.get("applications", [])
    assert len(apps) > 0
    assert apps[0]["media_interface_name"] == expected

@pytest.mark.parametrize("media_type,mif,expected", TEST_CASES)
def test_media_interface_page01_caller(media_type, mif, expected):
    pages = load_pages(OSFP_STEM)
    lower = bytearray(pages["lower"])
    lower[85] = media_type
    pages["lower"] = bytes(lower)

    page01 = bytearray(pages["01h"])
    page01[96] = mif
    pages["01h"] = bytes(page01)

    result = openxcvr.decode_cmis(pages)
    apps = result.get("applications", [])
    entry = next((a for a in apps if a.get("app_code") == 9 or a.get("application_code") == 9), None)
    assert entry is not None
    assert entry["media_interface_name"] == expected
