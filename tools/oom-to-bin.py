#!/usr/bin/env python3
"""Convert OCP OOM module_data text dumps to canonical .bin files.

Source: https://github.com/opencomputeproject/oom (MIT, OCP, 2016)

Two text formats are auto-detected:

  1. `;FCC SETUP / ;FORMAT: 1` — Finisar factory FCC test setup (SFP+ family).
     Each `;Table: N` block holds one device-address page (A0h base or A2h
     diagnostics). Hex bytes are grouped as 4-byte big-endian words:
         `0000: AABBCCDD AABBCCDD ...`

  2. `EEPROM Setup / FORMAT 1` — QSFP module dump.
     Each `!Index: Table 0xNNNN: 0xPP(P)` block is a 128-byte slice keyed by
     page id (the `0xPP` value). Hex bytes are single-byte tokens:
         `0000: AA BB CC DD AA BB CC DD ...`

Output is one .bin file per logical page, named per the openxcvr schema topology:

  SFP+ (SFF-8472):
    <stem>.a0h.bin  (128 bytes — A0h:0-127, the SFP base ID; OOM EEPROM*.txt)
    <stem>.a2h.bin  (256 bytes — A2h:0-255, diagnostics + user; OOM FCCABY*.txt)

  QSFP/QSFP28 (SFF-8636):
    <stem>.lower.bin     (128 bytes — Lower 0-127, always present)
    <stem>.page-00h.bin  (128 bytes — Upper Page 00h Administrative)
    <stem>.page-01h.bin  (128 bytes — Upper Page 01h, deprecated stub)
    <stem>.page-02h.bin  (128 bytes — Upper Page 02h User EEPROM)
    <stem>.page-03h.bin  (128 bytes — Upper Page 03h Thresholds + Controls)

Usage:
    tools/oom-to-bin.py <input.txt> <output_stem>

    For an SFP+ pair, run twice with the same stem — one EEPROM input then one
    FCCABY input — and the tool writes .a0h.bin and .a2h.bin respectively.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


def detect_format(text: str) -> str:
    head = text[:200]
    if ";FCC SETUP" in head and ";FORMAT:" in head:
        return "fcc-setup"
    if "EEPROM Setup" in head and "FORMAT 1" in head:
        return "eeprom-setup"
    raise ValueError("Unrecognized OOM format (expected ;FCC SETUP or EEPROM Setup header)")


_HEX_PAIR = re.compile(r"[0-9A-Fa-f]{2}")


def _bytes_from_data_line(line: str, *, group_size: int) -> bytes:
    # Drop the leading "XXXX:" offset; we trust the `;Bytes:` header instead.
    if ":" not in line:
        return b""
    _, payload = line.split(":", 1)
    out = bytearray()
    for token in payload.split():
        # Each token is either 4-byte (`AABBCCDD`) or 1-byte (`AA`); accept both.
        if len(token) % 2:
            raise ValueError(f"odd-length hex token {token!r} in line {line!r}")
        for pair in _HEX_PAIR.findall(token):
            out.append(int(pair, 16))
    return bytes(out)


def parse_fcc_setup(text: str) -> dict[int, bytes]:
    """Return {table_id: bytes}. Table 0 = bytes 0-127, Table 1 = bytes 128-255."""
    pages: dict[int, bytes] = {}
    current_table: int | None = None
    expected_count: int | None = None
    buf = bytearray()
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line:
            continue
        if line.startswith(";Table:"):
            if current_table is not None:
                _flush_fcc(pages, current_table, bytes(buf), expected_count)
            current_table = int(line.split(":", 1)[1].strip())
            expected_count = None
            buf = bytearray()
            continue
        if line.startswith(";Total bytes:"):
            expected_count = int(line.split(":", 1)[1].strip())
            continue
        if line.startswith(";"):
            continue
        if current_table is None:
            continue
        buf.extend(_bytes_from_data_line(line, group_size=4))
    if current_table is not None:
        _flush_fcc(pages, current_table, bytes(buf), expected_count)
    return pages


def _flush_fcc(pages: dict[int, bytes], table: int, data: bytes, expected: int | None) -> None:
    if expected is not None and len(data) != expected:
        raise ValueError(f"FCC table {table}: expected {expected} bytes, got {len(data)}")
    pages[table] = data


def parse_eeprom_setup(text: str) -> dict[tuple[int, int], bytes]:
    """Return {(table_id, page_id): bytes}.

    `!Index: Table 0xNNNN: 0xPP(P)` declares a slice; `!Bytes:AA - BB` declares
    its byte range; subsequent hex lines fill it.
    """
    slices: dict[tuple[int, int], bytes] = {}
    table_id: int | None = None
    page_id: int | None = None
    byte_start: int | None = None
    byte_end: int | None = None
    buf = bytearray()
    header_re = re.compile(r"!Index:\s*Table\s+0x([0-9A-Fa-f]+):\s*0x([0-9A-Fa-f]+)")
    bytes_re = re.compile(r"!Bytes:\s*([0-9A-Fa-f]+)\s*-\s*([0-9A-Fa-f]+)")
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line:
            continue
        m = header_re.match(line)
        if m:
            if table_id is not None and byte_start is not None and byte_end is not None:
                _flush_eeprom(slices, table_id, page_id, byte_start, byte_end, bytes(buf))
            table_id = int(m.group(1), 16)
            page_id = int(m.group(2), 16)
            byte_start = byte_end = None
            buf = bytearray()
            continue
        m = bytes_re.match(line)
        if m:
            byte_start = int(m.group(1), 16)
            byte_end = int(m.group(2), 16)
            continue
        if line.startswith("!"):
            continue
        if table_id is None:
            continue
        buf.extend(_bytes_from_data_line(line, group_size=1))
    if table_id is not None and byte_start is not None and byte_end is not None:
        _flush_eeprom(slices, table_id, page_id, byte_start, byte_end, bytes(buf))
    return slices


def _flush_eeprom(
    slices: dict[tuple[int, int], bytes],
    table_id: int,
    page_id: int | None,
    byte_start: int,
    byte_end: int,
    data: bytes,
) -> None:
    expected = byte_end - byte_start + 1
    if len(data) != expected:
        raise ValueError(
            f"QSFP table {table_id} (page 0x{page_id:02X}): expected {expected} bytes "
            f"({byte_start:#x}-{byte_end:#x}), got {len(data)}"
        )
    slices[(table_id, page_id if page_id is not None else 0)] = data


def write_sfp_outputs(stem: Path, pages: dict[int, bytes]) -> list[Path]:
    """SFP+ FCCABY: Table 0 = first 128 bytes of A2h, Table 1 = bytes 128-255 of A2h.

    Writes <stem>.a2h.bin (concatenated 256 bytes).
    """
    if 0 not in pages or 1 not in pages:
        raise ValueError(f"FCC setup missing Tables 0+1: have {sorted(pages)}")
    a2h = pages[0] + pages[1]
    out = stem.with_suffix(".a2h.bin")
    out.write_bytes(a2h)
    return [out]


def write_sfp_eeprom_output(stem: Path, raw: bytes) -> list[Path]:
    """SFP+ EEPROM*.txt is already a raw 128-byte binary blob = A0h:0-127."""
    out = stem.with_suffix(".a0h.bin")
    out.write_bytes(raw)
    return [out]


_QSFP_PAGE_NAMES = {
    0x00: "page-00h",
    0x01: "page-01h",
    0x02: "page-02h",
    0x03: "page-03h",
}


def write_qsfp_outputs(stem: Path, slices: dict[tuple[int, int], bytes]) -> list[Path]:
    """QSFP: produce <stem>.lower.bin (Lower) + <stem>.page-NNh.bin per upper page."""
    written: list[Path] = []
    if (0x0000, 0x00) not in slices:
        raise ValueError("QSFP dump missing Table 0x0000 (Lower memory 0-127)")
    written.append(stem.with_suffix(".lower.bin"))
    written[-1].write_bytes(slices[(0x0000, 0x00)][:128])
    for (table_id, page_id), data in sorted(slices.items()):
        if table_id == 0x0000:
            continue
        if page_id not in _QSFP_PAGE_NAMES:
            print(
                f"warning: skipping unknown page 0x{page_id:02X} (table 0x{table_id:04X})",
                file=sys.stderr,
            )
            continue
        out = stem.with_suffix(f".{_QSFP_PAGE_NAMES[page_id]}.bin")
        out.write_bytes(data)
        written.append(out)
    return written


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("input", type=Path, help="OOM module_data text file (or raw 128-byte EEPROM blob)")
    p.add_argument("stem", type=Path, help="Output path stem (without suffix)")
    args = p.parse_args(argv)

    raw_bytes = args.input.read_bytes()
    if len(raw_bytes) == 128 and not raw_bytes.startswith(b";") and not raw_bytes.startswith(b"E"):
        # SFP+ EEPROM*.txt is a raw 128-byte binary despite the .txt extension.
        outs = write_sfp_eeprom_output(args.stem, raw_bytes)
    else:
        text = raw_bytes.decode("ascii", errors="replace")
        fmt = detect_format(text)
        if fmt == "fcc-setup":
            outs = write_sfp_outputs(args.stem, parse_fcc_setup(text))
        else:
            outs = write_qsfp_outputs(args.stem, parse_eeprom_setup(text))
    for out in outs:
        print(f"wrote {out} ({out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
