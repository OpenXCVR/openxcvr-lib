#!/usr/bin/env python3
"""Filesystem/process orchestration for parity.c; SDK assertions stay in C."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from compare_json import load


def families(executable, root):
    result = subprocess.run([str(executable), "--families"], cwd=root,
                            check=True, capture_output=True, text=True)
    rows = [line.split("\t") for line in result.stdout.splitlines()]
    if not rows or any(len(row) != 3 or not all(row) for row in rows):
        raise RuntimeError("invalid or empty C family metadata")
    if len({row[0] for row in rows}) != len(rows):
        raise RuntimeError("duplicate C family metadata")
    return rows


def run_family(executable, root, family, output):
    label, golden_dir, fixture_dir = family
    goldens = sorted(path for path in (root / golden_dir).iterdir()
                     if not path.name.startswith("_") and path.name.endswith(".json"))
    discovered = len(goldens)
    executed = failed = 0
    for golden in goldens:
        actual = output / f"{label}-{golden.name}"
        try:
            subprocess.run([str(executable), "--fixture", label,
                            str(root / fixture_dir), golden.stem, str(actual)],
                           cwd=root, check=True)
            # Exactly compare_json.py's semantics: ignore only top-level _metadata.
            actual_json, golden_json = load(actual), load(golden)
            if actual_json != golden_json:
                raise RuntimeError(
                    f"{golden}: JSON mismatch\n"
                    f"actual: {json.dumps(actual_json, sort_keys=True)}\n"
                    f"golden: {json.dumps(golden_json, sort_keys=True)}")
            executed += 1
        except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
            print(error, file=sys.stderr)
            failed += 1
    print(f"C parity ({label}): {executed} executed; {discovered} discovered; {failed} failed")
    if discovered == 0:
        raise RuntimeError(f"family {label}: no goldens discovered")
    if executed != discovered or failed:
        raise RuntimeError(f"family {label}: executed {executed} != discovered {discovered} ({failed} failed)")
    return executed, discovered


def prepare_read_errors(root, directory):
    """Directories fail fopen on Windows or fread/ferror (EISDIR) on Linux."""
    (directory / "a0h-err.a0h.bin").mkdir()
    (directory / "a2h-err.a2h.bin").mkdir()
    fixture = root / "tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0"
    for page in ("a0h", "a2h"):
        data = Path(f"{fixture}.{page}.bin").read_bytes()[:100]
        if len(data) != 100:
            raise RuntimeError(f"{fixture}.{page}.bin: expected at least 100 bytes")
        (directory / f"short.{page}.bin").write_bytes(data)
    shutil.copyfile(directory / "short.a0h.bin", directory / "a2h-err.a0h.bin")


def run(executable, root):
    metadata = families(executable, root)
    subprocess.run([str(executable), "--witnesses"], cwd=root, check=True)
    # Spaces are intentional: every invocation must preserve argv boundaries.
    with tempfile.TemporaryDirectory(prefix="openxcvr c parity ") as temporary:
        output = Path(temporary)
        read_errors = output / "read errors"
        read_errors.mkdir()
        prepare_read_errors(root, read_errors)
        subprocess.run([str(executable), "--read-error", str(read_errors)],
                       cwd=root, check=True)
        executed = discovered = 0
        for family in metadata:
            count, found = run_family(executable, root, family, output)
            executed += count
            discovered += found
        print(f"C parity: {executed} executed; {discovered} discovered; 0 failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    # Retain an explicit ./ prefix so PATH lookup cannot select another binary.
    parser.add_argument("executable")
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    # Resolve before changing cwd in child processes, including PATH invocation
    # of the backwards-compatible no-args executable launcher.
    executable = Path(shutil.which(args.executable) or args.executable).resolve()
    try:
        run(executable, args.root.resolve())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
