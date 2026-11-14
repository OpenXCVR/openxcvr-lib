"""Where the standards tools look for the sibling `openxcvr` checkout.

digest-standards.py, extract-pdf-range.sh and watch-oif-ias.sh read the
standards PDFs and the publication watch list from the `openxcvr` checkout that
sits beside this repository. Each case copies the three tools into a synthetic
workspace and runs them from a checkout shape a contributor actually uses: a
linked worktree under `.worktrees/`, the main checkout, and a tree outside git.

Run: python3 -m unittest tools.test_standards_root -v
"""

from __future__ import annotations

import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parent
COPIED = ("digest-standards.py", "extract-pdf-range.sh", "watch-oif-ias.sh")


def _clean_env(root: Path | None = None) -> dict[str, str]:
    """The caller's environment without git redirection, and OPENXCVR_ROOT only
    when a test sets it, so a developer's own setting cannot leak in."""
    env = {k: v for k, v in os.environ.items()
           if k != "OPENXCVR_ROOT" and not k.startswith("GIT_")}
    if root is not None:
        env["OPENXCVR_ROOT"] = str(root)
    return env


def _git(cwd: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(cwd), "-c", "user.name=test",
                    "-c", "user.email=test@example.invalid", *args],
                   check=True, capture_output=True, env=_clean_env())


class StandardsRootTest(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.ws = Path(tmp.name).resolve() / "ws"
        self.lib = self.ws / "openxcvr-lib"
        (self.lib / "tools").mkdir(parents=True)
        for name in COPIED:
            shutil.copy2(TOOLS / name, self.lib / "tools" / name)
        # The sibling holds the directories the tools look for, but not the
        # files, so each tool stops at its "missing" error and names the path
        # it resolved without reading a PDF or fetching anything.
        self.sibling = self.ws / "openxcvr"
        (self.sibling / "docs" / "stds").mkdir(parents=True)
        (self.sibling / "docs" / "status").mkdir(parents=True)
        # An unrelated working directory, so a cwd-relative guess cannot pass.
        self.elsewhere = Path(tmp.name).resolve() / "elsewhere"
        self.elsewhere.mkdir()
        # pdftotext is checked for before the PDF; a stub satisfies the check.
        self.bin = Path(tmp.name).resolve() / "bin"
        self.bin.mkdir()
        stub = self.bin / "pdftotext"
        stub.write_text("#!/bin/sh\nexit 0\n")
        stub.chmod(0o755)

    def _linked_worktree(self) -> Path:
        _git(self.lib, "init", "-q", "-b", "main")
        _git(self.lib, "add", "-A")
        _git(self.lib, "commit", "-q", "-m", "tools")
        wt = self.lib / ".worktrees" / "feature"
        _git(self.lib, "worktree", "add", "-q", str(wt))
        return wt

    def _digest_default(self, checkout: Path, root: Path | None = None) -> Path:
        path = checkout / "tools" / "digest-standards.py"
        spec = importlib.util.spec_from_file_location("digest_standards_under_test", path)
        module = importlib.util.module_from_spec(spec)
        # dataclasses resolves annotations through sys.modules during import.
        sys.modules[spec.name] = module
        with mock.patch.dict(os.environ, _clean_env(root), clear=True):
            cwd = os.getcwd()
            os.chdir(self.elsewhere)
            try:
                spec.loader.exec_module(module)
            finally:
                os.chdir(cwd)
                del sys.modules[spec.name]
        return module.DEFAULT_STDS_ROOT

    def _run(self, checkout: Path, *argv: str, root: Path | None = None) -> subprocess.CompletedProcess:
        env = _clean_env(root)
        env["PATH"] = f"{self.bin}{os.pathsep}{env.get('PATH', '')}"
        return subprocess.run(["bash", str(checkout / "tools" / argv[0]), *argv[1:]],
                              cwd=self.elsewhere, env=env, capture_output=True, text=True)

    def _assert_all_tools_use_sibling(self, checkout: Path) -> None:
        self.assertEqual(self._digest_default(checkout), self.sibling / "docs" / "stds")

        pdf = self._run(checkout, "extract-pdf-range.sh", "cmis-5.3", "1", "2")
        self.assertEqual(pdf.returncode, 2, pdf.stderr)
        self.assertIn(f"PDF not found: {self.sibling}/docs/stds/cmis/", pdf.stderr)

        watch = self._run(checkout, "watch-oif-ias.sh")
        self.assertEqual(watch.returncode, 2, watch.stderr)
        self.assertIn(
            f"state file missing: {self.sibling}/docs/status/upstream-publications.md",
            watch.stderr)

    def test_linked_worktree_finds_sibling_beside_main_checkout(self) -> None:
        # The worktree's own parent is .worktrees/, which holds no openxcvr.
        self._assert_all_tools_use_sibling(self._linked_worktree())

    def test_main_checkout_finds_sibling(self) -> None:
        self._linked_worktree()
        self._assert_all_tools_use_sibling(self.lib)

    def test_tree_outside_git_finds_sibling_beside_it(self) -> None:
        self._assert_all_tools_use_sibling(self.lib)

    def test_explicit_root_wins(self) -> None:
        other = self.ws / "elsewhere-root"
        (other / "docs" / "stds").mkdir(parents=True)
        (other / "docs" / "status").mkdir(parents=True)
        wt = self._linked_worktree()
        self.assertEqual(self._digest_default(wt, root=other), other / "docs" / "stds")
        pdf = self._run(wt, "extract-pdf-range.sh", "cmis-5.3", "1", "2", root=other)
        watch = self._run(wt, "watch-oif-ias.sh", root=other)
        self.assertIn(f"PDF not found: {other}/docs/stds/cmis/", pdf.stderr)
        self.assertIn(f"state file missing: {other}/docs/status/", watch.stderr)

    def test_explicit_stds_flag_wins_over_environment(self) -> None:
        selected = self.ws / "selected-pdfs"
        (selected / "cmis").mkdir(parents=True)
        pdf = selected / "cmis" / "OIF-CMIS-05.3.pdf"
        pdf.write_bytes(b"synthetic PDF placeholder; converter is stubbed")
        result = self._run(self.lib, "extract-pdf-range.sh", "--stds-root", str(selected),
                           "cmis-5.3", "1", "2", root=self.sibling)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_explicit_state_flag_and_deterministic_fetch(self) -> None:
        state = self.ws / "selected-state.md"
        url = "https://example.invalid/OIF-CMIS-00.0.pdf"
        state.write_text("## OIF — CMIS family\n" + url + "\n## Other\n")
        curl = self.bin / "curl"
        curl.write_text('#!/bin/sh\nwhile [ "$#" -gt 0 ]; do\n'
                        'if [ "$1" = -o ]; then printf \'%s\\n\' \'<a href="'
                        + url + '">example</a>\' > "$2"; exit 0; fi\nshift\ndone\nexit 1\n')
        curl.chmod(0o755)
        result = self._run(self.lib, "watch-oif-ias.sh", "--state-file", str(state),
                           root=self.sibling)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no CMIS-family changes", result.stdout)
        result = self._run(self.lib, "watch-oif-ias.sh", "--state-file", str(state), "--urls")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_missing_explicit_inputs_name_the_flag(self) -> None:
        pdf = self._run(self.lib, "extract-pdf-range.sh", "--stds-root", str(self.ws / "absent"),
                        "cmis-5.3", "1", "2")
        watch = self._run(self.lib, "watch-oif-ias.sh", "--state-file", str(self.ws / "absent"))
        self.assertEqual(pdf.returncode, 2)
        self.assertIn("--stds-root", pdf.stderr)
        self.assertEqual(watch.returncode, 2)
        self.assertIn("--state-file", watch.stderr)


if __name__ == "__main__":
    unittest.main()
