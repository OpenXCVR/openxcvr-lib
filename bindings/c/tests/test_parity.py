#!/usr/bin/env python3
"""Negative controls for the portable driver, using the actual C test binary."""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import parity


class ParityDriverTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="parity driver tests ")
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)
        self.metadata = parity.families(EXECUTABLE, ROOT)
        # Metadata (including labels and directories) always comes from C.
        self.family = self.metadata[0]
        _, golden_dir, _ = self.family
        self.golden = next(p for p in sorted((ROOT / golden_dir).iterdir())
                           if not p.name.startswith("_") and p.suffix == ".json")
        self.goldens = self.output / "goldens with spaces"
        self.goldens.mkdir()
        self.actual = self.output / "actual with spaces"
        self.actual.mkdir()

    def family_with_local_goldens(self):
        label, _, fixtures = self.family
        return label, str(self.goldens), str(ROOT / fixtures)

    def run_family(self):
        # Keep negative controls' expected diagnostics out of CTest success logs.
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            return parity.run_family(EXECUTABLE, ROOT,
                                     self.family_with_local_goldens(), self.actual)

    def test_compare_metadata_and_filtering(self):
        data = json.loads(self.golden.read_text(encoding="utf-8"))
        data["_metadata"] = {"test": "ignored just like compare_json.py"}
        (self.goldens / self.golden.name).write_text(json.dumps(data), encoding="utf-8")
        (self.goldens / "_ignored.json").write_text("not JSON", encoding="utf-8")
        (self.goldens / "ignored.txt").write_text("not JSON", encoding="utf-8")
        self.assertEqual(self.run_family(), (1, 1))

    def test_mismatch_cannot_pass(self):
        (self.goldens / self.golden.name).write_text('{}', encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, r"executed 0 != discovered 1 \(1 failed\)"):
            self.run_family()

    def test_failed_c_invocation_cannot_pass(self):
        (self.goldens / "missing fixture with spaces.json").write_text('{}', encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, r"executed 0 != discovered 1 \(1 failed\)"):
            self.run_family()

    def test_empty_family_cannot_pass(self):
        (self.goldens / "_ignored.json").write_text('{}', encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "no goldens discovered"):
            self.run_family()

    def test_missing_family_cannot_pass(self):
        self.goldens.rmdir()
        with self.assertRaises(OSError):
            self.run_family()

    def test_invalid_json_cannot_pass(self):
        (self.goldens / self.golden.name).write_text('not JSON', encoding="utf-8")
        with self.assertRaises(RuntimeError):
            self.run_family()

    def test_every_family_is_required(self):
        for family in self.metadata:
            with self.subTest(family=family[0]), redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError, "no goldens discovered"):
                    parity.run_family(EXECUTABLE, ROOT,
                                      (family[0], str(self.goldens), family[2]), self.actual)

    def test_empty_metadata_cannot_pass(self):
        with patch.object(parity.subprocess, "run",
                          return_value=subprocess.CompletedProcess([], 0, stdout="")):
            with self.assertRaisesRegex(RuntimeError, "empty C family metadata"):
                parity.families(EXECUTABLE, ROOT)

    def test_read_error_oracles_and_fixture_sizes(self):
        directory = self.output / "read errors"
        directory.mkdir()
        parity.prepare_read_errors(ROOT, directory)
        self.assertTrue((directory / "a0h-err.a0h.bin").is_dir())
        self.assertTrue((directory / "a2h-err.a2h.bin").is_dir())
        for page in ("a0h", "a2h"):
            self.assertEqual((directory / f"short.{page}.bin").stat().st_size, 100)
        self.assertEqual((directory / "short.a0h.bin").read_bytes(),
                         (directory / "a2h-err.a0h.bin").read_bytes())
        subprocess.run([str(EXECUTABLE), "--read-error", str(directory)], check=True)

    def test_process_failure_reaches_cli_exit(self):
        result = subprocess.run([sys.executable, str(Path(parity.__file__)),
                                 str(self.output / "missing executable"), str(ROOT)],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)

    def test_relative_executable_path_with_spaces(self):
        directory = self.output / "binary directory"
        directory.mkdir()
        executable = directory / EXECUTABLE.name
        shutil.copy2(EXECUTABLE, executable)
        # Windows executable lookup ignores Popen(cwd=); set the caller's cwd.
        self.addCleanup(os.chdir, Path.cwd())
        os.chdir(self.output)
        result = subprocess.run([str(executable.relative_to(self.output))],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_explicit_relative_executable_is_not_shadowed_by_path(self):
        executable = self.output / EXECUTABLE.name
        shutil.copy2(EXECUTABLE, executable)
        shadow = self.output / "shadow"
        shadow.mkdir()
        impostor = shadow / EXECUTABLE.name
        impostor.write_text("not an executable", encoding="utf-8")
        impostor.chmod(0o755)
        environment = dict(os.environ, PATH=str(shadow) + os.pathsep + os.environ.get("PATH", ""))
        self.addCleanup(os.chdir, Path.cwd())
        os.chdir(self.output)
        result = subprocess.run(["./" + executable.name], env=environment,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_executable_path_with_spaces_and_self_test(self):
        executable = self.output / EXECUTABLE.name
        shutil.copyfile(EXECUTABLE, executable)
        executable.chmod(0o755)
        subprocess.run([str(executable), "--self-test"], check=True)
        # The no-args C launcher must preserve the executable path as one
        # argument too (including on Windows, where spawnv does not quote it).
        subprocess.run([str(executable)], cwd=self.output, check=True,
                       capture_output=True, text=True)
        with redirect_stdout(io.StringIO()):
            self.assertEqual(parity.families(executable, ROOT), self.metadata)
            self.assertEqual(parity.run_family(executable, ROOT, self.family, self.actual)[0],
                             len([p for p in (ROOT / self.family[1]).iterdir()
                                  if not p.name.startswith("_") and p.suffix == ".json"]))


if __name__ == "__main__":
    EXECUTABLE = Path(sys.argv.pop(1)).resolve()
    ROOT = Path(sys.argv.pop(1)).resolve()
    unittest.main()
