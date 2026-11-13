#!/usr/bin/env python3
"""Regression checks: reject unsupported rich toolchains before any Cargo build."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

cmake, source, build, compiler = sys.argv[1:]
root = Path(build) / "rich-configure-refusals"
if root.exists():
    shutil.rmtree(root)
root.mkdir()
(root / "tmp").mkdir()
env = dict(os.environ, TMPDIR=str(root / "tmp"))
# A native compiler wrapper with a forced, disagreeing driver target keeps
# compiler discovery working without requiring a cross sysroot on the test host.
wrapper = root / "mismatched-cc"
wrapper.write_text(
    f"#!{sys.executable}\nimport os, sys\n"
    "if '-dumpmachine' in sys.argv:\n    print('unsupported-openxcvr-target')\n"
    f"else:\n    os.execv({compiler!r}, [{compiler!r}, *sys.argv[1:]])\n"
)
wrapper.chmod(0o755)
cases = (
    ("cross", ["-DCMAKE_SYSTEM_NAME=Linux"], "cross builds"),
    ("target", [f"-DCMAKE_C_COMPILER={wrapper}"], "C/Rust target mismatch"),
    ("abi", ["-DCMAKE_C_FLAGS=-fshort-enums"], "C/Rust ABI mismatch"),
    ("profile", ["-DCMAKE_BUILD_TYPE=Unsupported"], "unsupported CMAKE_BUILD_TYPE"),
)
for name, flags, diagnostic in cases:
    result = subprocess.run(
        [cmake, "-S", source, "-B", str(root / name),
         f"-DCMAKE_C_COMPILER={compiler}", "-DOPENXCVR_C_RICH=ON",
         "-DBUILD_TESTING=OFF", *flags],
        env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    (root / f"{name}.log").write_text(result.stdout)
    if result.returncode == 0 or diagnostic not in result.stdout:
        raise AssertionError(f"{name}: expected configure refusal '{diagnostic}'\n{result.stdout}")
    print(f"{name}: configure refused ({result.returncode}): {diagnostic}")
print(f"Rich configure refusals: {len(cases)}/{len(cases)} passed")
