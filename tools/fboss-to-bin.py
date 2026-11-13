#!/usr/bin/env python3
"""Parse fboss FakeTransceiverImpl.cpp and extract per-page .bin files.

Source: https://github.com/facebook/fboss
File:   fboss/qsfp_service/module/tests/FakeTransceiverImpl.cpp
License: BSD-3-Clause (Copyright Facebook, Inc.)

The fboss test harness defines each fake transceiver as a set of `std::array<uint8_t, 128>`
constants — one per memory page — keyed together via `std::map<int, std::array<uint8_t, 128>>`
declarations like `kCmis<NAME>UpperPagesA0`. The map literals are the source of truth for
page IDs (e.g., `{0x10, kCmis<NAME>Page10}` means page 0x10, even though the variable name
suffix `Page10` looks like a decimal). This tool walks both the array and map declarations
and emits canonical per-page `.bin` files matching the openxcvr.page.v1 schema topology:

    <stem>.lower.bin           Lower memory (128 bytes)
    <stem>.page-NNh.bin        Upper Page 0xNN (128 bytes each)

The lower-memory variable uses one of two naming conventions:
    kCmis<NAME>LowerA0    (most fixtures)
    kCmis<NAME>Lower      (some fixtures, e.g., kCmis400GCr8)

Both are auto-detected.

Usage:
    tools/fboss-to-bin.py <FakeTransceiverImpl.cpp> --list
    tools/fboss-to-bin.py <FakeTransceiverImpl.cpp> <fixture_id> <output_stem>

Examples:
    tools/fboss-to-bin.py /tmp/fboss-fake-xcvr.cpp --list
    tools/fboss-to-bin.py /tmp/fboss-fake-xcvr.cpp kCmis800GZr \
        tests/fixtures/cmis/fboss-cmis-5-3-800g-zr-osfp
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ARRAY_HEAD_RE = re.compile(
    r"std::array<uint8_t,\s*128>\s+(\w+)\s*=\s*\{"
)
UPPER_MAP_HEAD_RE = re.compile(
    r"std::map<int,\s*std::array<uint8_t,\s*128>>\s+(\w+UpperPagesA0)\s*=\s*\{"
)
MAP_ENTRY_RE = re.compile(r"\{\s*(0x[0-9a-fA-F]+|\d+)\s*,\s*(\w+)\s*\}")
HEX_BYTE_RE = re.compile(r"0x([0-9A-Fa-f]{1,2})\b")


def _scan_balanced(src: str, open_pos: int) -> int:
    """Given `src[open_pos] == '{'`, return the index of the matching `}`."""
    depth = 0
    i = open_pos
    while i < len(src):
        c = src[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise ValueError(f"unbalanced braces starting at {open_pos}")


def parse_array_bytes(body: str) -> bytes:
    return bytes(int(m.group(1), 16) for m in HEX_BYTE_RE.finditer(body))


def index_arrays(src: str) -> dict[str, tuple[bytes, int]]:
    """Return {name: (bytes, source_offset)}. Offset is used for proximity pairing."""
    out: dict[str, tuple[bytes, int]] = {}
    for m in ARRAY_HEAD_RE.finditer(src):
        name = m.group(1)
        open_pos = m.end() - 1
        close_pos = _scan_balanced(src, open_pos)
        body = src[open_pos + 1 : close_pos]
        out[name] = (parse_array_bytes(body), m.start())
    return out


def index_upper_maps(src: str) -> dict[str, tuple[list[tuple[int, str]], int]]:
    """Return {map_name: (entries, source_offset)}. Offset enables proximity pairing."""
    out: dict[str, tuple[list[tuple[int, str]], int]] = {}
    for m in UPPER_MAP_HEAD_RE.finditer(src):
        name = m.group(1)
        open_pos = m.end() - 1
        close_pos = _scan_balanced(src, open_pos)
        body = src[open_pos + 1 : close_pos]
        entries = [
            (int(e.group(1), 0), e.group(2)) for e in MAP_ENTRY_RE.finditer(body)
        ]
        out[name] = (entries, m.start())
    return out


def find_paired_upper_map(
    fixture_id: str,
    lower_offset: int,
    upper_maps: dict[str, tuple[list[tuple[int, str]], int]],
) -> tuple[list[tuple[int, str]], str] | None:
    """Pair a Lower declaration with the matching UpperPagesA0 by:

    1. Exact name match (`kCmis<X>UpperPagesA0` for `kCmis<X>Lower[A0]`).
    2. Proximity: the next UpperPagesA0 declared after the lower memory in source,
       within 1500 chars (covers the largest fixture's ~22 page arrays).

    The fboss source uses inconsistent naming for OSFP fixtures (e.g., `kCmis400G2xDr4LowerA0`
    is paired with `kCmis2x400GDr4UpperPagesA0` — different word order), so name-match alone
    misses pairs. Proximity is the robust fallback.
    """
    exact = upper_maps.get(f"{fixture_id}UpperPagesA0")
    if exact is not None:
        return exact[0], f"{fixture_id}UpperPagesA0"
    candidates = [
        (offset - lower_offset, name, entries)
        for name, (entries, offset) in upper_maps.items()
        if offset > lower_offset and offset - lower_offset < 20000
    ]
    if not candidates:
        return None
    candidates.sort()
    _, name, entries = candidates[0]
    return entries, name


def list_cmis_fixtures(src: str) -> list[tuple[str, int, int, list[int], str]]:
    """Return [(fixture_id, identifier, rev_byte, [page_ids], upper_map_name)]."""
    arrays = index_arrays(src)
    upper_maps = index_upper_maps(src)
    out: list[tuple[str, int, int, list[int], str]] = []
    seen: set[str] = set()
    for var_name in arrays:
        if not var_name.startswith("kCmis"):
            continue
        for suffix in ("LowerA0", "Lower"):
            if var_name.endswith(suffix):
                fid = var_name[: -len(suffix)]
                break
        else:
            continue
        if fid in seen:
            continue
        seen.add(fid)
        lower, lower_offset = arrays[var_name]
        if len(lower) != 128:
            continue
        ident, rev = lower[0], lower[1]
        paired = find_paired_upper_map(fid, lower_offset, upper_maps)
        if paired is None:
            page_ids: list[int] = []
            map_name = "(none)"
        else:
            page_ids = sorted(pid for (pid, _) in paired[0])
            map_name = paired[1]
        out.append((fid, ident, rev, page_ids, map_name))
    return sorted(out)


def extract_fixture(src: str, fixture_id: str, stem: Path) -> list[Path]:
    arrays = index_arrays(src)
    upper_maps = index_upper_maps(src)

    lower_pair = arrays.get(f"{fixture_id}LowerA0") or arrays.get(f"{fixture_id}Lower")
    if lower_pair is None:
        raise SystemExit(
            f"No lower memory found for {fixture_id} "
            f"(tried {fixture_id}LowerA0 and {fixture_id}Lower)"
        )
    lower, lower_offset = lower_pair
    if len(lower) != 128:
        raise SystemExit(
            f"{fixture_id} lower memory has {len(lower)} bytes (expected 128)"
        )

    paired = find_paired_upper_map(fixture_id, lower_offset, upper_maps)
    if paired is None:
        raise SystemExit(f"No UpperPagesA0 map paired with {fixture_id}")
    upper_entries, map_name = paired
    if map_name != f"{fixture_id}UpperPagesA0":
        print(
            f"info: paired by proximity: {fixture_id} <-> {map_name}",
            file=sys.stderr,
        )

    written: list[Path] = []
    out_lower = stem.with_suffix(".lower.bin")
    out_lower.write_bytes(lower)
    written.append(out_lower)

    for page_id, var_name in sorted(upper_entries):
        page_pair = arrays.get(var_name)
        if page_pair is None:
            print(
                f"warning: {fixture_id} page 0x{page_id:02x} -> {var_name} (array not found)",
                file=sys.stderr,
            )
            continue
        page_bytes = page_pair[0]
        if len(page_bytes) != 128:
            print(
                f"warning: {fixture_id} page 0x{page_id:02x} ({var_name}) has "
                f"{len(page_bytes)} bytes (expected 128)",
                file=sys.stderr,
            )
            continue
        out = stem.with_suffix(f".page-{page_id:02x}h.bin")
        out.write_bytes(page_bytes)
        written.append(out)

    return written


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("source", type=Path, help="path to FakeTransceiverImpl.cpp")
    p.add_argument("--list", action="store_true", help="list all kCmis* fixtures and exit")
    p.add_argument("fixture_id", nargs="?", help="e.g., kCmis800GZr")
    p.add_argument("output_stem", nargs="?", type=Path)
    args = p.parse_args(argv)

    src = args.source.read_text()
    if args.list:
        rows = list_cmis_fixtures(src)
        rev_label = lambda r: f"{(r >> 4) & 0xF}.{r & 0xF}"
        ident_label = {
            0x18: "QSFP-DD",
            0x19: "OSFP",
            0x1B: "DSFP",
            0x1E: "QSFP112",
            0x0D: "QSFP+",
            0x11: "QSFP28",
        }
        print(f"{'Fixture':<32} {'ID':<10} {'Rev':<6} {'Map prefix':<32} Pages")
        print("-" * 120)
        for fid, ident, rev, pages, map_name in rows:
            id_lbl = ident_label.get(ident, f"unk(0x{ident:02x})")
            page_str = ", ".join(f"0x{p:02x}" for p in pages) or "(none)"
            short_map = map_name[: -len("UpperPagesA0")] if map_name.endswith("UpperPagesA0") else map_name
            print(f"{fid:<32} {id_lbl:<10} {rev_label(rev):<6} {short_map:<32} {page_str}")
        return 0

    if not args.fixture_id or not args.output_stem:
        p.error("fixture_id and output_stem are required (or use --list)")
    outs = extract_fixture(src, args.fixture_id, args.output_stem)
    for out in outs:
        print(f"wrote {out} ({out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
