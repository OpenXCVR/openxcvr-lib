"""Checksum-recompute tests for all pages with sum8 checksum fields.

For each fixture, read the raw page buffer, call recompute_checksums(), and
assert that the recomputed byte equals the byte already stored in the fixture.
This proves the generated recompute logic is correct for every fixture in the
corpus.

Mirrors the Rust tests in:
  crates/openxcvr-cmis/tests/checksum_recompute.rs
  crates/openxcvr-sff8636/tests/checksum_recompute.rs
  crates/openxcvr-sff8472/tests/checksum_recompute.rs
"""
from __future__ import annotations

from pathlib import Path

import pytest

from openxcvr._generated.cmis_page00h_administrative import CmisPage00hAdministrative
from openxcvr._generated.cmis_page01h_advertising import CmisPage01hAdvertising
from openxcvr._generated.cmis_page02h_thresholds import CmisPage02hThresholds
from openxcvr._generated.page_a0h_base_memory import page_a0h_base_memory as PageA0hBaseMemory
from openxcvr._generated.page_a2h_diagnostics_base import page_a2h_diagnostics_base as PageA2hDiagnosticsBase
from openxcvr._generated.sff8636_page00h_upper_administrative import (
    Sff8636Page00hUpperAdministrative,
)

REPO_ROOT = Path(__file__).parents[3]
FIXTURE_ROOT = REPO_ROOT / "tests" / "fixtures"


def _read_exact(path: Path, size: int) -> bytearray:
    data = path.read_bytes()
    assert len(data) == size, f"{path}: expected {size} bytes, got {len(data)}"
    return bytearray(data)


def _read_padded(path: Path, size: int) -> bytearray:
    data = path.read_bytes()
    assert len(data) <= size, f"{path}: file is {len(data)} bytes, expected at most {size}"
    buf = bytearray(size)
    buf[: len(data)] = data
    return buf


# ─── CMIS Page 00h: checksum at buf[94] covering buf[0:94] ───────────────────
# Only fixtures whose stored checksum matches the sum8 formula.
# 12 of the 17 fboss Page 00h fixtures carry vendor-specific pre-computed values
# and are intentionally excluded (same exclusion list as Rust tests).

_CMIS_PAGE00H_STEMS = [
    "fboss-cmis-4-0-qsfp112-200g-fr4",
    "fboss-cmis-4-0-qsfpdd-400g-fr4-multiport",
    "fboss-cmis-4-0-qsfpdd-400g-lr4",
    "synthetic-cmis-3-0-qsfp-dd",
    "synthetic-cmis-5-2-qsfp-dd",
]


@pytest.mark.parametrize("stem", _CMIS_PAGE00H_STEMS)
def test_cmis_page00h_recompute(stem: str) -> None:
    path = FIXTURE_ROOT / "cmis" / f"{stem}.page-00h.bin"
    buf = _read_exact(path, 128)
    stored = buf[94]
    CmisPage00hAdministrative.recompute_checksums(buf)
    assert buf[94] == stored, (
        f"{stem} page-00h checksum: recomputed {buf[94]:#04x} != stored {stored:#04x}"
    )


# ─── CMIS Page 01h: checksum at buf[127] covering buf[2:127] ─────────────────
# Page 01h is the only checksummed page whose covered region does NOT start at
# buf[0] (CMIS 5.3 § 8.4.17: sum8 of buf[2..=126] → buf[127]).  We use a
# synthetic buffer: buf[0]=buf[1]=0xff (sentinel proving those bytes are
# excluded), buf[2..=126]=0x01 (125 bytes).  Correct sum → 0x7d.  If the
# implementation incorrectly starts at buf[0] the result would be 0x7b.


def test_cmis_page01h_recompute_non_zero_start() -> None:
    buf = bytearray(128)
    buf[0] = 0xFF
    buf[1] = 0xFF
    for i in range(2, 127):
        buf[i] = 0x01
    CmisPage01hAdvertising.recompute_checksums(buf)
    assert buf[127] == 0x7D, (
        f"CMIS page-01h checksum: got {buf[127]:#04x}, expected 0x7d "
        "(buf[0]/buf[1] sentinel 0xff must be EXCLUDED from the sum)"
    )


# ─── CMIS Page 02h: checksum at buf[127] covering buf[0:127] ─────────────────
# All 15 Page 02h fixtures in the corpus carry valid sum8 checksums.

_CMIS_PAGE02H_STEMS = [
    "fboss-cmis-4-0-osfp-2x400g-dr4",
    "fboss-cmis-4-0-qsfp112-200g-fr4",
    "fboss-cmis-4-0-qsfpdd-400g-dr4",
    "fboss-cmis-4-0-qsfpdd-400g-fr4-multiport",
    "fboss-cmis-4-0-qsfpdd-400g-lr4",
    "fboss-cmis-5-0-osfp-2x400g-fr4",
    "fboss-cmis-5-1-osfp-2x400g-fr4-lite",
    "fboss-cmis-5-1-osfp-2x400g-fr4-lpo",
    "fboss-cmis-5-1-osfp-2x800g-dr4",
    "fboss-cmis-5-1-osfp-credo-800g-aec",
    "fboss-cmis-5-3-osfp-800g-zr",
    "fboss-cmis-5-3-osfp-800g-zr-bias-reserved",
    "fboss-cmis-5-3-osfp-800g-zr-dp-reserved",
    "synthetic-cmis-3-0-qsfp-dd",
    "synthetic-cmis-5-2-qsfp-dd",
]


@pytest.mark.parametrize("stem", _CMIS_PAGE02H_STEMS)
def test_cmis_page02h_recompute(stem: str) -> None:
    path = FIXTURE_ROOT / "cmis" / f"{stem}.page-02h.bin"
    buf = _read_exact(path, 128)
    stored = buf[127]
    CmisPage02hThresholds.recompute_checksums(buf)
    assert buf[127] == stored, (
        f"{stem} page-02h checksum: recomputed {buf[127]:#04x} != stored {stored:#04x}"
    )


# ─── SFF-8636 Page 00h: CC_BASE at buf[63] + CC_EXT at buf[95] ───────────────

_SFF8636_PAGE00H_STEMS = [
    "finisar-ftl410qe3c-40g-sr4",
    "finisar-ftlc9551repm-100g-sr4",
    "synthetic-qsfp-dac-40g-passive-cu",
    "synthetic-qsfp28-aoc-100g-sr4",
    "synthetic-qsfp28-bidi-100g-cwdm4",
]


@pytest.mark.parametrize("stem", _SFF8636_PAGE00H_STEMS)
def test_sff8636_page00h_recompute(stem: str) -> None:
    path = FIXTURE_ROOT / "sff-8636" / f"{stem}.page-00h.bin"
    buf = _read_exact(path, 128)
    cc_base_stored = buf[63]
    cc_ext_stored = buf[95]
    Sff8636Page00hUpperAdministrative.recompute_checksums(buf)
    assert buf[63] == cc_base_stored, (
        f"{stem} CC_BASE: recomputed {buf[63]:#04x} != stored {cc_base_stored:#04x}"
    )
    assert buf[95] == cc_ext_stored, (
        f"{stem} CC_EXT: recomputed {buf[95]:#04x} != stored {cc_ext_stored:#04x}"
    )


# ─── SFF-8472 A0h: CC_BASE at buf[63] + CC_EXT at buf[95] ───────────────────
# A0h fixture files are 128 bytes; the page is 256 bytes — zero-pad to match
# the Rust read_padded::<256>() behaviour.

_SFF8472_A0H_STEMS = [
    "finisar-ftlx8571d3bcl-mup0wb0",
    "finisar-ftlx8571d3bcl-muq1bzb",
    "synthetic-dac-sfp10g-passive-cu",
    "synthetic-extcal-sfp10g-sr",
    "synthetic-sfp28-25g-sr",
]


@pytest.mark.parametrize("stem", _SFF8472_A0H_STEMS)
def test_sff8472_a0h_recompute(stem: str) -> None:
    path = FIXTURE_ROOT / "sff-8472" / f"{stem}.a0h.bin"
    buf = _read_padded(path, 256)
    cc_base_stored = buf[63]
    cc_ext_stored = buf[95]
    PageA0hBaseMemory.recompute_checksums(buf)
    assert buf[63] == cc_base_stored, (
        f"{stem} a0h CC_BASE: recomputed {buf[63]:#04x} != stored {cc_base_stored:#04x}"
    )
    assert buf[95] == cc_ext_stored, (
        f"{stem} a0h CC_EXT: recomputed {buf[95]:#04x} != stored {cc_ext_stored:#04x}"
    )


# ─── SFF-8472 A2h diagnostics_base: CHKSUM at buf[95] ───────────────────────

_SFF8472_A2H_STEMS = [
    "finisar-ftlx8571d3bcl-mup0wb0",
    "finisar-ftlx8571d3bcl-muq1bzb",
    "synthetic-dac-sfp10g-passive-cu",
    "synthetic-extcal-sfp10g-sr",
    "synthetic-sfp28-25g-sr",
]


@pytest.mark.parametrize("stem", _SFF8472_A2H_STEMS)
def test_sff8472_a2h_recompute(stem: str) -> None:
    path = FIXTURE_ROOT / "sff-8472" / f"{stem}.a2h.bin"
    buf = _read_exact(path, 256)
    chksum_stored = buf[95]
    PageA2hDiagnosticsBase.recompute_checksums(buf)
    assert buf[95] == chksum_stored, (
        f"{stem} a2h CHKSUM: recomputed {buf[95]:#04x} != stored {chksum_stored:#04x}"
    )
