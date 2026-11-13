from __future__ import annotations

import json
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parents[3]  # bindings/python/tests -> worktree root
FIXTURE_ROOT = REPO_ROOT / "tests" / "fixtures"
GOLDEN_ROOT = REPO_ROOT / "tests" / "golden"


def _collect_parity_cases():
    """Yield (stem, pages, golden) for each golden that has matching fixtures."""
    fixture_dir = FIXTURE_ROOT / "cmis"
    golden_dir = GOLDEN_ROOT / "cmis"
    if not golden_dir.exists():
        return
    for golden_path in sorted(golden_dir.glob("*.json")):
        stem = golden_path.stem
        lower_path = fixture_dir / f"{stem}.lower.bin"
        if not lower_path.exists():
            continue
        golden = json.loads(golden_path.read_text())
        page00h_path = fixture_dir / f"{stem}.page-00h.bin"
        pages: dict[str, bytes] = {
            "lower": lower_path.read_bytes(),
            "00h": page00h_path.read_bytes() if page00h_path.exists() else bytes(128),
        }
        p01h = fixture_dir / f"{stem}.page-01h.bin"
        if p01h.exists():
            pages["01h"] = p01h.read_bytes()
        p11h = fixture_dir / f"{stem}.page-11h.bin"
        if p11h.exists():
            pages["11h"] = p11h.read_bytes()
        # VDM pages (CMIS § 8.27): loaded when the fixture carries them.
        # '2fh' gates the `vdm` block in decode_cmis. 2Eh is not part of
        # the VDM feature and has no slice.
        for tag in (
            "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
            "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
        ):
            vdm_path = fixture_dir / f"{stem}.page-{tag}.bin"
            if vdm_path.exists():
                pages[tag] = vdm_path.read_bytes()
        yield pytest.param("cmis", stem, pages, golden, id=f"cmis/{stem}")

    fixture_dir = FIXTURE_ROOT / "sff-8636"
    golden_dir = GOLDEN_ROOT / "sff-8636"
    for golden_path in sorted(golden_dir.glob("*.json")):
        stem = golden_path.stem
        lower_path = fixture_dir / f"{stem}.lower.bin"
        page00h_path = fixture_dir / f"{stem}.page-00h.bin"
        if not lower_path.exists() or not page00h_path.exists():
            continue
        yield pytest.param(
            "sff-8636",
            stem,
            {"lower": lower_path.read_bytes(), "00h": page00h_path.read_bytes()},
            json.loads(golden_path.read_text()),
            id=f"sff-8636/{stem}",
        )

    fixture_dir = FIXTURE_ROOT / "sff-8472"
    golden_dir = GOLDEN_ROOT / "sff-8472"
    for golden_path in sorted(golden_dir.glob("*.json")):
        stem = golden_path.stem
        a0h_path = fixture_dir / f"{stem}.a0h.bin"
        a2h_path = fixture_dir / f"{stem}.a2h.bin"
        if not a0h_path.exists() or not a2h_path.exists():
            continue
        yield pytest.param(
            "sff-8472",
            stem,
            {"a0h": a0h_path.read_bytes(), "a2h": a2h_path.read_bytes()},
            json.loads(golden_path.read_text()),
            id=f"sff-8472/{stem}",
        )


PARITY_CASES = list(_collect_parity_cases())
