"""Parity tests: Python decode_cmis output must match golden JSON fixtures.

Skipped fields (by design — differ legitimately between implementations):
  - _metadata: authorship/provenance info, not part of decode output
  - module.module_type: coherent-module classification not implemented in v0.1
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest

from openxcvr import decode_cmis, decode_sff8472, decode_sff8636
from tests.conftest import PARITY_CASES

_CMIS_SKIP_KEYS = frozenset({"_metadata", "module_type"})
_SKIP_KEYS = frozenset({"_metadata"})


def _diff(result: Any, golden: Any, skip_keys: frozenset[str], path: str = "") -> list[str]:
    """Recursively compute differences between result and golden, skipping _SKIP_KEYS."""
    diffs: list[str] = []
    if isinstance(result, dict) and isinstance(golden, dict):
        all_keys = set(result) | set(golden)
        for k in sorted(all_keys):
            if k in skip_keys:
                continue
            child = f"{path}.{k}" if path else k
            if k not in result:
                diffs.append(f"{child}: missing from result (golden={golden[k]!r})")
            elif k not in golden:
                diffs.append(f"{child}: extra in result (result={result[k]!r})")
            else:
                diffs.extend(_diff(result[k], golden[k], skip_keys, child))
    elif isinstance(result, list) and isinstance(golden, list):
        if len(result) != len(golden):
            diffs.append(f"{path}: list length {len(result)} != {len(golden)}")
        for i, (r, g) in enumerate(zip(result, golden)):
            diffs.extend(_diff(r, g, skip_keys, f"{path}[{i}]"))
    elif result != golden:
        diffs.append(f"{path}: {result!r} != {golden!r}")
    return diffs


def test_cmis_lane_status_latches() -> None:
    """Pin the per-lane status latches (rx_los ← LOSFlagRx, Table 8-88;
    tx_fault ← FailureFlagTx, Table 8-87) on the one fixture whose Page 11h
    carries real latched values, independently of the deep golden compare:
    fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched on lanes 1/3 and
    tx_fault on lanes 2/4, everything else clear.
    """
    fixture_dir = Path(__file__).parents[3] / "tests" / "fixtures" / "cmis"
    stem = "fboss-cmis-4-0-qsfp112-200g-fr4"
    suffixes = {"lower": "lower.bin", "00h": "page-00h.bin", "01h": "page-01h.bin", "11h": "page-11h.bin"}
    pages = {key: (fixture_dir / f"{stem}.{suffix}").read_bytes() for key, suffix in suffixes.items()}
    result = decode_cmis(pages)
    lanes = result["lanes"]
    assert len(lanes) == 8
    for i, lane in enumerate(lanes):
        idx = i + 1
        assert lane["rx_los"] == (idx in (1, 3)), f"lane {idx} rx_los"
        assert lane["tx_fault"] == (idx in (2, 4)), f"lane {idx} tx_fault"


@pytest.mark.parametrize("spec,stem,pages,golden", PARITY_CASES)
def test_parity(spec: str, stem: str, pages: dict[str, bytes], golden: dict) -> None:
    if spec == "cmis":
        result = decode_cmis(pages)
        skip_keys = _CMIS_SKIP_KEYS
    elif spec == "sff-8636":
        result = decode_sff8636(pages)
        skip_keys = _SKIP_KEYS
    elif spec == "sff-8472":
        result = decode_sff8472(pages)
        skip_keys = _SKIP_KEYS
    else:
        raise AssertionError(f"unknown spec {spec}")
    diffs = _diff(result, golden, skip_keys)
    assert not diffs, f"Parity failures for {stem}:\n" + "\n".join(f"  {d}" for d in diffs)
