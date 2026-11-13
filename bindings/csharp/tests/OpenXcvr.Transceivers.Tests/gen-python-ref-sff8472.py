#!/usr/bin/env python3
"""Regenerate the Python-reference JSON fixtures for Sff8472PaddingTests.

Run from the repo root of the checkout whose bindings/python should serve as
the reference (the C# worktree for normal regenerations):

    python3 bindings/csharp/tests/OpenXcvr.Transceivers.Tests/gen-python-ref-sff8472.py

Builds the exact bytes Sff8472PaddingTests builds (keep the two files in
sync), decodes them with the Python facade `decode_sff8472`, and writes one
JSON file per case beside this script, each with a header comment naming this
command. The test files parse those JSONs with comment-skipping options, so
the leading `//` header is part of the contract.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO_ROOT / "bindings" / "python"))

from openxcvr._sff8472 import decode_sff8472  # noqa: E402

COMMAND = (
    "python3 bindings/csharp/tests/OpenXcvr.Transceivers.Tests/"
    "gen-python-ref-sff8472.py"
)


def write_ascii(buf: bytearray, offset: int, value: str) -> None:
    for i, ch in enumerate(value):
        buf[offset + i] = ord(ch)


def csum(buf: bytearray, start: int, end: int) -> int:
    return sum(buf[start:end]) & 0xFF


def short_a0h() -> bytearray:
    buf = bytearray(100)
    buf[0] = 0x03
    buf[1] = 0x04
    buf[2] = 0x21
    for i in range(3, 11):
        buf[i] = 0x03 + (i - 3)
    buf[12] = 0x64
    write_ascii(buf, 20, "ACMEWIDGETSP")
    write_ascii(buf, 36, "ABC-123 ")
    write_ascii(buf, 44, "A1      ")
    write_ascii(buf, 52, "SN000001        ")
    write_ascii(buf, 68, "250113  ")
    buf[92] = 0x60
    buf[63] = csum(buf, 0, 63)
    buf[95] = csum(buf, 64, 95)
    return buf


def full_a2h() -> bytearray:
    buf = bytearray(256)
    buf[96] = 0x20
    buf[97] = 0x00
    buf[98] = 0x81
    buf[99] = 0x00
    buf[100] = 0x27
    buf[101] = 0x10
    buf[102] = 0x04
    buf[103] = 0x00
    buf[104] = 0x0C
    buf[105] = 0x35
    buf[110] = 0x00
    buf[112] = 0x01
    buf[113] = 0x02
    buf[114] = 0x04
    buf[115] = 0x08
    buf[116] = 0x10
    buf[117] = 0x20
    buf[95] = csum(buf, 0, 95)
    return buf


def a0h_300() -> bytearray:
    buf = bytearray(300)
    buf[:100] = short_a0h()
    for i in range(100, 300):
        buf[i] = i & 0xFF
    return buf


def main() -> None:
    here = Path(__file__).resolve().parent
    cases = [
        (
            "python-ref-sff8472-shorta0h-fulla2h.json",
            "ShortA0h() as A0h (100 bytes, padded to 256) + FullA2h() as A2h",
            bytes(short_a0h()),
        ),
        (
            "python-ref-sff8472-emptya0h.json",
            "a zero-length A0h (padded to all zeros) + FullA2h() as A2h",
            b"",
        ),
        (
            "python-ref-sff8472-a0h300.json",
            "a 300-byte A0h (ShortA0h() bytes 0-99, i & 0xFF for i in 100..299; "
            "passed through verbatim, never truncated) + FullA2h() as A2h",
            bytes(a0h_300()),
        ),
    ]
    a2h = bytes(full_a2h())
    for name, description, a0h in cases:
        result = decode_sff8472({"a0h": a0h, "a2h": a2h})
        header = (
            f"// Reference output of the Python facade decode_sff8472 "
            f"(bindings/python) on the exact\n"
            f"// bytes Sff8472PaddingTests builds. Input: {description}.\n"
            f"// Regenerate from the repo root:\n"
            f"//   {COMMAND}\n"
        )
        body = json.dumps(result, indent=2, sort_keys=True)
        (here / name).write_text(header + body + "\n", encoding="utf-8")
        print(f"wrote {name}")


if __name__ == "__main__":
    main()
