#!/usr/bin/env python3
"""Compare actual public C results with the safe Rust API, field for field."""
import itertools
import subprocess
import sys

c_runner, rust_runner = sys.argv[1:]
count = 0
for revision, media, scenario in itertools.product(
    (0x40, 0x50, 0x51, 0x52, 0x53, 0x54, 0x30), (0, 1, 2, 3, 4, 5, 0x40), range(12)
):
    args = list(map(str, (revision, media, scenario)))
    c_result = subprocess.check_output([c_runner, *args], text=True)
    rust_result = subprocess.check_output([rust_runner, *args], text=True)
    assert c_result == rust_result, (args, c_result, rust_result)
    assert len(c_result.splitlines()) == 15, args
    count += 1
print(f"Rust/C application equivalence: {count} fixtures, {count * 15} records")
